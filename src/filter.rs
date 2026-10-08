//! Pure list filtering, column formatting, and ⌘K parsing for the Issues view.

use crate::config::TeamEntry;
use crate::jira::{Issue, Sprint};
use std::collections::{BTreeMap, BTreeSet};

/// Which sub-team the Issues list is scoped to.
#[derive(Clone, Debug, PartialEq, Default)]
pub enum TeamScope {
    #[default]
    All,
    /// Issues with this label, or assigned to one of the team's members.
    Label(String),
    /// Issues that match no configured team by label or membership.
    None,
}

/// Which sprint the Issues list is scoped to, matched against each issue's latest non-closed sprint.
#[derive(Clone, Debug, PartialEq, Default)]
pub enum SprintScope {
    #[default]
    All,
    Backlog,
    Named(String),
}

#[derive(Default, Clone, Debug)]
pub struct IssueFilter {
    pub epic: Option<String>,
    pub sprint: SprintScope,
    pub quick: Option<usize>,
    pub mine: bool,
    pub text: String,
    pub team: TeamScope,
    /// Every configured team label, from config at startup.
    pub team_labels: Vec<String>,
    /// Team label → member display names or account ids, from config at startup.
    pub team_members: BTreeMap<String, Vec<String>>,
    /// Show only issues under a roadmap epic.
    pub roadmap: bool,
    /// Keys of the loaded epics carrying the roadmap initiative label (computed per frame).
    pub roadmap_epics: BTreeSet<String>,
}

/// Issues that are action items: epics are containers and never listed.
pub fn action_items<'a>(issues: &[&'a Issue]) -> Vec<&'a Issue> {
    issues.iter().copied().filter(|i| !i.issue_type.eq_ignore_ascii_case("epic")).collect()
}

/// Keys of the epics that carry `label` (exact, like team labels).
pub fn roadmap_epic_keys(epics: &[&Issue], label: &str) -> BTreeSet<String> {
    epics.iter().filter(|e| e.labels.iter().any(|l| l == label)).map(|e| e.key.clone()).collect()
}

/// The issue's assignee is one of `members`, by display name or account id, case-insensitively.
fn assigned_to_member(issue: &Issue, members: &[String]) -> bool {
    let Some(a) = &issue.assignee else { return false };
    let (name, id) = (a.display_name.to_lowercase(), a.account_id.to_lowercase());
    members.iter().map(|m| m.to_lowercase()).any(|m| m == name || m == id)
}

/// The issue belongs to the team with `label`: it carries the label (exact) or a member has it.
fn on_team(issue: &Issue, label: &str, members: &[String]) -> bool {
    issue.labels.iter().any(|l| l == label) || assigned_to_member(issue, members)
}

/// The issue counts as this team's by membership but lacks its label.
pub fn missing_team_label(issue: &Issue, label: &str, members: &[String]) -> bool {
    !issue.labels.iter().any(|l| l == label) && assigned_to_member(issue, members)
}

/// `searched`: the rows are a search's. A No team label search was already
/// scoped by the server (`store::jql_no_team`), so it keeps every row.
fn in_team_scope(issue: &Issue, f: &IssueFilter, searched: bool) -> bool {
    let members = |l: &str| f.team_members.get(l).map(Vec::as_slice).unwrap_or_default();
    match &f.team {
        TeamScope::All => true,
        TeamScope::Label(l) => on_team(issue, l, members(l)),
        TeamScope::None => searched || !f.team_labels.iter().any(|l| on_team(issue, l, members(l))),
    }
}

/// Only the team scope applied, ignoring every other filter (feeds the epic picker).
pub fn in_team<'a>(issues: &[&'a Issue], f: &IssueFilter, searched: bool) -> Vec<&'a Issue> {
    issues.iter().copied().filter(|i| in_team_scope(i, f, searched)).collect()
}

/// Picker entries: "All teams", each configured team, then "No team label".
pub fn team_options(teams: &[TeamEntry]) -> Vec<(TeamScope, String)> {
    std::iter::once((TeamScope::All, "All teams".to_string()))
        .chain(teams.iter().map(|t| (TeamScope::Label(t.label.clone()), format!("{} · {}", t.label, t.name))))
        .chain(std::iter::once((TeamScope::None, "No team label".to_string())))
        .collect()
}

/// Search-box text with a JQL operator runs as a Jira search on Enter; a bare word filters locally.
pub fn looks_like_jql(text: &str) -> bool {
    let t = text.trim();
    t.contains('=') || t.contains(" in ") || t.contains('~') || t.contains('!')
}

/// `searched`: the rows are a search's (see `in_team_scope`).
/// JQL left in the search box (it stays visible while its search runs) is not a local text filter.
pub fn visible<'a>(issues: &[&'a Issue], f: &IssueFilter, searched: bool, me: Option<&str>) -> Vec<&'a Issue> {
    let needle = if looks_like_jql(&f.text) { String::new() } else { f.text.trim().to_lowercase() };
    issues
        .iter()
        .copied()
        .filter(|i| in_team_scope(i, f, searched))
        .filter(|i| f.epic.as_deref().is_none_or(|e| i.parent_key.as_deref() == Some(e)))
        .filter(|i| match &f.sprint {
            SprintScope::All => true,
            SprintScope::Backlog => i.sprint.is_none(),
            SprintScope::Named(n) => i.sprint.as_deref() == Some(n.as_str()),
        })
        .filter(|i| !f.roadmap || i.parent_key.as_ref().is_some_and(|k| f.roadmap_epics.contains(k)))
        .filter(|i| !f.mine || i.assignee.as_ref().map(|a| a.display_name.as_str()) == me)
        .filter(|i| {
            needle.is_empty()
                || i.key.to_lowercase().contains(&needle)
                || i.summary.to_lowercase().contains(&needle)
                || i.labels.iter().any(|l| l.to_lowercase().contains(&needle))
                || i.assignee.as_ref().is_some_and(|a| a.display_name.to_lowercase().contains(&needle))
        })
        .collect()
}

/// Picker entries: "All sprints", each board sprint in Jira's order labelled with its state, then "Backlog".
pub fn sprint_options(sprints: &[Sprint]) -> Vec<(SprintScope, String)> {
    std::iter::once((SprintScope::All, "All sprints".to_string()))
        .chain(sprints.iter().map(|s| (SprintScope::Named(s.name.clone()), format!("{} ({})", s.name, s.state))))
        .chain(std::iter::once((SprintScope::Backlog, "Backlog".to_string())))
        .collect()
}

/// The first active sprint, or `All` when the board has none.
pub fn default_sprint_scope(sprints: &[Sprint]) -> SprintScope {
    sprints.iter().find(|s| s.state == "active").map_or(SprintScope::All, |s| SprintScope::Named(s.name.clone()))
}

/// The scope to keep after the sprint list refreshes: a named sprint that is no longer
/// listed (it closed) falls back to the default; anything else is kept.
pub fn sprint_scope_after_refresh(current: &SprintScope, sprints: &[Sprint]) -> SprintScope {
    match current {
        SprintScope::Named(n) if !sprints.iter().any(|s| &s.name == n) => default_sprint_scope(sprints),
        other => other.clone(),
    }
}

pub fn column_value(issue: &Issue, field: &str) -> String {
    match field {
        "status" => issue.status.name.clone(),
        "priority" => issue.priority.clone(),
        "labels" => issue.labels.join(", "),
        "assignee" => issue.assignee.as_ref().map(|a| a.display_name.clone()).unwrap_or_default(),
        "issuetype" => issue.issue_type.clone(),
        "parent.summary" => issue.parent_summary.clone().unwrap_or_default(),
        "parent.key" => issue.parent_key.clone().unwrap_or_default(),
        "sprint" => issue.sprint.clone().unwrap_or_default(),
        "points" => issue.story_points.map(fmt_num).unwrap_or_default(),
        f if f.starts_with("customfield_") => issue.custom.get(f).map(json_text).unwrap_or_default(),
        _ => String::new(),
    }
}

fn fmt_num(n: f64) -> String {
    if n.fract() == 0.0 { format!("{}", n as i64) } else { format!("{n}") }
}

fn json_text(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.as_f64().map(fmt_num).unwrap_or_default(),
        serde_json::Value::Object(o) => o.get("value").or(o.get("name")).or(o.get("displayName")).map(json_text).unwrap_or_default(),
        serde_json::Value::Array(a) => a.iter().map(json_text).collect::<Vec<_>>().join(", "),
        _ => String::new(),
    }
}

pub fn parse_goto(text: &str, project: &str) -> Option<String> {
    let t = text.trim();
    if t.is_empty() {
        return None;
    }
    if let Some(rest) = t.split("/browse/").nth(1) {
        let key: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').collect();
        return issue_key(&key);
    }
    if t.chars().all(|c| c.is_ascii_digit()) {
        return Some(format!("{project}-{t}"));
    }
    issue_key(t)
}

/// `PROJ-123`, uppercased: the project is `[A-Za-z][A-Za-z0-9_]*`, the number digits.
fn issue_key(t: &str) -> Option<String> {
    let (p, n) = t.split_once('-')?;
    let project_ok = p.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) && p.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    let number_ok = !n.is_empty() && n.chars().all(|c| c.is_ascii_digit());
    (project_ok && number_ok).then(|| format!("{}-{n}", p.to_uppercase()))
}

#[cfg(test)]
mod tests {
    #[test]
    fn jql_is_told_from_a_bare_word() {
        for j in ["status = Blocked", "labels in (PLAT)", "summary ~ alpha", "status != Done"] {
            assert!(super::looks_like_jql(j), "{j}");
        }
        for w in ["alpha", "  ", "in progress", "flox"] {
            assert!(!super::looks_like_jql(w), "{w}");
        }
    }

    use super::*;
    use crate::jira::{Issue, Sprint, Status, User};

    fn sprint(id: u64, name: &str, state: &str) -> Sprint {
        Sprint { id, name: name.into(), state: state.into() }
    }

    fn issue(key: &str, summary: &str, parent: Option<&str>, assignee: Option<&str>, labels: &[&str]) -> Issue {
        Issue {
            key: key.into(),
            summary: summary.into(),
            parent_key: parent.map(String::from),
            assignee: assignee.map(|a| User { account_id: a.into(), display_name: a.into() }),
            labels: labels.iter().map(|l| l.to_string()).collect(),
            status: Status { name: "New".into(), category: "new".into() },
            priority: "High".into(),
            ..Default::default()
        }
    }

    #[test]
    fn filters_compose() {
        let a = issue("CI-1", "Remove the old autoscaler", Some("E-1"), Some("Ada"), &["PLAT"]);
        let b = issue("CI-2", "Deployer overlays", Some("E-2"), None, &["PLAT", "infra"]);
        let all = vec![&a, &b];
        let mut f = IssueFilter::default();
        assert_eq!(visible(&all, &f, false, Some("Ada")).len(), 2);
        f.epic = Some("E-1".into());
        assert_eq!(visible(&all, &f, false, None)[0].key, "CI-1");
        f.epic = None;
        f.mine = true;
        assert_eq!(visible(&all, &f, false, Some("Ada")).len(), 1);
        f.mine = false;
        f.text = "INFRA".into();
        assert_eq!(visible(&all, &f, false, None)[0].key, "CI-2");
    }

    #[test]
    fn epics_are_not_action_items() {
        let mut e = issue("CI-10", "An epic", None, None, &[]);
        e.issue_type = "epic".into();
        let mut s = issue("CI-11", "A story", Some("CI-10"), None, &[]);
        s.issue_type = "Story".into();
        let keys: Vec<_> = action_items(&[&e, &s]).iter().map(|i| i.key.clone()).collect();
        assert_eq!(keys, ["CI-11"]);
    }

    #[test]
    fn roadmap_epics_by_label_and_filter() {
        let e1 = issue("E-1", "on roadmap", None, None, &["roadmap", "PLAT"]);
        let e2 = issue("E-2", "not", None, None, &["PLAT", "Roadmap"]);
        let keys = roadmap_epic_keys(&[&e1, &e2], "roadmap");
        assert_eq!(keys.iter().map(String::as_str).collect::<Vec<_>>(), ["E-1"], "label is exact");
        let a = issue("CI-1", "a", Some("E-1"), None, &[]);
        let b = issue("CI-2", "b", Some("E-2"), None, &[]);
        let c = issue("CI-3", "c", None, None, &[]);
        let all = vec![&a, &b, &c];
        let mut f = IssueFilter { roadmap_epics: keys, ..Default::default() };
        assert_eq!(visible(&all, &f, false, None).len(), 3);
        f.roadmap = true;
        assert_eq!(visible(&all, &f, false, None).iter().map(|i| i.key.as_str()).collect::<Vec<_>>(), ["CI-1"]);
    }

    fn team_filter() -> IssueFilter {
        IssueFilter {
            team_labels: vec!["PLAT".into(), "TOOLS".into()],
            team_members: BTreeMap::from([("PLAT".into(), vec!["nate".into()]), ("TOOLS".into(), vec!["acct-9".into()])]),
            ..Default::default()
        }
    }

    #[test]
    fn team_scope_filters_by_label_and_membership() {
        let a = issue("CI-1", "a", None, None, &["PLAT"]);
        let b = issue("CI-2", "b", None, None, &["TOOLS", "infra"]);
        let c = issue("CI-3", "c", None, None, &["plat"]);
        let d = issue("CI-4", "d", None, Some("Ada"), &[]);
        let mut e = issue("CI-5", "e", None, Some("Zed"), &[]);
        e.assignee.as_mut().unwrap().account_id = "ACCT-9".into();
        let all = vec![&a, &b, &c, &d, &e];
        let mut f = team_filter();
        let keys = |f: &IssueFilter| visible(&all, f, false, None).iter().map(|i| i.key.clone()).collect::<Vec<_>>();
        assert_eq!(f.team, TeamScope::All);
        assert_eq!(keys(&f), ["CI-1", "CI-2", "CI-3", "CI-4", "CI-5"]);
        f.team = TeamScope::Label("PLAT".into());
        assert_eq!(keys(&f), ["CI-1", "CI-4"], "label is exact and case-sensitive; display name matches case-insensitively");
        f.team = TeamScope::Label("TOOLS".into());
        assert_eq!(keys(&f), ["CI-2", "CI-5"], "account id matches case-insensitively");
        f.team = TeamScope::None;
        assert_eq!(keys(&f), ["CI-3"]);
        let searched = |f: &IssueFilter| visible(&all, f, true, None).iter().map(|i| i.key.clone()).collect::<Vec<_>>();
        assert_eq!(searched(&f), ["CI-1", "CI-2", "CI-3", "CI-4", "CI-5"], "a No team label search was scoped by the server");
        assert_eq!(in_team(&all, &f, true).len(), 5);
        assert_eq!(in_team(&all, &f, false).len(), 1);
        f.team = TeamScope::Label("PLAT".into());
        assert_eq!(searched(&f), ["CI-1", "CI-4"], "other scopes still filter a search locally");
    }

    #[test]
    fn missing_label_only_for_members_without_it() {
        let members = vec!["Ada".to_string()];
        assert!(missing_team_label(&issue("CI-1", "a", None, Some("nate"), &["infra"]), "PLAT", &members));
        assert!(!missing_team_label(&issue("CI-2", "a", None, Some("Ada"), &["PLAT"]), "PLAT", &members));
        assert!(!missing_team_label(&issue("CI-3", "a", None, Some("Zed"), &[]), "PLAT", &members));
        assert!(!missing_team_label(&issue("CI-4", "a", None, None, &[]), "PLAT", &members));
    }

    #[test]
    fn team_options_order_and_labels() {
        let t = |l: &str, n: &str| TeamEntry { label: l.into(), name: n.into(), board_id: 1, members: vec![] };
        assert_eq!(
            team_options(&[t("PLAT", "Platform"), t("TOOLS", "Developer Tools")]),
            vec![
                (TeamScope::All, "All teams".to_string()),
                (TeamScope::Label("PLAT".into()), "PLAT · Platform".to_string()),
                (TeamScope::Label("TOOLS".into()), "TOOLS · Developer Tools".to_string()),
                (TeamScope::None, "No team label".to_string()),
            ]
        );
    }

    #[test]
    fn column_values() {
        let mut i = issue("CI-1", "x", Some("E-1"), Some("Ada"), &["a", "b"]);
        i.parent_summary = Some("Epic one".into());
        i.custom.insert("customfield_1".into(), serde_json::json!({ "value": "Opt" }));
        i.custom.insert("customfield_2".into(), serde_json::json!(3.0));
        assert_eq!(column_value(&i, "labels"), "a, b");
        assert_eq!(column_value(&i, "assignee"), "Ada");
        assert_eq!(column_value(&i, "parent.summary"), "Epic one");
        assert_eq!(column_value(&i, "customfield_1"), "Opt");
        assert_eq!(column_value(&i, "customfield_2"), "3");
        assert_eq!(column_value(&i, "nope"), "");
    }

    #[test]
    fn goto_parsing() {
        assert_eq!(parse_goto("9001", "CI").as_deref(), Some("CI-9001"));
        assert_eq!(parse_goto("ci-9001", "CI").as_deref(), Some("CI-9001"));
        assert_eq!(parse_goto("https://example.atlassian.net/browse/CI-9001?x=1", "CI").as_deref(), Some("CI-9001"));
        assert_eq!(parse_goto("", "CI"), None);
        assert_eq!(parse_goto("my_proj2-7", "CI").as_deref(), Some("MY_PROJ2-7"));
        assert_eq!(parse_goto("https://x.atlassian.net/browse/9-1", "CI"), None);
        for bad in ["-12", "2CI-1", "C I-1", "CI$-1", "CI-", "CI-1a", "ci-1-2", "é-1"] {
            assert_eq!(parse_goto(bad, "CI"), None, "{bad}");
        }
    }

    #[test]
    fn sprint_scope_filters() {
        let mut a = issue("CI-1", "a", None, None, &[]);
        a.sprint = Some("Sprint 92".into());
        let mut b = issue("CI-2", "b", None, None, &[]);
        b.sprint = Some("Sprint 93".into());
        let c = issue("CI-3", "c", None, None, &[]);
        let all = vec![&a, &b, &c];
        let keys = |f: &IssueFilter| visible(&all, f, false, None).iter().map(|i| i.key.clone()).collect::<Vec<_>>();
        let mut f = IssueFilter::default();
        assert_eq!(f.sprint, SprintScope::All);
        assert_eq!(keys(&f), ["CI-1", "CI-2", "CI-3"]);
        f.sprint = SprintScope::Backlog;
        assert_eq!(keys(&f), ["CI-3"]);
        f.sprint = SprintScope::Named("Sprint 93".into());
        assert_eq!(keys(&f), ["CI-2"]);
    }

    #[test]
    fn sprint_options_order_and_labels() {
        let opts = sprint_options(&[sprint(1, "Sprint 92", "active"), sprint(2, "Sprint 93", "future")]);
        assert_eq!(
            opts,
            vec![
                (SprintScope::All, "All sprints".to_string()),
                (SprintScope::Named("Sprint 92".into()), "Sprint 92 (active)".to_string()),
                (SprintScope::Named("Sprint 93".into()), "Sprint 93 (future)".to_string()),
                (SprintScope::Backlog, "Backlog".to_string()),
            ]
        );
        assert_eq!(sprint_options(&[]), vec![(SprintScope::All, "All sprints".to_string()), (SprintScope::Backlog, "Backlog".to_string())]);
    }

    #[test]
    fn default_scope_prefers_active_sprint() {
        assert_eq!(default_sprint_scope(&[sprint(2, "Sprint 93", "future"), sprint(1, "Sprint 92", "active")]), SprintScope::Named("Sprint 92".into()));
        assert_eq!(default_sprint_scope(&[sprint(2, "Sprint 93", "future")]), SprintScope::All);
        assert_eq!(default_sprint_scope(&[]), SprintScope::All);
    }

    #[test]
    fn scope_after_refresh_falls_back_only_when_sprint_is_gone() {
        let now = [sprint(2, "Sprint 93", "active"), sprint(3, "Sprint 94", "future")];
        let named = |n: &str| SprintScope::Named(n.into());
        assert_eq!(sprint_scope_after_refresh(&named("Sprint 94"), &now), named("Sprint 94"));
        assert_eq!(sprint_scope_after_refresh(&named("Sprint 92"), &now), named("Sprint 93"));
        assert_eq!(sprint_scope_after_refresh(&named("Sprint 92"), &[sprint(3, "Sprint 94", "future")]), SprintScope::All);
        assert_eq!(sprint_scope_after_refresh(&SprintScope::All, &now), SprintScope::All);
        assert_eq!(sprint_scope_after_refresh(&SprintScope::Backlog, &now), SprintScope::Backlog);
    }
}
