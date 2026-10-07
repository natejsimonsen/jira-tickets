//! App state and the main loop. Network calls run on background threads and
//! send a `Msg` back over a channel; the UI never waits on the network.

use crate::auth::{self, Login};
use crate::config::Config;
use crate::jira::{self, Client, Issue, Resolution, Sprint, Transition, User};
use crate::store::{self, ListId, Store};
use crate::{theme, views};
use egui::{Key, KeyboardShortcut, Modifiers};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

const AUTO_REFRESH: Duration = Duration::from_secs(5 * 60);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Screen {
    Issues,
    Grooming,
    Team,
    Roadmap,
}

#[allow(clippy::large_enum_variant)] // Msgs are few and short-lived; keep the plain Result shape.
pub enum Msg {
    Login(Option<Login>),
    Me(jira::Result<User>),
    List {
        id: ListId,
        result: jira::Result<Vec<Issue>>,
        stale: bool,
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
    Users(jira::Result<Vec<User>>),
    Sprints(jira::Result<Vec<Sprint>>),
}

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
    SetQuick(Option<usize>),
    ToggleMine,
    RunSearch(String),
    ToggleGoTo,
    GoTo,
    Move(i32),
    Comment,
}

#[derive(Default)]
pub struct Loaded {
    pub loading: bool,
    pub error: Option<String>,
    pub fetched: Option<Instant>,
}

pub struct Toast {
    pub text: String,
    pub error: bool,
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
    pub filter: crate::filter::IssueFilter,
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
    /// The JQL of the running search, for Retry.
    pub last_search_jql: Option<String>,
    /// Unposted comment text, keyed by issue key.
    pub drafts: HashMap<String, String>,
    /// The issue key of the comment post in flight.
    pub posting: Option<String>,
    pub(crate) client: Option<Arc<Client>>,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    ctx: egui::Context,
    last_refresh: Instant,
    was_focused: bool,
}

impl App {
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
        let app = App {
            cfg: cfg.clone(),
            store: Store::load_cached().unwrap_or_default(),
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
            filter: Default::default(),
            focus_search: false,
            goto: None,
            cursor: 0,
            last_cursor: 0,
            search_active: false,
            search_seq: 0,
            last_search_jql: None,
            drafts: HashMap::new(),
            posting: None,
            client: None,
            tx,
            rx,
            ctx: cc.egui_ctx.clone(),
            last_refresh: Instant::now(),
            was_focused: true,
        };
        // Finding the token may run `op read`, which can block; keep it off the UI thread.
        let (tx, ctx) = (app.tx.clone(), app.ctx.clone());
        std::thread::spawn(move || {
            let _ = tx.send(Msg::Login(auth::find_login(&cfg.email, cfg.api_token.as_deref())));
            ctx.request_repaint();
        });
        app
    }

    pub fn spawn(&self, f: impl FnOnce(&Client, &Sender<Msg>) + Send + 'static) {
        let Some(client) = self.client.clone() else { return };
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            f(&client, &tx);
            ctx.request_repaint();
        });
    }

    pub fn toast(&mut self, text: impl Into<String>, error: bool) {
        self.toast = Some(Toast { text: text.into(), error, until: Instant::now() + Duration::from_secs(if error { 8 } else { 3 }) });
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

    pub fn refresh_all(&mut self) {
        self.last_refresh = Instant::now();
        let cfg = self.cfg.clone();
        let fields: Vec<String> = jira::LIST_FIELDS.iter().map(|s| s.to_string()).collect();
        self.load_list(ListId::TeamOpen, store::jql_team_open(&cfg), fields.clone());
        self.load_list(ListId::TeamInProgress, store::jql_team_in_progress(&cfg), fields);
    }

    /// Epics depend on TeamOpen's parents, so they load after it lands.
    fn load_epics(&mut self) {
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

    pub fn load_list(&mut self, id: ListId, jql: String, fields: Vec<String>) {
        self.load_list_seq(id, jql, fields, None);
    }

    fn load_list_seq(&mut self, id: ListId, jql: String, fields: Vec<String>, seq: Option<u64>) {
        let client_present = self.client.is_some();
        let entry = self.lists.entry(id).or_default();
        if !should_load(client_present, entry.loading) {
            return;
        }
        entry.loading = true;
        self.spawn(move |c, tx| {
            let fields: Vec<&str> = fields.iter().map(String::as_str).collect();
            let _ = tx.send(Msg::List { id, result: c.search(&jql, &fields), stale: false, seq });
        });
    }

    pub fn load_detail(&mut self, key: &str) {
        if !should_load(self.client.is_some(), self.detail_loading.contains(key)) {
            return;
        }
        self.detail_loading.insert(key.to_string());
        let key = key.to_string();
        self.spawn(move |c, tx| {
            let _ = tx.send(Msg::Detail { key: key.clone(), result: c.issue(&key, &[]) });
        });
    }

    fn handle(&mut self, msg: Msg) {
        match msg {
            Msg::Login(Some(login)) => self.connect(login),
            Msg::Login(None) => self.finding_token = false,
            Msg::Me(Ok(u)) => self.me = Some(u),
            Msg::Me(Err(jira::Error::Unauthorized(m))) => {
                self.client = None;
                self.token_error = Some(m);
            }
            Msg::Me(Err(e)) => self.toast(e.to_string(), true),
            Msg::List { id, result, stale: _, seq } => {
                if id == ListId::Search && !accept_search(self.search_seq, seq, self.search_active) {
                    return;
                }
                let entry = self.lists.entry(id).or_default();
                entry.loading = false;
                match result {
                    Ok(issues) => {
                        entry.error = None;
                        entry.fetched = Some(Instant::now());
                        self.offline = false;
                        self.store.absorb(id, issues);
                        self.store.fetched_at = Some(chrono::Utc::now().timestamp());
                        let store = self.store.clone();
                        std::thread::spawn(move || store.save_cached());
                        if id == ListId::TeamOpen && self.client.is_some() {
                            self.load_epics();
                        }
                    }
                    Err(jira::Error::Unauthorized(m)) => {
                        self.client = None;
                        self.token_error = Some(m);
                    }
                    Err(e) => {
                        if marks_offline(id, &e) {
                            self.offline = true;
                        }
                        entry.error = Some(e.to_string());
                    }
                }
            }
            Msg::Detail { key, result } => {
                self.detail_loading.remove(&key);
                match result {
                    Ok(issue) => self.store.merge_detail(issue),
                    Err(e) => self.toast(format!("Couldn't load {key}: {e}"), true),
                }
            }
            Msg::Done { key, what, result } => {
                if what == "Comment" {
                    self.posting = None;
                    if result.is_ok() {
                        self.drafts.remove(&key);
                    }
                }
                match result {
                    Ok(()) => self.toast(format!("{what} done"), false),
                    Err(e) => self.toast(format!("{what} failed: {e}"), true),
                }
                if self.client.is_some() {
                    self.load_detail(&key);
                    self.refresh_all();
                }
            }
            // Pickers (Task 11) handle these; until then they are ignored.
            Msg::Transitions { .. } | Msg::Resolutions(_) | Msg::Users(_) | Msg::Sprints(_) => {}
        }
    }

    fn apply(&mut self, a: Action, ctx: &egui::Context) {
        match a {
            Action::SetScreen(s) => self.screen = s,
            Action::Refresh => self.refresh_all(),
            Action::OpenUrl(u) => open_url(&u),
            Action::Copy(t) => {
                ctx.copy_text(t);
                self.toast("Copied", false);
            }
            Action::Select(key) => {
                self.selected = Some(key.clone());
                self.load_detail(&key);
            }
            Action::SubmitToken => {
                let t = self.token_input.trim().to_string();
                if t.is_empty() {
                    return;
                }
                let _ = auth::save_token(&t);
                self.token_error = None;
                self.connect(Login { email: self.cfg.email.clone(), token: t, source: auth::Source::Pasted });
            }
            Action::FindToken => {
                self.finding_token = true;
                let (tx, ctx2, cfg) = (self.tx.clone(), self.ctx.clone(), self.cfg.clone());
                std::thread::spawn(move || {
                    let _ = tx.send(Msg::Login(auth::find_login(&cfg.email, cfg.api_token.as_deref())));
                    ctx2.request_repaint();
                });
            }
            Action::SignOut => {
                auth::forget_token();
                self.client = None;
                self.login = None;
                self.me = None;
            }
            Action::Toast(t, e) => self.toast(t, e),
            Action::SetEpic(e) => {
                self.filter.epic = e;
                self.cursor = 0;
            }
            Action::SetQuick(q) => {
                self.filter.quick = q;
                match q.and_then(|i| self.cfg.issues.filters.get(i)) {
                    Some(f) => {
                        let jql = format!("labels = {} AND ({})", store::jql_quote(&self.cfg.scope.team_label), f.jql);
                        self.actions.push(Action::RunSearch(jql));
                    }
                    None => self.actions.push(Action::RunSearch(String::new())),
                }
            }
            Action::ToggleMine => {
                self.filter.mine = !self.filter.mine;
                self.cursor = 0;
            }
            Action::RunSearch(jql) => {
                self.cursor = 0;
                self.search_seq += 1;
                if jql.trim().is_empty() {
                    self.search_active = false;
                    self.filter.quick = None;
                    self.last_search_jql = None;
                    self.store.lists.remove(&ListId::Search);
                    // Drops the error and any in-flight flag; late results are ignored by seq.
                    self.lists.remove(&ListId::Search);
                    return;
                }
                self.search_active = true;
                self.last_search_jql = Some(jql.trim().to_string());
                let full = format!("project = {} AND ({}) ORDER BY Rank ASC", store::jql_quote(&self.cfg.scope.project), jql.trim());
                let fields: Vec<String> = jira::LIST_FIELDS.iter().map(|s| s.to_string()).collect();
                // A new search replaces any in flight.
                self.lists.entry(ListId::Search).or_default().loading = false;
                self.load_list_seq(ListId::Search, full, fields, Some(self.search_seq));
            }
            Action::ToggleGoTo => self.goto = if self.goto.is_some() { None } else { Some(String::new()) },
            Action::GoTo => {
                if let Some(text) = self.goto.take()
                    && let Some(key) = crate::filter::parse_goto(&text, &self.cfg.scope.project)
                {
                    self.selected = Some(key.clone());
                    self.load_detail(&key);
                }
            }
            Action::Move(delta) => {
                let n = views::issues::current_rows(self).len();
                if n == 0 {
                    return;
                }
                self.cursor = (self.cursor as i32 + delta).clamp(0, n as i32 - 1) as usize;
                if let Some(key) = views::issues::current_rows(self).get(self.cursor).map(|i| i.key.clone()) {
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
                self.spawn(move |c, tx| {
                    let _ = tx.send(Msg::Done { key: key.clone(), what: "Comment", result: c.add_comment(&key, &text) });
                });
            }
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        let cmd = Modifiers::COMMAND;
        let screens = [(Key::Num1, Screen::Issues), (Key::Num2, Screen::Grooming), (Key::Num3, Screen::Team), (Key::Num4, Screen::Roadmap)];
        for (k, s) in screens {
            if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(cmd, k))) {
                self.actions.push(Action::SetScreen(s));
            }
        }
        if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(cmd, Key::R))) {
            self.actions.push(Action::Refresh);
        }
        if self.screen != Screen::Issues {
            return;
        }
        let shortcut = |m: Modifiers, k: Key| ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(m, k)));
        if shortcut(cmd, Key::K) {
            self.actions.push(Action::ToggleGoTo);
        }
        if self.goto.is_some() {
            if shortcut(Modifiers::NONE, Key::Escape) {
                self.goto = None;
            }
            return;
        }
        if shortcut(cmd, Key::F) {
            self.focus_search = true;
        }
        let browse = self.selected.as_ref().map(|k| self.cfg.browse_url(k));
        if shortcut(cmd, Key::O)
            && let Some(u) = &browse
        {
            self.actions.push(Action::OpenUrl(u.clone()));
        }
        // Plain keys belong to a focused text field.
        if ctx.text_edit_focused() {
            return;
        }
        if shortcut(Modifiers::NONE, Key::Slash) {
            self.focus_search = true;
        }
        if shortcut(Modifiers::NONE, Key::J) | shortcut(Modifiers::NONE, Key::ArrowDown) {
            self.actions.push(Action::Move(1));
        }
        if shortcut(Modifiers::NONE, Key::K) | shortcut(Modifiers::NONE, Key::ArrowUp) {
            self.actions.push(Action::Move(-1));
        }
        if shortcut(Modifiers::NONE, Key::C)
            && let Some(u) = browse
        {
            self.actions.push(Action::Copy(u));
        }
    }
}

/// A load starts only with a client to run it and none already in flight;
/// otherwise nothing would ever clear the loading flag.
fn should_load(client_present: bool, already_loading: bool) -> bool {
    client_present && !already_loading
}

/// A Search result lands only while a search is active and only for the latest run.
fn accept_search(current_seq: u64, msg_seq: Option<u64>, active: bool) -> bool {
    active && msg_seq == Some(current_seq)
}

/// Only a transport failure on a refresh list means we are offline; bad JQL,
/// rate limits, or a failed search just show on that list.
fn marks_offline(id: ListId, e: &jira::Error) -> bool {
    matches!(e, jira::Error::Network(_)) && matches!(id, ListId::TeamOpen | ListId::TeamInProgress | ListId::Epics)
}

/// ⌘W closes the window on every screen, signed in or not.
fn close_shortcut(ctx: &egui::Context) {
    if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::W))) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}

pub fn open_url(url: &str) {
    #[cfg(target_os = "macos")]
    {
        let _ = webbrowser::open(url);
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
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
            views::main(self, ui);
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

    #[test]
    fn loads_need_a_client_and_no_load_in_flight() {
        assert!(should_load(true, false));
        assert!(!should_load(true, true));
        assert!(!should_load(false, false));
        assert!(!should_load(false, true));
    }

    #[test]
    fn search_results_only_land_for_the_latest_active_run() {
        assert!(accept_search(3, Some(3), true), "current");
        assert!(!accept_search(3, Some(2), true), "stale");
        assert!(!accept_search(3, Some(3), false), "inactive");
        assert!(!accept_search(3, None, true), "untagged");
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
}
