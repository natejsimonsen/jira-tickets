//! The Standup screen's state, and how its review runs and ticket adds change the App.

use super::msg::{Msg, done_toast};
use super::{App, Screen, ToastKind};
use crate::jira;
use crate::standup::{self, AddError, Added, Placed, Review, Saved, Target};
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
    /// The focused card, in reading order: the ticket cards (`saved.tickets.all()`), then the suggestions
    /// (`saved.visible()`).
    pub cursor: usize,
    /// The ticket cards' read is in flight.
    pub tickets_loading: bool,
    /// A ticket read asked for while one was in flight: runs when it lands.
    pub(super) tickets_reload: bool,
    /// Why the last ticket read failed; the cached cards stay.
    pub tickets_error: Option<String>,
    /// Edited titles, by dismiss key.
    pub titles: HashMap<String, String>,
    /// Dismiss keys with an add in flight.
    pub adding: HashSet<String>,
    /// The focused card's title field takes focus on its next frame (`e`).
    pub focus_title: bool,
    /// The project's issue types are being read (`App::load_issue_types`).
    pub types_loading: bool,
    /// The project's issue types were read this launch, so opening the screen does not read them again.
    pub(super) types_fresh: bool,
    /// A stopped run's thread has not ended yet; a new run waits for it, so its late answer is never taken.
    pub stopping: bool,
    /// The running review's stop flag (`standup::set_run_stop`).
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl App {
    /// Each frame: on a standup day the review starts by itself, once.
    pub(super) fn standup_auto_run(&mut self) {
        let today = (self.today)();
        if self.standup.phase.is_none() && standup::should_auto_run(today, self.standup.saved.last_run) {
            self.standup_run();
        }
    }

    /// Starts a review: the Jira activity, then `claude`, on a background thread. A no-op while one runs.
    pub(super) fn standup_run(&mut self) {
        if self.standup.phase.is_some() || self.standup.stopping || self.client.is_none() {
            return;
        }
        // Each run has its own stop flag, so a stopped run's request never reaches this one.
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        self.standup.stop = stop.clone();
        let today = (self.today)();
        let since = standup::window_start(today);
        self.standup.saved.last_run = Some(today);
        self.save_standup();
        self.standup.phase = Some(StandupPhase::Jira);
        self.standup.started = Some(Instant::now());
        self.standup.activity_count = None;
        self.standup.error = None;
        let (me, cfg, runner) = (self.me.clone(), self.cfg.clone(), self.review_runner);
        self.spawn(move |c, tx| {
            standup::set_run_stop(stop);
            let me = me.unwrap_or_else(|| c.myself().unwrap_or_default());
            let activity = standup::fetch_activity(c, since, &me.account_id);
            if standup::stop_requested() {
                let _ = tx.send(Msg::StandupReview(Err(standup::STOPPED.into())));
                return;
            }
            let activity = match activity {
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

    /// Stops the running review: its `claude` is killed (`standup::set_run_stop`), the last good review stays,
    /// and the run's late answer is dropped. A no-op when nothing runs.
    pub(super) fn standup_stop(&mut self) {
        if self.standup.phase.is_none() {
            return;
        }
        self.standup.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        self.standup.phase = None;
        self.standup.started = None;
        self.standup.activity_count = None;
        self.standup.stopping = true;
        self.toast_kind("Stopped the review", ToastKind::Info);
    }

    pub(super) fn standup_activity_loaded(&mut self, result: jira::Result<usize>) {
        if self.standup.phase.is_none() {
            // A stopped run whose Jira read failed ends here; one that read Jira still answers with a review.
            if result.is_err() {
                self.standup.stopping = false;
            }
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
        if std::mem::take(&mut self.standup.stopping) {
            // The stopped run's answer (its `claude` was killed): not this screen's any more.
            return;
        }
        self.standup.phase = None;
        match result {
            Ok(review) => {
                let window = standup::tickets_window(&self.standup.saved, (self.today)());
                self.standup.saved.review = Some(review);
                self.standup.saved.ran_on = self.standup.saved.last_run;
                self.standup.saved.ran_at = Some(chrono::Utc::now().timestamp());
                self.standup.cursor = 0;
                self.standup.titles.clear();
                self.save_standup();
                // The cards follow the review's window: read them again when it moved (a first read comes with the
                // launch refresh).
                if self.standup.saved.tickets.since.is_some() && standup::tickets_window(&self.standup.saved, (self.today)()) != window {
                    self.load_standup_tickets();
                }
                if self.screen != Screen::Standup {
                    self.toast_kind("Standup review ready (⌘6)", ToastKind::Info);
                }
            }
            Err(e) => self.standup.error = Some(e),
        }
    }

    /// The visible suggestion cards' dismiss keys, in order.
    pub fn standup_keys(&self) -> Vec<String> {
        self.standup.saved.visible().into_iter().map(standup::dismiss_key).collect()
    }

    /// How many ticket cards come before the suggestions.
    pub fn standup_ticket_count(&self) -> usize {
        let t = &self.standup.saved.tickets;
        t.done.len() + t.in_progress.len()
    }

    /// Every card's id in cursor order: "jira:KEY" for a ticket, the dismiss key for a suggestion.
    fn standup_row_ids(&self) -> Vec<String> {
        let tickets = self.standup.saved.tickets.all().map(|i| format!("jira:{}", i.key));
        tickets.chain(self.standup_keys()).collect()
    }

    /// The cursor clamped onto the cards; None with no cards.
    pub fn standup_cursor(&self) -> Option<usize> {
        Some(self.standup.cursor.min(self.standup_row_ids().len().checked_sub(1)?))
    }

    /// The focused ticket card's issue, when the cursor is on one.
    pub fn standup_focused_ticket(&self) -> Option<&jira::Issue> {
        self.standup.saved.tickets.all().nth(self.standup_cursor()?)
    }

    /// The focused suggestion card's index in `saved.visible()` and its dismiss key; None on a ticket card.
    fn standup_focused(&self) -> Option<(usize, String)> {
        let i = self.standup_cursor()?.checked_sub(self.standup_ticket_count())?;
        Some((i, self.standup_keys().get(i)?.clone()))
    }

    /// What Move to Done (`D`) moves on this screen: the focused ticket card while it is not done yet, or the focused
    /// suggestion once it was added as a ticket. `App::done_target`'s Standup arm.
    pub fn standup_done_target(&self) -> Option<String> {
        if let Some(i) = self.standup_focused_ticket() {
            return (!super::is_done_category(&i.status.category)).then(|| i.key.clone());
        }
        let (_, dkey) = self.standup_focused()?;
        self.standup.saved.added.get(&dkey).cloned()
    }

    pub(super) fn standup_move(&mut self, d: i32) {
        let last = self.standup_row_ids().len().saturating_sub(1);
        self.standup.cursor = self.standup.cursor.min(last).saturating_add_signed(d as isize).min(last);
    }

    pub(super) fn standup_focus(&mut self, i: usize) {
        self.standup.cursor = i.min(self.standup_row_ids().len().saturating_sub(1));
    }

    /// The ticket cards landed. The cursor stays on the card it was on (cards above it may come or go); the last
    /// good cards stay when the read failed.
    pub(super) fn standup_tickets_loaded(&mut self, result: jira::Result<standup::Tickets>) {
        self.standup.tickets_loading = false;
        let reload = std::mem::take(&mut self.standup.tickets_reload);
        match result {
            Ok(tickets) => {
                let focused = self.standup_cursor().and_then(|i| self.standup_row_ids().get(i).cloned());
                self.standup.saved.tickets = tickets;
                self.standup.tickets_error = None;
                if let Some(at) = focused.and_then(|id| self.standup_row_ids().iter().position(|r| *r == id)) {
                    self.standup.cursor = at;
                }
                self.save_standup();
            }
            Err(jira::Error::Unauthorized(m)) => {
                self.client = None;
                self.token_error = Some(m);
            }
            Err(e) => self.standup.tickets_error = Some(e.to_string()),
        }
        if reload && self.client.is_some() {
            self.load_standup_tickets();
        }
    }

    /// Hides the focused card for good: its key is kept, so a rerun's same suggestion stays hidden.
    pub(super) fn standup_dismiss(&mut self) {
        let Some((_, key)) = self.standup_focused() else { return };
        self.standup.titles.remove(&key);
        self.standup.saved.dismissed.insert(key);
        self.save_standup();
        self.standup_move(0);
    }

    /// The scope project's creatable issue types, as last read; None before the first read.
    pub fn standup_types(&self) -> Option<&[jira::IssueType]> {
        self.standup.saved.issue_types.as_ref().filter(|t| t.project == self.cfg.scope.project).map(|t| t.types.as_slice())
    }

    /// The type an add creates for `s` (`standup::resolve_issue_type`): None while the types are unknown or when the
    /// project has none to create.
    pub fn standup_type_name(&self, s: &standup::Suggestion) -> Option<String> {
        standup::resolve_issue_type(&s.issue_type, self.standup_types()?).map(|t| t.name.clone())
    }

    pub(super) fn issue_types_loaded(&mut self, project: String, result: jira::Result<Vec<jira::IssueType>>) {
        self.standup.types_loading = false;
        match result {
            Ok(types) => {
                self.standup.types_fresh |= project == self.cfg.scope.project;
                self.standup.saved.issue_types = Some(standup::ProjectTypes { project, types });
                self.save_standup();
            }
            Err(jira::Error::Unauthorized(m)) => {
                self.client = None;
                self.token_error = Some(m);
            }
            // The cached types stay; an add reads them itself when there are none.
            Err(_) => {}
        }
    }

    /// Creates the focused card's ticket (its edited title) in the scope project with the team label, as the
    /// project's type for it, then moves it into `target`'s sprint, on the write worker. What is known to fail is not
    /// sent: no type to create, or no such sprint on the board.
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
        let project = self.cfg.scope.project.clone();
        let types = self.standup_types().map(<[_]>::to_vec);
        if types.as_ref().is_some_and(|t| standup::resolve_issue_type(&s.issue_type, t).is_none()) {
            let (text, kind) = add_failed_toast(&project, &AddError::NoType);
            self.toast_kind(text, kind);
            return;
        }
        if let Err(why) = target_sprint(target, &self.sprints) {
            let (text, kind) = add_failed_toast(&project, &AddError::Sprint(why));
            self.toast_kind(text, kind);
            return;
        }
        let ticket = standup::NewTicket { project, label: self.cfg.scope.team_label.clone(), title, suggestion: s };
        let board = self.cfg.scope.board_id;
        self.standup.adding.insert(key.clone());
        self.spawn_write(move |c, tx| {
            let (result, fresh) = standup::add_ticket(c, &ticket, types, board, target);
            if let Some(types) = fresh {
                let _ = tx.send(Msg::IssueTypes { project: ticket.project.clone(), result: Ok(types) });
            }
            let _ = tx.send(Msg::StandupAdded { dkey: key, result });
        });
    }

    pub(super) fn standup_added(&mut self, dkey: String, result: Result<Added, AddError>) {
        self.standup.adding.remove(&dkey);
        let project = self.cfg.scope.project.clone();
        match result {
            Ok(added) => {
                let (text, kind) = added_toast(&project, &added);
                self.standup.saved.added.insert(dkey.clone(), added.key);
                self.standup.titles.remove(&dkey);
                self.save_standup();
                self.toast_kind(text, kind);
                if self.client.is_some() {
                    self.refresh_all();
                }
            }
            Err(e) => {
                let (text, kind) = add_failed_toast(&project, &e);
                self.toast_kind(text, kind);
                if let AddError::Create(jira::Error::Unauthorized(m)) | AddError::Types(jira::Error::Unauthorized(m)) = e {
                    self.client = None;
                    self.token_error = Some(m);
                }
            }
        }
    }

    /// What `o` opens: the focused ticket card's Jira page, or the focused suggestion's source link, if it has one.
    pub fn standup_source_url(&self) -> Option<String> {
        if let Some(i) = self.standup_focused_ticket() {
            return Some(standup::browse_url(&self.cfg.site, &i.key));
        }
        let (i, _) = self.standup_focused()?;
        let url = self.standup.saved.visible().get(i)?.source_url.trim().to_string();
        (!url.is_empty()).then_some(url)
    }

    /// The standup summary as text, when there is a review.
    pub fn standup_text(&self) -> Option<String> {
        let saved = &self.standup.saved;
        let review = saved.review.as_ref()?;
        let since = standup::window_start(saved.ran_on.unwrap_or_else(self.today));
        Some(standup::summary_text(&review.summary, since))
    }

    fn save_standup(&self) {
        if self.persist_cache {
            crate::cache::store(standup::CACHE_KEY, &self.standup.saved);
        }
    }
}

/// The sprint an add to `target` lands in, as far as the board's known sprints (`App::sprints`) tell: Ok(None) for
/// the backlog or while no sprint is known, Err(why) when the board has none for it.
pub fn target_sprint(target: Target, sprints: &[jira::Sprint]) -> Result<Option<String>, String> {
    if target == Target::Backlog || sprints.is_empty() {
        return Ok(None);
    }
    Ok(standup::pick_sprint(target, sprints)?.map(|s| s.name.clone()))
}

/// " (Task)", or " as a Task (no Story in CI)" when the project had no type of the asked-for name.
fn type_phrase(project: &str, added: &Added) -> String {
    let (got, wanted) = (added.issue_type.as_str(), added.wanted.trim());
    if wanted.is_empty() || got.eq_ignore_ascii_case(wanted) {
        return format!(" ({got})");
    }
    let article = if got.starts_with(['A', 'E', 'I', 'O', 'U', 'a', 'e', 'i', 'o', 'u']) { "an" } else { "a" };
    format!(" as {article} {got} (no {wanted} in {project})")
}

/// The toast for a created ticket: its key, its type, and where it is.
pub fn added_toast(project: &str, added: &Added) -> (String, ToastKind) {
    let ty = type_phrase(project, added);
    let key = &added.key;
    match &added.placed {
        Placed::Sprint(sprint) => (format!("Created {key}{ty} in {sprint}"), ToastKind::Success),
        Placed::Backlog => (format!("Created {key}{ty} in the backlog"), ToastKind::Success),
        Placed::NotMoved { sprint, why } => (format!("Created {key}{ty} in the backlog, but couldn't move it to {sprint}: {why}"), ToastKind::Error),
    }
}

/// The toast for an add that created nothing.
pub fn add_failed_toast(project: &str, e: &AddError) -> (String, ToastKind) {
    match e {
        AddError::NoType => (format!("Can't create in {project}: the project has no issue type you can create"), ToastKind::Error),
        AddError::Types(e) => (format!("Create failed: couldn't read the issue types of {project}: {}", jira::reason(e)), ToastKind::Error),
        AddError::Sprint(why) => (format!("Not created: {why}"), ToastKind::Error),
        AddError::Create(e) if jira::is_read_only(e) => done_toast("Create", &Err(e.clone())),
        AddError::Create(e) => (format!("Create failed: {}", jira::reason(e)), ToastKind::Error),
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
        // The fixture's dates are fixed, so the run's "today" is too: Monday 2 November 2026, whose window opens
        // the Friday before.
        app.today = || chrono::NaiveDate::from_ymd_opt(2026, 11, 2).unwrap();
        app.apply(Action::StandupRun, &egui::Context::default());
        assert_eq!(app.standup.phase, Some(StandupPhase::Jira));
        assert_eq!(app.standup.saved.last_run, Some((app.today)()), "today counts as run, so it does not start again");
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

    fn slow_review(_: &crate::config::Standup, _: &str) -> Result<Review, String> {
        // Like `run_review`: ends as soon as a stop is asked for.
        let t = Instant::now();
        while !standup::stop_requested() && t.elapsed() < std::time::Duration::from_secs(10) {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        Err(standup::STOPPED.into())
    }

    #[test]
    fn stop_ends_the_run_and_drops_its_answer() {
        let (base, _rx) = jira::tests::stub_server(vec![("POST", "/rest/api/3/search/jql", serde_json::json!({ "issues": [] }))]);
        let mut app = App::for_test(cfg(), Some(client(&base)));
        app.me = Some(jira::User::default());
        app.review_runner = slow_review;
        with_review(&mut app);
        app.apply(Action::StandupRun, &egui::Context::default());
        pump(&mut app, 1);
        assert_eq!(app.standup.phase, Some(StandupPhase::Claude));
        // The view pushes the Stop action; `apply` carries it out after the frame.
        app.apply(Action::StandupStop, &egui::Context::default());
        assert_eq!((app.standup.phase, app.standup.stopping), (None, true));
        assert!(app.toast.as_ref().is_some_and(|t| t.text == "Stopped the review"));
        app.apply(Action::StandupRun, &egui::Context::default());
        assert_eq!(app.standup.phase, None, "no new run until the stopped one has ended");
        pump(&mut app, 1);
        assert_eq!((app.standup.phase, app.standup.stopping, app.standup.error.as_deref()), (None, false, None), "the stop is not an error");
        assert_eq!(app.standup_keys().len(), 3, "the last good review stays");
        app.standup_stop();
        assert!(!app.standup.stopping, "nothing runs: nothing to stop");
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

    fn types_json() -> serde_json::Value {
        // Like a project without Story: Task, Bug, Epic and a sub-task.
        serde_json::json!({ "issueTypes": [
            { "id": "10001", "name": "Task", "subtask": false },
            { "id": "10002", "name": "Bug", "subtask": false },
            { "id": "10000", "name": "Epic", "subtask": false },
            { "id": "10003", "name": "Sub-task", "subtask": true },
        ], "startAt": 0, "maxResults": 50, "total": 4 })
    }

    fn types(names: &[(&str, &str)]) -> Vec<jira::IssueType> {
        names.iter().map(|(id, name)| jira::IssueType { id: (*id).into(), name: (*name).into(), subtask: false }).collect()
    }

    fn cache_types(app: &mut App, names: &[(&str, &str)]) {
        app.standup.saved.issue_types = Some(standup::ProjectTypes { project: "CI".into(), types: types(names) });
    }

    /// The reads `refresh_all` starts after a create, answered, so none of them toasts over the create's toast.
    const RELOADS: [(&str, &str); 2] = [("POST", "/rest/api/3/search/jql"), ("GET", "/rest/agile/1.0/board/")];

    fn reload_routes() -> Vec<(&'static str, &'static str, serde_json::Value)> {
        vec![(RELOADS[0].0, RELOADS[0].1, serde_json::json!({ "issues": [] })), (RELOADS[1].0, RELOADS[1].1, serde_json::json!({ "values": [] }))]
    }

    fn toast(app: &App) -> Option<(&str, ToastKind)> {
        app.toast.as_ref().map(|t| (t.text.as_str(), t.kind))
    }

    /// The create request among the recorded ones.
    fn create_body(rx: &std::sync::mpsc::Receiver<jira::tests::Recorded>) -> serde_json::Value {
        let r = rx.try_iter().find(|r| r.method == "POST" && r.path == "/rest/api/3/issue").expect("a create");
        serde_json::from_str(&r.body).unwrap()
    }

    #[test]
    fn add_creates_with_the_edited_title_and_remembers_it() {
        let _g = jira::tests::WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let sprints = serde_json::json!({ "values": [{ "id": 7, "name": "Sprint 92", "state": "active" }] });
        let mut routes = vec![
            ("GET", "/rest/api/3/issue/createmeta/CI/issuetypes", types_json()),
            ("POST", "/rest/api/3/issue", serde_json::json!({ "key": "CI-42" })),
            ("GET", "/rest/agile/1.0/board/101/sprint", sprints),
            ("POST", "/rest/agile/1.0/sprint/7/issue", serde_json::json!({})),
        ];
        routes.extend(reload_routes());
        let (base, rx) = jira::tests::stub_server(routes);
        let mut app = App::for_test(cfg(), Some(client(&base)));
        with_review(&mut app);
        assert_eq!(app.standup_type_name(&app.standup.saved.visible()[0].clone()), None, "the types are not read yet: no type shown");
        app.standup.titles.insert("url:https://a".into(), "Edited title".into());
        let ctx = egui::Context::default();
        app.apply(Action::StandupAdd(Target::CurrentSprint), &ctx);
        assert!(app.standup.adding.contains("url:https://a"));
        app.apply(Action::StandupAdd(Target::CurrentSprint), &ctx);
        // The write worker read the types itself (they come back for the cache), then created.
        pump(&mut app, 2);
        let create = create_body(&rx);
        assert_eq!(create["fields"]["summary"], "Edited title");
        assert_eq!(create["fields"]["project"]["key"], "CI");
        assert_eq!(create["fields"]["issuetype"], serde_json::json!({ "id": "10001" }));
        assert_eq!(create["fields"]["labels"], serde_json::json!(["PLAT"]));
        assert_eq!(app.standup.saved.added.get("url:https://a").map(String::as_str), Some("CI-42"));
        assert_eq!(toast(&app), Some(("Created CI-42 (Task) in Sprint 92", ToastKind::Success)));
        assert_eq!(app.standup_types().map(<[_]>::len), Some(4), "the read types are cached");
        assert!(app.standup.adding.is_empty());
        assert_eq!(app.standup_keys().len(), 3, "an added card stays, marked added");
        app.apply(Action::StandupAdd(Target::Backlog), &ctx);
        assert_eq!(toast(&app), Some(("Already added as CI-42", ToastKind::Info)));
        assert!(app.standup.adding.is_empty(), "not created twice");
    }

    #[test]
    fn a_story_in_a_project_without_one_is_created_as_a_task_and_the_toast_says_so() {
        let _g = jira::tests::WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut routes = vec![("POST", "/rest/api/3/issue", serde_json::json!({ "key": "CI-42" }))];
        routes.extend(reload_routes());
        let (base, rx) = jira::tests::stub_server(routes);
        let mut app = App::for_test(cfg(), Some(client(&base)));
        with_review(&mut app);
        app.standup.saved.review.as_mut().unwrap().suggestions[0].issue_type = "Story".into();
        cache_types(&mut app, &[("10001", "Task"), ("10002", "Bug"), ("10000", "Epic")]);
        let story = app.standup.saved.visible()[0].clone();
        assert_eq!(app.standup_type_name(&story).as_deref(), Some("Task"), "the card names what will be created");
        app.apply(Action::StandupAdd(Target::Backlog), &egui::Context::default());
        pump(&mut app, 1);
        assert_eq!(create_body(&rx)["fields"]["issuetype"], serde_json::json!({ "id": "10001" }), "Story wanted, Task sent by id");
        assert_eq!(toast(&app), Some(("Created CI-42 as a Task (no Story in CI) in the backlog", ToastKind::Success)));
    }

    #[test]
    fn known_failures_send_nothing_and_say_why() {
        let _g = jira::tests::WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (base, rx) = jira::tests::stub_server(vec![]);
        let mut app = App::for_test(cfg(), Some(client(&base)));
        with_review(&mut app);
        let ctx = egui::Context::default();
        cache_types(&mut app, &[("10000", "Epic")]);
        app.apply(Action::StandupAdd(Target::Backlog), &ctx);
        assert_eq!(toast(&app), Some(("Can't create in CI: the project has no issue type you can create", ToastKind::Error)));
        cache_types(&mut app, &[("10001", "Task")]);
        app.sprints = vec![jira::Sprint { id: 7, name: "Sprint 92".into(), state: "active".into() }];
        app.apply(Action::StandupAdd(Target::NextSprint), &ctx);
        assert_eq!(toast(&app), Some(("Not created: the board has no future sprint", ToastKind::Error)));
        assert!(app.standup.adding.is_empty());
        assert!(rx.try_recv().is_err(), "no request sent");
    }

    #[test]
    fn a_rejected_create_reads_as_jiras_reason() {
        let _g = jira::tests::WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let rejected = serde_json::json!({ "errorMessages": [], "errors": { "issuetype": "Specify a valid issue type" } });
        let (base, _rx) = jira::tests::stub_server_codes(vec![
            ("GET", "/rest/api/3/issue/createmeta/CI/issuetypes", 200, types_json()),
            ("POST", "/rest/api/3/issue", 400, rejected),
        ]);
        let mut app = App::for_test(cfg(), Some(client(&base)));
        with_review(&mut app);
        cache_types(&mut app, &[("10001", "Task")]);
        app.apply(Action::StandupAdd(Target::Backlog), &egui::Context::default());
        // Jira rejected the cached type: the types are read again (and cached), the create tried once more.
        pump(&mut app, 2);
        assert_eq!(toast(&app), Some(("Create failed: issuetype: Specify a valid issue type", ToastKind::Error)));
        assert!(!app.standup.saved.added.contains_key("url:https://a"));
        assert!(app.standup.adding.is_empty());
    }

    #[test]
    fn types_load_on_opening_standup_once_and_on_its_refresh() {
        let (base, rx) = jira::tests::stub_server(vec![("GET", "/rest/api/3/issue/createmeta/CI/issuetypes", types_json())]);
        let mut app = App::for_test(cfg(), None);
        let ctx = egui::Context::default();
        app.apply(Action::SetScreen(Screen::Standup), &ctx);
        assert!(!app.standup.types_loading, "no client: nothing would clear the flag");
        app.apply(Action::SetScreen(Screen::Issues), &ctx);
        app.client = Some(client(&base));
        app.apply(Action::SetScreen(Screen::Standup), &ctx);
        assert!(app.standup.types_loading);
        pump(&mut app, 1);
        assert_eq!(app.standup_types().map(|t| t.iter().map(|t| t.name.as_str()).collect::<Vec<_>>()), Some(vec!["Task", "Bug", "Epic", "Sub-task"]));
        app.apply(Action::SetScreen(Screen::Issues), &ctx);
        app.apply(Action::SetScreen(Screen::Standup), &ctx);
        assert!(!app.standup.types_loading, "read once a launch");
        assert_eq!(rx.try_iter().filter(|r| r.path.contains("createmeta")).count(), 1);
        app.handle(Msg::IssueTypes { project: "CI".into(), result: Err(jira::Error::Other("Jira returned 500".into())) });
        assert_eq!(app.standup_types().map(<[_]>::len), Some(4), "a failed read keeps the last types");
        app.standup.saved.issue_types.as_mut().unwrap().project = "OTHER".into();
        assert!(app.standup_types().is_none(), "another project's types are not this one's");
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
        assert_eq!(
            app.toast.as_ref().map(|t| (t.text.as_str(), t.kind)),
            Some(("Read-only mode is on, so the create was not sent. Restart without JIRA_TICKETS_READONLY to send changes.", ToastKind::Info))
        );
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
        let placed = Placed::NotMoved { sprint: "Sprint 92".into(), why: "Jira returned 403: no".into() };
        let added = Added { key: "CI-9".into(), issue_type: "Task".into(), wanted: "Task".into(), placed };
        app.handle(Msg::StandupAdded { dkey: "url:https://b".into(), result: Ok(added) });
        assert_eq!(app.standup.saved.added.get("url:https://b").map(String::as_str), Some("CI-9"));
        assert_eq!(toast(&app), Some(("Created CI-9 (Task) in the backlog, but couldn't move it to Sprint 92: Jira returned 403: no", ToastKind::Error)));
        app.handle(Msg::StandupAdded {
            dkey: "url:https://a".into(),
            result: Err(AddError::Create(jira::Error::Other("Jira returned 400: issuetype: x".into()))),
        });
        assert!(!app.standup.saved.added.contains_key("url:https://a"));
        assert_eq!(toast(&app), Some(("Create failed: issuetype: x", ToastKind::Error)), "no doubled prefix");
    }

    #[test]
    fn toasts_say_what_was_created_and_where() {
        let added = |ty: &str, wanted: &str, placed: Placed| Added { key: "CI-42".into(), issue_type: ty.into(), wanted: wanted.into(), placed };
        let text = |a: Added| added_toast("CI", &a).0;
        assert_eq!(text(added("Task", "Task", Placed::Sprint("Sprint 92".into()))), "Created CI-42 (Task) in Sprint 92");
        assert_eq!(text(added("Task", "task", Placed::Backlog)), "Created CI-42 (Task) in the backlog");
        assert_eq!(text(added("Task", "Story", Placed::Sprint("Sprint 92".into()))), "Created CI-42 as a Task (no Story in CI) in Sprint 92");
        assert_eq!(text(added("Improvement", "Bug", Placed::Backlog)), "Created CI-42 as an Improvement (no Bug in CI) in the backlog");
        assert_eq!(text(added("Task", "", Placed::Backlog)), "Created CI-42 (Task) in the backlog");
        let fail = |e: AddError| add_failed_toast("CI", &e);
        assert_eq!(fail(AddError::NoType), ("Can't create in CI: the project has no issue type you can create".into(), ToastKind::Error));
        assert_eq!(fail(AddError::Sprint("the board has no active sprint".into())).0, "Not created: the board has no active sprint");
        assert_eq!(
            fail(AddError::Types(jira::Error::Other("Jira returned 404: no project".into()))).0,
            "Create failed: couldn't read the issue types of CI: Jira returned 404: no project"
        );
        let ro = jira::Error::Other("read-only mode, so nothing was sent to Jira".into());
        assert_eq!(fail(AddError::Create(ro)).1, ToastKind::Info);
    }

    #[test]
    fn target_sprints_name_what_the_board_has() {
        let s = |id, name: &str, state: &str| jira::Sprint { id, name: name.into(), state: state.into() };
        let both = [s(7, "Sprint 92", "active"), s(8, "Sprint 93", "future")];
        assert_eq!(target_sprint(Target::CurrentSprint, &both), Ok(Some("Sprint 92".into())));
        assert_eq!(target_sprint(Target::NextSprint, &both), Ok(Some("Sprint 93".into())));
        assert_eq!(target_sprint(Target::Backlog, &both), Ok(None));
        assert_eq!(target_sprint(Target::NextSprint, &both[..1]), Err("the board has no future sprint".into()));
        assert_eq!(target_sprint(Target::NextSprint, &[]), Ok(None), "no sprints known yet: the add finds out");
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

    fn ticket(key: &str, cat: &str) -> jira::Issue {
        jira::Issue { key: key.into(), summary: key.into(), status: jira::Status { name: "S".into(), category: cat.into() }, ..Default::default() }
    }

    fn with_tickets(app: &mut App) {
        app.standup.saved.tickets = standup::Tickets {
            since: chrono::NaiveDate::from_ymd_opt(2026, 10, 30),
            done: vec![ticket("CI-1", "done")],
            in_progress: vec![ticket("CI-2", "indeterminate")],
        };
    }

    #[test]
    fn the_cursor_runs_over_the_tickets_then_the_suggestions() {
        let mut app = App::for_test(cfg(), None);
        with_review(&mut app);
        with_tickets(&mut app);
        let ctx = egui::Context::default();
        assert_eq!(app.standup_focused_ticket().map(|i| i.key.as_str()), Some("CI-1"));
        assert_eq!(app.standup_done_target(), None, "a done card is not moved again");
        assert_eq!(app.standup_source_url().as_deref(), Some("https://x.atlassian.net/browse/CI-1"), "o opens the ticket");
        app.apply(Action::StandupDismiss, &ctx);
        assert_eq!(app.standup_keys().len(), 3, "x on a ticket card dismisses nothing");
        app.apply(Action::StandupMove(1), &ctx);
        assert_eq!(app.standup_done_target().as_deref(), Some("CI-2"), "the focused in-progress card is D's target");
        app.apply(Action::StandupMove(1), &ctx);
        assert_eq!(app.standup_focused_ticket().map(|i| i.key.as_str()), None);
        assert_eq!(app.standup_source_url().as_deref(), Some("https://a"), "then the first suggestion");
        assert_eq!(app.standup_done_target(), None, "not added yet");
        app.standup.saved.added.insert("url:https://a".into(), "CI-42".into());
        assert_eq!(app.standup_done_target().as_deref(), Some("CI-42"));
        app.apply(Action::StandupMove(9), &ctx);
        assert_eq!(app.standup.cursor, 4, "clamped onto the last suggestion");
        app.apply(Action::StandupDismiss, &ctx);
        assert_eq!(app.standup_keys(), ["url:https://a", "url:https://b"]);
        assert_eq!(app.standup.cursor, 3);
    }

    #[test]
    fn tickets_load_with_a_client_and_keep_the_focused_card() {
        let issue =
            |key: &str, cat: &str| serde_json::json!({ "key": key, "fields": { "summary": key, "status": { "name": "S", "statusCategory": { "key": cat } } } });
        let (base, rx) = jira::tests::stub_server(vec![("POST", "/rest/api/3/search/jql", serde_json::json!({ "issues": [issue("CI-7", "indeterminate")] }))]);
        let mut app = App::for_test(cfg(), None);
        app.load_standup_tickets();
        assert!(!app.standup.tickets_loading, "no client: nothing would clear the flag");
        app.client = Some(client(&base));
        app.today = || chrono::NaiveDate::from_ymd_opt(2026, 11, 2).unwrap();
        with_review(&mut app);
        with_tickets(&mut app);
        app.standup.cursor = 2; // the first suggestion
        app.load_standup_tickets();
        app.load_standup_tickets();
        assert!(app.standup.tickets_loading && app.standup.tickets_reload, "a second ask runs after the first lands");
        pump(&mut app, 1);
        let t = &app.standup.saved.tickets;
        assert_eq!((t.since, t.done.len(), t.in_progress.len()), (chrono::NaiveDate::from_ymd_opt(2026, 10, 30), 1, 0), "the ticket shows once");
        assert_eq!(app.standup.cursor, 1, "still on the first suggestion, one card fewer above it");
        assert!(app.standup.tickets_loading, "the deferred read started");
        pump(&mut app, 1);
        assert!(!app.standup.tickets_loading);
        let bodies: Vec<String> = rx.try_iter().map(|r| r.body).collect();
        assert_eq!(bodies.len(), 4, "two reads of two searches");
        assert!(bodies[0].contains("statusCategoryChangedDate >= \\\"2026-10-30\\\""), "{}", bodies[0]);
        app.handle(Msg::StandupTickets(Err(jira::Error::Other("Jira returned 500".into()))));
        assert_eq!(app.standup.saved.tickets.done.len(), 1, "a failed read keeps the last cards");
        assert!(app.standup.tickets_error.is_some());
    }
}
