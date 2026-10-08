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
    let backdrop = egui::Area::new(egui::Id::new("help-backdrop")).order(egui::Order::Foreground).fixed_pos(egui::pos2(0.0, 0.0)).show(ctx, |ui| {
        let r = ui.ctx().content_rect();
        ui.painter().rect_filled(r, 0.0, egui::Color32::from_black_alpha(90));
        ui.allocate_rect(r, egui::Sense::click())
    });
    if backdrop.inner.clicked() {
        app.actions.push(Action::ToggleHelp);
    }
    egui::Area::new(egui::Id::new("help")).order(egui::Order::Tooltip).anchor(egui::Align2::CENTER_CENTER, vec2(0.0, -20.0)).show(ctx, |ui| {
        Frame::new().fill(p.surface).stroke(Stroke::new(1.0, p.border)).corner_radius(8).inner_margin(Margin::same(18)).show(ui, |ui| {
            ui.set_width(520.0);
            ui.label(RichText::new("Keyboard").font(theme::bold(16.0)).color(p.text));
            ui.add_space(8.0);
            egui::ScrollArea::vertical().max_height((ctx.content_rect().height() - 160.0).max(200.0)).show(ui, |ui| {
                for g in [Group::Global, Group::Issues, Group::Pickers] {
                    group(ui, p, g);
                    ui.add_space(8.0);
                }
            });
            ui.label(RichText::new("Press ? or Esc to close").color(p.muted).size(12.0));
        });
    });
}

fn group(ui: &mut egui::Ui, p: &Palette, g: Group) {
    ui.label(RichText::new(g.name()).font(theme::bold(12.0)).color(p.muted));
    ui.add_space(2.0);
    egui::Grid::new(("help-group", g.name())).num_columns(2).spacing(vec2(16.0, 6.0)).min_col_width(130.0).show(ui, |ui| {
        for k in BINDINGS.iter().filter(|k| k.group == g) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for c in keys::chips(k.keys) {
                    Frame::new().fill(p.surface_2).stroke(Stroke::new(1.0, p.border)).corner_radius(4).inner_margin(Margin::symmetric(6, 1)).show(ui, |ui| {
                        ui.label(RichText::new(c).font(theme::mono(13.0)).color(p.text));
                    });
                }
            });
            ui.label(RichText::new(k.what).font(theme::body(13.0)).color(p.text));
            ui.end_row();
        }
    });
}
