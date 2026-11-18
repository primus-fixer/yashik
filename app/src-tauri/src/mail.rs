//! Ящики: подключить, показать, отключить. Вход проверяет ядро (`yashik_core::imap`); ящик с паролем
//! приложения — запись хранилища kit, она сама уходит на другие устройства.

use crate::{err, AppState, Res};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};
use yashik_core::account::{self, Account, Provider};
use yashik_core::{maildb, mailsync};

/// Ящик для экрана — без пароля.
#[derive(Serialize)]
pub struct AccountView {
    id: String,
    email: String,
    name: String,
    provider: Provider,
    title: &'static str,
    /// Сервер, к которому подключились: у Яндекса это может быть imap.ya.ru.
    imap: String,
}

fn view(a: &Account) -> AccountView {
    AccountView { id: account::id_of(&a.email), email: a.email.clone(), name: a.name.clone(), provider: a.provider, title: a.provider.title(), imap: a.imap.host.clone() }
}

#[tauri::command]
pub fn accounts(state: State<'_, AppState>) -> Res<Vec<AccountView>> {
    let st = state.store()?;
    Ok(st.list::<Account>(account::KIND).map_err(err)?.iter().map(view).collect())
}

/// Новый ящик с экрана. Серверы для «Другой почты» — как ввели: «imap.example.ru» или с портом.
#[derive(Deserialize)]
pub struct NewAccount {
    email: String,
    password: String,
    #[serde(default)]
    name: String,
    provider: Provider,
    #[serde(default)]
    imap: String,
    #[serde(default)]
    smtp: String,
}

#[derive(Serialize)]
pub struct Added {
    account: AccountView,
    /// Сколько папок нашлось — вход проверен.
    folders: usize,
}

/// Подключить ящик: проверяет вход (и читает папки), только потом записывает.
#[tauri::command]
pub async fn account_add(state: State<'_, AppState>, mail: State<'_, Mail>, input: NewAccount) -> Res<Added> {
    let email = input.email.trim().to_string();
    if !account::looks_like_email(&email) {
        return Err("Похоже, в адресе ошибка: нужен вид имя@домен.ru".into());
    }
    let password = input.provider.clean_password(&input.password);
    if password.is_empty() {
        return Err("Введите пароль приложения".into());
    }
    let id = account::id_of(&email);
    if state.store()?.get::<Account>(account::KIND, &id).map_err(err)?.is_some() {
        return Err(format!("{email} уже подключён"));
    }
    let (servers, smtp) = match input.provider {
        Provider::Other => (
            vec![account::parse_server(&input.imap, 993).ok_or("Укажите сервер IMAP, например imap.example.ru")?],
            account::parse_server(&input.smtp, 465).ok_or("Укажите сервер SMTP, например smtp.example.ru")?,
        ),
        p => (p.imap(), p.smtp().ok_or("нет сервера отправки")?),
    };
    let draft = Account { email: email.clone(), name: input.name.trim().to_string(), provider: input.provider, imap: servers[0].clone(), smtp, login: email, password };
    let (imap, folders) = yashik_core::imap::check(&draft, &servers).await.map_err(|e| e.to_string())?;
    let account = Account { imap, ..draft };
    state.store()?.put(account::KIND, &id, &account).map_err(err)?;
    // Сразу загрузить письма нового ящика.
    mail.wake.notify_one();
    Ok(Added { account: view(&account), folders: folders.len() })
}

/// Отключить ящик на всех устройствах. Письма на сервере остаются; на этом устройстве — стираются.
#[tauri::command]
pub fn account_remove(state: State<'_, AppState>, id: String) -> Res<()> {
    let mut st = state.store()?;
    st.delete(account::KIND, &id).map_err(err)?;
    maildb::forget_account(st.conn_mut(), &id).map_err(err)
}

/// Страница почтовой службы: `password` — где создать пароль приложения, `settings` — доступ почтовых
/// программ (Яндекс). Открываются только адреса из ядра, не присланные экраном.
#[tauri::command]
pub fn mail_page(app: tauri::AppHandle, provider: Provider, page: String) -> Res<()> {
    let url = match page.as_str() {
        "password" => provider.app_password_url(),
        "settings" => provider.settings_url(),
        _ => None,
    }
    .ok_or("Для этой почты такой страницы нет")?;
    use tauri_plugin_opener::OpenerExt;
    app.opener().open_url(url, None::<&str>).map_err(err)
}

// ——— Загрузка писем в фоне ———

/// Как часто загружать письма, пока приложение открыто. Мгновенные уведомления — на следующих этапах.
const EVERY: Duration = Duration::from_secs(300);

/// Загрузка писем: при запуске, раз в 5 минут, после подключения ящика и по кнопке.
pub struct Mail {
    wake: tokio::sync::Notify,
    status: Mutex<Status>,
}

/// Состояние загрузки для экрана — оно же приходит событием `mail`.
#[derive(Clone, Default, Serialize)]
pub struct Status {
    running: bool,
    progress: Option<mailsync::Progress>,
    /// Когда загрузка закончилась последний раз (секунды Unix).
    last: Option<i64>,
    /// Сколько новых писем пришло за последнюю загрузку.
    new: usize,
    /// Ящики, которые не загрузились: адрес и что сказать.
    errors: Vec<(String, String)>,
}

impl Mail {
    pub fn new() -> Mail {
        Mail { wake: tokio::sync::Notify::new(), status: Mutex::new(Status::default()) }
    }

    fn update(&self, app: &AppHandle, f: impl FnOnce(&mut Status)) {
        let now = {
            let mut st = self.status.lock().unwrap_or_else(|e| e.into_inner());
            f(&mut st);
            st.clone()
        };
        let _ = app.emit("mail", now);
    }
}

pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            run(&app).await;
            let mail = app.state::<Mail>();
            tokio::select! {
                _ = mail.wake.notified() => {}
                _ = tokio::time::sleep(EVERY) => {}
            }
        }
    });
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// Одна загрузка всех ящиков. Пока приложение заблокировано, письма не загружаются.
async fn run(app: &AppHandle) {
    let state = app.state::<AppState>();
    let accounts: Vec<Account> = {
        let Ok(mut st) = state.store() else { return };
        let accounts: Vec<Account> = st.list(account::KIND).unwrap_or_default();
        // Ящик отключили на другом устройстве — его письма здесь больше не нужны.
        let keep: Vec<String> = accounts.iter().map(|a| account::id_of(&a.email)).collect();
        for gone in maildb::accounts(st.conn()).unwrap_or_default().into_iter().filter(|a| !keep.contains(a)) {
            let _ = maildb::forget_account(st.conn_mut(), &gone);
        }
        accounts
    };
    if accounts.is_empty() {
        return;
    }
    let mail = app.state::<Mail>();
    mail.update(app, |s| {
        s.running = true;
        s.progress = None;
    });
    let store = state.store.clone();
    let mut errors = Vec::new();
    let mut new = 0;
    for a in &accounts {
        let handle = app.clone();
        let progress = move |p: mailsync::Progress| handle.state::<Mail>().update(&handle, |s| s.progress = Some(p));
        match mailsync::sync_account(&store, a, &progress).await {
            Ok(stats) => new += stats.new,
            Err(e) => errors.push((a.email.clone(), e.to_string())),
        }
    }
    mail.update(app, |s| {
        s.running = false;
        s.progress = None;
        s.last = Some(now());
        s.new = new;
        s.errors = errors;
    });
}

#[tauri::command]
pub fn mail_status(mail: State<'_, Mail>) -> Status {
    mail.status.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Загрузить письма сейчас (кнопка, вход в приложение, возврат в него).
#[tauri::command]
pub fn mail_sync_now(mail: State<'_, Mail>) {
    mail.wake.notify_one();
}

/// Какие письма показать: общая папка всех ящиков или папка одного ящика.
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Where {
    Role { role: String },
    Folder { account: String, name: String },
}

/// Письма места, новые сверху, по 50; с запросом — найденные.
#[tauri::command]
pub fn messages(state: State<'_, AppState>, place: Where, query: String, before: Option<(i64, i64)>) -> Res<maildb::Page> {
    let place = match place {
        Where::Role { role } => maildb::Place::Role(maildb::role_from_code(&role).ok_or("неизвестная папка")?),
        Where::Folder { account, name } => maildb::Place::Folder { account, name },
    };
    let st = state.store()?;
    maildb::list(st.conn(), &place, &query, before, 50).map_err(err)
}

/// Непрочитанные по общим папкам — для меню.
#[tauri::command]
pub fn unread(state: State<'_, AppState>) -> Res<HashMap<String, u32>> {
    let st = state.store()?;
    Ok(maildb::unread(st.conn()).map_err(err)?.into_iter().collect())
}
