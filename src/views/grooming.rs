//! The Grooming screen: a header with the mode switch, the report line and the Ready for Work ceiling, then the
//! Pre-check or Session body.

use crate::app::{Action, App, GroomMode};
use crate::grooming::{self, Level};
use crate::report::{self, Badge};
use crate::theme::{self, Palette};
use crate::views::{PILL_MARGIN, empty_state, outline_pill, pill_sized, toggle_chip};
use egui::{Color32, Frame, Margin, RichText, Stroke, Ui};

/// The gap between the header's mode chips, as between the Issues quick-filter chips.
const CHIP_GAP: f32 = 8.0;
/// The gap between the header's groups (the switch, the report line), and under the header.
const GROUP_GAP: f32 = 16.0;

pub fn show(app: &mut App, ui: &mut Ui) {
    let p = theme::palette(ui.ctx());
    egui::CentralPanel::default()
        .frame(Frame::new().fill(p.bg).inner_margin(Margin { left: 16, right: 16, top: crate::views::issues::PANE_TOP as i8, bottom: 0 }))
        .show(ui, |ui| {
            header(app, ui, p);
            ui.add_space(GROUP_GAP);
            match app.grooming.mode {
                GroomMode::PreCheck => pre_check_body(app, ui, p),
                GroomMode::Session => session_body(app, ui, p),
            }
        });
}

/// The mode switch, the report line, and the ceiling pill at the right.
fn header(app: &mut App, ui: &mut Ui, p: &Palette) {
    let mode = app.grooming.mode;
    let line = report_line(app.grooming.report.as_ref().map(|(path, r)| (path.as_path(), r)), &app.cfg.grooming.report_dir);
    let g = &app.cfg.grooming;
    let ready = app.ready_total();
    let level = grooming::ceiling_level(ready, g.ready_soft_ceiling, g.ready_hard_ceiling);
    let ceiling = ceiling_text(ready, g.ready_soft_ceiling, g.ready_hard_ceiling);
    ui.horizontal(|ui| {
        ui.set_min_height(crate::views::BUTTON_H);
        ui.spacing_mut().item_spacing.x = CHIP_GAP;
        for (m, name) in [(GroomMode::PreCheck, "Pre-check"), (GroomMode::Session, "Session")] {
            if crate::views::tooltip(toggle_chip(ui, name, mode == m, p), "Tab").clicked() && mode != m {
                app.actions.push(Action::GroomMode(m));
            }
        }
        ui.add_space(GROUP_GAP - CHIP_GAP);
        ui.label(RichText::new(line).color(p.muted).size(12.0));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let c = level_color(level, p);
            pill_sized(ui, &ceiling, Color32::TRANSPARENT, c, Stroke::new(1.0, c), PILL_MARGIN);
        });
    });
}

/// Task 6 fills this in: the `New` walk with verdict keys and the summary.
fn pre_check_body(app: &mut App, ui: &mut Ui, p: &Palette) {
    let rows = app.pre_check_candidates();
    let judged = rows.iter().filter(|c| app.grooming.verdicts.contains_key(&c.issue.key)).count();
    empty_state(ui, p, &format!("Pre-check: {} in New, {judged} with a verdict", rows.len()));
}

/// Task 7 fills this in: the epic rail, the Ready queue and its Planning candidates.
fn session_body(app: &mut App, ui: &mut Ui, p: &Palette) {
    let epics = app.session_queues().iter().filter(|q| !q.planning.is_empty()).count();
    empty_state(ui, p, &format!("Session: {epics} {} with Planning tickets", if epics == 1 { "epic" } else { "epics" }));
}

/// "Report 2026-09-04 · 11 promoted · 14 need detail", or "No report found in <dir>" naming the configured dir.
pub fn report_line(report: Option<(&std::path::Path, &report::Report)>, dir: &str) -> String {
    match report {
        Some((path, r)) => format!("Report {} · {} promoted · {} need detail", report_date(path, r), r.promoted.len(), r.needs_detail.len()),
        None => format!("No report found in {dir}"),
    }
}

/// The report's own date, else its file name's stem.
pub fn report_date(path: &std::path::Path, r: &report::Report) -> String {
    if r.date.is_empty() { path.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string() } else { r.date.clone() }
}

/// "Ready for Work 9 · soft 8 · hard 12".
pub fn ceiling_text(ready: usize, soft: usize, hard: usize) -> String {
    format!("Ready for Work {ready} · soft {soft} · hard {hard}")
}

/// A traffic-light level in the health colours.
pub fn level_color(level: Level, p: &Palette) -> Color32 {
    match level {
        Level::Ok => p.on_track,
        Level::Amber => p.at_risk,
        Level::Red => p.off_track,
    }
}

/// A report badge's text: what the last pre-check said about a ticket.
pub fn badge_text(badge: &Badge, report_date: &str) -> String {
    match badge {
        Badge::Promoted => "promoted by pre-check".to_string(),
        Badge::NeedsDetail(codes) => format!("needs detail: {} (report {report_date})", codes.join(", ")),
        Badge::Struck => "closed after pre-check".to_string(),
    }
}

/// The report badge as an outline pill (only status is filled): green when promoted, amber when it needs detail,
/// neutral when closed since.
pub fn badge(ui: &mut Ui, p: &Palette, badge: &Badge, report_date: &str) -> egui::Response {
    let text = badge_text(badge, report_date);
    let hue = match badge {
        Badge::Promoted => p.on_track,
        Badge::NeedsDetail(_) => p.at_risk,
        Badge::Struck => return outline_pill(ui, &text, p, PILL_MARGIN),
    };
    pill_sized(ui, &text, Color32::TRANSPARENT, hue, Stroke::new(1.0, hue), PILL_MARGIN)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn report_line_counts_or_names_the_configured_dir() {
        let r = report::parse(
            "# x — 2026-09-04\n\n## Planning — by epic\n\n### (no epic)\n\n- [CI-1](u) — a · rank 1 · older\n\n## Needs detail (still in `New`)\n\n### (no epic)\n\n- [CI-2](u) — b · rank 2 · created 2026-09-01 · missing: empty\n",
        );
        assert_eq!(report_line(Some((Path::new("/r/2026-09-04.md"), &r)), "~/x"), "Report 2026-09-04 · 1 promoted · 1 need detail");
        assert_eq!(report_line(None, "~/Documents/grooming-reports"), "No report found in ~/Documents/grooming-reports");
        let undated = report::Report { date: String::new(), ..r };
        assert_eq!(report_date(Path::new("/r/2026-10-01.md"), &undated), "2026-10-01");
    }

    #[test]
    fn badges_say_what_the_report_said() {
        assert_eq!(badge_text(&Badge::Promoted, "2026-09-04"), "promoted by pre-check");
        let nd = Badge::NeedsDetail(vec!["unclear-when".into(), "empty".into()]);
        assert_eq!(badge_text(&nd, "2026-09-04"), "needs detail: unclear-when, empty (report 2026-09-04)");
        assert_eq!(badge_text(&Badge::Struck, "2026-09-04"), "closed after pre-check");
    }

    #[test]
    fn ceiling_levels_take_the_health_colours() {
        let p = &theme::LIGHT;
        assert_eq!(level_color(Level::Ok, p), p.on_track);
        assert_eq!(level_color(Level::Amber, p), p.at_risk);
        assert_eq!(level_color(Level::Red, p), p.off_track);
        assert_eq!(ceiling_text(9, 8, 12), "Ready for Work 9 · soft 8 · hard 12");
    }

    #[test]
    fn both_modes_draw_headless() {
        let cfg: crate::config::Config = toml::from_str("site = \"https://x.atlassian.net\"\nemail = \"a@b.c\"\n").unwrap();
        let mut app = App::for_test(cfg, None);
        let status = |n: &str| crate::jira::Status { name: n.into(), category: "new".into() };
        app.store.absorb(
            crate::store::ListId::TeamOpen,
            vec![
                crate::jira::Issue { key: "CI-1".into(), labels: vec!["PLAT".into()], status: status("New"), ..Default::default() },
                crate::jira::Issue { key: "CI-2".into(), labels: vec!["PLAT".into()], status: status("Ready for Work"), ..Default::default() },
            ],
        );
        let texts = |app: &mut App| {
            let shapes = crate::testutil::paint_at2(egui::vec2(1200.0, 800.0), |ui| show(app, ui));
            crate::testutil::inks(&shapes).into_iter().map(|(t, _)| t).collect::<Vec<_>>()
        };
        let pre = texts(&mut app);
        for want in [
            "Pre-check",
            "Session",
            "No report found in ~/Documents/grooming-reports",
            "Ready for Work 1 · soft 10 · hard 15",
            "Pre-check: 1 in New, 0 with a verdict",
        ] {
            assert!(pre.iter().any(|t| t == want), "{want} in {pre:?}");
        }
        app.grooming.mode = GroomMode::Session;
        let session = texts(&mut app);
        assert!(session.iter().any(|t| t == "Session: 0 epics with Planning tickets"), "{session:?}");
    }
}
