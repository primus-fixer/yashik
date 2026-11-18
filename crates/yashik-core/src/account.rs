//! Ящики: адрес, сервер, вход. Ящик — запись `account` в хранилище kit: вместе с паролем приложения
//! она уходит на другие ваши устройства, и пароль вводится один раз.

use serde::{Deserialize, Serialize};

/// Вид записи ящика в хранилище.
pub const KIND: &str = "account";

/// Почтовая служба: от неё — серверы и где создают пароль приложения.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Yandex,
    Gmail,
    Mailru,
    Other,
}

/// Сервер: имя и порт; соединение всегда шифрованное (TLS сразу, порты 993 и 465).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Server {
    pub host: String,
    pub port: u16,
}

impl Server {
    pub fn new(host: &str, port: u16) -> Server {
        Server { host: host.to_string(), port }
    }
}

/// Ящик, как он хранится.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub email: String,
    /// Имя отправителя в письмах; пусто — только адрес.
    #[serde(default)]
    pub name: String,
    pub provider: Provider,
    pub imap: Server,
    pub smtp: Server,
    /// Имя для входа — обычно сам адрес.
    pub login: String,
    /// Пароль приложения.
    pub password: String,
}

const YANDEX: &[&str] = &["yandex.ru", "ya.ru", "yandex.com", "yandex.by", "yandex.kz", "yandex.ua", "yandex.uz", "yandex.com.tr", "narod.ru"];
const GMAIL: &[&str] = &["gmail.com", "googlemail.com"];
const MAILRU: &[&str] = &["mail.ru", "inbox.ru", "list.ru", "bk.ru", "internet.ru", "xmail.ru"];

impl Provider {
    /// Служба по адресу. Свой домен (например, рабочий на Яндекс 360) — `Other`, пока его не выберут вручную.
    pub fn of(email: &str) -> Provider {
        let domain = email.rsplit_once('@').map(|(_, d)| d.trim().to_lowercase()).unwrap_or_default();
        if YANDEX.contains(&domain.as_str()) {
            Provider::Yandex
        } else if GMAIL.contains(&domain.as_str()) {
            Provider::Gmail
        } else if MAILRU.contains(&domain.as_str()) {
            Provider::Mailru
        } else {
            Provider::Other
        }
    }

    /// Серверы IMAP по порядку: первый — основной; следующие пробуются, только если к предыдущему нет связи.
    /// У Яндекса второй — imap.ya.ru, для подключения не из России.
    pub fn imap(self) -> Vec<Server> {
        match self {
            Provider::Yandex => vec![Server::new("imap.yandex.ru", 993), Server::new("imap.ya.ru", 993)],
            Provider::Gmail => vec![Server::new("imap.gmail.com", 993)],
            Provider::Mailru => vec![Server::new("imap.mail.ru", 993)],
            Provider::Other => Vec::new(),
        }
    }

    /// Сервер отправки (SMTP, TLS сразу).
    pub fn smtp(self) -> Option<Server> {
        match self {
            Provider::Yandex => Some(Server::new("smtp.yandex.ru", 465)),
            Provider::Gmail => Some(Server::new("smtp.gmail.com", 465)),
            Provider::Mailru => Some(Server::new("smtp.mail.ru", 465)),
            Provider::Other => None,
        }
    }

    /// Название службы для экрана.
    pub fn title(self) -> &'static str {
        match self {
            Provider::Yandex => "Яндекс",
            Provider::Gmail => "Gmail",
            Provider::Mailru => "Mail.ru",
            Provider::Other => "Другая почта",
        }
    }

    /// Где создают пароль приложения.
    pub fn app_password_url(self) -> Option<&'static str> {
        match self {
            Provider::Yandex => Some("https://passport.yandex.ru/security/app-passwords"),
            Provider::Gmail => Some("https://myaccount.google.com/apppasswords"),
            Provider::Mailru => Some("https://account.mail.ru/user/2-step-auth/passwords/"),
            Provider::Other => None,
        }
    }

    /// Где в почте включают доступ почтовых программ (у Яндекса он выключается отдельно).
    pub fn settings_url(self) -> Option<&'static str> {
        match self {
            Provider::Yandex => Some("https://mail.yandex.ru/#setup/client"),
            _ => None,
        }
    }

    /// Пароль приложения — без пробелов: Google показывает его группами по четыре знака. У чужого
    /// сервера пароль может быть обычным, там убираются только пробелы по краям.
    pub fn clean_password(self, password: &str) -> String {
        match self {
            Provider::Other => password.trim().to_string(),
            _ => password.chars().filter(|c| !c.is_whitespace()).collect(),
        }
    }

    /// Что сказать, если сервер не пустил: где взять пароль приложения и что включить в почте.
    pub fn login_hint(self) -> &'static str {
        match self {
            Provider::Yandex => {
                "Нужен пароль приложения для Почты (passport.yandex.ru → Безопасность → Пароли приложений), а в Яндекс Почте — \
                 доступ почтовых программ: Все настройки → Почтовые программы → «С сервера imap.yandex.ru по протоколу IMAP» \
                 и «Пароли приложений и OAuth-токены»."
            }
            Provider::Gmail => {
                "Нужен пароль приложения (аккаунт Google → Безопасность → Пароли приложений; открывается при включённой \
                 двухэтапной проверке). Обычный пароль Google не подойдёт."
            }
            Provider::Mailru => "Нужен пароль для внешнего приложения: Mail.ru → Настройки → Безопасность → Пароли для внешних приложений.",
            Provider::Other => "Проверьте адрес, имя для входа и пароль; если у почты есть пароли приложений — нужен такой.",
        }
    }
}

/// Сервер, введённый руками: «imap.example.ru» или «imap.example.ru:993». Без схемы, пробелов и лишних знаков.
pub fn parse_server(text: &str, default_port: u16) -> Option<Server> {
    let text = text.trim().trim_end_matches('.');
    let (host, port) = match text.rsplit_once(':') {
        Some((h, p)) => (h, p.parse::<u16>().ok().filter(|p| *p > 0)?),
        None => (text, default_port),
    };
    let ok = !host.is_empty() && host.contains('.') && host.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-') && !host.starts_with(['.', '-']);
    ok.then(|| Server::new(&host.to_ascii_lowercase(), port))
}

/// Номер записи ящика — сам адрес в нижнем регистре: ящик, подключённый на двух устройствах, не задвоится.
pub fn id_of(email: &str) -> String {
    email.trim().to_lowercase()
}

/// Похоже ли на адрес почты: что-то@домен.зона, без пробелов.
pub fn looks_like_email(s: &str) -> bool {
    let s = s.trim();
    match s.rsplit_once('@') {
        Some((user, domain)) => !user.is_empty() && domain.contains('.') && !domain.starts_with('.') && !domain.ends_with('.') && !s.contains(char::is_whitespace),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_by_address() {
        assert_eq!(Provider::of("ivan@yandex.ru"), Provider::Yandex);
        assert_eq!(Provider::of("Ivan@Ya.Ru"), Provider::Yandex);
        assert_eq!(Provider::of("ivan@gmail.com"), Provider::Gmail);
        assert_eq!(Provider::of("ivan@bk.ru"), Provider::Mailru);
        // Свой домен на Яндекс 360 — выбирается вручную.
        assert_eq!(Provider::of("ivan@i-mt.net"), Provider::Other);
        assert_eq!(Provider::of("без собаки"), Provider::Other);
    }

    #[test]
    fn servers_are_encrypted_ports() {
        for p in [Provider::Yandex, Provider::Gmail, Provider::Mailru] {
            assert!(p.imap().iter().all(|s| s.port == 993), "{p:?}");
            assert_eq!(p.smtp().map(|s| s.port), Some(465), "{p:?}");
            assert!(p.app_password_url().is_some_and(|u| u.starts_with("https://")));
        }
        assert_eq!(Provider::Yandex.imap()[1].host, "imap.ya.ru");
        assert!(Provider::Other.imap().is_empty());
    }

    #[test]
    fn typed_servers_and_passwords() {
        assert_eq!(parse_server(" IMAP.Example.ru ", 993), Some(Server::new("imap.example.ru", 993)));
        assert_eq!(parse_server("mail.example.ru:143", 993), Some(Server::new("mail.example.ru", 143)));
        assert_eq!(parse_server("https://mail.example.ru", 993), None);
        assert_eq!(parse_server("mail example.ru", 993), None);
        assert_eq!(parse_server("localhost", 993), None);
        assert_eq!(parse_server("mail.example.ru:0", 993), None);
        assert_eq!(Provider::Gmail.clean_password("abcd efgh ijkl mnop"), "abcdefghijklmnop");
        assert_eq!(Provider::Other.clean_password(" pass word "), "pass word");
        assert_eq!(id_of(" Ivan@Yandex.RU "), "ivan@yandex.ru");
    }

    #[test]
    fn email_shape() {
        assert!(looks_like_email("ivan@yandex.ru"));
        assert!(looks_like_email(" ivan@i-mt.net "));
        assert!(!looks_like_email("ivan@yandex"));
        assert!(!looks_like_email("@yandex.ru"));
        assert!(!looks_like_email("iv an@yandex.ru"));
    }

    #[test]
    fn record_round_trip() {
        let a = Account {
            email: "ivan@yandex.ru".into(),
            name: "Иван".into(),
            provider: Provider::Yandex,
            imap: Server::new("imap.yandex.ru", 993),
            smtp: Server::new("smtp.yandex.ru", 465),
            login: "ivan@yandex.ru".into(),
            password: "abcd".into(),
        };
        let json = serde_json::to_value(&a).unwrap();
        assert_eq!(json["provider"], "yandex");
        assert_eq!(serde_json::from_value::<Account>(json).unwrap(), a);
    }
}
