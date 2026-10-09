//! Modal pickers for transitions, resolutions, assignees, sprints, and field edits.
//! Keys (↑/↓/j/k, Enter, Esc) are handled in `App::shortcuts`; this only draws.

use crate::app::{
    Action, App, Picker, assign_current_row, assign_cursor_after_filter, assign_matches, assign_note, assign_rows, sprint_current_row, sprint_row_label,
    transition_row,
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
        Frame::new().fill(crate::views::dialog_fill(p)).stroke(crate::views::dialog_stroke(p)).corner_radius(theme::RADIUS).inner_margin(DIALOG_MARGIN).show(
            ui,
            |ui| {
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
                        let r = crate::views::add_field(ui, None, filter, |t| field(t, "Type a name", p));
                        ui.add_space(gap_space(ASSIGN_FIELD_GAP, ui.spacing().item_spacing.y));
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
                        let note = assign_note(filter, shown.len(), assign_matches(items, filter, current.as_ref()), cut);
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
                        let r = edit_group(ui, p, "Labels, comma separated", labels, |t| field(t, "", p).id(labels_id));
                        // Labels takes focus on open (and whenever no field holds it); Tab moves on.
                        if !help_open && ui.memory(|m| m.focused().is_none()) {
                            r.request_focus();
                        }
                        ui.add_space(group_space(ui, "Priority"));
                        edit_group(ui, p, "Priority", priority, |t| field(t, crate::app::PRIORITY_HINT, p));
                        ui.add_space(group_space(ui, "Story points"));
                        edit_group(ui, p, "Story points", points, |t| field(t, crate::app::POINTS_HINT, p));
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
            },
        );
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

/// The dialog's top edge: 20% down the window less DIALOG_LIFT, so at 900 (167) it covers the detail pane's pill row
/// under a two-line title (ink 175–192.5) with 8 of margin above it; 174 put its edge on the pills' first pixel row and
/// 188 cut across them (round 11 G1, round 12 G2: up, not down, since lower would show the pills' tops).
pub fn dialog_top(window_h: f32) -> f32 {
    ((window_h * 0.2).round() - DIALOG_LIFT).max(0.0)
}

/// How far the dialog sits above the window's 20% line (round 13 G4: 10 left 5 over the pills; 13 leaves 8).
const DIALOG_LIFT: f32 = 13.0;

/// The dialog's margin: 16 at the sides and bottom; the top is the title's (`title`), set to its ink.
const DIALOG_MARGIN: Margin = Margin { left: 16, right: 16, top: 0, bottom: 16 };
/// The dialog edge's stroke: egui starts the frame's content inside it, and the title's ink is measured from it.
const DIALOG_EDGE: f32 = 1.0;
/// The dialog's inner edge → the title's ink (round 13 G1: 19.5 with a 16 margin to its line box).
const TITLE_INK_TOP: f32 = 16.0;
/// An Edit field → the next group's label ink (round 13 G3: 20.5 with a 12 space).
const GROUP_INK_GAP: f32 = 16.0;

/// The space before an Edit group's `label`: GROUP_INK_GAP from the field above (its fill is its ink) to the
/// label's ink, less the layout's item spacing.
fn group_space(ui: &egui::Ui, label: &str) -> f32 {
    let top = theme::ink_rect(&ui.painter().layout_no_wrap(label.to_string(), theme::body(12.0), egui::Color32::WHITE)).top();
    gap_space(GROUP_INK_GAP - top, ui.spacing().item_spacing.y)
}

/// The Assign filter field → the first row (round 11 G2: 8 left the field and the cursor row reading as one).
const ASSIGN_FIELD_GAP: f32 = 12.0;

/// The space to add for a `gap` between two widgets: the layout's item spacing already lands after the first
/// (the Assign gap measured 18 = 12 + 6; round 12 G1).
pub fn gap_space(gap: f32, item_spacing_y: f32) -> f32 {
    (gap - item_spacing_y).max(0.0)
}

/// Rows the Assign list holds: Unassigned, up to ASSIGN_USERS users, and the "9 of N · type to see more" line.
const ASSIGN_ROWS: f32 = (crate::app::ASSIGN_USERS + 2) as f32;
/// One picker row: 14 px text plus 6 px above and below.
const ROW_H: f32 = 30.0;

/// Title → first row, and last row → buttons: 24 measured ink to fill (round 6 measured 22 with 12 and 16).
const TITLE_GAP: f32 = 14.0;
const BUTTONS_GAP: f32 = 18.0;

/// The dialog's title, TITLE_INK_TOP under the dialog's inner edge to its ink, then TITLE_GAP.
fn title(ui: &mut egui::Ui, p: &Palette, text: &str) {
    let g = ui.painter().layout_no_wrap(text.to_string(), theme::bold(15.0), p.text);
    // The frame's content already starts inside its 1 px edge.
    ui.add_space(TITLE_INK_TOP - theme::ink_rect(&g).top());
    ui.add(egui::Label::new(g));
    ui.add_space(TITLE_GAP);
}

/// Assign's fixed block: the note row ("N matches", "9 of N · type to see more") right after the last row, then the
/// space that keeps the block one height however few rows the filter leaves. Returns (note top, space after).
pub fn assign_block(block_h: f32, rows_h: f32, note_h: f32) -> (f32, f32) {
    (rows_h, (block_h - rows_h - note_h).max(0.0))
}

/// A picker row's text inset (its frame margin): fields in pickers share it.
pub const ROW_INSET: f32 = 8.0;

/// The app's one single-line input (`views::input_field`), its text and placeholder at the rows' 8 px inset.
pub(crate) fn field<'a>(text: &'a mut String, hint: &str, p: &Palette) -> egui::TextEdit<'a> {
    crate::views::input_field_in(text, hint, theme::body(14.0), p, crate::views::input_margin(ROW_INSET))
}

/// A small muted label, 4 px, then the field (full width).
fn edit_group(ui: &mut egui::Ui, p: &Palette, label: &str, text: &mut String, field: impl FnOnce(&mut String) -> egui::TextEdit<'_>) -> egui::Response {
    ui.label(RichText::new(label).color(crate::views::dialog_muted(p)).size(12.0));
    ui.add_space(4.0);
    crate::views::add_field(ui, None, text, field)
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
/// filter leaves, and the note ("9 of N · type to see more") shows when matches were cut.
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
                    let muted = if on_cursor { crate::views::muted_on_selected(p) } else { crate::views::dialog_muted(p) };
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
        assert_eq!(super::dialog_top(900.0), 167.0, "8 over the two-line-title pills' first ink row (175)");
        assert!(super::dialog_top(900.0) + 8.0 <= 175.0, "the dialog covers the pill row with margin");
        assert_eq!(super::dialog_top(0.0), 0.0);
        assert_eq!(super::ASSIGN_FIELD_GAP, 12.0);
    }

    #[test]
    fn titles_and_edit_groups_sit_16_to_the_ink() {
        // Round 13 G1 / G3: 16 from the dialog's inner edge to the title's ink, 16 from an Edit field to the next
        // label's ink, with the real fonts at the captures' density.
        let p = crate::theme::LIGHT;
        let mut labels = String::from("a");
        let shapes = crate::testutil::paint_at2(egui::vec2(800.0, 600.0), |ui| {
            egui::Frame::new().fill(crate::views::dialog_fill(&p)).stroke(crate::views::dialog_stroke(&p)).inner_margin(super::DIALOG_MARGIN).show(ui, |ui| {
                ui.set_width(440.0);
                crate::views::dialog_fields(ui, &p);
                super::title(ui, &p, "Edit CI-3779");
                super::edit_group(ui, &p, "Labels, comma separated", &mut labels, |t| super::field(t, "", &p));
                ui.add_space(super::group_space(ui, "Priority"));
                super::edit_group(ui, &p, "Priority", &mut String::new(), |t| super::field(t, "Hint", &p));
            });
        });
        let fills = crate::testutil::fills(&shapes);
        let dialog = fills.iter().find(|(_, c)| *c == crate::views::dialog_fill(&p)).expect("dialog").0;
        let title = crate::testutil::ink_of(&shapes, "Edit CI-3779");
        assert!((title.top() - (dialog.top() + super::DIALOG_EDGE) - 16.0).abs() <= 0.5, "title {}", title.top() - dialog.top());
        let field = fills.iter().find(|(_, c)| *c == crate::theme::dialog_input_fill(&p)).expect("field").0;
        let gap = crate::testutil::ink_of(&shapes, "Priority").top() - field.bottom();
        assert!((gap - 16.0).abs() <= 0.5, "field → label {gap}");
    }

    #[test]
    fn the_assign_field_sits_12_over_its_list() {
        // G1: the space plus the theme's item spacing is the 12 gap (it was 12 + 6).
        let spacing = 6.0;
        assert_eq!(super::gap_space(super::ASSIGN_FIELD_GAP, spacing) + spacing, 12.0);
        assert_eq!(super::gap_space(4.0, 6.0), 0.0, "never negative");
        crate::testutil::with_ui(|ui| assert_eq!(ui.spacing().item_spacing.y, spacing, "the theme's spacing"));
    }
}
