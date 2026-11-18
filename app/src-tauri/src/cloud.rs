//! Облако: Яндекс Диск по логину и паролю приложения или свой сервер WebDAV, пароль копий и обмен в фоне —
//! общие, из kit (`kit_tauri::cloud`). Своё у «Ящика» — формат облака (`yashik_core::CLOUD`) и где что
//! лежит в базе.

use yashik_core::Store;
use tauri::AppHandle;

kit_tauri::cloud_commands!(Store);

/// Облако в приложении — состояние для `app.manage`.
pub type Sky = kit_tauri::cloud::Sky<Store>;

/// Где что лежит. Менять нельзя: вход в облако на устройствах пропадёт.
pub fn settings() -> kit_tauri::cloud::Settings {
    kit_tauri::cloud::Settings { cloud: &yashik_core::CLOUD, backup: &yashik_core::BACKUP, keys: &crate::names::KEYS, secret: "cloud.password", backup_key: "backup.key" }
}

/// Обмен в фоне.
pub fn start(app: AppHandle) {
    kit_tauri::cloud::start::<_, Store>(app);
}

/// Вернулись в приложение: обменяться, если давно не обменивались.
pub fn resumed() {
    kit_tauri::cloud::resumed();
}
