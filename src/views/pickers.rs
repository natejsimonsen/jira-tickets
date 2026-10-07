//! Modal pickers for transitions, resolutions, assignees, sprints, and field edits.
//! Keys (↑/↓/j/k, Enter, Esc) are handled in `App::shortcuts`; this only draws.

use crate::app::{Action, App, Picker, filtered_users};
use crate::theme::{self, Palette};
use crate::views::{button, primary_button};
use egui::{Frame, Margin, RichText, Stroke, vec2};

pub fn show(app: &mut App, ctx: &egui::Context) {
    let p = theme::palette(ctx);
    if app.picker_loading {
        egui::Area::new(egui::Id::new("picker-loading")).order(egui::Order::Foreground).anchor(egui::Align2::CENTER_CENTER, vec2(0.0, 0.0)).show(ctx, |ui| {
            ui.add(egui::Spinner::new().size(24.0));
        });
    }
    let Some(picker) = app.picker.as_mut() else { return };
    // Dim the page behind the dialog and swallow its clicks.
    egui::Area::new(egui::Id::new("picker-backdrop")).order(egui::Order::Middle).fixed_pos(egui::pos2(0.0, 0.0)).show(ctx, |ui| {
        let r = ui.ctx().content_rect();
        ui.painter().rect_filled(r, 0.0, egui::Color32::from_black_alpha(90));
        ui.allocate_rect(r, egui::Sense::click());
    });
    let mut confirm = false;
    let mut cancel = false;
    let confirm_label = if matches!(picker, Picker::Edit { .. }) { "Save" } else { "Confirm" };
    egui::Area::new(egui::Id::new("picker")).order(egui::Order::Foreground).anchor(egui::Align2::CENTER_CENTER, vec2(0.0, -40.0)).show(ctx, |ui| {
        Frame::new().fill(p.surface).stroke(Stroke::new(1.0, p.border)).corner_radius(8).inner_margin(Margin::same(14)).show(ui, |ui| {
            ui.set_width(440.0);
            match picker {
                Picker::Transition { key, items, cursor } => {
                    title(ui, p, &format!("Transition {key}"));
                    rows(ui, p, items.iter().map(|t| format!("{}  →  {}", t.name, t.to)).collect(), &mut confirm, cursor);
                }
                Picker::Resolution { key, items, cursor, .. } => {
                    title(ui, p, &format!("Resolution for {key}"));
                    rows(ui, p, items.iter().map(|r| r.name.clone()).collect(), &mut confirm, cursor);
                }
                Picker::Assign { key, items, cursor, filter } => {
                    title(ui, p, &format!("Assign {key}"));
                    let r = ui.add(egui::TextEdit::singleline(filter).hint_text("Type a name").desired_width(f32::INFINITY));
                    r.request_focus();
                    let mut names = vec!["Unassigned".to_string()];
                    names.extend(filtered_users(items, filter).iter().map(|u| u.display_name.clone()));
                    if *cursor >= names.len() {
                        *cursor = 0;
                    }
                    rows(ui, p, names, &mut confirm, cursor);
                }
                Picker::Sprint { key, items, cursor } => {
                    title(ui, p, &format!("Move {key} to sprint"));
                    let mut names = vec!["Backlog".to_string()];
                    names.extend(items.iter().map(|s| format!("{} ({})", s.name, s.state)));
                    rows(ui, p, names, &mut confirm, cursor);
                }
                Picker::Edit { key, labels, priority, points } => {
                    title(ui, p, &format!("Edit {key}"));
                    ui.label(RichText::new("Labels (comma separated)").color(p.muted).size(12.0));
                    ui.add(egui::TextEdit::singleline(labels).desired_width(f32::INFINITY));
                    ui.label(RichText::new("Priority").color(p.muted).size(12.0));
                    ui.add(egui::TextEdit::singleline(priority).desired_width(f32::INFINITY));
                    ui.label(RichText::new("Story points").color(p.muted).size(12.0));
                    ui.add(egui::TextEdit::singleline(points).desired_width(120.0));
                }
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.add(primary_button(confirm_label, p)).clicked() {
                    confirm = true;
                }
                if ui.add(button("Cancel", p)).clicked() {
                    cancel = true;
                }
                let hint = if confirm_label == "Save" { "Esc" } else { "Enter · Esc" };
                ui.label(RichText::new(hint).color(p.muted).size(11.0));
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

fn title(ui: &mut egui::Ui, p: &Palette, text: &str) {
    ui.label(RichText::new(text).font(theme::bold(15.0)).color(p.text));
    ui.add_space(6.0);
}

/// Draws the rows with `cursor` highlighted. A click picks the row and confirms;
/// hover does not move the cursor, so a resting mouse can't fight the keys.
fn rows(ui: &mut egui::Ui, p: &Palette, names: Vec<String>, confirm: &mut bool, cursor: &mut usize) {
    if names.is_empty() {
        ui.label(RichText::new("Nothing available").color(p.muted));
        return;
    }
    egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
        for (i, n) in names.iter().enumerate() {
            let fill = if i == *cursor { p.selected_row } else { p.surface };
            let r = Frame::new()
                .fill(fill)
                .corner_radius(4)
                .inner_margin(Margin::symmetric(8, 6))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(n).size(13.5).color(p.text));
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
