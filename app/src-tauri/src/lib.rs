//! «Ящик»: мост между экранами и ядром `yashik-core`.
//!
//! Вход по PIN-коду, отпечатку и Windows Hello (`kit_tauri::guard`), связь по Wi-Fi и облако — общие,
//! из kit. Пока приложение заблокировано, команды с данными отвечают `locked`. Свои команды экранов
//! добавляются сюда и в `generate_handler!` ниже.

use yashik_core::Store;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};
use tauri::{Manager, State};

mod cloud;
mod lan;
mod mail;
mod names;

use kit_tauri::guard::{self, Lock};

/// Версия приложения — для экрана настроек.
pub(crate) const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

type Res<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

struct AppState {
    /// База — общая с обменом по Wi-Fi и облаком.
    store: Arc<Mutex<Store>>,
    /// Вход по PIN-коду: заблокировано ли приложение.
    guard: Lock,
}

impl AppState {
    /// База для команд с данными: пока приложение заблокировано, ответ — `locked`.
    fn store(&self) -> Res<MutexGuard<'_, Store>> {
        self.guard.check()?;
        self.store.lock().map_err(|_| "база занята — попробуйте ещё раз".to_string())
    }
}

/// Местные значения базы — для входа: там лежат хеш PIN-кода и счётчик ошибок.
struct StoreMeta(Arc<Mutex<Store>>);

impl guard::Meta for StoreMeta {
    fn get(&self, key: &str) -> Res<Option<String>> {
        self.0.lock().map_err(err)?.meta(key).map_err(err)
    }

    fn set(&self, key: &str, value: &str) -> Res<()> {
        self.0.lock().map_err(err)?.set_meta(key, value).map_err(err)
    }
}

// ——— Устройство и ключ базы ———

/// Как называть это устройство при связи: имя компьютера, на телефоне — «Телефон».
fn device_name() -> String {
    if cfg!(mobile) {
        return "Телефон".into();
    }
    let raw = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .ok()
        .or_else(|| std::fs::read_to_string("/etc/hostname").ok())
        .unwrap_or_default();
    let name: String = raw.trim().chars().filter(|c| !c.is_control()).take(40).collect();
    if name.is_empty() {
        "Компьютер".into()
    } else {
        name
    }
}

/// Номер этого устройства — в журнале записей и при связи.
#[derive(Serialize, Deserialize)]
struct Identity {
    id: String,
}

fn load_identity(dir: &Path) -> Res<Identity> {
    let path = dir.join("device.json");
    if let Ok(text) = std::fs::read_to_string(&path) {
        if let Ok(id) = serde_json::from_str::<Identity>(&text) {
            if !id.id.is_empty() {
                return Ok(id);
            }
        }
    }
    let id = Identity { id: yashik_core::new_id()? };
    write_atomic(&path, serde_json::to_string_pretty(&id).map_err(err)?.as_bytes())?;
    Ok(id)
}

fn write_atomic(path: &Path, data: &[u8]) -> Res<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, data).map_err(err)?;
    std::fs::rename(&tmp, path).map_err(err)
}

/// Секрет на диске: на Windows и Android — зашифрованный средствами системы (DPAPI, Android Keystore),
/// на Linux (разработка) — как есть.
fn read_secret(dir: &Path, name: &str) -> Res<Option<Vec<u8>>> {
    match std::fs::read(dir.join(format!("{name}.sec"))) {
        Ok(blob) => names::unprotect(&blob).map(Some).map_err(|e| format!("{name}: не достать из хранилища системы: {e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(err(e)),
    }
}

fn write_secret(dir: &Path, name: &str, data: &[u8]) -> Res<()> {
    let blob = names::protect(data).map_err(err)?;
    write_atomic(&dir.join(format!("{name}.sec")), &blob)
}

/// Ключ базы. Новый создаётся только для новой базы: если база есть, а ключа нет, — это ошибка,
/// а не повод начать с пустой базы.
fn load_key(dir: &Path, db: &Path) -> Res<[u8; 32]> {
    if let Some(k) = read_secret(dir, "key")? {
        return <[u8; 32]>::try_from(k.as_slice()).map_err(|_| "ключ базы повреждён".to_string());
    }
    if db.exists() {
        return Err("база есть, а ключа к ней нет — восстановите файл key.sec или резервную копию".into());
    }
    let mut key = [0u8; 32];
    getrandom::fill(&mut key).map_err(err)?;
    write_secret(dir, "key", &key)?;
    Ok(key)
}

// ——— Команды экранов ———

/// О приложении — для экрана настроек: версия, чем защищён ключ базы, номер устройства.
#[derive(Serialize)]
struct About {
    version: &'static str,
    key_method: &'static str,
    device: String,
    records: u64,
}

#[tauri::command]
fn about(state: State<'_, AppState>) -> Res<About> {
    let st = state.store()?;
    Ok(About { version: APP_VERSION, key_method: kit_secure::method(), device: st.device().to_string(), records: st.event_count().map_err(err)? })
}

/// Второй запуск: показать уже открытое окно.
#[cfg(desktop)]
fn show_main(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();
    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_main(app)));
    #[cfg(mobile)]
    let builder = builder.plugin(tauri_plugin_biometric::init()).plugin(tauri_plugin_barcode_scanner::init());
    builder
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let db = dir.join("yashik.db");
            let me = load_identity(&dir)?;
            let key = load_key(&dir, &db)?;
            let store = Arc::new(Mutex::new(yashik_core::open(&db, &key, &me.id)?));
            let handle = app.handle().clone();
            let hooks = guard::Hooks {
                // Пока приложение заблокировано, по прежнему коду связи не подключиться.
                on_lock: Box::new(move || lan::on_lock(&handle)),
                // Вернулись в приложение — заодно обменяться с облаком, если давно не обменивались.
                on_resume: Box::new(cloud::resumed),
            };
            let guard = Lock::new(names::APP, StoreMeta(store.clone()), hooks);
            app.manage(guard.clone());
            app.manage(cloud::Sky::new(cloud::settings(), store.clone(), None));
            app.manage(lan::Wifi::new(&yashik_core::LAN, &names::KEYS, store.clone(), me.id.clone(), device_name()));
            app.manage(AppState { store, guard });
            app.manage(mail::Mail::new());
            lan::start(app.handle().clone());
            cloud::start(app.handle().clone());
            mail::start(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            // Свернули или вернулись: через 5 минут в фоне — блокировка (с PIN-кодом).
            if let tauri::WindowEvent::Focused(focused) = event {
                guard::on_focus(window.app_handle(), *focused);
            }
        })
        .invoke_handler(tauri::generate_handler![
            guard::lock_status,
            guard::pin_unlock,
            guard::pin_set,
            guard::pin_off,
            guard::bio_unlock,
            guard::bio_set,
            guard::pin_recover,
            guard::lock_now,
            guard::app_visible,
            lan::lan_status,
            lan::lan_scan,
            lan::lan_code_start,
            lan::lan_code_stop,
            lan::lan_pair,
            lan::lan_pair_qr,
            lan::lan_sync_now,
            lan::lan_unpair,
            cloud::cloud_status,
            cloud::cloud_app_passwords,
            cloud::cloud_dav_login,
            cloud::cloud_password,
            cloud::cloud_sync_now,
            cloud::cloud_every,
            cloud::cloud_disconnect,
            mail::accounts,
            mail::account_add,
            mail::account_remove,
            mail::mail_page,
            mail::mail_status,
            mail::mail_sync_now,
            mail::messages,
            mail::unread,
            about,
        ])
        .run(tauri::generate_context!())
        .expect("«Ящик»: не удалось запустить");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_is_created_once_and_never_replaced() {
        let dir = std::env::temp_dir().join(format!("yashik-app-{}", yashik_core::new_id().unwrap()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("yashik.db");
        let k1 = load_key(&dir, &db).unwrap();
        assert_ne!(k1, [0u8; 32]);
        assert_eq!(load_key(&dir, &db).unwrap(), k1);
        // База есть, ключа нет — не создаём новый молча.
        std::fs::write(&db, b"x").unwrap();
        std::fs::remove_file(dir.join("key.sec")).unwrap();
        assert!(load_key(&dir, &db).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn device_keeps_its_number() {
        let dir = std::env::temp_dir().join(format!("yashik-id-{}", yashik_core::new_id().unwrap()));
        std::fs::create_dir_all(&dir).unwrap();
        let a = load_identity(&dir).unwrap().id;
        assert_eq!(load_identity(&dir).unwrap().id, a);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
