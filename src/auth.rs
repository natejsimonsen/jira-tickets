//! Finds the Jira API token. Order: JIRA_API_TOKEN, the config's api_token
//! reference (resolved on every launch, e.g. `op read`, and never written to
//! disk), the saved token file (only ever a pasted token), then the paste
//! prompt in the window (handled by the app).

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
            Source::ConfigRef => "config api_token (resolved each launch)",
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

/// JIRA_EMAIL when set, else the config's email.
pub fn login_email(cfg_email: &str) -> String {
    std::env::var("JIRA_EMAIL").ok().filter(|e| !e.trim().is_empty()).unwrap_or_else(|| cfg_email.to_string())
}

/// The first token found, or the reasons each configured source failed
/// (empty when nothing was configured at all).
pub fn find_login(cfg_email: &str, cfg_token_ref: Option<&str>) -> Result<Login, Vec<String>> {
    let email = login_email(cfg_email);
    let mut errors = Vec::new();
    if let Ok(t) = std::env::var("JIRA_API_TOKEN")
        && !t.trim().is_empty()
    {
        return Ok(Login { email, token: t.trim().to_string(), source: Source::Env });
    }
    if let Some(r) = cfg_token_ref {
        match crate::config::resolve_secret(r) {
            Ok(t) if !t.is_empty() => return Ok(Login { email, token: t, source: Source::ConfigRef }),
            Ok(_) => errors.push("config api_token: resolved to an empty token".to_string()),
            Err(e) => errors.push(format!("config api_token: {e}")),
        }
    }
    if let Some(t) = read_token_file() {
        return Ok(Login { email, token: t, source: Source::SavedFile });
    }
    Err(errors)
}

/// The token for display: first 4 + "…" + last 4, or "****" when 8 chars or fewer.
pub fn mask_token(t: &str) -> String {
    let chars: Vec<char> = t.chars().collect();
    if chars.len() <= 8 {
        return "****".into();
    }
    let head: String = chars[..4].iter().collect();
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("{head}…{tail}")
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
    fn config_before_saved_file() {
        with_token_file(|p| {
            std::fs::write(p, "file-tok\n").unwrap();
            let l = find_login("me@x.com", Some("plain-cfg")).unwrap();
            assert_eq!(l.token, "plain-cfg");
            assert!(matches!(l.source, Source::ConfigRef));
        });
    }

    #[test]
    fn config_ref_is_not_saved() {
        with_token_file(|p| {
            let l = find_login("me@x.com", Some("plain-cfg")).unwrap();
            assert_eq!(l.token, "plain-cfg");
            assert!(matches!(l.source, Source::ConfigRef));
            assert!(!p.exists());
        });
    }

    #[test]
    fn falls_back_to_file_when_config_ref_fails() {
        with_token_file(|p| {
            std::fs::write(p, "file-tok\n").unwrap();
            let l = find_login("me@x.com", Some("$(exit 1)")).unwrap();
            assert_eq!(l.token, "file-tok");
            assert!(matches!(l.source, Source::SavedFile));
        });
    }

    #[test]
    fn nothing_found() {
        with_token_file(|_| assert_eq!(find_login("me@x.com", None).unwrap_err(), Vec::<String>::new()));
    }

    #[test]
    fn failed_config_ref_reports_its_error() {
        with_token_file(|_| {
            let errs = find_login("me@x.com", Some("$(exit 1)")).unwrap_err();
            assert_eq!(errs.len(), 1, "{errs:?}");
            assert!(errs[0].starts_with("config api_token: sh failed"), "{errs:?}");
        });
    }

    #[test]
    fn masks_tokens() {
        assert_eq!(mask_token("abcdefgh"), "****");
        assert_eq!(mask_token("abcd12345wxyz"), "abcd…wxyz");
    }

    #[test]
    fn jira_email_overrides_config() {
        let _g = crate::testutil::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        unsafe { std::env::set_var("JIRA_EMAIL", "env@x.com") };
        assert_eq!(login_email("cfg@x.com"), "env@x.com");
        unsafe { std::env::remove_var("JIRA_EMAIL") };
        assert_eq!(login_email("cfg@x.com"), "cfg@x.com");
    }
}
