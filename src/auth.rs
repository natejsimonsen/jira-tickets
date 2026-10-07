//! Finds the Jira API token. Order: JIRA_API_TOKEN, the saved token file,
//! the config's api_token reference (which may run `op read`), then the
//! paste prompt in the window (handled by the app).

use std::io::Write;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug)]
pub enum Source {
    Env,
    SavedFile,
    ConfigRef,
    Pasted,
}

impl Source {
    pub fn describe(self) -> &'static str {
        match self {
            Source::Env => "JIRA_API_TOKEN",
            Source::SavedFile => "saved token",
            Source::ConfigRef => "config api_token",
            Source::Pasted => "pasted token",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Login {
    pub email: String,
    pub token: String,
    pub source: Source,
}

pub fn find_login(cfg_email: &str, cfg_token_ref: Option<&str>) -> Option<Login> {
    let email = std::env::var("JIRA_EMAIL").ok().filter(|e| !e.trim().is_empty()).unwrap_or_else(|| cfg_email.to_string());
    if let Ok(t) = std::env::var("JIRA_API_TOKEN")
        && !t.trim().is_empty()
    {
        return Some(Login { email, token: t.trim().to_string(), source: Source::Env });
    }
    if let Some(t) = read_token_file() {
        return Some(Login { email, token: t, source: Source::SavedFile });
    }
    if let Some(r) = cfg_token_ref
        && let Ok(t) = crate::config::resolve_secret(r)
        && !t.is_empty()
    {
        let _ = save_token(&t);
        return Some(Login { email, token: t, source: Source::ConfigRef });
    }
    None
}

/// macOS: ~/Library/Application Support/jira-tickets/token
/// Linux: ~/.config/jira-tickets/token
pub fn token_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("JIRA_TICKETS_TOKEN_FILE") {
        return Some(PathBuf::from(p));
    }
    Some(dirs::config_dir()?.join("jira-tickets").join("token"))
}

fn read_token_file() -> Option<String> {
    let t = std::fs::read_to_string(token_path()?).ok()?;
    let t = t.trim();
    (!t.is_empty()).then(|| t.to_string())
}

/// 0600 on unix; the directory 0700.
pub fn save_token(token: &str) -> Result<(), String> {
    let path = token_path().ok_or("no config folder")?;
    let dir = path.parent().ok_or("no parent folder")?;
    let mut db = std::fs::DirBuilder::new();
    db.recursive(true);
    let mut oo = std::fs::OpenOptions::new();
    oo.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
        db.mode(0o700);
        oo.mode(0o600);
    }
    db.create(dir).map_err(|e| e.to_string())?;
    let tmp = dir.join("token.tmp");
    let mut f = oo.open(&tmp).map_err(|e| e.to_string())?;
    f.write_all(token.as_bytes()).map_err(|e| e.to_string())?;
    drop(f);
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

pub fn forget_token() {
    if let Some(p) = token_path() {
        let _ = std::fs::remove_file(p);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_token_file<R>(f: impl FnOnce(&std::path::Path) -> R) -> R {
        let _g = crate::testutil::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("token");
        unsafe {
            std::env::set_var("JIRA_TICKETS_TOKEN_FILE", &p);
            std::env::remove_var("JIRA_API_TOKEN");
        }
        let r = f(&p);
        unsafe { std::env::remove_var("JIRA_TICKETS_TOKEN_FILE") };
        r
    }

    #[test]
    fn env_wins() {
        with_token_file(|_| {
            unsafe { std::env::set_var("JIRA_API_TOKEN", "env-tok") };
            let l = find_login("me@x.com", Some("plain-cfg")).unwrap();
            assert_eq!(l.token, "env-tok");
            assert!(matches!(l.source, Source::Env));
            unsafe { std::env::remove_var("JIRA_API_TOKEN") };
        });
    }

    #[test]
    fn saved_file_before_config() {
        with_token_file(|p| {
            std::fs::write(p, "file-tok\n").unwrap();
            let l = find_login("me@x.com", Some("plain-cfg")).unwrap();
            assert_eq!(l.token, "file-tok");
            assert!(matches!(l.source, Source::SavedFile));
        });
    }

    #[test]
    fn config_ref_is_saved() {
        with_token_file(|p| {
            let l = find_login("me@x.com", Some("plain-cfg")).unwrap();
            assert_eq!(l.token, "plain-cfg");
            assert!(matches!(l.source, Source::ConfigRef));
            assert_eq!(std::fs::read_to_string(p).unwrap(), "plain-cfg");
        });
    }

    #[test]
    fn nothing_found() {
        with_token_file(|_| assert!(find_login("me@x.com", None).is_none()));
    }
}
