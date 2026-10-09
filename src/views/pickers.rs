//! Modal pickers for transitions, resolutions, assignees, sprints, and field edits.
//! Keys (↑/↓/j/k, Enter, Esc) are handled in `App::shortcuts`; this only draws.

use crate::app::{
    Action, App, Picker, assign_current_row, assign_cursor_after_filter, assign_note, assign_rows, sprint_current_row, sprint_row_label, transition_row,
};
use crate::theme::{self, Palette};
use crate::views::primary_button;
use egui::{Frame, Margin, RichText, vec2};

pub fn show(app: &mut App, ctx: &egui::Context) {
    let p = theme::palette(ctx);
    if app.picker_loading {
        egui::Area::new(egui::Id::new("picker-loading")).order(egui::Order::Foreground).anchor(egui::Align2::CENTER_CENTER, vec2(0.0, 0.0)).show(ctx, |ui| {
            crate::views::icons::spinner(ui, crate::views::icons::INLINE_SPINNER, p);
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
        Frame::new()
            .fill(crate::views::dialog_fill(p))
            .stroke(crate::views::dialog_stroke(p))
            .corner_radius(theme::RADIUS)
            .inner_margin(Margin::same(16))
            .show(ui, |ui| {
                ui.set_width(440.0);
                // Fields on `bg` with the dialog's own edge, in both themes.
                crate::views::dialog_fields(ui, p);
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
                        let r = ui.add(field(filter, "Type a name", p));
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
                        names.extend(items.iter().map(|s| Row::plain(sprint_row_label(&s.name, &s.state))));
                        for (i, n) in names.iter_mut().enumerate() {
                            n.current = now == Some(i);
                        }
                        rows(ui, p, names, &mut confirm, cursor, None);
                    }
                    Picker::Edit { key, labels, priority, points, .. } => {
                        title(ui, p, &format!("Edit {key}"));
                        let labels_id = egui::Id::new("edit-labels");
                        let r = edit_group(ui, p, "Labels, comma separated", field(labels, "", p).id(labels_id));
                        // Labels takes focus on open (and whenever no field holds it); Tab moves on.
                        if !help_open && ui.memory(|m| m.focused().is_none()) {
                            r.request_focus();
                        }
                        ui.add_space(12.0);
                        edit_group(ui, p, "Priority", field(priority, crate::app::PRIORITY_HINT, p));
                        ui.add_space(12.0);
                        edit_group(ui, p, "Story points", field(points, crate::app::POINTS_HINT, p));
                    }
                }
                ui.add_space(BUTTONS_GAP);
                // The hint bar carries the keys; the dialog carries one primary action.
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    if ui.add(primary_button("Confirm", p)).clicked() {
                        confirm = true;
                    }
                    if ui.add(crate::views::dialog_button("Cancel", p)).clicked() {
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

/// The dialog's top edge: 20% down the window and 8 more, so at 900 it clears the detail pane's pill row
/// instead of cutting across it.
pub fn dialog_top(window_h: f32) -> f32 {
    (window_h * 0.2).round() + 8.0
}

/// Rows the Assign list holds: Unassigned, up to ASSIGN_USERS users, and the "Type to see more" line.
const ASSIGN_ROWS: f32 = (crate::app::ASSIGN_USERS + 2) as f32;
/// One picker row: 14 px text plus 6 px above and below.
const ROW_H: f32 = 30.0;

/// Title → first row, and last row → buttons: 24 measured ink to fill (round 6 measured 22 with 12 and 16).
const TITLE_GAP: f32 = 14.0;
const BUTTONS_GAP: f32 = 18.0;

fn title(ui: &mut egui::Ui, p: &Palette, text: &str) {
    ui.label(RichText::new(text).font(theme::bold(15.0)).color(p.text));
    ui.add_space(TITLE_GAP);
}

/// Assign's fixed block: the note row ("N matches", "Type to see more") right after the last row, then the
/// space that keeps the block one height however few rows the filter leaves. Returns (note top, space after).
pub fn assign_block(block_h: f32, rows_h: f32, note_h: f32) -> (f32, f32) {
    (rows_h, (block_h - rows_h - note_h).max(0.0))
}

/// A picker row's text inset (its frame margin): fields in pickers share it.
pub const ROW_INSET: f32 = 8.0;

/// The app's one single-line input (`views::input_field`), its text and placeholder at the rows' 8 px inset.
fn field<'a>(text: &'a mut String, hint: &str, p: &Palette) -> egui::TextEdit<'a> {
    crate::views::input_field(text, hint, theme::body(14.0), p).margin(crate::views::input_margin(ROW_INSET))
}

/// A small muted label, 4 px, then the field (full width).
fn edit_group(ui: &mut egui::Ui, p: &Palette, label: &str, field: egui::TextEdit<'_>) -> egui::Response {
    ui.label(RichText::new(label).color(crate::views::dialog_muted(p)).size(12.0));
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
        ui.label(RichText::new("Nothing available").color(crate::views::dialog_muted(p)));
        return;
    }
    if let Some(note) = fixed {
        let h = ASSIGN_ROWS * ROW_H;
        ui.allocate_ui_with_layout(vec2(ui.available_width(), h), egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.set_min_height(h);
            let top = ui.cursor().top();
            row_list(ui, p, &names, confirm, cursor);
            if let Some(note) = note {
                // Right under the rows, then the rest of the block, so the buttons stay where they are.
                let (_, after) = assign_block(h, ui.cursor().top() - top, ROW_H);
                Frame::new().inner_margin(Margin::symmetric(ROW_INSET as i8, 6)).show(ui, |ui| {
                    ui.set_min_height(ROW_H - 12.0);
                    ui.label(RichText::new(note).size(12.0).color(crate::views::dialog_muted(p)));
                });
                ui.add_space(after);
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
            let on_cursor = i == *cursor;
            let fill = if on_cursor { crate::views::dialog_cursor_fill(p) } else { egui::Color32::TRANSPARENT };
            let r = Frame::new()
                .fill(fill)
                .corner_radius(4)
                .inner_margin(Margin::symmetric(ROW_INSET as i8, 6))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.set_min_height(ROW_H - 12.0);
                    // Secondary text lifts on the cursor fill where muted would fall under 5.0:1.
                    let muted = if on_cursor { theme::muted_on(fill, p) } else { crate::views::dialog_muted(p) };
                    let color = if n.muted { muted } else { p.text };
                    let mut job = egui::text::LayoutJob::default();
                    job.append(&n.text, 0.0, egui::TextFormat { font_id: theme::body(14.0), color, ..Default::default() });
                    if n.current {
                        job.append("current", 8.0, egui::TextFormat { font_id: theme::body(12.0), color: muted, ..Default::default() });
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
    fn assign_count_follows_the_rows() {
        assert_eq!(super::assign_block(300.0, 60.0, 30.0), (60.0, 210.0), "two rows: the count right after them, the rest below");
        assert_eq!(super::assign_block(300.0, 270.0, 30.0), (270.0, 0.0), "a full list");
        assert_eq!(super::assign_block(300.0, 290.0, 30.0), (290.0, 0.0));
    }

    #[test]
    fn picker_fields_share_the_row_inset() {
        assert_eq!(crate::views::input_margin(super::ROW_INSET).left, 8);
        assert_eq!(super::ROW_INSET, crate::views::INPUT_PAD_X, "one input padding everywhere");
    }

    #[test]
    fn dialog_sits_a_fifth_down() {
        assert_eq!(super::dialog_top(900.0), 188.0);
        assert_eq!(super::dialog_top(0.0), 8.0);
    }
}
