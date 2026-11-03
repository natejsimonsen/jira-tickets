//! Small JSON disk cache (~/Library/Caches/jira-tickets). Launch shows cached
//! data instantly, then refreshes in the background.

use serde::{Serialize, de::DeserializeOwned};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

/// Makes each save's temp file unique, so concurrent saves never share one.
static COUNTER: AtomicU64 = AtomicU64::new(0);

fn dir() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("JIRA_TICKETS_CACHE_DIR") {
        return Some(PathBuf::from(p));
    }
    Some(dirs::cache_dir()?.join("jira-tickets"))
}

/// `key` as a safe file name stem.
fn file_stem(key: &str) -> String {
    key.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).take(150).collect()
}

fn path(key: &str) -> Option<PathBuf> {
    Some(dir()?.join(format!("{}.json", file_stem(key))))
}

fn bytes_path(sub: &str, key: &str) -> Option<PathBuf> {
    Some(dir()?.join(sub).join(format!("{}.bin", file_stem(key))))
}

/// Raw bytes saved by `store_bytes` under the cache subfolder `sub`.
pub fn load_bytes(sub: &str, key: &str) -> Option<Vec<u8>> {
    std::fs::read(bytes_path(sub, key)?).ok()
}

/// Saves raw bytes (images) under the cache subfolder `sub`, atomically like `store`.
pub fn store_bytes(sub: &str, key: &str, bytes: &[u8]) {
    if let Some(p) = bytes_path(sub, key) {
        write_atomic(&p, bytes);
    }
}

fn write_atomic(p: &std::path::Path, bytes: &[u8]) {
    let (Some(dir), Some(name)) = (p.parent(), p.file_name().map(|f| f.to_string_lossy().into_owned())) else { return };
    let _ = std::fs::create_dir_all(dir);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp = dir.join(format!("{name}.{}.{n}.tmp", std::process::id()));
    if std::fs::write(&tmp, bytes).is_ok() {
        let _ = std::fs::rename(tmp, p);
    }
}

pub fn load<T: DeserializeOwned>(key: &str) -> Option<T> {
    let bytes = std::fs::read(path(key)?).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Write to a temp file and rename, so a crash never leaves half a file.
pub fn store<T: Serialize>(key: &str, value: &T) {
    let (Some(p), Ok(bytes)) = (path(key), serde_json::to_vec(value)) else { return };
    write_atomic(&p, &bytes);
}

#[cfg(test)]
mod tests {
    #[test]
    fn round_trip() {
        let _g = crate::testutil::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var("JIRA_TICKETS_CACHE_DIR", dir.path()) };
        super::store("k/1", &vec![1, 2, 3]);
        assert_eq!(super::load::<Vec<i32>>("k/1"), Some(vec![1, 2, 3]));
        // Concurrent saves of one key each use their own temp file and leave none behind.
        let saves: Vec<_> = (0..8).map(|i| std::thread::spawn(move || super::store("k/1", &vec![i; 1000]))).collect();
        saves.into_iter().for_each(|h| h.join().unwrap());
        let v = super::load::<Vec<i32>>("k/1").expect("a whole file");
        assert!(v.len() == 1000 && v.iter().all(|x| *x == v[0]));
        let leftovers = std::fs::read_dir(dir.path()).unwrap().filter(|e| e.as_ref().unwrap().path().extension().is_some_and(|x| x == "tmp")).count();
        assert_eq!(leftovers, 0);
        unsafe { std::env::remove_var("JIRA_TICKETS_CACHE_DIR") };
    }

    #[test]
    fn bytes_round_trip_in_a_subfolder() {
        let _g = crate::testutil::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var("JIRA_TICKETS_CACHE_DIR", dir.path()) };
        assert_eq!(super::load_bytes("avatars", "557058:a"), None);
        super::store_bytes("avatars", "557058:a", b"\x89PNG");
        assert_eq!(super::load_bytes("avatars", "557058:a").as_deref(), Some(&b"\x89PNG"[..]));
        assert!(dir.path().join("avatars/557058_a.bin").exists());
        unsafe { std::env::remove_var("JIRA_TICKETS_CACHE_DIR") };
    }
}
