//! The Grooming screen's state changes: the report overlay, pre-check verdicts, and the session rail.

use super::{App, GroomMode};
use crate::grooming::{self, Candidate, EpicQueue, Verdict};
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
    /// The pre-check rows: the team's `New` tickets in rank order.
    pub fn pre_check_candidates(&self) -> Vec<Candidate<'_>> {
        let g = &self.cfg.grooming;
        grooming::new_candidates(&self.store.list(ListId::TeamOpen), &self.cfg.scope.team_label, &g.status_new, chrono::Local::now().date_naive())
    }

    /// The session rail: the team's tickets grouped by epic.
    pub fn session_queues(&self) -> Vec<EpicQueue<'_>> {
        grooming::epic_queues(&self.store.list(ListId::TeamOpen), &self.cfg.scope.team_label, &self.cfg.grooming)
    }

    /// The team's tickets in `Ready for Work`, for the ceiling pill.
    pub fn ready_total(&self) -> usize {
        let (label, ready) = (&self.cfg.scope.team_label, &self.cfg.grooming.status_ready);
        self.store.list(ListId::TeamOpen).iter().filter(|i| i.labels.iter().any(|l| l == label) && &i.status.name == ready).count()
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
}
