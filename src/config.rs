//! The TOML config at ~/.config/jira-tickets/config.toml: site, scope,
//! grooming statuses, team roster, columns, roadmap field mapping.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub site: String,
    pub email: String,
    pub api_token: Option<String>,
    pub op_account: Option<String>,
    pub scope: Scope,
    pub grooming: Grooming,
    pub team: Team,
    pub issues: Issues,
    pub roadmap: Roadmap,
    pub standup: Standup,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Scope {
    pub projects: Vec<String>,
    pub project: String,
    pub team_label: String,
    pub board_id: u64,
    pub filter_id: u64,
    /// The parent board the sub-teams roll up to.
    pub parent_board_id: u64,
    /// Sub-teams whose tickets the Issues view loads; filtered by team in the UI.
    pub teams: Vec<TeamEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TeamEntry {
    pub label: String,
    pub name: String,
    pub board_id: u64,
    /// Jira display names or account ids whose tickets count as this team's
    /// even without the label. Empty on the own team means `[team].members`.
    #[serde(default)]
    pub members: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Grooming {
    pub status_new: String,
    pub status_planning: String,
    pub status_ready: String,
    pub status_backlog: String,
    /// The status `c` closes a ticket into.
    pub status_closed: String,
    /// The resolutions `c` offers, by Jira name, in this order.
    pub close_resolutions: Vec<String>,
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
    /// Status name (case-insensitive) to "#rrggbb".
    pub status_colors: BTreeMap<String, String>,
    /// Epic key to "#rrggbb".
    pub epic_colors: BTreeMap<String, String>,
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
    /// Epics with this label are roadmap initiatives.
    pub initiative_label: String,
}

/// The Standup screen's review: the `claude` CLI it runs and the model it asks for.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Standup {
    /// A path, or a name looked up on PATH and the usual install folders (a Dock launch has a minimal PATH).
    pub claude: String,
    pub model: String,
    /// How long the review may run before it is stopped.
    pub timeout_secs: u64,
}

impl Default for Standup {
    fn default() -> Self {
        Standup { claude: s("claude"), model: s("claude-opus-5-5"), timeout_secs: 900 }
    }
}

fn s(v: &str) -> String {
    v.to_string()
}

impl Default for Scope {
    fn default() -> Self {
        Scope {
            projects: vec![s("CI"), s("OPS")],
            project: s("CI"),
            team_label: s("PLAT"),
            board_id: 101,
            filter_id: 201,
            parent_board_id: 100,
            teams: vec![
                TeamEntry { label: s("PLAT"), name: s("Platform"), board_id: 101, members: Vec::new() },
                TeamEntry { label: s("TOOLS"), name: s("Developer Tools"), board_id: 102, members: Vec::new() },
            ],
        }
    }
}

impl Scope {
    /// Every configured team's label, or just `team_label` when no teams are listed.
    pub fn team_labels(&self) -> Vec<String> {
        if self.teams.is_empty() { vec![self.team_label.clone()] } else { self.teams.iter().map(|t| t.label.clone()).collect() }
    }

    /// A team's members; the own team falls back to `own_members` (`[team].members`) when it lists none.
    pub fn members_of(&self, label: &str, own_members: &[String]) -> Vec<String> {
        let listed = self.teams.iter().find(|t| t.label == label).map(|t| t.members.clone()).unwrap_or_default();
        if listed.is_empty() && label == self.team_label { own_members.to_vec() } else { listed }
    }

    /// Label → members for every team label, own-team fallback applied.
    pub fn team_members(&self, own_members: &[String]) -> BTreeMap<String, Vec<String>> {
        self.team_labels().into_iter().map(|l| (l.clone(), self.members_of(&l, own_members))).collect()
    }
}

impl Default for Grooming {
    fn default() -> Self {
        Grooming {
            status_new: s("New"),
            status_planning: s("Planning"),
            status_ready: s("Ready for Work"),
            status_backlog: s("Backlog"),
            status_closed: s("Closed"),
            close_resolutions: ["Done", "Won't Do"].map(s).to_vec(),
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
                QuickFilter { name: s("Only opened"), jql: s("status not in (Closed)") },
                QuickFilter { name: s("To do"), jql: s("statusCategory = \"To Do\"") },
            ],
            status_colors: BTreeMap::new(),
            epic_colors: BTreeMap::new(),
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
                (s("Work in Progress"), s("now")),
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
            initiative_label: s("roadmap"),
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
pub fn resolve_secret(value: &str, op_account: Option<&str>) -> Result<String, String> {
    let v = value.trim();
    if v.starts_with("op://") {
        return op_read(v, op_account);
    }
    if let Some(inner) = v.strip_prefix("$(").and_then(|r| r.strip_suffix(')')) {
        return run("sh", &["-c", inner]);
    }
    Ok(v.to_string())
}

/// A Finder or Dock launch inherits launchd's PATH, which rarely has `op`;
/// when it is not found, retry through the user's login shell.
fn op_read(reference: &str, account: Option<&str>) -> Result<String, String> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
    op_read_via("op", &shell, reference, account)
}

fn op_read_via(op: &str, shell: &str, reference: &str, account: Option<&str>) -> Result<String, String> {
    let mut cmd = std::process::Command::new(op);
    cmd.arg("read");
    if let Some(a) = account {
        cmd.args(["--account", a]);
    }
    match cmd.arg(reference).output() {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if reference.contains('\'') {
                return Err("op reference contains a single quote".into());
            }
            let acct = match account {
                Some(a) if a.contains('\'') => return Err("op_account contains a single quote".into()),
                Some(a) => format!(" --account '{a}'"),
                None => String::new(),
            };
            run(shell, &["-lc", &format!("op read{acct} '{reference}'")])
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

pub const EXAMPLE: &str = r##"# Jira Tickets configuration

site = "https://yourcompany.atlassian.net"
email = "you@example.com"
# Optional. Plain token, "op://vault/item/field", or "$(command)".
# Resolved on every launch, before the saved token file; never written to disk.
# api_token = "op://Private/Jira API Token/credential"
# Needed when `op` knows more than one account (the error says "multiple accounts
# found"). Use the account's sign-in address or its email.
# op_account = "yourcompany.1password.com"

[scope]
projects = ["CI", "OPS"]   # projects searched for team members' work
project = "CI"              # project the team label lives in
team_label = "PLAT"
board_id = 101
filter_id = 201
parent_board_id = 100          # parent board
# Sub-teams whose tickets the Issues view loads; filter by team in the UI.
# Tickets assigned to a team's members count as that team's even without the label.
[[scope.teams]]
label = "PLAT"
name = "Platform"
board_id = 101
# members = ["Ada Lovelace", "..."]  # Jira display names or account ids; PLAT defaults to [team].members
[[scope.teams]]
label = "TOOLS"
name = "Developer Tools"
board_id = 102
# members = ["5ed580000000000000000001", "5ed580000000000000000002", "5ed580000000000000000003", "5ed580000000000000000004", "5ed580000000000000000005"]

[grooming]
status_new = "New"
status_planning = "Planning"
status_ready = "Ready for Work"
status_backlog = "Backlog"
status_closed = "Closed"                   # where `c` closes a ticket in Session
close_resolutions = ["Done", "Won't Do"]   # the resolutions `c` offers, by Jira name
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
name = "Only opened"
jql = "status not in (Closed)"
[[issues.filters]]
name = "To do"
jql = 'statusCategory = "To Do"'
# Optional colour overrides, "#rrggbb". Status names are matched case-insensitively.
# [issues.status_colors]
# "Ready for Work" = "#0f9d9a"
# [issues.epic_colors]
# "CI-1860" = "#8250df"

[roadmap]
# epics = ["CI-1860"]            # optional allowlist
lanes = { PLAT = "Platform" }
initiative_label = "roadmap"     # epics with this label are roadmap initiatives
horizon_by_status = { "In Progress" = "now", "Work in Progress" = "now", "Ready for Work" = "next", "Planning" = "next", "New" = "later", "Backlog" = "later" }
# health_field = "customfield_XXXXX"
# target_field = "customfield_XXXXX"   # or "duedate"; default: the target:YYYY-Qn label
# start_field = "customfield_XXXXX"
# confidence_field = "customfield_XXXXX"
# next_milestone_field = "customfield_XXXXX"
# definition_of_done_field = "customfield_XXXXX"

[standup]
# The Claude Code CLI the Standup screen runs: a path, or a name found on PATH,
# ~/.local/bin, ~/.claude/local, ~/.npm-global/bin, the flox default environment,
# the nix profiles, /opt/homebrew/bin or /usr/local/bin.
claude = "claude"
model = "claude-opus-5-5"
timeout_secs = 900
"##;

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = r#"
site = "https://example.atlassian.net"
email = "me@example.com"
"#;

    #[test]
    fn partial_grooming_table_keeps_the_new_defaults() {
        let c: Config = toml::from_str("site = \"https://x.atlassian.net\"\nemail = \"a@b.c\"\n[grooming]\nstatus_backlog = \"Backlog\"\n").unwrap();
        assert_eq!(c.grooming.status_closed, "Closed");
        assert_eq!(c.grooming.close_resolutions, ["Done", "Won't Do"]);
    }

    #[test]
    fn minimal_config_gets_defaults() {
        let c: Config = toml::from_str(MINIMAL).unwrap();
        assert_eq!(c.scope.project, "CI");
        assert_eq!(c.scope.team_label, "PLAT");
        assert_eq!(c.scope.board_id, 101);
        assert_eq!(c.scope.parent_board_id, 100);
        assert_eq!(c.scope.team_labels(), ["PLAT", "TOOLS"]);
        assert_eq!(c.scope.teams[1], TeamEntry { label: "TOOLS".into(), name: "Developer Tools".into(), board_id: 102, members: vec![] });
        assert_eq!(c.scope.teams[0].name, "Platform");
        assert_eq!(c.grooming.planning_transition_id, "11");
        assert_eq!(c.grooming.planning_cap, 10);
        assert_eq!(c.issues.columns, vec!["status", "labels", "parent.summary", "assignee"]);
        assert_eq!(c.roadmap.lanes.get("PLAT").map(String::as_str), Some("Platform"));
        assert_eq!(c.roadmap.horizon_by_status.get("In Progress").map(String::as_str), Some("now"));
        assert_eq!(c.roadmap.horizon_by_status.get("Work in Progress").map(String::as_str), Some("now"));
        assert_eq!(c.roadmap.initiative_label, "roadmap");
        assert_eq!(c.standup.claude, "claude");
        assert_eq!(c.standup.model, "claude-opus-5-5");
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
        assert_eq!(parsed.scope.parent_board_id, 100);
        assert_eq!(parsed.scope.teams, Scope::default().teams);
        assert!(parsed.issues.status_colors.is_empty() && parsed.issues.epic_colors.is_empty());
        assert_eq!((parsed.standup.claude.as_str(), parsed.standup.model.as_str(), parsed.standup.timeout_secs), ("claude", "claude-opus-5-5", 900));
        assert!(c.is_err());
    }

    #[test]
    fn resolve_secret_plain_and_shell() {
        assert_eq!(resolve_secret("abc", None).unwrap(), "abc");
        assert_eq!(resolve_secret("$(echo hi)", None).unwrap(), "hi");
    }

    #[test]
    #[cfg(unix)]
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
        let got = op_read_via(missing.to_str().unwrap(), sh.to_str().unwrap(), "op://Private/Jira/credential", None).unwrap();
        assert_eq!(got, "-lc op read 'op://Private/Jira/credential'");
        assert!(op_read_via(missing.to_str().unwrap(), sh.to_str().unwrap(), "op://a/it's/x", None).is_err());
    }

    #[test]
    #[cfg(unix)]
    fn op_account_reaches_login_shell_command() {
        let dir = tempfile::tempdir().unwrap();
        let sh = dir.path().join("fake-shell");
        std::fs::write(&sh, "#!/bin/sh\nprintf '%s %s' \"$1\" \"$2\"\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&sh, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let missing = dir.path().join("no-such-op");
        let (op, shell) = (missing.to_str().unwrap(), sh.to_str().unwrap());
        let got = op_read_via(op, shell, "op://Private/Jira/credential", Some("acme.1password.com")).unwrap();
        assert_eq!(got, "-lc op read --account 'acme.1password.com' 'op://Private/Jira/credential'");
        assert!(op_read_via(op, shell, "op://a/b/c", Some("it's")).is_err());
    }

    #[test]
    fn colour_override_tables_parse() {
        let text = format!("{MINIMAL}\n[issues.status_colors]\n\"Ready for Work\" = \"#0f9d9a\"\n[issues.epic_colors]\n\"CI-1860\" = \"#8250df\"\n");
        let c: Config = toml::from_str(&text).unwrap();
        assert_eq!(c.issues.status_colors.get("Ready for Work").map(String::as_str), Some("#0f9d9a"));
        assert_eq!(c.issues.epic_colors.get("CI-1860").map(String::as_str), Some("#8250df"));
        assert_eq!(c.issues.columns.len(), 4);
    }

    #[test]
    fn team_labels_and_members() {
        let mut sc = Scope::default();
        let own = vec!["Ada".to_string()];
        assert_eq!(sc.members_of("PLAT", &own), own, "own team falls back to [team].members");
        assert!(sc.members_of("TOOLS", &own).is_empty());
        assert!(sc.members_of("XYZ", &own).is_empty());
        sc.teams[0].members = vec!["Ann".into()];
        sc.teams[1].members = vec!["acct-1".into()];
        assert_eq!(sc.members_of("PLAT", &own), ["Ann"]);
        let m = sc.team_members(&own);
        assert_eq!(m.get("TOOLS").unwrap(), &["acct-1"]);
        sc.teams.clear();
        assert_eq!(sc.team_labels(), ["PLAT"]);
        assert_eq!(sc.members_of("PLAT", &own), own);
    }

    #[test]
    fn team_members_parse_from_toml() {
        let text = format!("{MINIMAL}\n[scope]\nteam_label = \"TOOLS\"\n[[scope.teams]]\nlabel = \"TOOLS\"\nname = \"D\"\nboard_id = 1\nmembers = [\"x\"]\n");
        let c: Config = toml::from_str(&text).unwrap();
        assert_eq!(c.scope.teams.len(), 1);
        assert_eq!(c.scope.members_of("TOOLS", &c.team.members), ["x"]);
    }

    #[test]
    fn standup_table_parses_partially() {
        let c: Config = toml::from_str(&format!("{MINIMAL}\n[standup]\nclaude = \"/opt/claude\"\n")).unwrap();
        assert_eq!(c.standup.claude, "/opt/claude");
        assert_eq!(c.standup.model, "claude-opus-5-5", "unset keys keep their defaults");
        assert_eq!(c.standup.timeout_secs, 900);
    }

    #[test]
    fn browse_url_joins_site() {
        let c: Config = toml::from_str(MINIMAL).unwrap();
        assert_eq!(c.browse_url("CI-1"), "https://example.atlassian.net/browse/CI-1");
    }
}
