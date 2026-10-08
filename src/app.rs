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
/// A detail fetched this recently is not refetched on selection.
const DETAIL_FRESH: Duration = Duration::from_secs(60);

/// A write run on the ordered write worker.
type WriteJob = Box<dyn FnOnce(&Client, &Sender<Msg>) + Send>;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Screen {
    Issues,
    Grooming,
    Team,
    Roadmap,
}

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
    SetSprint(crate::filter::SprintScope),
    SetTeam(crate::filter::TeamScope),
    /// Adds the scoped team's label to the selected issue (key `l`).
    ApplyTeamLabel,
    SetQuick(Option<usize>),
    ToggleMine,
    ToggleRoadmap,
    RunSearch(String),
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
}

/// The open modal picker. `cursor` indexes the rows the picker draws; Assign
/// and Sprint put a synthetic first row ("Unassigned", "Backlog") at 0.
pub enum Picker {
    Transition {
        key: String,
        items: Vec<Transition>,
        cursor: usize,
    },
    Resolution {
        key: String,
        transition_id: String,
        items: Vec<Resolution>,
        cursor: usize,
    },
    Assign {
        key: String,
        items: Vec<User>,
        cursor: usize,
        filter: String,
    },
    Sprint {
        key: String,
        items: Vec<Sprint>,
        cursor: usize,
    },
    /// The `orig_*` fields hold the values the dialog opened with; only changed fields are sent.
    Edit {
        key: String,
        labels: String,
        priority: String,
        points: String,
        orig_labels: String,
        orig_priority: String,
        orig_points: String,
    },
}

#[derive(Default)]
pub struct Loaded {
    pub loading: bool,
    /// A load was asked for while one was in flight; run it when that one lands.
    pub reload_after: bool,
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
    /// When each issue's detail last arrived.
    detail_fetched: HashMap<String, Instant>,
    pub filter: crate::filter::IssueFilter,
    /// The board's active and future sprints in Jira's order, for the sprint scope picker.
    pub sprints: Vec<Sprint>,
    /// The sprint scope has been defaulted to the active sprint once; later refreshes keep the user's choice.
    pub sprint_default_applied: bool,
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
    /// The JQL of the running search, for Retry.
    pub last_search_jql: Option<String>,
    /// Jira rejected a configured member, so team queries match labels only until restart.
    pub members_disabled: bool,
    /// Unposted comment text, keyed by issue key.
    pub drafts: HashMap<String, String>,
    /// The issue key of the comment post in flight.
    pub posting: Option<String>,
    pub picker: Option<Picker>,
    /// The keyboard help overlay is showing; it owns the keyboard while open.
    pub help_open: bool,
    /// A picker's items are being fetched; a spinner shows meanwhile.
    pub picker_loading: bool,
    /// (issue key, transition id) of the transition in flight, so a
    /// "resolution required" error can retry it with a resolution.
    pending_transition: Option<(String, String)>,
    pub(crate) client: Option<Arc<Client>>,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    /// Feeds the single write worker, so writes reach Jira in the order made.
    writes: Sender<(Arc<Client>, WriteJob)>,
    ctx: egui::Context,
    last_refresh: Instant,
    was_focused: bool,
}

impl App {
    /// True once the first team list has loaded; snapshots wait for it.
    pub fn shot_ready(&self) -> bool {
        self.lists.get(&crate::store::ListId::TeamOpen).is_some_and(|l| l.fetched.is_some())
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
        let (writes, write_rx) = channel::<(Arc<Client>, WriteJob)>();
        {
            let (tx, ctx) = (tx.clone(), cc.egui_ctx.clone());
            // Ends when the App (the only sender) is dropped.
            std::thread::spawn(move || {
                for (client, job) in write_rx {
                    job(&client, &tx);
                    ctx.request_repaint();
                }
            });
        }
        let cfg = Arc::new(cfg);
        let filter = crate::filter::IssueFilter {
            team: crate::filter::TeamScope::Label(cfg.scope.team_label.clone()),
            team_labels: cfg.scope.team_labels(),
            team_members: cfg.scope.team_members(&cfg.team.members),
            ..Default::default()
        };
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
            detail_fetched: HashMap::new(),
            filter,
            sprints: Vec::new(),
            sprint_default_applied: false,
            focus_search: false,
            goto: None,
            cursor: 0,
            last_cursor: 0,
            search_active: false,
            search_seq: 0,
            search_pending: None,
            last_search_jql: None,
            members_disabled: false,
            drafts: HashMap::new(),
            posting: None,
            picker: None,
            help_open: false,
            picker_loading: false,
            pending_transition: None,
            client: None,
            tx,
            rx,
            writes,
            ctx: cc.egui_ctx.clone(),
            last_refresh: Instant::now(),
            was_focused: true,
        };
        // Finding the token may run `op read`, which can block; keep it off the UI thread.
        let (tx, ctx) = (app.tx.clone(), app.ctx.clone());
        std::thread::spawn(move || {
            let _ = tx.send(Msg::Login(auth::find_login(&cfg.email, cfg.api_token.as_deref(), cfg.op_account.as_deref())));
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

    /// Queues a write on the ordered write worker; a no-op without a client.
    pub fn spawn_write(&self, f: impl FnOnce(&Client, &Sender<Msg>) + Send + 'static) {
        let Some(client) = self.client.clone() else { return };
        let _ = self.writes.send((client, Box::new(f)));
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

    /// TeamOpen and TeamInProgress are built with members unless they are disabled.
    pub fn load_list(&mut self, id: ListId, jql: String, fields: Vec<String>) {
        let with_members = matches!(id, ListId::TeamOpen | ListId::TeamInProgress) && !self.members_disabled;
        self.load_list_seq(id, jql, fields, None, with_members);
    }

    fn load_list_seq(&mut self, id: ListId, jql: String, fields: Vec<String>, seq: Option<u64>, with_members: bool) {
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
    fn reload(&mut self, id: ListId) {
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

    /// The search the team picker and quick filter call for: the quick filter
    /// ANDed with the team clause, or under No team label the no-team query
    /// alone. None when neither applies (the list shows TeamOpen).
    fn scoped_search_jql(&self) -> Option<String> {
        let quick = self.filter.quick.and_then(|i| self.cfg.issues.filters.get(i));
        match (quick, &self.filter.team) {
            (Some(f), team) => Some(format!("{} AND ({})", store::quick_scope_jql(team, &self.cfg, !self.members_disabled), f.jql)),
            (None, crate::filter::TeamScope::None) => Some(store::jql_no_team(&self.cfg, !self.members_disabled)),
            (None, _) => None,
        }
    }

    /// Runs `jql` (non-empty) as the Search list under the current `search_seq`,
    /// replacing any search in flight.
    fn search(&mut self, jql: &str) {
        // Only the team-scoped search, built while members are enabled, carries
        // them; a JQL the user typed, or one built labels-only, never retries.
        let with_members = !self.members_disabled && self.scoped_search_jql().is_some_and(|j| j.trim() == jql.trim());
        self.search_active = true;
        self.last_search_jql = Some(jql.trim().to_string());
        let full = format!("project = {} AND ({}) ORDER BY Rank ASC", store::jql_quote(&self.cfg.scope.project), jql.trim());
        let fields: Vec<String> = jira::LIST_FIELDS.iter().map(|s| s.to_string()).collect();
        self.lists.entry(ListId::Search).or_default().loading = false;
        // Without a client nothing is spawned, so nothing would ever clear it.
        self.search_pending = self.client.is_some().then_some(self.search_seq);
        self.load_list_seq(ListId::Search, full, fields, Some(self.search_seq), with_members);
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

    fn handle(&mut self, msg: Msg) {
        match msg {
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
                    // A configured member Jira doesn't know fails the whole query; retry on labels alone.
                    Err(_) if member_retry => {
                        entry.error = None;
                        if !self.members_disabled {
                            self.members_disabled = true;
                            self.toast("A team member in the config did not resolve; showing labelled tickets only", true);
                        }
                        match id {
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
                if what == "Comment" {
                    self.posting = None;
                    if result.is_ok() {
                        self.drafts.remove(&key);
                    }
                }
                let (text, error) = done_toast(what, &result);
                self.toast(text, error);
                if self.client.is_some() {
                    self.load_detail(&key, true);
                    self.refresh_all();
                }
            }
            Msg::Transitions { key, result } => {
                self.picker_loading = false;
                match result {
                    Ok(items) => self.picker = Some(Picker::Transition { key, items, cursor: 0 }),
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
                    Ok(items) => self.picker = Some(Picker::Assign { key, items, cursor: 0, filter: String::new() }),
                    Err(e) => self.toast(e.to_string(), true),
                }
            }
            Msg::Sprints { key, result } => {
                self.picker_loading = false;
                match result {
                    Ok(items) => self.picker = Some(Picker::Sprint { key, items, cursor: 0 }),
                    Err(e) => self.toast(e.to_string(), true),
                }
            }
            Msg::BoardSprints(Ok(sprints)) => {
                self.sprints = sprints;
                let next = if self.sprint_default_applied {
                    crate::filter::sprint_scope_after_refresh(&self.filter.sprint, &self.sprints)
                } else {
                    crate::filter::default_sprint_scope(&self.sprints)
                };
                if !self.sprint_default_applied || next != self.filter.sprint {
                    self.filter.sprint = next;
                    self.sprint_default_applied = true;
                    self.cursor = 0;
                }
            }
            // Keep the last good list on a failed refresh; only say so when there is none.
            Msg::BoardSprints(Err(e)) => {
                if self.sprints.is_empty() {
                    self.toast(format!("Couldn't load sprints: {e}"), true);
                }
            }
        }
    }

    fn apply(&mut self, a: Action, ctx: &egui::Context) {
        match a {
            Action::SetScreen(s) => self.screen = s,
            Action::Refresh => {
                self.refresh_all();
                // A quick filter's or No team label's search is part of what's shown; rerun it
                // too, keeping the cursor. The seq bump drops the older run's reply.
                if let Some(jql) = self.scoped_search_jql() {
                    self.search_seq += 1;
                    self.search(&jql);
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
                self.cursor = 0;
            }
            Action::SetTeam(t) => {
                let was_none = self.filter.team == crate::filter::TeamScope::None;
                self.filter.team = t;
                self.cursor = 0;
                // A quick filter's search, and No team label's, is scoped to the team; rerun it
                // for the new one. Leaving No team label drops its search.
                match self.scoped_search_jql() {
                    Some(jql) => self.actions.push(Action::RunSearch(jql)),
                    None if was_none => self.actions.push(Action::RunSearch(String::new())),
                    None => {}
                }
            }
            Action::ApplyTeamLabel => {
                let crate::filter::TeamScope::Label(label) = &self.filter.team else {
                    self.toast("Pick a team to label", false);
                    return;
                };
                let Some(i) = self.selected.as_ref().and_then(|k| self.store.get(k)) else { return };
                if i.labels.iter().any(|l| l == label) {
                    self.toast("Already labelled", false);
                    return;
                }
                let (key, label) = (i.key.clone(), label.clone());
                self.spawn_write(move |c, tx| {
                    let _ = tx.send(Msg::Done { key: key.clone(), what: "Label", result: c.add_label(&key, &label) });
                });
            }
            Action::SetQuick(q) => {
                self.filter.quick = q;
                self.actions.push(Action::RunSearch(self.scoped_search_jql().unwrap_or_default()));
            }
            Action::ToggleMine => {
                self.filter.mine = !self.filter.mine;
                self.cursor = 0;
            }
            Action::ToggleRoadmap => {
                self.filter.roadmap = !self.filter.roadmap;
                self.cursor = 0;
            }
            Action::RunSearch(jql) => {
                self.cursor = 0;
                self.search_seq += 1;
                if jql.trim().is_empty() {
                    self.search_active = false;
                    self.filter.quick = None;
                    self.last_search_jql = None;
                    self.search_pending = None;
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
                    self.selected = Some(key.clone());
                    self.load_detail(&key, false);
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
                self.spawn_write(move |c, tx| {
                    let _ = tx.send(Msg::Done { key: key.clone(), what: "Comment", result: c.add_comment(&key, &text) });
                });
            }
            // spawn is a no-op without a client, so every Open* checks first
            // or `picker_loading` would never clear.
            Action::OpenTransition => {
                let Some(key) = self.selected.clone() else { return };
                if self.client.is_none() || self.picker_loading {
                    return;
                }
                self.picker_loading = true;
                self.spawn(move |c, tx| {
                    let _ = tx.send(Msg::Transitions { key: key.clone(), result: c.transitions(&key) });
                });
            }
            Action::OpenAssign => {
                let Some(key) = self.selected.clone() else { return };
                if self.client.is_none() || self.picker_loading {
                    return;
                }
                self.picker_loading = true;
                let project = self.cfg.scope.project.clone();
                self.spawn(move |c, tx| {
                    let _ = tx.send(Msg::Users { key, result: c.assignable_users(&project) });
                });
            }
            Action::OpenSprint => {
                let Some(key) = self.selected.clone() else { return };
                if self.client.is_none() || self.picker_loading {
                    return;
                }
                self.picker_loading = true;
                let board = self.cfg.scope.board_id;
                self.spawn(move |c, tx| {
                    let _ = tx.send(Msg::Sprints { key, result: c.board_sprints(board, "active,future") });
                });
            }
            Action::OpenEdit => {
                // A late picker response would replace the Edit dialog.
                if self.client.is_none() || self.picker_loading {
                    return;
                }
                let Some(i) = self.selected.as_ref().and_then(|k| self.store.get(k)) else { return };
                let (labels, priority, points) = (i.labels.join(", "), i.priority.clone(), i.story_points.map(|n| n.to_string()).unwrap_or_default());
                self.picker = Some(Picker::Edit {
                    key: i.key.clone(),
                    labels: labels.clone(),
                    priority: priority.clone(),
                    points: points.clone(),
                    orig_labels: labels,
                    orig_priority: priority,
                    orig_points: points,
                });
            }
            Action::PickerMove(d) => {
                if let Some(p) = &mut self.picker {
                    let (cur, len) = match p {
                        Picker::Transition { cursor, items, .. } => (cursor, items.len()),
                        Picker::Resolution { cursor, items, .. } => (cursor, items.len()),
                        Picker::Assign { cursor, items, filter, .. } => (cursor, filtered_users(items, filter).len() + 1),
                        Picker::Sprint { cursor, items, .. } => (cursor, items.len() + 1),
                        Picker::Edit { .. } => return,
                    };
                    *cur = wrap_cursor(*cur, d, len);
                }
            }
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
                // Move the row now; a failed rank snaps back on the refresh that follows Done.
                if let Some(list) = self.store.lists.get_mut(&src)
                    && let (Some(a), Some(b)) = (list.iter().position(|k| *k == key), list.iter().position(|k| *k == neighbour))
                {
                    list.swap(a, b);
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

    fn confirm_picker(&mut self) {
        // Without a client spawn does nothing; keep the picker open rather than drop the choice silently.
        if self.client.is_none() {
            return;
        }
        let Some(p) = self.picker.take() else { return };
        match p {
            Picker::Transition { key, items, cursor } => {
                let Some(t) = items.get(cursor).cloned() else { return };
                // Remember the transition so a resolution error can retry it.
                self.pending_transition = Some((key.clone(), t.id.clone()));
                self.spawn_write(move |c, tx| {
                    let _ = tx.send(Msg::Done { key: key.clone(), what: "Transition", result: c.transition(&key, &t.id, None) });
                });
            }
            Picker::Resolution { key, transition_id, items, cursor } => {
                let Some(r) = items.get(cursor).cloned() else { return };
                self.spawn_write(move |c, tx| {
                    let _ = tx.send(Msg::Done { key: key.clone(), what: "Transition", result: c.transition(&key, &transition_id, Some(&r.id)) });
                });
            }
            Picker::Assign { key, items, cursor, filter } => {
                let shown = filtered_users(&items, &filter);
                let account = if cursor == 0 { None } else { shown.get(cursor - 1).map(|u| u.account_id.clone()) };
                if cursor != 0 && account.is_none() {
                    return;
                }
                self.spawn_write(move |c, tx| {
                    let _ = tx.send(Msg::Done { key: key.clone(), what: "Assign", result: c.assign(&key, account.as_deref()) });
                });
            }
            Picker::Sprint { key, items, cursor } => {
                let sprint = if cursor == 0 { None } else { items.get(cursor - 1).map(|s| s.id) };
                if cursor != 0 && sprint.is_none() {
                    return;
                }
                self.spawn_write(move |c, tx| {
                    let r = match sprint {
                        Some(id) => c.move_to_sprint(id, &[&key]),
                        None => c.move_to_backlog(&[&key]),
                    };
                    let _ = tx.send(Msg::Done { key: key.clone(), what: "Sprint move", result: r });
                });
            }
            Picker::Edit { key, labels, priority, points, orig_labels, orig_priority, orig_points } => {
                let Some(fields) = edit_fields(&labels, &priority, &points, &orig_labels, &orig_priority, &orig_points) else {
                    self.toast("No changes", false);
                    return;
                };
                self.spawn_write(move |c, tx| {
                    let _ = tx.send(Msg::Done { key: key.clone(), what: "Edit", result: c.update_fields(&key, fields) });
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
        let shortcut = |m: Modifiers, k: Key| ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(m, k)));
        // Help owns the keyboard while open, ahead of pickers and the Issues keys.
        if self.help_open {
            if shortcut(Modifiers::NONE, Key::Escape) || shortcut(Modifiers::NONE, Key::Questionmark) {
                self.actions.push(Action::ToggleHelp);
            }
            return;
        }
        // `?` is Shift+/; its logical key is Questionmark, so it never reads as `/`.
        if !ctx.text_edit_focused() && shortcut(Modifiers::NONE, Key::Questionmark) {
            self.actions.push(Action::ToggleHelp);
            return;
        }
        // An open picker owns the keyboard. Assign's filter box has focus, so
        // j/k type there; Edit has text fields, so only Esc applies (Enter in a
        // field does not save; use the Save button).
        if let Some((edit, assign)) = self.picker.as_ref().map(|p| (matches!(p, Picker::Edit { .. }), matches!(p, Picker::Assign { .. }))) {
            if shortcut(Modifiers::NONE, Key::Escape) {
                self.actions.push(Action::PickerCancel);
                return;
            }
            if edit {
                return;
            }
            let letters = !assign;
            if shortcut(Modifiers::NONE, Key::ArrowDown) || (letters && shortcut(Modifiers::NONE, Key::J)) {
                self.actions.push(Action::PickerMove(1));
            }
            if shortcut(Modifiers::NONE, Key::ArrowUp) || (letters && shortcut(Modifiers::NONE, Key::K)) {
                self.actions.push(Action::PickerMove(-1));
            }
            if shortcut(Modifiers::NONE, Key::Enter) {
                self.actions.push(Action::PickerConfirm);
            }
            return;
        }
        if self.screen != Screen::Issues {
            return;
        }
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
        // Before plain j/k: consume_key(NONE, J) also matches Shift+J.
        if shortcut(Modifiers::SHIFT, Key::J) {
            self.actions.push(Action::Rank(1));
        }
        if shortcut(Modifiers::SHIFT, Key::K) {
            self.actions.push(Action::Rank(-1));
        }
        for (k, a) in [
            (Key::T, Action::OpenTransition),
            (Key::A, Action::OpenAssign),
            (Key::S, Action::OpenSprint),
            (Key::E, Action::OpenEdit),
            (Key::L, Action::ApplyTeamLabel),
            (Key::M, Action::ToggleRoadmap),
        ] {
            if shortcut(Modifiers::NONE, k) {
                self.actions.push(a);
            }
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

/// The toast for a finished write: done, blocked by read-only mode, or failed.
pub fn done_toast(what: &str, result: &jira::Result<()>) -> (String, bool) {
    match result {
        Ok(()) => (format!("{what} done"), false),
        Err(e) if jira::is_read_only(e) => (format!("Read-only mode: {} not sent to Jira", what.to_lowercase()), true),
        Err(e) => (format!("{what} failed: {e}"), true),
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

/// Users whose display name contains `filter`, case-insensitively.
pub fn filtered_users(items: &[User], filter: &str) -> Vec<User> {
    let f = filter.trim().to_lowercase();
    items.iter().filter(|u| f.is_empty() || u.display_name.to_lowercase().contains(&f)).cloned().collect()
}

/// The Assign cursor after its filter text changes: the first match (row 1,
/// after "Unassigned") when the filter is non-empty and matches someone, so
/// type-then-Enter assigns rather than unassigns; otherwise "Unassigned".
pub fn assign_cursor_after_filter(matches: usize, filter_empty: bool) -> usize {
    if filter_empty || matches == 0 { 0 } else { 1 }
}

/// Moves a picker cursor by `d`, wrapping at both ends.
fn wrap_cursor(cur: usize, d: i32, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    (cur as i32 + d).rem_euclid(len as i32) as usize
}

/// The `fields` body for the Edit picker, holding only what changed from the
/// values it opened with, so a save never overwrites someone else's edit to
/// another field: labels when the list differs (comma split, trimmed, empties
/// dropped), priority when it differs and is named, points when they differ
/// and parse. None when nothing changed.
fn edit_fields(labels: &str, priority: &str, points: &str, orig_labels: &str, orig_priority: &str, orig_points: &str) -> Option<serde_json::Value> {
    let split = |s: &str| s.split(',').map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect::<Vec<_>>();
    let mut fields = serde_json::Map::new();
    if split(labels) != split(orig_labels) {
        fields.insert("labels".into(), serde_json::json!(split(labels)));
    }
    if !priority.trim().is_empty() && priority.trim() != orig_priority.trim() {
        fields.insert("priority".into(), serde_json::json!({ "name": priority.trim() }));
    }
    if let Ok(n) = points.trim().parse::<f64>()
        && orig_points.trim().parse::<f64>().ok() != Some(n)
    {
        fields.insert(jira::POINTS_FIELD.into(), serde_json::json!(n));
    }
    (!fields.is_empty()).then_some(serde_json::Value::Object(fields))
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
/// earlier result it keeps that until the new one lands.
fn rows_source(search_active: bool, pending: bool, has_search_result: bool) -> RowsSource {
    match (search_active, pending, has_search_result) {
        (false, _, _) => RowsSource::TeamOpen,
        (true, true, false) => RowsSource::TeamOpen,
        (true, _, _) => RowsSource::Search,
    }
}

/// A Search result lands only while a search is active and only for the latest run.
fn accept_search(current_seq: u64, msg_seq: Option<u64>, active: bool) -> bool {
    active && msg_seq == Some(current_seq)
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

/// ⌘W closes the window on every screen, signed in or not.
fn close_shortcut(ctx: &egui::Context) {
    if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::W))) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
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
            views::main(self, ui);
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

    #[test]
    fn retry_without_members_only_for_queries_that_carried_them() {
        assert!(should_retry_without_members(true, true), "unresolved member in a members query: retry on labels");
        assert!(!should_retry_without_members(true, false), "members-free query (e.g. a quick filter naming the user): no loop");
        assert!(!should_retry_without_members(false, true), "any other error: no retry");
        assert!(!should_retry_without_members(false, false));
    }

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

    #[test]
    fn picker_cursor_wraps() {
        assert_eq!(wrap_cursor(0, -1, 3), 2);
        assert_eq!(wrap_cursor(2, 1, 3), 0);
        assert_eq!(wrap_cursor(1, 1, 3), 2);
        assert_eq!(wrap_cursor(0, 1, 0), 0);
    }

    #[test]
    fn users_filter_by_name_case_insensitively() {
        let u = |n: &str| User { account_id: n.to_lowercase(), display_name: n.into() };
        let items = [u("Ann Lee"), u("Bob Ray"), u("Joanne")];
        let names = |f: &str| filtered_users(&items, f).into_iter().map(|u| u.display_name).collect::<Vec<_>>();
        assert_eq!(names(""), ["Ann Lee", "Bob Ray", "Joanne"]);
        assert_eq!(names(" ANN "), ["Ann Lee", "Joanne"]);
        assert!(names("zed").is_empty());
    }

    #[test]
    fn edit_fields_body() {
        let f = edit_fields(" a, ,b ", "  ", "3.5", "", "Low", "");
        assert_eq!(f, Some(serde_json::json!({ "labels": ["a", "b"], "customfield_10016": 3.5 })));
        let f = edit_fields("", "High", "x", "old", "Low", "2");
        assert_eq!(f, Some(serde_json::json!({ "labels": [], "priority": { "name": "High" } })));
    }

    #[test]
    fn edit_fields_omit_unchanged() {
        // Only priority changed; labels (reformatted) and points (same value) stay out.
        let f = edit_fields("a,b", "High", "3.0", "a, b", "Low", "3");
        assert_eq!(f, Some(serde_json::json!({ "priority": { "name": "High" } })));
        let f = edit_fields("a, b", "Low", "3", "a, b", "Low", "3");
        assert_eq!(f, None, "nothing changed");
        assert_eq!(edit_fields("a, b", "Low", "5", "a, b", "Low", "3"), Some(serde_json::json!({ "customfield_10016": 5.0 })));
    }

    #[test]
    fn assign_filter_moves_cursor_to_first_match() {
        assert_eq!(assign_cursor_after_filter(2, false), 1, "typed and matched");
        assert_eq!(assign_cursor_after_filter(0, false), 0, "typed, no match");
        assert_eq!(assign_cursor_after_filter(5, true), 0, "cleared");
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

    #[test]
    fn done_toast_names_read_only_blocks() {
        assert_eq!(done_toast("Rank", &Ok(())), ("Rank done".to_string(), false));
        let ro = jira::Error::Other("read-only mode, so nothing was sent to Jira".into());
        assert_eq!(done_toast("Sprint move", &Err(ro)), ("Read-only mode: sprint move not sent to Jira".to_string(), true));
        let other = jira::Error::Other("Jira returned 400".into());
        assert_eq!(done_toast("Assign", &Err(other)), ("Assign failed: Jira returned 400".to_string(), true));
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
