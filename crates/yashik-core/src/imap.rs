//! IMAP: вход в ящик и его папки. Соединение — только шифрованное: TLS сразу (порт 993), сертификат
//! сервера проверяется по корням Mozilla (одинаково на Windows и Android).

use crate::account::{Account, Server};
use async_imap::types::NameAttribute;
use async_imap::{Client, Session};
use base64::Engine;
use futures::TryStreamExt;
use serde::Serialize;
use std::fmt::Debug;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;

/// Сколько ждать сервер на каждом шаге: соединение, приветствие, вход, список папок.
pub(crate) const WAIT: Duration = Duration::from_secs(20);

/// Назначение папки — из ответа сервера (SPECIAL-USE) или, если сервер его не сообщает, по названию.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Inbox,
    Sent,
    Drafts,
    Junk,
    Trash,
    Archive,
    All,
    Flagged,
    Other,
}

/// Папка ящика.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Folder {
    /// Имя на сервере (в IMAP — modified UTF-7): по нему папку открывают.
    pub name: String,
    /// Имя для экрана: «Входящие», «Работа/Проекты».
    pub title: String,
    pub role: Role,
    /// Можно ли открыть: у «[Gmail]» — нельзя, это только узел.
    pub selectable: bool,
}

/// Почему не получилось — словами для человека.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// Нет связи с сервером: стоит попробовать следующий.
    Network(String),
    /// Сервер не пустил: неверный пароль или выключен доступ почтовых программ.
    Login(String),
    /// Остальное: сертификат, непонятный ответ сервера.
    Other(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Network(s) | Error::Login(s) | Error::Other(s) => f.write_str(s),
        }
    }
}

impl std::error::Error for Error {}

/// Проверка ящика перед подключением: входит и читает папки. Серверы — по порядку; следующий пробуется,
/// только если к предыдущему нет связи (неверный пароль на другом сервере верным не станет).
/// Возвращает сервер, к которому удалось подключиться, — его и стоит записать в ящик.
pub async fn check(account: &Account, servers: &[Server]) -> Result<(Server, Vec<Folder>), Error> {
    check_with(account, servers, mozilla_roots()).await
}

/// Папки ящика.
pub async fn folders(account: &Account) -> Result<Vec<Folder>, Error> {
    check(account, std::slice::from_ref(&account.imap)).await.map(|(_, f)| f)
}

async fn check_with(account: &Account, servers: &[Server], roots: Arc<rustls::RootCertStore>) -> Result<(Server, Vec<Folder>), Error> {
    let mut last = Error::Other("не указан сервер IMAP".to_string());
    for server in servers {
        match connect(server, roots.clone()).await {
            Ok(client) => {
                let mut session = login(client, account, server).await?;
                let list = list_folders(&mut session, server).await;
                let _ = tokio::time::timeout(WAIT, session.logout()).await;
                return list.map(|f| (server.clone(), f));
            }
            Err(e @ Error::Network(_)) => last = e,
            Err(e) => return Err(e),
        }
    }
    Err(last)
}

pub(crate) fn mozilla_roots() -> Arc<rustls::RootCertStore> {
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    Arc::new(roots)
}

/// Шаг с ожиданием: не дождались — нет связи.
pub(crate) async fn timed<T, E>(server: &Server, step: impl Future<Output = Result<T, E>>, err: impl FnOnce(E) -> Error) -> Result<T, Error> {
    match tokio::time::timeout(WAIT, step).await {
        Ok(r) => r.map_err(err),
        Err(_) => Err(Error::Network(format!("{} не отвечает {} секунд — проверьте интернет.", server.host, WAIT.as_secs()))),
    }
}

/// Соединение с проверкой сертификата.
pub(crate) async fn connect(server: &Server, roots: Arc<rustls::RootCertStore>) -> Result<Client<TlsStream<TcpStream>>, Error> {
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()
        .map_err(|e| Error::Other(format!("шифрование: {e}")))?
        .with_root_certificates(roots)
        .with_no_client_auth();
    let name = rustls::pki_types::ServerName::try_from(server.host.clone()).map_err(|_| Error::Other(format!("неверное имя сервера: {}", server.host)))?;
    let tcp = timed(server, TcpStream::connect((server.host.as_str(), server.port)), |e| {
        Error::Network(format!("Нет связи с {}:{} — проверьте интернет ({e}).", server.host, server.port))
    })
    .await?;
    let tls = timed(server, tokio_rustls::TlsConnector::from(Arc::new(config)).connect(name, tcp), |e| tls_error(server, e)).await?;
    Ok(Client::new(tls))
}

/// Ошибка рукопожатия: сертификат не тот — не сеть, а повод насторожиться.
fn tls_error(server: &Server, e: std::io::Error) -> Error {
    match e.get_ref().and_then(|inner| inner.downcast_ref::<rustls::Error>()) {
        Some(rustls::Error::InvalidCertificate(why)) => {
            Error::Other(format!("Сертификат сервера {} не прошёл проверку ({why:?}) — соединение прервано, пароль не отправлен.", server.host))
        }
        Some(other) => Error::Other(format!("Не удалось установить шифрованное соединение с {}: {other}.", server.host)),
        None => Error::Network(format!("Связь с {} прервалась при установке шифрования ({e}).", server.host)),
    }
}

/// Приветствие сервера и вход по паролю приложения.
pub(crate) async fn login<S: AsyncRead + AsyncWrite + Unpin + Debug + Send>(mut client: Client<S>, account: &Account, server: &Server) -> Result<Session<S>, Error> {
    let lost = || Error::Network(format!("{} закрыл соединение.", server.host));
    timed(server, client.read_response(), |e| Error::Network(format!("{}: {e}", server.host))).await?.ok_or_else(lost)?;
    match tokio::time::timeout(WAIT, client.login(&account.login, &account.password)).await {
        Err(_) => Err(Error::Network(format!("{} не отвечает {} секунд — проверьте интернет.", server.host, WAIT.as_secs()))),
        Ok(Ok(session)) => Ok(session),
        Ok(Err((async_imap::error::Error::No(text), _))) => {
            Err(Error::Login(format!("{} не пустил: {}. {}", server.host, text.trim(), account.provider.login_hint())))
        }
        Ok(Err((async_imap::error::Error::Io(e), _))) => Err(Error::Network(format!("{}: {e}", server.host))),
        Ok(Err((async_imap::error::Error::ConnectionLost, _))) => Err(lost()),
        Ok(Err((e, _))) => Err(Error::Other(format!("{} ответил непонятно: {e}", server.host))),
    }
}

/// Все папки: сначала «Входящие», «Отправленные», «Черновики», «Спам», «Корзина», потом остальные по алфавиту.
pub(crate) async fn list_folders<S: AsyncRead + AsyncWrite + Unpin + Debug + Send>(session: &mut Session<S>, server: &Server) -> Result<Vec<Folder>, Error> {
    let names = timed(
        server,
        async { session.list(Some(""), Some("*")).await?.try_collect::<Vec<_>>().await },
        |e| Error::Other(format!("{} не отдал список папок: {e}", server.host)),
    )
    .await?;
    let mut out: Vec<Folder> = names.iter().map(|n| folder(n.name(), n.delimiter(), n.attributes())).collect();
    out.sort_by(|a, b| a.role.cmp(&b.role).then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase())));
    Ok(out)
}

fn folder(name: &str, delimiter: Option<&str>, attrs: &[NameAttribute<'_>]) -> Folder {
    let decoded = decode_utf7(name);
    let role = role(name, &decoded, delimiter, attrs);
    let title = if role == Role::Inbox { "Входящие".to_string() } else { decoded };
    Folder { name: name.to_string(), title, role, selectable: !attrs.iter().any(|a| matches!(a, NameAttribute::NoSelect)) }
}

/// Назначение папки: по SPECIAL-USE (RFC 6154), иначе по последней части названия.
fn role(name: &str, title: &str, delimiter: Option<&str>, attrs: &[NameAttribute<'_>]) -> Role {
    if name.eq_ignore_ascii_case("INBOX") {
        return Role::Inbox;
    }
    for a in attrs {
        match a {
            NameAttribute::Sent => return Role::Sent,
            NameAttribute::Drafts => return Role::Drafts,
            NameAttribute::Junk => return Role::Junk,
            NameAttribute::Trash => return Role::Trash,
            NameAttribute::Archive => return Role::Archive,
            NameAttribute::All => return Role::All,
            NameAttribute::Flagged => return Role::Flagged,
            _ => {}
        }
    }
    let last = match delimiter {
        Some(d) if !d.is_empty() => title.rsplit(d).next().unwrap_or(title),
        _ => title,
    };
    match last.trim().to_lowercase().as_str() {
        "sent" | "sent items" | "sent messages" | "sent mail" | "отправленные" => Role::Sent,
        "drafts" | "draft" | "черновики" => Role::Drafts,
        "spam" | "junk" | "junk e-mail" | "junk mail" | "bulk mail" | "спам" => Role::Junk,
        "trash" | "deleted" | "deleted items" | "deleted messages" | "bin" | "корзина" | "удаленные" | "удалённые" => Role::Trash,
        _ => Role::Other,
    }
}

/// Имя папки из modified UTF-7 (RFC 3501, 5.1.3): «&BB4EQgQ,BEAEMAQyBDsENQQ9BD0ESwQ1-» — «Отправленные».
/// Испорченный кусок остаётся как есть.
pub fn decode_utf7(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        let Some(end) = after.find('-') else {
            out.push_str(&rest[i..]);
            return out;
        };
        if end == 0 {
            out.push('&');
        } else {
            let decoded = base64::engine::general_purpose::STANDARD_NO_PAD
                .decode(after[..end].replace(',', "/"))
                .ok()
                .filter(|b| b.len() % 2 == 0)
                .map(|b| String::from_utf16_lossy(&b.chunks(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect::<Vec<_>>()));
            match decoded {
                Some(text) => out.push_str(&text),
                None => out.push_str(&rest[i..i + 2 + end]),
            }
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::account::Provider;
    use std::net::TcpListener;
    use std::process::{Child, Command};

    #[test]
    fn utf7_names() {
        assert_eq!(decode_utf7("&BB4EQgQ,BEAEMAQyBDsENQQ9BD0ESwQ1-"), "Отправленные");
        assert_eq!(decode_utf7("&BCAEMAQxBD4EQgQw-/&BB8EQAQ+BDUEOgRCBEs-"), "Работа/Проекты");
        assert_eq!(decode_utf7("Tom &- Jerry"), "Tom & Jerry");
        assert_eq!(decode_utf7("INBOX"), "INBOX");
        assert_eq!(decode_utf7("&broken"), "&broken");
        assert_eq!(decode_utf7("&%%%-x"), "&%%%-x");
    }

    #[test]
    fn roles_by_flag_and_by_name() {
        assert_eq!(role("INBOX", "INBOX", Some("/"), &[]), Role::Inbox);
        assert_eq!(role("[Gmail]/Sent Mail", "[Gmail]/Sent Mail", Some("/"), &[NameAttribute::Sent]), Role::Sent);
        assert_eq!(role("Spam", "Spam", Some("|"), &[]), Role::Junk);
        assert_eq!(role("x", "Корзина", Some("/"), &[]), Role::Trash);
        assert_eq!(role("x", "Работа/Черновики", Some("/"), &[]), Role::Drafts);
        assert_eq!(role("Outbox", "Outbox", Some("/"), &[]), Role::Other);
        let f = folder("[Gmail]", Some("/"), &[NameAttribute::NoSelect]);
        assert!(!f.selectable);
        assert_eq!(folder("INBOX", Some("/"), &[]).title, "Входящие");
    }

    /// Сервер IMAP для проверок: pymap (`pip install pymap`) с ящиком-образцом в памяти —
    /// demouser / demopass, папки INBOX, Sent, Trash. Путь к программе — в YASHIK_PYMAP; нет — проверки пропускаются.
    pub(crate) struct Pymap {
        child: Child,
        pub(crate) port: u16,
    }

    impl Drop for Pymap {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    fn free_port() -> u16 {
        TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
    }

    pub(crate) fn pymap() -> Option<Pymap> {
        let Ok(bin) = std::env::var("YASHIK_PYMAP") else {
            eprintln!("YASHIK_PYMAP не задан — проверка с настоящим сервером IMAP пропущена");
            return None;
        };
        let port = free_port();
        let child = Command::new(bin).args(["--port", &port.to_string(), "--no-tls", "dict", "--demo-data"]).spawn().expect("pymap не запустился");
        let p = Pymap { child, port };
        for _ in 0..100 {
            if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
                return Some(p);
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        panic!("pymap не открыл порт {port}");
    }

    pub(crate) fn demo(password: &str, server: Server) -> Account {
        Account {
            email: "demouser@example.com".into(),
            name: String::new(),
            provider: Provider::Other,
            imap: server.clone(),
            smtp: server,
            login: "demouser".into(),
            password: password.into(),
        }
    }

    pub(crate) fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap()
    }

    #[test]
    fn folders_from_a_real_server() {
        let Some(p) = pymap() else { return };
        let server = Server::new("127.0.0.1", p.port);
        rt().block_on(async {
            let tcp = TcpStream::connect(("127.0.0.1", p.port)).await.unwrap();
            let mut session = login(Client::new(tcp), &demo("demopass", server.clone()), &server).await.unwrap();
            let folders = list_folders(&mut session, &server).await.unwrap();
            let roles: Vec<(Role, &str)> = folders.iter().map(|f| (f.role, f.title.as_str())).collect();
            assert_eq!(roles, [(Role::Inbox, "Входящие"), (Role::Sent, "Sent"), (Role::Trash, "Trash")]);
            assert!(folders.iter().all(|f| f.selectable));
        });
    }

    #[test]
    fn wrong_password_says_what_to_check() {
        let Some(p) = pymap() else { return };
        let server = Server::new("127.0.0.1", p.port);
        rt().block_on(async {
            let tcp = TcpStream::connect(("127.0.0.1", p.port)).await.unwrap();
            let mut account = demo("wrong", server.clone());
            account.provider = Provider::Yandex;
            let Err(Error::Login(text)) = login(Client::new(tcp), &account, &server).await else { panic!("ожидался отказ во входе") };
            assert!(text.contains("пароль приложения"), "{text}");
        });
    }

    /// TLS-посредник перед pymap со своим сертификатом: проверяет шифрованный путь целиком.
    async fn tls_front(upstream: u16) -> (u16, rustls::pki_types::CertificateDer<'static>) {
        let key = rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();
        let cert = key.cert.der().clone();
        let private = rustls::pki_types::PrivateKeyDer::Pkcs8(key.signing_key.serialize_der().into());
        let config = rustls::ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(vec![cert.clone()], private)
            .unwrap();
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(config));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            while let Ok((tcp, _)) = listener.accept().await {
                let acceptor = acceptor.clone();
                tokio::spawn(async move {
                    let Ok(mut tls) = acceptor.accept(tcp).await else { return };
                    let Ok(mut up) = TcpStream::connect(("127.0.0.1", upstream)).await else { return };
                    let _ = tokio::io::copy_bidirectional(&mut tls, &mut up).await;
                });
            }
        });
        (port, cert)
    }

    #[test]
    fn tls_checks_the_certificate_and_tries_the_next_server() {
        let Some(p) = pymap() else { return };
        rt().block_on(async {
            let (port, cert) = tls_front(p.port).await;
            let good = Server::new("localhost", port);
            let account = demo("demopass", good.clone());
            let mut trusted = rustls::RootCertStore::empty();
            trusted.add(cert).unwrap();
            let trusted = Arc::new(trusted);

            // Первый сервер недоступен — подключаемся ко второму.
            let dead = Server::new("127.0.0.1", free_port());
            let (used, folders) = check_with(&account, &[dead, good.clone()], trusted).await.unwrap();
            assert_eq!(used, good);
            assert_eq!(folders.len(), 3);

            // Сертификату, которого нет среди корней Mozilla, не верим — и пароль не отправляем.
            let Err(Error::Other(text)) = check_with(&account, std::slice::from_ref(&good), mozilla_roots()).await else { panic!("сертификат не проверен") };
            assert!(text.contains("Сертификат"), "{text}");
        });
    }
}
