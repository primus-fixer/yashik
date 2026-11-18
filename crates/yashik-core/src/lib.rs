//! Ядро «Ящика»: записи и чем «Ящик» отличается от других приложений на общем модуле kit.
//!
//! Хранилище записей (журнал «вид, номер, содержимое»), обмен по Wi-Fi, облако и формат копий — общие,
//! из kit (`kit_core::store`, `lan`, `cloud`, `backup`). Здесь — название, порты, метки и имена файлов.
//!
//! **Строки ниже у выпущенного приложения не меняются:** с другими метками и портами устройства перестанут
//! узнавать друг друга, облако и резервные копии — открываться. Проверка `strings_are_kept` падает, если их тронуть.

use std::path::Path;

pub use kit_core::store::{Event, EventBody, Store};

/// Как «Ящик» называется в сообщениях.
pub const NAMES: kit_core::AppNames = kit_core::AppNames { id: "yashik", name: "Ящик", name_acc: "Ящик", name_gen: "Ящика", feminine: false };

/// Версия — уходит другим устройствам при обмене.
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Вложения записей. Пока их нет; появятся — их тип, предел и поля со ссылками задаются здесь.
pub static BLOBS: kit_core::store::Blobs = kit_core::store::NO_BLOBS;

/// Обмен записями между устройствами: версия; больших файлов кусками нет.
pub const PROFILE: kit_core::sync::Profile = kit_core::sync::Profile { version: APP_VERSION, files_flag: false, old_peer: "", chunked_since: None };

/// Связь по Wi-Fi: свои порты и метки, чтобы приложения на kit в одной сети не мешали друг другу.
pub const LAN: kit_core::lan::Lan = kit_core::lan::Lan {
    names: NAMES,
    profile: &PROFILE,
    tcp_port: 47520,
    discovery_port: 47521,
    magic: *b"YSHL",
    query_magic: *b"YSHQ",
    answer_magic: *b"YSHA",
    joiner: b"yashik-joiner",
    host: b"yashik-host",
    personal: b"yashik-lan",
    not_ours: "это не «Ящик»",
    qr_scheme: "yashik://pair?",
    qr_version: 1,
};

fn no_blob(_mime: &str, _data: &[u8]) -> bool {
    false
}

/// Облако: файл ключа, метка файлов и расширения.
pub static CLOUD: kit_core::cloud::Format = kit_core::cloud::Format {
    names: NAMES,
    key_file: "yashik.key",
    magic: *b"YSHC",
    events_ext: ".yev",
    // Номера событий в базе — i64.
    max_seq: i64::MAX as u64,
    blobs: kit_core::cloud::Blobs { ext: ".ybl", max_file: 1024, what: "вложение", this: "это вложение", of: "вложения", feminine: false, check: no_blob },
    files: None,
};

/// Резервная копия: метка в начале файла, расширение и слова-пароли, которые перебирают первыми.
/// Пароль копий нужен и облаку: из него выводится ключ облака.
pub const BACKUP: kit_core::backup::Format = kit_core::backup::Format { magic: b"YASHIKBK", ext: "yashik", names: NAMES, common: &["yashik", "ящик"], records: &[] };

/// Открывает зашифрованную базу «Ящика». `device` — номер этого устройства.
pub fn open(path: &Path, key: &[u8; 32], device: &str) -> Result<Store, String> {
    Store::open(path, key, device, &BLOBS).map_err(|e| match e {
        kit_core::db::Open::WrongKey => "ключ не подходит к базе — восстановите файл key.sec или резервную копию".to_string(),
        kit_core::db::Open::Db(e) => format!("база не открылась: {e}"),
    })
}

/// Новый номер записи или устройства: 16 шестнадцатеричных знаков.
pub fn new_id() -> Result<String, String> {
    let mut b = [0u8; 8];
    getrandom::fill(&mut b).map_err(|e| format!("генератор случайных чисел: {e}"))?;
    Ok(b.iter().map(|x| format!("{x:02x}")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Строки, которые у выпущенного приложения менять нельзя.
    #[test]
    fn strings_are_kept() {
        assert_eq!(NAMES.id, "yashik");
        assert_eq!((LAN.tcp_port, LAN.discovery_port), (47520, 47521));
        assert_eq!(&LAN.magic, b"YSHL");
        assert_eq!(LAN.personal, b"yashik-lan");
        assert_eq!(CLOUD.key_file, "yashik.key");
        assert_eq!(&CLOUD.magic, b"YSHC");
        assert_eq!(BACKUP.magic, b"YASHIKBK");
    }

    #[test]
    fn records_live_in_the_store() {
        let dir = std::env::temp_dir().join(format!("yashik-core-{}", new_id().unwrap()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut store = open(&dir.join("yashik.db"), &[9u8; 32], "aaaa").unwrap();
        store.put("setting", "view", &"list").unwrap();
        assert_eq!(store.get::<String>("setting", "view").unwrap().as_deref(), Some("list"));
        drop(store);
        assert!(open(&dir.join("yashik.db"), &[8u8; 32], "aaaa").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ids_are_random_hex() {
        let a = new_id().unwrap();
        assert_eq!(a.len(), 16);
        assert!(a.bytes().all(|b| b.is_ascii_hexdigit()));
        assert_ne!(a, new_id().unwrap());
    }
}
