//! The Standup screen's state, and how its review runs and ticket adds change the App.

use super::msg::{Msg, done_toast};
use super::{App, Screen, ToastKind};
use crate::jira;
use crate::standup::{self, Review, Saved, Target};
use std::collections::{HashMap, HashSet};
use std::time::Instant;

/// What a running review is doing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StandupPhase {
    /// Reading the user's Jira activity in the window.
    Jira,
    /// `claude` is reading GitHub and Slack and writing the answer.
    Claude,
}

/// Runs the review for a prompt; `standup::run_review` outside tests.
pub type ReviewRunner = fn(&crate::config::Standup, &str) -> Result<Review, String>;

/// Everything the Standup screen keeps between frames.
#[derive(Default)]
pub struct StandupState {
    /// The last review, the dismissals and the added tickets, kept on disk between launches.
    pub saved: Saved,
    /// Set while a review runs.
    pub phase: Option<StandupPhase>,
    /// When the running review started.
    pub started: Option<Instant>,
    /// How many issues the running review found in Jira.
    pub activity_count: Option<usize>,
    /// Why the last review failed; the last good review stays on screen.
    pub error: Option<String>,
    /// The focused card: an index into `saved.visible()`.
    pub cursor: usize,
    /// Edited titles, by dismiss key.
    pub titles: HashMap<String, String>,
    /// Dismiss keys with an add in flight.
    pub adding: HashSet<String>,
    /// The focused card's title field takes focus on its next frame (`e`).
    pub focus_title: bool,
}

impl App {
    /// Each frame: on a standup day the review starts by itself, once.
    pub(super) fn standup_auto_run(&mut self) {
        let today = chrono::Local::now().date_naive();
        if self.standup.phase.is_none() && standup::should_auto_run(today, self.standup.saved.last_run) {
            self.standup_run();
        }
    }

    /// Starts a review: the Jira activity, then `claude`, on a background thread. A no-op while one runs.
    pub(super) fn standup_run(&mut self) {
        if self.standup.phase.is_some() || self.client.is_none() {
            return;
        }
        let today = chrono::Local::now().date_naive();
        let since = standup::window_start(today);
        self.standup.saved.last_run = Some(today);
        self.save_standup();
        self.standup.phase = Some(StandupPhase::Jira);
        self.standup.started = Some(Instant::now());
        self.standup.activity_count = None;
        self.standup.error = None;
        let (me, cfg, runner) = (self.me.clone(), self.cfg.clone(), self.review_runner);
        self.spawn(move |c, tx| {
            let me = me.unwrap_or_else(|| c.myself().unwrap_or_default());
            let activity = match standup::fetch_activity(c, since, &me.account_id) {
                Ok(a) => a,
                Err(e) => {
                    let _ = tx.send(Msg::StandupActivity(Err(e)));
                    return;
                }
            };
            let _ = tx.send(Msg::StandupActivity(Ok(activity.len())));
            let name = if me.display_name.is_empty() { cfg.email.clone() } else { me.display_name.clone() };
            let prompt = standup::prompt(&name, today, &standup::render_activity(&activity, &cfg.site));
            let _ = tx.send(Msg::StandupReview(runner(&cfg.standup, &prompt)));
        });
    }

    pub(super) fn standup_activity_loaded(&mut self, result: jira::Result<usize>) {
        if self.standup.phase.is_none() {
            return;
        }
        match result {
            Ok(n) => {
                self.standup.phase = Some(StandupPhase::Claude);
                self.standup.activity_count = Some(n);
            }
            Err(jira::Error::Unauthorized(m)) => {
                self.standup.phase = None;
                self.client = None;
                self.token_error = Some(m);
            }
            Err(e) => {
                self.standup.phase = None;
                self.standup.error = Some(format!("Couldn't read your Jira activity: {e}"));
            }
        }
    }

    pub(super) fn standup_reviewed(&mut self, result: Result<Review, String>) {
        self.standup.phase = None;
        match result {
            Ok(review) => {
                self.standup.saved.review = Some(review);
                self.standup.saved.ran_on = self.standup.saved.last_run;
                self.standup.saved.ran_at = Some(chrono::Utc::now().timestamp());
                self.standup.cursor = 0;
                self.standup.titles.clear();
                self.save_standup();
                if self.screen != Screen::Standup {
                    self.toast_kind("Standup review ready (⌘6)", ToastKind::Info);
                }
            }
            Err(e) => self.standup.error = Some(e),
        }
    }

    /// The visible cards' dismiss keys, in order.
    pub fn standup_keys(&self) -> Vec<String> {
        self.standup.saved.visible().into_iter().map(standup::dismiss_key).collect()
    }

    /// The focused card's index and dismiss key, the cursor clamped onto the cards.
    fn standup_focused(&self) -> Option<(usize, String)> {
        let keys = self.standup_keys();
        let i = self.standup.cursor.min(keys.len().checked_sub(1)?);
        Some((i, keys[i].clone()))
    }

    pub(super) fn standup_move(&mut self, d: i32) {
        let last = self.standup_keys().len().saturating_sub(1);
        self.standup.cursor = self.standup.cursor.min(last).saturating_add_signed(d as isize).min(last);
    }

    pub(super) fn standup_focus(&mut self, i: usize) {
        self.standup.cursor = i.min(self.standup_keys().len().saturating_sub(1));
    }

    /// Hides the focused card for good: its key is kept, so a rerun's same suggestion stays hidden.
    pub(super) fn standup_dismiss(&mut self) {
        let Some((_, key)) = self.standup_focused() else { return };
        self.standup.titles.remove(&key);
        self.standup.saved.dismissed.insert(key);
        self.save_standup();
        self.standup_move(0);
    }

    /// Creates the focused card's ticket (its edited title) in the scope project with the team label, then moves it
    /// into `target`'s sprint, on the write worker.
    pub(super) fn standup_add(&mut self, target: Target) {
        let Some((i, key)) = self.standup_focused() else { return };
        if let Some(issue) = self.standup.saved.added.get(&key) {
            self.toast_kind(format!("Already added as {issue}"), ToastKind::Info);
            return;
        }
        if self.standup.adding.contains(&key) || self.client.is_none() {
            return;
        }
        if let Err(e) = jira::check_writable() {
            let (text, kind) = done_toast("Create", &Err(e));
            self.toast_kind(text, kind);
            return;
        }
        let Some(s) = self.standup.saved.visible().get(i).map(|s| (*s).clone()) else { return };
        let title = self.standup.titles.get(&key).cloned().unwrap_or_else(|| s.title.clone());
        if title.trim().is_empty() {
            self.toast_kind("Give the ticket a title first", ToastKind::Info);
            return;
        }
        let fields = standup::create_fields(&self.cfg.scope.project, &self.cfg.scope.team_label, &title, &s);
        let board = self.cfg.scope.board_id;
        self.standup.adding.insert(key.clone());
        self.spawn_write(move |c, tx| {
            let _ = tx.send(Msg::StandupAdded { dkey: key, result: standup::add_ticket(c, fields, board, target) });
        });
    }

    pub(super) fn standup_added(&mut self, dkey: String, result: jira::Result<(String, Result<Option<String>, String>)>) {
        self.standup.adding.remove(&dkey);
        match result {
            Ok((issue, placed)) => {
                let (text, kind) = match &placed {
                    Ok(Some(sprint)) => (format!("Created {issue} in {sprint}"), ToastKind::Success),
                    Ok(None) => (format!("Created {issue} in the backlog"), ToastKind::Success),
                    Err(e) => (format!("Created {issue}, but couldn't move it to the sprint: {e}"), ToastKind::Error),
                };
                self.standup.saved.added.insert(dkey.clone(), issue);
                self.standup.titles.remove(&dkey);
                self.save_standup();
                self.toast_kind(text, kind);
                if self.client.is_some() {
                    self.refresh_all();
                }
            }
            Err(e) => {
                let (text, kind) = done_toast("Create", &Err(e));
                self.toast_kind(text, kind);
            }
        }
    }

    /// The focused card's source link, if it has one.
    pub fn standup_source_url(&self) -> Option<String> {
        let (i, _) = self.standup_focused()?;
        let url = self.standup.saved.visible().get(i)?.source_url.trim().to_string();
        (!url.is_empty()).then_some(url)
    }

    /// The standup summary as text, when there is a review.
    pub fn standup_text(&self) -> Option<String> {
        let saved = &self.standup.saved;
        let review = saved.review.as_ref()?;
        let since = standup::window_start(saved.ran_on.unwrap_or_else(|| chrono::Local::now().date_naive()));
        Some(standup::summary_text(&review.summary, since))
    }

    fn save_standup(&self) {
        if self.persist_cache {
            crate::cache::store(standup::CACHE_KEY, &self.standup.saved);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Action;
    use crate::standup::Suggestion;

    fn cfg() -> crate::config::Config {
        toml::from_str("site = \"https://x.atlassian.net\"\nemail = \"a@b.c\"\n").unwrap()
    }

    fn suggestion(title: &str, url: &str) -> Suggestion {
        Suggestion { title: title.into(), source_url: url.into(), issue_type: "Task".into(), ..Default::default() }
    }

    fn with_review(app: &mut App) {
        let review = Review { suggestions: vec![suggestion("A", "https://a"), suggestion("B", "https://b"), suggestion("C", "")], ..Default::default() };
        app.standup.saved.review = Some(review);
    }

    fn client(base: &str) -> std::sync::Arc<jira::Client> {
        let login = crate::auth::Login { email: "a@b.c".into(), token: "t".into(), source: crate::auth::Source::Env };
        std::sync::Arc::new(jira::Client::new(base, &login))
    }

    /// Waits for the background threads' messages and handles them, `n` of them.
    fn pump(app: &mut App, n: usize) {
        for _ in 0..n {
            let m = app.rx.recv_timeout(std::time::Duration::from_secs(10)).expect("a message");
            app.handle(m);
        }
    }

    fn fake_review(_: &crate::config::Standup, prompt: &str) -> Result<Review, String> {
        assert!(prompt.contains("CI-1"), "the Jira activity is in the prompt");
        Ok(Review { suggestions: vec![suggestion("Ticket the logger fix", "https://github.com/o/r/pull/1")], ..Default::default() })
    }

    fn failing_review(_: &crate::config::Standup, _: &str) -> Result<Review, String> {
        Err("claude took longer than 900 s and was stopped".into())
    }

    #[test]
    fn a_run_reads_jira_then_claude_and_keeps_the_review() {
        let page: serde_json::Value = serde_json::from_str(&std::fs::read_to_string("tests/fixtures/standup_activity.json").unwrap()).unwrap();
        let (base, rx) = jira::tests::stub_server(vec![("POST", "/rest/api/3/search/jql", page)]);
        let mut app = App::for_test(cfg(), Some(client(&base)));
        app.me = Some(jira::User { account_id: "me-1".into(), display_name: "Me".into() });
        app.review_runner = fake_review;
        app.apply(Action::StandupRun, &egui::Context::default());
        assert_eq!(app.standup.phase, Some(StandupPhase::Jira));
        assert_eq!(app.standup.saved.last_run, Some(chrono::Local::now().date_naive()), "today counts as run, so it does not start again");
        app.standup_auto_run();
        app.apply(Action::StandupRun, &egui::Context::default());
        pump(&mut app, 1);
        assert_eq!((app.standup.phase, app.standup.activity_count), (Some(StandupPhase::Claude), Some(2)));
        pump(&mut app, 1);
        assert_eq!(app.standup.phase, None);
        assert_eq!(app.standup_keys(), ["url:https://github.com/o/r/pull/1"]);
        assert!(app.toast.as_ref().is_some_and(|t| t.text.contains("⌘6")), "off the screen, a toast says it is ready");
        assert_eq!(rx.try_iter().count(), 1, "one search, no second run started meanwhile");
        assert!(app.rx.try_recv().is_err());
    }

    #[test]
    fn a_failed_run_keeps_the_last_review() {
        let (base, _rx) = jira::tests::stub_server(vec![("POST", "/rest/api/3/search/jql", serde_json::json!({ "issues": [] }))]);
        let mut app = App::for_test(cfg(), Some(client(&base)));
        app.me = Some(jira::User::default());
        app.review_runner = failing_review;
        with_review(&mut app);
        app.apply(Action::StandupRun, &egui::Context::default());
        pump(&mut app, 2);
        assert_eq!(app.standup.phase, None);
        assert!(app.standup.error.as_deref().is_some_and(|e| e.contains("was stopped")));
        assert_eq!(app.standup_keys().len(), 3);
    }

    #[test]
    fn jira_failures_end_the_run() {
        let mut app = App::for_test(cfg(), None);
        app.standup.phase = Some(StandupPhase::Jira);
        app.handle(Msg::StandupActivity(Err(jira::Error::Other("Jira returned 400: bad".into()))));
        assert_eq!(app.standup.phase, None);
        assert!(app.standup.error.as_deref().unwrap().starts_with("Couldn't read your Jira activity"));
        app.standup.phase = Some(StandupPhase::Jira);
        app.handle(Msg::StandupActivity(Err(jira::Error::Unauthorized("401".into()))));
        assert_eq!((app.standup.phase, app.token_error.as_deref()), (None, Some("401")));
    }

    #[test]
    fn no_client_no_run() {
        let mut app = App::for_test(cfg(), None);
        app.standup_run();
        assert_eq!(app.standup.phase, None, "nothing would ever end it");
        assert_eq!(app.standup.saved.last_run, None);
    }

    #[test]
    fn move_and_dismiss_clamp_and_stick() {
        let mut app = App::for_test(cfg(), None);
        with_review(&mut app);
        let ctx = egui::Context::default();
        app.apply(Action::StandupMove(9), &ctx);
        assert_eq!(app.standup.cursor, 2);
        app.apply(Action::StandupDismiss, &ctx);
        assert_eq!(app.standup.cursor, 1, "clamped onto the last card left");
        app.apply(Action::StandupFocus(0), &ctx);
        app.apply(Action::StandupDismiss, &ctx);
        assert_eq!(app.standup_keys(), ["url:https://b"]);
        // A rerun brings the same suggestions back; the dismissed ones stay hidden.
        with_review(&mut app);
        assert_eq!(app.standup_keys(), ["url:https://b"]);
        app.apply(Action::StandupDismiss, &ctx);
        app.apply(Action::StandupDismiss, &ctx);
        assert!(app.standup_keys().is_empty());
        app.apply(Action::StandupMove(-1), &ctx);
        assert_eq!(app.standup.cursor, 0);
    }

    #[test]
    fn add_creates_with_the_edited_title_and_remembers_it() {
        let _g = jira::tests::WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let sprints = serde_json::json!({ "values": [{ "id": 7, "name": "Sprint 92", "state": "active" }] });
        let (base, rx) = jira::tests::stub_server(vec![
            ("POST", "/rest/api/3/issue", serde_json::json!({ "key": "CI-42" })),
            ("GET", "/rest/agile/1.0/board/101/sprint", sprints),
            ("POST", "/rest/agile/1.0/sprint/7/issue", serde_json::json!({})),
            ("POST", "/rest/api/3/search/jql", serde_json::json!({ "issues": [] })),
        ]);
        let mut app = App::for_test(cfg(), Some(client(&base)));
        with_review(&mut app);
        app.standup.titles.insert("url:https://a".into(), "Edited title".into());
        let ctx = egui::Context::default();
        app.apply(Action::StandupAdd(Target::CurrentSprint), &ctx);
        assert!(app.standup.adding.contains("url:https://a"));
        app.apply(Action::StandupAdd(Target::CurrentSprint), &ctx);
        pump(&mut app, 1);
        let create: serde_json::Value = serde_json::from_str(&rx.recv().unwrap().body).unwrap();
        assert_eq!(create["fields"]["summary"], "Edited title");
        assert_eq!(create["fields"]["project"]["key"], "CI");
        assert_eq!(create["fields"]["labels"], serde_json::json!(["PLAT"]));
        assert_eq!(app.standup.saved.added.get("url:https://a").map(String::as_str), Some("CI-42"));
        assert_eq!(app.toast.as_ref().map(|t| t.text.as_str()), Some("Created CI-42 in Sprint 92"));
        assert!(app.standup.adding.is_empty());
        assert_eq!(app.standup_keys().len(), 3, "an added card stays, marked added");
        app.apply(Action::StandupAdd(Target::Backlog), &ctx);
        assert_eq!(app.toast.as_ref().map(|t| t.text.as_str()), Some("Already added as CI-42"));
        assert!(app.standup.adding.is_empty(), "not created twice");
    }

    #[test]
    fn add_in_read_only_mode_sends_nothing() {
        let _g = jira::tests::WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _ro = jira::tests::ReadOnlyGuard::new();
        let (base, rx) = jira::tests::stub_server(vec![]);
        let mut app = App::for_test(cfg(), Some(client(&base)));
        with_review(&mut app);
        app.apply(Action::StandupAdd(Target::Backlog), &egui::Context::default());
        assert!(app.standup.adding.is_empty());
        assert_eq!(app.toast.as_ref().map(|t| (t.text.as_str(), t.kind)), Some(("Read-only mode: create not sent to Jira", ToastKind::Info)));
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn an_empty_title_is_not_created() {
        let (base, rx) = jira::tests::stub_server(vec![]);
        let mut app = App::for_test(cfg(), Some(client(&base)));
        with_review(&mut app);
        app.standup.titles.insert("url:https://a".into(), "  ".into());
        app.apply(Action::StandupAdd(Target::Backlog), &egui::Context::default());
        assert!(app.standup.adding.is_empty());
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn a_failed_sprint_move_still_records_the_ticket() {
        let mut app = App::for_test(cfg(), None);
        with_review(&mut app);
        app.standup.adding.insert("url:https://b".into());
        app.handle(Msg::StandupAdded { dkey: "url:https://b".into(), result: Ok(("CI-9".into(), Err("the board has no future sprint".into()))) });
        assert_eq!(app.standup.saved.added.get("url:https://b").map(String::as_str), Some("CI-9"));
        assert_eq!(app.toast.as_ref().map(|t| t.kind), Some(ToastKind::Error));
        app.handle(Msg::StandupAdded { dkey: "url:https://a".into(), result: Err(jira::Error::Other("Jira returned 400: issuetype".into())) });
        assert!(!app.standup.saved.added.contains_key("url:https://a"));
    }

    #[test]
    fn text_and_source_of_the_focused_card() {
        let mut app = App::for_test(cfg(), None);
        assert!(app.standup_text().is_none());
        with_review(&mut app);
        app.standup.saved.ran_on = chrono::NaiveDate::from_ymd_opt(2026, 11, 2);
        app.standup.saved.review.as_mut().unwrap().summary.done = vec!["Shipped".into()];
        assert!(app.standup_text().unwrap().starts_with("Since Fri Oct 30\n- Shipped"));
        assert_eq!(app.standup_source_url().as_deref(), Some("https://a"));
        app.standup.cursor = 2;
        assert_eq!(app.standup_source_url(), None, "C has no source");
    }
}
