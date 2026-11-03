//! Offscreen screenshots for UI review. Build with `--features snapshot`:
//!   JIRA_TICKETS_SCREENSHOT=out.png JIRA_TICKETS_VIEW=issues ./target/release/jira-tickets
//!   JIRA_TICKETS_SCRIPT='key:j;wait:1500;shot:detail.png' ./target/release/jira-tickets
//! Script steps (`;`-separated): `key:<combo>`, `type:<text>`, `click:<label>`, `point:<label>` (hover a
//! widget, exact label first; `point:<label>@left` hovers a quarter in, inside its left third, so the pointer hides little of the label), `at:x,y`, `hover:x,y`, `wait:<ms>`, `shot:<path>`, `theme:light|dark`, `width:<px>`,
//! `scroll:<dy>` (a wheel scroll at the pointer: positive goes up, so `hover:700,400;scroll:5000` reaches the top).
//! Writes are always blocked in this mode.
//! `JIRA_TICKETS_SIZE=1200x800` sets the window size in points (default 1440x900).
//! Standup fixtures, so a capture never waits on a real review: `JIRA_TICKETS_STANDUP=<file>` loads a saved review
//! (`standup::Saved` JSON, as in the cache) and never starts one; `JIRA_TICKETS_STANDUP_STATE=jira|claude|error` forces
//! a running phase (its clock pinned at 1:24) or a failed run.

use crate::app::{Action, App, Screen, StandupPhase};
use crate::config::Config;
use egui::{Key, Modifiers, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use std::time::{Duration, Instant};

const SIZE: egui::Vec2 = egui::vec2(1440.0, 900.0);
const FRAME: Duration = Duration::from_millis(50);
/// `1200x800` as a size in points.
pub fn parse_size(s: &str) -> Option<egui::Vec2> {
    let (w, h) = s.trim().split_once('x')?;
    let (w, h): (f32, f32) = (w.trim().parse().ok()?, h.trim().parse().ok()?);
    (w > 0.0 && h > 0.0).then(|| egui::vec2(w, h))
}

/// A forced running review's clock: "1:24".
const PINNED_ELAPSED: Duration = Duration::from_secs(84);
const FORCED_ERROR: &str = "claude took longer than 900 s and was stopped";

/// A forced Standup state from `JIRA_TICKETS_STANDUP_STATE`: a running phase, or None with an error.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Forced {
    Running(StandupPhase),
    Error,
}

pub fn parse_forced(s: &str) -> Option<Forced> {
    match s.trim() {
        "jira" => Some(Forced::Running(StandupPhase::Jira)),
        "claude" | "running" => Some(Forced::Running(StandupPhase::Claude)),
        "error" => Some(Forced::Error),
        _ => None,
    }
}

fn forced() -> Option<Forced> {
    std::env::var("JIRA_TICKETS_STANDUP_STATE").ok().as_deref().and_then(parse_forced)
}

fn no_review(_: &crate::config::Standup, _: &str) -> Result<crate::standup::Review, String> {
    Err("snapshot fixtures never run a review".into())
}

/// Loads `JIRA_TICKETS_STANDUP`'s saved review and applies a forced state. True when a fixture is loaded.
fn standup_fixture(app: &mut App) -> bool {
    let Ok(path) = std::env::var("JIRA_TICKETS_STANDUP") else { return false };
    let saved = std::fs::read(&path).map_err(|e| e.to_string()).and_then(|b| serde_json::from_slice(&b).map_err(|e| e.to_string()));
    match saved {
        Ok(saved) => app.standup.saved = saved,
        Err(e) => eprintln!("snapshot: could not load {path}: {e}"),
    }
    // Today counts as run, and no run can start, so the fixture stays on screen.
    app.standup.saved.last_run = Some(chrono::Local::now().date_naive());
    app.review_runner = no_review;
    match forced() {
        Some(Forced::Running(phase)) => {
            app.standup.phase = Some(phase);
            app.standup.activity_count = (phase == StandupPhase::Claude).then_some(12);
            pin_clock(app);
        }
        Some(Forced::Error) => app.standup.error = Some(FORCED_ERROR.into()),
        None => {}
    }
    true
}

/// A forced running review reads the same elapsed time in every shot. True when it pinned the clock.
fn pin_clock(app: &mut App) -> bool {
    let pin = app.standup.phase.is_some() && matches!(forced(), Some(Forced::Running(_)));
    if pin {
        app.standup.started = Instant::now().checked_sub(PINNED_ELAPSED);
    }
    pin
}

#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    Key(Modifiers, Key),
    Type(String),
    Click(String),
    Point(String),
    At(Pos2),
    Hover(Pos2),
    Wait(u64),
    Shot(String),
    Theme(egui::Theme),
    Width(f32),
    Scroll(f32),
    Unknown(String),
}

/// Parses `key:cmd+k`, `shift+j`, `escape`, `question` (the `?` key), or a single letter/digit.
fn parse_key(combo: &str) -> Option<(Modifiers, Key)> {
    let mut mods = Modifiers::NONE;
    let mut parts: Vec<&str> = combo.split('+').map(str::trim).collect();
    let name = parts.pop().filter(|n| !n.is_empty())?;
    for m in parts {
        match m.to_ascii_lowercase().as_str() {
            "cmd" | "command" => mods |= Modifiers::COMMAND,
            "shift" => mods |= Modifiers::SHIFT,
            "alt" | "option" => mods |= Modifiers::ALT,
            "ctrl" => mods |= Modifiers::CTRL,
            _ => return None,
        }
    }
    let key = match name.to_ascii_lowercase().as_str() {
        "question" => Key::Questionmark,
        "esc" | "escape" => Key::Escape,
        "enter" | "return" => Key::Enter,
        "up" => Key::ArrowUp,
        "down" => Key::ArrowDown,
        "left" => Key::ArrowLeft,
        "right" => Key::ArrowRight,
        "tab" => Key::Tab,
        "space" => Key::Space,
        "backspace" => Key::Backspace,
        _ => Key::from_name(name)?,
    };
    Some((mods, key))
}

fn parse_point(s: &str) -> Option<Pos2> {
    let (x, y) = s.split_once(',')?;
    Some(Pos2::new(x.trim().parse().ok()?, y.trim().parse().ok()?))
}

fn parse_step(step: &str) -> Step {
    let unknown = || Step::Unknown(step.to_owned());
    let Some((kind, arg)) = step.split_once(':') else {
        return unknown();
    };
    let parsed = match kind.trim() {
        "key" => parse_key(arg.trim()).map(|(m, k)| Step::Key(m, k)),
        "type" => Some(Step::Type(arg.to_owned())),
        "click" => Some(arg.trim()).filter(|a| !a.is_empty()).map(|a| Step::Click(a.to_owned())),
        "point" => Some(arg.trim()).filter(|a| !a.is_empty()).map(|a| Step::Point(a.to_owned())),
        "at" => parse_point(arg).map(Step::At),
        "hover" => parse_point(arg).map(Step::Hover),
        "wait" => arg.trim().parse().ok().map(Step::Wait),
        "shot" => Some(arg.trim()).filter(|a| !a.is_empty()).map(|a| Step::Shot(a.to_owned())),
        "theme" => match arg.trim() {
            "light" => Some(Step::Theme(egui::Theme::Light)),
            "dark" => Some(Step::Theme(egui::Theme::Dark)),
            _ => None,
        },
        "width" => arg.trim().parse().ok().filter(|w: &f32| *w > 0.0).map(Step::Width),
        "scroll" => arg.trim().parse().ok().map(Step::Scroll),
        _ => None,
    };
    parsed.unwrap_or_else(unknown)
}

/// Splits a `;`-separated script into steps; blank entries are skipped.
pub fn parse_steps(script: &str) -> Vec<Step> {
    script.split(';').filter(|s| !s.trim().is_empty()).map(|s| parse_step(s.trim_start())).collect()
}

/// Steps frames in real time; background threads deliver data meanwhile.
fn frames(harness: &mut Harness<'_, App>, n: u64) {
    for _ in 0..n {
        harness.step();
        std::thread::sleep(FRAME);
    }
}

/// Before a shot, lets a background refresh that is still on the wire land (up to IDLE_WAIT), so the header shows
/// the idle sync glyph unless the shot is a loading state (JIRA_TICKETS_NO_WAIT) or a fetch really hangs.
fn settle_header(harness: &mut Harness<'_, App>) {
    if std::env::var_os("JIRA_TICKETS_NO_WAIT").is_some() {
        return;
    }
    let start = Instant::now();
    while crate::views::header_syncing(&harness.state().lists) && start.elapsed() < IDLE_WAIT {
        frames(harness, 1);
    }
    if crate::views::header_syncing(&harness.state().lists) {
        eprintln!("snapshot: a list is still loading after {:.0}s; the header shows the spinner", IDLE_WAIT.as_secs_f32());
    }
}

/// The longest `settle_header` waits for a background refresh.
const IDLE_WAIT: Duration = Duration::from_secs(10);

fn save(harness: &mut Harness<'_, App>, out: &str) {
    settle_header(harness);
    if pin_clock(harness.state_mut()) {
        harness.step();
    }
    match harness.render() {
        Ok(image) => match image.save(out) {
            Ok(()) => eprintln!("saved {out}"),
            Err(e) => eprintln!("snapshot: could not save {out}: {e}"),
        },
        Err(e) => eprintln!("snapshot: render failed for {out}: {e}"),
    }
}

/// A node's accessible label, value, and placeholder. egui puts a ComboBox's selected text in its
/// value and a TextEdit's hint text in its placeholder.
fn texts(n: &egui_kittest::kittest::AccessKitNode<'_>) -> [Option<String>; 3] {
    [n.label(), n.value(), n.placeholder().map(str::to_owned)]
}

/// `click:` matching ignores case, so a flow names a chip the same way under any config's capitalisation.
fn label_matches(text: &str, label: &str) -> bool {
    text.to_lowercase().contains(&label.to_lowercase())
}

fn click(harness: &mut Harness<'_, App>, label: &str) {
    // Prefer widgets labelled by their text (buttons), then widgets whose value or placeholder
    // matches (ComboBox selected text, TextEdit hint), then plain text, so `click:Roadmap` hits the toggle, not the nav tab.
    let plain = |n: &egui_kittest::kittest::AccessKitNode<'_>| matches!(n.role(), egui::accesskit::Role::Label | egui::accesskit::Role::TextRun);
    let rect = harness
        .query_all_by(|n| n.label().is_some_and(|t| label_matches(&t, label)))
        .next()
        .or_else(|| harness.query_all_by(|n| !plain(n) && texts(n)[1..].iter().flatten().any(|t| label_matches(t, label))).next())
        .or_else(|| harness.query_all_by(|n| texts(n).iter().flatten().any(|t| label_matches(t, label))).next())
        .map(|n| n.rect());
    match rect {
        Some(r) => {
            let pos = r.center();
            harness.hover_at(pos);
            for pressed in [true, false] {
                harness.event(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
            }
        }
        None => {
            let seen: Vec<String> = harness
                .query_all_by(|n| !plain(n))
                .map(|n| {
                    let node = n.accesskit_node();
                    format!("{:?} {:?}", node.role(), texts(&node))
                })
                .collect();
            eprintln!("snapshot: no widget labelled {label:?}; on screen: {seen:?}");
        }
    }
}

/// A `point:` argument as (label, left third): "Comment@left" → ("Comment", true).
pub fn point_target(arg: &str) -> (&str, bool) {
    match arg.strip_suffix("@left") {
        Some(l) => (l.trim_end(), true),
        None => (arg, false),
    }
}

/// Where a `point:` lands in a widget's rect: its centre, or a quarter in, inside its left third (round 14 F:
/// the third's middle put the pointer over the label's first letter).
pub fn point_pos(r: egui::Rect, left: bool) -> Pos2 {
    if left { Pos2::new(r.left() + r.width() / 4.0, r.center().y) } else { r.center() }
}

/// Hovers the widget whose label is exactly `label` (ignoring case), else the first `click:` would pick.
fn point(harness: &mut Harness<'_, App>, arg: &str) {
    let (label, left) = point_target(arg);
    let exact = harness.query_all_by(|n| n.label().is_some_and(|t| t.eq_ignore_ascii_case(label))).next().map(|n| n.rect());
    let rect = exact.or_else(|| harness.query_all_by(|n| texts(n).iter().flatten().any(|t| label_matches(t, label))).next().map(|n| n.rect()));
    match rect {
        Some(r) => harness.hover_at(point_pos(r, left)),
        None => eprintln!("snapshot: nothing labelled {label:?} to point at"),
    }
}

fn run_step(harness: &mut Harness<'_, App>, step: &Step) {
    match step {
        Step::Key(m, k) => {
            if *m == Modifiers::NONE {
                harness.key_press(*k);
            } else {
                harness.key_press_modifiers(*m, *k);
            }
        }
        Step::Type(t) => harness.event(egui::Event::Text(t.clone())),
        Step::Click(label) => click(harness, label),
        Step::Point(label) => point(harness, label),
        Step::At(pos) => {
            harness.hover_at(*pos);
            for pressed in [true, false] {
                harness.event(egui::Event::PointerButton { pos: *pos, button: egui::PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
            }
        }
        Step::Hover(pos) => harness.hover_at(*pos),
        Step::Wait(ms) => return frames(harness, ms.div_ceil(FRAME.as_millis() as u64)),
        Step::Shot(out) => return save(harness, out),
        Step::Theme(t) => harness.ctx.set_theme(*t),
        Step::Width(w) => {
            let h = harness.ctx.content_rect().height();
            harness.set_size(egui::vec2(*w, h));
        }
        Step::Scroll(dy) => harness.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, *dy),
            phase: egui::TouchPhase::Move,
            modifiers: Modifiers::NONE,
        }),
        Step::Unknown(s) => return eprintln!("snapshot: skipping unknown step {s:?}"),
    }
    // Drain queued events (one frame each), then let actions apply and layout settle.
    harness.step();
    for _ in 0..3 {
        harness.step();
    }
}

/// The Standup shot waits for a whole review: with no saved review and none started by itself, it starts one,
/// then waits for it to end, up to `[standup].timeout_secs` plus a minute for Jira.
fn standup_wait(harness: &mut Harness<'_, App>) {
    frames(harness, 2);
    if harness.state().standup.phase.is_none() && harness.state().standup.saved.review.is_none() {
        harness.state_mut().actions.push(Action::StandupRun);
        frames(harness, 2);
    }
    let limit = Duration::from_secs(harness.state().cfg.standup.timeout_secs + 60);
    let start = Instant::now();
    while harness.state().standup.phase.is_some() && start.elapsed() < limit {
        frames(harness, 10);
    }
    let st = &harness.state().standup;
    eprintln!(
        "snapshot: standup {} after {:.0}s, {} activity issues, {} suggestions{}",
        if st.phase.is_some() { "still running" } else { "done" },
        start.elapsed().as_secs_f32(),
        st.activity_count.unwrap_or(0),
        st.saved.visible().len(),
        st.error.as_deref().map(|e| format!(", error: {e}")).unwrap_or_default()
    );
}

pub fn run(out: Option<&str>, cfg: Config) {
    crate::jira::READ_ONLY.store(true, std::sync::atomic::Ordering::Relaxed);
    let mut harness = Harness::builder()
        .with_size(std::env::var("JIRA_TICKETS_SIZE").ok().as_deref().and_then(parse_size).unwrap_or(SIZE))
        .with_pixels_per_point(2.0)
        .with_theme(match std::env::var("JIRA_TICKETS_THEME").as_deref() {
            Ok("dark") => egui::Theme::Dark,
            _ => egui::Theme::Light,
        })
        .with_max_steps(4)
        .wgpu()
        .build_eframe(move |cc| App::new(cc, cfg));
    let fixture = standup_fixture(harness.state_mut());
    // JIRA_TICKETS_NO_WAIT: start the script at once (with an empty cache, the first-load states show).
    let no_wait = std::env::var_os("JIRA_TICKETS_NO_WAIT").is_some();
    let screen = match std::env::var("JIRA_TICKETS_VIEW").as_deref() {
        Ok("grooming") => Screen::Grooming,
        Ok("team") => Screen::Team,
        Ok("roadmap") => Screen::Roadmap,
        Ok("me") => Screen::Me,
        Ok("standup") => Screen::Standup,
        _ => Screen::Issues,
    };
    let start = Instant::now();
    while !no_wait && !harness.state().shot_ready(screen) && start.elapsed() < Duration::from_secs(20) {
        frames(&mut harness, 1);
    }
    eprintln!("snapshot: data {} after {:.1}s", if harness.state().shot_ready(screen) { "ready" } else { "not ready" }, start.elapsed().as_secs_f32());
    harness.state_mut().actions.push(Action::SetScreen(screen));
    if screen == Screen::Standup && !fixture {
        standup_wait(&mut harness);
    }
    if let Ok(n) = std::env::var("JIRA_TICKETS_ROW").map(|v| v.parse::<i32>().unwrap_or(0)) {
        harness.state_mut().actions.push(Action::Move(n));
    }
    frames(&mut harness, if no_wait { 1 } else { 30 });
    let script = std::env::var("JIRA_TICKETS_SCRIPT").unwrap_or_default();
    for step in parse_steps(&script) {
        eprintln!("snapshot: {step:?}");
        run_step(&mut harness, &step);
    }
    if let Some(out) = out {
        save(&mut harness, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_script_has_no_steps() {
        assert!(parse_steps("").is_empty());
        assert!(parse_steps(" ; ;").is_empty());
    }

    #[test]
    fn point_can_aim_at_the_left_third() {
        assert_eq!(point_target("Comment@left"), ("Comment", true));
        assert_eq!(point_target("Comment"), ("Comment", false));
        let r = egui::Rect::from_min_size(Pos2::new(100.0, 10.0), egui::vec2(90.0, 28.0));
        assert_eq!(point_pos(r, true), Pos2::new(122.5, 24.0));
        assert!(point_pos(r, true).x < r.left() + r.width() / 3.0, "still in the left third");
        assert_eq!(point_pos(r, false), Pos2::new(145.0, 24.0));
    }

    #[test]
    fn parses_each_kind() {
        let steps = parse_steps(
            "key:cmd+k;type:status = Blocked;click:All teams;point:Comment;at:10,20;hover:1.5, 2;wait:200;shot:a.png;theme:dark;width:1000;scroll:-300",
        );
        assert_eq!(
            steps,
            vec![
                Step::Key(Modifiers::COMMAND, Key::K),
                Step::Type("status = Blocked".into()),
                Step::Click("All teams".into()),
                Step::Point("Comment".into()),
                Step::At(Pos2::new(10.0, 20.0)),
                Step::Hover(Pos2::new(1.5, 2.0)),
                Step::Wait(200),
                Step::Shot("a.png".into()),
                Step::Theme(egui::Theme::Dark),
                Step::Width(1000.0),
                Step::Scroll(-300.0),
            ]
        );
    }

    #[test]
    fn parses_key_names() {
        let keys: Vec<Step> = parse_steps("key:shift+j;key:escape;key:enter;key:up;key:down;key:tab;key:question;key:j;key:3;key:cmd+enter");
        assert_eq!(
            keys,
            vec![
                Step::Key(Modifiers::SHIFT, Key::J),
                Step::Key(Modifiers::NONE, Key::Escape),
                Step::Key(Modifiers::NONE, Key::Enter),
                Step::Key(Modifiers::NONE, Key::ArrowUp),
                Step::Key(Modifiers::NONE, Key::ArrowDown),
                Step::Key(Modifiers::NONE, Key::Tab),
                Step::Key(Modifiers::NONE, Key::Questionmark),
                Step::Key(Modifiers::NONE, Key::J),
                Step::Key(Modifiers::NONE, Key::Num3),
                Step::Key(Modifiers::COMMAND, Key::Enter),
            ]
        );
    }

    #[test]
    fn bad_steps_are_unknown() {
        for bad in ["nope", "point:", "key:hyper+j", "key:", "wait:soon", "at:1", "theme:blue", "width:-3", "scroll:up", "click:", "frobnicate:1"] {
            assert_eq!(parse_steps(bad), vec![Step::Unknown(bad.into())], "{bad}");
        }
    }

    #[test]
    fn click_labels_ignore_case() {
        assert!(label_matches("Only Opened", "Only opened"));
        assert!(label_matches("✓ Only opened", "only Opened"));
        assert!(!label_matches("To Do", "Only"));
    }

    #[test]
    fn sizes() {
        assert_eq!(parse_size("1200x800"), Some(egui::vec2(1200.0, 800.0)));
        for bad in ["1200", "0x800", "ax800", ""] {
            assert_eq!(parse_size(bad), None, "{bad}");
        }
    }

    #[test]
    fn forced_standup_states() {
        assert_eq!(parse_forced("jira"), Some(Forced::Running(StandupPhase::Jira)));
        assert_eq!(parse_forced("claude"), Some(Forced::Running(StandupPhase::Claude)));
        assert_eq!(parse_forced(" error "), Some(Forced::Error));
        assert_eq!(parse_forced("loaded"), None);
    }

    #[test]
    fn type_keeps_spaces() {
        assert_eq!(parse_steps("type: alpha "), vec![Step::Type(" alpha ".into())]);
    }
}
