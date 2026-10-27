//! The Grooming screen's state changes: the report overlay, pre-check verdicts, and the session rail.

use super::msg::{Msg, done_toast};
use super::{App, GroomMode, PromoteRun, ToastKind};
use crate::grooming::{self, Candidate, Code, EpicQueue, Promotion, RunRow, Verdict};
use crate::jira::{self, Client, Issue};
use crate::report::{self, WriteError};
use crate::store::ListId;
use std::path::PathBuf;
use std::time::Instant;

/// The epic `delta` steps from `from` in a rail whose epics have `planning` tickets each, clamped to the ends.
/// With `skip_empty`, epics with no Planning tickets are stepped over; when none is left that way, `from` stays.
pub fn step_epic(planning: &[usize], from: usize, delta: i32, skip_empty: bool) -> usize {
    if planning.is_empty() || delta == 0 {
        return from.min(planning.len().saturating_sub(1));
    }
    let mut at = from.min(planning.len() - 1);
    for _ in 0..delta.unsigned_abs() {
        let mut next = at;
        loop {
            let Some(n) = (if delta > 0 { next.checked_add(1).filter(|n| *n < planning.len()) } else { next.checked_sub(1) }) else {
                return at;
            };
            next = n;
            if !skip_empty || planning[next] > 0 {
                break;
            }
        }
        at = next;
    }
    at
}

impl App {
    /// The pre-check rows: the team's `New` action items in rank order, numbered among the team's `New` tickets
    /// before epics are dropped, less the tickets this run already promoted (`GroomingState::retired`).
    pub fn pre_check_candidates(&self) -> Vec<Candidate<'_>> {
        let g = &self.cfg.grooming;
        grooming::new_candidates(&self.store.list(ListId::TeamOpen), &self.cfg.scope.team_label, &g.status_new, chrono::Local::now().date_naive())
            .into_iter()
            .filter(|c| crate::filter::is_action_item(c.issue) && !self.grooming.retired.contains(&c.issue.key))
            .collect()
    }

    /// The session rail: the team's action items (no epics) grouped by epic.
    pub fn session_queues(&self) -> Vec<EpicQueue<'_>> {
        grooming::epic_queues(&crate::filter::action_items(&self.store.list(ListId::TeamOpen)), &self.cfg.scope.team_label, &self.cfg.grooming)
    }

    /// The team's action items in `Ready for Work`, for the ceiling pill.
    pub fn ready_total(&self) -> usize {
        let (label, ready) = (&self.cfg.scope.team_label, &self.cfg.grooming.status_ready);
        crate::filter::action_items(&self.store.list(ListId::TeamOpen))
            .iter()
            .filter(|i| i.labels.iter().any(|l| l == label) && &i.status.name == ready)
            .count()
    }

    /// A team list landed: the promoted keys it shows out of `New` (or not at all) leave `retired`.
    pub(super) fn team_open_landed(&mut self, issues: &[Issue]) {
        let new = &self.cfg.grooming.status_new;
        self.grooming.retired.retain(|k| issues.iter().any(|i| &i.key == k && &i.status.name == new));
    }

    /// Keeps the newest report and, when it is a new one (another path than the one held), pre-fills a verdict
    /// from it for every key that has none yet. Re-reading the same report on refresh never brings back a
    /// verdict the user undid.
    pub(super) fn report_loaded(&mut self, report: Option<(PathBuf, crate::report::Report)>) {
        let held = self.grooming.report.as_ref().map(|(p, _)| p);
        if let Some((path, r)) = &report
            && held != Some(path)
        {
            let keys = r.promoted.iter().map(|p| &p.key).chain(r.needs_detail.iter().map(|n| &n.key));
            for key in keys {
                if !self.grooming.verdicts.contains_key(key)
                    && !self.grooming.retired.contains(key)
                    && let Some(v) = grooming::prefill(Some(r), key)
                {
                    self.grooming.verdicts.insert(key.clone(), v);
                }
            }
        }
        self.grooming.report = report;
    }

    pub(super) fn groom_mode(&mut self, mode: GroomMode) {
        if self.grooming.mode == mode {
            return;
        }
        let g = &mut self.grooming;
        g.mode = mode;
        g.cursor = 0;
        g.placement = None;
        g.bounce_text = None;
        g.focused_since = Some(Instant::now());
    }

    /// Pre-check: rows plus the summary (cursor == rows). Session: the selected epic's Planning candidates.
    pub(super) fn groom_move(&mut self, delta: i32) {
        let last = match self.grooming.mode {
            GroomMode::PreCheck => self.pre_check_candidates().len(),
            GroomMode::Session => self.session_queues().get(self.grooming.epic_index).map_or(0, |q| q.planning.len().saturating_sub(1)),
        };
        let to = (self.grooming.cursor as i64 + delta as i64).clamp(0, last as i64) as usize;
        if to != self.grooming.cursor {
            self.grooming.cursor = to;
            self.grooming.placement = None;
            self.grooming.focused_since = Some(Instant::now());
        }
    }

    pub(super) fn groom_epic(&mut self, delta: i32, skip_empty: bool) {
        let planning: Vec<usize> = self.session_queues().iter().map(|q| q.planning.len()).collect();
        let to = step_epic(&planning, self.grooming.epic_index, delta, skip_empty);
        if to != self.grooming.epic_index {
            let g = &mut self.grooming;
            g.epic_index = to;
            g.cursor = 0;
            g.placement = None;
            g.bounce_text = None;
            g.focused_since = Some(Instant::now());
        }
    }

    /// Judges the pre-check row under the cursor, then moves on (past the last row is the summary).
    pub(super) fn groom_verdict(&mut self, v: Verdict) {
        if self.grooming.mode != GroomMode::PreCheck {
            return;
        }
        let keys: Vec<String> = self.pre_check_candidates().iter().map(|c| c.issue.key.clone()).collect();
        let Some(key) = keys.get(self.grooming.cursor) else { return };
        self.grooming.verdicts.insert(key.clone(), v);
        self.grooming.cursor = (self.grooming.cursor + 1).min(keys.len());
        self.grooming.focused_since = Some(Instant::now());
    }

    /// Adds or removes `code` on the row under the cursor. The cursor stays, so one ticket can take several codes.
    pub(super) fn groom_toggle_code(&mut self, code: Code) {
        if self.grooming.mode != GroomMode::PreCheck {
            return;
        }
        let Some(key) = self.pre_check_candidates().get(self.grooming.cursor).map(|c| c.issue.key.clone()) else { return };
        let v = grooming::toggle_code(self.grooming.verdicts.get(&key), code);
        self.grooming.verdicts.insert(key, v);
    }

    /// The pre-check candidates' keys in rank order.
    fn candidate_keys(&self) -> Vec<String> {
        self.pre_check_candidates().iter().map(|c| c.issue.key.clone()).collect()
    }

    /// What Promote would do now: the plan over the candidates in rank order, and how many have no verdict.
    pub fn promotion_plan(&self) -> (Promotion, usize) {
        let keys = self.candidate_keys();
        (grooming::promote_order(&self.grooming.verdicts, &keys, self.cfg.grooming.planning_cap), grooming::unjudged(&self.grooming.verdicts, &keys))
    }

    /// The candidates as the report shows them, with each epic's numeric id and status.
    fn run_rows(&self) -> Vec<RunRow> {
        self.pre_check_candidates()
            .iter()
            .map(|c| {
                let i = c.issue;
                let epic = i.parent_key.as_ref().map(|k| report::EpicRef {
                    key: k.clone(),
                    summary: i.parent_summary.clone().unwrap_or_default(),
                    numeric_id: i.parent_id.clone(),
                    status: self.store.get(k).map(|e| e.status.name.clone()).filter(|s| !s.is_empty()),
                });
                RunRow {
                    key: i.key.clone(),
                    summary: i.summary.clone(),
                    rank: c.rank_pos,
                    age: c.age.clone(),
                    created: i.created.get(..10).unwrap_or_default().to_string(),
                    epic,
                }
            })
            .collect()
    }

    /// Promote: one `New → Planning` transition per promoted ticket, in rank order, with no fields, no
    /// resolution and no comment. The results come back as `Msg::Done { what: "Promote" }`.
    pub(super) fn promote(&mut self) {
        self.grooming.summary_open = false;
        if self.grooming.promoting || self.grooming.mode != GroomMode::PreCheck || self.client.is_none() {
            return;
        }
        if let Err(e) = jira::check_writable() {
            let (text, kind) = done_toast("Promote", &Err(e));
            self.toast_kind(text, kind);
            return;
        }
        let (plan, _) = self.promotion_plan();
        if plan.promoted.is_empty() {
            self.toast_kind("Nothing ready to promote", ToastKind::Info);
            return;
        }
        let run = PromoteRun {
            date: chrono::Local::now().date_naive().format("%Y-%m-%d").to_string(),
            rows: self.run_rows(),
            promoted: plan.promoted.clone(),
            held_back: plan.held_back.len(),
            needs_detail: plan.needs_detail,
        };
        let g = &mut self.grooming;
        g.promoting = true;
        g.promote_remaining = plan.promoted.len();
        g.promote_failures.clear();
        g.pending_report = None;
        g.promote_run = Some(run);
        let id = self.cfg.grooming.planning_transition_id.clone();
        for key in plan.promoted {
            let id = id.clone();
            self.spawn_write(move |c, tx| {
                let result = jira::check_writable().and_then(|_| c.transition(&key, &id, None));
                let _ = tx.send(Msg::Done { key, what: "Promote", result });
            });
        }
    }

    /// One Promote transition answered. A failure is collected and the rest carry on; after the last one the
    /// lists refresh once and the promoted epics' Ready for Work siblings are fetched for the report.
    pub(super) fn promote_done(&mut self, key: &str, result: jira::Result<()>) {
        let Some(run) = &self.grooming.promote_run else { return };
        if let Err(e) = result {
            let summary = run.rows.iter().find(|r| r.key == key).map(|r| r.summary.clone()).unwrap_or_default();
            self.grooming.promote_failures.push((key.to_string(), summary, e.to_string()));
        }
        self.grooming.promote_remaining = self.grooming.promote_remaining.saturating_sub(1);
        if self.grooming.promote_remaining > 0 {
            return;
        }
        self.retire_promoted();
        // The lists only: the report is reloaded once, after it is written (`write_report`).
        if self.client.is_some() {
            self.refresh_lists();
        }
        let mut parents: Vec<String> = self.promoted_rows().iter().filter_map(|r| r.epic.as_ref().map(|e| e.key.clone())).collect();
        parents.sort();
        parents.dedup();
        if parents.is_empty() || self.client.is_none() {
            self.finish_report(&[], None);
            return;
        }
        let jql = grooming::siblings_jql(&self.cfg.scope.project, &self.cfg.grooming.status_ready, &parents);
        self.spawn(move |c, tx| {
            let _ = tx.send(Msg::Siblings(c.search(&jql, jira::LIST_FIELDS)));
        });
    }

    /// The tickets this run moved to Planning leave the pre-check at once: their status is set to Planning in the
    /// store (the refresh confirms it), their verdicts go, and they are held in `retired`, so neither the plan nor a
    /// stale refresh or the report's prefill can offer them again before a team list shows them out of `New`.
    fn retire_promoted(&mut self) {
        let done: Vec<String> = self.promoted_rows().iter().map(|r| r.key.clone()).collect();
        let planning = self.cfg.grooming.status_planning.clone();
        for key in &done {
            if let Some(i) = self.store.issues.get_mut(key) {
                i.status.name = planning.clone();
                i.status.category = "indeterminate".into();
            }
            self.grooming.verdicts.remove(key);
            self.grooming.retired.insert(key.clone());
        }
        let n = self.pre_check_candidates().len();
        self.grooming.cursor = self.grooming.cursor.min(n);
    }

    /// The run's rows whose transition succeeded, in rank order.
    fn promoted_rows(&self) -> Vec<&RunRow> {
        let Some(run) = &self.grooming.promote_run else { return vec![] };
        let failed = |k: &str| self.grooming.promote_failures.iter().any(|f| f.0 == k);
        run.rows.iter().filter(|r| run.promoted.contains(&r.key) && !failed(&r.key)).collect()
    }

    pub(super) fn siblings_loaded(&mut self, result: jira::Result<Vec<Issue>>) {
        if !self.grooming.promoting || self.grooming.promote_remaining > 0 {
            return;
        }
        match result {
            Ok(siblings) => self.finish_report(&siblings, None),
            Err(e) => self.finish_report(&[], Some(e.to_string())),
        }
    }

    /// Renders the report from the run and the siblings, and writes it unless today's exists (then it waits on
    /// the confirm).
    fn finish_report(&mut self, siblings: &[Issue], siblings_error: Option<String>) {
        let Some(run) = &self.grooming.promote_run else { return };
        let cfg = &self.cfg;
        let promoted = grooming::promoted_epics(&self.promoted_rows(), siblings);
        let nd_rows: Vec<(&RunRow, Vec<Code>)> =
            run.needs_detail.iter().filter_map(|(k, codes)| run.rows.iter().find(|r| r.key == *k).map(|r| (r, codes.clone()))).collect();
        let (needs_detail, needs_detail_total) = grooming::needs_detail_epics(&nd_rows, cfg.grooming.needs_detail_cap);
        let jql = grooming::pre_check_jql(&cfg.scope.project, &cfg.scope.team_label, &cfg.grooming.status_new);
        let text = report::render(&report::WriterInput {
            date: &run.date,
            jql: &jql,
            site: cfg.site.trim_end_matches('/'),
            board_id: cfg.scope.board_id,
            filter_id: cfg.scope.filter_id,
            parent_board_id: cfg.scope.parent_board_id,
            planning_cap: cfg.grooming.planning_cap,
            needs_detail_cap: cfg.grooming.needs_detail_cap,
            promoted: &promoted,
            needs_detail: &needs_detail,
            needs_detail_total,
            failed: &self.grooming.promote_failures,
            scanned: run.rows.len(),
            held_back: run.held_back,
        });
        let promoted_n: usize = promoted.iter().map(|e| e.candidates.len()).sum();
        let lead = promote_lead(promoted_n, run.promoted.len());
        let lead = match siblings_error {
            Some(e) => format!("{lead} (Ready for Work siblings not loaded: {e})"),
            None => lead,
        };
        let date = run.date.clone();
        self.grooming.promoting = false;
        self.grooming.promote_run = None;
        self.grooming.last_summary = Some(text.clone());
        self.write_report(&date, text, false, &lead);
    }

    /// Writes today's report; an existing one waits on the confirm unless `overwrite`.
    fn write_report(&mut self, date: &str, text: String, overwrite: bool, lead: &str) {
        let failed = !self.grooming.promote_failures.is_empty();
        match report::write(&self.cfg.report_dir(), date, &text, overwrite) {
            Ok(path) => {
                let kind = if failed { ToastKind::Error } else { ToastKind::Success };
                self.toast_kind(format!("{lead} · report written to {}", path.display()), kind);
                self.load_report();
            }
            Err(WriteError::Exists(_)) => {
                self.grooming.pending_report = Some(text);
                if failed {
                    self.toast_kind(lead.to_string(), ToastKind::Error);
                }
            }
            Err(WriteError::Io(e)) => self.toast(format!("{lead} · couldn't write the report: {e}"), true),
        }
    }

    /// The overwrite confirm's answer.
    pub(super) fn overwrite_report(&mut self, yes: bool) {
        let Some(text) = self.grooming.pending_report.take() else { return };
        if yes {
            // The report's own date: the day Promote ran, even if the answer comes after midnight.
            let date = Some(report::parse(&text).date).filter(|d| !d.is_empty());
            let date = date.unwrap_or_else(|| chrono::Local::now().date_naive().format("%Y-%m-%d").to_string());
            self.write_report(&date, text, true, "Report replaced");
        } else {
            self.toast_kind("Kept the existing report", ToastKind::Info);
        }
    }

    /// Clears the previous row's verdict and moves back to it.
    pub(super) fn groom_undo(&mut self) {
        if self.grooming.mode != GroomMode::PreCheck || self.grooming.cursor == 0 {
            return;
        }
        let keys: Vec<String> = self.pre_check_candidates().iter().map(|c| c.issue.key.clone()).collect();
        let back = (self.grooming.cursor - 1).min(keys.len().saturating_sub(1));
        if let Some(key) = keys.get(back) {
            self.grooming.verdicts.remove(key);
            self.grooming.cursor = back;
            self.grooming.focused_since = Some(Instant::now());
        }
    }
}

/// Moves `key` to the status named `target_status`: the transition that leads there from its current status (by
/// target name, case-insensitive), with no fields.
pub fn transition_to(c: &Client, key: &str, target_status: &str) -> jira::Result<()> {
    let ts = c.transitions(key)?;
    let id =
        grooming::pick_transition(&ts, target_status).ok_or_else(|| jira::Error::Other(format!("no transition to {target_status} from the current status")))?;
    c.transition(key, &id, None)
}

/// The session's decisions: accept with placement, bounce with a comment, park.
impl App {
    /// Session: the focused Planning candidate.
    pub fn session_focus(&self) -> Option<&Issue> {
        let at = self.grooming.cursor;
        self.session_queues().get(self.grooming.epic_index).and_then(|q| q.planning.get(at).copied())
    }

    /// The selected epic's Ready for Work keys, in rank order.
    fn session_ready_keys(&self) -> Vec<String> {
        self.session_queues().get(self.grooming.epic_index).map(|q| q.ready.iter().map(|i| i.key.clone()).collect()).unwrap_or_default()
    }

    /// The candidate a decision acts on: in Session mode, with a client, and no decision in flight.
    fn decidable(&self) -> Option<String> {
        let ok = self.grooming.mode == GroomMode::Session && self.grooming.deciding.is_none() && self.client.is_some();
        ok.then(|| self.session_focus().map(|i| i.key.clone())).flatten()
    }

    /// `y`: the ghost starts under the Ready queue.
    pub(super) fn start_accept(&mut self) {
        if self.decidable().is_none() {
            return;
        }
        self.grooming.bounce_text = None;
        self.grooming.placement = Some(self.session_ready_keys().len());
    }

    pub(super) fn placement_move(&mut self, delta: i32) {
        let Some(at) = self.grooming.placement else { return };
        let n = self.session_ready_keys().len() as i64;
        self.grooming.placement = Some((at as i64 + delta as i64).clamp(0, n) as usize);
    }

    /// Enter in placement: one transition to Ready for Work, then one rank call before (or after) the ghost's
    /// neighbour. An empty Ready queue has nothing to rank against, so only the transition is made. A rank that
    /// fails after the move is its own answer ("Accept rank"): the ticket did move, so the store follows it.
    pub(super) fn accept(&mut self) {
        let Some(at) = self.grooming.placement else { return };
        let Some(key) = self.decidable() else { return };
        let (before, after) = grooming::placement_target(&self.session_ready_keys(), at);
        self.grooming.placement = None;
        self.grooming.deciding = Some(key.clone());
        let status = self.cfg.grooming.status_ready.clone();
        self.spawn_write(move |c, tx| {
            let (what, result) = match jira::check_writable().and_then(|_| transition_to(c, &key, &status)) {
                Err(e) => ("Accept", Err(e)),
                Ok(()) => match (&before, &after) {
                    (None, None) => ("Accept", Ok(())),
                    _ => match c.rank(&key, before.as_deref(), after.as_deref()) {
                        Ok(()) => ("Accept", Ok(())),
                        Err(e) => ("Accept rank", Err(jira::Error::Other(accept_rank_failed(&key, &status, &e)))),
                    },
                },
            };
            let _ = tx.send(Msg::Done { key, what, result });
        });
    }

    /// `b`: the comment box opens, empty, and takes focus.
    pub(super) fn start_bounce(&mut self) {
        if self.decidable().is_none() {
            return;
        }
        self.grooming.placement = None;
        self.grooming.bounce_text = Some(String::new());
        self.grooming.bounce_focus = true;
    }

    /// ⌘Enter in the bounce box: the comment first, then the transition to New. A blank or longer-than-two-line
    /// comment sends nothing.
    pub(super) fn bounce(&mut self) {
        let Some(text) = self.grooming.bounce_text.clone().filter(|t| grooming::bounce_comment_ok(t)) else { return };
        let Some(key) = self.decidable() else { return };
        self.grooming.deciding = Some(key.clone());
        let status = self.cfg.grooming.status_new.clone();
        self.spawn_write(move |c, tx| {
            // A move that fails after the comment posted is its own answer ("Bounce move"): the box must close so
            // the same comment is never posted twice.
            let (what, result) = match jira::check_writable().and_then(|_| c.add_comment(&key, text.trim())) {
                Err(e) => ("Bounce", Err(e)),
                Ok(()) => match transition_to(c, &key, &status) {
                    Ok(()) => ("Bounce", Ok(())),
                    Err(e) => ("Bounce move", Err(jira::Error::Other(bounce_move_failed(&key, &status, &e)))),
                },
            };
            let _ = tx.send(Msg::Done { key, what, result });
        });
    }

    /// `p`: one transition to Backlog.
    pub(super) fn park(&mut self) {
        let Some(key) = self.decidable() else { return };
        self.grooming.placement = None;
        self.grooming.bounce_text = None;
        self.grooming.deciding = Some(key.clone());
        let status = self.cfg.grooming.status_backlog.clone();
        self.spawn_write(move |c, tx| {
            let result = jira::check_writable().and_then(|_| transition_to(c, &key, &status));
            let _ = tx.send(Msg::Done { key, what: "Park", result });
        });
    }

    /// A decision answered. On success the ticket takes its new status in the store at once (the refresh the
    /// generic Done handling starts confirms it), so it leaves the Planning list and the focus moves on.
    pub(super) fn session_done(&mut self, key: &str, what: &str, result: &jira::Result<()>) {
        if self.grooming.deciding.as_deref() == Some(key) {
            self.grooming.deciding = None;
        }
        if what == "Bounce move" {
            // The comment is on the ticket; only the move is left to retry.
            self.grooming.bounce_text = None;
        }
        // The move to Ready for Work happened even though its rank did not.
        if result.is_err() && what != "Accept rank" {
            return;
        }
        let g = &self.cfg.grooming;
        let (status, category) = match what {
            "Accept" | "Accept rank" => (g.status_ready.clone(), "indeterminate"),
            "Bounce" => (g.status_new.clone(), "new"),
            _ => (g.status_backlog.clone(), "new"),
        };
        match what {
            "Accept" | "Accept rank" => self.grooming.placement = None,
            "Bounce" => self.grooming.bounce_text = None,
            _ => {}
        }
        let (epic, focus) = self.session_anchor();
        if let Some(i) = self.store.issues.get_mut(key) {
            i.status.name = status;
            i.status.category = category.into();
        }
        self.session_keep(epic, focus);
    }

    /// The selected epic's key and the focused candidate's, taken before the lists change.
    pub(super) fn session_anchor(&self) -> (Option<Option<String>>, Option<String>) {
        let epic = self.session_queues().get(self.grooming.epic_index).map(|q| q.epic.clone());
        (epic, self.session_focus().map(|i| i.key.clone()))
    }

    /// After the lists changed: the rail stays on the same epic (its place may move, as the rail sorts by Planning
    /// count), the cursor clamps to its candidates, and a new focused ticket restarts the timer.
    pub(super) fn session_keep(&mut self, epic: Option<Option<String>>, focus: Option<String>) {
        if self.grooming.mode != GroomMode::Session {
            return;
        }
        let queues = self.session_queues();
        let at = epic.and_then(|k| queues.iter().position(|q| q.epic == k)).unwrap_or(self.grooming.epic_index).min(queues.len().saturating_sub(1));
        let n = queues.get(at).map_or(0, |q| q.planning.len());
        drop(queues);
        self.grooming.epic_index = at;
        self.grooming.cursor = self.grooming.cursor.min(n.saturating_sub(1));
        if self.session_focus().map(|i| i.key.clone()) != focus {
            // A placement or bounce box was for the ticket that left.
            self.grooming.placement = None;
            self.grooming.bounce_text = None;
            self.grooming.focused_since = Some(Instant::now());
        }
    }
}

/// The toast when a bounce's comment posted but its move failed.
pub fn bounce_move_failed(key: &str, status: &str, e: &jira::Error) -> String {
    format!("Comment posted on {key}, but the move to {status} failed: {e}")
}

/// The toast when an accept's move to Ready for Work succeeded but its rank call failed.
pub fn accept_rank_failed(key: &str, status: &str, e: &jira::Error) -> String {
    format!("Moved {key} to {status}, but ranking failed: {e}")
}

/// "Promoted 9 to Planning", or "Promoted 8 of 10 to Planning, 2 failed".
pub fn promote_lead(done: usize, tried: usize) -> String {
    if done == tried { format!("Promoted {done} to Planning") } else { format!("Promoted {done} of {tried} to Planning, {} failed", tried - done) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epic_steps_clamp_and_skip_empty_rails() {
        let rail = [3, 0, 2, 0];
        assert_eq!(step_epic(&rail, 0, 1, true), 2, "skips the empty epic");
        assert_eq!(step_epic(&rail, 0, 1, false), 1, "Shift lands on it");
        assert_eq!(step_epic(&rail, 2, 1, true), 2, "nothing non-empty after: stays");
        assert_eq!(step_epic(&rail, 2, 1, false), 3);
        assert_eq!(step_epic(&rail, 2, -1, true), 0);
        assert_eq!(step_epic(&rail, 0, -1, true), 0, "clamped at the top");
        assert_eq!(step_epic(&rail, 3, 5, false), 3, "clamped at the bottom");
        assert_eq!(step_epic(&[], 4, 1, true), 0);
        assert_eq!(step_epic(&rail, 9, 0, true), 3, "an index past the end is pulled back");
    }

    #[test]
    fn promote_leads() {
        assert_eq!(promote_lead(3, 3), "Promoted 3 to Planning");
        assert_eq!(promote_lead(8, 10), "Promoted 8 of 10 to Planning, 2 failed");
    }

    fn team_new(key: &str, parent: Option<&str>) -> crate::jira::Issue {
        crate::jira::Issue {
            key: key.into(),
            summary: format!("Summary {key}"),
            labels: vec!["PLAT".into()],
            status: crate::jira::Status { name: "New".into(), category: "new".into() },
            parent_key: parent.map(String::from),
            parent_id: parent.map(|_| "10002".to_string()),
            parent_summary: parent.map(|p| format!("Epic {p}")),
            created: "2026-01-01T00:00:00.000-0700".into(),
            ..Default::default()
        }
    }

    fn promote_app(base: &str, report_dir: &std::path::Path) -> App {
        let mut cfg: crate::config::Config = toml::from_str("site = \"https://x.atlassian.net\"\nemail = \"a@b.c\"\n").unwrap();
        cfg.grooming.report_dir = report_dir.to_string_lossy().into_owned();
        let login = crate::auth::Login { email: "a@b.c".into(), token: "t".into(), source: crate::auth::Source::Env };
        let client = std::sync::Arc::new(crate::jira::Client::new(base, &login));
        let mut app = App::for_test(cfg, Some(client));
        app.screen = super::super::Screen::Grooming;
        app.store.absorb(ListId::TeamOpen, vec![team_new("CI-1", Some("CI-79")), team_new("CI-2", None), team_new("CI-3", None)]);
        app.grooming.verdicts.insert("CI-1".into(), Verdict::Ready);
        app.grooming.verdicts.insert("CI-2".into(), Verdict::Ready);
        app.grooming.verdicts.insert("CI-3".into(), Verdict::NeedsDetail(vec![grooming::Code::UnclearWhen]));
        app
    }

    /// Drains until `done` holds, for up to 2 s (the write worker and reads run on threads).
    fn drain_until(app: &mut App, done: impl Fn(&App) -> bool) {
        let start = Instant::now();
        while start.elapsed() < std::time::Duration::from_secs(2) {
            app.drain();
            if done(app) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("timed out");
    }

    #[test]
    fn promote_transitions_only_then_fetches_siblings_and_writes_the_report() {
        let _g = crate::jira::tests::WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (base, rx) = crate::jira::tests::stub_server(vec![
            ("POST", "/rest/api/3/issue/CI-1/transitions", serde_json::json!({})),
            ("POST", "/rest/api/3/issue/CI-2/transitions", serde_json::json!({})),
            ("POST", "/rest/api/3/search/jql", serde_json::json!({"issues": []})),
        ]);
        let dir = tempfile::tempdir().unwrap();
        let mut app = promote_app(&base, dir.path());
        app.actions.push(super::super::Action::Promote);
        app.drain();
        assert!(app.grooming.promoting, "in flight until every transition and the siblings answer");
        drain_until(&mut app, |a| !a.grooming.promoting);
        let recorded: Vec<_> = rx.try_iter().collect();
        let transitions: Vec<_> = recorded.iter().filter(|r| r.path.ends_with("/transitions")).collect();
        assert_eq!(
            transitions.iter().map(|r| r.path.as_str()).collect::<Vec<_>>(),
            ["/rest/api/3/issue/CI-1/transitions", "/rest/api/3/issue/CI-2/transitions"]
        );
        for r in &transitions {
            assert_eq!(r.method, "POST");
            let body: serde_json::Value = serde_json::from_str(&r.body).unwrap();
            assert_eq!(body, serde_json::json!({"transition": {"id": "11"}}), "no fields, no resolution, no comment");
        }
        assert!(!recorded.iter().any(|r| r.path.contains("/comment") || r.method == "PUT"), "Promote writes nothing else: {recorded:?}");
        let siblings: Vec<_> = recorded.iter().filter(|r| r.path == "/rest/api/3/search/jql" && r.body.contains("parent in (CI-79)")).collect();
        assert_eq!(siblings.len(), 1, "{recorded:?}");
        assert!(siblings[0].body.contains("Ready for Work"));
        let date = chrono::Local::now().date_naive().format("%Y-%m-%d").to_string();
        let text = std::fs::read_to_string(dir.path().join(format!("{date}.md"))).expect("report written");
        let back = crate::report::parse(&text);
        assert_eq!(back.promoted.iter().map(|p| p.key.as_str()).collect::<Vec<_>>(), ["CI-1", "CI-2"]);
        assert_eq!(back.needs_detail.iter().map(|n| (n.key.as_str(), n.codes.clone())).collect::<Vec<_>>(), [("CI-3", vec!["unclear-when".to_string()])]);
        assert!(text.contains("JQL: `project = CI AND labels = PLAT AND status = New ORDER BY rank ASC`"), "{text}");
        assert!(text.contains("issueParent=10002"), "the epic's numeric id: {text}");
        assert_eq!(app.grooming.last_summary.as_deref(), Some(text.as_str()));
        assert!(app.grooming.promote_failures.is_empty());
    }

    #[test]
    fn promoted_tickets_leave_the_plan_before_the_refresh_lands() {
        let _g = crate::jira::tests::WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // No search route: the refresh's lists fail (404) and keep their rows, as if they had not landed yet.
        let (base, _rx) = crate::jira::tests::stub_server(vec![
            ("POST", "/rest/api/3/issue/CI-1/transitions", serde_json::json!({})),
            ("POST", "/rest/api/3/issue/CI-2/transitions", serde_json::json!({})),
        ]);
        let dir = tempfile::tempdir().unwrap();
        let mut app = promote_app(&base, dir.path());
        app.actions.push(super::super::Action::Promote);
        drain_until(&mut app, |a| !a.grooming.promoting);
        assert_eq!(app.store.list(ListId::TeamOpen).len(), 3, "the team list still holds the rows from before the run");
        let (plan, _) = app.promotion_plan();
        assert!(plan.promoted.is_empty(), "nothing left to promote again: {:?}", plan.promoted);
        assert!(!app.grooming.verdicts.contains_key("CI-1") && !app.grooming.verdicts.contains_key("CI-2"));
        assert_eq!(app.pre_check_candidates().iter().map(|c| c.issue.key.as_str()).collect::<Vec<_>>(), ["CI-3"]);
        assert!(app.grooming.cursor <= 1);
    }

    #[test]
    fn epics_are_never_groomed() {
        let mut app = App::for_test(toml::from_str("site = \"https://x.atlassian.net\"\nemail = \"a@b.c\"\n").unwrap(), None);
        let mut epic = team_new("CI-304", None);
        epic.issue_type = "Epic".into();
        let mut ready_epic = team_new("CI-768", None);
        ready_epic.issue_type = "Epic".into();
        ready_epic.status.name = "Ready for Work".into();
        let mut ready = team_new("CI-9", None);
        ready.status.name = "Ready for Work".into();
        app.store.absorb(ListId::TeamOpen, vec![team_new("CI-1", None), epic, team_new("CI-2", None), ready_epic, ready]);
        let cands: Vec<(String, u32)> = app.pre_check_candidates().iter().map(|c| (c.issue.key.clone(), c.rank_pos)).collect();
        assert_eq!(cands, [("CI-1".to_string(), 1), ("CI-2".to_string(), 3)], "the epic is not a candidate, but it is numbered before it is dropped");
        let queues = app.session_queues();
        assert_eq!(queues.len(), 1);
        assert_eq!(queues[0].new_count, 2, "the epic in New is in no session queue");
        assert_eq!(queues[0].ready.iter().map(|i| i.key.as_str()).collect::<Vec<_>>(), ["CI-9"]);
        assert_eq!(app.ready_total(), 1, "the epic in Ready for Work is not counted");
    }

    #[test]
    fn a_stale_refresh_does_not_bring_promoted_tickets_back() {
        let _g = crate::jira::tests::WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // No search route: the refresh's own lists fail (404), so only the lists delivered below land.
        let (base, _rx) = crate::jira::tests::stub_server(vec![
            ("POST", "/rest/api/3/issue/CI-1/transitions", serde_json::json!({})),
            ("POST", "/rest/api/3/issue/CI-2/transitions", serde_json::json!({})),
        ]);
        let dir = tempfile::tempdir().unwrap();
        let mut app = promote_app(&base, dir.path());
        app.actions.push(super::super::Action::Promote);
        drain_until(&mut app, |a| !a.grooming.promoting && a.grooming.report.is_some() && !a.lists.get(&ListId::TeamOpen).is_some_and(|l| l.loading));
        let mut retired: Vec<&String> = app.grooming.retired.iter().collect();
        retired.sort();
        assert_eq!(retired, ["CI-1", "CI-2"]);
        // An eventually consistent search still shows them in New, and the report just written loads under a new path.
        let list = |status: &str| {
            let mut v = vec![team_new("CI-1", Some("CI-79")), team_new("CI-2", None), team_new("CI-3", None)];
            for i in v.iter_mut().take(2) {
                i.status.name = status.into();
            }
            v
        };
        app.handle(Msg::List { id: ListId::TeamOpen, result: Ok(list("New")), seq: None, with_members: false });
        let text = app.grooming.last_summary.clone().unwrap();
        app.handle(Msg::ReportLoaded(Some((dir.path().join("2099-01-01.md"), crate::report::parse(&text)))));
        assert_eq!(app.pre_check_candidates().iter().map(|c| c.issue.key.as_str()).collect::<Vec<_>>(), ["CI-3"]);
        assert!(!app.grooming.verdicts.contains_key("CI-1") && !app.grooming.verdicts.contains_key("CI-2"), "{:?}", app.grooming.verdicts);
        assert_eq!(app.grooming.retired.len(), 2, "still held while the list shows them in New");
        // The list catches up: they are in Planning, and the set lets go of them.
        app.handle(Msg::List { id: ListId::TeamOpen, result: Ok(list("Planning")), seq: None, with_members: false });
        assert!(app.grooming.retired.is_empty(), "{:?}", app.grooming.retired);
    }

    #[test]
    fn the_report_reloads_once_after_the_write() {
        let _g = crate::jira::tests::WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (base, _rx) = crate::jira::tests::stub_server(vec![
            ("POST", "/rest/api/3/issue/CI-1/transitions", serde_json::json!({})),
            ("POST", "/rest/api/3/issue/CI-2/transitions", serde_json::json!({})),
            ("POST", "/rest/api/3/search/jql", serde_json::json!({"issues": []})),
        ]);
        let dir = tempfile::tempdir().unwrap();
        let mut app = promote_app(&base, dir.path());
        app.actions.push(super::super::Action::Promote);
        drain_until(&mut app, |a| !a.grooming.promoting && a.grooming.report.is_some());
        assert_eq!(app.report_reads, 1, "one read, after the write: an earlier one could land last with the previous report");
        // Nothing later replaces it.
        let start = Instant::now();
        while start.elapsed() < std::time::Duration::from_millis(200) {
            app.drain();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let date = chrono::Local::now().date_naive().format("%Y-%m-%d").to_string();
        let held = app.grooming.report.as_ref().map(|(p, _)| p.clone());
        assert_eq!(held, Some(dir.path().join(format!("{date}.md"))), "the header shows the report just written");
    }

    #[test]
    fn promote_collects_failures_and_carries_on() {
        let _g = crate::jira::tests::WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // CI-1's transition has no route: the stub answers 404.
        let (base, rx) = crate::jira::tests::stub_server(vec![
            ("POST", "/rest/api/3/issue/CI-2/transitions", serde_json::json!({})),
            ("POST", "/rest/api/3/search/jql", serde_json::json!({"issues": []})),
        ]);
        let dir = tempfile::tempdir().unwrap();
        let mut app = promote_app(&base, dir.path());
        app.actions.push(super::super::Action::Promote);
        drain_until(&mut app, |a| !a.grooming.promoting);
        let recorded: Vec<_> = rx.try_iter().collect();
        assert_eq!(recorded.iter().filter(|r| r.path.ends_with("/transitions")).count(), 2, "the rest continue");
        assert_eq!(app.grooming.promote_failures.iter().map(|f| f.0.as_str()).collect::<Vec<_>>(), ["CI-1"]);
        assert!(!recorded.iter().any(|r| r.body.contains("parent in")), "the failed ticket's epic gets no siblings search");
        let text = app.grooming.last_summary.clone().unwrap();
        assert!(text.contains("## Failed transitions"), "{text}");
        assert_eq!(crate::report::parse(&text).promoted.iter().map(|p| p.key.as_str()).collect::<Vec<_>>(), ["CI-2"]);
    }

    #[test]
    fn an_existing_report_waits_for_overwrite() {
        let _g = crate::jira::tests::WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (base, _rx) = crate::jira::tests::stub_server(vec![
            ("POST", "/rest/api/3/issue/CI-1/transitions", serde_json::json!({})),
            ("POST", "/rest/api/3/issue/CI-2/transitions", serde_json::json!({})),
            ("POST", "/rest/api/3/search/jql", serde_json::json!({"issues": []})),
        ]);
        let dir = tempfile::tempdir().unwrap();
        let date = chrono::Local::now().date_naive().format("%Y-%m-%d").to_string();
        let path = dir.path().join(format!("{date}.md"));
        std::fs::write(&path, "old").unwrap();
        let mut app = promote_app(&base, dir.path());
        app.actions.push(super::super::Action::Promote);
        drain_until(&mut app, |a| !a.grooming.promoting);
        assert!(app.grooming.pending_report.is_some(), "asks before overwriting");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "old");
        app.actions.push(super::super::Action::Overwrite(false));
        app.drain();
        assert!(app.grooming.pending_report.is_none());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "old", "No keeps the old report");
        app.grooming.pending_report = Some("new".into());
        app.actions.push(super::super::Action::Overwrite(true));
        app.drain();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
    }

    #[test]
    fn read_only_promote_sends_nothing() {
        let _g = crate::jira::tests::WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _ro = crate::jira::tests::ReadOnlyGuard::new();
        let (base, rx) = crate::jira::tests::stub_server(vec![]);
        let dir = tempfile::tempdir().unwrap();
        let mut app = promote_app(&base, dir.path());
        app.actions.push(super::super::Action::Promote);
        app.drain();
        assert!(!app.grooming.promoting);
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert!(rx.try_iter().next().is_none());
        assert!(app.toast.as_ref().is_some_and(|t| t.text.contains("Read-only")));
    }

    fn session_issue(key: &str, status: &str) -> crate::jira::Issue {
        crate::jira::Issue {
            key: key.into(),
            summary: format!("Summary {key}"),
            labels: vec!["PLAT".into()],
            status: crate::jira::Status { name: status.into(), category: "indeterminate".into() },
            parent_key: Some("CI-79".into()),
            parent_summary: Some("Epic CI-79".into()),
            ..Default::default()
        }
    }

    /// A session on epic CI-79: CI-5 and CI-6 in Ready for Work (in rank order), CI-1 and CI-2 in Planning.
    fn session_app(base: &str) -> App {
        let cfg: crate::config::Config = toml::from_str("site = \"https://x.atlassian.net\"\nemail = \"a@b.c\"\n").unwrap();
        let login = crate::auth::Login { email: "a@b.c".into(), token: "t".into(), source: crate::auth::Source::Env };
        let client = std::sync::Arc::new(crate::jira::Client::new(base, &login));
        let mut app = App::for_test(cfg, Some(client));
        app.screen = super::super::Screen::Grooming;
        app.grooming.mode = GroomMode::Session;
        app.store.absorb(
            ListId::TeamOpen,
            vec![
                session_issue("CI-5", "Ready for Work"),
                session_issue("CI-1", "Planning"),
                session_issue("CI-6", "Ready for Work"),
                session_issue("CI-2", "Planning"),
            ],
        );
        app
    }

    fn transitions_json() -> serde_json::Value {
        serde_json::json!({"transitions": [
            {"id": "31", "name": "Ready", "to": {"name": "Ready for Work"}},
            {"id": "41", "name": "Bounce", "to": {"name": "New"}},
            {"id": "51", "name": "Park", "to": {"name": "Backlog"}},
        ]})
    }

    /// The writes the stub saw (POST and PUT), in order, as (method, path, body).
    fn writes(rx: &std::sync::mpsc::Receiver<crate::jira::tests::Recorded>) -> Vec<(String, String, serde_json::Value)> {
        rx.try_iter()
            .filter(|r| r.method == "POST" || r.method == "PUT")
            .filter(|r| !r.path.starts_with("/rest/api/3/search"))
            .map(|r| (r.method.clone(), r.path.clone(), serde_json::from_str(&r.body).unwrap_or_default()))
            .collect()
    }

    #[test]
    fn accept_transitions_to_ready_then_ranks_at_the_ghost() {
        let _g = crate::jira::tests::WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (base, rx) = crate::jira::tests::stub_server(vec![
            ("GET", "/rest/api/3/issue/CI-1/transitions", serde_json::json!({"transitions":[{"id":"31","name":"Ready","to":{"name":"Ready for Work"}}]})),
            ("POST", "/rest/api/3/issue/CI-1/transitions", serde_json::json!({})),
            ("PUT", "/rest/agile/1.0/issue/rank", serde_json::json!({})),
        ]);
        let mut app = session_app(&base);
        app.grooming.placement = Some(0);
        app.actions.push(super::super::Action::Accept);
        app.drain();
        assert_eq!(app.grooming.deciding.as_deref(), Some("CI-1"), "in flight");
        drain_until(&mut app, |a| a.grooming.deciding.is_none());
        let w = writes(&rx);
        assert_eq!(
            w.iter().map(|(m, p, _)| (m.as_str(), p.as_str())).collect::<Vec<_>>(),
            [("POST", "/rest/api/3/issue/CI-1/transitions"), ("PUT", "/rest/agile/1.0/issue/rank")]
        );
        assert_eq!(w[0].2, serde_json::json!({"transition": {"id": "31"}}));
        assert_eq!(w[1].2["issues"], serde_json::json!(["CI-1"]));
        assert_eq!(w[1].2["rankBeforeIssue"], "CI-5", "before the first Ready ticket");
        assert!(app.grooming.placement.is_none());
        assert_eq!(app.store.get("CI-1").map(|i| i.status.name.as_str()), Some("Ready for Work"), "moved at once");
    }

    #[test]
    fn an_accept_whose_rank_fails_still_moves_the_ticket() {
        let _g = crate::jira::tests::WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // The transition succeeds; the rank call has no route (404).
        let (base, rx) = crate::jira::tests::stub_server(vec![
            ("GET", "/rest/api/3/issue/CI-1/transitions", transitions_json()),
            ("POST", "/rest/api/3/issue/CI-1/transitions", serde_json::json!({})),
        ]);
        let mut app = session_app(&base);
        app.grooming.placement = Some(0);
        app.actions.push(super::super::Action::Accept);
        drain_until(&mut app, |a| a.grooming.deciding.is_none());
        let w = writes(&rx);
        assert_eq!(w.iter().filter(|(_, p, _)| p.ends_with("/transitions")).count(), 1, "{w:?}");
        assert_eq!(app.store.get("CI-1").map(|i| i.status.name.as_str()), Some("Ready for Work"), "the move happened");
        assert!(app.grooming.placement.is_none());
        let (text, kind) = app.toast.as_ref().map(|t| (t.text.clone(), t.kind)).expect("a toast");
        assert!(text.starts_with("Moved CI-1 to Ready for Work, but ranking failed: "), "{text}");
        assert_eq!(kind, ToastKind::Error);
        assert_ne!(app.session_focus().map(|i| i.key.as_str()), Some("CI-1"), "it left the Planning list");
    }

    #[test]
    fn accept_at_the_end_ranks_after_the_last_ready_ticket() {
        let _g = crate::jira::tests::WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (base, rx) = crate::jira::tests::stub_server(vec![
            ("GET", "/rest/api/3/issue/CI-1/transitions", transitions_json()),
            ("POST", "/rest/api/3/issue/CI-1/transitions", serde_json::json!({})),
            ("PUT", "/rest/agile/1.0/issue/rank", serde_json::json!({})),
        ]);
        let mut app = session_app(&base);
        app.actions.push(super::super::Action::StartAccept);
        app.drain();
        assert_eq!(app.grooming.placement, Some(2), "the ghost starts under the queue");
        app.actions.push(super::super::Action::PlacementMove(1));
        app.drain();
        assert_eq!(app.grooming.placement, Some(2), "clamped at the end");
        app.actions.push(super::super::Action::Accept);
        drain_until(&mut app, |a| a.grooming.deciding.is_none());
        let w = writes(&rx);
        assert_eq!(w.len(), 2, "one transition and one rank: {w:?}");
        assert_eq!(w[1].2["rankAfterIssue"], "CI-6");
    }

    #[test]
    fn bounce_posts_the_comment_then_moves_to_new() {
        let _g = crate::jira::tests::WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (base, rx) = crate::jira::tests::stub_server(vec![
            ("POST", "/rest/api/3/issue/CI-1/comment", serde_json::json!({})),
            ("GET", "/rest/api/3/issue/CI-1/transitions", transitions_json()),
            ("POST", "/rest/api/3/issue/CI-1/transitions", serde_json::json!({})),
        ]);
        let mut app = session_app(&base);
        app.actions.push(super::super::Action::StartBounce);
        app.drain();
        assert_eq!(app.grooming.bounce_text.as_deref(), Some(""));
        app.grooming.bounce_text = Some("Which clusters?".into());
        app.actions.push(super::super::Action::Bounce);
        drain_until(&mut app, |a| a.grooming.deciding.is_none());
        let w = writes(&rx);
        assert_eq!(
            w.iter().map(|(m, p, _)| (m.as_str(), p.as_str())).collect::<Vec<_>>(),
            [("POST", "/rest/api/3/issue/CI-1/comment"), ("POST", "/rest/api/3/issue/CI-1/transitions")]
        );
        assert!(w[0].2.to_string().contains("Which clusters?"));
        assert_eq!(w[1].2, serde_json::json!({"transition": {"id": "41"}}));
        assert!(app.grooming.bounce_text.is_none(), "the box closes on success");
    }

    #[test]
    fn bounce_needs_a_comment() {
        let _g = crate::jira::tests::WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (base, rx) = crate::jira::tests::stub_server(vec![]);
        let mut app = session_app(&base);
        for text in ["  ", "a\nb\nc"] {
            app.grooming.bounce_text = Some(text.into());
            app.actions.push(super::super::Action::Bounce);
            app.drain();
            assert!(app.grooming.deciding.is_none());
            assert_eq!(app.grooming.bounce_text.as_deref(), Some(text), "the box stays open");
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert!(rx.try_iter().next().is_none(), "nothing sent");
    }

    #[test]
    fn a_failed_bounce_keeps_the_text() {
        let _g = crate::jira::tests::WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // The comment has no route (404): the transition is never tried.
        let (base, rx) = crate::jira::tests::stub_server(vec![("GET", "/rest/api/3/issue/CI-1/transitions", transitions_json())]);
        let mut app = session_app(&base);
        app.grooming.bounce_text = Some("Which clusters?".into());
        app.actions.push(super::super::Action::Bounce);
        drain_until(&mut app, |a| a.grooming.deciding.is_none());
        assert_eq!(app.grooming.bounce_text.as_deref(), Some("Which clusters?"));
        assert!(!writes(&rx).iter().any(|(_, p, _)| p.ends_with("/transitions")));
        assert_eq!(app.store.get("CI-1").map(|i| i.status.name.as_str()), Some("Planning"));
    }

    #[test]
    fn a_bounce_whose_move_fails_keeps_the_posted_comment_and_closes_the_box() {
        let _g = crate::jira::tests::WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // The comment posts; no transition leads to New from here.
        let (base, rx) = crate::jira::tests::stub_server(vec![
            ("POST", "/rest/api/3/issue/CI-1/comment", serde_json::json!({})),
            ("GET", "/rest/api/3/issue/CI-1/transitions", serde_json::json!({"transitions": []})),
        ]);
        let mut app = session_app(&base);
        app.grooming.bounce_text = Some("Which clusters?".into());
        app.actions.push(super::super::Action::Bounce);
        drain_until(&mut app, |a| a.grooming.deciding.is_none());
        let w = writes(&rx);
        assert_eq!(w.iter().filter(|(_, p, _)| p.ends_with("/comment")).count(), 1, "{w:?}");
        assert!(app.grooming.bounce_text.is_none(), "the comment is on the ticket: ⌘Enter again must not post it twice");
        let toast = app.toast.as_ref().map(|t| t.text.clone()).unwrap_or_default();
        assert!(toast.contains("Comment posted") && toast.contains("move to New failed"), "{toast}");
        assert_eq!(app.store.get("CI-1").map(|i| i.status.name.as_str()), Some("Planning"), "still in Planning");
        // Nothing more is sent: the box is gone, so ⌘Enter has nothing to post.
        app.actions.push(super::super::Action::Bounce);
        app.drain();
        assert!(app.grooming.deciding.is_none());
    }

    #[test]
    fn park_is_one_transition_to_backlog() {
        let _g = crate::jira::tests::WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (base, rx) = crate::jira::tests::stub_server(vec![
            ("GET", "/rest/api/3/issue/CI-1/transitions", transitions_json()),
            ("POST", "/rest/api/3/issue/CI-1/transitions", serde_json::json!({})),
        ]);
        let mut app = session_app(&base);
        app.actions.push(super::super::Action::Park);
        drain_until(&mut app, |a| a.grooming.deciding.is_none());
        let w = writes(&rx);
        assert_eq!(w.len(), 1, "{w:?}");
        assert_eq!(w[0].2, serde_json::json!({"transition": {"id": "51"}}));
        assert_eq!(app.grooming.cursor, 0, "the next candidate takes the focus");
        assert_eq!(app.session_focus().map(|i| i.key.clone()).as_deref(), Some("CI-2"));
    }

    #[test]
    fn a_missing_transition_says_so() {
        let _g = crate::jira::tests::WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (base, rx) = crate::jira::tests::stub_server(vec![("GET", "/rest/api/3/issue/CI-1/transitions", serde_json::json!({"transitions": []}))]);
        let mut app = session_app(&base);
        app.actions.push(super::super::Action::Park);
        drain_until(&mut app, |a| a.grooming.deciding.is_none());
        assert!(writes(&rx).is_empty());
        assert!(app.toast.as_ref().is_some_and(|t| t.text.contains("no transition to Backlog")), "{:?}", app.toast.as_ref().map(|t| &t.text));
    }

    #[test]
    fn read_only_session_writes_send_nothing() {
        let _g = crate::jira::tests::WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _ro = crate::jira::tests::ReadOnlyGuard::new();
        let (base, rx) = crate::jira::tests::stub_server(vec![("GET", "/rest/api/3/issue/CI-1/transitions", transitions_json())]);
        let mut app = session_app(&base);
        app.actions.push(super::super::Action::Park);
        drain_until(&mut app, |a| a.grooming.deciding.is_none());
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert!(rx.try_iter().next().is_none(), "not even the transitions read");
        assert!(app.toast.as_ref().is_some_and(|t| t.text.contains("Read-only")));
    }

    #[test]
    fn entering_grooming_starts_the_timer() {
        let mut app = App::for_test(toml::from_str("site = \"https://x.atlassian.net\"\nemail = \"a@b.c\"\n").unwrap(), None);
        assert!(app.grooming.focused_since.is_none());
        app.apply(super::super::Action::SetScreen(super::super::Screen::Grooming), &egui::Context::default());
        assert!(app.grooming.focused_since.is_some());
    }

    #[test]
    fn code_keys_toggle_without_moving() {
        let mut app = App::for_test(toml::from_str("site = \"https://x.atlassian.net\"\nemail = \"a@b.c\"\n").unwrap(), None);
        app.store.absorb(ListId::TeamOpen, vec![team_new("CI-1", None), team_new("CI-2", None)]);
        let ctx = egui::Context::default();
        app.apply(super::super::Action::ToggleCode(grooming::Code::UnclearWhat), &ctx);
        app.apply(super::super::Action::ToggleCode(grooming::Code::Empty), &ctx);
        assert_eq!(app.grooming.verdicts.get("CI-1"), Some(&Verdict::NeedsDetail(vec![grooming::Code::UnclearWhat, grooming::Code::Empty])));
        assert_eq!(app.grooming.cursor, 0, "several codes can be set on one ticket");
        app.grooming.cursor = 2;
        app.apply(super::super::Action::ToggleCode(grooming::Code::Empty), &ctx);
        assert_eq!(app.grooming.verdicts.len(), 1, "the summary row takes no verdict");
        app.apply(super::super::Action::OpenSummary, &ctx);
        assert!(app.grooming.summary_open);
    }
}
