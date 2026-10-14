//! Results from the background threads, and how each one changes the App.

use super::actions::{Action, undo_swap};
use super::loads::accept_search;
use super::pickers::{Picker, assign_cursor_after_filter, assign_rows, sprint_cursor, transition_cursor};
use super::{App, ToastKind, sprints_cache_key};
use crate::auth::{self, Login};
use crate::jira::{self, Issue, Resolution, Sprint, Transition, User};
use crate::store::ListId;
use std::time::Instant;

#[allow(clippy::large_enum_variant)] // Msgs are few and short-lived; keep the plain Result shape.
pub enum Msg {
    /// The token search: a login, or why each configured source failed.
    Login(Result<Login, Vec<String>>),
    Me(jira::Result<User>),
    List {
        id: ListId,
        result: jira::Result<Vec<Issue>>,
        stale: bool,
        /// The query carried the members' assignee clause, so an unresolved
        /// member may retry it without them (once; the retry carries none).
        with_members: bool,
        /// Which `RunSearch` a `ListId::Search` result answers; None for other lists.
        seq: Option<u64>,
    },
    Detail {
        key: String,
        result: jira::Result<Issue>,
    },
    /// A write finished. `what` names it for the toast ("Transition", "Rank", …).
    Done {
        key: String,
        what: &'static str,
        result: jira::Result<()>,
    },
    Transitions {
        key: String,
        result: jira::Result<Vec<Transition>>,
    },
    Resolutions(jira::Result<Vec<Resolution>>),
    /// `key` is the issue selected when the picker was asked for.
    Users {
        key: String,
        result: jira::Result<Vec<User>>,
    },
    Sprints {
        key: String,
        result: jira::Result<Vec<Sprint>>,
    },
    /// The board's active and future sprints, for the Issues sprint scope.
    BoardSprints(jira::Result<Vec<Sprint>>),
    /// The newest pre-check report in `report_dir` (None when there is none), read on every refresh.
    ReportLoaded(Option<(std::path::PathBuf, crate::report::Report)>),
}

impl App {
    pub(crate) fn handle(&mut self, msg: Msg) {
        match msg {
            Msg::ReportLoaded(report) => self.report_loaded(report),
            Msg::Login(Ok(login)) => {
                self.token_error = None;
                self.connect(login);
            }
            Msg::Login(Err(errors)) => {
                self.finding_token = false;
                self.login = None;
                self.token_error = (!errors.is_empty()).then(|| errors.join("\n"));
            }
            Msg::Me(Ok(u)) => {
                self.me = Some(u);
                // A pasted token is saved only once Jira accepted it.
                if let Some(login) = &self.login
                    && matches!(login.source, auth::Source::Pasted)
                {
                    let token = login.token.clone();
                    if let Err(e) = auth::save_token(&token) {
                        self.toast(format!("Signed in, but couldn't save the token: {e}"), true);
                    }
                    self.token_input.clear();
                }
            }
            Msg::Me(Err(jira::Error::Unauthorized(m))) => {
                self.client = None;
                self.token_error = Some(m);
            }
            Msg::Me(Err(e)) => self.toast(e.to_string(), true),
            Msg::List { id, result, stale: _, seq, with_members } => {
                if id == ListId::Search {
                    if !accept_search(self.search_seq, seq, self.search_active) {
                        // A pending seq older than the current run can never land.
                        if self.search_pending.is_some_and(|p| p != self.search_seq) {
                            self.search_pending = None;
                        }
                        return;
                    }
                    self.search_pending = None;
                    self.typed_shown = self.typed_search;
                    self.text_shown = self.text_search.clone();
                }
                // Gated on what this query carried, not on members_disabled: TeamOpen and
                // TeamInProgress can both fail on the same member and the second must still
                // retry (or clear) its list, while a members-free query never retries.
                let member_retry = should_retry_without_members(matches!(&result, Err(e) if jira::is_unresolved_assignee(e)), with_members);
                let entry = self.lists.entry(id).or_default();
                entry.loading = false;
                let reload = std::mem::take(&mut entry.reload_after);
                match result {
                    Ok(issues) => {
                        entry.error = None;
                        entry.fetched = Some(Instant::now());
                        self.offline = false;
                        self.store.absorb(id, issues);
                        self.store.fetched_at = Some(chrono::Utc::now().timestamp());
                        if self.persist_cache {
                            let store = self.store.clone();
                            std::thread::spawn(move || store.save_cached());
                        }
                        if id == ListId::TeamOpen && self.client.is_some() {
                            self.load_epics();
                        }
                    }
                    Err(jira::Error::Unauthorized(m)) => {
                        self.client = None;
                        self.token_error = Some(m);
                    }
                    // A configured member Jira doesn't know fails the whole query; retry on labels alone.
                    Err(_) if member_retry => {
                        entry.error = None;
                        if !self.members_disabled {
                            self.members_disabled = true;
                            self.toast("A team member in the config did not resolve; showing labelled tickets only", true);
                        }
                        match id {
                            // A text search is rerun by `text_search_tick`: its JQL changed with the members.
                            ListId::Search if self.text_search.is_some() => {}
                            ListId::Search => {
                                if let Some(jql) = self.scoped_search_jql() {
                                    self.actions.push(Action::RunSearch(jql));
                                }
                            }
                            // In progress is by assignee only: leave it as it is, not reloaded.
                            ListId::TeamInProgress => {}
                            _ => self.reload(id),
                        }
                    }
                    Err(e) => {
                        if marks_offline(id, &e) {
                            self.offline = true;
                        }
                        entry.error = Some(e.to_string());
                        // The previous search's rows were kept while this one ran; drop them so the error shows.
                        if id == ListId::Search {
                            self.store.lists.remove(&ListId::Search);
                        }
                    }
                }
                if reload {
                    self.reload(id);
                }
            }
            Msg::Detail { key, result } => {
                self.detail_loading.remove(&key);
                match result {
                    Ok(issue) => {
                        self.detail_fetched.insert(key, Instant::now());
                        self.store.merge_detail(issue);
                    }
                    Err(e) => self.toast(format!("Couldn't load {key}: {e}"), true),
                }
            }
            Msg::Done { key, what, result } => {
                if what == "Transition" {
                    // Only this issue's pending transition; another issue's stays for its own Done.
                    let pending = self.pending_transition.take_if(|(k, _)| *k == key);
                    if let Err(e) = &result
                        && jira::needs_resolution(e)
                        && let Some(pending) = pending
                        && self.client.is_some()
                    {
                        self.pending_transition = Some(pending);
                        self.picker_loading = true;
                        self.spawn(|c, tx| {
                            let _ = tx.send(Msg::Resolutions(c.resolutions()));
                        });
                        return;
                    }
                }
                if what == "Rank"
                    && let Some(i) = self.pending_ranks.iter().position(|(_, k, _)| *k == key)
                {
                    let (list, a, b) = self.pending_ranks.remove(i);
                    if result.is_err()
                        && let Some(rows) = self.store.lists.get_mut(&list)
                    {
                        undo_swap(rows, &a, &b);
                    }
                }
                if what == "Comment" {
                    self.posting = None;
                    if result.is_ok() {
                        self.drafts.remove(&key);
                    }
                }
                let (text, kind) = done_toast(what, &result);
                self.toast_kind(text, kind);
                // A write read-only mode refused changed nothing, so there is nothing to reload.
                if self.client.is_some() && reload_after_write(&result) {
                    self.load_detail(&key, true);
                    self.refresh_all();
                }
            }
            Msg::Transitions { key, result } => {
                self.picker_loading = false;
                match result {
                    Ok(items) => {
                        let status = self.store.get(&key).map(|i| i.status.name.clone()).unwrap_or_default();
                        let cursor = transition_cursor(&items, &status);
                        self.picker = Some(Picker::Transition { key, items, cursor });
                    }
                    Err(e) => self.toast(e.to_string(), true),
                }
            }
            Msg::Resolutions(result) => {
                self.picker_loading = false;
                match (result, self.pending_transition.take()) {
                    (Ok(items), Some((key, transition_id))) => self.picker = Some(Picker::Resolution { key, transition_id, items, cursor: 0 }),
                    (Err(e), _) => self.toast(e.to_string(), true),
                    _ => {}
                }
            }
            Msg::Users { key, result } => {
                self.picker_loading = false;
                match result {
                    Ok(items) => {
                        let current = self.store.get(&key).and_then(|i| i.assignee.clone());
                        let cursor = assign_cursor_after_filter(&assign_rows(&items, "", current.as_ref()).0, "", current.is_some());
                        self.picker = Some(Picker::Assign { key, items, current, cursor, filter: String::new() });
                    }
                    Err(e) => self.toast(e.to_string(), true),
                }
            }
            Msg::Sprints { key, result } => {
                self.picker_loading = false;
                match result {
                    Ok(items) => {
                        let current = self.store.get(&key).and_then(|i| i.sprint.clone());
                        let cursor = sprint_cursor(&items, current.as_deref());
                        self.picker = Some(Picker::Sprint { key, items, cursor });
                    }
                    Err(e) => self.toast(e.to_string(), true),
                }
            }
            Msg::BoardSprints(Ok(sprints)) => {
                self.sprints_answered = true;
                if self.persist_cache {
                    crate::cache::store(&sprints_cache_key(self.cfg.scope.board_id), &sprints);
                }
                self.sprints = sprints;
                let next = crate::filter::sprint_scope_on_answer(&self.filter.sprint, self.sprint_default_applied, &self.sprints);
                self.sprint_default_applied = true;
                if next != self.filter.sprint {
                    self.filter.sprint = next;
                    self.cursor = 0;
                }
            }
            // Keep the last good list on a failed refresh; only say so when there is none.
            Msg::BoardSprints(Err(e)) => {
                self.sprints_answered = true;
                if self.sprints.is_empty() {
                    self.toast(format!("Couldn't load sprints: {e}"), true);
                }
            }
        }
    }
}

/// The toast for a finished write: done, blocked by read-only mode (a notice, not a failure), or failed.
pub fn done_toast(what: &str, result: &jira::Result<()>) -> (String, ToastKind) {
    match result {
        Ok(()) => (format!("{what} done"), ToastKind::Success),
        Err(e) if jira::is_read_only(e) => (format!("Read-only mode: {} not sent to Jira", what.to_lowercase()), ToastKind::Info),
        Err(e) => (format!("{what} failed: {e}"), ToastKind::Error),
    }
}

/// Whether a finished write reloads the issue and the lists: not when read-only mode refused it (nothing changed).
pub fn reload_after_write(result: &jira::Result<()>) -> bool {
    !matches!(result, Err(e) if jira::is_read_only(e))
}

/// A query fails over to labels only when Jira rejected an assignee and the
/// query was sent with the members' clause; the retry carries none, so it is bounded.
fn should_retry_without_members(err_is_unresolved_assignee: bool, sent_with_members: bool) -> bool {
    err_is_unresolved_assignee && sent_with_members
}

/// Only a transport failure on a refresh list means we are offline; bad JQL,
/// rate limits, or a failed search just show on that list.
fn marks_offline(id: ListId, e: &jira::Error) -> bool {
    matches!(e, jira::Error::Network(_)) && matches!(id, ListId::TeamOpen | ListId::TeamInProgress | ListId::Epics)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_without_members_only_for_queries_that_carried_them() {
        assert!(should_retry_without_members(true, true), "unresolved member in a members query: retry on labels");
        assert!(!should_retry_without_members(true, false), "members-free query (e.g. a quick filter naming the user): no loop");
        assert!(!should_retry_without_members(false, true), "any other error: no retry");
        assert!(!should_retry_without_members(false, false));
    }

    #[test]
    fn only_network_errors_on_refresh_lists_mark_offline() {
        let net = jira::Error::Network("Network error: refused".into());
        for id in [ListId::TeamOpen, ListId::TeamInProgress, ListId::Epics] {
            assert!(marks_offline(id, &net), "{id:?}");
        }
        assert!(!marks_offline(ListId::Search, &net));
        for e in [jira::Error::Other("Jira returned 400: bad JQL".into()), jira::Error::RateLimited("rate limited (429)".into())] {
            assert!(!marks_offline(ListId::TeamOpen, &e));
        }
    }

    #[test]
    fn done_toast_names_read_only_blocks() {
        assert_eq!(done_toast("Rank", &Ok(())), ("Rank done".to_string(), ToastKind::Success));
        let ro = jira::Error::Other("read-only mode, so nothing was sent to Jira".into());
        assert_eq!(done_toast("Sprint move", &Err(ro)), ("Read-only mode: sprint move not sent to Jira".to_string(), ToastKind::Info));
        let other = jira::Error::Other("Jira returned 400".into());
        assert_eq!(done_toast("Assign", &Err(other)), ("Assign failed: Jira returned 400".to_string(), ToastKind::Error));
    }

    #[test]
    fn refused_writes_do_not_reload() {
        assert!(reload_after_write(&Ok(())));
        assert!(reload_after_write(&Err(jira::Error::Other("Jira returned 400".into()))), "a failure may have half-applied");
        assert!(!reload_after_write(&Err(jira::Error::Other("read-only mode, so nothing was sent to Jira".into()))));
    }
}
