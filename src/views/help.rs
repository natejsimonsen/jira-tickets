//! The keyboard help overlay (`?`), drawn from `keys::BINDINGS`. Its keys
//! (`?`, Esc) are handled in `App::shortcuts`; this only draws.

use crate::app::{Action, App};
use crate::keys::{self, BINDINGS, Group};
use crate::theme::{self, Palette};
use egui::{Frame, Margin, RichText, Stroke, vec2};

pub fn show(app: &mut App, ctx: &egui::Context) {
    if !app.help_open {
        return;
    }
    let p = theme::palette(ctx);
    // Above the pickers' Foreground dialog: the backdrop at Foreground (shown
    // after the dialog, so on top of it) and the panel at Tooltip.
    if crate::views::backdrop(ctx, "help-backdrop", egui::Order::Foreground, p).clicked() {
        app.actions.push(Action::ToggleHelp);
    }
    egui::Area::new(egui::Id::new("help")).order(egui::Order::Tooltip).anchor(egui::Align2::CENTER_CENTER, vec2(0.0, -20.0)).show(ctx, |ui| {
        Frame::new().fill(crate::views::dialog_fill(p)).stroke(Stroke::new(1.0, p.border)).corner_radius(6).inner_margin(Margin::same(16)).show(ui, |ui| {
            ui.set_width(WIDTH);
            ui.label(RichText::new("Keyboard").font(theme::bold(15.0)).color(p.text));
            ui.add_space(12.0);
            // Issues on the left; Global then Pickers on the right. No scrolling: it all fits.
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 24.0;
                let col = (WIDTH - 24.0) / 2.0;
                ui.allocate_ui_with_layout(vec2(col, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.set_width(col);
                    group(ui, p, Group::Issues);
                });
                ui.allocate_ui_with_layout(vec2(col, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.set_width(col);
                    group(ui, p, Group::Global);
                    ui.add_space(16.0);
                    group(ui, p, Group::Pickers);
                });
            });
            ui.add_space(12.0);
            ui.label(RichText::new("Press ? or Esc to close").color(p.muted).size(12.0));
        });
    });
}

const WIDTH: f32 = 680.0;
/// The key column: at least this wide, then 16 px to the action.
const KEY_COL: f32 = 72.0;

fn group(ui: &mut egui::Ui, p: &Palette, g: Group) {
    ui.label(RichText::new(g.name()).font(theme::bold(12.0)).color(p.muted));
    ui.add_space(8.0);
    egui::Grid::new(("help-group", g.name())).num_columns(2).spacing(vec2(16.0, 8.0)).min_col_width(KEY_COL).show(ui, |ui| {
        for k in BINDINGS.iter().filter(|k| k.group == g) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for c in keys::chips(k.keys) {
                    chip(ui, c, p);
                }
            });
            ui.add(egui::Label::new(RichText::new(k.what).font(theme::body(14.0)).color(p.text)).wrap());
            ui.end_row();
        }
    });
}

/// A key chip: mono 12.5 in a 6/2 frame, at least 20 wide, text centred.
fn chip(ui: &mut egui::Ui, text: &str, p: &Palette) {
    let galley = ui.painter().layout_no_wrap(text.to_string(), theme::mono(12.5), p.text);
    let size = vec2((galley.size().x + 12.0).max(20.0), galley.size().y + 4.0);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    ui.painter().rect(rect, 4.0, p.surface_2, Stroke::new(1.0, p.border), egui::StrokeKind::Inside);
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, p.text);
}
