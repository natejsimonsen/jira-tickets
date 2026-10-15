//! Loading the lists and issue details, and which list the Issues view shows.

use super::App;
use super::msg::Msg;
use crate::jira;
use crate::store::{self, ListId};
use std::time::{Duration, Instant};

/// A detail fetched this recently is not refetched on selection.
const DETAIL_FRESH: Duration = Duration::from_secs(60);

impl App {
    pub fn refresh_all(&mut self) {
        self.load_report();
        self.refresh_lists();
    }

    /// Everything `refresh_all` reloads except the report: Promote reloads the report itself, once, after
    /// writing it, so no earlier read can land after it with the previous report.
    pub(super) fn refresh_lists(&mut self) {
        self.last_refresh = Instant::now();
        let cfg = self.cfg.clone();
        let fields: Vec<String> = jira::LIST_FIELDS.iter().map(|s| s.to_string()).collect();
        self.load_list(ListId::TeamOpen, store::jql_team_open(&cfg, !self.members_disabled), fields.clone());
        if let Some(jql) = store::jql_team_in_progress(&cfg, !self.members_disabled) {
            self.load_list(ListId::TeamInProgress, jql, fields);
        }
        let board = cfg.scope.board_id;
        self.spawn(move |c, tx| {
            let _ = tx.send(Msg::BoardSprints(c.board_sprints(board, "active,future")));
        });
    }

    /// Epics depend on TeamOpen's parents, so they load after it lands.
    pub(super) fn load_epics(&mut self) {
        let cfg = self.cfg.clone();
        let mut parents: Vec<String> = self.store.list(ListId::TeamOpen).iter().filter_map(|i| i.parent_key.clone()).collect();
        parents.sort();
        parents.dedup();
        let mut fields: Vec<String> = jira::LIST_FIELDS.iter().map(|s| s.to_string()).collect();
        fields.extend(["description", "comment"].map(String::from));
        for f in [
            &cfg.roadmap.health_field,
            &cfg.roadmap.target_field,
            &cfg.roadmap.start_field,
            &cfg.roadmap.confidence_field,
            &cfg.roadmap.next_milestone_field,
            &cfg.roadmap.definition_of_done_field,
        ]
        .into_iter()
        .flatten()
        {
            fields.push(f.clone());
        }
        self.load_list(ListId::Epics, store::jql_epics(&cfg, &parents), fields);
    }

    /// TeamOpen and TeamInProgress are built with members unless they are disabled.
    pub fn load_list(&mut self, id: ListId, jql: String, fields: Vec<String>) {
        let with_members = matches!(id, ListId::TeamOpen | ListId::TeamInProgress) && !self.members_disabled;
        self.load_list_seq(id, jql, fields, None, with_members);
    }

    pub(super) fn load_list_seq(&mut self, id: ListId, jql: String, fields: Vec<String>, seq: Option<u64>, with_members: bool) {
        let client_present = self.client.is_some();
        let entry = self.lists.entry(id).or_default();
        if defer_load(client_present, entry.loading) {
            entry.reload_after = true;
        }
        if !should_load(client_present, entry.loading) {
            return;
        }
        entry.loading = true;
        self.spawn(move |c, tx| {
            let fields: Vec<&str> = fields.iter().map(String::as_str).collect();
            let _ = tx.send(Msg::List { id, result: c.search(&jql, &fields), stale: false, seq, with_members });
        });
    }

    /// Re-runs a refresh list's load (a Search is superseded, not reloaded).
    pub(super) fn reload(&mut self, id: ListId) {
        let fields: Vec<String> = jira::LIST_FIELDS.iter().map(|s| s.to_string()).collect();
        match id {
            ListId::TeamOpen => self.load_list(id, store::jql_team_open(&self.cfg, !self.members_disabled), fields),
            ListId::TeamInProgress => {
                if let Some(jql) = store::jql_team_in_progress(&self.cfg, !self.members_disabled) {
                    self.load_list(id, jql, fields);
                }
            }
            ListId::Epics => self.load_epics(),
            ListId::Search => {}
        }
    }

    /// The list the Issues view draws its rows from.
    pub fn rows_list(&self) -> ListId {
        let has_result = self.store.lists.contains_key(&ListId::Search);
        match rows_source(self.search_active, self.search_pending.is_some(), has_result) {
            RowsSource::Search => ListId::Search,
            RowsSource::TeamOpen => ListId::TeamOpen,
        }
    }

    /// Reads the newest report in `report_dir`. A local file read needs no client, so it runs on its own thread,
    /// not `spawn`.
    pub(super) fn load_report(&mut self) {
        #[cfg(test)]
        {
            self.report_reads += 1;
        }
        let dir = self.cfg.report_dir();
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            let _ = tx.send(Msg::ReportLoaded(crate::report::latest(&dir)));
            ctx.request_repaint();
        });
    }

    /// `force` refetches even a detail fetched within DETAIL_FRESH.
    pub fn load_detail(&mut self, key: &str, force: bool) {
        if !should_load(self.client.is_some(), self.detail_loading.contains(key)) {
            return;
        }
        if !force && self.detail_fetched.get(key).is_some_and(|t| t.elapsed() < DETAIL_FRESH) {
            return;
        }
        self.detail_loading.insert(key.to_string());
        let key = key.to_string();
        self.spawn(move |c, tx| {
            let _ = tx.send(Msg::Detail { key: key.clone(), result: c.issue(&key, &[]) });
        });
    }
}

/// A load starts only with a client to run it and none already in flight;
/// otherwise nothing would ever clear the loading flag.
fn should_load(client_present: bool, already_loading: bool) -> bool {
    client_present && !already_loading
}

/// A load asked for while one is in flight runs after it lands, so a write's
/// refresh is never lost to an older refresh still on the wire.
fn defer_load(client_present: bool, already_loading: bool) -> bool {
    client_present && already_loading
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowsSource {
    Search,
    TeamOpen,
}

/// Which list the Issues view shows. While a search runs with no earlier result
/// it keeps the TeamOpen rows on screen rather than an empty list; with an
/// earlier result it keeps that until the new one lands. Either way the rows in flight are the ones the
/// user was looking at: a Search result only exists once this session accepted one (`Store::for_launch`
/// drops a cached one, and clearing a search removes it).
fn rows_source(search_active: bool, pending: bool, has_search_result: bool) -> RowsSource {
    match (search_active, pending, has_search_result) {
        (false, _, _) => RowsSource::TeamOpen,
        (true, true, false) => RowsSource::TeamOpen,
        (true, _, _) => RowsSource::Search,
    }
}

/// A Search result lands only while a search is active and only for the latest run.
pub(super) fn accept_search(current_seq: u64, msg_seq: Option<u64>, active: bool) -> bool {
    active && msg_seq == Some(current_seq)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;

    #[test]
    fn loads_need_a_client_and_no_load_in_flight() {
        assert!(should_load(true, false));
        assert!(!should_load(true, true));
        assert!(!should_load(false, false));
        assert!(!should_load(false, true));
    }

    #[test]
    fn a_load_during_a_load_is_deferred_not_dropped() {
        assert!(defer_load(true, true));
        assert!(!defer_load(true, false), "starts now instead");
        assert!(!defer_load(false, true), "no client to run it");
        assert!(!defer_load(false, false));
    }

    #[test]
    fn rows_stay_on_screen_while_a_search_runs() {
        assert_eq!(rows_source(false, false, false), RowsSource::TeamOpen, "no search");
        assert_eq!(rows_source(false, false, true), RowsSource::TeamOpen, "no search, stale result");
        assert_eq!(rows_source(true, false, true), RowsSource::Search, "search with result");
        assert_eq!(rows_source(true, true, false), RowsSource::TeamOpen, "first search in flight");
        assert_eq!(rows_source(true, true, true), RowsSource::Search, "keeps the previous result");
        assert_eq!(rows_source(true, false, false), RowsSource::Search, "failed search shows its error");
        // A quick filter started at launch: the cache's Search list is gone (Store::for_launch), so the
        // rows in flight are the team list the user was looking at, not an old search.
        let store = Store::for_launch(Some({
            let mut s = Store::default();
            s.lists.insert(ListId::Search, vec!["CI-9".into()]);
            s
        }));
        assert_eq!(rows_source(true, true, store.lists.contains_key(&ListId::Search)), RowsSource::TeamOpen);
    }

    #[test]
    fn search_results_only_land_for_the_latest_active_run() {
        assert!(accept_search(3, Some(3), true), "current");
        assert!(!accept_search(3, Some(2), true), "stale");
        assert!(!accept_search(3, Some(3), false), "inactive");
        assert!(!accept_search(3, None, true), "untagged");
    }
}
