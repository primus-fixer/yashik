//! Письма в базе: свои таблицы в той же зашифрованной базе, рядом с журналом kit. В обмен между
//! устройствами они не попадают — каждое устройство загружает письма с сервера само.
//!
//! - `mail_folder` — папки ящиков: назначение, UIDVALIDITY (сменился — письма папки загружаются заново);
//! - `mail_message` — заголовки писем: от кого, кому, тема, дата, отметки, есть ли вложения;
//! - `mail_fts` — указатель поиска (FTS5) по ключам из [`crate::search`]: номер строки = номер письма.

use crate::imap::{Folder, Role};
use crate::search::{index_text, other_layout, Query};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

pub type Result<T> = rusqlite::Result<T>;

/// Таблицы писем: создаются при первом открытии, дальше не трогаются.
pub fn migrate(db: &Connection) -> Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS mail_folder (
            account TEXT NOT NULL,
            name TEXT NOT NULL,
            title TEXT NOT NULL,
            role TEXT NOT NULL,
            selectable INTEGER NOT NULL DEFAULT 1,
            uidvalidity INTEGER,
            PRIMARY KEY (account, name)
        );
        CREATE TABLE IF NOT EXISTS mail_message (
            id INTEGER PRIMARY KEY,
            account TEXT NOT NULL,
            folder TEXT NOT NULL,
            uid INTEGER NOT NULL,
            date INTEGER NOT NULL DEFAULT 0,
            from_name TEXT NOT NULL DEFAULT '',
            from_addr TEXT NOT NULL DEFAULT '',
            recipients TEXT NOT NULL DEFAULT '',
            subject TEXT NOT NULL DEFAULT '',
            message_id TEXT,
            in_reply_to TEXT,
            seen INTEGER NOT NULL DEFAULT 0,
            flagged INTEGER NOT NULL DEFAULT 0,
            answered INTEGER NOT NULL DEFAULT 0,
            size INTEGER NOT NULL DEFAULT 0,
            attachments INTEGER NOT NULL DEFAULT 0,
            UNIQUE (account, folder, uid)
        );
        CREATE INDEX IF NOT EXISTS mail_message_by_date ON mail_message (account, folder, date DESC);
        CREATE VIRTUAL TABLE IF NOT EXISTS mail_fts USING fts5(terms, tokenize = 'ascii');",
    )
}

/// Отметки письма на сервере.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Flags {
    pub seen: bool,
    pub flagged: bool,
    pub answered: bool,
}

/// Заголовок письма, как он загружен с сервера.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Header {
    pub uid: u32,
    /// Когда письмо пришло на сервер (секунды Unix), если нет — дата из письма.
    pub date: i64,
    pub from_name: String,
    pub from_addr: String,
    /// Кому и копия — имена, а у кого имени нет, адреса: «Юлия, ivan@example.com».
    pub recipients: String,
    /// Адреса получателей через пробел — только для поиска.
    pub recipient_addrs: String,
    pub subject: String,
    pub message_id: Option<String>,
    pub in_reply_to: Option<String>,
    pub flags: Flags,
    pub size: u32,
    pub attachments: bool,
}

pub fn role_code(role: Role) -> &'static str {
    match role {
        Role::Inbox => "inbox",
        Role::Sent => "sent",
        Role::Drafts => "drafts",
        Role::Junk => "junk",
        Role::Trash => "trash",
        Role::Archive => "archive",
        Role::All => "all",
        Role::Flagged => "flagged",
        Role::Other => "other",
    }
}

/// Назначение по коду из экрана: «inbox», «sent»…
pub fn role_from_code(code: &str) -> Option<Role> {
    [Role::Inbox, Role::Sent, Role::Drafts, Role::Junk, Role::Trash, Role::Archive, Role::All, Role::Flagged, Role::Other].into_iter().find(|r| role_code(*r) == code)
}

/// Папки ящика после LIST: новые добавляются, исчезнувшие на сервере удаляются вместе с письмами.
pub fn save_folders(db: &mut Connection, account: &str, folders: &[Folder]) -> Result<()> {
    let tx = db.transaction()?;
    {
        let mut up = tx.prepare(
            "INSERT INTO mail_folder (account, name, title, role, selectable) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT (account, name) DO UPDATE SET title = excluded.title, role = excluded.role, selectable = excluded.selectable",
        )?;
        for f in folders {
            up.execute(params![account, f.name, f.title, role_code(f.role), f.selectable])?;
        }
        let names: Vec<String> = tx.prepare("SELECT name FROM mail_folder WHERE account = ?1")?.query_map([account], |r| r.get(0))?.collect::<Result<_>>()?;
        for gone in names.iter().filter(|n| !folders.iter().any(|f| &f.name == *n)) {
            drop_messages(&tx, account, gone)?;
            tx.execute("DELETE FROM mail_folder WHERE account = ?1 AND name = ?2", params![account, gone])?;
        }
    }
    tx.commit()
}

/// UIDVALIDITY, с которым загружены письма папки.
pub fn uidvalidity(db: &Connection, account: &str, folder: &str) -> Result<Option<u32>> {
    db.query_row("SELECT uidvalidity FROM mail_folder WHERE account = ?1 AND name = ?2", params![account, folder], |r| r.get::<_, Option<u32>>(0))
        .optional()
        .map(Option::flatten)
}

/// Сервер сменил UIDVALIDITY: прежние номера писем ничего не значат — письма папки загружаются заново.
pub fn reset_folder(db: &mut Connection, account: &str, folder: &str, uidvalidity: u32) -> Result<()> {
    let tx = db.transaction()?;
    drop_messages(&tx, account, folder)?;
    tx.execute("UPDATE mail_folder SET uidvalidity = ?3 WHERE account = ?1 AND name = ?2", params![account, folder, uidvalidity])?;
    tx.commit()
}

fn drop_messages(tx: &Connection, account: &str, folder: &str) -> Result<()> {
    tx.execute("DELETE FROM mail_fts WHERE rowid IN (SELECT id FROM mail_message WHERE account = ?1 AND folder = ?2)", params![account, folder])?;
    tx.execute("DELETE FROM mail_message WHERE account = ?1 AND folder = ?2", params![account, folder])?;
    Ok(())
}

/// Письма папки на устройстве: номер на сервере и отметки.
pub fn known(db: &Connection, account: &str, folder: &str) -> Result<Vec<(u32, Flags)>> {
    let mut st = db.prepare("SELECT uid, seen, flagged, answered FROM mail_message WHERE account = ?1 AND folder = ?2")?;
    let rows = st.query_map(params![account, folder], |r| Ok((r.get(0)?, Flags { seen: r.get(1)?, flagged: r.get(2)?, answered: r.get(3)? })))?;
    rows.collect()
}

/// Новые письма папки — вместе с указателем поиска.
pub fn save_messages(db: &mut Connection, account: &str, folder: &str, headers: &[Header]) -> Result<()> {
    let tx = db.transaction()?;
    {
        let mut ins = tx.prepare(
            "INSERT INTO mail_message (account, folder, uid, date, from_name, from_addr, recipients, subject, message_id, in_reply_to, seen, flagged, answered, size, attachments)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
             ON CONFLICT (account, folder, uid) DO UPDATE SET seen = excluded.seen, flagged = excluded.flagged, answered = excluded.answered
             RETURNING id",
        )?;
        let mut fts = tx.prepare("INSERT OR REPLACE INTO mail_fts (rowid, terms) VALUES (?1, ?2)")?;
        for h in headers {
            let id: i64 = ins.query_row(
                params![
                    account,
                    folder,
                    h.uid,
                    h.date,
                    h.from_name,
                    h.from_addr,
                    h.recipients,
                    h.subject,
                    h.message_id,
                    h.in_reply_to,
                    h.flags.seen,
                    h.flags.flagged,
                    h.flags.answered,
                    h.size,
                    h.attachments
                ],
                |r| r.get(0),
            )?;
            fts.execute(params![id, index_text(&format!("{} {} {} {} {}", h.subject, h.from_name, h.from_addr, h.recipients, h.recipient_addrs))])?;
        }
    }
    tx.commit()
}

/// Отметки, изменившиеся на сервере.
pub fn set_flags(db: &mut Connection, account: &str, folder: &str, changed: &[(u32, Flags)]) -> Result<()> {
    let tx = db.transaction()?;
    {
        let mut up = tx.prepare("UPDATE mail_message SET seen = ?4, flagged = ?5, answered = ?6 WHERE account = ?1 AND folder = ?2 AND uid = ?3")?;
        for (uid, f) in changed {
            up.execute(params![account, folder, uid, f.seen, f.flagged, f.answered])?;
        }
    }
    tx.commit()
}

/// Письма, которых на сервере больше нет (удалены или перенесены).
pub fn remove(db: &mut Connection, account: &str, folder: &str, uids: &[u32]) -> Result<()> {
    let tx = db.transaction()?;
    {
        let mut fts = tx.prepare("DELETE FROM mail_fts WHERE rowid = (SELECT id FROM mail_message WHERE account = ?1 AND folder = ?2 AND uid = ?3)")?;
        let mut del = tx.prepare("DELETE FROM mail_message WHERE account = ?1 AND folder = ?2 AND uid = ?3")?;
        for uid in uids {
            fts.execute(params![account, folder, uid])?;
            del.execute(params![account, folder, uid])?;
        }
    }
    tx.commit()
}

/// Строка списка писем.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Row {
    pub id: i64,
    pub account: String,
    pub folder: String,
    pub uid: u32,
    pub date: i64,
    /// Имя отправителя, если нет — адрес (в «Отправленных» и «Черновиках» — кому).
    pub who: String,
    pub subject: String,
    pub flags: Flags,
    pub attachments: bool,
}

/// Что показать: общую папку всех ящиков (по назначению) или папку одного ящика.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Place {
    Role(Role),
    Folder { account: String, name: String },
}

/// Найденное: письма и, если по запросу ничего не было, а в другой раскладке нашлось, — каким запросом искали.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Page {
    pub rows: Vec<Row>,
    /// «привет», если искали «ghbdtn» и нашлось только так.
    pub switched: Option<String>,
}

/// Письма места, новые сверху; с запросом — только подходящие ([`crate::search`]). `before` — дата и номер
/// последнего показанного письма: следующая страница начинается после него.
pub fn list(db: &Connection, place: &Place, query: &str, before: Option<(i64, i64)>, limit: u32) -> Result<Page> {
    let q = Query::parse(query);
    if query.trim().is_empty() || q.is_empty() {
        return Ok(Page { rows: rows(db, place, None, before, limit)?, switched: None });
    }
    let found = rows(db, place, q.fts().as_deref(), before, limit)?;
    if !found.is_empty() || before.is_some() {
        return Ok(Page { rows: found, switched: None });
    }
    // Ничего — может, набрано не в той раскладке: «ghbdtn» = «привет».
    if let Some(other) = other_layout(query) {
        if let Some(expr) = Query::parse(&other).fts() {
            let again = rows(db, place, Some(&expr), None, limit)?;
            if !again.is_empty() {
                return Ok(Page { rows: again, switched: Some(other) });
            }
        }
    }
    Ok(Page { rows: Vec::new(), switched: None })
}

fn rows(db: &Connection, place: &Place, fts: Option<&str>, before: Option<(i64, i64)>, limit: u32) -> Result<Vec<Row>> {
    let (from, to) = match place {
        Place::Role(Role::Sent | Role::Drafts) => (false, true),
        _ => (true, false),
    };
    let mut sql = String::from(
        "SELECT m.id, m.account, m.folder, m.uid, m.date, m.from_name, m.from_addr, m.recipients, m.subject, m.seen, m.flagged, m.answered, m.attachments
         FROM mail_message m JOIN mail_folder f ON f.account = m.account AND f.name = m.folder",
    );
    if fts.is_some() {
        sql.push_str(" JOIN mail_fts ON mail_fts.rowid = m.id");
    }
    sql.push_str(" WHERE ");
    let mut args: Vec<rusqlite::types::Value> = Vec::new();
    match place {
        Place::Role(role) => {
            sql.push_str("f.role = ?");
            args.push(role_code(*role).to_string().into());
        }
        Place::Folder { account, name } => {
            sql.push_str("m.account = ? AND m.folder = ?");
            args.push(account.clone().into());
            args.push(name.clone().into());
        }
    }
    if let Some(expr) = fts {
        sql.push_str(" AND mail_fts MATCH ?");
        args.push(expr.to_string().into());
    }
    if let Some((date, id)) = before {
        sql.push_str(" AND (m.date < ? OR (m.date = ? AND m.id < ?))");
        args.extend([date.into(), date.into(), id.into()]);
    }
    sql.push_str(" ORDER BY m.date DESC, m.id DESC LIMIT ?");
    args.push(i64::from(limit).into());
    let mut st = db.prepare(&sql)?;
    let out = st.query_map(rusqlite::params_from_iter(args), |r| {
        let name: String = r.get(5)?;
        let addr: String = r.get(6)?;
        let recipients: String = r.get(7)?;
        let who = if to && !recipients.is_empty() {
            recipients
        } else if from && !name.is_empty() {
            name
        } else {
            addr
        };
        Ok(Row {
            id: r.get(0)?,
            account: r.get(1)?,
            folder: r.get(2)?,
            uid: r.get(3)?,
            date: r.get(4)?,
            who,
            subject: r.get(8)?,
            flags: Flags { seen: r.get(9)?, flagged: r.get(10)?, answered: r.get(11)? },
            attachments: r.get(12)?,
        })
    })?;
    out.collect()
}

/// Сколько непрочитанных в общих папках — для меню.
pub fn unread(db: &Connection) -> Result<Vec<(String, u32)>> {
    let mut st = db.prepare(
        "SELECT f.role, COUNT(*) FROM mail_message m JOIN mail_folder f ON f.account = m.account AND f.name = m.folder
         WHERE m.seen = 0 GROUP BY f.role",
    )?;
    let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
    rows.collect()
}

/// Ящик отключили: его папки и письма на устройстве больше не нужны.
pub fn forget_account(db: &mut Connection, account: &str) -> Result<()> {
    let tx = db.transaction()?;
    tx.execute("DELETE FROM mail_fts WHERE rowid IN (SELECT id FROM mail_message WHERE account = ?1)", [account])?;
    tx.execute("DELETE FROM mail_message WHERE account = ?1", [account])?;
    tx.execute("DELETE FROM mail_folder WHERE account = ?1", [account])?;
    tx.commit()
}

/// Ящики, у которых на устройстве есть письма или папки.
pub fn accounts(db: &Connection) -> Result<Vec<String>> {
    db.prepare("SELECT DISTINCT account FROM mail_folder")?.query_map([], |r| r.get(0))?.collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        migrate(&db).unwrap();
        db
    }

    fn folder(name: &str, role: Role) -> Folder {
        Folder { name: name.into(), title: name.into(), role, selectable: true }
    }

    fn header(uid: u32, date: i64, subject: &str, from: &str) -> Header {
        Header { uid, date, subject: subject.into(), from_name: from.into(), from_addr: "x@example.com".into(), ..Default::default() }
    }

    #[test]
    fn unified_folders_newest_first_and_paging() {
        let mut db = db();
        save_folders(&mut db, "a@ya.ru", &[folder("INBOX", Role::Inbox), folder("Sent", Role::Sent)]).unwrap();
        save_folders(&mut db, "b@gmail.com", &[folder("INBOX", Role::Inbox)]).unwrap();
        save_messages(&mut db, "a@ya.ru", "INBOX", &[header(1, 100, "Отчёт за ноябрь", "Щукин"), header(2, 300, "Привет", "Юлия")]).unwrap();
        save_messages(&mut db, "b@gmail.com", "INBOX", &[header(7, 200, "Invoice", "Shop")]).unwrap();
        let page = list(&db, &Place::Role(Role::Inbox), "", None, 2).unwrap();
        assert_eq!(page.rows.iter().map(|r| r.subject.as_str()).collect::<Vec<_>>(), ["Привет", "Invoice"]);
        let last = page.rows.last().unwrap();
        let next = list(&db, &Place::Role(Role::Inbox), "", Some((last.date, last.id)), 2).unwrap();
        assert_eq!(next.rows.iter().map(|r| r.subject.as_str()).collect::<Vec<_>>(), ["Отчёт за ноябрь"]);
        assert!(list(&db, &Place::Role(Role::Sent), "", None, 10).unwrap().rows.is_empty());
    }

    #[test]
    fn search_forms_translit_and_layout() {
        let mut db = db();
        save_folders(&mut db, "a@ya.ru", &[folder("INBOX", Role::Inbox)]).unwrap();
        save_messages(&mut db, "a@ya.ru", "INBOX", &[header(1, 100, "Отчёт за ноябрь", "Щукин"), header(2, 300, "Привет", "Юлия")]).unwrap();
        let inbox = Place::Role(Role::Inbox);
        let subjects = |q: &str| list(&db, &inbox, q, None, 10).unwrap();
        assert_eq!(subjects("отчёты").rows.len(), 1);
        assert_eq!(subjects("otchet").rows[0].subject, "Отчёт за ноябрь");
        assert_eq!(subjects("Shchukin").rows.len(), 1);
        let p = subjects("ghbdtn");
        assert_eq!((p.rows.len(), p.switched.as_deref()), (1, Some("привет")));
        assert!(subjects("счёт").rows.is_empty());
    }

    #[test]
    fn flags_removals_and_new_uidvalidity() {
        let mut db = db();
        save_folders(&mut db, "a", &[folder("INBOX", Role::Inbox)]).unwrap();
        save_messages(&mut db, "a", "INBOX", &[header(1, 1, "один", "x"), header(2, 2, "два", "y")]).unwrap();
        set_flags(&mut db, "a", "INBOX", &[(1, Flags { seen: true, ..Default::default() })]).unwrap();
        assert_eq!(unread(&db).unwrap(), [("inbox".to_string(), 1)]);
        remove(&mut db, "a", "INBOX", &[2]).unwrap();
        assert_eq!(known(&db, "a", "INBOX").unwrap().len(), 1);
        assert!(list(&db, &Place::Role(Role::Inbox), "два", None, 10).unwrap().rows.is_empty());
        reset_folder(&mut db, "a", "INBOX", 77).unwrap();
        assert_eq!(uidvalidity(&db, "a", "INBOX").unwrap(), Some(77));
        assert!(known(&db, "a", "INBOX").unwrap().is_empty());
        // Папки больше нет на сервере — уходит с письмами.
        save_messages(&mut db, "a", "INBOX", &[header(5, 5, "пять", "z")]).unwrap();
        save_folders(&mut db, "a", &[]).unwrap();
        assert_eq!(db.query_row("SELECT COUNT(*) FROM mail_fts", [], |r| r.get::<_, i64>(0)).unwrap(), 0);
        forget_account(&mut db, "a").unwrap();
        assert!(accounts(&db).unwrap().is_empty());
    }
}
