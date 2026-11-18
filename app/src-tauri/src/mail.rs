//! Ящики: подключить, показать, отключить. Вход проверяет ядро (`yashik_core::imap`); ящик с паролем
//! приложения — запись хранилища kit, она сама уходит на другие устройства.

use crate::{err, AppState, Res};
use serde::{Deserialize, Serialize};
use tauri::State;
use yashik_core::account::{self, Account, Provider};

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
pub async fn account_add(state: State<'_, AppState>, input: NewAccount) -> Res<Added> {
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
    Ok(Added { account: view(&account), folders: folders.len() })
}

/// Отключить ящик на всех устройствах. Письма на сервере остаются.
#[tauri::command]
pub fn account_remove(state: State<'_, AppState>, id: String) -> Res<()> {
    state.store()?.delete(account::KIND, &id).map_err(err)
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
