//! App state and the main loop. Network calls run on background threads and
//! send a `Msg` back over a channel; the UI never waits on the network.

mod actions;
mod groom;
mod loads;
mod msg;
mod pickers;
mod shortcuts;

pub use actions::Action;
pub use msg::Msg;
pub use pickers::{
    ASSIGN_USERS, POINTS_HINT, PRIORITY_HINT, Picker, assign_current_row, assign_cursor_after_filter, assign_matches, assign_note, assign_rows,
    sprint_current_row, sprint_row_label, transition_row,
};
// Part of `crate::app`'s surface for the plan 2 views; nothing outside app/ calls them yet.
#[allow(unused_imports)]
pub use {actions::rank_target, pickers::filtered_users};

use shortcuts::close_shortcut;

use crate::auth::{self, Login};
use crate::config::Config;
use crate::jira::{self, Client, Sprint, User};
use crate::store::{ListId, Store};
use crate::{theme, views};
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

const AUTO_REFRESH: Duration = Duration::from_secs(5 * 60);

/// A write run on the ordered write worker.
type WriteJob = Box<dyn FnOnce(&Client, &Sender<Msg>) + Send>;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Screen {
    Issues,
    Grooming,
    Team,
    Roadmap,
}

#[derive(Default)]
pub struct Loaded {
    pub loading: bool,
    /// A load was asked for while one was in flight; run it when that one lands.
    pub reload_after: bool,
    pub error: Option<String>,
    pub fetched: Option<Instant>,
}

/// What a toast reports: its dot is green, accent, or red.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastKind {
    Success,
    /// A notice, not a failure (read-only refusals, "No changes").
    Info,
    Error,
}

/// The Grooming screen's two modes: walking `New` before the meeting, and the epic-by-epic session.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum GroomMode {
    #[default]
    PreCheck,
    Session,
}

/// Everything the Grooming screen keeps between frames.
#[derive(Default)]
pub struct GroomingState {
    pub mode: GroomMode,
    /// The newest report in `report_dir`, read on every refresh.
    pub report: Option<(PathBuf, crate::report::Report)>,
    /// Pre-check verdicts by issue key, pre-filled from the report.
    pub verdicts: BTreeMap<String, crate::grooming::Verdict>,
    /// The pre-check row, or the session candidate index.
    pub cursor: usize,
    /// Session: the selected epic in the rail.
    pub epic_index: usize,
    /// Session: the ghost row's index in the Ready queue while accepting.
    pub placement: Option<usize>,
    /// Session: the open bounce box's text.
    pub bounce_text: Option<String>,
    /// Session: the bounce box takes keyboard focus on its next frame.
    pub bounce_focus: bool,
    /// Session: the ticket whose Accept, Bounce or Park is in flight; no other decision starts meanwhile.
    pub deciding: Option<String>,
    /// When the focused ticket took focus, for the per-ticket timer.
    pub focused_since: Option<Instant>,
    /// The pre-check summary dialog is open.
    pub summary_open: bool,
    /// The pre-check ticket whose detail was last asked for on focus.
    pub detail_for: Option<String>,
    /// A Promote is in flight: from its first transition until the report is written (or waits on the confirm).
    pub promoting: bool,
    /// Promote's transitions that have not answered yet.
    pub promote_remaining: usize,
    /// What the running Promote acts on, taken when it started.
    pub promote_run: Option<PromoteRun>,
    /// (key, summary, error) for each transition the last Promote could not make.
    pub promote_failures: Vec<(String, String, String)>,
    /// The text of the last report Promote rendered, for copy.
    pub last_summary: Option<String>,
    /// A rendered report waiting on "Report for today exists. Overwrite?".
    pub pending_report: Option<String>,
}

/// A Promote's inputs, frozen when it starts, so a refresh meanwhile cannot change the report.
pub struct PromoteRun {
    pub date: String,
    /// Every candidate in rank order.
    pub rows: Vec<crate::grooming::RunRow>,
    /// The keys being transitioned, in rank order.
    pub promoted: Vec<String>,
    pub held_back: usize,
    pub needs_detail: Vec<(String, Vec<crate::grooming::Code>)>,
}

pub struct Toast {
    pub text: String,
    pub kind: ToastKind,
    pub until: Instant,
}

pub struct App {
    pub cfg: Arc<Config>,
    pub store: Store,
    pub lists: HashMap<ListId, Loaded>,
    pub screen: Screen,
    pub me: Option<User>,
    pub login: Option<Login>,
    pub finding_token: bool,
    pub token_input: String,
    pub token_error: Option<String>,
    pub toast: Option<Toast>,
    pub actions: Vec<Action>,
    /// Set when a refresh failed on the network; the banner shows the store's fetched_at.
    pub offline: bool,
    pub selected: Option<String>,
    /// Issue keys with a detail fetch in flight.
    pub detail_loading: std::collections::HashSet<String>,
    /// When each issue's detail last arrived.
    detail_fetched: HashMap<String, Instant>,
    pub filter: crate::filter::IssueFilter,
    /// The board's active and future sprints in Jira's order, for the sprint scope picker.
    pub sprints: Vec<Sprint>,
    /// The sprint scope has been defaulted to the active sprint once; later refreshes keep the user's choice.
    pub sprint_default_applied: bool,
    /// The board's sprints have answered (or failed) once, so the sprint combo shows the real scope.
    pub sprints_answered: bool,
    /// The sprint scope came from the last run's cached sprint list (`filter::launch_sprint_scope`), so the combo
    /// and epic hues treat it as known before the board answers.
    pub sprints_cached: bool,
    /// Set by ⌘F or `/`; the search box takes focus on its next frame.
    pub focus_search: bool,
    /// The ⌘K box text while it is open.
    pub goto: Option<String>,
    /// Keyboard cursor: an index into the Issues view's current rows.
    pub cursor: usize,
    /// The cursor the list last drew; when it differs the row scrolls into view.
    pub last_cursor: usize,
    /// The list shows ListId::Search instead of TeamOpen.
    pub search_active: bool,
    /// Bumped by every `RunSearch`; Search results from older runs are dropped.
    pub search_seq: u64,
    /// The seq of the search in flight; the list keeps its previous rows until it lands.
    pub search_pending: Option<u64>,
    /// The search in flight was rerun by a header refresh (⌘R), so the header spinner, not the count row, shows it.
    pub search_from_refresh: bool,
    /// The JQL of the running search, for Retry.
    pub last_search_jql: Option<String>,
    /// The active search was typed into the search box (not a quick filter or team scope).
    pub typed_search: bool,
    /// The search rows on screen came from a typed JQL search (set when its result is accepted),
    /// so the count row says "· JQL" only once those rows are showing.
    pub typed_shown: bool,
    /// When the search box text last changed, for the text search's debounce.
    pub text_edited: Option<Instant>,
    /// The term of the active text search (in flight or shown); None when the Search list is not one.
    pub text_search: Option<String>,
    /// The full scope + text JQL that search ran, so a scope change (or members dropping out) reruns it.
    pub text_search_jql: Option<String>,
    /// The term whose server rows are on screen (set when its result is accepted). While these rows
    /// match the box, the local text filter steps aside: the server already matched more than summary.
    pub text_shown: Option<String>,
    /// Jira rejected a configured member, so team queries match labels only until restart.
    pub members_disabled: bool,
    /// Each epic's hue slot, given to the epics on screen first and kept for the session (`theme::assign_epic_slots`).
    pub epic_slots: std::collections::BTreeMap<String, usize>,
    /// The slots above were given from the resolved sprint scope's rows and are kept from now on.
    pub epic_slots_final: bool,
    /// Unposted comment text, keyed by issue key.
    pub drafts: HashMap<String, String>,
    /// The issue key of the comment post in flight.
    pub posting: Option<String>,
    pub picker: Option<Picker>,
    /// The detail pane's x-range on the Issues screen last frame; the toast centres over it.
    pub detail_x: Option<(f32, f32)>,
    /// The detail pane's pill row top (its fill's edge) when it showed in the pane's viewport last frame; pickers
    /// anchor their dialog over it.
    pub detail_pills_top: Option<f32>,
    /// The keyboard help overlay is showing; it owns the keyboard while open.
    pub help_open: bool,
    /// A picker's items are being fetched; a spinner shows meanwhile.
    pub picker_loading: bool,
    /// (issue key, transition id) of the transition in flight, so a
    /// "resolution required" error can retry it with a resolution.
    pending_transition: Option<(String, String)>,
    /// Optimistic rank swaps awaiting their Done: (list, issue, neighbour), oldest first.
    pending_ranks: Vec<(ListId, String, String)>,
    pub grooming: GroomingState,
    /// How many report reads were started, for tests.
    #[cfg(test)]
    pub(crate) report_reads: usize,
    pub(crate) client: Option<Arc<Client>>,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    /// Feeds the single write worker, so writes reach Jira in the order made.
    writes: Sender<(Arc<Client>, WriteJob)>,
    /// The write worker's end of `writes`, held until the first write starts the worker.
    write_rx: Option<Receiver<(Arc<Client>, WriteJob)>>,
    /// Results are saved to the disk cache; off in `for_test`, so tests never touch the real cache.
    persist_cache: bool,
    ctx: egui::Context,
    last_refresh: Instant,
    was_focused: bool,
}

impl App {
    /// True once the first team list has loaded; snapshots wait for it.
    pub fn shot_ready(&self) -> bool {
        self.lists.get(&crate::store::ListId::TeamOpen).is_some_and(|l| l.fetched.is_some())
    }

    /// Every field at its starting value, with no window behind it: no theme, no cache read, no token search,
    /// and the write worker not yet started.
    fn empty(cfg: Arc<Config>, tx: Sender<Msg>, rx: Receiver<Msg>, ctx: egui::Context) -> App {
        let (writes, write_rx) = channel::<(Arc<Client>, WriteJob)>();
        let filter = crate::filter::IssueFilter {
            team: crate::filter::TeamScope::Label(cfg.scope.team_label.clone()),
            team_labels: cfg.scope.team_labels(),
            team_members: cfg.scope.team_members(&cfg.team.members),
            ..Default::default()
        };
        App {
            cfg,
            store: Store::default(),
            lists: HashMap::new(),
            screen: Screen::Issues,
            me: None,
            login: None,
            finding_token: true,
            token_input: String::new(),
            token_error: None,
            toast: None,
            actions: Vec::new(),
            offline: false,
            selected: None,
            detail_loading: Default::default(),
            detail_fetched: HashMap::new(),
            filter,
            sprints_cached: false,
            sprints: Vec::new(),
            sprint_default_applied: false,
            sprints_answered: false,
            focus_search: false,
            goto: None,
            cursor: 0,
            last_cursor: 0,
            search_active: false,
            search_seq: 0,
            search_pending: None,
            search_from_refresh: false,
            detail_x: None,
            detail_pills_top: None,
            last_search_jql: None,
            typed_search: false,
            typed_shown: false,
            text_edited: None,
            text_search: None,
            text_search_jql: None,
            text_shown: None,
            members_disabled: false,
            epic_slots: Default::default(),
            epic_slots_final: false,
            drafts: HashMap::new(),
            posting: None,
            picker: None,
            help_open: false,
            picker_loading: false,
            pending_transition: None,
            pending_ranks: Vec::new(),
            grooming: GroomingState::default(),
            #[cfg(test)]
            report_reads: 0,
            client: None,
            tx,
            rx,
            writes,
            write_rx: Some(write_rx),
            persist_cache: false,
            ctx,
            last_refresh: Instant::now(),
            was_focused: true,
        }
    }

    pub fn new(cc: &eframe::CreationContext<'_>, cfg: Config) -> Self {
        theme::apply(&cc.egui_ctx);
        match std::env::var("JIRA_TICKETS_THEME").as_deref() {
            Ok("dark") => cc.egui_ctx.set_theme(egui::Theme::Dark),
            Ok("light") => cc.egui_ctx.set_theme(egui::Theme::Light),
            _ => cc.egui_ctx.set_theme(egui::ThemePreference::System),
        }
        if std::env::var_os("JIRA_TICKETS_READONLY").is_some() {
            jira::READ_ONLY.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        let (tx, rx) = channel();
        let cfg = Arc::new(cfg);
        let mut app = App::empty(cfg.clone(), tx, rx, cc.egui_ctx.clone());
        app.persist_cache = true;
        app.store = Store::for_launch(Store::load_cached());
        let cached_sprints: Vec<Sprint> = crate::cache::load(&sprints_cache_key(cfg.scope.board_id)).unwrap_or_default();
        let launch_scope = crate::filter::launch_sprint_scope(&cached_sprints);
        if let Some(scope) = &launch_scope {
            app.filter.sprint = scope.clone();
        }
        app.sprints_cached = launch_scope.is_some();
        app.sprints = cached_sprints;
        // Finding the token may run `op read`, which can block; keep it off the UI thread.
        let (tx, ctx) = (app.tx.clone(), app.ctx.clone());
        std::thread::spawn(move || {
            let _ = tx.send(Msg::Login(auth::find_login(&cfg.email, cfg.api_token.as_deref(), cfg.op_account.as_deref())));
            ctx.request_repaint();
        });
        app
    }

    /// A headless App for unit tests: no window, no theme, optional client.
    #[cfg(test)]
    pub fn for_test(cfg: Config, client: Option<Arc<Client>>) -> App {
        let (tx, rx) = channel();
        let mut app = App::empty(Arc::new(cfg), tx, rx, egui::Context::default());
        app.client = client;
        app.finding_token = false;
        app
    }

    /// Handles every result that has arrived, then applies the queued actions (a test's frame without the views).
    #[cfg(test)]
    pub fn drain(&mut self) {
        while let Ok(m) = self.rx.try_recv() {
            self.handle(m);
        }
        let ctx = self.ctx.clone();
        for a in std::mem::take(&mut self.actions) {
            self.apply(a, &ctx);
        }
    }

    pub fn spawn(&self, f: impl FnOnce(&Client, &Sender<Msg>) + Send + 'static) {
        let Some(client) = self.client.clone() else { return };
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            f(&client, &tx);
            ctx.request_repaint();
        });
    }

    /// Queues a write on the ordered write worker; a no-op without a client.
    pub fn spawn_write(&mut self, f: impl FnOnce(&Client, &Sender<Msg>) + Send + 'static) {
        let Some(client) = self.client.clone() else { return };
        self.ensure_write_worker();
        let _ = self.writes.send((client, Box::new(f)));
    }

    /// Starts the write worker on the first write; later calls do nothing.
    fn ensure_write_worker(&mut self) {
        let Some(write_rx) = self.write_rx.take() else { return };
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        // Ends when the App (the only sender) is dropped.
        std::thread::spawn(move || {
            for (client, job) in write_rx {
                job(&client, &tx);
                ctx.request_repaint();
            }
        });
    }

    pub fn toast(&mut self, text: impl Into<String>, error: bool) {
        self.toast_kind(text, if error { ToastKind::Error } else { ToastKind::Success });
    }

    pub fn toast_kind(&mut self, text: impl Into<String>, kind: ToastKind) {
        let secs = match kind {
            ToastKind::Success => 3,
            ToastKind::Info => 5,
            ToastKind::Error => 8,
        };
        self.toast = Some(Toast { text: text.into(), kind, until: Instant::now() + Duration::from_secs(secs) });
    }

    fn connect(&mut self, login: Login) {
        self.client = Some(Arc::new(Client::new(&self.cfg.site, &login)));
        self.login = Some(login);
        self.finding_token = false;
        self.spawn(|c, tx| {
            let _ = tx.send(Msg::Me(c.myself()));
        });
        self.refresh_all();
    }
}

/// The cache key of a board's sprint list (the first frame's sprint scope, `filter::launch_sprint_scope`).
pub fn sprints_cache_key(board: u64) -> String {
    format!("sprints-{board}")
}

/// Only web and mail links open; anything else (file:, custom schemes) is ignored.
pub fn openable(url: &str) -> bool {
    let u = url.trim_start().to_ascii_lowercase();
    ["http://", "https://", "mailto:"].iter().any(|s| u.starts_with(s))
}

pub fn open_url(url: &str) {
    if !openable(url) {
        return;
    }
    #[cfg(target_os = "macos")]
    {
        let _ = webbrowser::open(url);
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = std::process::Command::new("xdg-open").arg("--").arg(url).spawn();
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        while let Ok(msg) = self.rx.try_recv() {
            self.handle(msg);
        }
        let focused = ctx.input(|i| i.viewport().focused.unwrap_or(true));
        let back = focused && !self.was_focused && self.last_refresh.elapsed() > Duration::from_secs(60);
        self.was_focused = focused;
        if self.client.is_some() && (back || self.last_refresh.elapsed() > AUTO_REFRESH) {
            self.refresh_all();
        }
        ctx.request_repaint_after(AUTO_REFRESH);

        // The root ui has no background of its own in eframe 0.36.
        ui.painter().rect_filled(ui.max_rect(), 0.0, theme::palette(&ctx).bg);
        close_shortcut(&ctx);
        if self.client.is_some() {
            self.shortcuts(&ctx);
            // Set again by the detail pane when its pill row shows.
            self.detail_pills_top = None;
            views::main(self, ui);
            self.text_search_tick(&ctx);
            views::pickers::show(self, &ctx);
            views::help::show(self, &ctx);
        } else {
            views::sign_in(self, ui);
        }
        views::toast(self, &ctx);

        // Actions run after the views drew, so draw once more to show their effect.
        let actions = std::mem::take(&mut self.actions);
        if !actions.is_empty() {
            ctx.request_repaint();
        }
        for a in actions {
            self.apply(a, &ctx);
        }
        let urls = ctx.output_mut(|o| {
            let mut urls = Vec::new();
            o.commands.retain(|c| match c {
                egui::OutputCommand::OpenUrl(u) => {
                    urls.push(u.url.clone());
                    false
                }
                _ => true,
            });
            urls
        });
        for u in urls {
            open_url(&u);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::config::Config;
    use crate::store::ListId;

    fn cfg() -> Config {
        toml::from_str("site = \"https://x.atlassian.net\"\nemail = \"a@b.c\"\n").unwrap()
    }

    #[test]
    fn for_test_builds_without_a_window() {
        let app = App::for_test(cfg(), None);
        assert!(app.client.is_none());
        assert_eq!(app.screen, Screen::Issues);
        assert!(app.store.issues.is_empty());
    }

    #[test]
    fn unauthorized_list_result_returns_to_sign_in() {
        let mut app = App::for_test(cfg(), None);
        app.handle(Msg::Me(Err(crate::jira::Error::Unauthorized("401".into()))));
        assert!(app.client.is_none());
        assert_eq!(app.token_error.as_deref(), Some("401"));
    }

    #[test]
    fn team_open_result_populates_store_and_queues_epics() {
        let (base, _rx) = crate::jira::tests::stub_server(vec![]);
        let login = crate::auth::Login { email: "a@b.c".into(), token: "t".into(), source: crate::auth::Source::Env };
        let client = std::sync::Arc::new(crate::jira::Client::new(&base, &login));
        let mut app = App::for_test(cfg(), Some(client));
        let issue = crate::jira::Issue { key: "CI-1".into(), parent_key: Some("CI-9".into()), ..Default::default() };
        app.handle(Msg::List { id: ListId::TeamOpen, result: Ok(vec![issue]), stale: false, seq: None, with_members: false });
        assert_eq!(app.store.list(ListId::TeamOpen).len(), 1);
        assert!(app.lists.get(&ListId::Epics).is_some_and(|l| l.loading), "epics load was started");
    }

    #[test]
    fn stale_search_result_is_dropped() {
        let mut app = App::for_test(cfg(), None);
        app.search_active = true;
        app.search_seq = 5;
        let issue = crate::jira::Issue { key: "CI-2".into(), ..Default::default() };
        app.handle(Msg::List { id: ListId::Search, result: Ok(vec![issue]), stale: false, seq: Some(4), with_members: false });
        assert!(app.store.list(ListId::Search).is_empty());
    }

    #[test]
    fn report_loaded_prefills_pre_check_verdicts() {
        let mut app = App::for_test(cfg(), None);
        let r = crate::report::parse(
            "# x — 2026-10-01\n\n## Planning — by epic\n\n### [E-1 Epic](u?issueParent=1)\n\n- [CI-1](u) — a · rank 1 · older\n\n## Needs detail (still in `New`)\n\n### (no epic)\n\n- [CI-2](u) — b · rank 2 · created 2026-09-01 · missing: empty\n",
        );
        app.handle(Msg::ReportLoaded(Some((std::path::PathBuf::from("/tmp/x.md"), r))));
        assert_eq!(app.grooming.verdicts.get("CI-1"), Some(&crate::grooming::Verdict::Ready));
        assert!(matches!(app.grooming.verdicts.get("CI-2"), Some(crate::grooming::Verdict::NeedsDetail(c)) if c == &vec![crate::grooming::Code::Empty]));
    }

    #[test]
    fn same_report_again_does_not_bring_back_removed_verdicts() {
        let mut app = App::for_test(cfg(), None);
        let text = "# x — 2026-10-01\n\n## Planning — by epic\n\n### (no epic)\n\n- [CI-1](u) — a · rank 1 · older\n";
        let at = |p: &str| Some((std::path::PathBuf::from(p), crate::report::parse(text)));
        app.handle(Msg::ReportLoaded(at("/r/2026-10-01.md")));
        assert!(app.grooming.verdicts.contains_key("CI-1"));
        app.grooming.verdicts.remove("CI-1");
        app.handle(Msg::ReportLoaded(at("/r/2026-10-01.md")));
        assert!(!app.grooming.verdicts.contains_key("CI-1"), "a refresh re-reading the same report keeps the undo");
        assert!(app.grooming.report.is_some(), "the report itself is still kept");
        app.handle(Msg::ReportLoaded(at("/r/2026-10-02.md")));
        assert_eq!(app.grooming.verdicts.get("CI-1"), Some(&crate::grooming::Verdict::Ready), "a new report prefills");
    }

    #[test]
    fn verdict_actions_set_and_undo() {
        let mut app = App::for_test(cfg(), None);
        app.screen = Screen::Grooming;
        app.store.absorb(
            ListId::TeamOpen,
            vec![crate::jira::Issue {
                key: "CI-1".into(),
                labels: vec!["PLAT".into()],
                status: crate::jira::Status { name: "New".into(), category: "new".into() },
                ..Default::default()
            }],
        );
        let ctx = egui::Context::default();
        app.apply(Action::Verdict(crate::grooming::Verdict::Ready), &ctx);
        assert_eq!(app.grooming.verdicts.get("CI-1"), Some(&crate::grooming::Verdict::Ready));
        app.apply(Action::UndoVerdict, &ctx);
        assert!(!app.grooming.verdicts.contains_key("CI-1"));
    }

    #[test]
    fn only_web_and_mail_links_open() {
        for ok in ["https://x.atlassian.net/browse/CI-1", "HTTP://example.com", "mailto:a@b.c"] {
            assert!(openable(ok), "{ok}");
        }
        for bad in ["file:///etc/passwd", "javascript:alert(1)", "smb://host/share", "-n", "x-apple.systempreferences:"] {
            assert!(!openable(bad), "{bad}");
        }
    }
}
