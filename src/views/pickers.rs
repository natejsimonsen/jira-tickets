//! Modal pickers for transitions, resolutions, assignees, sprints, and field edits.
//! Keys (↑/↓/j/k, Enter, Esc) are handled in `App::shortcuts`; this only draws.

use crate::app::{
    Action, App, Picker, assign_current_row, assign_cursor_after_filter, assign_note, assign_rows, edit_hint, sprint_current_row, transition_row,
};
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
    // The issue's current status and sprint, for the "current" marker.
    let issue = app.picker.as_ref().map(picker_key).and_then(|k| app.store.get(k));
    let status = issue.map(|i| i.status.name.clone()).unwrap_or_default();
    let sprint = issue.and_then(|i| i.sprint.clone());
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
                    let names = items.iter().map(|t| Row::plain(transition_row(t)).current_if(t.to.eq_ignore_ascii_case(&status))).collect();
                    rows(ui, p, names, &mut confirm, cursor, None);
                }
                Picker::Resolution { key, items, cursor, .. } => {
                    title(ui, p, &format!("Resolution for {key}"));
                    rows(ui, p, items.iter().map(|r| Row::plain(r.name.clone())).collect(), &mut confirm, cursor, None);
                }
                Picker::Assign { key, items, current, cursor, filter } => {
                    title(ui, p, &format!("Assign {key}"));
                    let r = ui.add(field(filter).hint_text(crate::views::field_hint("Type a name", theme::body(14.0), p)));
                    ui.add_space(8.0);
                    // Not under the help overlay, or typing would land in the filter behind it.
                    if !help_open {
                        r.request_focus();
                    }
                    let (shown, cut) = assign_rows(items, filter, current.as_ref());
                    if r.changed() {
                        *cursor = assign_cursor_after_filter(&shown, filter, current.is_some());
                    }
                    let now = assign_current_row(&shown, current.as_ref());
                    let mut names = vec![Row::muted("Unassigned")];
                    names.extend(shown.iter().map(|u| Row::plain(u.display_name.clone())));
                    for (i, n) in names.iter_mut().enumerate() {
                        n.current = now == Some(i);
                    }
                    if *cursor >= names.len() {
                        *cursor = 0;
                    }
                    let note = assign_note(filter, shown.len(), cut);
                    rows(ui, p, names, &mut confirm, cursor, Some(note.as_deref()));
                }
                Picker::Sprint { key, items, cursor } => {
                    title(ui, p, &format!("Move {key} to sprint"));
                    let now = sprint_current_row(items, sprint.as_deref());
                    let mut names = vec![Row::plain("Backlog".to_string())];
                    names.extend(items.iter().map(|s| Row::plain(format!("{} ({})", s.name, s.state))));
                    for (i, n) in names.iter_mut().enumerate() {
                        n.current = now == Some(i);
                    }
                    rows(ui, p, names, &mut confirm, cursor, None);
                }
                Picker::Edit { key, labels, priority, points, orig_priority, orig_points, .. } => {
                    title(ui, p, &format!("Edit {key}"));
                    let labels_id = egui::Id::new("edit-labels");
                    let r = edit_group(ui, p, "Labels, comma separated", field(labels).id(labels_id));
                    // Labels takes focus on open (and whenever no field holds it); Tab moves on.
                    if !help_open && ui.memory(|m| m.focused().is_none()) {
                        r.request_focus();
                    }
                    ui.add_space(12.0);
                    let hint = |t: String| crate::views::field_hint(&t, theme::body(14.0), p);
                    edit_group(ui, p, "Priority", field(priority).hint_text(hint(edit_hint(orig_priority, Some(crate::app::PRIORITIES)))));
                    ui.add_space(12.0);
                    edit_group(ui, p, "Story points", field(points).hint_text(hint(edit_hint(orig_points, None))));
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

/// The issue a picker acts on.
fn picker_key(p: &Picker) -> &str {
    match p {
        Picker::Transition { key, .. }
        | Picker::Resolution { key, .. }
        | Picker::Assign { key, .. }
        | Picker::Sprint { key, .. }
        | Picker::Edit { key, .. } => key,
    }
}

/// The dialog's top edge: 20% down the window.
pub fn dialog_top(window_h: f32) -> f32 {
    (window_h * 0.2).round()
}

/// Rows the Assign list holds: Unassigned, up to ASSIGN_USERS users, and the "Type to see more" line.
const ASSIGN_ROWS: f32 = (crate::app::ASSIGN_USERS + 2) as f32;
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

/// A small muted label, 4 px, then the field (full width).
fn edit_group(ui: &mut egui::Ui, p: &Palette, label: &str, field: egui::TextEdit<'_>) -> egui::Response {
    ui.label(RichText::new(label).color(p.muted).size(12.0));
    ui.add_space(4.0);
    ui.add(field)
}

/// One picker row's text; `muted` for the synthetic "no value" row (Unassigned); `current` marks the
/// issue's present value with a muted "current" after it, wherever the cursor is.
struct Row {
    text: String,
    muted: bool,
    current: bool,
}

impl Row {
    fn plain(text: String) -> Self {
        Row { text, muted: false, current: false }
    }
    fn muted(text: &str) -> Self {
        Row { text: text.to_string(), muted: true, current: false }
    }
    fn current_if(mut self, current: bool) -> Self {
        self.current = current;
        self
    }
}

/// Draws the rows with `cursor` highlighted. A click picks the row and confirms;
/// hover does not move the cursor, so a resting mouse can't fight the keys.
/// `fixed`: Assign's capped list. It keeps one height (Unassigned, the users, a note row) whatever the
/// filter leaves, and the note ("Type to see more") shows when matches were cut.
fn rows(ui: &mut egui::Ui, p: &Palette, names: Vec<Row>, confirm: &mut bool, cursor: &mut usize, fixed: Option<Option<&str>>) {
    if names.is_empty() {
        ui.label(RichText::new("Nothing available").color(p.muted));
        return;
    }
    if let Some(note) = fixed {
        let h = ASSIGN_ROWS * ROW_H;
        ui.allocate_ui_with_layout(vec2(ui.available_width(), h), egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.set_min_height(h);
            row_list(ui, p, &names, confirm, cursor);
            if let Some(note) = note {
                Frame::new().inner_margin(Margin::symmetric(8, 6)).show(ui, |ui| {
                    ui.set_min_height(ROW_H - 12.0);
                    ui.label(RichText::new(note).size(12.0).color(p.muted));
                });
            }
        });
        return;
    }
    egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| row_list(ui, p, &names, confirm, cursor));
}

fn row_list(ui: &mut egui::Ui, p: &Palette, names: &[Row], confirm: &mut bool, cursor: &mut usize) {
    {
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
                    let color = if n.muted { p.muted } else { p.text };
                    let mut job = egui::text::LayoutJob::default();
                    job.append(&n.text, 0.0, egui::TextFormat { font_id: theme::body(14.0), color, ..Default::default() });
                    if n.current {
                        job.append("current", 8.0, egui::TextFormat { font_id: theme::body(12.0), color: p.muted, ..Default::default() });
                    }
                    ui.add(egui::Label::new(job).truncate().selectable(false));
                })
                .response
                .interact(egui::Sense::click());
            if r.clicked() {
                *cursor = i;
                *confirm = true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn dialog_sits_a_fifth_down() {
        assert_eq!(super::dialog_top(900.0), 180.0);
        assert_eq!(super::dialog_top(0.0), 0.0);
    }
}
