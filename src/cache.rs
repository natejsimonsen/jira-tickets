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

fn path(key: &str) -> Option<PathBuf> {
    let name: String = key.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).take(150).collect();
    Some(dir()?.join(format!("{name}.json")))
}

pub fn load<T: DeserializeOwned>(key: &str) -> Option<T> {
    let bytes = std::fs::read(path(key)?).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Write to a temp file and rename, so a crash never leaves half a file.
pub fn store<T: Serialize>(key: &str, value: &T) {
    let (Some(dir), Some(p)) = (dir(), path(key)) else { return };
    let _ = std::fs::create_dir_all(&dir);
    let Ok(bytes) = serde_json::to_vec(value) else { return };
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let Some(name) = p.file_name().map(|f| f.to_string_lossy().into_owned()) else { return };
    let tmp = dir.join(format!("{name}.{}.{n}.tmp", std::process::id()));
    if std::fs::write(&tmp, bytes).is_ok() {
        let _ = std::fs::rename(tmp, p);
    }
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
}
