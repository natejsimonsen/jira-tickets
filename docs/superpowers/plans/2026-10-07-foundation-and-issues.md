# Jira Tickets, plan 1 of 3: foundation and the Issues view

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A signed-in egui desktop app that lists, searches, shows, and mutates Jira issues for the Platform team, with the config, auth, client, ADF renderer, store, and cache that plans 2 (Grooming) and 3 (Team, Roadmap) build on.

**Architecture:** One `App` struct owns an `mpsc` channel; every network call runs on a background thread and sends a `Msg` back. Views are pure functions over `&mut App` that push `Action`s, applied after the frame. All issue data lives in one `Store` keyed by issue key, serialised to a JSON disk cache. This mirrors `github-rs` file for file.

**Tech Stack:** Rust 2024, eframe/egui 0.36 (wgpu), ureq 3, serde/serde_json, toml, chrono, dirs, webbrowser. Toolchain via `nix develop` (no system cargo).

**Spec:** `docs/superpowers/specs/2026-10-07-jira-tickets-design.md`

## Global Constraints

- Binary and app id `jira-tickets`; window title "Jira Tickets".
- Rust edition 2024. Dependency versions: eframe/egui/egui_extras `0.36`, ureq `3`, toml `0.8`, serde `1`, serde_json `1`, chrono `0.4`, dirs `6`, webbrowser `1`.
- No system cargo: run every cargo command as `nix develop --command cargo <args>` from the repo root (alias below as `ndc`).
- rustfmt settings copied from github-rs: `max_width = 160`, `use_small_heuristics = "Max"`.
- The only subprocess the app may start is `op read` / `sh -c` for the config `api_token` reference.
- Every write is blocked when `JIRA_TICKETS_READONLY=1`.
- Conventional commits, no co-author trailers. If `git commit` fails on 1Password signing, stop and report BLOCKED; do not change signing config.
- 4-space indentation (rustfmt default).

Shorthand used in steps:

```sh
alias ndc='nix develop --command'
```

## File structure (plan 1)

| File | Responsibility |
|---|---|
| `Cargo.toml`, `rustfmt.toml`, `.gitignore`, `LICENSE` | Crate metadata, formatting, ignores, MIT |
| `flake.nix`, `nix/package.nix`, `nix/shell.nix` | Toolchain and package |
| `src/main.rs` | Opens the window; snapshot entry point |
| `src/config.rs` | `Config` TOML, defaults, example generation, `resolve_secret` |
| `src/auth.rs` | Token lookup order, save, forget |
| `src/jira.rs` | `Client`, `Error`, models, reads and writes |
| `src/adf.rs` | ADF JSON → `Doc` (blocks of styled spans) and plain text |
| `src/store.rs` | `Store`, `ListId`, JQL builders, projections, disk cache |
| `src/cache.rs` | Atomic JSON file cache |
| `src/theme.rs` | Palette (light/dark) from the mockup tokens, fonts |
| `src/app.rs` | `App`, `Msg`, `Action`, `Loaded`, spawn, shortcuts, apply |
| `src/views/mod.rs` | Header with view switcher, sign-in, toast, shared widgets |
| `src/views/issues.rs` | Issues list + detail |
| `src/views/adf_view.rs` | Paints a `Doc` into egui |
| `src/views/pickers.rs` | Transition, resolution, assign, sprint, edit dialogs |
| `src/snapshot.rs` | Offscreen screenshots (feature `snapshot`) |
| `scripts/bundle-macos.sh` | `.app` bundle |
| `tests/fixtures/*.json`, `tests/fixtures/*.md` | Recorded Jira responses, ADF samples |

Grooming, Team, and Roadmap views and `report.rs` / `roadmap.rs` are plans 2 and 3.

---

### Task 1: Crate scaffold and toolchain

**Files:**
- Create: `Cargo.toml`, `rustfmt.toml`, `.gitignore`, `LICENSE`, `flake.nix`, `nix/package.nix`, `nix/shell.nix`, `src/main.rs`

**Interfaces:**
- Produces: a crate that builds and runs `cargo test` under `nix develop`.

- [ ] **Step 1: Write Cargo.toml**

```toml
[package]
name = "jira-tickets"
version = "0.1.0"
edition = "2024"
description = "A fast desktop app for Jira tickets, grooming, and roadmaps"
license = "MIT"

[dependencies]
eframe = { version = "0.36", default-features = false, features = ["wgpu", "accesskit", "default_fonts", "persistence", "wayland", "x11"] }
egui = "0.36"
egui_extras = "0.36"
ureq = { version = "3", features = ["json", "platform-verifier"] }
serde = { version = "1", features = ["derive", "rc"] }
serde_json = "1"
toml = "0.8"
chrono = { version = "0.4", features = ["serde"] }
dirs = "6"
egui_kittest = { version = "0.36", features = ["wgpu", "eframe"], optional = true }

[target.'cfg(target_os = "macos")'.dependencies]
webbrowser = "1"

[dev-dependencies]
tempfile = "3"

[profile.release]
lto = "thin"
codegen-units = 1
strip = true

[profile.dev]
opt-level = 1

[profile.dev.package."*"]
opt-level = 3

[features]
snapshot = ["dep:egui_kittest"]
```

- [ ] **Step 2: Write rustfmt.toml, .gitignore, LICENSE**

`rustfmt.toml`:
```toml
max_width = 160
use_small_heuristics = "Max"
```

`.gitignore`:
```
/target
/result
.flox/run
.flox/cache
```

`LICENSE`: the MIT text with `Copyright (c) 2026 Shawn Sorichetti`.

- [ ] **Step 3: Write flake.nix and nix/**

`flake.nix`:
```nix
{
  description = "A fast desktop app for Jira tickets, grooming, and roadmaps";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  outputs = { self, nixpkgs }:
    let
      systems = [ "aarch64-darwin" "x86_64-darwin" "x86_64-linux" "aarch64-linux" ];
      forAll = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    in {
      packages = forAll (pkgs: rec {
        jira-tickets = pkgs.callPackage ./nix/package.nix { };
        default = jira-tickets;
      });
      apps = forAll (pkgs: {
        default = { type = "app"; program = "${self.packages.${pkgs.stdenv.hostPlatform.system}.default}/bin/jira-tickets"; };
      });
      devShells = forAll (pkgs: { default = pkgs.callPackage ./nix/shell.nix { }; });
      formatter = forAll (pkgs: pkgs.nixfmt);
    };
}
```

`nix/shell.nix`:
```nix
{ lib, stdenv, mkShell, cargo, rustc, clippy, rustfmt, rust-analyzer, libxkbcommon, wayland, libGL, vulkan-loader, xorg }:
let
  runtimeLibs = [ libxkbcommon wayland libGL vulkan-loader xorg.libX11 xorg.libXcursor xorg.libXi xorg.libXrandr xorg.libxcb ];
in
mkShell {
  packages = [ cargo rustc clippy rustfmt rust-analyzer ];
  LD_LIBRARY_PATH = lib.optionalString stdenv.hostPlatform.isLinux (lib.makeLibraryPath runtimeLibs);
}
```

`nix/package.nix`:
```nix
{ lib, stdenv, rustPlatform, libxkbcommon, wayland, libGL, vulkan-loader, xorg }:
let
  manifest = (lib.importTOML ../Cargo.toml).package;
  runtimeLibs = [ libxkbcommon wayland libGL vulkan-loader xorg.libX11 xorg.libXcursor xorg.libXi xorg.libXrandr xorg.libxcb ];
in
rustPlatform.buildRustPackage {
  pname = manifest.name;
  version = manifest.version;
  src = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [ ../Cargo.toml ../Cargo.lock ../src ];
  };
  cargoLock.lockFile = ../Cargo.lock;
  postFixup = lib.optionalString stdenv.hostPlatform.isLinux ''
    patchelf --add-rpath ${lib.makeLibraryPath runtimeLibs} $out/bin/jira-tickets
  '';
  meta = {
    description = manifest.description;
    license = lib.licenses.mit;
    mainProgram = "jira-tickets";
    platforms = lib.platforms.darwin ++ lib.platforms.linux;
  };
}
```

- [ ] **Step 4: Write a minimal src/main.rs**

```rust
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Jira Tickets")
            .with_app_id("jira-tickets")
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([900.0, 560.0]),
        run_and_return: false,
        ..Default::default()
    };
    eframe::run_native("jira-tickets", options, Box::new(|_cc| Ok(Box::new(Hello))))
}

struct Hello;

impl eframe::App for Hello {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label("Jira Tickets");
        });
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn builds() {}
}
```

- [ ] **Step 5: Enter the dev shell and build**

Run: `cd /Volumes/example/non-zip/jira-rs && nix develop --command cargo build 2>&1 | tail -5`
Expected: `Finished` line. First build downloads crates and compiles egui (several minutes). If `nix develop` reports flakes are disabled, run it as `nix --extra-experimental-features 'nix-command flakes' develop --command cargo build` and use that form for every later `ndc`.

- [ ] **Step 6: Run the tests**

Run: `ndc cargo test 2>&1 | tail -3`
Expected: `test result: ok. 1 passed`

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml Cargo.lock rustfmt.toml .gitignore LICENSE flake.nix flake.lock nix src
git commit -m "feat: scaffold jira-tickets crate with nix dev shell"
```

---

### Task 2: Config

**Files:**
- Create: `src/config.rs`
- Modify: `src/main.rs` (add `mod config;`)

**Interfaces:**
- Produces:
  - `pub struct Config { pub site: String, pub email: String, pub api_token: Option<String>, pub scope: Scope, pub grooming: Grooming, pub team: Team, pub issues: Issues, pub roadmap: Roadmap }` with `#[serde(default)]` on every section.
  - `pub struct Scope { pub projects: Vec<String>, pub project: String, pub team_label: String, pub board_id: u64, pub filter_id: u64 }`
  - `pub struct Grooming { pub status_new, status_planning, status_ready, status_backlog: String, pub planning_transition_id: String, pub planning_cap: usize, pub needs_detail_cap: usize, pub ready_soft_ceiling: usize, pub ready_hard_ceiling: usize, pub report_dir: String }`
  - `pub struct Team { pub members: Vec<String> }`
  - `pub struct Issues { pub columns: Vec<String>, pub filters: Vec<QuickFilter> }`, `pub struct QuickFilter { pub name: String, pub jql: String }`
  - `pub struct Roadmap { pub epics: Vec<String>, pub lanes: BTreeMap<String, String>, pub horizon_by_status: BTreeMap<String, String>, pub health_field, target_field, start_field, confidence_field, next_milestone_field, definition_of_done_field: Option<String> }`
  - `pub fn path() -> PathBuf` (`~/.config/jira-tickets/config.toml`, overridable by `JIRA_TICKETS_CONFIG`)
  - `pub fn load(path: &Path) -> Result<Config, String>`
  - `pub fn write_example(path: &Path) -> Result<(), String>`
  - `pub fn resolve_secret(value: &str) -> Result<String, String>`
  - `impl Config { pub fn validate(&self) -> Result<(), String>; pub fn report_dir(&self) -> PathBuf; pub fn browse_url(&self, key: &str) -> String }`

- [ ] **Step 1: Write the failing tests**

Append to `src/config.rs` (create the file with only the test module for now):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = r#"
site = "https://example.atlassian.net"
email = "me@example.com"
"#;

    #[test]
    fn minimal_config_gets_defaults() {
        let c: Config = toml::from_str(MINIMAL).unwrap();
        assert_eq!(c.scope.project, "CI");
        assert_eq!(c.scope.team_label, "PLAT");
        assert_eq!(c.scope.board_id, 101);
        assert_eq!(c.grooming.planning_transition_id, "11");
        assert_eq!(c.grooming.planning_cap, 10);
        assert_eq!(c.issues.columns, vec!["status", "labels", "parent.summary", "assignee"]);
        assert_eq!(c.roadmap.lanes.get("PLAT").map(String::as_str), Some("Platform"));
        assert_eq!(c.roadmap.horizon_by_status.get("In Progress").map(String::as_str), Some("now"));
        assert!(c.validate().is_ok());
    }

    #[test]
    fn validate_rejects_missing_site_and_email() {
        let c: Config = toml::from_str("site = \"\"\nemail = \"\"\n").unwrap();
        let e = c.validate().unwrap_err();
        assert!(e.contains("site"), "{e}");
    }

    #[test]
    fn example_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("config.toml");
        write_example(&p).unwrap();
        let c = load(&p);
        // The example has placeholder site/email, so load fails validation
        // but parsing must succeed.
        let text = std::fs::read_to_string(&p).unwrap();
        let parsed: Config = toml::from_str(&text).unwrap();
        assert_eq!(parsed.team.members.len(), 6);
        assert!(c.is_err());
    }

    #[test]
    fn resolve_secret_plain_and_shell() {
        assert_eq!(resolve_secret("abc").unwrap(), "abc");
        assert_eq!(resolve_secret("$(echo hi)").unwrap(), "hi");
    }

    #[test]
    fn browse_url_joins_site() {
        let c: Config = toml::from_str(MINIMAL).unwrap();
        assert_eq!(c.browse_url("CI-1"), "https://example.atlassian.net/browse/CI-1");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Add `mod config;` to `src/main.rs`. Run: `ndc cargo test config 2>&1 | tail -5`
Expected: compile errors, `Config` not found.

- [ ] **Step 3: Write the implementation**

Put this above the test module in `src/config.rs`:

```rust
//! The TOML config at ~/.config/jira-tickets/config.toml: site, scope,
//! grooming statuses, team roster, columns, roadmap field mapping.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub site: String,
    pub email: String,
    pub api_token: Option<String>,
    pub scope: Scope,
    pub grooming: Grooming,
    pub team: Team,
    pub issues: Issues,
    pub roadmap: Roadmap,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Scope {
    pub projects: Vec<String>,
    pub project: String,
    pub team_label: String,
    pub board_id: u64,
    pub filter_id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Grooming {
    pub status_new: String,
    pub status_planning: String,
    pub status_ready: String,
    pub status_backlog: String,
    pub planning_transition_id: String,
    pub planning_cap: usize,
    pub needs_detail_cap: usize,
    pub ready_soft_ceiling: usize,
    pub ready_hard_ceiling: usize,
    pub report_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Team {
    pub members: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Issues {
    pub columns: Vec<String>,
    pub filters: Vec<QuickFilter>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QuickFilter {
    pub name: String,
    pub jql: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Roadmap {
    pub epics: Vec<String>,
    pub lanes: BTreeMap<String, String>,
    pub horizon_by_status: BTreeMap<String, String>,
    pub health_field: Option<String>,
    pub target_field: Option<String>,
    pub start_field: Option<String>,
    pub confidence_field: Option<String>,
    pub next_milestone_field: Option<String>,
    pub definition_of_done_field: Option<String>,
}

fn s(v: &str) -> String {
    v.to_string()
}

impl Default for Config {
    fn default() -> Self {
        Config {
            site: String::new(),
            email: String::new(),
            api_token: None,
            scope: Scope::default(),
            grooming: Grooming::default(),
            team: Team::default(),
            issues: Issues::default(),
            roadmap: Roadmap::default(),
        }
    }
}

impl Default for Scope {
    fn default() -> Self {
        Scope { projects: vec![s("CI"), s("OPS")], project: s("CI"), team_label: s("PLAT"), board_id: 101, filter_id: 201 }
    }
}

impl Default for Grooming {
    fn default() -> Self {
        Grooming {
            status_new: s("New"),
            status_planning: s("Planning"),
            status_ready: s("Ready for Work"),
            status_backlog: s("Backlog"),
            planning_transition_id: s("11"),
            planning_cap: 10,
            needs_detail_cap: 10,
            ready_soft_ceiling: 10,
            ready_hard_ceiling: 15,
            report_dir: s("~/Documents/grooming-reports"),
        }
    }
}

impl Default for Team {
    fn default() -> Self {
        Team { members: ["barbara", "edsger.dijkstra", "ada.lovelace", "grace", "sam", "alan"].map(s).to_vec() }
    }
}

impl Default for Issues {
    fn default() -> Self {
        Issues {
            columns: ["status", "labels", "parent.summary", "assignee"].map(s).to_vec(),
            filters: vec![
                QuickFilter { name: s("Only Opened"), jql: s("status not in (Closed)") },
                QuickFilter { name: s("To Do"), jql: s("statusCategory = \"To Do\"") },
            ],
        }
    }
}

impl Default for Roadmap {
    fn default() -> Self {
        Roadmap {
            epics: Vec::new(),
            lanes: BTreeMap::from([(s("PLAT"), s("Platform"))]),
            horizon_by_status: BTreeMap::from([
                (s("In Progress"), s("now")),
                (s("Ready for Work"), s("next")),
                (s("Planning"), s("next")),
                (s("New"), s("later")),
                (s("Backlog"), s("later")),
            ]),
            health_field: None,
            target_field: None,
            start_field: None,
            confidence_field: None,
            next_milestone_field: None,
            definition_of_done_field: None,
        }
    }
}

/// `JIRA_TICKETS_CONFIG` overrides the location, for tests and snapshots.
pub fn path() -> PathBuf {
    if let Ok(p) = std::env::var("JIRA_TICKETS_CONFIG") {
        return PathBuf::from(p);
    }
    dirs::home_dir().unwrap_or_default().join(".config").join("jira-tickets").join("config.toml")
}

pub fn load(path: &Path) -> Result<Config, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    let cfg: Config = toml::from_str(&text).map_err(|e| format!("parsing {}: {e}", path.display()))?;
    cfg.validate()?;
    Ok(cfg)
}

impl Config {
    pub fn validate(&self) -> Result<(), String> {
        if self.site.trim().is_empty() || self.site.contains("yourcompany") {
            return Err("config: site is required (e.g. https://yourcompany.atlassian.net)".into());
        }
        if self.email.trim().is_empty() || self.email.contains("example.com") {
            return Err("config: email is required".into());
        }
        if self.scope.project.trim().is_empty() {
            return Err("config: scope.project is required".into());
        }
        Ok(())
    }

    pub fn report_dir(&self) -> PathBuf {
        expand_home(&self.grooming.report_dir)
    }

    pub fn browse_url(&self, key: &str) -> String {
        format!("{}/browse/{key}", self.site.trim_end_matches('/'))
    }
}

fn expand_home(p: &str) -> PathBuf {
    match p.strip_prefix("~/") {
        Some(rest) => dirs::home_dir().unwrap_or_default().join(rest),
        None => PathBuf::from(p),
    }
}

/// `op://vault/item/field` runs `op read`; `$(cmd)` runs `sh -c cmd`;
/// anything else is returned as is.
pub fn resolve_secret(value: &str) -> Result<String, String> {
    let v = value.trim();
    if v.starts_with("op://") {
        return run("op", &["read", v]);
    }
    if let Some(inner) = v.strip_prefix("$(").and_then(|r| r.strip_suffix(')')) {
        return run("sh", &["-c", inner]);
    }
    Ok(v.to_string())
}

fn run(name: &str, args: &[&str]) -> Result<String, String> {
    let out = std::process::Command::new(name).args(args).output().map_err(|e| format!("running {name}: {e}"))?;
    if !out.status.success() {
        return Err(format!("{name} failed: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub fn write_example(path: &Path) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, EXAMPLE).map_err(|e| e.to_string())
}

pub const EXAMPLE: &str = r#"# Jira Tickets configuration

site = "https://yourcompany.atlassian.net"
email = "you@example.com"
# Optional. Plain token, "op://vault/item/field", or "$(command)". Used only
# when no token is found in JIRA_API_TOKEN or the saved token file.
# api_token = "op://Private/Jira API Token/credential"

[scope]
projects = ["CI", "OPS"]   # projects searched for team members' work
project = "CI"              # project the team label lives in
team_label = "PLAT"
board_id = 101
filter_id = 201

[grooming]
status_new = "New"
status_planning = "Planning"
status_ready = "Ready for Work"
status_backlog = "Backlog"
planning_transition_id = "11"
planning_cap = 10
needs_detail_cap = 10
ready_soft_ceiling = 10
ready_hard_ceiling = 15
report_dir = "~/Documents/grooming-reports"

[team]
# Jira display names or account ids, in column order. Replace these SSO
# usernames with the names Jira shows, or the Team view says "no tickets found".
members = ["barbara", "edsger.dijkstra", "ada.lovelace", "grace", "sam", "alan"]

[issues]
columns = ["status", "labels", "parent.summary", "assignee"]
[[issues.filters]]
name = "Only Opened"
jql = "status not in (Closed)"
[[issues.filters]]
name = "To Do"
jql = 'statusCategory = "To Do"'

[roadmap]
# epics = ["CI-1860"]            # optional allowlist
lanes = { PLAT = "Platform" }
horizon_by_status = { "In Progress" = "now", "Ready for Work" = "next", "Planning" = "next", "New" = "later", "Backlog" = "later" }
# health_field = "customfield_XXXXX"
# target_field = "customfield_XXXXX"   # default: duedate
# start_field = "customfield_XXXXX"
# confidence_field = "customfield_XXXXX"
# next_milestone_field = "customfield_XXXXX"
# definition_of_done_field = "customfield_XXXXX"
"#;
```

- [ ] **Step 4: Run tests**

Run: `ndc cargo test config 2>&1 | tail -5`
Expected: `5 passed`

- [ ] **Step 5: Commit**

```bash
git add src/config.rs src/main.rs
git commit -m "feat: add TOML config with defaults and example generation"
```

---

### Task 3: Auth

**Files:**
- Create: `src/auth.rs`
- Modify: `src/main.rs` (add `mod auth;`)

**Interfaces:**
- Consumes: `config::resolve_secret`.
- Produces:
  - `pub enum Source { Env, SavedFile, ConfigRef, Pasted }` with `pub fn describe(self) -> &'static str`.
  - `pub struct Login { pub email: String, pub token: String, pub source: Source }`
  - `pub fn find_login(cfg_email: &str, cfg_token_ref: Option<&str>) -> Option<Login>`: order Env → SavedFile → ConfigRef. Saves to file after ConfigRef.
  - `pub fn save_token(token: &str) -> Result<(), String>`, `pub fn forget_token()`.
  - `pub fn token_path() -> Option<PathBuf>` (`dirs::config_dir()/jira-tickets/token`, overridable by `JIRA_TICKETS_TOKEN_FILE`).

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // Tests set env vars, so they must not run in parallel with each other.
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn with_token_file<R>(f: impl FnOnce(&std::path::Path) -> R) -> R {
        let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("token");
        unsafe {
            std::env::set_var("JIRA_TICKETS_TOKEN_FILE", &p);
            std::env::remove_var("JIRA_API_TOKEN");
        }
        let r = f(&p);
        unsafe { std::env::remove_var("JIRA_TICKETS_TOKEN_FILE") };
        r
    }

    #[test]
    fn env_wins() {
        with_token_file(|_| {
            unsafe { std::env::set_var("JIRA_API_TOKEN", "env-tok") };
            let l = find_login("me@x.com", Some("plain-cfg")).unwrap();
            assert_eq!(l.token, "env-tok");
            assert!(matches!(l.source, Source::Env));
            unsafe { std::env::remove_var("JIRA_API_TOKEN") };
        });
    }

    #[test]
    fn saved_file_before_config() {
        with_token_file(|p| {
            std::fs::write(p, "file-tok\n").unwrap();
            let l = find_login("me@x.com", Some("plain-cfg")).unwrap();
            assert_eq!(l.token, "file-tok");
            assert!(matches!(l.source, Source::SavedFile));
        });
    }

    #[test]
    fn config_ref_is_saved() {
        with_token_file(|p| {
            let l = find_login("me@x.com", Some("plain-cfg")).unwrap();
            assert_eq!(l.token, "plain-cfg");
            assert!(matches!(l.source, Source::ConfigRef));
            assert_eq!(std::fs::read_to_string(p).unwrap(), "plain-cfg");
        });
    }

    #[test]
    fn nothing_found() {
        with_token_file(|_| assert!(find_login("me@x.com", None).is_none()));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Add `mod auth;` to `src/main.rs`. Run: `ndc cargo test auth 2>&1 | tail -5`
Expected: compile error, `find_login` not found.

- [ ] **Step 3: Write the implementation**

```rust
//! Finds the Jira API token. Order: JIRA_API_TOKEN, the saved token file,
//! the config's api_token reference (which may run `op read`), then the
//! paste prompt in the window (handled by the app).

use std::io::Write;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug)]
pub enum Source {
    Env,
    SavedFile,
    ConfigRef,
    Pasted,
}

impl Source {
    pub fn describe(self) -> &'static str {
        match self {
            Source::Env => "JIRA_API_TOKEN",
            Source::SavedFile => "saved token",
            Source::ConfigRef => "config api_token",
            Source::Pasted => "pasted token",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Login {
    pub email: String,
    pub token: String,
    pub source: Source,
}

pub fn find_login(cfg_email: &str, cfg_token_ref: Option<&str>) -> Option<Login> {
    let email = std::env::var("JIRA_EMAIL").ok().filter(|e| !e.trim().is_empty()).unwrap_or_else(|| cfg_email.to_string());
    if let Ok(t) = std::env::var("JIRA_API_TOKEN")
        && !t.trim().is_empty()
    {
        return Some(Login { email, token: t.trim().to_string(), source: Source::Env });
    }
    if let Some(t) = read_token_file() {
        return Some(Login { email, token: t, source: Source::SavedFile });
    }
    if let Some(r) = cfg_token_ref
        && let Ok(t) = crate::config::resolve_secret(r)
        && !t.is_empty()
    {
        let _ = save_token(&t);
        return Some(Login { email, token: t, source: Source::ConfigRef });
    }
    None
}

/// macOS: ~/Library/Application Support/jira-tickets/token
/// Linux: ~/.config/jira-tickets/token
pub fn token_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("JIRA_TICKETS_TOKEN_FILE") {
        return Some(PathBuf::from(p));
    }
    Some(dirs::config_dir()?.join("jira-tickets").join("token"))
}

fn read_token_file() -> Option<String> {
    let t = std::fs::read_to_string(token_path()?).ok()?;
    let t = t.trim();
    (!t.is_empty()).then(|| t.to_string())
}

/// 0600 on unix; the directory 0700.
pub fn save_token(token: &str) -> Result<(), String> {
    let path = token_path().ok_or("no config folder")?;
    let dir = path.parent().ok_or("no parent folder")?;
    let mut db = std::fs::DirBuilder::new();
    db.recursive(true);
    let mut oo = std::fs::OpenOptions::new();
    oo.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
        db.mode(0o700);
        oo.mode(0o600);
    }
    db.create(dir).map_err(|e| e.to_string())?;
    let tmp = dir.join("token.tmp");
    let mut f = oo.open(&tmp).map_err(|e| e.to_string())?;
    f.write_all(token.as_bytes()).map_err(|e| e.to_string())?;
    drop(f);
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

pub fn forget_token() {
    if let Some(p) = token_path() {
        let _ = std::fs::remove_file(p);
    }
}
```

- [ ] **Step 4: Run tests**

Run: `ndc cargo test auth 2>&1 | tail -5`
Expected: `4 passed`

- [ ] **Step 5: Commit**

```bash
git add src/auth.rs src/main.rs
git commit -m "feat: add Jira token lookup and saved token file"
```

---

### Task 4: Cache and the Jira client (reads)

**Files:**
- Create: `src/cache.rs`, `src/jira.rs`, `tests/fixtures/search_page.json`, `tests/fixtures/issue_detail.json`
- Modify: `src/main.rs` (add `mod cache; mod jira;`)

**Interfaces:**
- Consumes: `auth::Login`.
- Produces (`cache.rs`): `pub fn load<T: DeserializeOwned>(key: &str) -> Option<T>`, `pub fn store<T: Serialize>(key: &str, value: &T)`; dir `dirs::cache_dir()/jira-tickets`, overridable by `JIRA_TICKETS_CACHE_DIR`.
- Produces (`jira.rs`):
  - `pub struct Client { base: String, agent: ureq::Agent, email: String, token: String }`, `pub fn new(site: &str, login: &Login) -> Client`.
  - `pub type Result<T> = std::result::Result<T, Error>`; `pub enum Error { Unauthorized(String), RateLimited(String), Other(String) }` implementing `Display`.
  - `pub static READ_ONLY: AtomicBool`.
  - Models (all `Serialize + Deserialize + Clone + Debug + Default`):
    - `pub struct User { pub account_id: String, pub display_name: String }`
    - `pub struct Status { pub name: String, pub category: String }` (category is `statusCategory.key`: `new`, `indeterminate`, `done`)
    - `pub struct Comment { pub id: String, pub author: String, pub created: String, pub body: serde_json::Value }`
    - `pub struct Issue { pub key: String, pub id: String, pub summary: String, pub issue_type: String, pub status: Status, pub priority: String, pub labels: Vec<String>, pub assignee: Option<User>, pub reporter: Option<User>, pub parent_key: Option<String>, pub parent_id: Option<String>, pub parent_summary: Option<String>, pub created: String, pub updated: String, pub due_date: Option<String>, pub status_changed: Option<String>, pub rank: String, pub story_points: Option<f64>, pub sprint: Option<String>, pub description: Option<serde_json::Value>, pub comments: Vec<Comment>, pub custom: BTreeMap<String, serde_json::Value> }`
    - `pub struct Transition { pub id: String, pub name: String, pub to: String }`, `pub struct Resolution { pub id: String, pub name: String }`, `pub struct Sprint { pub id: u64, pub name: String, pub state: String }`
  - Reads: `myself() -> Result<User>`, `search(jql: &str, fields: &[&str]) -> Result<Vec<Issue>>` (follows `nextPageToken`), `issue(key: &str, extra_fields: &[&str]) -> Result<Issue>`, `transitions(key) -> Result<Vec<Transition>>`, `resolutions() -> Result<Vec<Resolution>>`, `assignable_users(project: &str) -> Result<Vec<User>>`, `board_sprints(board_id: u64, states: &str) -> Result<Vec<Sprint>>`.
  - `pub const LIST_FIELDS: &[&str]` = summary, status, assignee, reporter, parent, labels, priority, issuetype, created, updated, duedate, customfield_10019, statuscategorychangedate, sprint (customfield_10020), story points (customfield_10016).
  - `pub fn parse_issue(v: &serde_json::Value) -> Issue`.
  - Test helper (cfg(test)): `pub fn stub_server(routes: Vec<(&'static str, &'static str, serde_json::Value)>) -> (String, Receiver<Recorded>)` — a one-thread HTTP/1.1 server on `127.0.0.1:0` that matches `(method, path-prefix)` and records each request's method, path, and body.

- [ ] **Step 1: Write cache.rs**

```rust
//! Small JSON disk cache (~/Library/Caches/jira-tickets). Launch shows cached
//! data instantly, then refreshes in the background.

use serde::{Serialize, de::DeserializeOwned};
use std::path::PathBuf;

fn dir() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("JIRA_TICKETS_CACHE_DIR") {
        return Some(PathBuf::from(p));
    }
    Some(dirs::cache_dir()?.join("jira-tickets"))
}

fn path(key: &str) -> Option<PathBuf> {
    let name: String = key.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).take(150).collect();
    Some(dir()?.join(format!("{name}.json")))
}

pub fn load<T: DeserializeOwned>(key: &str) -> Option<T> {
    let bytes = std::fs::read(path(key)?).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Write to a temp file and rename, so a crash never leaves half a file.
pub fn store<T: Serialize>(key: &str, value: &T) {
    let (Some(dir), Some(p)) = (dir(), path(key)) else { return };
    let _ = std::fs::create_dir_all(&dir);
    let Ok(bytes) = serde_json::to_vec(value) else { return };
    let tmp = p.with_extension("tmp");
    if std::fs::write(&tmp, bytes).is_ok() {
        let _ = std::fs::rename(tmp, p);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn round_trip() {
        let dir = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var("JIRA_TICKETS_CACHE_DIR", dir.path()) };
        super::store("k/1", &vec![1, 2, 3]);
        assert_eq!(super::load::<Vec<i32>>("k/1"), Some(vec![1, 2, 3]));
        unsafe { std::env::remove_var("JIRA_TICKETS_CACHE_DIR") };
    }
}
```

- [ ] **Step 2: Write the fixtures**

`tests/fixtures/search_page.json` (a trimmed real shape of `POST /rest/api/3/search/jql`):

```json
{
  "issues": [
    {
      "id": "100001",
      "key": "CI-9001",
      "fields": {
        "summary": "Remove the old autoscaler",
        "issuetype": { "name": "Task" },
        "status": { "name": "New", "statusCategory": { "key": "new" } },
        "priority": { "name": "Medium" },
        "labels": ["PLAT"],
        "assignee": null,
        "reporter": { "accountId": "abc", "displayName": "Shawn Sorichetti" },
        "parent": { "id": "10002", "key": "CI-79", "fields": { "summary": "Cluster Operations (ongoing)" } },
        "created": "2026-10-01T10:00:00.000-0700",
        "updated": "2026-10-05T10:00:00.000-0700",
        "duedate": null,
        "customfield_10019": "0|i0abcd:",
        "statuscategorychangedate": "2026-10-01T10:00:00.000-0700",
        "customfield_10020": [{ "id": 55, "name": "Sprint 92", "state": "active" }],
        "customfield_10016": 3.0
      }
    },
    {
      "id": "100002",
      "key": "CI-9002",
      "fields": {
        "summary": "Convert the deployer to the overlay workflow",
        "issuetype": { "name": "Story" },
        "status": { "name": "In Progress", "statusCategory": { "key": "indeterminate" } },
        "priority": { "name": "High" },
        "labels": ["PLAT", "health:on-track"],
        "assignee": { "accountId": "def", "displayName": "Ada Lovelace" },
        "reporter": null,
        "parent": null,
        "created": "2026-09-01T10:00:00.000-0700",
        "updated": "2026-10-06T10:00:00.000-0700",
        "duedate": "2026-12-31",
        "customfield_10019": "0|i0abce:",
        "statuscategorychangedate": "2026-09-20T10:00:00.000-0700",
        "customfield_10020": null,
        "customfield_10016": null
      }
    }
  ]
}
```

`tests/fixtures/issue_detail.json`:

```json
{
  "id": "100001",
  "key": "CI-9001",
  "fields": {
    "summary": "Remove the old autoscaler",
    "issuetype": { "name": "Task" },
    "status": { "name": "New", "statusCategory": { "key": "new" } },
    "priority": { "name": "Medium" },
    "labels": ["PLAT"],
    "assignee": null,
    "reporter": { "accountId": "abc", "displayName": "Shawn Sorichetti" },
    "parent": { "id": "10002", "key": "CI-79", "fields": { "summary": "Cluster Operations (ongoing)" } },
    "created": "2026-10-01T10:00:00.000-0700",
    "updated": "2026-10-05T10:00:00.000-0700",
    "description": {
      "type": "doc", "version": 1,
      "content": [
        { "type": "paragraph", "content": [ { "type": "text", "text": "Delete the " }, { "type": "text", "text": "autoscaler", "marks": [ { "type": "code" } ] }, { "type": "text", "text": " helm release and its IAM role." } ] },
        { "type": "bulletList", "content": [
          { "type": "listItem", "content": [ { "type": "paragraph", "content": [ { "type": "text", "text": "Remove the Deployer app" } ] } ] },
          { "type": "listItem", "content": [ { "type": "paragraph", "content": [ { "type": "text", "text": "Drop the role in terraform" } ] } ] }
        ] }
      ]
    },
    "comment": {
      "comments": [
        {
          "id": "5001",
          "author": { "accountId": "def", "displayName": "Ada Lovelace" },
          "created": "2026-10-03T09:00:00.000-0700",
          "body": { "type": "doc", "version": 1, "content": [ { "type": "paragraph", "content": [ { "type": "text", "text": "Is the role still used by anything?" } ] } ] }
        }
      ]
    }
  }
}
```

- [ ] **Step 3: Write the failing client tests**

In `src/jira.rs`, test module:

```rust
#[cfg(test)]
pub mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::sync::mpsc::{Receiver, channel};

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
    fn read_only_blocks_writes() {
        READ_ONLY.store(true, std::sync::atomic::Ordering::Relaxed);
        assert!(check_writable().is_err());
        READ_ONLY.store(false, std::sync::atomic::Ordering::Relaxed);
        assert!(check_writable().is_ok());
    }
}
```

- [ ] **Step 4: Run tests to verify they fail**

Add `mod cache; mod jira;` to `src/main.rs`. Run: `ndc cargo test jira 2>&1 | tail -5`
Expected: compile errors.

- [ ] **Step 5: Write the client**

```rust
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
    ureq::Agent::config_builder().http_status_as_error(false).timeout_global(Some(Duration::from_secs(60))).user_agent("jira-tickets").tls_config(tls).build().into()
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
    "summary", "status", "assignee", "reporter", "parent", "labels", "priority", "issuetype", "created", "updated", "duedate", RANK_FIELD, "statuscategorychangedate", SPRINT_FIELD,
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
    let sprint = f[SPRINT_FIELD].as_array().and_then(|a| {
        a.iter().filter(|s| s["state"].as_str() != Some("closed")).last().or(a.last()).and_then(|s| s["name"].as_str().map(String::from))
    });
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
        status: Status { name: str_at(f, &["status", "name"]).unwrap_or_default(), category: str_at(f, &["status", "statusCategory", "key"]).unwrap_or_default() },
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
        Client { base: site.trim_end_matches('/').to_string(), agent: http_agent(), auth: format!("Basic {}", encode(format!("{}:{}", login.email, login.token))) }
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
        let mut resp = req.header("Authorization", &self.auth).header("Accept", "application/json").header("Content-Type", "application/json").send_json(body)?;
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
            if let Some(a) = v["issues"].as_array() {
                out.extend(a.iter().map(parse_issue));
            }
            token = v["nextPageToken"].as_str().map(String::from);
            if token.is_none() {
                break;
            }
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
                    .map(|t| Transition { id: str_at(t, &["id"]).unwrap_or_default(), name: str_at(t, &["name"]).unwrap_or_default(), to: str_at(t, &["to", "name"]).unwrap_or_default() })
                    .collect()
            })
            .unwrap_or_default())
    }

    pub fn resolutions(&self) -> Result<Vec<Resolution>> {
        let v = self.get("/rest/api/3/resolution")?;
        Ok(v.as_array().map(|a| a.iter().map(|r| Resolution { id: str_at(r, &["id"]).unwrap_or_default(), name: str_at(r, &["name"]).unwrap_or_default() }).collect()).unwrap_or_default())
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
                    .map(|s| Sprint { id: s["id"].as_u64().unwrap_or(0), name: str_at(s, &["name"]).unwrap_or_default(), state: str_at(s, &["state"]).unwrap_or_default() })
                    .collect()
            })
            .unwrap_or_default())
    }
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
```

Note for the implementer: `resp.body_mut().read_to_string()` is ureq 3's API; if the compiler says otherwise, check the ureq 3 docs in `~/.cargo/registry` (inside the nix shell) for the current body reader name and adjust in one place.

- [ ] **Step 6: Run tests**

Run: `ndc cargo test jira 2>&1 | tail -8`
Expected: `5 passed` plus cache's `round_trip`.

- [ ] **Step 7: Format, clippy, commit**

Run: `ndc cargo fmt && ndc cargo clippy --all-targets -- -D warnings 2>&1 | tail -3`
Expected: no warnings.

```bash
git add src/cache.rs src/jira.rs src/main.rs tests/fixtures
git commit -m "feat: add Jira REST client reads, models, and disk cache"
```

---

### Task 5: ADF renderer (document model and plain text)

**Files:**
- Create: `src/adf.rs`, `tests/fixtures/adf_sample.json`
- Modify: `src/main.rs` (add `mod adf;`)

**Interfaces:**
- Produces:
  - `pub struct Doc { pub blocks: Vec<Block> }`
  - `pub enum Block { Paragraph(Vec<Span>), Heading { level: u8, spans: Vec<Span> }, List { ordered: bool, items: Vec<Vec<Block>> }, Code { lang: Option<String>, text: String }, Quote(Vec<Block>), Panel { kind: String, blocks: Vec<Block> }, Table(Vec<Vec<Vec<Span>>>), Rule }`
  - `pub struct Span { pub text: String, pub bold: bool, pub italic: bool, pub code: bool, pub strike: bool, pub link: Option<String> }`
  - `pub fn parse(v: &serde_json::Value) -> Doc`
  - `pub fn to_plain_text(v: &serde_json::Value) -> String`
  - `pub fn text_doc(text: &str) -> serde_json::Value`: plain text → ADF with one paragraph per line (used for posting comments).

- [ ] **Step 1: Write the fixture**

`tests/fixtures/adf_sample.json`:

```json
{
  "type": "doc", "version": 1,
  "content": [
    { "type": "heading", "attrs": { "level": 2 }, "content": [ { "type": "text", "text": "Goal" } ] },
    { "type": "paragraph", "content": [
      { "type": "text", "text": "Ship " },
      { "type": "text", "text": "canary", "marks": [ { "type": "strong" } ] },
      { "type": "text", "text": " via " },
      { "type": "text", "text": "Gateway API", "marks": [ { "type": "link", "attrs": { "href": "https://gateway-api.sigs.k8s.io" } } ] },
      { "type": "hardBreak" },
      { "type": "mention", "attrs": { "id": "abc", "text": "@Ada Lovelace" } },
      { "type": "text", "text": " owns it." }
    ] },
    { "type": "orderedList", "content": [
      { "type": "listItem", "content": [ { "type": "paragraph", "content": [ { "type": "text", "text": "first" } ] } ] },
      { "type": "listItem", "content": [ { "type": "paragraph", "content": [ { "type": "text", "text": "second" } ] } ] }
    ] },
    { "type": "codeBlock", "attrs": { "language": "sh" }, "content": [ { "type": "text", "text": "kubectl get pods" } ] },
    { "type": "panel", "attrs": { "panelType": "warning" }, "content": [ { "type": "paragraph", "content": [ { "type": "text", "text": "Careful" } ] } ] },
    { "type": "rule" },
    { "type": "table", "content": [
      { "type": "tableRow", "content": [
        { "type": "tableHeader", "content": [ { "type": "paragraph", "content": [ { "type": "text", "text": "A" } ] } ] },
        { "type": "tableHeader", "content": [ { "type": "paragraph", "content": [ { "type": "text", "text": "B" } ] } ] }
      ] },
      { "type": "tableRow", "content": [
        { "type": "tableCell", "content": [ { "type": "paragraph", "content": [ { "type": "text", "text": "1" } ] } ] },
        { "type": "tableCell", "content": [ { "type": "paragraph", "content": [ { "type": "text", "text": "2" } ] } ] }
      ] }
    ] },
    { "type": "mysteryNode", "content": [ { "type": "text", "text": "fallback text" } ] }
  ]
}
```

- [ ] **Step 2: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> serde_json::Value {
        serde_json::from_str(&std::fs::read_to_string("tests/fixtures/adf_sample.json").unwrap()).unwrap()
    }

    #[test]
    fn parses_block_types() {
        let d = parse(&sample());
        assert!(matches!(d.blocks[0], Block::Heading { level: 2, .. }));
        let Block::Paragraph(spans) = &d.blocks[1] else { panic!("paragraph") };
        assert!(spans.iter().any(|s| s.bold && s.text == "canary"));
        assert!(spans.iter().any(|s| s.link.as_deref() == Some("https://gateway-api.sigs.k8s.io")));
        assert!(spans.iter().any(|s| s.text == "\n"), "hardBreak becomes a newline span");
        assert!(spans.iter().any(|s| s.text == "@Ada Lovelace"));
        assert!(matches!(&d.blocks[2], Block::List { ordered: true, items } if items.len() == 2));
        assert!(matches!(&d.blocks[3], Block::Code { lang: Some(l), text } if l == "sh" && text == "kubectl get pods"));
        assert!(matches!(&d.blocks[4], Block::Panel { kind, .. } if kind == "warning"));
        assert!(matches!(d.blocks[5], Block::Rule));
        assert!(matches!(&d.blocks[6], Block::Table(rows) if rows.len() == 2 && rows[0].len() == 2));
        // Unknown node falls back to a paragraph of its text.
        assert!(matches!(&d.blocks[7], Block::Paragraph(s) if s[0].text == "fallback text"));
    }

    #[test]
    fn plain_text_joins_blocks() {
        let t = to_plain_text(&sample());
        assert!(t.starts_with("Goal\nShip canary via Gateway API\n@Ada Lovelace owns it."), "{t}");
        assert!(t.contains("1. first\n2. second"), "{t}");
        assert!(t.contains("kubectl get pods"));
        assert!(t.contains("A | B\n1 | 2"), "{t}");
    }

    #[test]
    fn text_doc_builds_paragraphs() {
        let v = text_doc("one\ntwo");
        assert_eq!(v["content"].as_array().unwrap().len(), 2);
        assert_eq!(v["content"][1]["content"][0]["text"], "two");
        assert_eq!(to_plain_text(&v), "one\ntwo");
    }

    #[test]
    fn null_is_empty() {
        assert_eq!(to_plain_text(&serde_json::Value::Null), "");
        assert!(parse(&serde_json::Value::Null).blocks.is_empty());
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Add `mod adf;` to `src/main.rs`. Run: `ndc cargo test adf 2>&1 | tail -5`
Expected: compile errors.

- [ ] **Step 4: Write the implementation**

```rust
//! Atlassian Document Format → a small block model the views paint, plus
//! plain text for triage screens and reports.

use serde_json::{Value, json};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Doc {
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Paragraph(Vec<Span>),
    Heading { level: u8, spans: Vec<Span> },
    List { ordered: bool, items: Vec<Vec<Block>> },
    Code { lang: Option<String>, text: String },
    Quote(Vec<Block>),
    Panel { kind: String, blocks: Vec<Block> },
    /// rows → cells → spans. The first row is the header when it came from tableHeader cells.
    Table(Vec<Vec<Vec<Span>>>),
    Rule,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Span {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
    pub strike: bool,
    pub link: Option<String>,
}

pub fn parse(v: &Value) -> Doc {
    Doc { blocks: blocks(v.get("content")) }
}

fn blocks(content: Option<&Value>) -> Vec<Block> {
    content.and_then(Value::as_array).map(|a| a.iter().filter_map(block).collect()).unwrap_or_default()
}

fn block(n: &Value) -> Option<Block> {
    let t = n["type"].as_str().unwrap_or("");
    Some(match t {
        "paragraph" => Block::Paragraph(spans(n.get("content"))),
        "heading" => Block::Heading { level: n["attrs"]["level"].as_u64().unwrap_or(1) as u8, spans: spans(n.get("content")) },
        "bulletList" | "orderedList" => Block::List {
            ordered: t == "orderedList",
            items: n["content"].as_array().map(|a| a.iter().map(|li| blocks(li.get("content"))).collect()).unwrap_or_default(),
        },
        "codeBlock" => Block::Code { lang: n["attrs"]["language"].as_str().map(String::from), text: collect_text(n) },
        "blockquote" => Block::Quote(blocks(n.get("content"))),
        "panel" => Block::Panel { kind: n["attrs"]["panelType"].as_str().unwrap_or("info").to_string(), blocks: blocks(n.get("content")) },
        "rule" => Block::Rule,
        "table" => Block::Table(
            n["content"]
                .as_array()
                .map(|rows| {
                    rows.iter()
                        .map(|row| {
                            row["content"]
                                .as_array()
                                .map(|cells| cells.iter().map(|c| blocks(c.get("content")).into_iter().flat_map(block_spans).collect()).collect())
                                .unwrap_or_default()
                        })
                        .collect()
                })
                .unwrap_or_default(),
        ),
        "mediaSingle" | "mediaGroup" | "media" => Block::Paragraph(vec![Span { text: "[attachment]".into(), italic: true, ..Default::default() }]),
        _ => {
            let text = collect_text(n);
            if text.is_empty() {
                return None;
            }
            Block::Paragraph(vec![Span { text, ..Default::default() }])
        }
    })
}

/// Flatten a block to spans (for table cells and plain text).
fn block_spans(b: Block) -> Vec<Span> {
    match b {
        Block::Paragraph(s) | Block::Heading { spans: s, .. } => s,
        Block::Code { text, .. } => vec![Span { text, code: true, ..Default::default() }],
        Block::List { items, .. } => items.into_iter().flat_map(|i| i.into_iter().flat_map(block_spans)).collect(),
        Block::Quote(b) | Block::Panel { blocks: b, .. } => b.into_iter().flat_map(block_spans).collect(),
        Block::Table(rows) => rows.into_iter().flatten().flatten().collect(),
        Block::Rule => Vec::new(),
    }
}

fn spans(content: Option<&Value>) -> Vec<Span> {
    let mut out = Vec::new();
    for n in content.and_then(Value::as_array).into_iter().flatten() {
        match n["type"].as_str().unwrap_or("") {
            "text" => {
                let mut s = Span { text: n["text"].as_str().unwrap_or("").to_string(), ..Default::default() };
                for m in n["marks"].as_array().into_iter().flatten() {
                    match m["type"].as_str().unwrap_or("") {
                        "strong" => s.bold = true,
                        "em" => s.italic = true,
                        "code" => s.code = true,
                        "strike" => s.strike = true,
                        "link" => s.link = m["attrs"]["href"].as_str().map(String::from),
                        _ => {}
                    }
                }
                out.push(s);
            }
            "hardBreak" => out.push(Span { text: "\n".into(), ..Default::default() }),
            "mention" => out.push(Span { text: n["attrs"]["text"].as_str().unwrap_or("@someone").to_string(), bold: true, ..Default::default() }),
            "emoji" => out.push(Span { text: n["attrs"]["text"].or_shortname(n), ..Default::default() }),
            "inlineCard" => {
                let url = n["attrs"]["url"].as_str().unwrap_or("").to_string();
                out.push(Span { text: url.clone(), link: Some(url), ..Default::default() });
            }
            "status" => out.push(Span { text: format!("[{}]", n["attrs"]["text"].as_str().unwrap_or("")), code: true, ..Default::default() }),
            "date" => out.push(Span { text: n["attrs"]["timestamp"].as_str().unwrap_or("").to_string(), ..Default::default() }),
            _ => {
                let t = collect_text(n);
                if !t.is_empty() {
                    out.push(Span { text: t, ..Default::default() });
                }
            }
        }
    }
    out
}

trait OrShortname {
    fn or_shortname(&self, n: &Value) -> String;
}
impl OrShortname for Value {
    fn or_shortname(&self, n: &Value) -> String {
        self.as_str().or(n["attrs"]["shortName"].as_str()).unwrap_or("").to_string()
    }
}

fn collect_text(n: &Value) -> String {
    if let Some(t) = n["text"].as_str() {
        return t.to_string();
    }
    n["content"].as_array().map(|a| a.iter().map(collect_text).collect::<Vec<_>>().join("")).unwrap_or_default()
}

pub fn to_plain_text(v: &Value) -> String {
    let mut out = Vec::new();
    for b in parse(v).blocks {
        push_plain(&b, &mut out, 0);
    }
    out.join("\n").trim_end().to_string()
}

fn spans_text(s: &[Span]) -> String {
    s.iter().map(|x| x.text.as_str()).collect()
}

fn push_plain(b: &Block, out: &mut Vec<String>, depth: usize) {
    let indent = "  ".repeat(depth);
    match b {
        Block::Paragraph(s) | Block::Heading { spans: s, .. } => out.push(format!("{indent}{}", spans_text(s))),
        Block::List { ordered, items } => {
            for (i, item) in items.iter().enumerate() {
                let marker = if *ordered { format!("{}. ", i + 1) } else { "- ".to_string() };
                let mut lines = Vec::new();
                for ib in item {
                    push_plain(ib, &mut lines, 0);
                }
                let mut it = lines.into_iter();
                if let Some(first) = it.next() {
                    out.push(format!("{indent}{marker}{first}"));
                }
                for rest in it {
                    out.push(format!("{indent}  {rest}"));
                }
            }
        }
        Block::Code { text, .. } => out.push(format!("{indent}{text}")),
        Block::Quote(bs) | Block::Panel { blocks: bs, .. } => {
            for ib in bs {
                push_plain(ib, out, depth + 1);
            }
        }
        Block::Table(rows) => {
            for r in rows {
                out.push(format!("{indent}{}", r.iter().map(|c| spans_text(c)).collect::<Vec<_>>().join(" | ")));
            }
        }
        Block::Rule => out.push(format!("{indent}---")),
    }
}

/// Plain text → ADF, one paragraph per line (empty lines are skipped).
pub fn text_doc(text: &str) -> Value {
    let paras: Vec<Value> = text.lines().filter(|l| !l.trim().is_empty()).map(|l| json!({ "type": "paragraph", "content": [ { "type": "text", "text": l } ] })).collect();
    json!({ "type": "doc", "version": 1, "content": paras })
}
```

- [ ] **Step 5: Run tests**

Run: `ndc cargo test adf 2>&1 | tail -5`
Expected: `4 passed`

- [ ] **Step 6: Commit**

```bash
git add src/adf.rs src/main.rs tests/fixtures/adf_sample.json
git commit -m "feat: add ADF document model and plain-text renderer"
```

---

### Task 6: Store, JQL builders, and projections

**Files:**
- Create: `src/store.rs`
- Modify: `src/main.rs` (add `mod store;`)

**Interfaces:**
- Consumes: `jira::Issue`, `config::Config`, `cache`.
- Produces:
  - `#[derive(Hash, Eq, PartialEq, Clone, Copy, Debug, Serialize, Deserialize)] pub enum ListId { TeamOpen, Epics, TeamInProgress, Search }`
  - `pub struct Store { pub issues: HashMap<String, Issue>, pub lists: HashMap<ListId, Vec<String>>, pub fetched_at: Option<i64> }` (`fetched_at` is unix seconds, for the offline banner)
  - `impl Store { pub fn absorb(&mut self, id: ListId, issues: Vec<Issue>); pub fn merge_detail(&mut self, issue: Issue); pub fn list(&self, id: ListId) -> Vec<&Issue>; pub fn get(&self, key: &str) -> Option<&Issue>; pub fn by_status(&self, id: ListId, status: &str) -> Vec<&Issue>; pub fn by_parent(&self, id: ListId) -> BTreeMap<Option<String>, Vec<&Issue>>; pub fn by_assignee(&self, ids: &[ListId]) -> BTreeMap<String, Vec<&Issue>>; pub fn children_of(&self, epic_key: &str) -> Vec<&Issue>; pub fn load_cached() -> Option<Store>; pub fn save_cached(&self) }`
  - `pub fn jql_team_open(cfg: &Config) -> String`
  - `pub fn jql_epics(cfg: &Config, parent_keys: &[String]) -> String`
  - `pub fn jql_team_in_progress(cfg: &Config) -> String`
  - `pub fn jql_quote(s: &str) -> String` (wraps in double quotes, escaping `"` and `\`).
  - `merge_detail` keeps `description`/`comments` from the new issue; `absorb` keeps an existing record's `description`/`comments` when the incoming list row has none.

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::jira::{Issue, Status, User};

    fn issue(key: &str, status: &str, parent: Option<&str>, assignee: Option<&str>) -> Issue {
        Issue {
            key: key.into(),
            status: Status { name: status.into(), category: "new".into() },
            parent_key: parent.map(String::from),
            assignee: assignee.map(|a| User { account_id: a.to_lowercase(), display_name: a.into() }),
            ..Default::default()
        }
    }

    fn cfg() -> Config {
        toml::from_str("site = \"https://x.atlassian.net\"\nemail = \"a@b.c\"\n[team]\nmembers = [\"Ada Lovelace\", \"Shawn Sorichetti\"]\n").unwrap()
    }

    #[test]
    fn jql_builders() {
        let c = cfg();
        assert_eq!(jql_team_open(&c), r#"project = "CI" AND labels = "PLAT" AND statusCategory != Done ORDER BY Rank ASC"#);
        assert_eq!(
            jql_epics(&c, &["CI-1".into(), "CI-2".into()]),
            r#"project = "CI" AND issuetype = Epic AND (labels in ("PLAT") OR key in (CI-1, CI-2)) ORDER BY Rank ASC"#
        );
        assert_eq!(jql_epics(&c, &[]), r#"project = "CI" AND issuetype = Epic AND (labels in ("PLAT")) ORDER BY Rank ASC"#);
        assert_eq!(
            jql_team_in_progress(&c),
            r#"project in ("CI", "OPS") AND assignee in ("Ada Lovelace", "Shawn Sorichetti") AND statusCategory = "In Progress" ORDER BY updated DESC"#
        );
        assert_eq!(jql_quote(r#"a"b"#), r#""a\"b""#);
    }

    #[test]
    fn jql_epics_with_allowlist() {
        let mut c = cfg();
        c.roadmap.epics = vec!["CI-9".into()];
        assert_eq!(jql_epics(&c, &["CI-1".into()]), r#"project = "CI" AND issuetype = Epic AND (labels in ("PLAT") OR key in (CI-1) OR key in (CI-9)) ORDER BY Rank ASC"#);
    }

    #[test]
    fn absorb_and_projections() {
        let mut s = Store::default();
        s.absorb(ListId::TeamOpen, vec![issue("CI-1", "New", Some("E-1"), None), issue("CI-2", "Planning", Some("E-1"), Some("Ada Lovelace")), issue("CI-3", "New", None, None)]);
        assert_eq!(s.list(ListId::TeamOpen).iter().map(|i| i.key.as_str()).collect::<Vec<_>>(), ["CI-1", "CI-2", "CI-3"]);
        assert_eq!(s.by_status(ListId::TeamOpen, "New").len(), 2);
        let groups = s.by_parent(ListId::TeamOpen);
        assert_eq!(groups[&Some("E-1".to_string())].len(), 2);
        assert_eq!(groups[&None].len(), 1);
        let by = s.by_assignee(&[ListId::TeamOpen]);
        assert_eq!(by["Ada Lovelace"][0].key, "CI-2");
        assert_eq!(s.children_of("E-1").len(), 2);
    }

    #[test]
    fn detail_survives_list_refresh() {
        let mut s = Store::default();
        s.absorb(ListId::TeamOpen, vec![issue("CI-1", "New", None, None)]);
        let mut d = issue("CI-1", "New", None, None);
        d.description = Some(serde_json::json!({"type": "doc"}));
        s.merge_detail(d);
        s.absorb(ListId::TeamOpen, vec![issue("CI-1", "Planning", None, None)]);
        let i = s.get("CI-1").unwrap();
        assert_eq!(i.status.name, "Planning");
        assert!(i.description.is_some());
    }

    #[test]
    fn cache_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var("JIRA_TICKETS_CACHE_DIR", dir.path()) };
        let mut s = Store::default();
        s.absorb(ListId::Epics, vec![issue("E-1", "In Progress", None, None)]);
        s.save_cached();
        let back = Store::load_cached().unwrap();
        assert_eq!(back.list(ListId::Epics).len(), 1);
        unsafe { std::env::remove_var("JIRA_TICKETS_CACHE_DIR") };
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Add `mod store;` to `src/main.rs`. Run: `ndc cargo test store 2>&1 | tail -5`
Expected: compile errors.

- [ ] **Step 3: Write the implementation**

```rust
//! One cached store of issues. Views are projections over it.

use crate::config::Config;
use crate::jira::Issue;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

#[derive(Hash, Eq, PartialEq, Clone, Copy, Debug, Serialize, Deserialize)]
pub enum ListId {
    TeamOpen,
    Epics,
    TeamInProgress,
    Search,
}

#[derive(Default, Clone, Serialize, Deserialize)]
pub struct Store {
    pub issues: HashMap<String, Issue>,
    pub lists: HashMap<ListId, Vec<String>>,
    /// Unix seconds of the last successful full refresh.
    pub fetched_at: Option<i64>,
}

const CACHE_KEY: &str = "store";

impl Store {
    /// Replace a list. List rows carry no description or comments, so keep
    /// those from the record already in the store.
    pub fn absorb(&mut self, id: ListId, issues: Vec<Issue>) {
        let mut keys = Vec::with_capacity(issues.len());
        for mut i in issues {
            if let Some(old) = self.issues.get(&i.key) {
                if i.description.is_none() {
                    i.description = old.description.clone();
                }
                if i.comments.is_empty() {
                    i.comments = old.comments.clone();
                }
            }
            keys.push(i.key.clone());
            self.issues.insert(i.key.clone(), i);
        }
        self.lists.insert(id, keys);
    }

    pub fn merge_detail(&mut self, issue: Issue) {
        self.issues.insert(issue.key.clone(), issue);
    }

    pub fn get(&self, key: &str) -> Option<&Issue> {
        self.issues.get(key)
    }

    pub fn list(&self, id: ListId) -> Vec<&Issue> {
        self.lists.get(&id).map(|keys| keys.iter().filter_map(|k| self.issues.get(k)).collect()).unwrap_or_default()
    }

    pub fn by_status(&self, id: ListId, status: &str) -> Vec<&Issue> {
        self.list(id).into_iter().filter(|i| i.status.name == status).collect()
    }

    pub fn by_parent(&self, id: ListId) -> BTreeMap<Option<String>, Vec<&Issue>> {
        let mut out: BTreeMap<Option<String>, Vec<&Issue>> = BTreeMap::new();
        for i in self.list(id) {
            out.entry(i.parent_key.clone()).or_default().push(i);
        }
        out
    }

    /// Assignee display name → issues, de-duplicated across lists, in list order.
    pub fn by_assignee(&self, ids: &[ListId]) -> BTreeMap<String, Vec<&Issue>> {
        let mut out: BTreeMap<String, Vec<&Issue>> = BTreeMap::new();
        let mut seen = std::collections::HashSet::new();
        for id in ids {
            for i in self.list(*id) {
                if !seen.insert(i.key.clone()) {
                    continue;
                }
                let name = i.assignee.as_ref().map(|a| a.display_name.clone()).unwrap_or_default();
                out.entry(name).or_default().push(i);
            }
        }
        out
    }

    pub fn children_of(&self, epic_key: &str) -> Vec<&Issue> {
        self.list(ListId::TeamOpen).into_iter().filter(|i| i.parent_key.as_deref() == Some(epic_key)).collect()
    }

    pub fn load_cached() -> Option<Store> {
        crate::cache::load(CACHE_KEY)
    }

    pub fn save_cached(&self) {
        crate::cache::store(CACHE_KEY, self);
    }
}

pub fn jql_quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

pub fn jql_team_open(cfg: &Config) -> String {
    format!("project = {} AND labels = {} AND statusCategory != Done ORDER BY Rank ASC", jql_quote(&cfg.scope.project), jql_quote(&cfg.scope.team_label))
}

pub fn jql_epics(cfg: &Config, parent_keys: &[String]) -> String {
    let labels: Vec<String> = cfg.roadmap.lanes.keys().map(|l| jql_quote(l)).collect();
    let mut clauses = vec![format!("labels in ({})", labels.join(", "))];
    if !parent_keys.is_empty() {
        clauses.push(format!("key in ({})", parent_keys.join(", ")));
    }
    if !cfg.roadmap.epics.is_empty() {
        clauses.push(format!("key in ({})", cfg.roadmap.epics.join(", ")));
    }
    format!("project = {} AND issuetype = Epic AND ({}) ORDER BY Rank ASC", jql_quote(&cfg.scope.project), clauses.join(" OR "))
}

pub fn jql_team_in_progress(cfg: &Config) -> String {
    let projects: Vec<String> = cfg.scope.projects.iter().map(|p| jql_quote(p)).collect();
    let members: Vec<String> = cfg.team.members.iter().map(|m| jql_quote(m)).collect();
    format!("project in ({}) AND assignee in ({}) AND statusCategory = \"In Progress\" ORDER BY updated DESC", projects.join(", "), members.join(", "))
}
```

- [ ] **Step 4: Run tests**

Run: `ndc cargo test store 2>&1 | tail -5`
Expected: `5 passed`

- [ ] **Step 5: Commit**

```bash
git add src/store.rs src/main.rs
git commit -m "feat: add issue store with JQL builders and projections"
```

---

### Task 7: Jira client writes

**Files:**
- Modify: `src/jira.rs`

**Interfaces:**
- Produces on `Client`:
  - `transition(key: &str, transition_id: &str, resolution_id: Option<&str>) -> Result<()>`
  - `rank(key: &str, before: Option<&str>, after: Option<&str>) -> Result<()>` (exactly one of before/after is Some)
  - `assign(key: &str, account_id: Option<&str>) -> Result<()>` (None unassigns)
  - `move_to_sprint(sprint_id: u64, keys: &[&str]) -> Result<()>`, `move_to_backlog(keys: &[&str]) -> Result<()>`
  - `update_fields(key: &str, fields: serde_json::Value) -> Result<()>` (caller builds `{"labels": [...], "priority": {"name": ..}, "customfield_10016": n}`)
  - `add_comment(key: &str, text: &str) -> Result<()>` (uses `adf::text_doc`)
  - `pub fn needs_resolution(e: &Error) -> bool` (error text mentions "resolution").

- [ ] **Step 1: Write the failing tests** (append inside `mod tests`)

```rust
    #[test]
    fn transition_sends_id_and_optional_resolution() {
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
    }

    #[test]
    fn assign_and_unassign() {
        let (base, rx) = stub_server(vec![("PUT", "/rest/api/3/issue/CI-1/assignee", serde_json::json!({}))]);
        let c = client(&base);
        c.assign("CI-1", Some("abc")).unwrap();
        assert_eq!(serde_json::from_str::<serde_json::Value>(&rx.recv().unwrap().body).unwrap()["accountId"], "abc");
        c.assign("CI-1", None).unwrap();
        assert!(serde_json::from_str::<serde_json::Value>(&rx.recv().unwrap().body).unwrap()["accountId"].is_null());
    }

    #[test]
    fn sprint_moves_and_fields_and_comment() {
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
        let (base, rx) = stub_server(vec![]);
        let c = client(&base);
        READ_ONLY.store(true, std::sync::atomic::Ordering::Relaxed);
        assert!(c.transition("CI-1", "1", None).is_err());
        READ_ONLY.store(false, std::sync::atomic::Ordering::Relaxed);
        assert!(rx.try_recv().is_err(), "nothing was sent");
    }

    #[test]
    fn detects_resolution_requirement() {
        assert!(needs_resolution(&Error::Other("Jira returned 400: resolution: Resolution is required".into())));
        assert!(!needs_resolution(&Error::Other("boom".into())));
    }
```

Also change `read_only_blocks_writes` from Task 4 to hold a `static WRITE_LOCK: Mutex<()>` so the two READ_ONLY tests do not race: lock it at the top of both tests.

- [ ] **Step 2: Run tests to verify they fail**

Run: `ndc cargo test jira 2>&1 | tail -5`
Expected: compile errors for the new methods.

- [ ] **Step 3: Write the implementation** (append to `impl Client`, plus the free function)

```rust
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
```

- [ ] **Step 4: Run tests, fmt, clippy**

Run: `ndc cargo test jira 2>&1 | tail -5 && ndc cargo fmt && ndc cargo clippy --all-targets -- -D warnings 2>&1 | tail -3`
Expected: `11 passed`, no warnings.

- [ ] **Step 5: Commit**

```bash
git add src/jira.rs
git commit -m "feat: add Jira write operations with read-only guard"
```

---

### Task 8: Theme, app shell, sign-in, header, toast

**Files:**
- Create: `src/theme.rs`, `src/app.rs`, `src/views/mod.rs`
- Modify: `src/main.rs` (replace `Hello` with `app::App`; add `mod theme; mod app; mod views;`)

**Interfaces:**
- Produces (`theme.rs`): `pub struct Palette { pub dark: bool, pub bg, surface, surface_2, border, text, muted, accent, on_track, at_risk, off_track, no_health, done, today, selected_row, hover_row, danger_subtle, tooltip: Color32 }`, `pub const LIGHT: Palette`, `pub const DARK: Palette`, `pub fn palette(ctx: &egui::Context) -> &'static Palette`, `pub fn apply(ctx: &egui::Context)`, `pub fn bold(size: f32) -> FontId`, `pub fn body(size: f32) -> FontId`, `pub fn mono(size: f32) -> FontId`, `pub fn status_color(category: &str, p: &Palette) -> Color32` (`new` → muted, `indeterminate` → accent, `done` → on_track).
- Produces (`app.rs`):
  - `pub enum Screen { Issues, Grooming, Team, Roadmap }`
  - `pub enum Msg { Login(Option<auth::Login>), Me(jira::Result<jira::User>), List { id: ListId, result: jira::Result<Vec<Issue>>, stale: bool }, Detail { key: String, result: jira::Result<Issue> }, Done { key: String, what: &'static str, result: jira::Result<()> }, Transitions { key: String, result: jira::Result<Vec<Transition>> }, Resolutions(jira::Result<Vec<Resolution>>), Users(jira::Result<Vec<User>>), Sprints(jira::Result<Vec<Sprint>>) }`
  - `pub enum Action { SetScreen(Screen), Refresh, OpenUrl(String), Copy(String), Select(String), SubmitToken, FindToken, SignOut, Toast(String, bool), /* Issues-view actions added in Tasks 9–11 */ }`
  - `pub struct Loaded { pub loading: bool, pub error: Option<String>, pub fetched: Option<Instant> }` (per list; data lives in the store)
  - `pub struct Toast { pub text: String, pub error: bool, pub until: Instant }`
  - `pub struct App { pub cfg: Arc<Config>, pub store: Store, pub lists: HashMap<ListId, Loaded>, pub screen: Screen, pub me: Option<User>, pub login: Option<auth::Login>, pub finding_token: bool, pub token_input: String, pub token_error: Option<String>, pub toast: Option<Toast>, pub actions: Vec<Action>, pub offline_since: Option<i64>, client: Option<Arc<Client>>, tx: Sender<Msg>, rx: Receiver<Msg>, ctx: egui::Context, last_refresh: Instant, ... }`
  - `impl App { pub fn new(cc: &eframe::CreationContext<'_>, cfg: Config) -> Self; pub fn spawn(&self, f: impl FnOnce(&Client, &Sender<Msg>) + Send + 'static); pub fn refresh_all(&mut self); pub fn load_list(&mut self, id: ListId, jql: String, fields: Vec<String>); pub fn toast(&mut self, text: impl Into<String>, error: bool); fn handle(&mut self, msg: Msg); fn apply(&mut self, a: Action, ctx: &egui::Context) }`
  - `main.rs`: loads config; if missing, writes the example and prints the jira-cli-style welcome to stdout, exits 0; if invalid, prints the error and exits 1.
- Produces (`views/mod.rs`): `pub fn main(app: &mut App, ui: &mut Ui)` (header + screen body; body for screens other than Issues is a centred "coming in plan 2/3" label until those plans land), `pub fn sign_in(app: &mut App, ui: &mut Ui)`, `pub fn toast(app: &mut App, ctx: &egui::Context)`, shared widgets `pub fn button(text, p) -> Button`, `pub fn primary_button(text, p) -> Button`, `pub fn pill(ui, text, bg, fg) -> Response`, `pub fn health_dot(ui, color)`, `pub fn error_banner(ui, p, title, raw) -> bool` (returns true when Retry clicked).

- [ ] **Step 1: Write theme.rs**

```rust
//! Colours from the roadmap mockup tokens, light and dark, and system fonts.

use egui::{Color32, FontData, FontDefinitions, FontFamily, FontId, Stroke, Visuals};
use std::sync::Arc;

pub const BOLD: &str = "bold";

pub fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(BOLD.into()))
}
pub fn body(size: f32) -> FontId {
    FontId::proportional(size)
}
pub fn mono(size: f32) -> FontId {
    FontId::monospace(size)
}

#[derive(Clone, Copy)]
pub struct Palette {
    pub dark: bool,
    pub bg: Color32,
    pub surface: Color32,
    pub surface_2: Color32,
    pub border: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub accent: Color32,
    pub on_track: Color32,
    pub at_risk: Color32,
    pub off_track: Color32,
    pub no_health: Color32,
    pub done: Color32,
    pub today: Color32,
    pub selected_row: Color32,
    pub hover_row: Color32,
    pub danger_subtle: Color32,
    pub tooltip: Color32,
}

const fn hex(v: u32) -> Color32 {
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

pub const LIGHT: Palette = Palette {
    dark: false,
    bg: hex(0xf7f7f5),
    surface: hex(0xffffff),
    surface_2: hex(0xf0f0ed),
    border: hex(0xe3e2de),
    text: hex(0x1f1f1d),
    muted: hex(0x6b6a66),
    accent: hex(0x2f6fdb),
    on_track: hex(0x2e8b57),
    at_risk: hex(0xc98a12),
    off_track: hex(0xc8403a),
    no_health: hex(0x9a9893),
    done: hex(0x6b6a66),
    today: hex(0xc8403a),
    selected_row: hex(0xe8eefb),
    hover_row: hex(0xf0f0ed),
    danger_subtle: hex(0xfbe9e8),
    tooltip: hex(0x25292e),
};

pub const DARK: Palette = Palette {
    dark: true,
    bg: hex(0x171716),
    surface: hex(0x20201f),
    surface_2: hex(0x2a2a28),
    border: hex(0x3a3a37),
    text: hex(0xecebe8),
    muted: hex(0xa3a29d),
    accent: hex(0x7aa5f0),
    on_track: hex(0x5cc28a),
    at_risk: hex(0xe6b04a),
    off_track: hex(0xec6d66),
    no_health: hex(0x7d7b76),
    done: hex(0xa3a29d),
    today: hex(0xec6d66),
    selected_row: hex(0x1f2a3d),
    hover_row: hex(0x2a2a28),
    danger_subtle: hex(0x3a2422),
    tooltip: hex(0x25292e),
};

pub fn palette(ctx: &egui::Context) -> &'static Palette {
    match ctx.theme() {
        egui::Theme::Dark => &DARK,
        egui::Theme::Light => &LIGHT,
    }
}

pub fn status_color(category: &str, p: &Palette) -> Color32 {
    match category {
        "indeterminate" => p.accent,
        "done" => p.on_track,
        _ => p.muted,
    }
}

fn visuals(p: &Palette) -> Visuals {
    let mut v = if p.dark { Visuals::dark() } else { Visuals::light() };
    v.panel_fill = p.bg;
    v.window_fill = p.surface;
    v.extreme_bg_color = p.surface;
    v.faint_bg_color = p.surface_2;
    v.override_text_color = Some(p.text);
    v.hyperlink_color = p.accent;
    v.selection.bg_fill = p.selected_row;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    v.widgets.inactive.bg_fill = p.surface_2;
    v.widgets.inactive.weak_bg_fill = p.surface_2;
    v.widgets.hovered.bg_fill = p.hover_row;
    v.widgets.hovered.weak_bg_fill = p.hover_row;
    v.widgets.active.bg_fill = p.selected_row;
    v.widgets.active.weak_bg_fill = p.selected_row;
    v
}

pub fn apply(ctx: &egui::Context) {
    ctx.set_visuals_of(egui::Theme::Light, visuals(&LIGHT));
    ctx.set_visuals_of(egui::Theme::Dark, visuals(&DARK));
    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        s.text_styles.insert(egui::TextStyle::Body, body(14.0));
        s.text_styles.insert(egui::TextStyle::Button, body(14.0));
        s.text_styles.insert(egui::TextStyle::Small, body(12.0));
        s.text_styles.insert(egui::TextStyle::Monospace, mono(12.5));
        s.text_styles.insert(egui::TextStyle::Heading, bold(20.0));
    });
    ctx.set_fonts(fonts());
}

struct Face {
    path: &'static str,
    index: u32,
}

#[cfg(target_os = "macos")]
const REGULAR: &[Face] = &[Face { path: "/System/Library/Fonts/SFNS.ttf", index: 0 }, Face { path: "/System/Library/Fonts/HelveticaNeue.ttc", index: 0 }];
#[cfg(target_os = "macos")]
const SEMIBOLD: &[Face] = &[Face { path: "/System/Library/Fonts/HelveticaNeue.ttc", index: 1 }];
#[cfg(target_os = "macos")]
const MONO: &[Face] = &[Face { path: "/System/Library/Fonts/SFNSMono.ttf", index: 0 }, Face { path: "/System/Library/Fonts/Menlo.ttc", index: 0 }];
#[cfg(not(target_os = "macos"))]
const REGULAR: &[Face] = &[Face { path: "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", index: 0 }];
#[cfg(not(target_os = "macos"))]
const SEMIBOLD: &[Face] = &[Face { path: "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf", index: 0 }];
#[cfg(not(target_os = "macos"))]
const MONO: &[Face] = &[Face { path: "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf", index: 0 }];

fn load(faces: &[Face]) -> Option<FontData> {
    faces.iter().find_map(|f| {
        let bytes = std::fs::read(f.path).ok()?;
        let mut data = FontData::from_owned(bytes);
        data.index = f.index;
        Some(data)
    })
}

fn fonts() -> FontDefinitions {
    let mut defs = FontDefinitions::default();
    let builtin_prop = defs.families[&FontFamily::Proportional].clone();
    let builtin_mono = defs.families[&FontFamily::Monospace].clone();
    let mut family = |name: &str, faces: &[Face], fallback: &[String]| -> Vec<String> {
        let mut list = Vec::new();
        if let Some(data) = load(faces) {
            defs.font_data.insert(name.to_string(), Arc::new(data));
            list.push(name.to_string());
        }
        list.extend(fallback.iter().cloned());
        list
    };
    let prop = family("system", REGULAR, &builtin_prop);
    let bold = family("system-bold", SEMIBOLD, &prop);
    let mono = family("system-mono", MONO, &builtin_mono);
    defs.families.insert(FontFamily::Proportional, prop);
    defs.families.insert(FontFamily::Name(BOLD.into()), bold);
    defs.families.insert(FontFamily::Monospace, mono);
    defs
}
```

- [ ] **Step 2: Write app.rs**

```rust
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

pub enum Msg {
    Login(Option<Login>),
    Me(jira::Result<User>),
    List { id: ListId, result: jira::Result<Vec<Issue>>, stale: bool },
    Detail { key: String, result: jira::Result<Issue> },
    /// A write finished. `what` names it for the toast ("Transition", "Rank", …).
    Done { key: String, what: &'static str, result: jira::Result<()> },
    Transitions { key: String, result: jira::Result<Vec<Transition>> },
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
        for f in [&cfg.roadmap.health_field, &cfg.roadmap.target_field, &cfg.roadmap.start_field, &cfg.roadmap.confidence_field, &cfg.roadmap.next_milestone_field, &cfg.roadmap.definition_of_done_field]
            .into_iter()
            .flatten()
        {
            fields.push(f.clone());
        }
        self.load_list(ListId::Epics, store::jql_epics(&cfg, &parents), fields);
    }

    pub fn load_list(&mut self, id: ListId, jql: String, fields: Vec<String>) {
        let entry = self.lists.entry(id).or_default();
        if entry.loading {
            return;
        }
        entry.loading = true;
        self.spawn(move |c, tx| {
            let fields: Vec<&str> = fields.iter().map(String::as_str).collect();
            let _ = tx.send(Msg::List { id, result: c.search(&jql, &fields), stale: false });
        });
    }

    pub fn load_detail(&mut self, key: &str) {
        if !self.detail_loading.insert(key.to_string()) {
            return;
        }
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
            Msg::List { id, result, stale: _ } => {
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
                        if id == ListId::TeamOpen {
                            self.load_epics();
                        }
                    }
                    Err(jira::Error::Unauthorized(m)) => {
                        self.client = None;
                        self.token_error = Some(m);
                    }
                    Err(e) => {
                        entry.error = Some(e.to_string());
                        self.offline = true;
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
                match result {
                    Ok(()) => self.toast(format!("{what} done"), false),
                    Err(e) => self.toast(format!("{what} failed: {e}"), true),
                }
                self.load_detail(&key);
                self.refresh_all();
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
        if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(cmd, Key::W))) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
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
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
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

        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |ui| {
            if self.client.is_some() {
                self.shortcuts(ctx);
                views::main(self, ui);
            } else {
                views::sign_in(self, ui);
            }
        });
        views::toast(self, ctx);

        for a in std::mem::take(&mut self.actions) {
            self.apply(a, ctx);
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
```

- [ ] **Step 3: Write views/mod.rs**

```rust
//! Header with the screen switcher, sign-in, toast, shared widgets.

pub mod issues;

use crate::app::{Action, App, Screen};
use crate::theme::{self, Palette};
use egui::{Color32, Frame, Margin, RichText, Sense, Stroke, Ui, vec2};

pub fn main(app: &mut App, ui: &mut Ui) {
    let p = theme::palette(ui.ctx());
    header(app, ui, p);
    if app.offline {
        offline_banner(app, ui, p);
    }
    match app.screen {
        Screen::Issues => issues::show(app, ui),
        Screen::Grooming => placeholder(ui, p, "Grooming arrives in plan 2."),
        Screen::Team => placeholder(ui, p, "Team view arrives in plan 3."),
        Screen::Roadmap => placeholder(ui, p, "Roadmap arrives in plan 3."),
    }
}

fn placeholder(ui: &mut Ui, p: &Palette, text: &str) {
    ui.vertical_centered(|ui| {
        ui.add_space(80.0);
        ui.label(RichText::new(text).color(p.muted).size(16.0));
    });
}

fn header(app: &mut App, ui: &mut Ui, p: &Palette) {
    egui::Panel::top("header").exact_size(48.0).frame(Frame::new().fill(p.surface).inner_margin(Margin::symmetric(16, 0)).stroke(Stroke::new(1.0, p.border))).show(ui, |ui| {
        ui.horizontal_centered(|ui| {
            ui.label(RichText::new("Jira Tickets").font(theme::bold(15.0)).color(p.text));
            ui.add_space(16.0);
            for (i, (s, name)) in [(Screen::Issues, "Issues"), (Screen::Grooming, "Grooming"), (Screen::Team, "Team"), (Screen::Roadmap, "Roadmap")].into_iter().enumerate() {
                let active = app.screen == s;
                let text = RichText::new(name).font(if active { theme::bold(14.0) } else { theme::body(14.0) }).color(if active { p.text } else { p.muted });
                let r = ui.add(egui::Label::new(text).sense(Sense::click())).on_hover_text(format!("⌘{}", i + 1));
                if active {
                    let rect = r.rect;
                    ui.painter().hline(rect.x_range(), rect.bottom() + 14.0, Stroke::new(2.0, p.accent));
                }
                if r.clicked() {
                    app.actions.push(Action::SetScreen(s));
                }
                ui.add_space(6.0);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(me) = &app.me {
                    let r = ui.label(RichText::new(&me.display_name).color(p.muted).size(13.0)).interact(Sense::click());
                    r.context_menu(|ui| {
                        if ui.button("Sign out").clicked() {
                            app.actions.push(Action::SignOut);
                        }
                    });
                }
                ui.add_space(8.0);
                if app.lists.values().any(|l| l.loading) {
                    ui.add(egui::Spinner::new().size(16.0).color(p.muted));
                } else if ui.add(button("Refresh", p)).on_hover_text("⌘R").clicked() {
                    app.actions.push(Action::Refresh);
                }
                if crate::jira::READ_ONLY.load(std::sync::atomic::Ordering::Relaxed) {
                    pill(ui, "read-only", p.danger_subtle, p.off_track);
                }
            });
        });
    });
}

fn offline_banner(app: &mut App, ui: &mut Ui, p: &Palette) {
    let when = app.store.fetched_at.and_then(|t| chrono::DateTime::from_timestamp(t, 0)).map(|t| t.with_timezone(&chrono::Local).format("%H:%M").to_string()).unwrap_or("earlier".into());
    Frame::new().fill(p.danger_subtle).inner_margin(Margin::symmetric(16, 6)).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("Offline. Showing data from {when}.")).color(p.off_track).size(13.0));
            if ui.add(button("Retry", p)).clicked() {
                app.actions.push(Action::Refresh);
            }
        });
    });
}

pub fn sign_in(app: &mut App, ui: &mut Ui) {
    let p = theme::palette(ui.ctx());
    ui.vertical_centered(|ui| {
        ui.add_space((ui.available_height() / 2.0 - 160.0).max(40.0));
        ui.label(RichText::new("Sign in to Jira").font(theme::body(24.0)).color(p.text));
        ui.add_space(16.0);
        if app.finding_token {
            ui.add(egui::Spinner::new().size(20.0));
            ui.label(RichText::new("Looking for your Jira token…").color(p.muted));
            return;
        }
        Frame::new().fill(p.surface).stroke(Stroke::new(1.0, p.border)).corner_radius(6).inner_margin(Margin::same(16)).show(ui, |ui| {
            ui.set_width(380.0);
            ui.vertical(|ui| {
                if let Some(e) = &app.token_error {
                    ui.label(RichText::new(e).color(p.off_track).size(13.0));
                    ui.label(
                        RichText::new(format!(
                            "Site: {}\nEmail: {}\nCheck that the email matches your Atlassian account and the token is valid (https://id.atlassian.com/manage-profile/security/api-tokens).",
                            app.cfg.site, app.cfg.email
                        ))
                        .color(p.muted)
                        .size(12.0),
                    );
                    ui.add_space(8.0);
                }
                ui.label(RichText::new("No JIRA_API_TOKEN, saved token, or config api_token was found. Paste an Atlassian API token.").size(13.0).color(p.muted));
                ui.add_space(10.0);
                let r = ui.add(egui::TextEdit::singleline(&mut app.token_input).password(true).desired_width(f32::INFINITY).margin(vec2(8.0, 6.0)));
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    app.actions.push(Action::SubmitToken);
                }
                ui.add_space(12.0);
                let w = ui.available_width();
                if ui.add_sized([w, 32.0], primary_button("Sign in", p)).clicked() {
                    app.actions.push(Action::SubmitToken);
                }
            });
        });
        ui.add_space(8.0);
        if ui.add(button("Look again", p)).clicked() {
            app.actions.push(Action::FindToken);
        }
    });
}

pub fn toast(app: &mut App, ctx: &egui::Context) {
    let Some(t) = &app.toast else { return };
    if std::time::Instant::now() > t.until {
        app.toast = None;
        return;
    }
    ctx.request_repaint_after(std::time::Duration::from_millis(250));
    let p = theme::palette(ctx);
    egui::Area::new(egui::Id::new("toast")).anchor(egui::Align2::CENTER_BOTTOM, vec2(0.0, -24.0)).show(ctx, |ui| {
        Frame::new().fill(p.tooltip).corner_radius(6).inner_margin(Margin::symmetric(14, 10)).show(ui, |ui| {
            let color = if t.error { Color32::from_rgb(0xff, 0x81, 0x82) } else { Color32::from_rgb(0x4a, 0xc2, 0x6b) };
            ui.horizontal(|ui| {
                health_dot(ui, color);
                ui.label(RichText::new(&t.text).color(Color32::WHITE).size(14.0));
            });
        });
    });
}

// ---------- Shared widgets ----------

pub fn button(text: &str, p: &Palette) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text.to_string()).color(p.text).size(13.0)).fill(p.surface_2).stroke(Stroke::new(1.0, p.border)).corner_radius(6)
}

pub fn primary_button(text: &str, p: &Palette) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text.to_string()).color(Color32::WHITE).size(13.0)).fill(p.accent).stroke(Stroke::NONE).corner_radius(6)
}

pub fn pill(ui: &mut Ui, text: &str, bg: Color32, fg: Color32) -> egui::Response {
    Frame::new().fill(bg).corner_radius(10).inner_margin(Margin::symmetric(7, 1)).show(ui, |ui| ui.label(RichText::new(text).color(fg).size(11.0))).response
}

pub fn health_dot(ui: &mut Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(9.0, 9.0), Sense::hover());
    ui.painter().circle_filled(rect.center(), 4.5, color);
}

/// Returns true when Retry is clicked.
pub fn error_banner(ui: &mut Ui, p: &Palette, title: &str, raw: &str) -> bool {
    let mut retry = false;
    Frame::new().fill(p.danger_subtle).corner_radius(6).inner_margin(Margin::same(10)).show(ui, |ui| {
        ui.label(RichText::new(title).font(theme::bold(14.0)).color(p.off_track));
        ui.label(RichText::new(raw).color(p.muted).size(12.0));
        retry = ui.add(button("Retry", p)).clicked();
    });
    retry
}
```

Create `src/views/issues.rs` with a stub so this task compiles; Task 9 replaces it:

```rust
use crate::app::App;
use egui::Ui;

pub fn show(_app: &mut App, ui: &mut Ui) {
    ui.label("Issues");
}
```

- [ ] **Step 4: Rewrite main.rs**

```rust
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod adf;
mod app;
mod auth;
mod cache;
mod config;
mod jira;
#[cfg(feature = "snapshot")]
mod snapshot;
mod store;
mod theme;
mod views;

fn main() -> eframe::Result {
    let path = config::path();
    let cfg = match config::load(&path) {
        Ok(c) => c,
        Err(e) if !path.exists() => {
            if let Err(w) = config::write_example(&path) {
                eprintln!("Error writing example config: {w}");
                std::process::exit(1);
            }
            println!("Welcome to Jira Tickets!\n\nA config file has been created at:\n  {}\n", path.display());
            println!("Edit it: set site and email, add an api_token (plain, op://…, or $(cmd)) or export JIRA_API_TOKEN,");
            println!("then run jira-tickets again.\n\n  $EDITOR {}\n\n({e})", path.display());
            return Ok(());
        }
        Err(e) => {
            eprintln!("Error loading config: {e}\n\nConfig file: {}", path.display());
            std::process::exit(1);
        }
    };
    #[cfg(feature = "snapshot")]
    if let Ok(out) = std::env::var("JIRA_TICKETS_SCREENSHOT") {
        snapshot::run(&out, cfg);
        return Ok(());
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_title("Jira Tickets").with_app_id("jira-tickets").with_inner_size([1440.0, 900.0]).with_min_inner_size([900.0, 560.0]),
        run_and_return: false,
        ..Default::default()
    };
    eframe::run_native("jira-tickets", options, Box::new(|cc| Ok(Box::new(app::App::new(cc, cfg)))))
}
```

Leave `mod snapshot` behind the feature; the file is created in Task 12.

- [ ] **Step 5: Build, run tests, clippy**

Run: `ndc cargo build 2>&1 | tail -3 && ndc cargo test 2>&1 | tail -3 && ndc cargo clippy --all-targets -- -D warnings 2>&1 | tail -3`
Expected: builds; all earlier tests pass; no warnings. Fix any egui 0.36 API drift by checking the egui source under `~/.cargo/registry/src` inside the nix shell (`ndc sh -c 'ls ~/.cargo/registry/src/*/ | rg egui'`).

- [ ] **Step 6: Manual check with a temp config**

Write a config pointing at the real site but with an invalid token path so sign-in shows:

Run:
```sh
printf 'site = "https://example.atlassian.net"\nemail = "you@example.com"\n' > /tmp/jt-config.toml
JIRA_TICKETS_CONFIG=/tmp/jt-config.toml JIRA_TICKETS_TOKEN_FILE=/tmp/jt-token JIRA_TICKETS_READONLY=1 ndc cargo run 2>&1 | tail -3
```
Expected: a window opens with "Sign in to Jira" and a token box. Paste a token (or set `JIRA_API_TOKEN`) and confirm the header shows your display name, the Refresh spinner runs, and the "read-only" pill is visible. Close with ⌘W. If `op` is unlocked, instead put `api_token = "op://..."` in the temp config and confirm the Source::ConfigRef path saves `/tmp/jt-token`.

- [ ] **Step 7: Commit**

```bash
git add src/theme.rs src/app.rs src/views src/main.rs
git commit -m "feat: add app shell with theme, sign-in, header, and toasts"
```

---

### Task 9: Issues list with filters, search, keyboard navigation, and ⌘K

**Files:**
- Create: `src/views/issues.rs` (replace the stub), `src/filter.rs`
- Modify: `src/app.rs` (new `Action`s and state), `src/main.rs` (add `mod filter;`)

**Interfaces:**
- Produces (`filter.rs`, pure and tested):
  - `pub struct IssueFilter { pub epic: Option<String>, pub quick: Option<usize>, pub mine: bool, pub text: String }`
  - `pub fn visible<'a>(issues: &[&'a Issue], f: &IssueFilter, me: Option<&str>) -> Vec<&'a Issue>`: epic matches `parent_key`; `mine` matches assignee display name == me; `text` is a case-insensitive substring over key, summary, labels, assignee (quick filters run as JQL, see below).
  - `pub fn column_value(issue: &Issue, field: &str) -> String`: `status`, `priority`, `labels` (comma joined), `assignee`, `issuetype`, `parent.summary`, `parent.key`, `sprint`, `points`, `customfield_*` (string or `.value`/`.name` of an object, numbers formatted).
  - `pub fn parse_goto(text: &str, project: &str) -> Option<String>`: `"9001"` → `PROJECT-9001`, `"CI-9001"` → itself, a browse URL → its key.
- Produces (`app.rs` additions): `pub filter: IssueFilter`, `pub focus_search: bool`, `pub goto: Option<String>` (the ⌘K box text when open), `pub cursor: usize` (list row index), and `Action::{SetEpic(Option<String>), SetQuick(Option<usize>), ToggleMine, RunSearch(String), ToggleGoTo, GoTo, Move(i32)}`. `RunSearch(jql)` loads `ListId::Search` with `project = {project} AND ({jql}) ORDER BY Rank ASC` and switches the list source to Search; an empty search returns to TeamOpen. A quick filter sets `RunSearch` with `labels = PLAT AND ({quick jql})`.
- Produces (`views/issues.rs`): `pub fn show(app: &mut App, ui: &mut Ui)`; `pub fn current_rows(app: &App) -> Vec<&Issue>` (Search list if active, else TeamOpen, then `filter::visible`).

- [ ] **Step 1: Write filter.rs tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::jira::{Issue, Status, User};

    fn issue(key: &str, summary: &str, parent: Option<&str>, assignee: Option<&str>, labels: &[&str]) -> Issue {
        Issue {
            key: key.into(),
            summary: summary.into(),
            parent_key: parent.map(String::from),
            assignee: assignee.map(|a| User { account_id: a.into(), display_name: a.into() }),
            labels: labels.iter().map(|l| l.to_string()).collect(),
            status: Status { name: "New".into(), category: "new".into() },
            priority: "High".into(),
            ..Default::default()
        }
    }

    #[test]
    fn filters_compose() {
        let a = issue("CI-1", "Remove the old autoscaler", Some("E-1"), Some("Ada"), &["PLAT"]);
        let b = issue("CI-2", "Deployer overlays", Some("E-2"), None, &["PLAT", "infra"]);
        let all = vec![&a, &b];
        let mut f = IssueFilter::default();
        assert_eq!(visible(&all, &f, Some("Ada")).len(), 2);
        f.epic = Some("E-1".into());
        assert_eq!(visible(&all, &f, None)[0].key, "CI-1");
        f.epic = None;
        f.mine = true;
        assert_eq!(visible(&all, &f, Some("Ada")).len(), 1);
        f.mine = false;
        f.text = "INFRA".into();
        assert_eq!(visible(&all, &f, None)[0].key, "CI-2");
    }

    #[test]
    fn column_values() {
        let mut i = issue("CI-1", "x", Some("E-1"), Some("Ada"), &["a", "b"]);
        i.parent_summary = Some("Epic one".into());
        i.custom.insert("customfield_1".into(), serde_json::json!({ "value": "Opt" }));
        i.custom.insert("customfield_2".into(), serde_json::json!(3.0));
        assert_eq!(column_value(&i, "labels"), "a, b");
        assert_eq!(column_value(&i, "assignee"), "Ada");
        assert_eq!(column_value(&i, "parent.summary"), "Epic one");
        assert_eq!(column_value(&i, "customfield_1"), "Opt");
        assert_eq!(column_value(&i, "customfield_2"), "3");
        assert_eq!(column_value(&i, "nope"), "");
    }

    #[test]
    fn goto_parsing() {
        assert_eq!(parse_goto("9001", "CI").as_deref(), Some("CI-9001"));
        assert_eq!(parse_goto("ci-9001", "CI").as_deref(), Some("CI-9001"));
        assert_eq!(parse_goto("https://example.atlassian.net/browse/CI-9001?x=1", "CI").as_deref(), Some("CI-9001"));
        assert_eq!(parse_goto("", "CI"), None);
    }
}
```

- [ ] **Step 2: Run to verify failure**

Add `mod filter;` to `src/main.rs`. Run: `ndc cargo test filter 2>&1 | tail -3`
Expected: compile errors.

- [ ] **Step 3: Write filter.rs**

```rust
//! Pure list filtering, column formatting, and ⌘K parsing for the Issues view.

use crate::jira::Issue;

#[derive(Default, Clone, Debug)]
pub struct IssueFilter {
    pub epic: Option<String>,
    pub quick: Option<usize>,
    pub mine: bool,
    pub text: String,
}

pub fn visible<'a>(issues: &[&'a Issue], f: &IssueFilter, me: Option<&str>) -> Vec<&'a Issue> {
    let needle = f.text.trim().to_lowercase();
    issues
        .iter()
        .copied()
        .filter(|i| f.epic.as_deref().is_none_or(|e| i.parent_key.as_deref() == Some(e)))
        .filter(|i| !f.mine || i.assignee.as_ref().map(|a| a.display_name.as_str()) == me)
        .filter(|i| {
            needle.is_empty()
                || i.key.to_lowercase().contains(&needle)
                || i.summary.to_lowercase().contains(&needle)
                || i.labels.iter().any(|l| l.to_lowercase().contains(&needle))
                || i.assignee.as_ref().is_some_and(|a| a.display_name.to_lowercase().contains(&needle))
        })
        .collect()
}

pub fn column_value(issue: &Issue, field: &str) -> String {
    match field {
        "status" => issue.status.name.clone(),
        "priority" => issue.priority.clone(),
        "labels" => issue.labels.join(", "),
        "assignee" => issue.assignee.as_ref().map(|a| a.display_name.clone()).unwrap_or_default(),
        "issuetype" => issue.issue_type.clone(),
        "parent.summary" => issue.parent_summary.clone().unwrap_or_default(),
        "parent.key" => issue.parent_key.clone().unwrap_or_default(),
        "sprint" => issue.sprint.clone().unwrap_or_default(),
        "points" => issue.story_points.map(fmt_num).unwrap_or_default(),
        f if f.starts_with("customfield_") => issue.custom.get(f).map(json_text).unwrap_or_default(),
        _ => String::new(),
    }
}

fn fmt_num(n: f64) -> String {
    if n.fract() == 0.0 { format!("{}", n as i64) } else { format!("{n}") }
}

fn json_text(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.as_f64().map(fmt_num).unwrap_or_default(),
        serde_json::Value::Object(o) => o.get("value").or(o.get("name")).or(o.get("displayName")).map(json_text).unwrap_or_default(),
        serde_json::Value::Array(a) => a.iter().map(json_text).collect::<Vec<_>>().join(", "),
        _ => String::new(),
    }
}

pub fn parse_goto(text: &str, project: &str) -> Option<String> {
    let t = text.trim();
    if t.is_empty() {
        return None;
    }
    if let Some(rest) = t.split("/browse/").nth(1) {
        let key: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '-').collect();
        return (!key.is_empty()).then(|| key.to_uppercase());
    }
    if t.chars().all(|c| c.is_ascii_digit()) {
        return Some(format!("{project}-{t}"));
    }
    let (p, n) = t.split_once('-')?;
    (n.chars().all(|c| c.is_ascii_digit()) && !n.is_empty()).then(|| format!("{}-{n}", p.to_uppercase()))
}
```

- [ ] **Step 4: Run filter tests**

Run: `ndc cargo test filter 2>&1 | tail -3`
Expected: `3 passed`

- [ ] **Step 5: Add app state and actions**

In `app.rs`:

```rust
// fields on App:
pub filter: crate::filter::IssueFilter,
pub focus_search: bool,
pub goto: Option<String>,
pub cursor: usize,
pub search_active: bool,

// Action variants:
SetEpic(Option<String>),
SetQuick(Option<usize>),
ToggleMine,
RunSearch(String),
ToggleGoTo,
GoTo,
Move(i32),
```

Initialise them in `App::new` (`filter: Default::default(), focus_search: false, goto: None, cursor: 0, search_active: false`). In `apply`:

```rust
Action::SetEpic(e) => { self.filter.epic = e; self.cursor = 0; }
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
Action::ToggleMine => { self.filter.mine = !self.filter.mine; self.cursor = 0; }
Action::RunSearch(jql) => {
    self.cursor = 0;
    if jql.trim().is_empty() {
        self.search_active = false;
        self.store.lists.remove(&ListId::Search);
        return;
    }
    self.search_active = true;
    let full = format!("project = {} AND ({}) ORDER BY Rank ASC", store::jql_quote(&self.cfg.scope.project), jql.trim());
    let fields: Vec<String> = jira::LIST_FIELDS.iter().map(|s| s.to_string()).collect();
    self.lists.entry(ListId::Search).or_default().loading = false;
    self.load_list(ListId::Search, full, fields);
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
    if n == 0 { return; }
    self.cursor = (self.cursor as i32 + delta).clamp(0, n as i32 - 1) as usize;
    if let Some(key) = views::issues::current_rows(self).get(self.cursor).map(|i| i.key.clone()) {
        self.actions.push(Action::Select(key));
    }
}
```

`Msg::List` for `ListId::Search` must not trigger `load_epics` (the existing `if id == ListId::TeamOpen` guard already handles this). In `shortcuts`, add: `⌘K` → `ToggleGoTo`; `⌘F` and `/` (when no text field has focus: `!ctx.wants_keyboard_input()`) → `self.focus_search = true`; `j`/`↓` → `Move(1)`, `k`/`↑` → `Move(-1)`; `c` → `Copy(browse_url(selected))`; `⌘O` → `OpenUrl(browse_url(selected))`; `Esc` closes the goto box. Only handle the letter keys when `!ctx.wants_keyboard_input()`.

- [ ] **Step 6: Write views/issues.rs (list half)**

```rust
//! Issues: list on the left, selected issue on the right.

use crate::app::{Action, App};
use crate::filter;
use crate::jira::Issue;
use crate::store::ListId;
use crate::theme::{self, Palette};
use crate::views::{button, pill};
use egui::{Frame, Margin, RichText, Sense, Stroke, Ui, vec2};

pub fn current_rows(app: &App) -> Vec<&Issue> {
    let src = if app.search_active { ListId::Search } else { ListId::TeamOpen };
    let all = app.store.list(src);
    filter::visible(&all, &app.filter, app.me.as_ref().map(|m| m.display_name.as_str()))
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let p = theme::palette(ui.ctx());
    let width = ui.available_width();
    let list_w = (width * 0.42).clamp(360.0, 720.0);
    egui::Panel::left("issue-list").resizable(true).default_size(list_w).size_range(320.0..=list_w.max(320.0)).frame(Frame::new().fill(p.bg).inner_margin(Margin { left: 16, right: 12, top: 12, bottom: 0 })).show_inside(ui, |ui| {
        filters(app, ui, p);
        ui.add_space(8.0);
        list(app, ui, p);
    });
    egui::CentralPanel::default().frame(Frame::new().fill(p.surface).inner_margin(Margin { left: 24, right: 24, top: 16, bottom: 0 })).show_inside(ui, |ui| super::detail::show(app, ui));
    goto_box(app, ui.ctx(), p);
}

fn filters(app: &mut App, ui: &mut Ui, p: &Palette) {
    ui.horizontal_wrapped(|ui| {
        // Epic picker: epics seen among TeamOpen parents, by summary.
        let mut epics: Vec<(String, String)> = app.store.list(ListId::TeamOpen).iter().filter_map(|i| Some((i.parent_key.clone()?, i.parent_summary.clone().unwrap_or_default()))).collect();
        epics.sort();
        epics.dedup();
        let label = app.filter.epic.as_ref().and_then(|k| epics.iter().find(|(key, _)| key == k)).map(|(k, s)| format!("{k} · {s}")).unwrap_or("All epics".into());
        egui::ComboBox::from_id_salt("epic").selected_text(label).width(220.0).show_ui(ui, |ui| {
            if ui.selectable_label(app.filter.epic.is_none(), "All epics").clicked() {
                app.actions.push(Action::SetEpic(None));
            }
            for (k, s) in &epics {
                if ui.selectable_label(app.filter.epic.as_deref() == Some(k), format!("{k} · {s}")).clicked() {
                    app.actions.push(Action::SetEpic(Some(k.clone())));
                }
            }
        });
        for (i, f) in app.cfg.issues.filters.clone().iter().enumerate() {
            let active = app.filter.quick == Some(i);
            if ui.add(button(&f.name, p).fill(if active { p.selected_row } else { p.surface_2 })).clicked() {
                app.actions.push(Action::SetQuick(if active { None } else { Some(i) }));
            }
        }
        if ui.add(button("Mine", p).fill(if app.filter.mine { p.selected_row } else { p.surface_2 })).clicked() {
            app.actions.push(Action::ToggleMine);
        }
    });
    ui.add_space(6.0);
    let r = ui.add(egui::TextEdit::singleline(&mut app.filter.text).hint_text("Filter, or type JQL and press Enter (⌘F)").desired_width(f32::INFINITY).margin(vec2(8.0, 6.0)));
    if app.focus_search {
        r.request_focus();
        app.focus_search = false;
    }
    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
        let t = app.filter.text.trim().to_string();
        // JQL has an operator; a bare word is a local text filter.
        if t.contains('=') || t.contains(" in ") || t.contains('~') || t.contains('!') {
            app.filter.text.clear();
            app.actions.push(Action::RunSearch(t));
        }
    }
}

fn list(app: &mut App, ui: &mut Ui, p: &Palette) {
    let rows: Vec<Issue> = current_rows(app).into_iter().cloned().collect();
    let src = if app.search_active { ListId::Search } else { ListId::TeamOpen };
    let loaded = app.lists.get(&src);
    if let Some(e) = loaded.and_then(|l| l.error.clone())
        && rows.is_empty()
        && super::error_banner(ui, p, "Couldn't load issues.", &e)
    {
        app.actions.push(Action::Refresh);
    }
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("{} issues", rows.len())).color(p.muted).size(12.0));
        if app.search_active && ui.add(button("Clear search", p)).clicked() {
            app.actions.push(Action::RunSearch(String::new()));
        }
    });
    let columns = app.cfg.issues.columns.clone();
    let selected = app.selected.clone();
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for (idx, i) in rows.iter().enumerate() {
            let is_sel = selected.as_deref() == Some(&i.key);
            let fill = if is_sel { p.selected_row } else if idx == app.cursor { p.hover_row } else { p.surface };
            let r = Frame::new().fill(fill).stroke(Stroke::new(1.0, p.border)).corner_radius(6).inner_margin(Margin::symmetric(10, 8)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&i.key).font(theme::mono(12.0)).color(p.muted));
                    ui.label(RichText::new(&i.summary).font(theme::bold(13.5)).color(p.text));
                });
                ui.horizontal_wrapped(|ui| {
                    for c in &columns {
                        let v = filter::column_value(i, c);
                        if v.is_empty() {
                            continue;
                        }
                        let (bg, fg) = if c == "status" { (p.surface_2, theme::status_color(&i.status.category, p)) } else { (p.surface_2, p.muted) };
                        pill(ui, &v, bg, fg);
                    }
                });
            });
            let r = r.response.interact(Sense::click());
            if r.clicked() {
                app.cursor = idx;
                app.actions.push(Action::Select(i.key.clone()));
            }
            if r.double_clicked() {
                app.actions.push(Action::OpenUrl(app.cfg.browse_url(&i.key)));
            }
            if idx == app.cursor && ui.input(|inp| inp.key_pressed(egui::Key::J) || inp.key_pressed(egui::Key::K)) {
                r.scroll_to_me(None);
            }
            ui.add_space(4.0);
        }
    });
}

fn goto_box(app: &mut App, ctx: &egui::Context, p: &Palette) {
    let Some(text) = app.goto.as_mut() else { return };
    egui::Area::new(egui::Id::new("goto")).anchor(egui::Align2::CENTER_TOP, vec2(0.0, 80.0)).show(ctx, |ui| {
        Frame::new().fill(p.surface).stroke(Stroke::new(1.0, p.border)).corner_radius(8).inner_margin(Margin::same(12)).show(ui, |ui| {
            ui.set_width(420.0);
            ui.label(RichText::new("Go to issue").font(theme::bold(13.0)));
            let r = ui.add(egui::TextEdit::singleline(text).hint_text("9001, CI-9001, or a browse link").desired_width(f32::INFINITY));
            r.request_focus();
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                app.actions.push(Action::GoTo);
            }
        });
    });
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        app.goto = None;
    }
}
```

Create `src/views/detail.rs` with a stub for now (Task 10 fills it) and add `pub mod detail;` to `views/mod.rs`:

```rust
use crate::app::App;
use egui::Ui;

pub fn show(app: &mut App, ui: &mut Ui) {
    if let Some(k) = &app.selected {
        ui.label(k);
    }
}
```

- [ ] **Step 7: Build, run, verify by hand**

Run: `ndc cargo clippy --all-targets -- -D warnings 2>&1 | tail -3 && JIRA_TICKETS_READONLY=1 ndc cargo run 2>&1 | tail -2`
Expected with a valid token: PLAT issues listed with status pills; epic picker filters; `j`/`k` move the cursor and select; typing a word filters locally; typing `status = New` then Enter runs a JQL search and "Clear search" appears; `⌘K` then `9001` selects CI-9001; double-click opens the browser.

- [ ] **Step 8: Commit**

```bash
git add src/filter.rs src/views src/app.rs src/main.rs
git commit -m "feat: add Issues list with filters, search, and keyboard navigation"
```

---

### Task 10: Issue detail with ADF rendering and comments

**Files:**
- Create: `src/views/adf_view.rs`
- Modify: `src/views/detail.rs` (replace stub), `src/views/mod.rs` (add `pub mod adf_view;`), `src/app.rs` (`composer`, `Action::Comment`)

**Interfaces:**
- Produces (`adf_view.rs`): `pub fn show(ui: &mut Ui, doc: &adf::Doc, p: &Palette)`; `pub fn show_value(ui: &mut Ui, v: &serde_json::Value, p: &Palette)` (parses then shows; empty value shows "No description" in muted).
- Produces (`app.rs`): `pub composer: String`, `pub posting: bool`, `Action::Comment` → spawns `add_comment(selected, composer)`, sends `Msg::Done { what: "Comment" }`, clears the composer on success.
- Produces (`detail.rs`): `pub fn show(app: &mut App, ui: &mut Ui)`.

- [ ] **Step 1: Write adf_view.rs**

```rust
//! Paints an `adf::Doc` with egui widgets.

use crate::adf::{self, Block, Span};
use crate::theme::{self, Palette};
use egui::text::LayoutJob;
use egui::{Frame, Margin, RichText, Stroke, TextFormat, Ui};

pub fn show_value(ui: &mut Ui, v: &serde_json::Value, p: &Palette) {
    let doc = adf::parse(v);
    if doc.blocks.is_empty() {
        ui.label(RichText::new("No description").italics().color(p.muted));
        return;
    }
    show(ui, &doc, p);
}

pub fn show(ui: &mut Ui, doc: &adf::Doc, p: &Palette) {
    for b in &doc.blocks {
        block(ui, b, p, 14.0);
        ui.add_space(6.0);
    }
}

fn block(ui: &mut Ui, b: &Block, p: &Palette, size: f32) {
    match b {
        Block::Paragraph(s) => spans(ui, s, p, size, false),
        Block::Heading { level, spans: s } => {
            let sz = match level { 1 => 22.0, 2 => 18.0, 3 => 16.0, _ => 14.5 };
            spans(ui, s, p, sz, true);
        }
        Block::List { ordered, items } => {
            for (i, item) in items.iter().enumerate() {
                ui.horizontal_top(|ui| {
                    ui.add_space(8.0);
                    let marker = if *ordered { format!("{}.", i + 1) } else { "•".to_string() };
                    ui.label(RichText::new(marker).color(p.muted).size(size));
                    ui.vertical(|ui| {
                        for ib in item {
                            block(ui, ib, p, size);
                        }
                    });
                });
            }
        }
        Block::Code { text, .. } => {
            Frame::new().fill(p.surface_2).corner_radius(6).inner_margin(Margin::same(10)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(text).font(theme::mono(12.5)).color(p.text));
            });
        }
        Block::Quote(bs) => {
            ui.horizontal_top(|ui| {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(3.0, 0.0), egui::Sense::hover());
                let top = rect.top();
                let inner = ui.vertical(|ui| {
                    for ib in bs {
                        block(ui, ib, p, size);
                    }
                });
                let bottom = inner.response.rect.bottom();
                ui.painter().vline(rect.left() + 1.5, top..=bottom, Stroke::new(3.0, p.border));
            });
        }
        Block::Panel { kind, blocks } => {
            let (bg, border) = match kind.as_str() {
                "warning" => (p.at_risk.gamma_multiply(0.15), p.at_risk),
                "error" => (p.off_track.gamma_multiply(0.15), p.off_track),
                "success" => (p.on_track.gamma_multiply(0.15), p.on_track),
                _ => (p.accent.gamma_multiply(0.12), p.accent),
            };
            Frame::new().fill(bg).stroke(Stroke::new(1.0, border)).corner_radius(6).inner_margin(Margin::same(10)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                for ib in blocks {
                    block(ui, ib, p, size);
                }
            });
        }
        Block::Table(rows) => {
            egui::Grid::new(ui.next_auto_id()).striped(true).spacing([12.0, 4.0]).show(ui, |ui| {
                for (ri, row) in rows.iter().enumerate() {
                    for cell in row {
                        spans(ui, cell, p, 13.0, ri == 0);
                    }
                    ui.end_row();
                }
            });
        }
        Block::Rule => {
            ui.separator();
        }
    }
}

fn spans(ui: &mut Ui, s: &[Span], p: &Palette, size: f32, heading: bool) {
    let mut job = LayoutJob::default();
    job.wrap.max_width = ui.available_width();
    for sp in s {
        let mut fmt = TextFormat { font_id: if sp.bold || heading { theme::bold(size) } else { theme::body(size) }, color: p.text, ..Default::default() };
        if sp.code {
            fmt.font_id = theme::mono(size - 1.5);
            fmt.background = p.surface_2;
        }
        if sp.italic {
            fmt.italics = true;
        }
        if sp.strike {
            fmt.strikethrough = Stroke::new(1.0, p.muted);
        }
        if sp.link.is_some() {
            fmt.color = p.accent;
            fmt.underline = Stroke::new(1.0, p.accent);
        }
        job.append(&sp.text, 0.0, fmt);
    }
    let r = ui.add(egui::Label::new(job).sense(egui::Sense::click()));
    // One link per paragraph is the common case; open the first on click.
    if r.clicked()
        && let Some(url) = s.iter().find_map(|sp| sp.link.clone())
    {
        ui.ctx().open_url(egui::OpenUrl::new_tab(url));
    }
}
```

- [ ] **Step 2: Write detail.rs**

```rust
//! The selected issue: summary, status, description, fields, comments, composer.

use crate::app::{Action, App};
use crate::theme::{self, Palette};
use crate::views::{adf_view, button, pill, primary_button};
use egui::{Frame, Margin, RichText, Stroke, Ui, vec2};

pub fn show(app: &mut App, ui: &mut Ui) {
    let p = theme::palette(ui.ctx());
    let Some(key) = app.selected.clone() else {
        ui.vertical_centered(|ui| {
            ui.add_space(120.0);
            ui.label(RichText::new("Select an issue").color(p.muted).size(16.0));
        });
        return;
    };
    let Some(issue) = app.store.get(&key).cloned() else {
        ui.add(egui::Spinner::new());
        return;
    };
    let loading = app.detail_loading.contains(&key);
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(&issue.key).font(theme::mono(13.0)).color(p.muted));
            if let (Some(pk), Some(ps)) = (&issue.parent_key, &issue.parent_summary) {
                ui.label(RichText::new("·").color(p.muted));
                if ui.link(RichText::new(format!("{pk} {ps}")).color(p.muted).size(13.0)).clicked() {
                    app.actions.push(Action::SetEpic(Some(pk.clone())));
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(button("Open in Jira", p)).on_hover_text("⌘O").clicked() {
                    app.actions.push(Action::OpenUrl(app.cfg.browse_url(&issue.key)));
                }
                if ui.add(button("Copy link", p)).on_hover_text("c").clicked() {
                    app.actions.push(Action::Copy(app.cfg.browse_url(&issue.key)));
                }
                if loading {
                    ui.add(egui::Spinner::new().size(14.0));
                }
            });
        });
        ui.label(RichText::new(&issue.summary).font(theme::bold(22.0)).color(p.text));
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            pill(ui, &issue.status.name, p.surface_2, theme::status_color(&issue.status.category, p));
            pill(ui, &issue.issue_type, p.surface_2, p.muted);
            if !issue.priority.is_empty() {
                pill(ui, &issue.priority, p.surface_2, p.muted);
            }
            for l in &issue.labels {
                pill(ui, l, p.surface_2, p.muted);
            }
        });
        ui.add_space(12.0);
        ui.horizontal_top(|ui| {
            let main_w = (ui.available_width() - 260.0).max(360.0);
            ui.allocate_ui_with_layout(vec2(main_w, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.set_width(main_w);
                ui.label(RichText::new("Description").font(theme::bold(12.0)).color(p.muted));
                ui.add_space(4.0);
                match &issue.description {
                    Some(d) => adf_view::show_value(ui, d, p),
                    None if loading => {
                        ui.add(egui::Spinner::new().size(14.0));
                    }
                    None => adf_view::show_value(ui, &serde_json::Value::Null, p),
                }
                ui.add_space(16.0);
                ui.label(RichText::new(format!("Comments ({})", issue.comments.len())).font(theme::bold(12.0)).color(p.muted));
                for c in &issue.comments {
                    ui.add_space(6.0);
                    Frame::new().fill(p.bg).stroke(Stroke::new(1.0, p.border)).corner_radius(6).inner_margin(Margin::same(10)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&c.author).font(theme::bold(13.0)));
                            ui.label(RichText::new(short_date(&c.created)).color(p.muted).size(12.0));
                        });
                        adf_view::show_value(ui, &c.body, p);
                    });
                }
                ui.add_space(10.0);
                ui.add(egui::TextEdit::multiline(&mut app.composer).hint_text("Add a comment (⌘Enter to post)").desired_rows(3).desired_width(f32::INFINITY));
                let post = ui.input_mut(|i| i.consume_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Enter)));
                ui.horizontal(|ui| {
                    let enabled = !app.composer.trim().is_empty() && !app.posting;
                    if ui.add_enabled(enabled, primary_button("Comment", p)).clicked() || (post && enabled) {
                        app.actions.push(Action::Comment);
                    }
                    if app.posting {
                        ui.add(egui::Spinner::new().size(14.0));
                    }
                });
            });
            ui.add_space(16.0);
            ui.vertical(|ui| {
                ui.set_width(220.0);
                field(ui, p, "Assignee", &issue.assignee.as_ref().map(|a| a.display_name.clone()).unwrap_or("Unassigned".into()));
                field(ui, p, "Reporter", &issue.reporter.as_ref().map(|a| a.display_name.clone()).unwrap_or_default());
                field(ui, p, "Sprint", issue.sprint.as_deref().unwrap_or("—"));
                field(ui, p, "Story points", &issue.story_points.map(|n| format!("{n}")).unwrap_or("—".into()));
                field(ui, p, "Due", issue.due_date.as_deref().unwrap_or("—"));
                field(ui, p, "Created", &short_date(&issue.created));
                field(ui, p, "Updated", &short_date(&issue.updated));
                ui.add_space(8.0);
                ui.label(RichText::new("t transition · a assign · s sprint · e edit · J/K rank").color(p.muted).size(11.0));
            });
        });
        ui.add_space(24.0);
    });
}

fn field(ui: &mut Ui, p: &Palette, name: &str, value: &str) {
    ui.label(RichText::new(name).font(theme::bold(11.5)).color(p.muted));
    ui.label(RichText::new(if value.is_empty() { "—" } else { value }).size(13.0).color(p.text));
    ui.add_space(6.0);
}

/// "2026-10-03T09:00:00.000-0700" → "2026-10-03 09:00"
pub fn short_date(s: &str) -> String {
    if s.len() >= 16 { format!("{} {}", &s[..10], &s[11..16]) } else { s.to_string() }
}
```

- [ ] **Step 3: Add composer state and the Comment action**

In `app.rs`: fields `pub composer: String, pub posting: bool` (init empty/false). `Action::Comment`:

```rust
Action::Comment => {
    let (Some(key), false) = (self.selected.clone(), self.posting) else { return };
    let text = self.composer.trim().to_string();
    if text.is_empty() { return; }
    self.posting = true;
    self.spawn(move |c, tx| {
        let _ = tx.send(Msg::Done { key: key.clone(), what: "Comment", result: c.add_comment(&key, &text) });
    });
}
```

In `handle` for `Msg::Done`: if `what == "Comment"` set `self.posting = false` and, on `Ok`, `self.composer.clear()`.

- [ ] **Step 4: Build, clippy, run**

Run: `ndc cargo clippy --all-targets -- -D warnings 2>&1 | tail -3 && JIRA_TICKETS_READONLY=1 ndc cargo run 2>&1 | tail -2`
Expected: selecting an issue shows its rendered description (bold, code, lists, links clickable), the fields column, comments with authors and dates. Typing a comment and ⌘Enter shows a "Comment failed: read-only mode…" toast (the guard works). Run once without `JIRA_TICKETS_READONLY` and post a real comment on a scratch ticket you own to confirm the success path and composer clearing.

- [ ] **Step 5: Commit**

```bash
git add src/views src/app.rs
git commit -m "feat: add issue detail with ADF rendering and comments"
```

---

### Task 11: Pickers and mutations (transition, assign, sprint, edit, rank)

**Files:**
- Create: `src/views/pickers.rs`
- Modify: `src/app.rs` (picker state, actions, Msg handling), `src/views/mod.rs` (add `pub mod pickers;` and call `pickers::show(app, ctx)` after `main`), `src/views/issues.rs` (rank keys)

**Interfaces:**
- Produces (`app.rs`):
  - `pub enum Picker { Transition { key: String, items: Vec<Transition>, cursor: usize }, Resolution { key: String, transition_id: String, items: Vec<Resolution>, cursor: usize }, Assign { key: String, items: Vec<User>, cursor: usize, filter: String }, Sprint { key: String, items: Vec<Sprint>, cursor: usize }, Edit { key: String, labels: String, priority: String, points: String } }`
  - `pub picker: Option<Picker>`, `pub picker_loading: bool`
  - `Action::{OpenTransition, OpenAssign, OpenSprint, OpenEdit, PickerMove(i32), PickerConfirm, PickerCancel, Rank(i32)}`
  - `Msg::Transitions/Resolutions/Users/Sprints` populate the matching picker; `Msg::Done { what: "Transition" }` with `jira::needs_resolution(&err)` opens the Resolution picker instead of toasting.
  - `Rank(delta)`: with rows = `current_rows`, the selected issue at index `i` and neighbour at `i + delta` (bounds checked), call `rank(key, before = neighbour if delta < 0, after = neighbour if delta > 0)`, optimistically swap the two keys in the active list (`store.lists[src]`) so the row moves immediately; on `Err`, refresh to snap back.
- Produces (`pickers.rs`): `pub fn show(app: &mut App, ctx: &egui::Context)` draws the open picker as a centred modal: title, optional filter box, items with the cursor highlighted, `↑`/`↓`/`j`/`k` move, `Enter` confirms, `Esc` cancels. The Edit picker shows three text fields (labels comma separated, priority name, story points) and Save/Cancel.

- [ ] **Step 1: Write a pure test for rank neighbour math**

In `app.rs` test module:

```rust
#[cfg(test)]
mod tests {
    use super::rank_target;

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
```

With, in `app.rs`:

```rust
/// Which issue to rank next to, and whether it goes before it.
pub fn rank_target(rows: &[String], key: &str, delta: i32) -> Option<(String, bool)> {
    let i = rows.iter().position(|k| k == key)?;
    let j = i as i32 + delta;
    if j < 0 || j as usize >= rows.len() {
        return None;
    }
    Some((rows[j as usize].clone(), delta < 0))
}
```

Run: `ndc cargo test rank_neighbours 2>&1 | tail -3` → `1 passed`.

- [ ] **Step 2: Add picker state, actions, and message handling to app.rs**

```rust
pub enum Picker {
    Transition { key: String, items: Vec<Transition>, cursor: usize },
    Resolution { key: String, transition_id: String, items: Vec<Resolution>, cursor: usize },
    Assign { key: String, items: Vec<User>, cursor: usize, filter: String },
    Sprint { key: String, items: Vec<Sprint>, cursor: usize },
    Edit { key: String, labels: String, priority: String, points: String },
}
// App fields: pub picker: Option<Picker>, pub picker_loading: bool
// Actions: OpenTransition, OpenAssign, OpenSprint, OpenEdit, PickerMove(i32), PickerConfirm, PickerCancel, Rank(i32)
```

`apply` additions:

```rust
Action::OpenTransition => {
    let Some(key) = self.selected.clone() else { return };
    self.picker_loading = true;
    self.spawn(move |c, tx| { let _ = tx.send(Msg::Transitions { key: key.clone(), result: c.transitions(&key) }); });
}
Action::OpenAssign => {
    if self.selected.is_none() { return; }
    self.picker_loading = true;
    let project = self.cfg.scope.project.clone();
    self.spawn(move |c, tx| { let _ = tx.send(Msg::Users(c.assignable_users(&project))); });
}
Action::OpenSprint => {
    if self.selected.is_none() { return; }
    self.picker_loading = true;
    let board = self.cfg.scope.board_id;
    self.spawn(move |c, tx| { let _ = tx.send(Msg::Sprints(c.board_sprints(board, "active,future"))); });
}
Action::OpenEdit => {
    let Some(i) = self.selected.as_ref().and_then(|k| self.store.get(k)) else { return };
    self.picker = Some(Picker::Edit { key: i.key.clone(), labels: i.labels.join(", "), priority: i.priority.clone(), points: i.story_points.map(|n| n.to_string()).unwrap_or_default() });
}
Action::PickerMove(d) => {
    if let Some(p) = &mut self.picker {
        let (cur, len) = match p {
            Picker::Transition { cursor, items, .. } => (cursor, items.len()),
            Picker::Resolution { cursor, items, .. } => (cursor, items.len()),
            Picker::Assign { cursor, items, .. } => (cursor, items.len() + 1), // + Unassigned
            Picker::Sprint { cursor, items, .. } => (cursor, items.len() + 1), // + Backlog
            Picker::Edit { .. } => return,
        };
        if len > 0 { *cur = (*cur as i32 + d).rem_euclid(len as i32) as usize; }
    }
}
Action::PickerCancel => self.picker = None,
Action::PickerConfirm => self.confirm_picker(),
Action::Rank(delta) => {
    let Some(key) = self.selected.clone() else { return };
    let src = if self.search_active { ListId::Search } else { ListId::TeamOpen };
    let rows: Vec<String> = views::issues::current_rows(self).iter().map(|i| i.key.clone()).collect();
    let Some((neighbour, before)) = rank_target(&rows, &key, delta) else { return };
    if let Some(list) = self.store.lists.get_mut(&src)
        && let (Some(a), Some(b)) = (list.iter().position(|k| *k == key), list.iter().position(|k| *k == neighbour))
    {
        list.swap(a, b);
        self.cursor = (self.cursor as i32 + delta).max(0) as usize;
    }
    self.spawn(move |c, tx| {
        let (b, a) = if before { (Some(neighbour.as_str()), None) } else { (None, Some(neighbour.as_str())) };
        let _ = tx.send(Msg::Done { key: key.clone(), what: "Rank", result: c.rank(&key, b, a) });
    });
}
```

`confirm_picker`:

```rust
fn confirm_picker(&mut self) {
    let Some(p) = self.picker.take() else { return };
    match p {
        Picker::Transition { key, items, cursor } => {
            let Some(t) = items.get(cursor).cloned() else { return };
            self.spawn(move |c, tx| { let _ = tx.send(Msg::Done { key: key.clone(), what: "Transition", result: c.transition(&key, &t.id, None) }); });
            // Remember the transition so a resolution error can retry it.
            self.pending_transition = Some(t.id);
        }
        Picker::Resolution { key, transition_id, items, cursor } => {
            let Some(r) = items.get(cursor).cloned() else { return };
            self.spawn(move |c, tx| { let _ = tx.send(Msg::Done { key: key.clone(), what: "Transition", result: c.transition(&key, &transition_id, Some(&r.id)) }); });
        }
        Picker::Assign { key, items, cursor, filter } => {
            let shown = filtered_users(&items, &filter);
            let account = if cursor == 0 { None } else { shown.get(cursor - 1).map(|u| u.account_id.clone()) };
            if cursor != 0 && account.is_none() { return; }
            self.spawn(move |c, tx| { let _ = tx.send(Msg::Done { key: key.clone(), what: "Assign", result: c.assign(&key, account.as_deref()) }); });
        }
        Picker::Sprint { key, items, cursor } => {
            let sprint = if cursor == 0 { None } else { items.get(cursor - 1).map(|s| s.id) };
            if cursor != 0 && sprint.is_none() { return; }
            self.spawn(move |c, tx| {
                let r = match sprint { Some(id) => c.move_to_sprint(id, &[&key]), None => c.move_to_backlog(&[&key]) };
                let _ = tx.send(Msg::Done { key: key.clone(), what: "Sprint move", result: r });
            });
        }
        Picker::Edit { key, labels, priority, points } => {
            let mut fields = serde_json::json!({ "labels": labels.split(',').map(|l| l.trim()).filter(|l| !l.is_empty()).collect::<Vec<_>>() });
            if !priority.trim().is_empty() { fields["priority"] = serde_json::json!({ "name": priority.trim() }); }
            if let Ok(n) = points.trim().parse::<f64>() { fields[jira::POINTS_FIELD] = serde_json::json!(n); }
            self.spawn(move |c, tx| { let _ = tx.send(Msg::Done { key: key.clone(), what: "Edit", result: c.update_fields(&key, fields) }); });
        }
    }
}

pub fn filtered_users(items: &[User], filter: &str) -> Vec<User> {
    let f = filter.trim().to_lowercase();
    items.iter().filter(|u| f.is_empty() || u.display_name.to_lowercase().contains(&f)).cloned().collect()
}
```

Add `pending_transition: Option<String>` to `App`. In `handle`:

```rust
Msg::Transitions { key, result } => {
    self.picker_loading = false;
    match result {
        Ok(items) => self.picker = Some(Picker::Transition { key, items, cursor: 0 }),
        Err(e) => self.toast(e.to_string(), true),
    }
}
Msg::Resolutions(result) => {
    self.picker_loading = false;
    match (result, self.selected.clone(), self.pending_transition.take()) {
        (Ok(items), Some(key), Some(transition_id)) => self.picker = Some(Picker::Resolution { key, transition_id, items, cursor: 0 }),
        (Err(e), _, _) => self.toast(e.to_string(), true),
        _ => {}
    }
}
Msg::Users(result) => {
    self.picker_loading = false;
    match (result, self.selected.clone()) {
        (Ok(items), Some(key)) => self.picker = Some(Picker::Assign { key, items, cursor: 0, filter: String::new() }),
        (Err(e), _) => self.toast(e.to_string(), true),
        _ => {}
    }
}
Msg::Sprints(result) => {
    self.picker_loading = false;
    match (result, self.selected.clone()) {
        (Ok(items), Some(key)) => self.picker = Some(Picker::Sprint { key, items, cursor: 0 }),
        (Err(e), _) => self.toast(e.to_string(), true),
        _ => {}
    }
}
```

And in `Msg::Done`, before the generic toast:

```rust
if what == "Transition" && let Err(e) = &result && jira::needs_resolution(e) && self.pending_transition.is_some() {
    self.picker_loading = true;
    self.spawn(|c, tx| { let _ = tx.send(Msg::Resolutions(c.resolutions())); });
    return;
}
if what == "Transition" { self.pending_transition = None; }
```

Keys in `shortcuts` (only when `!ctx.wants_keyboard_input()` and `self.picker.is_none()`): `t` → OpenTransition, `a` → OpenAssign, `s` → OpenSprint, `e` → OpenEdit, `Shift+J` → Rank(1), `Shift+K` → Rank(-1). When a picker is open: `↓`/`j` → PickerMove(1), `↑`/`k` → PickerMove(-1), `Enter` → PickerConfirm, `Esc` → PickerCancel (for Assign, `j`/`k` are typed into the filter, so only arrows move).

- [ ] **Step 3: Write pickers.rs**

```rust
//! Modal pickers for transitions, resolutions, assignees, sprints, and field edits.

use crate::app::{Action, App, Picker, filtered_users};
use crate::theme::{self, Palette};
use crate::views::{button, primary_button};
use egui::{Frame, Margin, RichText, Stroke, vec2};

pub fn show(app: &mut App, ctx: &egui::Context) {
    let p = theme::palette(ctx);
    if app.picker_loading {
        egui::Area::new(egui::Id::new("picker-loading")).anchor(egui::Align2::CENTER_CENTER, vec2(0.0, 0.0)).show(ctx, |ui| {
            ui.add(egui::Spinner::new().size(24.0));
        });
    }
    let Some(picker) = app.picker.as_mut() else { return };
    // Dim the page behind the dialog.
    egui::Area::new(egui::Id::new("picker-backdrop")).order(egui::Order::Middle).fixed_pos(egui::pos2(0.0, 0.0)).show(ctx, |ui| {
        let r = ui.ctx().content_rect();
        ui.painter().rect_filled(r, 0.0, egui::Color32::from_black_alpha(90));
        ui.allocate_rect(r, egui::Sense::click());
    });
    let mut confirm = false;
    let mut cancel = false;
    egui::Area::new(egui::Id::new("picker")).order(egui::Order::Foreground).anchor(egui::Align2::CENTER_CENTER, vec2(0.0, -40.0)).show(ctx, |ui| {
        Frame::new().fill(p.surface).stroke(Stroke::new(1.0, p.border)).corner_radius(8).inner_margin(Margin::same(14)).show(ui, |ui| {
            ui.set_width(440.0);
            match picker {
                Picker::Transition { key, items, cursor } => {
                    title(ui, p, &format!("Transition {key}"));
                    rows(ui, p, *cursor, items.iter().map(|t| format!("{}  →  {}", t.name, t.to)).collect(), &mut confirm, cursor);
                }
                Picker::Resolution { key, items, cursor, .. } => {
                    title(ui, p, &format!("Resolution for {key}"));
                    rows(ui, p, *cursor, items.iter().map(|r| r.name.clone()).collect(), &mut confirm, cursor);
                }
                Picker::Assign { key, items, cursor, filter } => {
                    title(ui, p, &format!("Assign {key}"));
                    let r = ui.add(egui::TextEdit::singleline(filter).hint_text("Type a name").desired_width(f32::INFINITY));
                    r.request_focus();
                    let mut names = vec!["Unassigned".to_string()];
                    names.extend(filtered_users(items, filter).iter().map(|u| u.display_name.clone()));
                    if *cursor >= names.len() { *cursor = 0; }
                    rows(ui, p, *cursor, names, &mut confirm, cursor);
                }
                Picker::Sprint { key, items, cursor } => {
                    title(ui, p, &format!("Move {key} to sprint"));
                    let mut names = vec!["Backlog".to_string()];
                    names.extend(items.iter().map(|s| format!("{} ({})", s.name, s.state)));
                    rows(ui, p, *cursor, names, &mut confirm, cursor);
                }
                Picker::Edit { key, labels, priority, points } => {
                    title(ui, p, &format!("Edit {key}"));
                    ui.label(RichText::new("Labels (comma separated)").color(p.muted).size(12.0));
                    ui.add(egui::TextEdit::singleline(labels).desired_width(f32::INFINITY));
                    ui.label(RichText::new("Priority").color(p.muted).size(12.0));
                    ui.add(egui::TextEdit::singleline(priority).desired_width(f32::INFINITY));
                    ui.label(RichText::new("Story points").color(p.muted).size(12.0));
                    ui.add(egui::TextEdit::singleline(points).desired_width(120.0));
                }
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.add(primary_button("Confirm", p)).clicked() { confirm = true; }
                if ui.add(button("Cancel", p)).clicked() { cancel = true; }
                ui.label(RichText::new("Enter · Esc").color(p.muted).size(11.0));
            });
        });
    });
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) { cancel = true; }
    if confirm { app.actions.push(Action::PickerConfirm); }
    if cancel { app.actions.push(Action::PickerCancel); }
}

fn title(ui: &mut egui::Ui, p: &Palette, text: &str) {
    ui.label(RichText::new(text).font(theme::bold(15.0)).color(p.text));
    ui.add_space(6.0);
}

fn rows(ui: &mut egui::Ui, p: &Palette, cur: usize, names: Vec<String>, confirm: &mut bool, cursor: &mut usize) {
    if names.is_empty() {
        ui.label(RichText::new("Nothing available").color(p.muted));
        return;
    }
    egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
        for (i, n) in names.iter().enumerate() {
            let fill = if i == cur { p.selected_row } else { p.surface };
            let r = Frame::new().fill(fill).corner_radius(4).inner_margin(Margin::symmetric(8, 6)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(n).size(13.5));
            }).response.interact(egui::Sense::click());
            if r.clicked() { *cursor = i; *confirm = true; }
            if r.hovered() { *cursor = i; }
        }
    });
}
```

- [ ] **Step 4: Build, clippy, verify by hand**

Run: `ndc cargo clippy --all-targets -- -D warnings 2>&1 | tail -3 && ndc cargo run 2>&1 | tail -2`
Expected on a scratch ticket you own: `t` lists transitions and performs one (status pill updates after refetch); transitioning to a Done status triggers the resolution picker; `a` filters users as you type and assigns; `s` moves to Backlog and back to the active sprint; `e` saves labels; `Shift+J` moves the row down immediately and stays after refresh; `Shift+J` on the last row does nothing. Start once with `JIRA_TICKETS_READONLY=1` and confirm each action toasts the read-only error and the rank swap snaps back on refresh.

- [ ] **Step 5: Commit**

```bash
git add src/views src/app.rs
git commit -m "feat: add transition, assign, sprint, edit pickers and rank keys"
```

---

### Task 12: Snapshot feature, macOS bundle, README

**Files:**
- Create: `src/snapshot.rs`, `scripts/bundle-macos.sh`, `README.md`
- Modify: `src/app.rs` (`pub fn shot_ready(&self) -> bool`)

**Interfaces:**
- Produces: `snapshot::run(out_path: &str, cfg: Config)`; honours `JIRA_TICKETS_VIEW=issues|grooming|team|roadmap`, `JIRA_TICKETS_THEME`, `JIRA_TICKETS_ROW=<n>` (select the nth row). Forces `READ_ONLY`. `App::shot_ready()` is true once `lists[TeamOpen].fetched` is Some.

- [ ] **Step 1: Write snapshot.rs**

```rust
//! Offscreen screenshots for UI review. Build with `--features snapshot`:
//!   JIRA_TICKETS_SCREENSHOT=out.png JIRA_TICKETS_VIEW=issues ./target/release/jira-tickets
//! Writes are always blocked in this mode.

use crate::app::{Action, App, Screen};
use crate::config::Config;
use egui_kittest::Harness;
use std::time::{Duration, Instant};

pub fn run(out: &str, cfg: Config) {
    crate::jira::READ_ONLY.store(true, std::sync::atomic::Ordering::Relaxed);
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1440.0, 900.0))
        .with_pixels_per_point(2.0)
        .with_theme(match std::env::var("JIRA_TICKETS_THEME").as_deref() {
            Ok("dark") => egui::Theme::Dark,
            _ => egui::Theme::Light,
        })
        .with_max_steps(4)
        .wgpu()
        .build_eframe(move |cc| App::new(cc, cfg));
    let start = Instant::now();
    while !harness.state().shot_ready() && start.elapsed() < Duration::from_secs(60) {
        harness.step();
        std::thread::sleep(Duration::from_millis(50));
    }
    let screen = match std::env::var("JIRA_TICKETS_VIEW").as_deref() {
        Ok("grooming") => Screen::Grooming,
        Ok("team") => Screen::Team,
        Ok("roadmap") => Screen::Roadmap,
        _ => Screen::Issues,
    };
    harness.state_mut().actions.push(Action::SetScreen(screen));
    if let Ok(n) = std::env::var("JIRA_TICKETS_ROW").map(|v| v.parse::<i32>().unwrap_or(0)) {
        harness.state_mut().actions.push(Action::Move(n));
    }
    for _ in 0..30 {
        harness.step();
        std::thread::sleep(Duration::from_millis(50));
    }
    let image = harness.render().expect("render");
    image.save(out).expect("save png");
    eprintln!("saved {out}");
}
```

Add to `App`:

```rust
pub fn shot_ready(&self) -> bool {
    self.lists.get(&crate::store::ListId::TeamOpen).is_some_and(|l| l.fetched.is_some())
}
```

Check the egui_kittest 0.36 API for `render` and `step` names under the nix shell's registry if the compiler disagrees; adjust only in this file.

- [ ] **Step 2: Build with the feature and take a screenshot**

Run: `ndc cargo build --release --features snapshot 2>&1 | tail -2 && JIRA_TICKETS_SCREENSHOT=/tmp/jt-issues.png ./target/release/jira-tickets && JIRA_TICKETS_SCREENSHOT=/tmp/jt-issues-dark.png JIRA_TICKETS_THEME=dark JIRA_TICKETS_ROW=2 ./target/release/jira-tickets`
Expected: two PNGs. Open them (`open /tmp/jt-issues.png`) and check: header with four tabs, list rows with pills, detail with rendered description, dark theme readable. Fix any contrast problems in `theme.rs` before continuing.

- [ ] **Step 3: Write scripts/bundle-macos.sh**

```sh
#!/bin/sh
# Builds "Jira Tickets.app" and installs it in /Applications.
# Usage: scripts/bundle-macos.sh [install-dir]   (default: /Applications)
set -eu
cd "$(dirname "$0")/.."
CARGO="${CARGO:-nix develop --command cargo}"
DEST="${1:-/Applications}"
NAME="Jira Tickets"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
APP="target/release/$NAME.app"

$CARGO build --release
BIN="target/release/jira-tickets"

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/jira-tickets"
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>$NAME</string>
  <key>CFBundleDisplayName</key><string>$NAME</string>
  <key>CFBundleIdentifier</key><string>dev.jira-tickets</string>
  <key>CFBundleExecutable</key><string>jira-tickets</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.developer-tools</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST
codesign --force --sign - "$APP"
rm -rf "$DEST/$NAME.app"
cp -R "$APP" "$DEST/"
echo "Installed $DEST/$NAME.app"
```

Run: `chmod +x scripts/bundle-macos.sh && scripts/bundle-macos.sh ~/Applications && open ~/Applications/"Jira Tickets.app"`
Expected: the app launches from the bundle. An app icon is out of scope for this plan.

- [ ] **Step 4: Write README.md**

Cover, in this order and in plain prose: what the app is (one paragraph), install from source (`nix develop --command cargo run --release`, `scripts/bundle-macos.sh`), the config file (where it is, the generated example, how to find custom field ids at `<site>/rest/api/3/field`), signing in (the four-step token order and where the token file lives), keyboard table (`⌘1–4`, `⌘R`, `⌘K`, `⌘F` or `/`, `j`/`k`, `Enter`, `t`, `a`, `s`, `e`, `J`/`K`, `c`, `⌘O`, `⌘W`), developer switches (`JIRA_TICKETS_READONLY`, `JIRA_TICKETS_PERF`, `JIRA_TICKETS_CONFIG`, `JIRA_TICKETS_SCREENSHOT` and friends), and a code map table matching the file structure above. Note that Grooming, Team, and Roadmap are placeholders until plans 2 and 3.

- [ ] **Step 5: Final checks and commit**

Run: `ndc cargo fmt --check && ndc cargo clippy --all-targets --features snapshot -- -D warnings 2>&1 | tail -3 && ndc cargo test 2>&1 | tail -3`
Expected: formatted, no warnings, all tests pass.

```bash
git add src/snapshot.rs src/app.rs scripts/bundle-macos.sh README.md
git commit -m "feat: add offscreen snapshots, macOS bundle script, and README"
```

---

## Hand-off to plan 2

Plan 2 (Grooming) consumes from this plan: `Store::by_status`, `Store::by_parent`, `Client::{transition, rank, add_comment, search}`, `adf::to_plain_text`, `App::spawn`, `Msg::Done`, `Picker` conventions, `views::{button, pill, health_dot}`, and `config.grooming`. Plan 3 (Team, Roadmap) consumes `Store::by_assignee`, `Store::children_of`, `ListId::{Epics, TeamInProgress}`, `Issue::{status_changed, due_date, labels, custom, comments}`, `config.roadmap`, and the palette's health colours.
