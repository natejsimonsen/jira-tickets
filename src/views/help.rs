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
    if crate::views::backdrop(ctx, "help-backdrop", egui::Order::Foreground, crate::views::hint_bar_height(app), p).clicked() {
        app.actions.push(Action::ToggleHelp);
    }
    egui::Area::new(egui::Id::new("help")).order(egui::Order::Tooltip).anchor(egui::Align2::CENTER_CENTER, vec2(0.0, -20.0)).show(ctx, |ui| {
        // Issues on the left; Global then Pickers on the right. Each column is as wide as its widest key cell
        // plus 16 plus its widest action, so the panel has 24 px on every side and no dead space.
        let left = [Group::Issues];
        let right = [Group::Global, Group::Pickers];
        let measure = |groups: &[Group]| {
            let rows = BINDINGS.iter().filter(|k| groups.contains(&k.group));
            let keys: Vec<f32> = rows.clone().map(|k| keys_width(ui, k.keys, p)).collect();
            let actions: Vec<f32> = rows.map(|k| ui.painter().layout_no_wrap(k.what.to_string(), theme::body(14.0), p.text).size().x).collect();
            (keys.iter().copied().fold(0.0, f32::max), column_width(&keys, &actions))
        };
        let (lk, lw) = measure(&left);
        let (rk, rw) = measure(&right);
        Frame::new().fill(crate::views::dialog_fill(p)).stroke(crate::views::dialog_stroke(p)).corner_radius(6).inner_margin(Margin::same(PAD as i8)).show(
            ui,
            |ui| {
                ui.set_width(lw + COL_GAP + rw);
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.label(RichText::new("Keyboard").font(theme::bold(15.0)).color(p.text));
                ui.add_space(12.0);
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = COL_GAP;
                    ui.allocate_ui_with_layout(vec2(lw, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                        ui.set_width(lw);
                        group(ui, p, Group::Issues, lk);
                    });
                    ui.allocate_ui_with_layout(vec2(rw, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                        ui.set_width(rw);
                        group(ui, p, Group::Global, rk);
                        ui.add_space(16.0);
                        group(ui, p, Group::Pickers, rk);
                    });
                });
            },
        );
    });
}

/// The panel's padding on every side, and the gap between its columns.
const PAD: f32 = 24.0;
const COL_GAP: f32 = 24.0;
/// Key cell → action: 16 px, never more.
const KEY_TO_ACTION: f32 = 16.0;
/// Between a chip and the "or" / "to" beside it.
const CHIP_GAP: f32 = 4.0;
/// One row's pitch: the row (chip height) plus the gap to the next.
const ROW_PITCH: f32 = 34.0;

/// A help column's width: its widest key cell, 16, its widest action.
pub fn column_width(keys: &[f32], actions: &[f32]) -> f32 {
    let max = |v: &[f32]| v.iter().copied().fold(0.0, f32::max);
    (max(keys) + KEY_TO_ACTION + max(actions)).ceil()
}

fn chip_size(ui: &egui::Ui, text: &str, p: &Palette) -> egui::Vec2 {
    let g = ui.painter().layout_no_wrap(text.to_string(), theme::mono(12.5), p.text);
    vec2((g.size().x + 12.0).max(20.0), g.size().y + 4.0)
}

fn word_width(ui: &egui::Ui, word: &str, p: &Palette) -> f32 {
    ui.painter().layout_no_wrap(word.into(), theme::body(12.0), p.muted).size().x
}

/// The width of one binding's key cell: its chips, with a muted "or" between alternatives and "to" between
/// a range's ends, each 4 px from its chips.
fn keys_width(ui: &egui::Ui, keys: &str, p: &Palette) -> f32 {
    let mut w = 0.0;
    for (i, alt) in keys::chips(keys).enumerate() {
        if i > 0 {
            w += 2.0 * CHIP_GAP + word_width(ui, "or", p);
        }
        for (j, k) in keys::range(alt).into_iter().enumerate() {
            if j > 0 {
                w += 2.0 * CHIP_GAP + word_width(ui, "to", p);
            }
            w += chip_size(ui, k, p).x;
        }
    }
    w
}

/// The space under a help row so rows sit ROW_PITCH apart whatever the row's own height.
pub fn row_gap(row_h: f32) -> f32 {
    (ROW_PITCH - row_h).max(0.0)
}

fn group(ui: &mut egui::Ui, p: &Palette, g: Group, key_w: f32) {
    ui.label(RichText::new(g.name()).font(theme::bold(12.0)).color(p.muted));
    ui.add_space(8.0);
    // Every row is as tall as its tallest part (a chip or the action), and chips, words and the action are
    // all centred on that row.
    let row_h = chip_size(ui, "⌘", p).y.max(ui.fonts_mut(|f| f.row_height(&theme::body(14.0))));
    for (i, k) in BINDINGS.iter().filter(|k| k.group == g).enumerate() {
        if i > 0 {
            ui.add_space(row_gap(row_h));
        }
        let centred = egui::Layout::left_to_right(egui::Align::Center);
        ui.allocate_ui_with_layout(vec2(ui.available_width(), row_h), centred, |ui| {
            ui.set_min_height(row_h);
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.allocate_ui_with_layout(vec2(key_w, row_h), centred, |ui| {
                ui.set_width(key_w);
                ui.set_min_height(row_h);
                ui.spacing_mut().item_spacing.x = CHIP_GAP;
                let word = |ui: &mut egui::Ui, w: &str| ui.label(RichText::new(w).font(theme::body(12.0)).color(p.muted));
                for (i, alt) in keys::chips(k.keys).enumerate() {
                    if i > 0 {
                        word(ui, "or");
                    }
                    for (j, c) in keys::range(alt).into_iter().enumerate() {
                        if j > 0 {
                            word(ui, "to");
                        }
                        chip(ui, c, p);
                    }
                }
            });
            ui.add_space(KEY_TO_ACTION);
            ui.add(egui::Label::new(RichText::new(k.what).font(theme::body(14.0)).color(p.text)).extend());
        });
    }
}

/// A key chip: mono 12.5 in a 6/2 frame, at least 20 wide, text centred. Its fill steps off the dialog's
/// (`chip_fill_on`).
fn chip(ui: &mut egui::Ui, text: &str, p: &Palette) {
    let galley = ui.painter().layout_no_wrap(text.to_string(), theme::mono(12.5), p.text);
    let size = chip_size(ui, text, p);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let fill = theme::chip_fill_on(crate::views::dialog_fill(p), p);
    // In dark the chip fill is the dialog edge itself, so its outline is the next step up.
    let edge = if p.dark { crate::views::selected_edge(p) } else { p.border };
    ui.painter().rect(rect, 4.0, fill, Stroke::new(1.0, edge), egui::StrokeKind::Inside);
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, p.text);
}

#[cfg(test)]
mod tests {
    #[test]
    fn rows_sit_34_apart() {
        assert_eq!(super::row_gap(18.0), 16.0);
        assert_eq!(super::row_gap(20.0), 14.0);
    }

    #[test]
    fn columns_hug_their_content() {
        assert_eq!(super::column_width(&[20.0, 72.5], &[100.0, 180.2]), 269.0);
        assert_eq!(super::column_width(&[], &[]), 16.0);
    }
}
