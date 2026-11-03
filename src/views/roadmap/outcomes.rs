//! Roadmap tab 4: a card per metric with the active initiatives that move it, then the later ones, and a gap
//! note on metrics nothing in progress moves.

use super::{CARD_GAP, GAP, LINE_GAP, health_dot};
use crate::roadmap::{self, Health, Horizon, Initiative};
use crate::theme::{self, Palette};
use egui::{RichText, Ui};

/// A card's narrowest width; the cards fill as many columns as fit.
const CARD_MIN_W: f32 = 280.0;
/// The note on a metric with no Now initiative.
pub const GAP_NOTE: &str = "Nothing in progress moves this metric.";

/// "2 active" on a metric card's heading.
pub fn active_text(n: usize) -> String {
    format!("{n} active")
}

/// A later item's horizon, as it follows the name: "(next)".
pub fn horizon_text(h: Horizon) -> &'static str {
    match h {
        Horizon::Now => "(now)",
        Horizon::Next => "(next)",
        Horizon::Later => "(later)",
        Horizon::Done => "(done)",
    }
}

/// How many cards fit across `w`, at least one.
pub fn card_columns(w: f32) -> usize {
    (((w + GAP) / (CARD_MIN_W + GAP)).floor() as usize).max(1)
}

/// The empty state when no epic carries a `metric:` label: what is missing, then how to add one.
pub const NO_METRICS: (&str, &str) = ("No outcomes yet", "Add a `metric:` label to an epic, for example `metric:lead-time`, to track what it moves.");
/// The empty state when every initiative with a metric has shipped.
pub const ALL_SHIPPED: (&str, &str) = ("Every initiative with a metric has shipped", "Add a `metric:` label to a new epic to track it here.");

pub(super) fn show(ui: &mut Ui, p: &Palette, items: &[Initiative]) {
    let groups = roadmap::outcomes(items);
    if groups.is_empty() {
        let (msg, hint) = if items.iter().all(|i| i.metrics.is_empty()) { NO_METRICS } else { ALL_SHIPPED };
        // In the column panel, like the Now / Next / Later empty states.
        super::panel(ui, p, |ui| {
            ui.set_width(ui.available_width());
            super::empty_state(ui, p, msg, hint, p.surface_2);
        });
        return;
    }
    let n = card_columns(ui.available_width());
    ui.spacing_mut().item_spacing = egui::vec2(GAP, GAP);
    for row in groups.chunks(n) {
        ui.columns(n, |uis| {
            for (ui, (metric, active, later)) in uis.iter_mut().zip(row) {
                card(ui, p, metric, active, later);
            }
        });
    }
}

/// A metric card: the metric and "n active", the Now items with health dots, the later items muted with their
/// horizon, and the gap note when nothing in progress moves it.
fn card(ui: &mut Ui, p: &Palette, metric: &str, active: &[&Initiative], later: &[&Initiative]) {
    crate::views::components::card_frame(ui, p, false).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing = egui::vec2(CARD_GAP, LINE_GAP);
        ui.horizontal(|ui| {
            ui.label(RichText::new(metric).font(theme::bold(theme::spacing(ui.ctx()).h3)).color(p.text));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(active_text(active.len())).size(theme::spacing(ui.ctx()).small).color(p.muted));
            });
        });
        ui.add_space(LINE_GAP);
        for i in active {
            ui.horizontal(|ui| {
                health_dot(ui, i.health, p);
                ui.add(egui::Label::new(RichText::new(&i.name).color(p.text)).truncate());
            });
        }
        for i in later {
            ui.horizontal_wrapped(|ui| {
                health_dot(ui, Health::None, p);
                ui.label(RichText::new(&i.name).color(p.muted));
                ui.label(RichText::new(horizon_text(i.horizon)).size(12.0).color(p.muted));
            });
        }
        if active.is_empty() {
            ui.add_space(CARD_GAP - LINE_GAP);
            ui.label(RichText::new(GAP_NOTE).size(12.0).color(p.at_risk));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wording_and_columns() {
        assert_eq!(active_text(2), "2 active");
        assert_eq!(horizon_text(Horizon::Later), "(later)");
        assert_eq!(GAP_NOTE, "Nothing in progress moves this metric.");
        assert_eq!(card_columns(200.0), 1);
        assert_eq!(card_columns(1408.0), 4);
    }
}
