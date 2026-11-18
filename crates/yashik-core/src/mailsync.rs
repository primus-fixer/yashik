//! Загрузка ящика с сервера: папки, заголовки новых писем (новые — первыми), отметки и письма,
//! которых на сервере больше нет. Тексты писем загружаются отдельно, когда письмо открывают.
//!
//! Сеть и база не держат друг друга: письма приходят пачками, и каждая пачка записывается в базу
//! короткой транзакцией — экраны тем временем читают базу.

use crate::account::{self, Account};
use crate::imap::{self, timed, Error, Folder};
use crate::maildb::{self, Flags, Header};
use crate::Store;
use async_imap::imap_proto::BodyStructure;
use async_imap::types::{Fetch, Flag};
use async_imap::Session;
use futures::TryStreamExt;
use serde::Serialize;
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::Mutex;
use tokio::io::{AsyncRead, AsyncWrite};

/// Сколько заголовков просить за один запрос.
const BATCH: usize = 200;

/// Какие поля заголовка нужны для списка и цепочек.
const FIELDS: &str = "(UID FLAGS INTERNALDATE RFC822.SIZE BODYSTRUCTURE BODY.PEEK[HEADER.FIELDS (DATE FROM TO CC SUBJECT MESSAGE-ID IN-REPLY-TO)])";

/// Как идёт загрузка — для экрана.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Progress {
    pub account: String,
    pub folder: String,
    pub done: usize,
    pub total: usize,
}

/// Итог загрузки ящика.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Stats {
    pub new: usize,
    pub changed: usize,
    pub removed: usize,
}

/// Загрузить ящик: вход, папки, письма каждой папки.
pub async fn sync_account(store: &Mutex<Store>, account: &Account, progress: &(dyn Fn(Progress) + Send + Sync)) -> Result<Stats, Error> {
    let client = imap::connect(&account.imap, imap::mozilla_roots()).await?;
    let mut session = imap::login(client, account, &account.imap).await?;
    let result = sync_session(&mut session, store, account, progress).await;
    let _ = tokio::time::timeout(imap::WAIT, session.logout()).await;
    result
}

async fn sync_session<S: AsyncRead + AsyncWrite + Unpin + Debug + Send>(
    session: &mut Session<S>,
    store: &Mutex<Store>,
    account: &Account,
    progress: &(dyn Fn(Progress) + Send + Sync),
) -> Result<Stats, Error> {
    let id = account::id_of(&account.email);
    let folders = imap::list_folders(session, &account.imap).await?;
    with_db(store, |db| maildb::save_folders(db, &id, &folders))?;
    let mut stats = Stats::default();
    // Папки уже по порядку: «Входящие», «Отправленные», «Черновики», «Спам», «Корзина», потом остальные.
    for f in folders.iter().filter(|f| f.selectable) {
        sync_folder(session, store, account, &id, f, &mut stats, progress).await?;
    }
    Ok(stats)
}

fn with_db<T>(store: &Mutex<Store>, f: impl FnOnce(&mut rusqlite::Connection) -> maildb::Result<T>) -> Result<T, Error> {
    let mut st = store.lock().map_err(|_| Error::Other("база занята".to_string()))?;
    f(st.conn_mut()).map_err(|e| Error::Other(format!("база писем: {e}")))
}

async fn sync_folder<S: AsyncRead + AsyncWrite + Unpin + Debug + Send>(
    session: &mut Session<S>,
    store: &Mutex<Store>,
    account: &Account,
    id: &str,
    folder: &Folder,
    stats: &mut Stats,
    progress: &(dyn Fn(Progress) + Send + Sync),
) -> Result<(), Error> {
    let server = &account.imap;
    let failed = |e: async_imap::error::Error| Error::Other(format!("{}: папка «{}» не открылась ({e})", server.host, folder.title));
    let mailbox = timed(server, session.examine(&folder.name), failed).await?;
    let validity = mailbox.uid_validity.unwrap_or(0);
    if with_db(store, |db| maildb::uidvalidity(db, id, &folder.name))? != Some(validity) {
        with_db(store, |db| maildb::reset_folder(db, id, &folder.name, validity))?;
    }

    // Что на сервере сейчас: номера писем и отметки (удалённые, но ещё не стёртые — как будто их нет).
    let on_server: HashMap<u32, Flags> = if mailbox.exists == 0 {
        HashMap::new()
    } else {
        let fetches = timed(server, async { session.uid_fetch("1:*", "(UID FLAGS)").await?.try_collect::<Vec<_>>().await }, |e| {
            Error::Other(format!("{}: не удалось прочитать папку «{}» ({e})", server.host, folder.title))
        })
        .await?;
        fetches
            .iter()
            .filter_map(|f| {
                let (flags, deleted) = flags_of(f.flags());
                let uid = f.uid?;
                (!deleted).then_some((uid, flags))
            })
            .collect()
    };
    let local: HashMap<u32, Flags> = with_db(store, |db| maildb::known(db, id, &folder.name))?.into_iter().collect();

    let gone: Vec<u32> = local.keys().filter(|uid| !on_server.contains_key(uid)).copied().collect();
    let changed: Vec<(u32, Flags)> = on_server.iter().filter(|(uid, f)| local.get(uid).is_some_and(|l| l != *f)).map(|(u, f)| (*u, *f)).collect();
    let mut new: Vec<u32> = on_server.keys().filter(|uid| !local.contains_key(uid)).copied().collect();
    new.sort_unstable_by(|a, b| b.cmp(a));
    if !gone.is_empty() {
        with_db(store, |db| maildb::remove(db, id, &folder.name, &gone))?;
    }
    if !changed.is_empty() {
        with_db(store, |db| maildb::set_flags(db, id, &folder.name, &changed))?;
    }
    stats.removed += gone.len();
    stats.changed += changed.len();

    let total = new.len();
    let mut done = 0;
    for chunk in new.chunks(BATCH) {
        let set = uid_set(chunk);
        let fetches = timed(server, async { session.uid_fetch(&set, FIELDS).await?.try_collect::<Vec<_>>().await }, |e| {
            Error::Other(format!("{}: письма папки «{}» не загрузились ({e})", server.host, folder.title))
        })
        .await?;
        let headers: Vec<Header> = fetches.iter().filter_map(header_of).filter(|h| !h.deleted).map(|h| h.header).collect();
        with_db(store, |db| maildb::save_messages(db, id, &folder.name, &headers))?;
        done += chunk.len();
        stats.new += headers.len();
        progress(Progress { account: account.email.clone(), folder: folder.title.clone(), done, total });
    }
    Ok(())
}

/// Номера писем для запроса: подряд идущие — диапазоном, «101:104,200».
fn uid_set(uids: &[u32]) -> String {
    let mut sorted = uids.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < sorted.len() {
        let start = sorted[i];
        let mut end = start;
        while i + 1 < sorted.len() && sorted[i + 1] == end + 1 {
            i += 1;
            end = sorted[i];
        }
        out.push(if start == end { start.to_string() } else { format!("{start}:{end}") });
        i += 1;
    }
    out.join(",")
}

fn flags_of<'a>(flags: impl Iterator<Item = Flag<'a>>) -> (Flags, bool) {
    let mut out = Flags::default();
    let mut deleted = false;
    for f in flags {
        match f {
            Flag::Seen => out.seen = true,
            Flag::Flagged => out.flagged = true,
            Flag::Answered => out.answered = true,
            Flag::Deleted => deleted = true,
            _ => {}
        }
    }
    (out, deleted)
}

struct Parsed {
    header: Header,
    deleted: bool,
}

/// Заголовок из ответа сервера: поля письма — через mail-parser (RFC 2047, старые кодировки).
fn header_of(f: &Fetch) -> Option<Parsed> {
    let uid = f.uid?;
    let (flags, deleted) = flags_of(f.flags());
    let mut h = Header { uid, flags, size: f.size.unwrap_or(0), attachments: f.bodystructure().is_some_and(has_attachment), ..Default::default() };
    let mut sent = None;
    if let Some(m) = f.header().and_then(|raw| mail_parser::MessageParser::default().parse_headers(raw)) {
        h.subject = clean(m.subject().unwrap_or(""));
        if let Some(from) = m.from().and_then(|l| l.first()) {
            h.from_name = clean(from.name().unwrap_or(""));
            h.from_addr = from.address().unwrap_or("").to_string();
        }
        let mut people = Vec::new();
        let mut addrs = Vec::new();
        for list in [m.to(), m.cc()].into_iter().flatten() {
            for p in list.iter() {
                let addr = p.address().unwrap_or("");
                people.push(clean(p.name().filter(|n| !n.trim().is_empty()).unwrap_or(addr)));
                addrs.push(addr.to_string());
            }
        }
        h.recipients = people.join(", ");
        h.recipient_addrs = addrs.join(" ");
        h.message_id = m.message_id().map(str::to_string);
        let reply = m.in_reply_to();
        h.in_reply_to = reply.as_text().or_else(|| reply.as_text_list().and_then(|l| l.first().map(|c| c.as_ref()))).map(str::to_string);
        sent = m.date().map(|d| d.to_timestamp());
    }
    h.date = f.internal_date().map(|d| d.timestamp()).or(sent).unwrap_or(0);
    Some(Parsed { header: h, deleted })
}

/// Без переводов строк и лишних пробелов: тема и имена — в одну строку.
fn clean(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Есть ли вложения: часть с disposition «attachment» или файл (не текст и не картинка в тексте).
fn has_attachment(b: &BodyStructure<'_>) -> bool {
    match b {
        BodyStructure::Multipart { bodies, .. } => bodies.iter().any(has_attachment),
        BodyStructure::Basic { common, .. } | BodyStructure::Text { common, .. } | BodyStructure::Message { common, .. } => {
            match common.disposition.as_ref().map(|d| d.ty.to_ascii_lowercase()) {
                Some(d) if d == "attachment" => true,
                Some(d) if d == "inline" => false,
                _ => matches!(b, BodyStructure::Message { .. }) || matches!(common.ty.ty.to_ascii_lowercase().as_str(), "application" | "audio" | "video"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::Server;
    use crate::imap::tests::{demo, pymap, rt};
    use crate::imap::Role;
    use crate::maildb::Place;
    use tokio::net::TcpStream;

    #[test]
    fn uid_sets() {
        assert_eq!(uid_set(&[104, 103, 101, 102, 200]), "101:104,200");
        assert_eq!(uid_set(&[7]), "7");
        assert_eq!(uid_set(&[3, 1]), "1,3");
    }

    fn store() -> (std::path::PathBuf, Mutex<Store>) {
        let dir = std::env::temp_dir().join(format!("yashik-sync-{}", crate::new_id().unwrap()));
        std::fs::create_dir_all(&dir).unwrap();
        let st = crate::open(&dir.join("yashik.db"), &[3u8; 32], "aaaa").unwrap();
        (dir, Mutex::new(st))
    }

    const CYRILLIC: &[u8] = b"From: =?UTF-8?B?0KnRg9C60LjQvSDQkNC90LTRgNC10Lk=?= <shchukin@example.com>\r\n\
To: =?windows-1251?B?3uvo/w==?= <me@example.com>\r\n\
Subject: =?KOI8-R?B?79Teo9Qg2sEg0M/Mx8/EwQ==?=\r\n\
Date: Wed, 18 Nov 2026 10:00:00 +0500\r\n\
Message-ID: <otchet-1@example.com>\r\n\
MIME-Version: 1.0\r\n\
Content-Type: multipart/mixed; boundary=b1\r\n\
\r\n\
--b1\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
See the attachment.\r\n\
--b1\r\n\
Content-Type: application/pdf; name=otchet.pdf\r\n\
Content-Disposition: attachment; filename=otchet.pdf\r\n\
Content-Transfer-Encoding: base64\r\n\
\r\n\
JVBERi0xLjQK\r\n\
--b1--\r\n";

    #[test]
    fn sync_with_a_real_server() {
        let Some(p) = pymap() else { return };
        let (dir, store) = store();
        let server = Server::new("127.0.0.1", p.port);
        let account = demo("demopass", server.clone());
        let id = account::id_of(&account.email);
        rt().block_on(async {
            let tcp = TcpStream::connect(("127.0.0.1", p.port)).await.unwrap();
            let mut session = imap::login(async_imap::Client::new(tcp), &account, &server).await.unwrap();
            let seen = Mutex::new(Vec::new());
            let progress = |pr: Progress| seen.lock().unwrap().push(pr);

            // Первая загрузка: 4 во «Входящих», 2 в «Отправленных», в «Корзине» одно — помеченное удалённым, его не видно.
            let first = sync_session(&mut session, &store, &account, &progress).await.unwrap();
            assert_eq!(first, Stats { new: 6, changed: 0, removed: 0 });
            let inbox = |q: &str| maildb::list(store.lock().unwrap().conn(), &Place::Role(Role::Inbox), q, None, 50).unwrap().rows;
            let subjects: Vec<String> = inbox("").iter().map(|r| r.subject.clone()).collect();
            assert_eq!(subjects, ["Hello, World!", "Important notice regarding your account", "Random question", "Re: Re: Random question"]);
            assert!(inbox("")[1].flags.flagged && !inbox("")[1].flags.seen);
            assert_eq!(seen.lock().unwrap().first().map(|p| p.folder.as_str()), Some("Входящие"));

            // Новое письмо по-русски: тема в KOI8-R, имя получателя в windows-1251, вложение.
            session.append("INBOX", None, None, CYRILLIC).await.unwrap();
            // Письмо «Hello, World!» прочитали, «Random question» удалили на сервере.
            session.select("INBOX").await.unwrap();
            let _: Vec<_> = session.uid_store("104", "+FLAGS (\\Seen)").await.unwrap().try_collect().await.unwrap();
            let _: Vec<_> = session.uid_store("102", "+FLAGS (\\Deleted)").await.unwrap().try_collect().await.unwrap();
            let _: Vec<_> = session.expunge().await.unwrap().try_collect().await.unwrap();

            let second = sync_session(&mut session, &store, &account, &progress).await.unwrap();
            assert_eq!(second, Stats { new: 1, changed: 1, removed: 1 });
            let rows = inbox("");
            let top = &rows[0];
            assert_eq!((top.subject.as_str(), top.who.as_str(), top.attachments), ("Отчёт за полгода", "Щукин Андрей", true));
            assert!(rows.iter().find(|r| r.subject == "Hello, World!").unwrap().flags.seen);
            assert!(!rows.iter().any(|r| r.subject == "Random question"));
            // Ищется по словоформам и транслиту — в теме, отправителе и получателях.
            assert_eq!(inbox("отчёты").len(), 1);
            assert_eq!(inbox("Юлии").len(), 1);
            assert_eq!(inbox("Shchukin").len(), 1);
            assert_eq!(inbox("ghbdtn").len(), 0);
            let sent = maildb::list(store.lock().unwrap().conn(), &Place::Role(Role::Sent), "", None, 50).unwrap().rows;
            assert_eq!(sent.len(), 2);
            assert_eq!(sent[0].who, "colleague@example.com");
            let _ = id;
        });
        drop(store);
        let _ = std::fs::remove_dir_all(dir);
    }
}
