//! What the views ask for, and how each `Action` changes the App after the frame.

use super::msg::{Msg, done_toast};
use super::{App, Screen, ToastKind, open_url};
use crate::auth::{self, Login};
use crate::jira;
use crate::store::{self, ListId};
use crate::views;
use std::time::Duration;

pub enum Action {
    SetScreen(Screen),
    Refresh,
    OpenUrl(String),
    Copy(String),
    Select(String),
    SubmitToken,
    FindToken,
    SignOut,
    Toast(String, bool),
    SetEpic(Option<String>),
    SetSprint(crate::filter::SprintScope),
    SetTeam(crate::filter::TeamScope),
    /// Adds the scoped team's label to the selected issue (key `l`).
    ApplyTeamLabel,
    SetQuick(Option<usize>),
    ToggleMine,
    ToggleRoadmap,
    /// Resets every row filter (quick filter, Mine, Roadmap, typed JQL); the team, epic and sprint scopes stay.
    ClearFilters,
    RunSearch(String),
    /// A JQL search typed into the search box (it stays there while active).
    RunTypedSearch(String),
    /// Plain search-box text searched on the server within the current scope (`store::scope_text_jql`).
    RunTextSearch(String),
    /// The box no longer holds a term: back to the scope's rows (the quick filter's or No team label's search
    /// is rerun, the team list needs no fetch). A no-op when something earlier this frame already ended it.
    EndTextSearch,
    ToggleGoTo,
    GoTo,
    Move(i32),
    Comment,
    OpenTransition,
    OpenAssign,
    OpenSprint,
    OpenEdit,
    PickerMove(i32),
    PickerConfirm,
    PickerCancel,
    Rank(i32),
    /// Shows or hides the keyboard help overlay (`?`).
    ToggleHelp,
    /// Switches the Grooming screen between Pre-check and Session.
    GroomMode(super::GroomMode),
    /// Moves the Grooming cursor: a pre-check row, or a session candidate.
    GroomMove(i32),
    /// Steps the session rail by `delta` epics, skipping those with nothing in Planning when `skip_empty`.
    GroomEpic {
        delta: i32,
        skip_empty: bool,
    },
    /// Judges the pre-check row under the cursor and moves on.
    Verdict(crate::grooming::Verdict),
    /// Clears the previous pre-check row's verdict and moves back to it.
    UndoVerdict,
    /// Adds or removes a needs-detail code on the pre-check row under the cursor (`grooming::toggle_code`).
    ToggleCode(crate::grooming::Code),
    /// Opens the pre-check summary dialog.
    OpenSummary,
    /// Closes the pre-check summary dialog.
    CloseSummary,
    /// Transitions the summary's promoted tickets `New → Planning`, then writes the report.
    Promote,
    /// Answers "Report for today exists. Overwrite?".
    Overwrite(bool),
    /// The pre-check focus moved to this issue: fetch its full detail (description) without selecting it.
    GroomFocusDetail(String),
}

impl App {
    /// The search the team picker and quick filter call for: the quick filter
    /// ANDed with the team clause, or under No team label the no-team query
    /// alone. None when neither applies (the list shows TeamOpen).
    pub(super) fn scoped_search_jql(&self) -> Option<String> {
        let quick = self.filter.quick.and_then(|i| self.cfg.issues.filters.get(i));
        match (quick, &self.filter.team) {
            (Some(f), team) => Some(format!("{} AND ({})", store::quick_scope_jql(team, &self.cfg, !self.members_disabled), f.jql)),
            (None, crate::filter::TeamScope::None) => Some(store::jql_no_team(&self.cfg, !self.members_disabled)),
            (None, _) => None,
        }
    }

    /// The query behind the rows the list shows before any text: the quick filter or No team label search,
    /// else the picked team's open work (TeamOpen narrowed to the team, as `filter::visible` narrows it).
    fn scope_jql(&self) -> String {
        self.scoped_search_jql()
            .unwrap_or_else(|| format!("{} AND statusCategory != Done", store::quick_scope_jql(&self.filter.team, &self.cfg, !self.members_disabled)))
    }

    /// The server text search for `term` within the current team, quick filter, sprint and epic scope.
    pub(super) fn text_search_jql(&self, term: &str) -> String {
        store::scope_text_jql(&self.scope_jql(), &self.filter.sprint, self.filter.epic.as_deref(), term)
    }

    /// Each frame: once the box holds a plain term that has sat still for the debounce, search the server
    /// for it in the current scope (again whenever that scope changes). When the box no longer holds a term,
    /// the text search goes and the list returns to its scope's rows.
    pub(super) fn text_search_tick(&mut self, ctx: &egui::Context) {
        let since = self.text_edited.map_or(Duration::MAX, |t| t.elapsed());
        match crate::filter::text_search_term(&self.filter.text).map(String::from) {
            // Enter ran this very text as JQL: that search stands until the box changes.
            Some(term) if crate::views::issues::typed_for(self.typed_search, self.last_search_jql.as_deref(), &term) => {}
            Some(term) if crate::filter::should_text_search(&term, since) => {
                if self.text_search_jql.as_deref() != Some(self.text_search_jql(&term).as_str()) {
                    self.actions.push(Action::RunTextSearch(term));
                }
            }
            Some(_) => ctx.request_repaint_after(crate::filter::TEXT_SEARCH_DEBOUNCE.saturating_sub(since)),
            None if self.text_search.is_some() => self.actions.push(Action::EndTextSearch),
            None => {}
        }
    }

    /// Runs `jql` (non-empty) as the Search list under the current `search_seq`,
    /// replacing any search in flight.
    fn search(&mut self, jql: &str) {
        // Only the searches built from the team scope while members are enabled carry
        // them; a JQL the user typed, or one built labels-only, never retries.
        let built = |j: Option<String>| j.is_some_and(|j| j.trim() == jql.trim());
        let text = crate::filter::text_search_term(&self.filter.text).map(|t| self.text_search_jql(t));
        let with_members = !self.members_disabled && (built(self.scoped_search_jql()) || built(text));
        self.search_active = true;
        self.last_search_jql = Some(jql.trim().to_string());
        let full = format!("project = {} AND ({}) ORDER BY Rank ASC", store::jql_quote(&self.cfg.scope.project), jql.trim());
        let fields: Vec<String> = jira::LIST_FIELDS.iter().map(|s| s.to_string()).collect();
        self.lists.entry(ListId::Search).or_default().loading = false;
        // Without a client nothing is spawned, so nothing would ever clear it.
        self.search_pending = self.client.is_some().then_some(self.search_seq);
        self.load_list_seq(ListId::Search, full, fields, Some(self.search_seq), with_members);
    }

    pub(crate) fn apply(&mut self, a: Action, ctx: &egui::Context) {
        match a {
            Action::SetScreen(s) => self.screen = s,
            Action::GroomMode(m) => self.groom_mode(m),
            Action::GroomMove(d) => self.groom_move(d),
            Action::GroomEpic { delta, skip_empty } => self.groom_epic(delta, skip_empty),
            Action::Verdict(v) => self.groom_verdict(v),
            Action::UndoVerdict => self.groom_undo(),
            Action::ToggleCode(c) => self.groom_toggle_code(c),
            Action::OpenSummary => self.grooming.summary_open = self.grooming.mode == super::GroomMode::PreCheck,
            Action::CloseSummary => self.grooming.summary_open = false,
            Action::Promote => self.promote(),
            Action::Overwrite(yes) => self.overwrite_report(yes),
            Action::GroomFocusDetail(key) => {
                self.load_detail(&key, false);
                self.grooming.detail_for = Some(key);
            }
            Action::Refresh => {
                self.refresh_all();
                // A quick filter's, No team label's or the text search is part of what's shown; rerun it
                // too, keeping the cursor. The seq bump drops the older run's reply.
                let jql = match self.text_search.clone() {
                    Some(term) => Some(self.text_search_jql(&term)),
                    None => self.scoped_search_jql(),
                };
                if let Some(jql) = jql {
                    self.search_seq += 1;
                    self.search(&jql);
                    self.search_from_refresh = true;
                }
            }
            Action::OpenUrl(u) => open_url(&u),
            Action::Copy(t) => {
                ctx.copy_text(t);
                self.toast("Copied", false);
            }
            Action::Select(key) => {
                self.selected = Some(key.clone());
                self.load_detail(&key, false);
            }
            Action::SubmitToken => {
                let t = self.token_input.trim().to_string();
                if t.is_empty() {
                    return;
                }
                self.token_error = None;
                self.connect(Login { email: auth::login_email(&self.cfg.email), token: t, source: auth::Source::Pasted });
            }
            Action::FindToken => {
                self.finding_token = true;
                let (tx, ctx2, cfg) = (self.tx.clone(), self.ctx.clone(), self.cfg.clone());
                std::thread::spawn(move || {
                    let _ = tx.send(Msg::Login(auth::find_login(&cfg.email, cfg.api_token.as_deref(), cfg.op_account.as_deref())));
                    ctx2.request_repaint();
                });
            }
            Action::SignOut => {
                auth::forget_token();
                self.token_error = None;
                self.client = None;
                self.login = None;
                self.me = None;
            }
            Action::Toast(t, e) => self.toast(t, e),
            Action::SetEpic(e) => {
                self.filter.epic = e;
                self.cursor = 0;
            }
            Action::SetSprint(s) => {
                self.filter.sprint = s;
                // A pick made before the board answers is kept (`sprint_scope_on_answer`).
                self.sprint_default_applied = true;
                self.cursor = 0;
            }
            Action::SetTeam(t) => {
                let was_none = self.filter.team == crate::filter::TeamScope::None;
                self.filter.team = t;
                self.cursor = 0;
                // A quick filter's search, and No team label's, is scoped to the team; rerun it
                // for the new one. Leaving No team label drops its search. A text search
                // is rerun in the new scope by `text_search_tick` instead.
                match self.scoped_search_jql() {
                    _ if self.text_search.is_some() => {}
                    Some(jql) => self.actions.push(Action::RunSearch(jql)),
                    None if was_none => self.actions.push(Action::RunSearch(String::new())),
                    None => {}
                }
            }
            Action::ApplyTeamLabel => {
                let crate::filter::TeamScope::Label(label) = &self.filter.team else {
                    self.toast_kind("Pick a team to label", ToastKind::Info);
                    return;
                };
                let Some(i) = self.selected.as_ref().and_then(|k| self.store.get(k)) else { return };
                if i.labels.iter().any(|l| l == label) {
                    self.toast_kind("Already labelled", ToastKind::Info);
                    return;
                }
                let (key, label) = (i.key.clone(), label.clone());
                self.spawn_write(move |c, tx| {
                    let _ = tx.send(Msg::Done { key: key.clone(), what: "Label", result: c.add_label(&key, &label) });
                });
            }
            Action::SetQuick(q) => {
                self.filter.quick = q;
                // Under a text search `text_search_tick` reruns it with the quick filter in its scope.
                if self.text_search.is_none() {
                    self.actions.push(Action::RunSearch(self.scoped_search_jql().unwrap_or_default()));
                }
            }
            Action::ToggleMine => {
                self.filter.mine = !self.filter.mine;
                self.cursor = 0;
            }
            Action::ToggleRoadmap => {
                self.filter.roadmap = !self.filter.roadmap;
                self.cursor = 0;
            }
            Action::ClearFilters => {
                self.filter.mine = false;
                self.filter.roadmap = false;
                self.filter.quick = None;
                // The box too: a local text filter as well as typed JQL ("Clear search" shows for either).
                self.filter.text.clear();
                self.cursor = 0;
                // No team label is itself a search; keep it, without the quick filter or typed JQL.
                match self.scoped_search_jql() {
                    Some(jql) => self.apply(Action::RunSearch(jql), ctx),
                    None if self.search_active => self.apply(Action::RunSearch(String::new()), ctx),
                    None => {}
                }
            }
            Action::RunTypedSearch(jql) => {
                // A typed search replaces any quick filter.
                self.filter.quick = None;
                self.apply(Action::RunSearch(jql.clone()), ctx);
                self.typed_search = !jql.trim().is_empty();
            }
            Action::EndTextSearch => {
                if self.text_search.is_some() {
                    self.apply(Action::RunSearch(self.scoped_search_jql().unwrap_or_default()), ctx);
                }
            }
            Action::RunTextSearch(term) => {
                let jql = self.text_search_jql(&term);
                self.apply(Action::RunSearch(jql.clone()), ctx);
                self.text_search = Some(term);
                self.text_search_jql = Some(jql);
            }
            Action::RunSearch(jql) => {
                self.cursor = 0;
                self.search_from_refresh = false;
                self.typed_search = false;
                self.text_search = None;
                self.text_search_jql = None;
                self.search_seq += 1;
                if jql.trim().is_empty() {
                    self.search_active = false;
                    self.filter.quick = None;
                    if crate::filter::looks_like_jql(&self.filter.text) {
                        self.filter.text.clear();
                    }
                    self.last_search_jql = None;
                    self.search_pending = None;
                    self.typed_shown = false;
                    self.text_shown = None;
                    self.store.lists.remove(&ListId::Search);
                    // Drops the error and any in-flight flag; late results are ignored by seq.
                    self.lists.remove(&ListId::Search);
                    return;
                }
                self.search(&jql);
            }
            Action::ToggleGoTo => self.goto = if self.goto.is_some() { None } else { Some(String::new()) },
            Action::GoTo => {
                if let Some(text) = self.goto.take()
                    && let Some(key) = crate::filter::parse_goto(&text, &self.cfg.scope.project)
                {
                    if let Some(i) = views::issues::current_rows(self).iter().position(|i| i.key == key) {
                        self.cursor = i;
                    }
                    self.selected = Some(key.clone());
                    self.load_detail(&key, false);
                }
            }
            Action::Move(delta) => {
                let n = views::issues::current_rows(self).len();
                if n == 0 {
                    return;
                }
                let rows: Vec<String> = views::issues::current_rows(self).iter().map(|i| i.key.clone()).collect();
                let at = self.selected.as_ref().and_then(|k| rows.iter().position(|r| r == k));
                self.cursor = move_target(at, delta, n);
                if let Some(key) = rows.get(self.cursor).cloned() {
                    self.actions.push(Action::Select(key));
                }
            }
            Action::Comment => {
                let Some(key) = self.selected.clone() else { return };
                let draft = self.drafts.get(&key).map(String::as_str).unwrap_or_default();
                if !views::detail::comment_enabled(draft, &self.posting) {
                    return;
                }
                let text = draft.trim().to_string();
                // spawn is a no-op without a client; don't leave `posting` stuck.
                if self.client.is_none() {
                    return;
                }
                self.posting = Some(key.clone());
                self.spawn_write(move |c, tx| {
                    let _ = tx.send(Msg::Done { key: key.clone(), what: "Comment", result: c.add_comment(&key, &text) });
                });
            }
            Action::OpenTransition => self.open_transition(),
            Action::OpenAssign => self.open_assign(),
            Action::OpenSprint => self.open_sprint(),
            Action::OpenEdit => self.open_edit(),
            Action::PickerMove(d) => self.picker_move(d),
            Action::ToggleHelp => self.help_open = !self.help_open,
            Action::PickerCancel => self.picker = None,
            Action::PickerConfirm => self.confirm_picker(),
            Action::Rank(delta) => {
                let Some(key) = self.selected.clone() else { return };
                if self.client.is_none() {
                    return;
                }
                let src = self.rows_list();
                let rows: Vec<String> = views::issues::current_rows(self).iter().map(|i| i.key.clone()).collect();
                let Some((neighbour, before)) = rank_target(&rows, &key, delta) else { return };
                // Read-only: say so and leave the row where it is.
                if let Err(e) = jira::check_writable() {
                    let (text, kind) = done_toast("Rank", &Err(e));
                    self.toast_kind(text, kind);
                    return;
                }
                // Move the row now; a failed rank swaps it back as soon as its Done lands.
                if let Some(list) = self.store.lists.get_mut(&src)
                    && let (Some(a), Some(b)) = (list.iter().position(|k| *k == key), list.iter().position(|k| *k == neighbour))
                {
                    list.swap(a, b);
                    self.pending_ranks.push((src, key.clone(), neighbour.clone()));
                    // The selected row's new index, not the old cursor, which can disagree with it.
                    if let Some(i) = rows.iter().position(|k| *k == key) {
                        self.cursor = (i as i32 + delta).clamp(0, rows.len() as i32 - 1) as usize;
                    }
                }
                self.spawn_write(move |c, tx| {
                    let (b, a) = if before { (Some(neighbour.as_str()), None) } else { (None, Some(neighbour.as_str())) };
                    let _ = tx.send(Msg::Done { key: key.clone(), what: "Rank", result: c.rank(&key, b, a) });
                });
            }
        }
    }
}

/// The row a j/k move lands on: from the selected row when it is in the list, else from the top
/// (the first move with nothing selected selects the first row).
pub fn move_target(selected_at: Option<usize>, delta: i32, n: usize) -> usize {
    match selected_at {
        Some(i) => (i as i32 + delta).clamp(0, n as i32 - 1) as usize,
        None => 0,
    }
}

/// Swaps `a` and `b` back in a list's key order (a failed optimistic rank).
pub(super) fn undo_swap(rows: &mut [String], a: &str, b: &str) {
    if let (Some(x), Some(y)) = (rows.iter().position(|k| k == a), rows.iter().position(|k| k == b)) {
        rows.swap(x, y);
    }
}

/// Which issue to rank next to, and whether it goes before it.
pub fn rank_target(rows: &[String], key: &str, delta: i32) -> Option<(String, bool)> {
    let i = rows.iter().position(|k| k == key)?;
    let j = i as i32 + delta;
    if j < 0 || j as usize >= rows.len() {
        return None;
    }
    Some((rows[j as usize].clone(), delta < 0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moves_start_from_the_selected_row() {
        assert_eq!(move_target(None, 1, 5), 0, "nothing selected: first row");
        assert_eq!(move_target(Some(2), 1, 5), 3);
        assert_eq!(move_target(Some(0), -1, 5), 0);
        assert_eq!(move_target(Some(4), 1, 5), 4);
    }

    #[test]
    fn failed_rank_swaps_back() {
        let mut rows = ["A", "B", "C"].map(String::from).to_vec();
        rows.swap(0, 1);
        undo_swap(&mut rows, "A", "B");
        assert_eq!(rows, ["A", "B", "C"]);
        undo_swap(&mut rows, "A", "Z");
        assert_eq!(rows, ["A", "B", "C"], "a missing key leaves the order alone");
    }

    #[test]
    fn rank_neighbours() {
        let rows = ["A", "B", "C"].map(String::from);
        assert_eq!(rank_target(&rows, "B", -1), Some(("A".to_string(), true)));
        assert_eq!(rank_target(&rows, "B", 1), Some(("C".to_string(), false)));
        assert_eq!(rank_target(&rows, "A", -1), None);
        assert_eq!(rank_target(&rows, "C", 1), None);
        assert_eq!(rank_target(&rows, "Z", 1), None);
    }
}
