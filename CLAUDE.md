# jira-tickets

Rust 2024 / eframe-egui 0.36 desktop Jira client for the Platform (PLAT) sub-team of the parent team.
Spec (binding): `docs/superpowers/specs/2026-10-07-jira-tickets-design.md`. Plans: `docs/superpowers/plans/`.
Plan 1 (foundation + Issues view) is done; plans 2 (Grooming) and 3 (Team, Roadmap) are written and waiting.

## Build and test

- There is no system cargo. Run everything through the nix dev shell:
  `nix develop --command cargo build|test|fmt|clippy ...` (first `nix develop` takes a few minutes).
- Before every commit: `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, the same with
  `--features snapshot`, and `cargo test`. All must be clean.
- `scripts/bundle-macos.sh ~/Applications` builds and installs `Jira Tickets.app`; `open ~/Applications/"Jira Tickets.app"`.
- Hands-off launch check (agents cannot see the window):
  `JIRA_TICKETS_CONFIG=/tmp/jt-config.toml JIRA_TICKETS_TOKEN_FILE=/tmp/jt-token JIRA_TICKETS_READONLY=1 cargo run` for ~15 s, no panic.
- Offscreen screenshots: build with `--features snapshot`, run with `JIRA_TICKETS_SCREENSHOT=out.png` (needs a token).
- egui API drift: check `~/.cargo/registry/src/*/egui-0.36.2/` inside the nix shell before guessing names.
  eframe 0.36's trait is `fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame)`; there is no `update`.

## Architecture invariants (every review enforced these)

- All network work runs on background threads via `App::spawn` (reads) or `App::spawn_write` (writes, one ordered
  worker thread). Results arrive as `Msg` over mpsc; views push `Action`s that `apply` runs after the frame.
- `App::spawn` silently no-ops without a client, so every action that spawns checks `client.is_some()` first,
  and loading flags are set only after `should_load(client_present, already_loading)` passes.
- Every Jira write calls `jira::check_writable()` first; `JIRA_TICKETS_READONLY=1` blocks all writes.
- Search results carry a sequence number (`accept_search`); stale results are dropped.
- Tokens from the config `api_token` reference (`op://` or `$(cmd)`) are resolved on every launch and never written
  to disk. Only a pasted token is saved. Do not change this.
- Tests that set env vars lock `crate::testutil::ENV_LOCK`; tests that toggle `READ_ONLY` lock
  `jira::tests::WRITE_LOCK` and use `ReadOnlyGuard`. Use `jira::tests::stub_server` for client tests, fixtures in `tests/fixtures/`.
- Keyboard bindings live in `src/keys.rs::BINDINGS`; the README table between `<!-- keys:start -->` and
  `<!-- keys:end -->` is generated from it and a test fails if they differ. Add new keys there first.
- Pure helpers get unit tests; UI code gets a hands-off launch check. Prefer factoring decisions into pure functions.

## Conventions

- Conventional commits, no co-author or attribution trailers. If `git commit` fails on 1Password signing, stop and
  report BLOCKED; never change signing config.
- 4-space indentation, rustfmt `max_width = 160`. `#![allow(dead_code)]` in main.rs is temporary until plan 3 lands.
- Config lives at `~/.config/jira-tickets/config.toml`; the example is `config::EXAMPLE` (an `r##"…"##` string).

## Known follow-ups

- Split `app.rs` (~1300 lines) into submodules and add a windowless `App` test constructor: plan 2, task 1.
- Deferred minor findings from plan 1's reviews are listed in the plan files' "Hand-off" sections.
