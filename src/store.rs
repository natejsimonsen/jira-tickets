//! One cached store of issues. Views are projections over it.

use crate::config::Config;
use crate::jira::Issue;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

#[derive(Hash, Eq, PartialEq, Clone, Copy, Debug, Serialize, Deserialize)]
pub enum ListId {
    TeamOpen,
    Epics,
    TeamInProgress,
    Search,
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

    pub fn by_status(&self, id: ListId, status: &str) -> Vec<&Issue> {
        self.list(id).into_iter().filter(|i| i.status.name == status).collect()
    }

    pub fn by_parent(&self, id: ListId) -> BTreeMap<Option<String>, Vec<&Issue>> {
        let mut out: BTreeMap<Option<String>, Vec<&Issue>> = BTreeMap::new();
        for i in self.list(id) {
            out.entry(i.parent_key.clone()).or_default().push(i);
        }
        out
    }

    /// Assignee display name → issues, de-duplicated across lists, in list order.
    pub fn by_assignee(&self, ids: &[ListId]) -> BTreeMap<String, Vec<&Issue>> {
        let mut out: BTreeMap<String, Vec<&Issue>> = BTreeMap::new();
        let mut seen = std::collections::HashSet::new();
        for id in ids {
            for i in self.list(*id) {
                if !seen.insert(i.key.clone()) {
                    continue;
                }
                let name = i.assignee.as_ref().map(|a| a.display_name.clone()).unwrap_or_default();
                out.entry(name).or_default().push(i);
            }
        }
        out
    }

    pub fn children_of(&self, epic_key: &str) -> Vec<&Issue> {
        self.list(ListId::TeamOpen).into_iter().filter(|i| i.parent_key.as_deref() == Some(epic_key)).collect()
    }

    pub fn load_cached() -> Option<Store> {
        crate::cache::load(CACHE_KEY)
    }

    pub fn save_cached(&self) {
        crate::cache::store(CACHE_KEY, self);
    }
}

pub fn jql_quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

pub fn jql_team_open(cfg: &Config) -> String {
    format!("project = {} AND labels = {} AND statusCategory != Done ORDER BY Rank ASC", jql_quote(&cfg.scope.project), jql_quote(&cfg.scope.team_label))
}

pub fn jql_epics(cfg: &Config, parent_keys: &[String]) -> String {
    let labels: Vec<String> = cfg.roadmap.lanes.keys().map(|l| jql_quote(l)).collect();
    let mut clauses = vec![format!("labels in ({})", labels.join(", "))];
    if !parent_keys.is_empty() {
        clauses.push(format!("key in ({})", parent_keys.join(", ")));
    }
    if !cfg.roadmap.epics.is_empty() {
        clauses.push(format!("key in ({})", cfg.roadmap.epics.join(", ")));
    }
    format!("project = {} AND issuetype = Epic AND ({}) ORDER BY Rank ASC", jql_quote(&cfg.scope.project), clauses.join(" OR "))
}

pub fn jql_team_in_progress(cfg: &Config) -> String {
    let projects: Vec<String> = cfg.scope.projects.iter().map(|p| jql_quote(p)).collect();
    let members: Vec<String> = cfg.team.members.iter().map(|m| jql_quote(m)).collect();
    format!("project in ({}) AND assignee in ({}) AND statusCategory = \"In Progress\" ORDER BY updated DESC", projects.join(", "), members.join(", "))
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
        assert_eq!(jql_team_open(&c), r#"project = "CI" AND labels = "PLAT" AND statusCategory != Done ORDER BY Rank ASC"#);
        assert_eq!(
            jql_epics(&c, &["CI-1".into(), "CI-2".into()]),
            r#"project = "CI" AND issuetype = Epic AND (labels in ("PLAT") OR key in (CI-1, CI-2)) ORDER BY Rank ASC"#
        );
        assert_eq!(jql_epics(&c, &[]), r#"project = "CI" AND issuetype = Epic AND (labels in ("PLAT")) ORDER BY Rank ASC"#);
        assert_eq!(
            jql_team_in_progress(&c),
            r#"project in ("CI", "OPS") AND assignee in ("Ada Lovelace", "Shawn Sorichetti") AND statusCategory = "In Progress" ORDER BY updated DESC"#
        );
        assert_eq!(jql_quote(r#"a"b"#), r#""a\"b""#);
    }

    #[test]
    fn jql_epics_with_allowlist() {
        let mut c = cfg();
        c.roadmap.epics = vec!["CI-9".into()];
        assert_eq!(
            jql_epics(&c, &["CI-1".into()]),
            r#"project = "CI" AND issuetype = Epic AND (labels in ("PLAT") OR key in (CI-1) OR key in (CI-9)) ORDER BY Rank ASC"#
        );
    }

    #[test]
    fn absorb_and_projections() {
        let mut s = Store::default();
        s.absorb(
            ListId::TeamOpen,
            vec![issue("CI-1", "New", Some("E-1"), None), issue("CI-2", "Planning", Some("E-1"), Some("Ada Lovelace")), issue("CI-3", "New", None, None)],
        );
        assert_eq!(s.list(ListId::TeamOpen).iter().map(|i| i.key.as_str()).collect::<Vec<_>>(), ["CI-1", "CI-2", "CI-3"]);
        assert_eq!(s.by_status(ListId::TeamOpen, "New").len(), 2);
        let groups = s.by_parent(ListId::TeamOpen);
        assert_eq!(groups[&Some("E-1".to_string())].len(), 2);
        assert_eq!(groups[&None].len(), 1);
        let by = s.by_assignee(&[ListId::TeamOpen]);
        assert_eq!(by["Ada Lovelace"][0].key, "CI-2");
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
