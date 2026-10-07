//! Jira Cloud client: REST v3 for issues, Agile 1.0 for rank and sprints.
//! Blocking; always called from background threads.

use crate::auth::Login;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

pub static READ_ONLY: AtomicBool = AtomicBool::new(false);

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone)]
pub enum Error {
    Unauthorized(String),
    RateLimited(String),
    Other(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Unauthorized(m) | Error::RateLimited(m) | Error::Other(m) => f.write_str(m),
        }
    }
}

impl From<ureq::Error> for Error {
    fn from(e: ureq::Error) -> Self {
        Error::Other(format!("Network error: {e}"))
    }
}

pub fn check_writable() -> Result<()> {
    if READ_ONLY.load(Ordering::Relaxed) {
        return Err(Error::Other("read-only mode, so nothing was sent to Jira".into()));
    }
    Ok(())
}

pub fn map_status(code: u16, body: &str) -> Error {
    let msg = extract_error(body).unwrap_or_else(|| body.chars().take(300).collect());
    match code {
        401 | 403 => Error::Unauthorized(format!("authentication failed ({code}): {msg}")),
        429 => Error::RateLimited(format!("rate limited (429): {msg}")),
        _ => Error::Other(format!("Jira returned {code}: {msg}")),
    }
}

/// Jira error bodies: {"errorMessages": [...], "errors": {field: msg}}.
fn extract_error(body: &str) -> Option<String> {
    let v: Value = serde_json::from_str(body).ok()?;
    let mut parts: Vec<String> = v["errorMessages"].as_array()?.iter().filter_map(|m| m.as_str().map(String::from)).collect();
    if let Some(errs) = v["errors"].as_object() {
        parts.extend(errs.iter().map(|(k, m)| format!("{k}: {}", m.as_str().unwrap_or(""))));
    }
    (!parts.is_empty()).then(|| parts.join("; "))
}

pub fn http_agent() -> ureq::Agent {
    let tls = ureq::tls::TlsConfig::builder().root_certs(ureq::tls::RootCerts::PlatformVerifier).build();
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(60)))
        .user_agent("jira-tickets")
        .tls_config(tls)
        .build()
        .into()
}

pub struct Client {
    base: String,
    agent: ureq::Agent,
    auth: String,
}

// ---------- Models ----------

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct User {
    pub account_id: String,
    pub display_name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Status {
    pub name: String,
    /// statusCategory.key: "new", "indeterminate", or "done".
    pub category: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Comment {
    pub id: String,
    pub author: String,
    pub created: String,
    pub body: Value,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Issue {
    pub key: String,
    pub id: String,
    pub summary: String,
    pub issue_type: String,
    pub status: Status,
    pub priority: String,
    pub labels: Vec<String>,
    pub assignee: Option<User>,
    pub reporter: Option<User>,
    pub parent_key: Option<String>,
    pub parent_id: Option<String>,
    pub parent_summary: Option<String>,
    pub created: String,
    pub updated: String,
    pub due_date: Option<String>,
    pub status_changed: Option<String>,
    pub rank: String,
    pub story_points: Option<f64>,
    pub sprint: Option<String>,
    pub description: Option<Value>,
    pub comments: Vec<Comment>,
    /// Any customfield_* that was requested, raw.
    pub custom: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Transition {
    pub id: String,
    pub name: String,
    pub to: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Resolution {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Sprint {
    pub id: u64,
    pub name: String,
    pub state: String,
}

pub const RANK_FIELD: &str = "customfield_10019";
pub const SPRINT_FIELD: &str = "customfield_10020";
pub const POINTS_FIELD: &str = "customfield_10016";

pub const LIST_FIELDS: &[&str] = &[
    "summary",
    "status",
    "assignee",
    "reporter",
    "parent",
    "labels",
    "priority",
    "issuetype",
    "created",
    "updated",
    "duedate",
    RANK_FIELD,
    "statuscategorychangedate",
    SPRINT_FIELD,
    POINTS_FIELD,
];

pub const DETAIL_FIELDS: &[&str] = &["description", "comment"];

fn str_at(v: &Value, path: &[&str]) -> Option<String> {
    let mut cur = v;
    for p in path {
        cur = cur.get(p)?;
    }
    cur.as_str().map(String::from)
}

fn user_at(v: &Value) -> Option<User> {
    let display_name = str_at(v, &["displayName"])?;
    Some(User { account_id: str_at(v, &["accountId"]).unwrap_or_default(), display_name })
}

pub fn parse_issue(v: &Value) -> Issue {
    let f = &v["fields"];
    let mut custom = BTreeMap::new();
    if let Some(obj) = f.as_object() {
        for (k, val) in obj {
            if k.starts_with("customfield_") && !val.is_null() {
                custom.insert(k.clone(), val.clone());
            }
        }
    }
    let sprint = f[SPRINT_FIELD]
        .as_array()
        .and_then(|a| a.iter().rfind(|s| s["state"].as_str() != Some("closed")).or(a.last()).and_then(|s| s["name"].as_str().map(String::from)));
    let comments = f["comment"]["comments"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|c| Comment {
                    id: str_at(c, &["id"]).unwrap_or_default(),
                    author: str_at(c, &["author", "displayName"]).unwrap_or_default(),
                    created: str_at(c, &["created"]).unwrap_or_default(),
                    body: c["body"].clone(),
                })
                .collect()
        })
        .unwrap_or_default();
    Issue {
        key: str_at(v, &["key"]).unwrap_or_default(),
        id: str_at(v, &["id"]).unwrap_or_default(),
        summary: str_at(f, &["summary"]).unwrap_or_default(),
        issue_type: str_at(f, &["issuetype", "name"]).unwrap_or_default(),
        status: Status {
            name: str_at(f, &["status", "name"]).unwrap_or_default(),
            category: str_at(f, &["status", "statusCategory", "key"]).unwrap_or_default(),
        },
        priority: str_at(f, &["priority", "name"]).unwrap_or_default(),
        labels: f["labels"].as_array().map(|a| a.iter().filter_map(|l| l.as_str().map(String::from)).collect()).unwrap_or_default(),
        assignee: user_at(&f["assignee"]),
        reporter: user_at(&f["reporter"]),
        parent_key: str_at(f, &["parent", "key"]),
        parent_id: str_at(f, &["parent", "id"]),
        parent_summary: str_at(f, &["parent", "fields", "summary"]),
        created: str_at(f, &["created"]).unwrap_or_default(),
        updated: str_at(f, &["updated"]).unwrap_or_default(),
        due_date: str_at(f, &["duedate"]),
        status_changed: str_at(f, &["statuscategorychangedate"]),
        rank: str_at(f, &[RANK_FIELD]).unwrap_or_default(),
        story_points: f[POINTS_FIELD].as_f64(),
        sprint,
        description: f.get("description").filter(|d| !d.is_null()).cloned(),
        comments,
        custom,
    }
}

// ---------- Requests ----------

impl Client {
    pub fn new(site: &str, login: &Login) -> Client {
        use base64_lite::encode;
        Client {
            base: site.trim_end_matches('/').to_string(),
            agent: http_agent(),
            auth: format!("Basic {}", encode(format!("{}:{}", login.email, login.token))),
        }
    }

    fn get(&self, path: &str) -> Result<Value> {
        let t = std::time::Instant::now();
        let mut resp = self.agent.get(format!("{}{path}", self.base)).header("Authorization", &self.auth).header("Accept", "application/json").call()?;
        let out = self.finish(resp.status().as_u16(), resp.body_mut().read_to_string()?);
        perf("GET", path, t);
        out
    }

    fn send(&self, method: &str, path: &str, body: &Value) -> Result<Value> {
        let t = std::time::Instant::now();
        let url = format!("{}{path}", self.base);
        let req = match method {
            "POST" => self.agent.post(&url),
            "PUT" => self.agent.put(&url),
            _ => unreachable!("method {method}"),
        };
        let mut resp =
            req.header("Authorization", &self.auth).header("Accept", "application/json").header("Content-Type", "application/json").send_json(body)?;
        let out = self.finish(resp.status().as_u16(), resp.body_mut().read_to_string()?);
        perf(method, path, t);
        out
    }

    fn finish(&self, code: u16, text: String) -> Result<Value> {
        if !(200..300).contains(&code) {
            return Err(map_status(code, &text));
        }
        if text.trim().is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_str(&text).map_err(|e| Error::Other(format!("bad JSON from Jira: {e}")))
    }

    pub fn myself(&self) -> Result<User> {
        let v = self.get("/rest/api/3/myself")?;
        user_at(&v).ok_or_else(|| Error::Other("myself: no displayName".into()))
    }

    /// All pages of a JQL search.
    pub fn search(&self, jql: &str, fields: &[&str]) -> Result<Vec<Issue>> {
        self.search_pages(jql, fields, usize::MAX)
    }

    /// Up to `max_pages` pages (tests use 1).
    pub fn search_pages(&self, jql: &str, fields: &[&str], max_pages: usize) -> Result<Vec<Issue>> {
        let mut out = Vec::new();
        let mut token: Option<String> = None;
        for _ in 0..max_pages {
            let mut body = json!({ "jql": jql, "fields": fields, "maxResults": 100 });
            if let Some(t) = &token {
                body["nextPageToken"] = json!(t);
            }
            let v = self.send("POST", "/rest/api/3/search/jql", &body)?;
            let page = v["issues"].as_array().map(Vec::as_slice).unwrap_or_default();
            out.extend(page.iter().map(parse_issue));
            let next = v["nextPageToken"].as_str().map(String::from);
            // Guard against a misbehaving server: stop on an empty page or a repeated token.
            if next.is_none() || page.is_empty() || next == token {
                break;
            }
            token = next;
        }
        Ok(out)
    }

    pub fn issue(&self, key: &str, extra_fields: &[&str]) -> Result<Issue> {
        let fields: Vec<&str> = LIST_FIELDS.iter().chain(DETAIL_FIELDS).chain(extra_fields).copied().collect();
        let v = self.get(&format!("/rest/api/3/issue/{key}?fields={}", fields.join(",")))?;
        Ok(parse_issue(&v))
    }

    pub fn transitions(&self, key: &str) -> Result<Vec<Transition>> {
        let v = self.get(&format!("/rest/api/3/issue/{key}/transitions"))?;
        Ok(v["transitions"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|t| Transition {
                        id: str_at(t, &["id"]).unwrap_or_default(),
                        name: str_at(t, &["name"]).unwrap_or_default(),
                        to: str_at(t, &["to", "name"]).unwrap_or_default(),
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    pub fn resolutions(&self) -> Result<Vec<Resolution>> {
        let v = self.get("/rest/api/3/resolution")?;
        Ok(v.as_array()
            .map(|a| a.iter().map(|r| Resolution { id: str_at(r, &["id"]).unwrap_or_default(), name: str_at(r, &["name"]).unwrap_or_default() }).collect())
            .unwrap_or_default())
    }

    pub fn assignable_users(&self, project: &str) -> Result<Vec<User>> {
        let v = self.get(&format!("/rest/api/3/user/assignable/search?project={project}&maxResults=100"))?;
        Ok(v.as_array().map(|a| a.iter().filter_map(user_at).collect()).unwrap_or_default())
    }

    /// `states` is a comma list: "active,future".
    pub fn board_sprints(&self, board_id: u64, states: &str) -> Result<Vec<Sprint>> {
        let v = self.get(&format!("/rest/agile/1.0/board/{board_id}/sprint?state={states}&maxResults=50"))?;
        Ok(v["values"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|s| Sprint {
                        id: s["id"].as_u64().unwrap_or(0),
                        name: str_at(s, &["name"]).unwrap_or_default(),
                        state: str_at(s, &["state"]).unwrap_or_default(),
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    pub fn transition(&self, key: &str, transition_id: &str, resolution_id: Option<&str>) -> Result<()> {
        check_writable()?;
        let mut body = json!({ "transition": { "id": transition_id } });
        if let Some(r) = resolution_id {
            body["fields"] = json!({ "resolution": { "id": r } });
        }
        self.send("POST", &format!("/rest/api/3/issue/{key}/transitions"), &body)?;
        Ok(())
    }

    pub fn rank(&self, key: &str, before: Option<&str>, after: Option<&str>) -> Result<()> {
        check_writable()?;
        let mut body = json!({ "issues": [key] });
        match (before, after) {
            (Some(b), _) => body["rankBeforeIssue"] = json!(b),
            (None, Some(a)) => body["rankAfterIssue"] = json!(a),
            (None, None) => return Err(Error::Other("rank: no neighbour given".into())),
        }
        self.send("PUT", "/rest/agile/1.0/issue/rank", &body)?;
        Ok(())
    }

    pub fn assign(&self, key: &str, account_id: Option<&str>) -> Result<()> {
        check_writable()?;
        self.send("PUT", &format!("/rest/api/3/issue/{key}/assignee"), &json!({ "accountId": account_id }))?;
        Ok(())
    }

    pub fn move_to_sprint(&self, sprint_id: u64, keys: &[&str]) -> Result<()> {
        check_writable()?;
        self.send("POST", &format!("/rest/agile/1.0/sprint/{sprint_id}/issue"), &json!({ "issues": keys }))?;
        Ok(())
    }

    pub fn move_to_backlog(&self, keys: &[&str]) -> Result<()> {
        check_writable()?;
        self.send("POST", "/rest/agile/1.0/backlog/issue", &json!({ "issues": keys }))?;
        Ok(())
    }

    pub fn update_fields(&self, key: &str, fields: Value) -> Result<()> {
        check_writable()?;
        self.send("PUT", &format!("/rest/api/3/issue/{key}"), &json!({ "fields": fields }))?;
        Ok(())
    }

    pub fn add_comment(&self, key: &str, text: &str) -> Result<()> {
        check_writable()?;
        self.send("POST", &format!("/rest/api/3/issue/{key}/comment"), &json!({ "body": crate::adf::text_doc(text) }))?;
        Ok(())
    }
}

pub fn needs_resolution(e: &Error) -> bool {
    e.to_string().to_ascii_lowercase().contains("resolution")
}

fn perf(method: &str, path: &str, start: std::time::Instant) {
    if std::env::var_os("JIRA_TICKETS_PERF").is_some() {
        eprintln!("{method} {path} {:.0} ms", start.elapsed().as_secs_f64() * 1000.0);
    }
}

/// Minimal base64 so the crate has no extra dependency for one header.
mod base64_lite {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    pub fn encode(s: String) -> String {
        let b = s.as_bytes();
        let mut out = String::with_capacity(b.len().div_ceil(3) * 4);
        for chunk in b.chunks(3) {
            let n = chunk.iter().enumerate().fold(0u32, |acc, (i, &x)| acc | (x as u32) << (16 - 8 * i));
            for i in 0..4 {
                if i <= chunk.len() {
                    out.push(T[((n >> (18 - 6 * i)) & 63) as usize] as char);
                } else {
                    out.push('=');
                }
            }
        }
        out
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::sync::mpsc::{Receiver, channel};

    pub static WRITE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Sets READ_ONLY for the guard's lifetime and always restores false on drop.
    struct ReadOnlyGuard;
    impl ReadOnlyGuard {
        fn new() -> Self {
            READ_ONLY.store(true, Ordering::Relaxed);
            ReadOnlyGuard
        }
    }
    impl Drop for ReadOnlyGuard {
        fn drop(&mut self) {
            READ_ONLY.store(false, Ordering::Relaxed);
        }
    }

    #[derive(Debug)]
    pub struct Recorded {
        pub method: String,
        pub path: String,
        pub body: String,
    }

    /// A tiny HTTP/1.1 server. Each route is (method, path prefix, JSON reply).
    /// Unmatched requests get 404. Answers one request per accepted connection.
    pub fn stub_server(routes: Vec<(&'static str, &'static str, serde_json::Value)>) -> (String, Receiver<Recorded>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut s) = stream else { break };
                let mut buf = Vec::new();
                let mut tmp = [0u8; 4096];
                let (mut head_end, mut content_len) = (None, 0usize);
                loop {
                    let n = match s.read(&mut tmp) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => n,
                    };
                    buf.extend_from_slice(&tmp[..n]);
                    if head_end.is_none()
                        && let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n")
                    {
                        head_end = Some(i + 4);
                        let head = String::from_utf8_lossy(&buf[..i]).to_string();
                        content_len = head
                            .lines()
                            .find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse().unwrap_or(0)))
                            .unwrap_or(0);
                    }
                    if let Some(h) = head_end
                        && buf.len() >= h + content_len
                    {
                        break;
                    }
                }
                let Some(h) = head_end else { continue };
                let head = String::from_utf8_lossy(&buf[..h]).to_string();
                let mut first = head.lines().next().unwrap_or("").split(' ');
                let method = first.next().unwrap_or("").to_string();
                let path = first.next().unwrap_or("").to_string();
                let body = String::from_utf8_lossy(&buf[h..h + content_len]).to_string();
                let reply = routes.iter().find(|(m, p, _)| *m == method && path.starts_with(p)).map(|(_, _, v)| v.to_string());
                let _ = tx.send(Recorded { method, path, body });
                let (code, text) = match reply {
                    Some(t) => ("200 OK", t),
                    None => ("404 Not Found", "{}".to_string()),
                };
                let _ = write!(s, "HTTP/1.1 {code}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}", text.len());
            }
        });
        (base, rx)
    }

    fn client(base: &str) -> Client {
        Client::new(base, &crate::auth::Login { email: "me@x.com".into(), token: "tok".into(), source: crate::auth::Source::Env })
    }

    fn fixture(name: &str) -> serde_json::Value {
        serde_json::from_str(&std::fs::read_to_string(format!("tests/fixtures/{name}")).unwrap()).unwrap()
    }

    #[test]
    fn parses_search_page() {
        let page = fixture("search_page.json");
        let issues: Vec<Issue> = page["issues"].as_array().unwrap().iter().map(parse_issue).collect();
        assert_eq!(issues.len(), 2);
        let a = &issues[0];
        assert_eq!(a.key, "CI-9001");
        assert_eq!(a.status.name, "New");
        assert_eq!(a.status.category, "new");
        assert_eq!(a.parent_key.as_deref(), Some("CI-79"));
        assert_eq!(a.parent_id.as_deref(), Some("10002"));
        assert_eq!(a.parent_summary.as_deref(), Some("Cluster Operations (ongoing)"));
        assert_eq!(a.sprint.as_deref(), Some("Sprint 92"));
        assert_eq!(a.story_points, Some(3.0));
        assert_eq!(a.rank, "0|i0abcd:");
        assert!(a.assignee.is_none());
        let b = &issues[1];
        assert_eq!(b.assignee.as_ref().unwrap().display_name, "Ada Lovelace");
        assert_eq!(b.due_date.as_deref(), Some("2026-12-31"));
        assert!(b.sprint.is_none());
    }

    #[test]
    fn parses_detail_with_description_and_comments() {
        let v = fixture("issue_detail.json");
        let i = parse_issue(&v);
        assert!(i.description.is_some());
        assert_eq!(i.comments.len(), 1);
        assert_eq!(i.comments[0].author, "Ada Lovelace");
    }

    #[test]
    fn search_posts_jql_and_follows_pages() {
        // The stub answers the same page (with a nextPageToken) to every call,
        // so cap at 2 pages and check the second request carried the token.
        let page = serde_json::json!({ "issues": fixture("search_page.json")["issues"], "nextPageToken": "t2" });
        let (base, rx) = stub_server(vec![("POST", "/rest/api/3/search/jql", page)]);
        let c = client(&base);
        let got = c.search_pages("project = CI", &["summary"], 2).unwrap();
        assert_eq!(got.len(), 4);
        let first: serde_json::Value = serde_json::from_str(&rx.recv().unwrap().body).unwrap();
        assert_eq!(first["jql"], "project = CI");
        assert_eq!(first["fields"][0], "summary");
        assert!(first.get("nextPageToken").is_none());
        let second: serde_json::Value = serde_json::from_str(&rx.recv().unwrap().body).unwrap();
        assert_eq!(second["nextPageToken"], "t2");
    }

    #[test]
    fn unauthorized_maps_to_error() {
        let (base, _rx) = stub_server(vec![]);
        let c = client(&base);
        // 404 from the stub is "Other"; a real 401 is tested through status mapping.
        assert!(matches!(c.myself(), Err(Error::Other(_))));
        assert!(matches!(map_status(401, "x"), Error::Unauthorized(_)));
        assert!(matches!(map_status(429, "x"), Error::RateLimited(_)));
    }

    #[test]
    fn search_stops_on_empty_page_with_token() {
        let page = serde_json::json!({ "issues": [], "nextPageToken": "same" });
        let (base, _rx) = stub_server(vec![("POST", "/rest/api/3/search/jql", page)]);
        let got = client(&base).search("project = CI", &["summary"]).unwrap();
        assert!(got.is_empty());
    }

    #[test]
    fn base64_known_vectors() {
        assert_eq!(base64_lite::encode("a".into()), "YQ==");
        assert_eq!(base64_lite::encode("ab".into()), "YWI=");
        assert_eq!(base64_lite::encode("abc".into()), "YWJj");
        assert_eq!(base64_lite::encode("me@x.com:tok".into()), "bWVAeC5jb206dG9r");
    }

    #[test]
    fn map_status_extracts_jira_errors() {
        let body = r#"{"errorMessages":["Issue does not exist"],"errors":{"resolution":"Resolution is required"}}"#;
        let msg = map_status(400, body).to_string();
        assert!(msg.contains("Issue does not exist"), "{msg}");
        assert!(msg.contains("resolution: Resolution is required"), "{msg}");
    }

    #[test]
    fn read_only_blocks_writes() {
        let _g = WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        {
            let _ro = ReadOnlyGuard::new();
            assert!(check_writable().is_err());
        }
        assert!(check_writable().is_ok());
    }

    #[test]
    fn transition_sends_id_and_optional_resolution() {
        let _g = WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (base, rx) = stub_server(vec![("POST", "/rest/api/3/issue/CI-1/transitions", serde_json::json!({}))]);
        let c = client(&base);
        c.transition("CI-1", "11", None).unwrap();
        let r = rx.recv().unwrap();
        let body: serde_json::Value = serde_json::from_str(&r.body).unwrap();
        assert_eq!(body["transition"]["id"], "11");
        assert!(body.get("fields").is_none());
        c.transition("CI-1", "31", Some("10000")).unwrap();
        let r = rx.recv().unwrap();
        let body: serde_json::Value = serde_json::from_str(&r.body).unwrap();
        assert_eq!(body["fields"]["resolution"]["id"], "10000");
    }

    #[test]
    fn rank_before_and_after() {
        let _g = WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (base, rx) = stub_server(vec![("PUT", "/rest/agile/1.0/issue/rank", serde_json::json!({}))]);
        let c = client(&base);
        c.rank("CI-1", Some("CI-2"), None).unwrap();
        let body: serde_json::Value = serde_json::from_str(&rx.recv().unwrap().body).unwrap();
        assert_eq!(body["issues"][0], "CI-1");
        assert_eq!(body["rankBeforeIssue"], "CI-2");
        assert!(body.get("rankAfterIssue").is_none());
        c.rank("CI-1", None, Some("CI-3")).unwrap();
        let body: serde_json::Value = serde_json::from_str(&rx.recv().unwrap().body).unwrap();
        assert_eq!(body["rankAfterIssue"], "CI-3");
        assert!(body.get("rankBeforeIssue").is_none());
        assert!(c.rank("CI-1", None, None).is_err());
        assert!(rx.try_recv().is_err(), "nothing was sent");
    }

    #[test]
    fn assign_and_unassign() {
        let _g = WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (base, rx) = stub_server(vec![("PUT", "/rest/api/3/issue/CI-1/assignee", serde_json::json!({}))]);
        let c = client(&base);
        c.assign("CI-1", Some("abc")).unwrap();
        assert_eq!(serde_json::from_str::<serde_json::Value>(&rx.recv().unwrap().body).unwrap()["accountId"], "abc");
        c.assign("CI-1", None).unwrap();
        assert!(serde_json::from_str::<serde_json::Value>(&rx.recv().unwrap().body).unwrap()["accountId"].is_null());
    }

    #[test]
    fn sprint_moves_and_fields_and_comment() {
        let _g = WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (base, rx) = stub_server(vec![
            ("POST", "/rest/agile/1.0/sprint/55/issue", serde_json::json!({})),
            ("POST", "/rest/agile/1.0/backlog/issue", serde_json::json!({})),
            ("PUT", "/rest/api/3/issue/CI-1", serde_json::json!({})),
            ("POST", "/rest/api/3/issue/CI-1/comment", serde_json::json!({ "id": "1" })),
        ]);
        let c = client(&base);
        c.move_to_sprint(55, &["CI-1"]).unwrap();
        assert_eq!(rx.recv().unwrap().path, "/rest/agile/1.0/sprint/55/issue");
        c.move_to_backlog(&["CI-1"]).unwrap();
        assert_eq!(rx.recv().unwrap().path, "/rest/agile/1.0/backlog/issue");
        c.update_fields("CI-1", serde_json::json!({ "labels": ["PLAT"] })).unwrap();
        let body: serde_json::Value = serde_json::from_str(&rx.recv().unwrap().body).unwrap();
        assert_eq!(body["fields"]["labels"][0], "PLAT");
        c.add_comment("CI-1", "hello\nworld").unwrap();
        let body: serde_json::Value = serde_json::from_str(&rx.recv().unwrap().body).unwrap();
        assert_eq!(body["body"]["content"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn writes_blocked_in_read_only() {
        let _g = WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (base, rx) = stub_server(vec![]);
        let c = client(&base);
        {
            let _ro = ReadOnlyGuard::new();
            let err = c.transition("CI-1", "1", None).unwrap_err();
            assert!(err.to_string().contains("read-only"), "{err}");
        }
        assert!(rx.try_recv().is_err(), "nothing was sent");
    }

    #[test]
    fn detects_resolution_requirement() {
        assert!(needs_resolution(&Error::Other("Jira returned 400: resolution: Resolution is required".into())));
        assert!(!needs_resolution(&Error::Other("boom".into())));
    }
}
