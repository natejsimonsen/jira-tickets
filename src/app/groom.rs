//! The Grooming screen's state changes: the report overlay, pre-check verdicts, and the session rail.

use super::msg::{Msg, done_toast};
use super::{App, GroomMode, PromoteRun, ToastKind};
use crate::grooming::{self, Candidate, Code, EpicQueue, Promotion, RunRow, Verdict};
use crate::jira::{self, Issue};
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
        if self.client.is_some() {
            self.refresh_all();
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
