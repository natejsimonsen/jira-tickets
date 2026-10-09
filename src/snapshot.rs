//! Offscreen screenshots for UI review. Build with `--features snapshot`:
//!   JIRA_TICKETS_SCREENSHOT=out.png JIRA_TICKETS_VIEW=issues ./target/release/jira-tickets
//!   JIRA_TICKETS_SCRIPT='key:j;wait:1500;shot:detail.png' ./target/release/jira-tickets
//! Script steps (`;`-separated): `key:<combo>`, `type:<text>`, `click:<label>`, `point:<label>` (hover a
//! widget, exact label first), `at:x,y`, `hover:x,y`, `wait:<ms>`, `shot:<path>`, `theme:light|dark`, `width:<px>`.
//! Writes are always blocked in this mode.

use crate::app::{Action, App, Screen};
use crate::config::Config;
use egui::{Key, Modifiers, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use std::time::{Duration, Instant};

const SIZE: egui::Vec2 = egui::vec2(1440.0, 900.0);
const FRAME: Duration = Duration::from_millis(50);

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

fn save(harness: &mut Harness<'_, App>, out: &str) {
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

/// Hovers the widget whose label is exactly `label` (ignoring case), else the first `click:` would pick.
fn point(harness: &mut Harness<'_, App>, label: &str) {
    let exact = harness.query_all_by(|n| n.label().is_some_and(|t| t.eq_ignore_ascii_case(label))).next().map(|n| n.rect());
    let rect = exact.or_else(|| harness.query_all_by(|n| texts(n).iter().flatten().any(|t| label_matches(t, label))).next().map(|n| n.rect()));
    match rect {
        Some(r) => harness.hover_at(r.center()),
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
        Step::Unknown(s) => return eprintln!("snapshot: skipping unknown step {s:?}"),
    }
    // Drain queued events (one frame each), then let actions apply and layout settle.
    harness.step();
    for _ in 0..3 {
        harness.step();
    }
}

pub fn run(out: Option<&str>, cfg: Config) {
    crate::jira::READ_ONLY.store(true, std::sync::atomic::Ordering::Relaxed);
    let mut harness = Harness::builder()
        .with_size(SIZE)
        .with_pixels_per_point(2.0)
        .with_theme(match std::env::var("JIRA_TICKETS_THEME").as_deref() {
            Ok("dark") => egui::Theme::Dark,
            _ => egui::Theme::Light,
        })
        .with_max_steps(4)
        .wgpu()
        .build_eframe(move |cc| App::new(cc, cfg));
    // JIRA_TICKETS_NO_WAIT: start the script at once (with an empty cache, the first-load states show).
    let no_wait = std::env::var_os("JIRA_TICKETS_NO_WAIT").is_some();
    let start = Instant::now();
    while !no_wait && !harness.state().shot_ready() && start.elapsed() < Duration::from_secs(20) {
        frames(&mut harness, 1);
    }
    eprintln!("snapshot: data {} after {:.1}s", if harness.state().shot_ready() { "ready" } else { "not ready" }, start.elapsed().as_secs_f32());
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
    fn parses_each_kind() {
        let steps =
            parse_steps("key:cmd+k;type:status = Blocked;click:All teams;point:Comment;at:10,20;hover:1.5, 2;wait:200;shot:a.png;theme:dark;width:1000");
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
        for bad in ["nope", "point:", "key:hyper+j", "key:", "wait:soon", "at:1", "theme:blue", "width:-3", "click:", "frobnicate:1"] {
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
    fn type_keeps_spaces() {
        assert_eq!(parse_steps("type: alpha "), vec![Step::Type(" alpha ".into())]);
    }
}
