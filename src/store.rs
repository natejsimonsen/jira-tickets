//! One cached store of issues. Views are projections over it.

use crate::config::Config;
use crate::filter::{SprintScope, TeamScope};
use crate::jira::Issue;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Hash, Eq, PartialEq, Clone, Copy, Debug, Serialize, Deserialize)]
pub enum ListId {
    TeamOpen,
    Epics,
    TeamInProgress,
    Search,
    /// Every child of the roadmap's initiative epics, whatever its status, for their progress.
    EpicChildren,
    /// The signed-in user's own tickets, for the Me screen (`me::JQL`).
    Me,
}

#[derive(Default, Clone, Serialize, Deserialize)]
pub struct Store {
    pub issues: HashMap<String, Issue>,
    pub lists: HashMap<ListId, Vec<String>>,
    /// Unix seconds of the last successful full refresh.
    pub fetched_at: Option<i64>,
}

const CACHE_KEY: &str = "store";

impl Store {
    /// Replace a list. List rows carry no description or comments, so keep
    /// those from the record already in the store.
    pub fn absorb(&mut self, id: ListId, issues: Vec<Issue>) {
        let mut keys = Vec::with_capacity(issues.len());
        for mut i in issues {
            if let Some(old) = self.issues.get(&i.key) {
                if i.description.is_none() {
                    i.description = old.description.clone();
                }
                if i.comments.is_empty() {
                    i.comments = old.comments.clone();
                }
            }
            keys.push(i.key.clone());
            self.issues.insert(i.key.clone(), i);
        }
        self.lists.insert(id, keys);
    }

    pub fn merge_detail(&mut self, issue: Issue) {
        self.issues.insert(issue.key.clone(), issue);
    }

    pub fn get(&self, key: &str) -> Option<&Issue> {
        self.issues.get(key)
    }

    pub fn list(&self, id: ListId) -> Vec<&Issue> {
        self.lists.get(&id).map(|keys| keys.iter().filter_map(|k| self.issues.get(k)).collect()).unwrap_or_default()
    }

    /// An epic's children: EpicChildren (all statuses) then any TeamOpen ones it lacks, so open children show
    /// before EpicChildren lands; each key once.
    pub fn children_of(&self, epic_key: &str) -> Vec<&Issue> {
        let mut seen = std::collections::HashSet::new();
        self.list(ListId::EpicChildren)
            .into_iter()
            .chain(self.list(ListId::TeamOpen))
            .filter(|i| i.parent_key.as_deref() == Some(epic_key) && seen.insert(i.key.as_str()))
            .collect()
    }

    pub fn load_cached() -> Option<Store> {
        crate::cache::load(CACHE_KEY)
    }

    /// The store a launch starts from: the cache without its Search list. That list answered some earlier
    /// run's search, so it must never be the base the list shows (or keeps on screen while a filter loads).
    pub fn for_launch(cached: Option<Store>) -> Store {
        let mut s = cached.unwrap_or_default();
        s.lists.remove(&ListId::Search);
        s
    }

    pub fn save_cached(&self) {
        crate::cache::store(CACHE_KEY, self);
    }
}

pub fn jql_quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// `labels in (...)`, OR'd with `assignee in (...)` in parentheses when there are members.
fn team_clause(labels: &[String], members: &[String]) -> String {
    let quoted: Vec<String> = labels.iter().map(|l| jql_quote(l)).collect();
    let labels = format!("labels in ({})", quoted.join(", "));
    if members.is_empty() {
        return labels;
    }
    let members: Vec<String> = members.iter().map(|m| jql_quote(m)).collect();
    format!("({labels} OR assignee in ({}))", members.join(", "))
}

/// Every team's members, de-duplicated, in team order (own team falls back to
/// `[team].members`); none when `include` is false (a member did not resolve).
fn all_members(cfg: &Config, include: bool) -> Vec<String> {
    let mut members: Vec<String> = Vec::new();
    if !include {
        return members;
    }
    for l in cfg.scope.team_labels() {
        for m in cfg.scope.members_of(&l, &cfg.team.members) {
            if !members.contains(&m) {
                members.push(m);
            }
        }
    }
    members
}

/// Every configured team's open work: tickets carrying a team label, or
/// assigned to any team's members. `include_members: false` drops the assignee
/// group (labels only), for when Jira rejects a configured member.
pub fn jql_team_open(cfg: &Config, include_members: bool) -> String {
    let team = team_clause(&cfg.scope.team_labels(), &all_members(cfg, include_members));
    format!("project = {} AND {team} AND statusCategory != Done ORDER BY Rank ASC", jql_quote(&cfg.scope.project))
}

/// Tickets no team claims: without any team label, and not assigned to any
/// team's member (the assignee group is left out when no members are configured
/// or `include_members` is false).
pub fn jql_no_team(cfg: &Config, include_members: bool) -> String {
    let labels: Vec<String> = cfg.scope.team_labels().iter().map(|l| jql_quote(l)).collect();
    let labels = format!("(labels is EMPTY OR labels not in ({}))", labels.join(", "));
    let members: Vec<String> = all_members(cfg, include_members).iter().map(|m| jql_quote(m)).collect();
    if members.is_empty() {
        return labels;
    }
    format!("{labels} AND (assignee is EMPTY OR assignee not in ({}))", members.join(", "))
}

/// The team clause a quick filter is ANDed with: the picked team's label or
/// members, every team's under All teams, and no team's under No team label.
/// `include_members: false` leaves out every member (labels only).
pub fn quick_scope_jql(scope: &TeamScope, cfg: &Config, include_members: bool) -> String {
    match scope {
        TeamScope::Label(l) => {
            let label = format!("labels = {}", jql_quote(l));
            let members: Vec<String> =
                if include_members { cfg.scope.members_of(l, &cfg.team.members).iter().map(|m| jql_quote(m)).collect() } else { Vec::new() };
            if members.is_empty() { label } else { format!("({label} OR assignee in ({}))", members.join(", ")) }
        }
        TeamScope::All => team_clause(&cfg.scope.team_labels(), &all_members(cfg, include_members)),
        TeamScope::None => jql_no_team(cfg, include_members),
    }
}

pub fn jql_epics(cfg: &Config, parent_keys: &[String]) -> String {
    let labels: Vec<String> = cfg.roadmap.lanes.keys().map(|l| jql_quote(l)).collect();
    let mut clauses = vec![format!("labels in ({})", labels.join(", ")), format!("labels = {}", jql_quote(&cfg.roadmap.initiative_label))];
    if !parent_keys.is_empty() {
        clauses.push(format!("key in ({})", parent_keys.join(", ")));
    }
    if !cfg.roadmap.epics.is_empty() {
        clauses.push(format!("key in ({})", cfg.roadmap.epics.join(", ")));
    }
    format!("project = {} AND issuetype = Epic AND ({}) ORDER BY Rank ASC", jql_quote(&cfg.scope.project), clauses.join(" OR "))
}

/// Every child of `epic_keys` (the initiative epics) across the scope's projects, Done ones included.
pub fn jql_epic_children(cfg: &Config, epic_keys: &[String]) -> String {
    let projects: Vec<String> = cfg.scope.projects.iter().map(|p| jql_quote(p)).collect();
    format!("project in ({}) AND parent in ({}) ORDER BY Rank ASC", projects.join(", "), epic_keys.join(", "))
}

/// The members' in-progress work across the scope's projects. The list is by
/// assignee only, with no label alternative, so it is None (not loaded) when
/// `include_members` is false or no members are configured.
pub fn jql_team_in_progress(cfg: &Config, include_members: bool) -> Option<String> {
    if !include_members || cfg.team.members.is_empty() {
        return None;
    }
    let projects: Vec<String> = cfg.scope.projects.iter().map(|p| jql_quote(p)).collect();
    let members: Vec<String> = cfg.team.members.iter().map(|m| jql_quote(m)).collect();
    Some(format!("project in ({}) AND assignee in ({}) AND statusCategory = \"In Progress\" ORDER BY updated DESC", projects.join(", "), members.join(", ")))
}

/// The sprint scope as JQL, the same rule `filter::visible` applies to each issue's parsed sprint. Backlog
/// is what Jira's board backlog holds: never in a sprint, or in none that is still active or future
/// (`sprint not in` alone drops issues with no sprint, hence the `is EMPTY`). None under All sprints.
pub fn sprint_jql(scope: &SprintScope) -> Option<String> {
    match scope {
        SprintScope::All => None,
        SprintScope::Backlog => Some("(sprint is EMPTY OR sprint not in (openSprints(), futureSprints()))".to_string()),
        SprintScope::Named(n) => Some(format!("sprint = {}", jql_quote(n))),
    }
}

/// Jira's board text box: `text ~ "<term>*"` over summary, description, comments and environment.
/// The term goes as typed, quoted (`"` and `\` escaped), with a trailing `*` for prefixes.
pub fn text_clause(term: &str) -> String {
    let t = term.trim();
    let t = if t.ends_with('*') { t.to_string() } else { format!("{t}*") };
    format!("text ~ {}", jql_quote(&t))
}

/// The search-box text search: the scope the list shows (`scope`, the team or quick-filter query), narrowed
/// by the sprint scope and epic, AND the text clause, so its rows are a subset of what the user is looking at.
pub fn scope_text_jql(scope: &str, sprint: &SprintScope, epic: Option<&str>, term: &str) -> String {
    let mut clauses = vec![format!("({scope})")];
    clauses.extend(sprint_jql(sprint));
    clauses.extend(epic.map(|e| format!("parent = {}", jql_quote(e))));
    clauses.push(text_clause(term));
    clauses.join(" AND ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jira::{Issue, Status, User};

    fn issue(key: &str, status: &str, parent: Option<&str>, assignee: Option<&str>) -> Issue {
        Issue {
            key: key.into(),
            status: Status { name: status.into(), category: "new".into() },
            parent_key: parent.map(String::from),
            assignee: assignee.map(|a| User { account_id: a.to_lowercase(), display_name: a.into() }),
            ..Default::default()
        }
    }

    fn cfg() -> Config {
        toml::from_str("site = \"https://x.atlassian.net\"\nemail = \"a@b.c\"\n[team]\nmembers = [\"Ada Lovelace\", \"Shawn Sorichetti\"]\n").unwrap()
    }

    #[test]
    fn jql_builders() {
        let c = cfg();
        assert_eq!(
            jql_team_open(&c, true),
            r#"project = "CI" AND (labels in ("PLAT", "TOOLS") OR assignee in ("Ada Lovelace", "Shawn Sorichetti")) AND statusCategory != Done ORDER BY Rank ASC"#
        );
        assert_eq!(
            jql_epics(&c, &["CI-1".into(), "CI-2".into()]),
            r#"project = "CI" AND issuetype = Epic AND (labels in ("PLAT") OR labels = "roadmap" OR key in (CI-1, CI-2)) ORDER BY Rank ASC"#
        );
        assert_eq!(jql_epics(&c, &[]), r#"project = "CI" AND issuetype = Epic AND (labels in ("PLAT") OR labels = "roadmap") ORDER BY Rank ASC"#);
        assert_eq!(
            jql_team_in_progress(&c, true).as_deref(),
            Some(r#"project in ("CI", "OPS") AND assignee in ("Ada Lovelace", "Shawn Sorichetti") AND statusCategory = "In Progress" ORDER BY updated DESC"#)
        );
        assert_eq!(jql_quote(r#"a"b"#), r#""a\"b""#);
    }

    #[test]
    fn sprint_scope_jql() {
        assert_eq!(sprint_jql(&SprintScope::All), None);
        assert_eq!(sprint_jql(&SprintScope::Backlog).as_deref(), Some("(sprint is EMPTY OR sprint not in (openSprints(), futureSprints()))"));
        assert_eq!(sprint_jql(&SprintScope::Named("Sprint 91".into())).as_deref(), Some(r#"sprint = "Sprint 91""#));
    }

    #[test]
    fn text_search_is_the_scope_and_the_term() {
        let scope = r#"labels = "PLAT" AND statusCategory != Done"#;
        assert_eq!(
            scope_text_jql(scope, &SprintScope::Backlog, None, "f1"),
            r#"(labels = "PLAT" AND statusCategory != Done) AND (sprint is EMPTY OR sprint not in (openSprints(), futureSprints())) AND text ~ "f1*""#
        );
        assert_eq!(
            scope_text_jql(scope, &SprintScope::Named("Sprint 91".into()), Some("CI-79"), "alpha"),
            r#"(labels = "PLAT" AND statusCategory != Done) AND sprint = "Sprint 91" AND parent = "CI-79" AND text ~ "alpha*""#
        );
        assert_eq!(scope_text_jql("x", &SprintScope::All, None, " say \"hi\" "), r#"(x) AND text ~ "say \"hi\"*""#);
        assert_eq!(text_clause("a\\b"), r#"text ~ "a\\b*""#);
        assert_eq!(text_clause("kube*"), r#"text ~ "kube*""#, "no doubled wildcard");
    }

    #[test]
    fn jql_team_in_progress_needs_members() {
        let mut c = cfg();
        assert_eq!(jql_team_in_progress(&c, false), None, "members disabled: there is no label alternative");
        c.team.members.clear();
        assert_eq!(jql_team_in_progress(&c, true), None, "no members would be `assignee in ()`");
    }

    #[test]
    fn jql_team_open_members_union_and_single_team() {
        let mut c = cfg();
        c.scope.teams[1].members = vec!["acct-1".into(), "Ada Lovelace".into()];
        assert_eq!(
            jql_team_open(&c, true),
            r#"project = "CI" AND (labels in ("PLAT", "TOOLS") OR assignee in ("Ada Lovelace", "Shawn Sorichetti", "acct-1")) AND statusCategory != Done ORDER BY Rank ASC"#
        );
        c.scope.teams.truncate(1);
        c.team.members.clear();
        assert_eq!(jql_team_open(&c, true), r#"project = "CI" AND labels in ("PLAT") AND statusCategory != Done ORDER BY Rank ASC"#);
        c.scope.teams.clear();
        c.team.members = vec!["Ann".into()];
        assert_eq!(jql_team_open(&c, true), r#"project = "CI" AND (labels in ("PLAT") OR assignee in ("Ann")) AND statusCategory != Done ORDER BY Rank ASC"#);
    }

    #[test]
    fn quick_filters_follow_the_team_scope() {
        let mut c = cfg();
        c.scope.teams[1].members = vec!["acct-1".into()];
        assert_eq!(quick_scope_jql(&TeamScope::Label("PLAT".into()), &c, true), r#"(labels = "PLAT" OR assignee in ("Ada Lovelace", "Shawn Sorichetti"))"#);
        assert_eq!(quick_scope_jql(&TeamScope::Label("TOOLS".into()), &c, true), r#"(labels = "TOOLS" OR assignee in ("acct-1"))"#);
        let all = r#"(labels in ("PLAT", "TOOLS") OR assignee in ("Ada Lovelace", "Shawn Sorichetti", "acct-1"))"#;
        assert_eq!(quick_scope_jql(&TeamScope::All, &c, true), all);
        assert_eq!(quick_scope_jql(&TeamScope::None, &c, true), jql_no_team(&c, true));
        c.scope.teams[1].members.clear();
        assert_eq!(quick_scope_jql(&TeamScope::Label("TOOLS".into()), &c, true), r#"labels = "TOOLS""#, "no members, no assignee part");
        c.team.members.clear();
        assert_eq!(quick_scope_jql(&TeamScope::All, &c, true), r#"labels in ("PLAT", "TOOLS")"#);
    }

    #[test]
    fn jql_no_team_excludes_labels_and_members() {
        let mut c = cfg();
        c.scope.teams[1].members = vec!["acct-1".into()];
        assert_eq!(
            jql_no_team(&c, true),
            r#"(labels is EMPTY OR labels not in ("PLAT", "TOOLS")) AND (assignee is EMPTY OR assignee not in ("Ada Lovelace", "Shawn Sorichetti", "acct-1"))"#
        );
        c.scope.teams[1].members.clear();
        c.team.members.clear();
        assert_eq!(jql_no_team(&c, true), r#"(labels is EMPTY OR labels not in ("PLAT", "TOOLS"))"#, "no members, no assignee group");
    }

    #[test]
    fn labels_only_queries_drop_the_assignee_group() {
        let mut c = cfg();
        c.scope.teams[1].members = vec!["acct-1".into()];
        assert_eq!(jql_team_open(&c, false), r#"project = "CI" AND labels in ("PLAT", "TOOLS") AND statusCategory != Done ORDER BY Rank ASC"#);
        assert_eq!(quick_scope_jql(&TeamScope::Label("PLAT".into()), &c, false), r#"labels = "PLAT""#);
        assert_eq!(quick_scope_jql(&TeamScope::All, &c, false), r#"labels in ("PLAT", "TOOLS")"#);
        assert_eq!(jql_no_team(&c, false), r#"(labels is EMPTY OR labels not in ("PLAT", "TOOLS"))"#);
        assert_eq!(quick_scope_jql(&TeamScope::None, &c, false), jql_no_team(&c, false));
    }

    #[test]
    fn jql_epics_with_allowlist() {
        let mut c = cfg();
        c.roadmap.epics = vec!["CI-9".into()];
        assert_eq!(
            jql_epics(&c, &["CI-1".into()]),
            r#"project = "CI" AND issuetype = Epic AND (labels in ("PLAT") OR labels = "roadmap" OR key in (CI-1) OR key in (CI-9)) ORDER BY Rank ASC"#
        );
    }

    #[test]
    fn jql_epic_children_spans_the_projects() {
        let mut c = cfg();
        c.scope.projects = vec!["CI".into(), "OPS".into()];
        assert_eq!(jql_epic_children(&c, &["CI-1".into(), "CI-2".into()]), r#"project in ("CI", "OPS") AND parent in (CI-1, CI-2) ORDER BY Rank ASC"#);
    }

    #[test]
    fn children_of_unions_epic_children_and_team_open_once() {
        let mut s = Store::default();
        s.absorb(ListId::EpicChildren, vec![issue("CI-1", "Done", Some("E-1"), None), issue("CI-2", "New", Some("E-1"), None)]);
        s.absorb(
            ListId::TeamOpen,
            vec![issue("CI-2", "New", Some("E-1"), None), issue("CI-3", "New", Some("E-1"), None), issue("CI-4", "New", Some("E-2"), None)],
        );
        let keys: Vec<&str> = s.children_of("E-1").iter().map(|i| i.key.as_str()).collect();
        assert_eq!(keys, ["CI-1", "CI-2", "CI-3"]);
    }

    #[test]
    fn absorb_and_projections() {
        let mut s = Store::default();
        s.absorb(
            ListId::TeamOpen,
            vec![issue("CI-1", "New", Some("E-1"), None), issue("CI-2", "Planning", Some("E-1"), Some("Ada Lovelace")), issue("CI-3", "New", None, None)],
        );
        assert_eq!(s.list(ListId::TeamOpen).iter().map(|i| i.key.as_str()).collect::<Vec<_>>(), ["CI-1", "CI-2", "CI-3"]);
        assert_eq!(s.children_of("E-1").len(), 2);
    }

    #[test]
    fn detail_survives_list_refresh() {
        let mut s = Store::default();
        s.absorb(ListId::TeamOpen, vec![issue("CI-1", "New", None, None)]);
        let mut d = issue("CI-1", "New", None, None);
        d.description = Some(serde_json::json!({"type": "doc"}));
        s.merge_detail(d);
        s.absorb(ListId::TeamOpen, vec![issue("CI-1", "Planning", None, None)]);
        let i = s.get("CI-1").unwrap();
        assert_eq!(i.status.name, "Planning");
        assert!(i.description.is_some());
    }

    #[test]
    fn a_cached_search_list_is_never_the_launch_base() {
        let mut cached = Store::default();
        cached.absorb(ListId::TeamOpen, vec![issue("CI-1", "New", None, None)]);
        cached.absorb(ListId::Search, vec![issue("CI-2", "Blocked", None, None)]);
        let s = Store::for_launch(Some(cached));
        assert!(!s.lists.contains_key(&ListId::Search), "the previous run's search is dropped");
        assert_eq!(s.list(ListId::TeamOpen).len(), 1, "the team list stays");
        assert!(s.get("CI-2").is_some(), "issue records stay for the detail pane");
        assert!(Store::for_launch(None).lists.is_empty());
    }

    #[test]
    fn cache_round_trip() {
        let _g = crate::testutil::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var("JIRA_TICKETS_CACHE_DIR", dir.path()) };
        let mut s = Store::default();
        s.absorb(ListId::Epics, vec![issue("E-1", "In Progress", None, None)]);
        s.save_cached();
        let back = Store::load_cached().unwrap();
        assert_eq!(back.list(ListId::Epics).len(), 1);
        unsafe { std::env::remove_var("JIRA_TICKETS_CACHE_DIR") };
    }
}
