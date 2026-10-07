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
    while !harness.state().shot_ready() && start.elapsed() < Duration::from_secs(20) {
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
