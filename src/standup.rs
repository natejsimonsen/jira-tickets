//! The Standup screen's logic: the review window, the Jira activity in it, the `claude` run that reads GitHub and
//! Slack, and its answer: a standup summary and suggested tickets for untracked work.

use crate::jira::{self, Client, Sprint};
use chrono::{Datelike, NaiveDate, Weekday};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// The analysis prompt; `prompt` fills in its `{{…}}` placeholders.
pub const PROMPT: &str = include_str!("../prompts/standup-review.md");

/// The disk cache key of `Saved`.
pub const CACHE_KEY: &str = "standup";

/// What the review may run without asking: read-only `gh` and the Slack MCP readers.
pub const ALLOWED_TOOLS: &[&str] = &[
    "Bash(gh search:*)",
    "Bash(gh pr list:*)",
    "Bash(gh pr view:*)",
    "Bash(gh auth status)",
    "mcp__slack__slack_search_public",
    "mcp__slack__slack_search_public_and_private",
    "mcp__slack__slack_search_channels",
    "mcp__slack__slack_search_users",
    "mcp__slack__slack_read_channel",
    "mcp__slack__slack_read_thread",
    "mcp__slack__slack_read_user_profile",
];

/// Never allowed, even when the user's own Claude settings allow them: the review only reads.
pub const DISALLOWED_TOOLS: &[&str] = &[
    "Edit",
    "Write",
    "NotebookEdit",
    "mcp__slack__slack_send_message",
    "mcp__slack__slack_send_message_draft",
    "mcp__slack__slack_schedule_message",
    "mcp__slack__slack_create_canvas",
    "mcp__slack__slack_update_canvas",
];

// ---------- The window ----------

/// The team meets Monday, Wednesday and Friday.
pub fn is_standup_day(d: NaiveDate) -> bool {
    matches!(d.weekday(), Weekday::Mon | Weekday::Wed | Weekday::Fri)
}

/// The first day the review covers: the standup day before `today` (Mon → Fri, Wed → Mon, Fri → Wed). On other days,
/// the latest standup day before today (Tue → Mon, Thu → Wed, Sat and Sun → Fri).
pub fn window_start(today: NaiveDate) -> NaiveDate {
    let back = match today.weekday() {
        Weekday::Mon => 3,
        Weekday::Tue | Weekday::Thu | Weekday::Sat => 1,
        Weekday::Wed | Weekday::Fri | Weekday::Sun => 2,
    };
    today - chrono::Days::new(back)
}

/// The review runs by itself once on each standup day.
pub fn should_auto_run(today: NaiveDate, last_run: Option<NaiveDate>) -> bool {
    is_standup_day(today) && last_run != Some(today)
}

/// "Fri Oct 30".
pub fn day_label(d: NaiveDate) -> String {
    d.format("%a %b %-d").to_string()
}

// ---------- Jira activity ----------

/// One issue the user touched in the window, with what happened to it there.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Activity {
    pub key: String,
    pub summary: String,
    pub issue_type: String,
    pub status: String,
    /// (local "YYYY-MM-DD HH:MM", what happened), oldest first.
    pub events: Vec<(String, String)>,
}

pub const ACTIVITY_FIELDS: &[&str] = &["summary", "status", "issuetype", "created", "updated", "assignee", "reporter", "comment"];

/// Issues updated since `since` that the user is assigned to, reported, or (with `updated_by`) changed. `updatedBy()`
/// is a Jira Cloud function; when a site rejects it the search is rerun without it.
pub fn activity_jql(since: NaiveDate, updated_by: bool) -> String {
    let who = if updated_by {
        format!("assignee = currentUser() OR reporter = currentUser() OR issue in updatedBy(currentUser(), \"{since}\")")
    } else {
        "assignee = currentUser() OR reporter = currentUser()".to_string()
    };
    format!("updated >= \"{since}\" AND ({who}) ORDER BY updated DESC")
}

/// A Jira timestamp ("2026-10-30T14:03:00.000-0700") in local time, or None when it does not parse.
fn local_time(ts: &str) -> Option<chrono::DateTime<chrono::Local>> {
    chrono::DateTime::parse_from_str(ts, "%Y-%m-%dT%H:%M:%S%.f%z").ok().map(|t| t.with_timezone(&chrono::Local))
}

/// The issue's events on or after `since` (local time): its creation, and the status changes, assignments and
/// comments made by `me` (anyone's when `me` is empty). None when nothing in the window is the user's.
pub fn activity_of(v: &Value, me: &str, since: NaiveDate) -> Option<Activity> {
    let f = &v["fields"];
    let s = |x: &Value| x.as_str().unwrap_or_default().to_string();
    let mine = |author: &Value| me.is_empty() || author["accountId"].as_str() == Some(me);
    let in_window = |ts: &str| local_time(ts).filter(|t| t.date_naive() >= since).map(|t| t.format("%Y-%m-%d %H:%M").to_string());
    let mut events = Vec::new();
    if let Some(at) = in_window(f["created"].as_str().unwrap_or_default())
        && mine(&f["reporter"])
    {
        events.push((at, "created".to_string()));
    }
    for h in v["changelog"]["histories"].as_array().map(Vec::as_slice).unwrap_or_default() {
        let Some(at) = in_window(h["created"].as_str().unwrap_or_default()) else { continue };
        if !mine(&h["author"]) {
            continue;
        }
        for item in h["items"].as_array().map(Vec::as_slice).unwrap_or_default() {
            let (from, to) = (s(&item["fromString"]), s(&item["toString"]));
            match item["field"].as_str() {
                Some("status") => events.push((at.clone(), format!("status {from} → {to}"))),
                Some("assignee") if !to.is_empty() => events.push((at.clone(), format!("assigned to {to}"))),
                Some("resolution") if !to.is_empty() => events.push((at.clone(), format!("resolved as {to}"))),
                _ => {}
            }
        }
    }
    for c in f["comment"]["comments"].as_array().map(Vec::as_slice).unwrap_or_default() {
        let Some(at) = in_window(c["created"].as_str().unwrap_or_default()) else { continue };
        if mine(&c["author"]) {
            let text: String = crate::adf::to_plain_text(&c["body"]).split_whitespace().collect::<Vec<_>>().join(" ");
            events.push((at, format!("commented: {}", clip(&text, 300))));
        }
    }
    if events.is_empty() {
        return None;
    }
    events.sort();
    Some(Activity { key: s(&v["key"]), summary: s(&f["summary"]), issue_type: s(&f["issuetype"]["name"]), status: s(&f["status"]["name"]), events })
}

/// `text` cut to `max` characters, with "…" when cut.
fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max { text.to_string() } else { format!("{}…", text.chars().take(max).collect::<String>()) }
}

/// The activity as the prompt's Markdown list.
pub fn render_activity(activity: &[Activity], site: &str) -> String {
    if activity.is_empty() {
        return "(No Jira activity found in the window.)".to_string();
    }
    let site = site.trim_end_matches('/');
    let mut out = String::new();
    for a in activity {
        out.push_str(&format!("- {} [{}] \"{}\" (now {}) {site}/browse/{}\n", a.key, a.issue_type, a.summary, a.status, a.key));
        for (at, what) in &a.events {
            out.push_str(&format!("  - {at} {what}\n"));
        }
    }
    out
}

/// The user's Jira activity since `since`, newest-updated issue first.
pub fn fetch_activity(c: &Client, since: NaiveDate, me: &str) -> jira::Result<Vec<Activity>> {
    let raw = match c.search_with_changelog(&activity_jql(since, true), ACTIVITY_FIELDS) {
        Err(jira::Error::Other(_)) => c.search_with_changelog(&activity_jql(since, false), ACTIVITY_FIELDS)?,
        other => other?,
    };
    Ok(raw.iter().filter_map(|v| activity_of(v, me, since)).collect())
}

// ---------- The window's tickets ----------

/// The user's tickets beside the summary, as Jira-style cards: moved to a Done-category status since the window
/// opened, and in progress now. Kept in `Saved`, so the cards show from the cache before (or without) Jira.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Tickets {
    /// The window start the done list was read for; None until a read lands.
    pub since: Option<NaiveDate>,
    pub done: Vec<jira::Issue>,
    pub in_progress: Vec<jira::Issue>,
}

/// Equal when they serialise the same (`jira::Issue` has no `PartialEq`), so `Saved` compares whole.
impl PartialEq for Tickets {
    fn eq(&self, other: &Self) -> bool {
        serde_json::to_value(self).ok() == serde_json::to_value(other).ok()
    }
}

impl Tickets {
    /// Every card, in reading order: done, then in progress.
    pub fn all(&self) -> impl Iterator<Item = &jira::Issue> {
        self.done.iter().chain(&self.in_progress)
    }
}

/// The user's tickets that moved into a Done-category status on or after `since`, latest first.
pub fn done_jql(since: NaiveDate) -> String {
    format!("assignee = currentUser() AND statusCategory = Done AND statusCategoryChangedDate >= \"{since}\" ORDER BY statusCategoryChangedDate DESC")
}

/// The user's tickets in progress now, last updated first.
pub const IN_PROGRESS_JQL: &str = "assignee = currentUser() AND statusCategory = \"In Progress\" ORDER BY updated DESC";

/// The window the cards are for: the shown review's (so they sit beside its summary), today's before any review.
pub fn tickets_window(saved: &Saved, today: NaiveDate) -> NaiveDate {
    window_start(saved.ran_on.unwrap_or(today))
}

/// Both card lists for the window starting `since`. A ticket the two reads both return (it moved between them)
/// shows once, as done.
pub fn fetch_tickets(c: &Client, since: NaiveDate) -> jira::Result<Tickets> {
    let done = c.search(&done_jql(since), jira::LIST_FIELDS)?;
    let mut in_progress = c.search(IN_PROGRESS_JQL, jira::LIST_FIELDS)?;
    in_progress.retain(|i| !done.iter().any(|d| d.key == i.key));
    Ok(Tickets { since: Some(since), done, in_progress })
}

/// The issue's page on `site`.
pub fn browse_url(site: &str, key: &str) -> String {
    format!("{}/browse/{key}", site.trim_end_matches('/'))
}

/// `spans` with every whole-word mention of one of `keys` in plain text made a link to its issue page, so the
/// summary draws it as a key chip (`adf_view`). Links, code and placeholders are left alone.
pub fn link_keys(spans: Vec<crate::adf::Span>, site: &str, keys: &BTreeSet<String>) -> Vec<crate::adf::Span> {
    let mut out = Vec::with_capacity(spans.len());
    for sp in spans {
        if sp.link.is_some() || sp.code || sp.placeholder || keys.is_empty() {
            out.push(sp);
            continue;
        }
        let chars: Vec<char> = sp.text.chars().collect();
        let mut plain = String::new();
        let mut i = 0;
        while i < chars.len() {
            let starts = chars[i].is_ascii_uppercase() && (i == 0 || !(chars[i - 1].is_alphanumeric() || chars[i - 1] == '-'));
            if starts && let Some(n) = key_len(&chars[i..]) {
                let key: String = chars[i..i + n].iter().collect();
                if keys.contains(&key) {
                    if !plain.is_empty() {
                        out.push(crate::adf::Span { text: std::mem::take(&mut plain), ..sp.clone() });
                    }
                    out.push(crate::adf::Span { link: Some(browse_url(site, &key)), text: key, ..sp.clone() });
                    i += n;
                    continue;
                }
            }
            plain.push(chars[i]);
            i += 1;
        }
        if !plain.is_empty() {
            out.push(crate::adf::Span { text: plain, ..sp });
        }
    }
    out
}

/// The length of the issue key ("CI-1234": capitals and digits, a dash, digits) `chars` starts with, when it is a
/// whole word (no letter, digit or dash straight after it).
fn key_len(chars: &[char]) -> Option<usize> {
    let project = chars.iter().take_while(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || **c == '_').count();
    if project == 0 || chars.get(project) != Some(&'-') {
        return None;
    }
    let digits = chars[project + 1..].iter().take_while(|c| c.is_ascii_digit()).count();
    let end = project + 1 + digits;
    (digits > 0 && !chars.get(end).is_some_and(|c| c.is_alphanumeric() || *c == '-' || *c == '_')).then_some(end)
}

// ---------- The claude run ----------

/// The prompt with its placeholders filled. The activity goes in last, so text inside it is never substituted.
pub fn prompt(user: &str, today: NaiveDate, activity: &str) -> String {
    let start = window_start(today);
    PROMPT
        .replace("{{USER}}", user)
        .replace("{{WINDOW_START}}", &start.to_string())
        .replace("{{DAY_BEFORE_WINDOW}}", &(start - chrono::Days::new(1)).to_string())
        .replace("{{TODAY}}", &today.to_string())
        .replace("{{JIRA_ACTIVITY}}", activity.trim_end())
}

/// `claude`'s arguments: headless, JSON out, the prompt on stdin, read-only tools.
pub fn claude_args(model: &str) -> Vec<String> {
    let mut args: Vec<String> = ["-p", "--model", model, "--output-format", "json", "--allowedTools"].iter().map(|s| s.to_string()).collect();
    args.extend(ALLOWED_TOOLS.iter().map(|s| s.to_string()));
    args.push("--disallowedTools".into());
    args.extend(DISALLOWED_TOOLS.iter().map(|s| s.to_string()));
    args
}

/// Where a Dock-launched app looks for `claude` and `gh`, after PATH: the usual user, flox, nix and Homebrew bins.
/// `flox_runs` are the flox default environments' run folders (`~/.flox/run/*default*`). No folder appears twice.
pub fn search_dirs(path_env: Option<&std::ffi::OsStr>, home: &Path, user: &str, flox_runs: &[PathBuf]) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = path_env.map(|p| std::env::split_paths(p).collect()).unwrap_or_default();
    dirs.extend([home.join(".local/bin"), home.join(".claude/local"), home.join(".npm-global/bin")]);
    dirs.extend(flox_runs.iter().map(|r| r.join("bin")));
    dirs.extend([
        home.join(".nix-profile/bin"),
        PathBuf::from(format!("/etc/profiles/per-user/{user}/bin")),
        PathBuf::from("/run/current-system/sw/bin"),
        PathBuf::from("/nix/var/nix/profiles/default/bin"),
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/usr/bin"),
        PathBuf::from("/bin"),
    ]);
    let mut seen = BTreeSet::new();
    dirs.retain(|d| !d.as_os_str().is_empty() && seen.insert(d.clone()));
    dirs
}

/// The binary for `configured`: a path (with `~/` expanded) when it names one, else the first `dirs` entry holding it.
pub fn find_binary(configured: &str, home: &Path, dirs: &[PathBuf], is_file: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    let c = configured.trim();
    if c.is_empty() {
        return None;
    }
    if c.contains('/') {
        let p = match c.strip_prefix("~/") {
            Some(rest) => home.join(rest),
            None => PathBuf::from(c),
        };
        return is_file(&p).then_some(p);
    }
    dirs.iter().map(|d| d.join(c)).find(|p| is_file(p))
}

/// This machine's search folders.
fn local_dirs() -> Vec<PathBuf> {
    let home = dirs::home_dir().unwrap_or_default();
    let flox: Vec<PathBuf> = std::fs::read_dir(home.join(".flox/run"))
        .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().contains("default"))).collect())
        .unwrap_or_default();
    let user = std::env::var("USER").unwrap_or_default();
    search_dirs(std::env::var_os("PATH").as_deref(), &home, &user, &flox)
}

/// What a stopped review ends with.
pub const STOPPED: &str = "stopped";

std::thread_local! {
    /// The stop flag of the review running on this thread (`set_run_stop`).
    static RUN_STOP: std::cell::RefCell<Arc<AtomicBool>> = std::cell::RefCell::default();
}

/// Gives this thread's review its stop flag: once it is set, `run_review` kills its `claude` within a poll
/// (100 ms) and ends with `STOPPED`. Call it on the run's own thread, before the runner.
pub fn set_run_stop(flag: Arc<AtomicBool>) {
    RUN_STOP.with(|s| *s.borrow_mut() = flag);
}

/// This thread's review stop flag.
fn run_stop() -> Arc<AtomicBool> {
    RUN_STOP.with(|s| s.borrow().clone())
}

/// Whether this thread's review was asked to stop.
pub fn stop_requested() -> bool {
    run_stop().load(Ordering::Relaxed)
}

/// Runs `bin args` with `input` on stdin and `path` as PATH, and returns its stdout. Killed (and an error) after
/// `timeout`, or with the error `STOPPED` once `stop` is set; a non-zero exit is an error carrying the end of its
/// output.
pub fn run_with_timeout(bin: &Path, args: &[String], input: &str, path: &std::ffi::OsStr, timeout: Duration, stop: &AtomicBool) -> Result<String, String> {
    let name = bin.display().to_string();
    let mut child = std::process::Command::new(bin)
        .args(args)
        .env("PATH", path)
        // The home folder, when it exists (the nix build's HOME does not).
        .current_dir(dirs::home_dir().filter(|h| h.is_dir()).unwrap_or_else(std::env::temp_dir))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("couldn't start {name}: {e}"))?;
    let mut stdin = child.stdin.take();
    let input = input.to_string();
    let writer = std::thread::spawn(move || {
        if let Some(s) = stdin.as_mut() {
            let _ = s.write_all(input.as_bytes());
        }
    });
    let read = |mut r: Box<dyn Read + Send>| {
        std::thread::spawn(move || {
            let mut buf = String::new();
            let _ = r.read_to_string(&mut buf);
            buf
        })
    };
    let out = child.stdout.take().map(|s| read(Box::new(s)));
    let err = child.stderr.take().map(|s| read(Box::new(s)));
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break st,
            Ok(None) if stop.load(Ordering::Relaxed) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(STOPPED.into());
            }
            Ok(None) if start.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("{name} took longer than {} s and was stopped", timeout.as_secs()));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => return Err(format!("waiting for {name}: {e}")),
        }
    };
    let _ = writer.join();
    let stdout = out.and_then(|h| h.join().ok()).unwrap_or_default();
    let stderr = err.and_then(|h| h.join().ok()).unwrap_or_default();
    if !status.success() {
        let said = envelope_error(&stdout).unwrap_or_else(|| if stderr.trim().is_empty() { stdout.clone() } else { stderr.clone() });
        let tail: String = said.trim().chars().rev().take(500).collect::<Vec<_>>().into_iter().rev().collect();
        return Err(format!("{name} failed ({status}): {tail}"));
    }
    Ok(stdout)
}

/// Finds `claude`, runs the review, and parses its answer.
pub fn run_review(cfg: &crate::config::Standup, prompt: &str) -> Result<Review, String> {
    let stop = run_stop();
    if stop.load(Ordering::Relaxed) {
        return Err(STOPPED.into());
    }
    let dirs = local_dirs();
    let home = dirs::home_dir().unwrap_or_default();
    let bin = find_binary(&cfg.claude, &home, &dirs, |p| p.is_file())
        .ok_or_else(|| format!("couldn't find `{}`; set [standup] claude in the config to its full path", cfg.claude))?;
    // The binary's own folder first, so a configured path also finds its siblings (node, gh).
    let mut path_dirs: Vec<PathBuf> = bin.parent().map(Path::to_path_buf).into_iter().collect();
    path_dirs.extend(dirs);
    let path: OsString = std::env::join_paths(path_dirs).map_err(|e| e.to_string())?;
    let out = run_with_timeout(&bin, &claude_args(&cfg.model), prompt, &path, Duration::from_secs(cfg.timeout_secs.max(30)), &stop)?;
    parse_review(&out)
}

// ---------- The answer ----------

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Summary {
    pub done: Vec<String>,
    pub today: Vec<String>,
    pub blockers: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SourceKind {
    #[default]
    Github,
    Slack,
    Jira,
}

impl SourceKind {
    pub fn name(self) -> &'static str {
        match self {
            SourceKind::Github => "GitHub",
            SourceKind::Slack => "Slack",
            SourceKind::Jira => "Jira",
        }
    }
}

/// Where an added ticket goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Target {
    CurrentSprint,
    NextSprint,
    #[default]
    Backlog,
}

impl Target {
    pub fn name(self) -> &'static str {
        match self {
            Target::CurrentSprint => "Current sprint",
            Target::NextSprint => "Next sprint",
            Target::Backlog => "Backlog",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Suggestion {
    pub title: String,
    pub description: String,
    /// Task, Story or Bug.
    pub issue_type: String,
    pub source_kind: SourceKind,
    pub source_url: String,
    pub reason: String,
    /// 0 to 1.
    pub confidence: f32,
    pub suggested_target: Target,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Review {
    pub summary: Summary,
    pub suggestions: Vec<Suggestion>,
    pub notes: String,
}

/// The error a `claude --output-format json` envelope reports, if it reports one.
fn envelope_error(text: &str) -> Option<String> {
    let v: Value = serde_json::from_str(text.trim()).ok()?;
    let failed = v["is_error"].as_bool() == Some(true) || v["subtype"].as_str().is_some_and(|s| s.starts_with("error"));
    failed.then(|| v["result"].as_str().map(String::from).unwrap_or_else(|| format!("claude reported {}", v["subtype"].as_str().unwrap_or("an error"))))
}

/// The JSON object inside the model's text: a ```json fence's body, else the outermost `{ … }`.
fn json_object(text: &str) -> Option<&str> {
    let t = text.trim();
    let inner = match t.find("```") {
        Some(i) => {
            let after = &t[i + 3..];
            let body = after.find('\n').map_or(after, |n| &after[n + 1..]);
            body.find("```").map_or(body, |e| &body[..e])
        }
        None => t,
    };
    let (a, b) = (inner.find('{')?, inner.rfind('}')?);
    (a < b).then(|| &inner[a..=b])
}

fn strings(v: &Value) -> Vec<String> {
    match v {
        Value::Array(a) => a.iter().filter_map(|x| x.as_str()).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
        Value::String(s) if !s.trim().is_empty() => vec![s.trim().to_string()],
        _ => Vec::new(),
    }
}

fn norm(v: &Value) -> String {
    v.as_str().unwrap_or_default().trim().to_ascii_lowercase().replace([' ', '-'], "_")
}

fn target_of(v: &Value) -> Target {
    match norm(v).as_str() {
        "current_sprint" | "current" | "this_sprint" | "sprint" | "active_sprint" => Target::CurrentSprint,
        "next_sprint" | "next" | "future_sprint" => Target::NextSprint,
        _ => Target::Backlog,
    }
}

fn source_of(v: &Value, url: &str) -> SourceKind {
    match norm(v).as_str() {
        "slack" => SourceKind::Slack,
        "jira" => SourceKind::Jira,
        "github" | "git_hub" | "gh" => SourceKind::Github,
        _ if url.contains("slack.com") => SourceKind::Slack,
        _ if url.contains("atlassian.net") => SourceKind::Jira,
        _ => SourceKind::Github,
    }
}

fn issue_type_of(v: &Value) -> String {
    match norm(v).as_str() {
        "story" => "Story",
        "bug" => "Bug",
        _ => "Task",
    }
    .to_string()
}

fn confidence_of(v: &Value) -> f32 {
    let n = v.as_f64().or_else(|| v.as_str().and_then(|s| s.trim().trim_end_matches('%').parse().ok())).unwrap_or(0.5);
    // "80" or "80%" means a percentage.
    let n = if n > 1.0 { n / 100.0 } else { n };
    n.clamp(0.0, 1.0) as f32
}

/// The review from `claude`'s stdout: the `--output-format json` envelope's `result` (or the bare text), with any code
/// fence or prose around the JSON stripped. Unknown enum values fall back (Task, GitHub, Backlog); suggestions without
/// a title are dropped, and only the first of several with one dismiss key is kept.
pub fn parse_review(stdout: &str) -> Result<Review, String> {
    if stdout.trim().is_empty() {
        return Err("claude printed nothing".into());
    }
    if let Some(e) = envelope_error(stdout) {
        return Err(e);
    }
    let envelope: Option<Value> = serde_json::from_str(stdout.trim()).ok();
    let text = match envelope.as_ref().and_then(|v| v["result"].as_str()) {
        Some(r) => r.to_string(),
        None => stdout.to_string(),
    };
    let body = json_object(&text).ok_or_else(|| format!("no JSON object in claude's answer: {}", clip(text.trim(), 200)))?;
    let v: Value = serde_json::from_str(body).map_err(|e| format!("claude's answer is not valid JSON ({e}): {}", clip(body, 200)))?;
    if !v.is_object() {
        return Err("claude's answer is not a JSON object".into());
    }
    let s = &v["summary"];
    let first = |keys: &[&str]| keys.iter().map(|k| strings(&s[*k])).find(|l| !l.is_empty()).unwrap_or_default();
    let summary = Summary { done: first(&["done", "since_last_standup", "yesterday"]), today: first(&["today", "next"]), blockers: first(&["blockers"]) };
    let mut seen = BTreeSet::new();
    let mut suggestions = Vec::new();
    for x in v["suggestions"].as_array().map(Vec::as_slice).unwrap_or_default() {
        let text = |k: &str| x[k].as_str().unwrap_or_default().trim().to_string();
        let title = text("title");
        if title.is_empty() {
            continue;
        }
        let source_url = text("source_url");
        let sug = Suggestion {
            title,
            description: text("description"),
            issue_type: issue_type_of(&x["issue_type"]),
            source_kind: source_of(&x["source_kind"], &source_url),
            reason: text("reason"),
            confidence: confidence_of(&x["confidence"]),
            suggested_target: target_of(&x["suggested_target"]),
            source_url,
        };
        if seen.insert(dismiss_key(&sug)) {
            suggestions.push(sug);
        }
    }
    Ok(Review { summary, suggestions, notes: v["notes"].as_str().unwrap_or_default().trim().to_string() })
}

/// FNV-1a: a hash that stays the same across builds and Rust versions, so saved dismissals keep matching.
fn fnv64(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3))
}

/// What identifies a suggestion across reruns: its source URL (without a trailing slash), else a hash of its title.
pub fn dismiss_key(s: &Suggestion) -> String {
    let url = s.source_url.trim().trim_end_matches('/');
    if url.is_empty() { format!("title:{:016x}", fnv64(&s.title.trim().to_lowercase())) } else { format!("url:{url}") }
}

/// The standup as plain text for pasting into chat.
pub fn summary_text(s: &Summary, since: NaiveDate) -> String {
    let section = |title: &str, items: &[String]| {
        let body = if items.is_empty() { "- None".to_string() } else { items.iter().map(|i| format!("- {i}")).collect::<Vec<_>>().join("\n") };
        format!("{title}\n{body}")
    };
    [section(&format!("Since {}", day_label(since)), &s.done), section("Today", &s.today), section("Blockers", &s.blockers)].join("\n\n")
}

// ---------- Adding a ticket ----------

/// The sprint `target` lands in: the board's active sprint, or its first future one; None for the backlog.
pub fn pick_sprint(target: Target, sprints: &[Sprint]) -> Result<Option<&Sprint>, String> {
    let find = |state: &str| sprints.iter().find(|s| s.state == state);
    match target {
        Target::Backlog => Ok(None),
        Target::CurrentSprint => find("active").map(Some).ok_or_else(|| "the board has no active sprint".to_string()),
        Target::NextSprint => find("future").map(Some).ok_or_else(|| "the board has no future sprint".to_string()),
    }
}

/// Jira's create `fields` for a suggestion: in `project`, labelled `label`, with the source under the description.
pub fn create_fields(project: &str, label: &str, title: &str, s: &Suggestion) -> Value {
    let mut text = s.description.trim().to_string();
    if !s.source_url.trim().is_empty() {
        text.push_str(&format!("\n\nSource ({}): {}", s.source_kind.name(), s.source_url.trim()));
    }
    let mut fields = json!({
        "project": { "key": project },
        "summary": title.trim(),
        "issuetype": { "name": s.issue_type },
        "labels": [label],
    });
    if !text.trim().is_empty() {
        fields["description"] = crate::adf::text_doc(&text);
    }
    fields
}

/// Creates the ticket and moves it into `target`'s sprint. Ok((key, Ok(sprint name or None for the backlog))), or
/// Ok((key, Err(why))) when the ticket was created but could not be moved.
pub fn add_ticket(c: &Client, fields: Value, board: u64, target: Target) -> jira::Result<(String, Result<Option<String>, String>)> {
    jira::check_writable()?;
    let key = c.create_issue(fields)?;
    if target == Target::Backlog {
        return Ok((key, Ok(None)));
    }
    let placed = c.board_sprints(board, "active,future").map_err(|e| e.to_string()).and_then(|sprints| {
        let sprint = pick_sprint(target, &sprints)?.cloned().ok_or("no sprint")?;
        c.move_to_sprint(sprint.id, &[&key]).map_err(|e| e.to_string())?;
        Ok(Some(sprint.name))
    });
    Ok((key, placed))
}

// ---------- Showing a suggestion ----------

/// The simple Markdown the review writes (paragraphs, "- " and "1." lists, `#` headings, `> ` quotes, fenced code,
/// `inline code`, **bold**, *italic*, [links](url) and bare URLs) as the rich-text renderer's document. A short
/// line ending in ":" on its own ("Done when:") is a small heading. Nested lists are flattened.
pub fn description_doc(md: &str) -> crate::adf::Doc {
    use crate::adf::Block;
    let mut blocks: Vec<Block> = Vec::new();
    let mut para: Vec<String> = Vec::new();
    let mut quote: Vec<String> = Vec::new();
    // (ordered, items' text)
    let mut list: Option<(bool, Vec<String>)> = None;
    let mut code: Option<Vec<String>> = None;
    let mut blank = false;

    fn flush_para(para: &mut Vec<String>, blocks: &mut Vec<Block>) {
        if para.is_empty() {
            return;
        }
        let text = para.join(" ");
        para.clear();
        let t = text.trim();
        match t.strip_suffix(':') {
            Some(label) if t.chars().count() <= 60 && !label.contains(". ") && !label.trim().is_empty() => {
                blocks.push(Block::Heading { level: 3, spans: inline_spans(label.trim()) })
            }
            _ => blocks.push(Block::Paragraph(inline_spans(t))),
        }
    }
    fn flush_quote(quote: &mut Vec<String>, blocks: &mut Vec<Block>) {
        if !quote.is_empty() {
            blocks.push(Block::Quote(vec![Block::Paragraph(inline_spans(&quote.join(" ")))]));
            quote.clear();
        }
    }
    fn flush_list(list: &mut Option<(bool, Vec<String>)>, blocks: &mut Vec<Block>) {
        if let Some((ordered, items)) = list.take() {
            blocks.push(Block::List { ordered, items: items.iter().map(|t| vec![Block::Paragraph(inline_spans(t.trim()))]).collect() });
        }
    }

    for line in md.lines() {
        if let Some(lines) = code.as_mut() {
            if line.trim_start().starts_with("```") {
                blocks.push(Block::Code { lang: None, text: lines.join("\n") });
                code = None;
            } else {
                lines.push(line.to_string());
            }
            continue;
        }
        let t = line.trim();
        if t.starts_with("```") {
            flush_para(&mut para, &mut blocks);
            flush_quote(&mut quote, &mut blocks);
            flush_list(&mut list, &mut blocks);
            code = Some(Vec::new());
            continue;
        }
        if t.is_empty() {
            flush_para(&mut para, &mut blocks);
            flush_quote(&mut quote, &mut blocks);
            blank = true;
            continue;
        }
        let was_blank = std::mem::take(&mut blank);
        let hashes = t.chars().take_while(|c| *c == '#').count();
        if (1..=6).contains(&hashes) && t[hashes..].starts_with(' ') {
            flush_para(&mut para, &mut blocks);
            flush_quote(&mut quote, &mut blocks);
            flush_list(&mut list, &mut blocks);
            blocks.push(Block::Heading { level: hashes as u8, spans: inline_spans(t[hashes..].trim()) });
            continue;
        }
        if let Some(item) = list_item(t) {
            flush_para(&mut para, &mut blocks);
            flush_quote(&mut quote, &mut blocks);
            if list.as_ref().is_some_and(|(o, _)| *o != item.0) {
                flush_list(&mut list, &mut blocks);
            }
            list.get_or_insert_with(|| (item.0, Vec::new())).1.push(item.1.to_string());
            continue;
        }
        if let Some(q) = t.strip_prefix('>') {
            flush_para(&mut para, &mut blocks);
            flush_list(&mut list, &mut blocks);
            quote.push(q.trim().to_string());
            continue;
        }
        // An indented line right under an item continues it.
        if let Some((_, items)) = list.as_mut()
            && !was_blank
            && line.starts_with([' ', '\t'])
            && let Some(last) = items.last_mut()
        {
            last.push(' ');
            last.push_str(t);
            continue;
        }
        flush_list(&mut list, &mut blocks);
        flush_quote(&mut quote, &mut blocks);
        para.push(t.to_string());
    }
    if let Some(lines) = code {
        blocks.push(Block::Code { lang: None, text: lines.join("\n") });
    }
    flush_para(&mut para, &mut blocks);
    flush_quote(&mut quote, &mut blocks);
    flush_list(&mut list, &mut blocks);
    crate::adf::Doc { blocks }
}

/// A list item's (ordered, text): "- x", "* x", "+ x", "1. x", "1) x".
fn list_item(t: &str) -> Option<(bool, &str)> {
    if let Some(rest) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")).or_else(|| t.strip_prefix("+ ")) {
        return Some((false, rest));
    }
    let digits = t.chars().take_while(char::is_ascii_digit).count();
    if (1..=3).contains(&digits) {
        let rest = &t[digits..];
        if let Some(r) = rest.strip_prefix(". ").or_else(|| rest.strip_prefix(") ")) {
            return Some((true, r));
        }
    }
    None
}

/// One line of simple Markdown as spans: `code`, **bold**, *italic*, [text](url), and bare URLs as links.
pub fn inline_spans(text: &str) -> Vec<crate::adf::Span> {
    use crate::adf::Span;
    let mut out: Vec<Span> = Vec::new();
    let mut plain = String::new();
    let flush = |plain: &mut String, out: &mut Vec<Span>| {
        if !plain.is_empty() {
            out.push(Span { text: std::mem::take(plain), ..Default::default() });
        }
    };
    let mut rest = text;
    while let Some(c) = rest.chars().next() {
        let after = &rest[c.len_utf8()..];
        if c == '`'
            && let Some(end) = after.find('`')
            && end > 0
        {
            flush(&mut plain, &mut out);
            out.push(Span { text: after[..end].to_string(), code: true, ..Default::default() });
            rest = &after[end + 1..];
            continue;
        }
        if let Some(inner) = rest.strip_prefix("**")
            && let Some(end) = inner.find("**")
            && end > 0
            && !inner.starts_with(' ')
        {
            flush(&mut plain, &mut out);
            out.push(Span { text: inner[..end].to_string(), bold: true, ..Default::default() });
            rest = &inner[end + 2..];
            continue;
        }
        if c == '*'
            && !after.starts_with([' ', '*'])
            && let Some(end) = after.find('*')
            && end > 0
            && !after[..end].ends_with(' ')
        {
            flush(&mut plain, &mut out);
            out.push(Span { text: after[..end].to_string(), italic: true, ..Default::default() });
            rest = &after[end + 1..];
            continue;
        }
        if c == '['
            && let Some(close) = after.find("](")
            && let Some(paren) = after[close + 2..].find(')')
            && close > 0
        {
            let url = after[close + 2..close + 2 + paren].trim();
            if url.contains("://") {
                flush(&mut plain, &mut out);
                out.push(Span { text: after[..close].to_string(), link: Some(url.to_string()), ..Default::default() });
                rest = &after[close + 2 + paren + 1..];
                continue;
            }
        }
        plain.push(c);
        rest = after;
    }
    flush(&mut plain, &mut out);
    crate::adf::linkify(&out)
}

/// A list of plain lines (a summary section) as one bulleted list; mentions of `keys` link to their issues
/// (`link_keys`).
pub fn bullets_doc(items: &[String], site: &str, keys: &BTreeSet<String>) -> crate::adf::Doc {
    use crate::adf::Block;
    let items = items.iter().map(|t| vec![Block::Paragraph(link_keys(inline_spans(t.trim()), site, keys))]).collect();
    crate::adf::Doc { blocks: vec![Block::List { ordered: false, items }] }
}

/// A block's words as plain text: marks dropped, list items, tasks and table cells joined with "; ".
fn block_text(b: &crate::adf::Block) -> String {
    use crate::adf::Block;
    let spans = |s: &[crate::adf::Span]| s.iter().map(|x| x.text.as_str()).collect::<String>();
    let blocks = |bs: &[Block]| bs.iter().map(block_text).filter(|t| !t.is_empty()).collect::<Vec<_>>().join(" ");
    let text = match b {
        Block::Paragraph(s) | Block::Heading { spans: s, .. } => spans(s),
        Block::List { items, .. } => items.iter().map(|i| blocks(i)).filter(|t| !t.is_empty()).collect::<Vec<_>>().join("; "),
        Block::Tasks(items) => items.iter().map(|(_, i)| blocks(i)).filter(|t| !t.is_empty()).collect::<Vec<_>>().join("; "),
        Block::Code { text, .. } => text.clone(),
        Block::Quote(bs) | Block::Panel { blocks: bs, .. } => blocks(bs),
        Block::Table(rows) => rows.iter().flat_map(|r| r.iter().map(|c| spans(c))).filter(|t| !t.trim().is_empty()).collect::<Vec<_>>().join("; "),
        Block::Rule => String::new(),
    };
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A collapsed card's description in plain words: its first block that is not a heading (a heading only when
/// nothing else has words), and whether the description holds more than that.
pub fn preview_text(doc: &crate::adf::Doc) -> (String, bool) {
    use crate::adf::Block;
    let worded: Vec<&Block> = doc.blocks.iter().filter(|b| !block_text(b).is_empty()).collect();
    let Some(first) = worded.iter().find(|b| !matches!(b, Block::Heading { .. })).or(worded.first()) else { return (String::new(), false) };
    (block_text(first), worded.len() > 1)
}

/// The description as an opened card shows it: every heading at the smallest step (h3, body size), so none
/// reaches the card title.
pub fn card_doc(mut doc: crate::adf::Doc) -> crate::adf::Doc {
    for b in &mut doc.blocks {
        if let crate::adf::Block::Heading { level, .. } = b {
            *level = (*level).max(3);
        }
    }
    doc
}

/// Where to cut `chars` to keep at most `n`: at a word boundary (a hard cut at `n` when the first `n` hold none),
/// trailing spaces and punctuation dropped. None when the whole text fits.
fn word_cut(chars: &[char], n: usize) -> Option<usize> {
    if chars.len() <= n {
        return None;
    }
    let mut k = if chars[n].is_whitespace() { n } else { chars[..n].iter().rposition(|c| c.is_whitespace()).unwrap_or(n) };
    while k > 0 && (chars[k - 1].is_whitespace() || ",;:.-–—".contains(chars[k - 1])) {
        k -= 1;
    }
    Some(if k == 0 { n } else { k })
}

/// `text` cut to at most `n` characters at a word boundary, with "…" after it; the whole text when it fits.
pub fn clip_words(text: &str, n: usize) -> String {
    let chars: Vec<char> = text.trim().chars().collect();
    match word_cut(&chars, n) {
        Some(k) => chars[..k].iter().collect::<String>() + "…",
        None => chars.into_iter().collect(),
    }
}

/// A short name for a source link: "PR #129316", "Issue #12", "Commit 1a2b3c4", "Slack thread", "CI-9600", else the
/// host.
pub fn source_label(url: &str) -> String {
    let u = url.trim().trim_end_matches('/');
    let path = u.split_once("://").map_or(u, |(_, r)| r);
    let (host, rest) = path.split_once('/').unwrap_or((path, ""));
    let parts: Vec<&str> = rest.split(['/', '?', '#']).filter(|s| !s.is_empty()).collect();
    if host.ends_with("github.com") {
        for (i, w) in parts.iter().enumerate() {
            let next = parts.get(i + 1).copied().unwrap_or_default();
            match *w {
                "pull" | "pulls" if next.chars().all(|c| c.is_ascii_digit()) && !next.is_empty() => return format!("PR #{next}"),
                "issues" if next.chars().all(|c| c.is_ascii_digit()) && !next.is_empty() => return format!("Issue #{next}"),
                "commit" if !next.is_empty() => return format!("Commit {}", next.chars().take(7).collect::<String>()),
                _ => {}
            }
        }
        return "GitHub".to_string();
    }
    if host.ends_with("slack.com") {
        return "Slack thread".to_string();
    }
    if let Some(i) = parts.iter().position(|w| *w == "browse")
        && let Some(key) = parts.get(i + 1)
    {
        return key.to_string();
    }
    if host.is_empty() { u.to_string() } else { host.trim_start_matches("www.").to_string() }
}

// ---------- What is kept between launches ----------

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Saved {
    /// The day the review last started, so it runs by itself once per standup day.
    pub last_run: Option<NaiveDate>,
    /// The day the shown review ran on (its window is `window_start` of it).
    pub ran_on: Option<NaiveDate>,
    /// Unix seconds the shown review finished.
    pub ran_at: Option<i64>,
    pub review: Option<Review>,
    /// Dismiss keys of the suggestions the user dismissed; they never show again.
    pub dismissed: BTreeSet<String>,
    /// Dismiss key → the issue key it was added as.
    pub added: BTreeMap<String, String>,
    /// The window's done and in-progress tickets, last read.
    pub tickets: Tickets,
}

impl Saved {
    /// The shown review's suggestions minus the dismissed ones, in the review's order.
    pub fn visible(&self) -> Vec<&Suggestion> {
        self.review.as_ref().map(|r| r.suggestions.iter().filter(|s| !self.dismissed.contains(&dismiss_key(s))).collect()).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn window_runs_from_the_previous_standup_day() {
        // 2026-11-02 is a Monday.
        let cases = [
            ("2026-11-02", "2026-10-30"), // Mon → Fri, across the weekend
            ("2026-11-03", "2026-11-02"), // Tue → Mon
            ("2026-11-04", "2026-11-02"), // Wed → Mon
            ("2026-11-05", "2026-11-04"), // Thu → Wed
            ("2026-11-06", "2026-11-04"), // Fri → Wed
            ("2026-11-07", "2026-11-06"), // Sat → Fri
            ("2026-11-08", "2026-11-06"), // Sun → Fri
        ];
        for (today, start) in cases {
            assert_eq!(window_start(d(today)), d(start), "{today}");
        }
    }

    #[test]
    fn window_crosses_month_and_year_ends() {
        assert_eq!(window_start(d("2026-06-01")), d("2026-05-29"), "Mon 1 June → Fri 29 May");
        assert_eq!(window_start(d("2027-01-01")), d("2026-12-30"), "Fri 1 Jan → Wed 30 Dec");
        assert_eq!(window_start(d("2029-01-01")), d("2028-12-29"), "Mon 1 Jan → Fri 29 Dec");
        assert_eq!(window_start(d("2028-03-01")), d("2028-02-28"), "Wed 1 Mar of a leap year → Mon 28 Feb");
        assert_eq!(window_start(d("2026-03-01")), d("2026-02-27"), "Sun 1 Mar → Fri 27 Feb");
    }

    #[test]
    fn window_start_is_always_an_earlier_standup_day_within_three_days() {
        let mut day = d("2026-01-01");
        for _ in 0..800 {
            let start = window_start(day);
            assert!(is_standup_day(start), "{day} → {start}");
            assert!(start < day && (day - start).num_days() <= 3, "{day} → {start}");
            // Nothing between the start and today is a standup day.
            let mut between = start.succ_opt().unwrap();
            while between < day {
                assert!(!is_standup_day(between), "{day}: {between} is a standup day inside the window");
                between = between.succ_opt().unwrap();
            }
            day = day.succ_opt().unwrap();
        }
    }

    #[test]
    fn auto_runs_once_on_standup_days_only() {
        let (mon, tue) = (d("2026-11-02"), d("2026-11-03"));
        assert!(should_auto_run(mon, None));
        assert!(should_auto_run(mon, Some(d("2026-10-30"))));
        assert!(!should_auto_run(mon, Some(mon)), "already ran today");
        assert!(!should_auto_run(tue, None), "not a standup day");
        assert!(should_auto_run(d("2026-11-04"), Some(mon)), "Wednesday runs again");
        assert_eq!(day_label(d("2026-10-30")), "Fri Oct 30");
    }

    #[test]
    fn activity_jql_spans_assigned_reported_and_updated() {
        assert_eq!(
            activity_jql(d("2026-10-30"), true),
            "updated >= \"2026-10-30\" AND (assignee = currentUser() OR reporter = currentUser() OR issue in updatedBy(currentUser(), \"2026-10-30\")) ORDER BY updated DESC"
        );
        assert_eq!(
            activity_jql(d("2026-10-30"), false),
            "updated >= \"2026-10-30\" AND (assignee = currentUser() OR reporter = currentUser()) ORDER BY updated DESC"
        );
    }

    fn fixture() -> Value {
        serde_json::from_str(&std::fs::read_to_string("tests/fixtures/standup_activity.json").unwrap()).unwrap()
    }

    /// The fixture's timestamps are at noon UTC, so their local day is the same in every test time zone.
    #[test]
    fn activity_keeps_my_events_in_the_window() {
        let page = fixture();
        let issues = page["issues"].as_array().unwrap();
        let got: Vec<Activity> = issues.iter().filter_map(|v| activity_of(v, "me-1", d("2026-10-30"))).collect();
        assert_eq!(got.len(), 2, "CI-3 has nothing of mine in the window: {got:?}");
        let a = &got[0];
        assert_eq!((a.key.as_str(), a.summary.as_str(), a.issue_type.as_str(), a.status.as_str()), ("CI-1", "Fix rollouts", "Task", "In Progress"));
        let what: Vec<&str> = a.events.iter().map(|(_, w)| w.as_str()).collect();
        assert_eq!(what, ["created", "status New → In Progress", "commented: Opened the PR for this"], "oldest first; others' changes left out");
        assert_eq!(got[1].events.len(), 1);
        assert_eq!(got[1].events[0].1, "resolved as Done");
        let anyone: Vec<Activity> = issues.iter().filter_map(|v| activity_of(v, "", d("2026-10-30"))).collect();
        assert_eq!(anyone.len(), 3, "no account id yet: everyone's events count");
        let text = render_activity(&got, "https://x.atlassian.net/");
        assert!(text.starts_with("- CI-1 [Task] \"Fix rollouts\" (now In Progress) https://x.atlassian.net/browse/CI-1\n  - "), "{text}");
        assert!(text.contains(" status New → In Progress\n"));
        assert_eq!(render_activity(&[], "x"), "(No Jira activity found in the window.)");
    }

    #[test]
    fn fetch_activity_falls_back_when_updated_by_is_rejected() {
        let (base, rx) = crate::jira::tests::stub_server(vec![("POST", "/rest/api/3/search/jql", fixture())]);
        let login = crate::auth::Login { email: "a@b.c".into(), token: "t".into(), source: crate::auth::Source::Env };
        let c = Client::new(&base, &login);
        let got = fetch_activity(&c, d("2026-10-30"), "me-1").unwrap();
        assert_eq!(got.iter().map(|a| a.key.as_str()).collect::<Vec<_>>(), ["CI-1", "CI-2"]);
        let body: Value = serde_json::from_str(&rx.recv().unwrap().body).unwrap();
        assert!(body["jql"].as_str().unwrap().contains("updatedBy(currentUser()"));
        assert_eq!(body["expand"], "changelog");
        // A server that rejects the query: the fallback runs without updatedBy, and its error is returned.
        let (base, rx) = crate::jira::tests::stub_server(vec![]);
        let c = Client::new(&base, &login);
        assert!(fetch_activity(&c, d("2026-10-30"), "me-1").is_err());
        let first: Value = serde_json::from_str(&rx.recv().unwrap().body).unwrap();
        let second: Value = serde_json::from_str(&rx.recv().unwrap().body).unwrap();
        assert!(first["jql"].as_str().unwrap().contains("updatedBy"));
        assert!(!second["jql"].as_str().unwrap().contains("updatedBy"));
    }

    #[test]
    fn descriptions_become_rich_text() {
        use crate::adf::{Block, Span};
        let md = "First line\nsame paragraph with `code`.\n\nWhat the ticket should cover:\n- One **step**\n- Two, see https://x.y/z.\n  more of two\n\nDone when:\n1. A\n2) B\n\n> quoted\n\n```\nlet x = 1;\n```\n## Head";
        let doc = description_doc(md);
        let plain = |t: &str| Span { text: t.into(), ..Default::default() };
        assert_eq!(doc.blocks[0], Block::Paragraph(vec![plain("First line same paragraph with "), Span { code: true, ..plain("code") }, plain(".")]));
        assert_eq!(doc.blocks[1], Block::Heading { level: 3, spans: vec![plain("What the ticket should cover")] });
        let Block::List { ordered: false, items } = &doc.blocks[2] else { panic!("{:?}", doc.blocks[2]) };
        assert_eq!(items[0], vec![Block::Paragraph(vec![plain("One "), Span { bold: true, ..plain("step") }])]);
        let Block::Paragraph(two) = &items[1][0] else { panic!() };
        assert_eq!(two.iter().map(|s| s.text.as_str()).collect::<String>(), "Two, see https://x.y/z. more of two");
        assert!(two.iter().any(|s| s.link.as_deref() == Some("https://x.y/z")), "bare URLs are links: {two:?}");
        assert!(matches!(&doc.blocks[3], Block::Heading { .. }));
        assert!(matches!(&doc.blocks[4], Block::List { ordered: true, items } if items.len() == 2));
        assert!(matches!(&doc.blocks[5], Block::Quote(_)));
        assert_eq!(doc.blocks[6], Block::Code { lang: None, text: "let x = 1;".into() });
        assert_eq!(doc.blocks[7], Block::Heading { level: 2, spans: vec![plain("Head")] });
        assert_eq!(doc.blocks.len(), 8);
        // A sentence that ends in a colon is still a paragraph; a list after a paragraph with no blank line splits.
        let doc = description_doc("This is long enough to be a sentence. It ends with a colon:\n- a");
        assert!(matches!(&doc.blocks[0], Block::Paragraph(_)) && matches!(&doc.blocks[1], Block::List { .. }));
        assert!(description_doc("").blocks.is_empty());
    }

    #[test]
    fn inline_markdown() {
        let s = inline_spans("see [the PR](https://g/p/1) and *this* but not 2 * 3 or snake_case");
        let texts: Vec<_> = s.iter().map(|x| (x.text.as_str(), x.italic, x.link.as_deref())).collect();
        assert_eq!(
            texts,
            [
                ("see ", false, None),
                ("the PR", false, Some("https://g/p/1")),
                (" and ", false, None),
                ("this", true, None),
                (" but not 2 * 3 or snake_case", false, None)
            ]
        );
        assert_eq!(inline_spans("a ` b").iter().map(|x| x.text.as_str()).collect::<String>(), "a ` b", "an unclosed tick is text");
    }

    #[test]
    fn clips_end_at_a_word_with_an_ellipsis() {
        assert_eq!(clip_words("not fixing the failures", 18), "not fixing the…");
        assert_eq!(clip_words("not fixing the failures", 14), "not fixing the…", "a cut at a word's end keeps it");
        assert_eq!(clip_words("The PR, and more", 7), "The PR…", "no comma before the ellipsis");
        assert_eq!(clip_words("short", 10), "short");
        assert_eq!(clip_words("abcdefgh", 4), "abcd…", "no space: a hard cut");
    }

    #[test]
    fn previews_are_the_first_worded_block_in_plain_words() {
        let doc = description_doc("Fixes the **TIME_WAIT** port\nreuse.\n\n- a\n- b");
        assert_eq!(preview_text(&doc), ("Fixes the TIME_WAIT port reuse.".to_string(), true));
        assert_eq!(preview_text(&description_doc("Done when:\n- a `x`\n- b")), ("a x; b".to_string(), true), "a leading heading is skipped");
        assert_eq!(preview_text(&description_doc("Only.")), ("Only.".to_string(), false));
        assert_eq!(preview_text(&description_doc("## Head")), ("Head".to_string(), false));
        assert_eq!(preview_text(&description_doc("")), (String::new(), false));
        assert!(bullets_doc(&["x".into()], "https://x", &BTreeSet::new()).blocks.len() == 1);
    }

    #[test]
    fn opened_cards_show_headings_at_the_smallest_step() {
        let doc = card_doc(description_doc("# Big\n\n## Mid\n\nDone when:\n- a"));
        let levels: Vec<u8> = doc.blocks.iter().filter_map(|b| if let crate::adf::Block::Heading { level, .. } = b { Some(*level) } else { None }).collect();
        assert_eq!(levels, [3, 3, 3]);
    }

    #[test]
    fn source_links_get_short_names() {
        assert_eq!(source_label("https://github.com/acme-corp/widget-stores/pull/129316"), "PR #129316");
        assert_eq!(source_label("https://github.com/o/r/pull/12/files"), "PR #12");
        assert_eq!(source_label("https://github.com/o/r/issues/9"), "Issue #9");
        assert_eq!(source_label("https://github.com/o/r/commit/1a2b3c4d5e"), "Commit 1a2b3c4");
        assert_eq!(source_label("https://acme.slack.com/archives/C1/p123"), "Slack thread");
        assert_eq!(source_label("https://x.atlassian.net/browse/CI-9600"), "CI-9600");
        assert_eq!(source_label("https://www.example.com/a"), "example.com");
    }

    #[test]
    fn prompt_fills_every_placeholder() {
        let p = prompt("Sam", d("2026-11-02"), "- CI-1 {{USER}}\n");
        assert!(p.contains("Sam's standup"));
        assert!(p.contains("from 2026-10-30 00:00 local time up to now (2026-11-02)"));
        assert!(p.contains("after:2026-10-29"), "Slack's after: is exclusive");
        assert!(p.contains("- CI-1 {{USER}}"), "activity text is not substituted");
        let rest = p.replace("- CI-1 {{USER}}", "");
        assert!(!rest.contains("{{"), "no placeholder left: {rest}");
        assert!(p.contains("under 70 characters") && p.contains("simple Markdown"), "short titles, simple descriptions");
    }

    #[test]
    fn claude_command_line_is_headless_json_and_read_only() {
        let a = claude_args("claude-opus-5-5");
        assert_eq!(&a[..6], ["-p", "--model", "claude-opus-5-5", "--output-format", "json", "--allowedTools"]);
        let allowed = &a[6..6 + ALLOWED_TOOLS.len()];
        assert!(allowed.contains(&"Bash(gh search:*)".to_string()));
        assert!(allowed.contains(&"mcp__slack__slack_search_public".to_string()));
        assert!(allowed.contains(&"mcp__slack__slack_read_thread".to_string()));
        assert!(!allowed.iter().any(|t| t.contains("send") || t.contains("create") || t.contains("update")), "{allowed:?}");
        assert_eq!(a[6 + ALLOWED_TOOLS.len()], "--disallowedTools");
        assert!(a.ends_with(&DISALLOWED_TOOLS.iter().map(|s| s.to_string()).collect::<Vec<_>>()));
        assert!(DISALLOWED_TOOLS.contains(&"mcp__slack__slack_send_message"));
    }

    #[test]
    fn search_dirs_add_the_install_folders_once() {
        let home = Path::new("/Users/u");
        let path = std::env::join_paths(["/usr/bin", "/Users/u/.local/bin"]).unwrap();
        let dirs = search_dirs(Some(&path), home, "u", &[PathBuf::from("/Users/u/.flox/run/aarch64-darwin.default-run")]);
        assert_eq!(dirs[0], Path::new("/usr/bin"), "PATH first");
        assert_eq!(dirs.iter().filter(|d| d.ends_with(".local/bin")).count(), 1, "no duplicates");
        for want in ["/Users/u/.flox/run/aarch64-darwin.default-run/bin", "/Users/u/.nix-profile/bin", "/etc/profiles/per-user/u/bin", "/opt/homebrew/bin"] {
            assert!(dirs.contains(&PathBuf::from(want)), "{want} in {dirs:?}");
        }
        assert!(search_dirs(None, home, "u", &[]).contains(&PathBuf::from("/Users/u/.claude/local")));
    }

    #[test]
    fn find_binary_takes_a_path_or_searches() {
        let home = Path::new("/Users/u");
        let dirs = [PathBuf::from("/a"), PathBuf::from("/b")];
        let exists = |p: &Path| p == Path::new("/b/claude") || p == Path::new("/Users/u/bin/claude");
        assert_eq!(find_binary("claude", home, &dirs, exists), Some(PathBuf::from("/b/claude")));
        assert_eq!(find_binary("~/bin/claude", home, &dirs, exists), Some(PathBuf::from("/Users/u/bin/claude")));
        assert_eq!(find_binary("/nope/claude", home, &dirs, exists), None, "a path is not searched for");
        assert_eq!(find_binary("other", home, &dirs, exists), None);
        assert_eq!(find_binary("  ", home, &dirs, exists), None);
    }

    #[cfg(unix)]
    fn script(dir: &Path, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let p = dir.join("fake-claude");
        std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        p
    }

    #[test]
    #[cfg(unix)]
    fn run_with_timeout_feeds_stdin_and_reports_failures() {
        let dir = tempfile::tempdir().unwrap();
        // The child gets the PATH it is handed, not ours: the temp folder leads it so the echo proves which one it saw.
        // The rest is the test's own PATH, so `cat` and `sleep` resolve where /usr/bin has neither (the nix sandbox).
        let path = std::env::join_paths(std::iter::once(dir.path().to_path_buf()).chain(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())))
            .unwrap();
        let go = AtomicBool::new(false);
        let echo = script(dir.path(), "printf '%s|' \"$1\" \"$PATH\"; cat");
        let out = run_with_timeout(&echo, &["-p".into()], "the prompt", &path, Duration::from_secs(10), &go).unwrap();
        assert_eq!(out, format!("-p|{}|the prompt", path.to_string_lossy()));
        let fail = script(dir.path(), "echo boom >&2; exit 3");
        let e = run_with_timeout(&fail, &[], "", &path, Duration::from_secs(10), &go).unwrap_err();
        assert!(e.contains("boom") && e.contains('3'), "{e}");
        let env_err = script(dir.path(), "echo '{\"type\":\"result\",\"is_error\":true,\"result\":\"Credit balance is too low\"}'; exit 1");
        let e = run_with_timeout(&env_err, &[], "", &path, Duration::from_secs(10), &go).unwrap_err();
        assert!(e.contains("Credit balance is too low"), "{e}");
        let slow = script(dir.path(), "sleep 5");
        let t = Instant::now();
        let e = run_with_timeout(&slow, &[], "", &path, Duration::from_millis(300), &go).unwrap_err();
        assert!(e.contains("was stopped"), "{e}");
        assert!(t.elapsed() < Duration::from_secs(4), "killed, not waited out");
        // Stop kills the child at once, long before its timeout.
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            flag.store(true, Ordering::Relaxed);
        });
        let t = Instant::now();
        assert_eq!(run_with_timeout(&slow, &[], "", &path, Duration::from_secs(60), &stop).unwrap_err(), STOPPED);
        assert!(t.elapsed() < Duration::from_secs(4), "killed on stop");
        let e = run_with_timeout(&dir.path().join("missing"), &[], "", &path, Duration::from_secs(1), &go).unwrap_err();
        assert!(e.starts_with("couldn't start"), "{e}");
    }

    const ANSWER: &str = r#"{"summary":{"done":["Shipped rollout fix (CI-1)"],"today":["Land the logger fix"],"blockers":[]},
        "suggestions":[{"title":"Add Slack notifications to rollouts","description":"Post each rollout.","issue_type":"story",
        "source_kind":"github","source_url":"https://github.com/o/r/pull/7/","reason":"No ticket","confidence":0.8,
        "suggested_target":"current_sprint"}],"notes":"Slack was not available"}"#;

    #[test]
    fn parses_the_envelope_fences_and_bare_json() {
        let want = parse_review(ANSWER).unwrap();
        assert_eq!(want.summary.done, ["Shipped rollout fix (CI-1)"]);
        assert_eq!(want.summary.today, ["Land the logger fix"]);
        assert!(want.summary.blockers.is_empty());
        assert_eq!(want.notes, "Slack was not available");
        let s = &want.suggestions[0];
        assert_eq!((s.issue_type.as_str(), s.source_kind, s.suggested_target), ("Story", SourceKind::Github, Target::CurrentSprint));
        assert!((s.confidence - 0.8).abs() < 1e-6);
        let envelope = json!({ "type": "result", "subtype": "success", "is_error": false, "result": format!("Here you go:\n```json\n{ANSWER}\n```\nDone."), "session_id": "x" });
        assert_eq!(parse_review(&envelope.to_string()).unwrap(), want, "envelope with a fenced answer and prose");
        let envelope = json!({ "type": "result", "is_error": false, "result": ANSWER });
        assert_eq!(parse_review(&envelope.to_string()).unwrap(), want, "envelope with bare JSON");
        assert_eq!(parse_review(&format!("```\n{ANSWER}\n```")).unwrap(), want, "fence without a language");
        assert_eq!(parse_review(&format!("Sure! {ANSWER} Hope that helps")).unwrap(), want, "prose around it");
    }

    #[test]
    fn parse_rejects_what_is_not_a_review() {
        assert!(parse_review("").unwrap_err().contains("nothing"));
        assert!(parse_review("I could not do it").unwrap_err().contains("no JSON object"));
        assert!(parse_review("{ not json }").unwrap_err().contains("not valid JSON"));
        let failed = json!({ "type": "result", "subtype": "error_max_turns", "is_error": true });
        assert!(parse_review(&failed.to_string()).unwrap_err().contains("error_max_turns"));
        let failed = json!({ "type": "result", "is_error": true, "result": "Not logged in" });
        assert_eq!(parse_review(&failed.to_string()).unwrap_err(), "Not logged in");
    }

    #[test]
    fn parse_is_lenient_about_values() {
        let text = r#"{"summary":{"yesterday":"One thing","next":["Two"]},"suggestions":[
            {"title":"","source_url":"https://x"},
            {"title":"A","source_url":"https://x.slack.com/archives/C1/p1","source_kind":"chat","confidence":"85%","suggested_target":"Next Sprint","issue_type":"Epic"},
            {"title":"B","source_url":"https://x.slack.com/archives/C1/p1"},
            {"title":"C","confidence":7,"suggested_target":"someday"},
            {"title":"D","source_url":"https://x.atlassian.net/browse/CI-1","confidence":null}]}"#;
        let r = parse_review(text).unwrap();
        assert_eq!(r.summary.done, ["One thing"], "aliases and a bare string");
        assert_eq!(r.summary.today, ["Two"]);
        let titles: Vec<&str> = r.suggestions.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, ["A", "C", "D"], "untitled dropped; B repeats A's source");
        let (a, c, dd) = (&r.suggestions[0], &r.suggestions[1], &r.suggestions[2]);
        assert_eq!((a.source_kind, a.suggested_target, a.issue_type.as_str()), (SourceKind::Slack, Target::NextSprint, "Task"));
        assert!((a.confidence - 0.85).abs() < 1e-6);
        assert!((c.confidence - 0.07).abs() < 1e-6, "7 reads as 7%");
        assert_eq!(c.suggested_target, Target::Backlog);
        assert_eq!((dd.source_kind, dd.confidence), (SourceKind::Jira, 0.5));
        assert!(r.notes.is_empty());
    }

    #[test]
    fn dismiss_keys_follow_the_source_or_the_title() {
        let s = |title: &str, url: &str| Suggestion { title: title.into(), source_url: url.into(), ..Default::default() };
        assert_eq!(dismiss_key(&s("A", " https://github.com/o/r/pull/7/ ")), "url:https://github.com/o/r/pull/7");
        assert_eq!(dismiss_key(&s("A", "https://x")), dismiss_key(&s("Reworded", "https://x/")), "a reworded title keeps its source's key");
        assert_eq!(dismiss_key(&s("Fix it", "")), dismiss_key(&s(" fix IT ", "")));
        assert_ne!(dismiss_key(&s("Fix it", "")), dismiss_key(&s("Fix that", "")));
        assert_eq!(dismiss_key(&s("Fix it", "")), format!("title:{:016x}", fnv64("fix it")));
        assert_eq!(fnv64(""), 0xcbf2_9ce4_8422_2325, "FNV-1a's offset basis: stable across builds");
        assert_eq!(fnv64("a"), 0xaf63_dc4c_8601_ec8c);
    }

    #[test]
    fn saved_hides_dismissed_suggestions_and_round_trips() {
        let mut saved = Saved { review: Some(parse_review(ANSWER).unwrap()), ..Default::default() };
        assert_eq!(saved.visible().len(), 1);
        saved.dismissed.insert(dismiss_key(&saved.visible()[0].clone()));
        assert!(saved.visible().is_empty());
        saved.last_run = Some(d("2026-11-02"));
        let back: Saved = serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap();
        assert_eq!(back, saved);
        assert_eq!(serde_json::from_str::<Saved>("{}").unwrap(), Saved::default(), "an older cache file still loads");
    }

    #[test]
    fn summary_text_is_pasteable() {
        let s = Summary { done: vec!["A".into(), "B".into()], today: vec!["C".into()], blockers: vec![] };
        assert_eq!(summary_text(&s, d("2026-10-30")), "Since Fri Oct 30\n- A\n- B\n\nToday\n- C\n\nBlockers\n- None");
    }

    #[test]
    fn targets_pick_the_active_or_next_sprint() {
        let sp = |id, state: &str| Sprint { id, name: format!("S{id}"), state: state.into() };
        let sprints = [sp(1, "active"), sp(2, "future"), sp(3, "future")];
        assert_eq!(pick_sprint(Target::CurrentSprint, &sprints).unwrap().map(|s| s.id), Some(1));
        assert_eq!(pick_sprint(Target::NextSprint, &sprints).unwrap().map(|s| s.id), Some(2), "the first future sprint");
        assert!(pick_sprint(Target::Backlog, &sprints).unwrap().is_none());
        assert!(pick_sprint(Target::CurrentSprint, &sprints[1..]).unwrap_err().contains("no active sprint"));
        assert!(pick_sprint(Target::NextSprint, &sprints[..1]).unwrap_err().contains("no future sprint"));
    }

    #[test]
    fn create_fields_carry_project_label_and_source() {
        let s = Suggestion {
            title: "Old".into(),
            description: "Do the thing.".into(),
            issue_type: "Bug".into(),
            source_kind: SourceKind::Slack,
            source_url: "https://x.slack.com/p1".into(),
            ..Default::default()
        };
        let f = create_fields("CI", "PLAT", " New title ", &s);
        assert_eq!(f["project"]["key"], "CI");
        assert_eq!(f["summary"], "New title", "the edited title, trimmed");
        assert_eq!(f["issuetype"]["name"], "Bug");
        assert_eq!(f["labels"], json!(["PLAT"]));
        let text = crate::adf::to_plain_text(&f["description"]);
        assert!(text.contains("Do the thing.") && text.contains("Source (Slack): https://x.slack.com/p1"), "{text}");
        let bare = create_fields("CI", "PLAT", "T", &Suggestion { issue_type: "Task".into(), ..Default::default() });
        assert!(bare.get("description").is_none(), "no empty description");
    }

    fn client(base: &str) -> Client {
        Client::new(base, &crate::auth::Login { email: "a@b.c".into(), token: "t".into(), source: crate::auth::Source::Env })
    }

    #[test]
    fn add_ticket_creates_then_moves_into_the_sprint() {
        let _g = crate::jira::tests::WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let sprints = json!({ "values": [{ "id": 7, "name": "Sprint 92", "state": "active" }, { "id": 8, "name": "Sprint 93", "state": "future" }] });
        let (base, rx) = crate::jira::tests::stub_server(vec![
            ("POST", "/rest/api/3/issue", json!({ "key": "CI-42" })),
            ("GET", "/rest/agile/1.0/board/101/sprint", sprints),
            ("POST", "/rest/agile/1.0/sprint/8/issue", json!({})),
        ]);
        let c = client(&base);
        assert_eq!(add_ticket(&c, json!({}), 101, Target::NextSprint).unwrap(), ("CI-42".to_string(), Ok(Some("Sprint 93".to_string()))));
        let paths: Vec<String> = (0..3).map(|_| rx.recv().unwrap().path).collect();
        assert_eq!(paths[0], "/rest/api/3/issue");
        assert!(paths[1].starts_with("/rest/agile/1.0/board/101/sprint?state=active,future"));
        assert_eq!(paths[2], "/rest/agile/1.0/sprint/8/issue");
        assert_eq!(add_ticket(&c, json!({}), 101, Target::Backlog).unwrap(), ("CI-42".to_string(), Ok(None)), "a new ticket is already in the backlog");
        assert_eq!(rx.recv().unwrap().path, "/rest/api/3/issue");
        assert!(rx.try_recv().is_err(), "no sprint call for the backlog");
    }

    #[test]
    fn add_ticket_reports_a_failed_move_and_respects_read_only() {
        let _g = crate::jira::tests::WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (base, rx) = crate::jira::tests::stub_server(vec![
            ("POST", "/rest/api/3/issue", json!({ "key": "CI-43" })),
            ("GET", "/rest/agile/1.0/board/101/sprint", json!({ "values": [{ "id": 8, "name": "Sprint 93", "state": "future" }] })),
        ]);
        let c = client(&base);
        let (key, placed) = add_ticket(&c, json!({}), 101, Target::CurrentSprint).unwrap();
        assert_eq!(key, "CI-43");
        assert!(placed.unwrap_err().contains("no active sprint"), "created, but not moved");
        while rx.try_recv().is_ok() {}
        {
            let _ro = crate::jira::tests::ReadOnlyGuard::new();
            assert!(jira::is_read_only(&add_ticket(&c, json!({}), 101, Target::Backlog).unwrap_err()));
        }
        assert!(rx.try_recv().is_err(), "read-only sent nothing");
    }

    #[test]
    fn ticket_jql_and_window() {
        assert_eq!(
            done_jql(d("2026-10-30")),
            "assignee = currentUser() AND statusCategory = Done AND statusCategoryChangedDate >= \"2026-10-30\" ORDER BY statusCategoryChangedDate DESC"
        );
        assert!(IN_PROGRESS_JQL.contains("statusCategory = \"In Progress\""));
        let mon = d("2026-11-02");
        assert_eq!(tickets_window(&Saved::default(), mon), d("2026-10-30"), "no review: today's window");
        let ran = Saved { ran_on: Some(d("2026-11-04")), ..Default::default() };
        assert_eq!(tickets_window(&ran, mon), d("2026-11-02"), "the shown review's window");
    }

    #[test]
    fn fetch_tickets_reads_both_lists_and_shows_a_ticket_once() {
        let issue = |key: &str, cat: &str| json!({ "key": key, "fields": { "summary": key, "status": { "name": "S", "statusCategory": { "key": cat } } } });
        let page = json!({ "issues": [issue("CI-1", "done"), issue("CI-2", "indeterminate")] });
        let (base, rx) = crate::jira::tests::stub_server(vec![("POST", "/rest/api/3/search/jql", page)]);
        let login = crate::auth::Login { email: "a@b.c".into(), token: "t".into(), source: crate::auth::Source::Env };
        let c = Client::new(&base, &login);
        let t = fetch_tickets(&c, d("2026-10-30")).unwrap();
        assert_eq!(t.since, Some(d("2026-10-30")));
        assert_eq!(t.done.len(), 2);
        assert!(t.in_progress.is_empty(), "both came back as done first");
        let jqls: Vec<String> = rx.try_iter().map(|r| serde_json::from_str::<Value>(&r.body).unwrap()["jql"].as_str().unwrap().to_string()).collect();
        assert_eq!(jqls, [done_jql(d("2026-10-30")), IN_PROGRESS_JQL.to_string()]);
    }

    #[test]
    fn summary_keys_link_to_their_cards() {
        let keys: BTreeSet<String> = ["CI-12".to_string(), "DEMO-1".to_string()].into();
        let spans = link_keys(inline_spans("CI-12: shipped, see CI-123 and xCI-12 and `CI-12`, then DEMO-1."), "https://x/", &keys);
        let linked: Vec<(&str, Option<&str>)> = spans.iter().map(|s| (s.text.as_str(), s.link.as_deref())).collect();
        assert_eq!(
            linked,
            [
                ("CI-12", Some("https://x/browse/CI-12")),
                (": shipped, see CI-123 and xCI-12 and ", None),
                ("CI-12", None),
                (", then ", None),
                ("DEMO-1", Some("https://x/browse/DEMO-1")),
                (".", None),
            ]
        );
        assert!(spans[2].code, "code is left alone");
        assert_eq!(key_len(&"CI-9-x".chars().collect::<Vec<_>>()), None, "not a whole word");
        assert_eq!(key_len(&"GOLD-312)".chars().collect::<Vec<_>>()), Some(8));
        assert_eq!(browse_url("https://x/", "CI-1"), "https://x/browse/CI-1");
    }
}
