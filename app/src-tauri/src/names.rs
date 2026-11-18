//! Чем «Ящик» отличается от других приложений на общем модуле kit: название для сообщений
//! и строки, которыми ключи привязаны к «Ящика».
//!
//! **Строки ключей у выпущенного приложения менять нельзя.** С другими строками система не расшифрует
//! ключ базы, пароль облака и ключи связи, которые уже лежат на устройствах.

use kit_core::AppNames;
use kit_secure::KeyNames;

pub const APP: AppNames = yashik_core::NAMES;

pub const KEYS: KeyNames = KeyNames { dpapi_entropy: b"yashik-db-key-v1", dpapi_label: "Yashik", keystore_alias: "yashik-db-key" };

/// Шифрует секрет средствами системы: DPAPI на Windows, Android Keystore на телефоне;
/// на Linux (разработка) секрет остаётся как есть.
pub fn protect(secret: &[u8]) -> kit_secure::Result<Vec<u8>> {
    kit_secure::protect(&KEYS, secret)
}

pub fn unprotect(blob: &[u8]) -> kit_secure::Result<Vec<u8>> {
    kit_secure::unprotect(&KEYS, blob)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Проверка падает, если строки ключей поменять: тогда на устройствах перестанут открываться база,
    /// облако и связанные устройства.
    #[test]
    fn key_names_are_kept() {
        assert_eq!(KEYS.dpapi_entropy, b"yashik-db-key-v1");
        assert_eq!(KEYS.dpapi_label, "Yashik");
        assert_eq!(KEYS.keystore_alias, "yashik-db-key");
        assert_eq!(APP.env("LOCK_AFTER_SECS"), "YASHIK_LOCK_AFTER_SECS");
    }

    #[test]
    fn secret_roundtrip() {
        let blob = protect(&[7u8; 32]).unwrap();
        assert_eq!(unprotect(&blob).unwrap(), [7u8; 32]);
    }
}
