//! Modal pickers for transitions, resolutions, assignees, sprints, and field edits.
//! Keys (↑/↓/j/k, Enter, Esc) are handled in `App::shortcuts`; this only draws.

use crate::app::{Action, App, Picker, assign_cursor_after_filter, filtered_users, transition_row};
use crate::theme::{self, Palette};
use crate::views::{button, primary_button};
use egui::{Frame, Margin, RichText, vec2};

pub fn show(app: &mut App, ctx: &egui::Context) {
    let p = theme::palette(ctx);
    if app.picker_loading {
        egui::Area::new(egui::Id::new("picker-loading")).order(egui::Order::Foreground).anchor(egui::Align2::CENTER_CENTER, vec2(0.0, 0.0)).show(ctx, |ui| {
            crate::views::icons::spinner(ui, 24.0, p);
        });
    }
    let help_open = app.help_open;
    let hint_bar = crate::views::hint_bar_height(app);
    let Some(picker) = app.picker.as_mut() else { return };
    // Dim the page behind the dialog and swallow its clicks.
    crate::views::backdrop(ctx, "picker-backdrop", egui::Order::Middle, hint_bar, p);
    let mut confirm = false;
    let mut cancel = false;
    // A fixed top edge, so a list that shrinks while filtering never moves the dialog.
    let top = dialog_top(ctx.content_rect().height());
    egui::Area::new(egui::Id::new("picker")).order(egui::Order::Foreground).anchor(egui::Align2::CENTER_TOP, vec2(0.0, top)).show(ctx, |ui| {
        Frame::new().fill(crate::views::dialog_fill(p)).stroke(crate::views::dialog_stroke(p)).corner_radius(6).inner_margin(Margin::same(16)).show(ui, |ui| {
            ui.set_width(440.0);
            match picker {
                Picker::Transition { key, items, cursor } => {
                    title(ui, p, &format!("Transition {key}"));
                    rows(ui, p, items.iter().map(transition_row).collect(), &mut confirm, cursor, false);
                }
                Picker::Resolution { key, items, cursor, .. } => {
                    title(ui, p, &format!("Resolution for {key}"));
                    rows(ui, p, items.iter().map(|r| r.name.clone()).collect(), &mut confirm, cursor, false);
                }
                Picker::Assign { key, items, cursor, filter } => {
                    title(ui, p, &format!("Assign {key}"));
                    let r = ui.add(field(filter).hint_text("Type a name"));
                    ui.add_space(8.0);
                    // Not under the help overlay, or typing would land in the filter behind it.
                    if !help_open {
                        r.request_focus();
                    }
                    let shown = filtered_users(items, filter);
                    if r.changed() {
                        *cursor = assign_cursor_after_filter(shown.len(), filter.trim().is_empty());
                    }
                    let mut names = vec!["Unassigned".to_string()];
                    names.extend(shown.iter().map(|u| u.display_name.clone()));
                    if *cursor >= names.len() {
                        *cursor = 0;
                    }
                    rows(ui, p, names, &mut confirm, cursor, true);
                }
                Picker::Sprint { key, items, cursor } => {
                    title(ui, p, &format!("Move {key} to sprint"));
                    let mut names = vec!["Backlog".to_string()];
                    names.extend(items.iter().map(|s| format!("{} ({})", s.name, s.state)));
                    rows(ui, p, names, &mut confirm, cursor, false);
                }
                Picker::Edit { key, labels, priority, points, .. } => {
                    title(ui, p, &format!("Edit {key}"));
                    edit_group(ui, p, "Labels, comma separated", field(labels));
                    ui.add_space(12.0);
                    edit_group(ui, p, "Priority", field(priority));
                    ui.add_space(12.0);
                    edit_group(ui, p, "Story points", field(points).desired_width(120.0));
                }
            }
            ui.add_space(16.0);
            // The hint bar carries the keys; the dialog carries one primary action.
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if ui.add(primary_button("Confirm", p)).clicked() {
                    confirm = true;
                }
                if ui.add(button("Cancel", p)).clicked() {
                    cancel = true;
                }
            });
        });
    });
    if confirm {
        app.actions.push(Action::PickerConfirm);
    }
    if cancel {
        app.actions.push(Action::PickerCancel);
    }
}

/// The dialog's top edge: 20% down the window.
pub fn dialog_top(window_h: f32) -> f32 {
    (window_h * 0.2).round()
}

/// Rows the Assign list shows before it scrolls.
const ASSIGN_ROWS: f32 = 10.0;
/// One picker row: 14 px text plus 6 px above and below.
const ROW_H: f32 = 30.0;

fn title(ui: &mut egui::Ui, p: &Palette, text: &str) {
    ui.label(RichText::new(text).font(theme::bold(15.0)).color(p.text));
    ui.add_space(12.0);
}

/// A framed single-line input (the theme's 1 px edge) with 8 px margins.
fn field(text: &mut String) -> egui::TextEdit<'_> {
    egui::TextEdit::singleline(text).desired_width(f32::INFINITY).margin(vec2(8.0, 6.0))
}

/// A small muted label, 4 px, then the field.
fn edit_group(ui: &mut egui::Ui, p: &Palette, label: &str, field: egui::TextEdit<'_>) {
    ui.label(RichText::new(label).color(p.muted).size(12.0));
    ui.add_space(4.0);
    ui.add(field);
}

/// Draws the rows with `cursor` highlighted. A click picks the row and confirms;
/// hover does not move the cursor, so a resting mouse can't fight the keys.
/// `fixed`: the list keeps a 10-row height with its scrollbar always shown (Assign, whose filter shrinks it).
fn rows(ui: &mut egui::Ui, p: &Palette, names: Vec<String>, confirm: &mut bool, cursor: &mut usize, fixed: bool) {
    if names.is_empty() {
        ui.label(RichText::new("Nothing available").color(p.muted));
        return;
    }
    let mut area = egui::ScrollArea::vertical().max_height(if fixed { ASSIGN_ROWS * ROW_H } else { 360.0 });
    if fixed {
        area = area
            .min_scrolled_height(ASSIGN_ROWS * ROW_H)
            .auto_shrink([false, false])
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible);
    }
    area.show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        for (i, n) in names.iter().enumerate() {
            let fill = if i == *cursor { p.selected_row } else { egui::Color32::TRANSPARENT };
            let r = Frame::new()
                .fill(fill)
                .corner_radius(4)
                .inner_margin(Margin::symmetric(8, 6))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.set_min_height(ROW_H - 12.0);
                    ui.add(egui::Label::new(RichText::new(n).size(14.0).color(p.text)).truncate().selectable(false));
                })
                .response
                .interact(egui::Sense::click());
            if r.clicked() {
                *cursor = i;
                *confirm = true;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn dialog_sits_a_fifth_down() {
        assert_eq!(super::dialog_top(900.0), 180.0);
        assert_eq!(super::dialog_top(0.0), 0.0);
    }
}
