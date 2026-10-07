//! The TOML config at ~/.config/jira-tickets/config.toml: site, scope,
//! grooming statuses, team roster, columns, roadmap field mapping.

#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub site: String,
    pub email: String,
    pub api_token: Option<String>,
    pub scope: Scope,
    pub grooming: Grooming,
    pub team: Team,
    pub issues: Issues,
    pub roadmap: Roadmap,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Scope {
    pub projects: Vec<String>,
    pub project: String,
    pub team_label: String,
    pub board_id: u64,
    pub filter_id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Grooming {
    pub status_new: String,
    pub status_planning: String,
    pub status_ready: String,
    pub status_backlog: String,
    pub planning_transition_id: String,
    pub planning_cap: usize,
    pub needs_detail_cap: usize,
    pub ready_soft_ceiling: usize,
    pub ready_hard_ceiling: usize,
    pub report_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Team {
    pub members: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Issues {
    pub columns: Vec<String>,
    pub filters: Vec<QuickFilter>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QuickFilter {
    pub name: String,
    pub jql: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Roadmap {
    pub epics: Vec<String>,
    pub lanes: BTreeMap<String, String>,
    pub horizon_by_status: BTreeMap<String, String>,
    pub health_field: Option<String>,
    pub target_field: Option<String>,
    pub start_field: Option<String>,
    pub confidence_field: Option<String>,
    pub next_milestone_field: Option<String>,
    pub definition_of_done_field: Option<String>,
}

fn s(v: &str) -> String {
    v.to_string()
}

impl Default for Scope {
    fn default() -> Self {
        Scope { projects: vec![s("CI"), s("OPS")], project: s("CI"), team_label: s("PLAT"), board_id: 101, filter_id: 201 }
    }
}

impl Default for Grooming {
    fn default() -> Self {
        Grooming {
            status_new: s("New"),
            status_planning: s("Planning"),
            status_ready: s("Ready for Work"),
            status_backlog: s("Backlog"),
            planning_transition_id: s("11"),
            planning_cap: 10,
            needs_detail_cap: 10,
            ready_soft_ceiling: 10,
            ready_hard_ceiling: 15,
            report_dir: s("~/Documents/grooming-reports"),
        }
    }
}

impl Default for Team {
    fn default() -> Self {
        Team { members: ["barbara", "edsger.dijkstra", "ada.lovelace", "grace", "sam", "alan"].map(s).to_vec() }
    }
}

impl Default for Issues {
    fn default() -> Self {
        Issues {
            columns: ["status", "labels", "parent.summary", "assignee"].map(s).to_vec(),
            filters: vec![
                QuickFilter { name: s("Only Opened"), jql: s("status not in (Closed)") },
                QuickFilter { name: s("To Do"), jql: s("statusCategory = \"To Do\"") },
            ],
        }
    }
}

impl Default for Roadmap {
    fn default() -> Self {
        Roadmap {
            epics: Vec::new(),
            lanes: BTreeMap::from([(s("PLAT"), s("Platform"))]),
            horizon_by_status: BTreeMap::from([
                (s("In Progress"), s("now")),
                (s("Ready for Work"), s("next")),
                (s("Planning"), s("next")),
                (s("New"), s("later")),
                (s("Backlog"), s("later")),
            ]),
            health_field: None,
            target_field: None,
            start_field: None,
            confidence_field: None,
            next_milestone_field: None,
            definition_of_done_field: None,
        }
    }
}

/// `JIRA_TICKETS_CONFIG` overrides the location, for tests and snapshots.
pub fn path() -> PathBuf {
    if let Ok(p) = std::env::var("JIRA_TICKETS_CONFIG") {
        return PathBuf::from(p);
    }
    dirs::home_dir().unwrap_or_default().join(".config").join("jira-tickets").join("config.toml")
}

pub fn load(path: &Path) -> Result<Config, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    let cfg: Config = toml::from_str(&text).map_err(|e| format!("parsing {}: {e}", path.display()))?;
    cfg.validate()?;
    Ok(cfg)
}

impl Config {
    pub fn validate(&self) -> Result<(), String> {
        if self.site.trim().is_empty() || self.site.contains("yourcompany") {
            return Err("config: site is required (e.g. https://yourcompany.atlassian.net)".into());
        }
        if self.email.trim().is_empty() || (self.email.trim() == "you@example.com") {
            return Err("config: email is required".into());
        }
        if self.scope.project.trim().is_empty() {
            return Err("config: scope.project is required".into());
        }
        Ok(())
    }

    pub fn report_dir(&self) -> PathBuf {
        expand_home(&self.grooming.report_dir)
    }

    pub fn browse_url(&self, key: &str) -> String {
        format!("{}/browse/{key}", self.site.trim_end_matches('/'))
    }
}

fn expand_home(p: &str) -> PathBuf {
    match p.strip_prefix("~/") {
        Some(rest) => dirs::home_dir().unwrap_or_default().join(rest),
        None => PathBuf::from(p),
    }
}

/// `op://vault/item/field` runs `op read`; `$(cmd)` runs `sh -c cmd`;
/// anything else is returned as is.
pub fn resolve_secret(value: &str) -> Result<String, String> {
    let v = value.trim();
    if v.starts_with("op://") {
        return op_read(v);
    }
    if let Some(inner) = v.strip_prefix("$(").and_then(|r| r.strip_suffix(')')) {
        return run("sh", &["-c", inner]);
    }
    Ok(v.to_string())
}

/// A Finder or Dock launch inherits launchd's PATH, which rarely has `op`;
/// when it is not found, retry through the user's login shell.
fn op_read(reference: &str) -> Result<String, String> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
    op_read_via("op", &shell, reference)
}

fn op_read_via(op: &str, shell: &str, reference: &str) -> Result<String, String> {
    match std::process::Command::new(op).args(["read", reference]).output() {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if reference.contains('\'') {
                return Err("op reference contains a single quote".into());
            }
            run(shell, &["-lc", &format!("op read '{reference}'")])
        }
        out => finish(op, out),
    }
}

fn run(name: &str, args: &[&str]) -> Result<String, String> {
    finish(name, std::process::Command::new(name).args(args).output())
}

fn finish(name: &str, out: std::io::Result<std::process::Output>) -> Result<String, String> {
    let out = out.map_err(|e| format!("running {name}: {e}"))?;
    if !out.status.success() {
        return Err(format!("{name} failed: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub fn write_example(path: &Path) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, EXAMPLE).map_err(|e| e.to_string())
}

pub const EXAMPLE: &str = r#"# Jira Tickets configuration

site = "https://yourcompany.atlassian.net"
email = "you@example.com"
# Optional. Plain token, "op://vault/item/field", or "$(command)".
# Resolved on every launch, before the saved token file; never written to disk.
# api_token = "op://Private/Jira API Token/credential"

[scope]
projects = ["CI", "OPS"]   # projects searched for team members' work
project = "CI"              # project the team label lives in
team_label = "PLAT"
board_id = 101
filter_id = 201

[grooming]
status_new = "New"
status_planning = "Planning"
status_ready = "Ready for Work"
status_backlog = "Backlog"
planning_transition_id = "11"
planning_cap = 10
needs_detail_cap = 10
ready_soft_ceiling = 10
ready_hard_ceiling = 15
report_dir = "~/Documents/grooming-reports"

[team]
# Jira display names or account ids, in column order. Replace these SSO
# usernames with the names Jira shows, or the Team view says "no tickets found".
members = ["barbara", "edsger.dijkstra", "ada.lovelace", "grace", "sam", "alan"]

[issues]
columns = ["status", "labels", "parent.summary", "assignee"]
[[issues.filters]]
name = "Only Opened"
jql = "status not in (Closed)"
[[issues.filters]]
name = "To Do"
jql = 'statusCategory = "To Do"'

[roadmap]
# epics = ["CI-1860"]            # optional allowlist
lanes = { PLAT = "Platform" }
horizon_by_status = { "In Progress" = "now", "Ready for Work" = "next", "Planning" = "next", "New" = "later", "Backlog" = "later" }
# health_field = "customfield_XXXXX"
# target_field = "customfield_XXXXX"   # default: duedate
# start_field = "customfield_XXXXX"
# confidence_field = "customfield_XXXXX"
# next_milestone_field = "customfield_XXXXX"
# definition_of_done_field = "customfield_XXXXX"
"#;

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = r#"
site = "https://example.atlassian.net"
email = "me@example.com"
"#;

    #[test]
    fn minimal_config_gets_defaults() {
        let c: Config = toml::from_str(MINIMAL).unwrap();
        assert_eq!(c.scope.project, "CI");
        assert_eq!(c.scope.team_label, "PLAT");
        assert_eq!(c.scope.board_id, 101);
        assert_eq!(c.grooming.planning_transition_id, "11");
        assert_eq!(c.grooming.planning_cap, 10);
        assert_eq!(c.issues.columns, vec!["status", "labels", "parent.summary", "assignee"]);
        assert_eq!(c.roadmap.lanes.get("PLAT").map(String::as_str), Some("Platform"));
        assert_eq!(c.roadmap.horizon_by_status.get("In Progress").map(String::as_str), Some("now"));
        assert!(c.validate().is_ok());
    }

    #[test]
    fn validate_rejects_missing_site_and_email() {
        let c: Config = toml::from_str("site = \"\"\nemail = \"\"\n").unwrap();
        let e = c.validate().unwrap_err();
        assert!(e.contains("site"), "{e}");
    }

    #[test]
    fn example_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("config.toml");
        write_example(&p).unwrap();
        let c = load(&p);
        // The example has placeholder site/email, so load fails validation
        // but parsing must succeed.
        let text = std::fs::read_to_string(&p).unwrap();
        let parsed: Config = toml::from_str(&text).unwrap();
        assert_eq!(parsed.team.members.len(), 6);
        assert!(c.is_err());
    }

    #[test]
    fn resolve_secret_plain_and_shell() {
        assert_eq!(resolve_secret("abc").unwrap(), "abc");
        assert_eq!(resolve_secret("$(echo hi)").unwrap(), "hi");
    }

    #[test]
    fn op_not_on_path_retries_through_login_shell() {
        let dir = tempfile::tempdir().unwrap();
        // A stand-in shell that prints the command it was asked to run.
        let sh = dir.path().join("fake-shell");
        std::fs::write(&sh, "#!/bin/sh\nprintf '%s %s' \"$1\" \"$2\"\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&sh, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let missing = dir.path().join("no-such-op");
        let got = op_read_via(missing.to_str().unwrap(), sh.to_str().unwrap(), "op://Private/Jira/credential").unwrap();
        assert_eq!(got, "-lc op read 'op://Private/Jira/credential'");
        assert!(op_read_via(missing.to_str().unwrap(), sh.to_str().unwrap(), "op://a/it's/x").is_err());
    }

    #[test]
    fn browse_url_joins_site() {
        let c: Config = toml::from_str(MINIMAL).unwrap();
        assert_eq!(c.browse_url("CI-1"), "https://example.atlassian.net/browse/CI-1");
    }
}
