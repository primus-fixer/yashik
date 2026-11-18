//! Связь по Wi-Fi: связанные устройства, код связи, поиск в сети и обмен в фоне — общие, из kit
//! (`kit_tauri::wifi`). Своё у «Ящика» — настройки связи (`yashik_core::LAN`) и строки ключей.

use yashik_core::Store;
use tauri::AppHandle;

kit_tauri::wifi_commands!(Store);

/// Связь по Wi-Fi в приложении — состояние для `app.manage`.
pub type Wifi = kit_tauri::wifi::Wifi<Store>;

/// Фон: открыть порты, когда нужно, находить связанные устройства и обмениваться.
pub fn start(app: AppHandle) {
    kit_tauri::wifi::start::<_, Store>(app);
}

/// Приложение заблокировалось: показанный код связи больше не действует.
pub fn on_lock(app: &AppHandle) {
    kit_tauri::wifi::on_lock::<_, Store>(app);
}
