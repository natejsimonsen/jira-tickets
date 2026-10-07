//! Small JSON disk cache (~/Library/Caches/jira-tickets). Launch shows cached
//! data instantly, then refreshes in the background.

use serde::{Serialize, de::DeserializeOwned};
use std::path::PathBuf;

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
    let tmp = p.with_extension("tmp");
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
        unsafe { std::env::remove_var("JIRA_TICKETS_CACHE_DIR") };
    }
}
