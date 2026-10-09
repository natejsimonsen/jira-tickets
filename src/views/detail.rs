//! The selected issue: summary, status, description, fields, comments, composer.

use crate::app::{Action, App};
use crate::theme::{self, Palette};
use crate::views::{adf_view, button, outline_pill, pill, primary_button_if};
use egui::{Frame, Margin, RichText, Stroke, Ui, vec2};

pub fn show(app: &mut App, ui: &mut Ui) {
    let p = theme::palette(ui.ctx());
    let Some(key) = app.selected.clone() else {
        // Hidden under the help overlay, where it would poke out beside the panel.
        if app.help_open {
            return;
        }
        ui.set_max_width(ui.available_width() - DETAIL_RIGHT);
        ui.vertical_centered(|ui| {
            ui.add_space(120.0);
            crate::views::empty_state(ui, p, "Select an issue");
        });
        return;
    };
    let Some(issue) = app.store.get(&key).cloned() else {
        crate::views::icons::spinner(ui, crate::views::icons::INLINE_SPINNER, p);
        return;
    };
    let loading = app.detail_loading.contains(&key);
    let saved = crate::views::set_thumb_fills(ui, p);
    // The list's thumb: round-ended, inset, and shown at rest when the pane overflows (K1, C1).
    crate::views::floating_thumb(&mut ui.spacing_mut().scroll);
    // egui's own fade dimmed the last ~19 px; the pane paints the list's short fade instead (below).
    crate::views::no_scroll_fade(ui);
    // The viewport runs to the hint bar; a scrolled line is cut there under the fade, never snapped (E1 ruling).
    let out = egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        crate::views::restore_fills(ui, saved);
        // The pane has no right margin: the bar sits in this inset, clear of the content.
        ui.set_max_width(ui.available_width() - DETAIL_RIGHT);
        // One row the buttons' height, everything centred on it, so the key line sits level with the buttons.
        ui.allocate_ui_with_layout(vec2(ui.available_width(), crate::views::BUTTON_H), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.set_min_height(crate::views::BUTTON_H);
            ui.label(RichText::new(&issue.key).font(theme::mono(12.5)).color(p.muted));
            if let (Some(pk), Some(ps)) = (&issue.parent_key, &issue.parent_summary) {
                ui.label(RichText::new("·").color(p.muted));
                let epic = theme::epic_tint_in(pk, &app.epic_slots, p, &app.cfg.issues.epic_colors);
                // In the epic's text colour, underlined on hover like every other link.
                let tip = super::issues::epic_tip(pk, ps);
                if crate::views::tooltip(crate::views::text_link(ui, &format!("{pk} {ps}"), theme::body(13.0), epic.fg), tip).clicked() {
                    app.actions.push(Action::SetEpic(Some(pk.clone())));
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if crate::views::tooltip(ui.add(button("Open in Jira", p)), "Open in Jira (⌘O)").clicked() {
                    app.actions.push(Action::OpenUrl(app.cfg.browse_url(&issue.key)));
                }
                if crate::views::tooltip(ui.add(button("Copy link", p)), "Copy link (c)").clicked() {
                    app.actions.push(Action::Copy(app.cfg.browse_url(&issue.key)));
                }
                if loading {
                    crate::views::icons::spinner(ui, crate::views::icons::INLINE_SPINNER, p);
                }
            });
        });
        ui.add_space(8.0);
        ui.label(RichText::new(&issue.summary).font(theme::bold(22.0)).color(p.text));
        ui.add_space(TITLE_TO_PILLS);
        ui.horizontal_wrapped(|ui| {
            // The list rows' pill gap (E3; 6 measured 6.5–7).
            ui.spacing_mut().item_spacing = vec2(super::issues::PILL_GAP, 4.0);
            let st = theme::status_tint(&issue.status.name, &issue.status.category, p, &app.cfg.issues.status_colors);
            pill(ui, &issue.status.name, st.bg, st.fg);
            if let crate::filter::TeamScope::Label(l) = &app.filter.team
                && super::issues::missing_label_pill(ui, &app.filter, &issue, l, p)
            {
                ui.label(RichText::new("l applies it").color(p.muted).size(12.0));
            }
            outline_pill(ui, &issue.issue_type, p, crate::views::PILL_MARGIN);
            if !issue.priority.is_empty() {
                outline_pill(ui, &issue.priority, p, crate::views::PILL_MARGIN);
            }
            for l in &issue.labels {
                outline_pill(ui, l, p, crate::views::PILL_MARGIN);
            }
        });
        // The pills' edge is their ink.
        ui.add_space(section_space(ui, "Description", 0.0));
        let window_w = ui.ctx().content_rect().width();
        ui.horizontal_top(|ui| {
            // The gutter is FIELDS_GUTTER exactly: no item spacing either side of it.
            ui.spacing_mut().item_spacing.x = 0.0;
            let (main_w, fields_w) = detail_columns(window_w, ui.available_width());
            ui.allocate_ui_with_layout(vec2(main_w, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.set_width(main_w);
                section_heading(ui, p, "Description");
                match &issue.description {
                    Some(d) => adf_view::show_value(ui, d, p, p.surface),
                    None if loading => {
                        crate::views::icons::spinner(ui, crate::views::icons::INLINE_SPINNER, p);
                    }
                    None => adf_view::show_value(ui, &serde_json::Value::Null, p, p.surface),
                }
                // The description's last line ends at its descenders, the body line's leading under them.
                let body = theme::Ink::of(ui.ctx(), &theme::body(14.0), Some(adf_view::line_height(14.0)));
                let comments = format!("Comments ({})", issue.comments.len());
                ui.add_space(section_space(ui, &comments, body.line - body.base - body.descent));
                section_heading(ui, p, &comments);
                // The comment cards and the composer sit 16 below their label (8 already given).
                ui.add_space(LABEL_TO_BOX - 8.0 - ui.spacing().item_spacing.y);
                for c in &issue.comments {
                    comment_card(ui, p, &c.author, &short_date(&c.created), &c.body);
                    ui.add_space(COMMENT_GAP - ui.spacing().item_spacing.y);
                }
                // After the cards, 16 to the composer (12 + 4); with no cards it sits 16 below the label like one.
                if !issue.comments.is_empty() {
                    ui.add_space(4.0);
                }
                if composer_shown(crate::jira::READ_ONLY.load(std::sync::atomic::Ordering::Relaxed), snapshot_composer()) {
                    composer(app, ui, p, &key);
                } else {
                    // Read-only: one muted line where the box would be; nothing to type into or press. Its ink
                    // starts where a card's edge would.
                    ui.add_space(-NOTE_LIFT);
                    ui.label(RichText::new(READ_ONLY_NOTE).size(14.0).color(p.muted));
                }
            });
            ui.add_space(FIELDS_GUTTER);
            ui.vertical(|ui| {
                ui.set_width(fields_w);
                ui.spacing_mut().item_spacing.y = 0.0;
                field(ui, p, "Assignee", &issue.assignee.as_ref().map(|a| a.display_name.clone()).unwrap_or("Unassigned".into()));
                field(ui, p, "Reporter", &issue.reporter.as_ref().map(|a| a.display_name.clone()).unwrap_or_default());
                field(ui, p, "Sprint", issue.sprint.as_deref().unwrap_or("—"));
                field(ui, p, "Story points", &issue.story_points.map(|n| format!("{n}")).unwrap_or("—".into()));
                field(ui, p, "Due", issue.due_date.as_deref().unwrap_or("—"));
                field(ui, p, "Created", &short_date(&issue.created));
                field(ui, p, "Updated", &short_date(&issue.updated));
            });
        });
        ui.add_space(24.0);
    });
    let view = out.inner_rect;
    if let Some((top, bottom)) = super::issues::detail_fade(view.bottom(), out.content_size.y - out.state.offset.y - view.height(), ui.ctx().pixels_per_point())
    {
        let r = egui::Rect::from_x_y_ranges(view.x_range(), top..=bottom);
        let fade = p.surface.gamma_multiply(super::issues::FADE_STRENGTH);
        ui.painter().add(egui::Shape::gradient_rect(r, egui::epaint::Direction::BottomUp, [fade, egui::Color32::TRANSPARENT]));
    }
}

/// Title → pills.
const TITLE_TO_PILLS: f32 = 12.0;
/// Pills → Description and the description's last line → Comments, measured to the ink (round 13 E5: 24 to
/// the boxes read 34 and 37).
const SECTION_INK_GAP: f32 = 32.0;

/// The space before a section heading `text` for SECTION_INK_GAP between the ink of the block above (ending
/// `below_ink` over its box's bottom) and the heading's ink; the layout's item spacing already follows the block.
fn section_space(ui: &Ui, text: &str, below_ink: f32) -> f32 {
    let top = theme::ink_rect(&ui.painter().layout_no_wrap(text.to_string(), theme::bold(12.0), egui::Color32::WHITE)).top();
    section_gap_space(below_ink, top, ui.spacing().item_spacing.y)
}

/// SECTION_INK_GAP less the ink's leading either side and the item spacing.
pub fn section_gap_space(below_ink: f32, heading_ink_top: f32, item_spacing_y: f32) -> f32 {
    SECTION_INK_GAP - below_ink - heading_ink_top - item_spacing_y
}
/// Section label → the comment cards or composer box.
const LABEL_TO_BOX: f32 = 16.0;
/// Between comment cards: the cards' own 12 px padding.
const COMMENT_GAP: f32 = 12.0;
/// The composer's fixed height.
const COMPOSER_H: f32 = 72.0;
/// The composer's inner margin: with its 1 px stroke, text sits 12 in, on a comment card's text edge.
const COMPOSER_PAD: f32 = 11.0;
/// The composer's stroke.
const COMPOSER_STROKE: f32 = 1.0;
/// How far the composer's text origin sits below an even margin (round 13 F3: its text read 1 higher than a card's).
const COMPOSER_DROP: i8 = 1;

/// The composer's inner margin: COMPOSER_PAD, its text COMPOSER_DROP lower; the box keeps COMPOSER_H.
pub fn composer_margin() -> Margin {
    let pad = COMPOSER_PAD as i8;
    Margin { left: pad, right: pad, top: pad + COMPOSER_DROP, bottom: pad - COMPOSER_DROP }
}
/// Comment author → date.
const COMMENT_META_GAP: f32 = 8.0;
/// The detail content's right inset, inside the scroll area: the pane's 16 px gutter, which holds the floating
/// scrollbar (its width + 4 at least).
pub const DETAIL_RIGHT: f32 = 16.0;

/// The read-only note's line box carries this much more above its ink than a card's edge does (round 8
/// measured label → note 21 vs label → first card 17.5), so the note moves up by it.
const NOTE_LIFT: f32 = 3.5;

/// What read-only mode shows instead of the composer.
pub const READ_ONLY_NOTE: &str = "Read-only, comments can't be posted";

/// A comment card's padding, measured to the ink on every side (round 13 E2: a 12 margin read 16–16.5 to the ink
/// above and below).
pub const CARD_PAD: f32 = 12.0;

/// One comment: surface_2, fill only (like code blocks), in both themes; code inside steps off it
/// (`chip_fill_on`). CARD_PAD to the header's ink above and to the body's descenders below; the sides keep the
/// 12 margin (the text's own bearing reads 12, round 13).
pub fn comment_card(ui: &mut Ui, p: &Palette, author: &str, date: &str, body: &serde_json::Value) {
    let below = theme::Ink::of(ui.ctx(), &theme::body(14.0), Some(adf_view::line_height(14.0))).pad_deep(CARD_PAD);
    let pad = CARD_PAD as i8;
    Frame::new().fill(p.surface_2).corner_radius(theme::RADIUS).inner_margin(Margin { left: pad, right: pad, top: 0, bottom: 0 }).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let header = header_galleys(ui, p, author, date);
        ui.add_space(CARD_PAD - header.ink_top);
        comment_header_row(ui, header);
        ui.add_space(4.0);
        adf_view::show_value(ui, body, p, p.surface_2);
        // The layout's item spacing already follows the body.
        ui.add_space(below - ui.spacing().item_spacing.y);
    });
}

/// A laid-out comment header: author and date galleys, the date's drop onto the author's baseline, the item
/// spacing that puts COMMENT_META_GAP between their ink, and the row's topmost ink.
#[derive(Clone)]
pub struct HeaderGalleys {
    author: std::sync::Arc<egui::Galley>,
    date: std::sync::Arc<egui::Galley>,
    drop: f32,
    gap: f32,
    ink_top: f32,
}

fn header_galleys(ui: &Ui, p: &Palette, author: &str, date: &str) -> HeaderGalleys {
    let author = ui.painter().layout_no_wrap(author.to_string(), theme::bold(13.5), p.text);
    let date = ui.painter().layout_no_wrap(date.to_string(), theme::body(12.0), p.muted);
    let drop = crate::views::galley_baseline(&author) - crate::views::galley_baseline(&date);
    let gap = theme::ink_gap(&author, &date, COMMENT_META_GAP);
    let ink_top = theme::ink_rect(&author).top().min(drop + theme::ink_rect(&date).top());
    HeaderGalleys { author, date, drop, gap, ink_top }
}

/// A comment card's header: the author in bold, COMMENT_META_GAP to the ink, the date muted on its baseline.
/// Returns both labels' rects.
pub fn comment_header(ui: &mut Ui, p: &Palette, author: &str, date: &str) -> (egui::Rect, egui::Rect) {
    let header = header_galleys(ui, p, author, date);
    comment_header_row(ui, header)
}

fn comment_header_row(ui: &mut Ui, h: HeaderGalleys) -> (egui::Rect, egui::Rect) {
    ui.horizontal_top(|ui| {
        // Set here: the detail pane zeroes item spacing for its column gutter, and this row inherits it.
        ui.spacing_mut().item_spacing.x = h.gap;
        let a = ui.add(egui::Label::new(h.author)).rect;
        let d = ui
            .vertical(|ui| {
                ui.add_space(h.drop);
                ui.add(egui::Label::new(h.date)).rect
            })
            .inner;
        (a, d)
    })
    .inner
}

/// Whether the composer box and Comment button are drawn: not in read-only mode, unless the snapshot-only
/// override asks for the writable UI (writes stay blocked by READ_ONLY either way).
pub fn composer_shown(read_only: bool, snapshot_override: bool) -> bool {
    !read_only || snapshot_override
}

/// Captures only: `JIRA_TICKETS_SNAPSHOT_COMPOSER=1` in a `snapshot` build draws the writable composer so it
/// can be rated, while every write is still refused by READ_ONLY. Always false in a normal build.
pub fn snapshot_composer() -> bool {
    #[cfg(feature = "snapshot")]
    {
        static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *ON.get_or_init(|| std::env::var("JIRA_TICKETS_SNAPSHOT_COMPOSER").as_deref() == Ok("1"))
    }
    #[cfg(not(feature = "snapshot"))]
    {
        false
    }
}

/// Writes are blocked and the composer is not drawn: the hint bar offers no composer keys.
pub fn composer_read_only() -> bool {
    !composer_shown(crate::jira::READ_ONLY.load(std::sync::atomic::Ordering::Relaxed), snapshot_composer())
}

/// A section heading; the space above it comes from `section_space`, 8 below.
fn section_heading(ui: &mut Ui, p: &Palette, text: &str) {
    ui.label(RichText::new(text).font(theme::bold(12.0)).color(p.muted));
    ui.add_space(8.0);
}

/// The composer's text field id for an issue.
pub fn composer_id(key: &str) -> egui::Id {
    egui::Id::new(("composer", key))
}

/// Always-visible comment box (border, accent when focused) with the Comment button right-aligned under it.
fn composer(app: &mut App, ui: &mut Ui, p: &Palette, key: &str) {
    let id = composer_id(key);
    let focused = ui.memory(|m| m.has_focus(id));
    let draft = app.drafts.entry(key.to_string()).or_default();
    let resp = Frame::new()
        // The one input rule: a step under the detail pane's surface.
        .fill(theme::input_fill(p.surface, p))
        .stroke(Stroke::new(COMPOSER_STROKE, if focused { p.accent } else { p.border }))
        .corner_radius(theme::RADIUS)
        // 11 + the 1 px stroke: the comment cards' 12 to the text at the sides; the text 1 lower (COMPOSER_DROP).
        .inner_margin(composer_margin())
        .show(ui, |ui| ui.add_sized([ui.available_width(), COMPOSER_H - 2.0 * (COMPOSER_PAD + COMPOSER_STROKE)], composer_edit(draft, p).id(id)))
        .inner;
    // Clicking the frame's padding focuses the box too.
    let enabled = comment_enabled(draft, &app.posting);
    let post = resp.has_focus() && ui.input_mut(|i| i.consume_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Enter)));
    // 12 px box → button, counting the layout's own item spacing.
    ui.add_space(BOX_TO_BUTTON - ui.spacing().item_spacing.y);
    ui.allocate_ui_with_layout(vec2(ui.available_width(), crate::views::BUTTON_H), egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        if crate::views::hover_tip_beside(ui.add(primary_button_if("Comment", enabled, p)), COMMENT_TIP, app).clicked() || (post && enabled) {
            app.actions.push(Action::Comment);
        }
        if app.posting.is_some() {
            crate::views::icons::spinner(ui, crate::views::icons::INLINE_SPINNER, p);
        }
    });
}

/// The composer's text field. The empty-field caret rule (I1) lives in the frame's margin: a TextEdit given its own
/// `frame` ignores `.margin()`, which is how the composer kept round 10's placeholder 2 right of the typed text
/// (round 12 F1). Caret CARET_LEAD left of the origin, placeholder and typed text on it.
pub fn composer_edit<'a>(draft: &'a mut String, p: &Palette) -> egui::TextEdit<'a> {
    let empty = draft.is_empty();
    egui::TextEdit::multiline(draft)
        .frame(Frame::new().inner_margin(crate::views::caret_margin(Margin::ZERO, empty)))
        .hint_text(crate::views::empty_field_hint(COMPOSER_HINT, theme::body(14.0), p))
}

/// The Comment button's tooltip.
pub const COMMENT_TIP: &str = "⌘Enter";

/// The composer's placeholder.
pub const COMPOSER_HINT: &str = "Add a comment (⌘Enter to post)";

/// Label, 4, value, then 16 to the next group.
fn field(ui: &mut Ui, p: &Palette, name: &str, value: &str) {
    ui.label(RichText::new(name).font(theme::bold(12.0)).color(p.muted));
    ui.add_space(4.0);
    ui.label(RichText::new(if value.is_empty() { "—" } else { value }).size(14.0).color(p.text));
    ui.add_space(16.0);
}

/// Gap between the description column and the fields column (24; it measured 33 with item spacing on top).
const FIELDS_GUTTER: f32 = 24.0;

/// The description column's widest measure (about 75 characters of 14 px text).
const MAIN_MAX: f32 = 520.0;

/// (main column, fields column) widths: fields are at least 220 wide, or 132 below a 1200 px window so the
/// description keeps its measure; the main column takes the rest (300 to MAIN_MAX), and any room past
/// MAIN_MAX goes to the fields column.
pub fn detail_columns(window_w: f32, available: f32) -> (f32, f32) {
    let fields_min = if window_w < 1200.0 { NARROW_FIELDS } else { 220.0 };
    let main = (available - fields_min - FIELDS_GUTTER).clamp(300.0, MAIN_MAX);
    (main, (available - main - FIELDS_GUTTER).max(fields_min))
}

/// The fields column below a 1200 px window: 132, so the description keeps 400 with the 24 gutter (K1).
pub const NARROW_FIELDS: f32 = 132.0;

/// Composer box → Comment button.
const BOX_TO_BUTTON: f32 = 12.0;

/// The Comment button and ⌘Enter work when the draft has text and no post is in flight.
pub fn comment_enabled(draft: &str, posting: &Option<String>) -> bool {
    !draft.trim().is_empty() && posting.is_none()
}

/// "2026-10-03T09:00:00.000-0700" → "2026-10-03 09:00"
pub fn short_date(s: &str) -> String {
    match s.get(..10).zip(s.get(11..16)) {
        Some((day, time)) => format!("{day} {time}"),
        None => s.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_column_narrows_below_1200() {
        assert_eq!(detail_columns(1440.0, 800.0), (520.0, 256.0), "main capped; the rest to the fields");
        assert_eq!(detail_columns(1000.0, 580.0), (424.0, 132.0));
        // The narrow pane's reserve holds a 400 description beside the 132 fields and the 24 gutter.
        assert_eq!(detail_columns(1000.0, crate::views::issues::NARROW_DETAIL_MIN - 32.0), (400.0, NARROW_FIELDS));
        assert_eq!(detail_columns(1000.0, 300.0).0, 300.0, "floor");
        assert_eq!(detail_columns(2560.0, 1800.0).0, MAIN_MAX);
    }

    #[test]
    fn read_only_draws_no_composer() {
        assert!(!composer_shown(true, false), "read-only: only the note");
        assert!(composer_shown(false, false));
        assert!(composer_shown(true, true), "the snapshot override draws the writable UI");
        assert!(!snapshot_composer() || cfg!(feature = "snapshot"), "never on outside snapshot builds");
        assert_eq!(READ_ONLY_NOTE, "Read-only, comments can't be posted");
        assert_eq!(21.0 - NOTE_LIFT, 17.5, "the note's ink sits where the first card's edge does");
        let mut bar = egui::style::ScrollStyle::floating();
        crate::views::floating_thumb(&mut bar);
        assert!(DETAIL_RIGHT >= bar.bar_width + bar.bar_outer_margin + 4.0, "the scrollbar sits clear of the content");
        assert_eq!((COMMENT_TIP, COMPOSER_HINT), ("⌘Enter", "Add a comment (⌘Enter to post)"));
        assert_eq!(COMPOSER_PAD + COMPOSER_STROKE, 12.0, "text on the comment cards' 12 px edge");
        let m = composer_margin();
        assert_eq!((m.top - m.bottom, m.top + m.bottom), (2, 22), "F3: the text 1 lower, the box height kept");
        assert_eq!(super::super::issues::PILL_GAP, 8.0, "the detail pill row takes the list rows' gap (E3)");
    }

    #[test]
    fn comment_cards_pad_12_to_the_ink() {
        // Round 13 E2 / E4: 12 from the card's edge to the header's ink above and to the body's descenders below,
        // 12 at the sides; the author's ink 8 from the date's, both on one baseline. Real fonts, captures' density.
        let p = crate::theme::LIGHT;
        let body =
            serde_json::json!({"type": "doc", "content": [{"type": "paragraph", "content": [{"type": "text", "text": "Looks good, shipping it today"}]}]});
        let shapes = crate::testutil::paint_at2(egui::vec2(800.0, 400.0), |ui| {
            ui.set_width(400.0);
            comment_card(ui, &p, "Ada Lovelace", "2026-07-29 13:28", &body);
        });
        let (card, _) = *crate::testutil::fills(&shapes).iter().find(|(_, c)| *c == p.surface_2).expect("the card");
        let (a, d) = (crate::testutil::ink_of(&shapes, "Ada Lovelace"), crate::testutil::ink_of(&shapes, "2026-07-29 13:28"));
        let b = crate::testutil::ink_of(&shapes, "Looks good, shipping it today");
        let near = |v: f32, want: f32| (v - want).abs() <= 0.5;
        assert!(near(a.top().min(d.top()) - card.top(), CARD_PAD), "top {}", a.top().min(d.top()) - card.top());
        assert!(near(card.bottom() - b.bottom(), CARD_PAD), "bottom {}", card.bottom() - b.bottom());
        assert!(near(a.left() - card.left(), CARD_PAD), "left {}", a.left() - card.left());
        assert!(near(d.left() - a.right(), COMMENT_META_GAP), "author · date {}", d.left() - a.right());
        // Neither has descenders, so the ink bottoms are the baselines.
        assert!(near(a.bottom(), d.bottom()), "one baseline: {} vs {}", a.bottom(), d.bottom());
    }

    #[test]
    fn sections_sit_32_below_the_ink_above() {
        // Round 13 E5: the pills' edge and the description's descenders each sit 32 above the heading's ink, with
        // the real fonts (a block, the item spacing, the space, then the heading).
        let p = crate::theme::LIGHT;
        let marker = egui::Color32::from_rgb(1, 2, 3);
        for below in ["pills", "text"] {
            let shapes = crate::testutil::paint_at2(egui::vec2(800.0, 400.0), |ui| {
                if below == "pills" {
                    crate::views::pill(ui, "New", marker, p.text);
                    ui.add_space(section_space(ui, "Description", 0.0));
                } else {
                    let body = theme::Ink::of(ui.ctx(), &theme::body(14.0), Some(adf_view::line_height(14.0)));
                    let v = serde_json::json!({"type": "doc", "content": [{"type": "paragraph", "content": [{"type": "text", "text": "a gpqy line"}]}]});
                    ui.scope(|ui| adf_view::show_value(ui, &v, &p, p.surface));
                    ui.add_space(section_space(ui, "Description", body.line - body.base - body.descent));
                }
                section_heading(ui, &p, "Description");
            });
            let above = if below == "pills" {
                crate::testutil::fills(&shapes).iter().find(|(_, c)| *c == marker).expect("pill").0.bottom()
            } else {
                crate::testutil::ink_of(&shapes, "a gpqy line").bottom()
            };
            let gap = crate::testutil::ink_of(&shapes, "Description").top() - above;
            assert!((gap - SECTION_INK_GAP).abs() <= 0.5, "{below}: {gap}");
        }
    }

    #[test]
    fn comment_author_and_date_stand_apart() {
        // Regression (round 7): "Alan Turing2026-07-29 13:28" — the pane's zero item spacing leaked in.
        let (a, d) = crate::testutil::with_ui(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            comment_header(ui, &crate::theme::LIGHT, "Alan Turing", "2026-07-29 13:28")
        });
        assert!(d.left() - a.right() >= 8.0, "gap {}", d.left() - a.right());
    }

    #[test]
    fn comment_enabled_needs_text_and_idle() {
        assert!(comment_enabled("hi", &None));
        assert!(!comment_enabled("  \n ", &None));
        assert!(!comment_enabled("", &None));
        assert!(!comment_enabled("hi", &Some("ABC-1".into())));
    }

    #[test]
    fn short_date_formats_and_never_panics() {
        assert_eq!(short_date("2026-10-03T09:00:00.000-0700"), "2026-10-03 09:00");
        assert_eq!(short_date("2026-10"), "2026-10");
        // Byte 10 and 11 fall inside multi-byte chars; slicing would panic.
        let odd = "2026-10-0éé:00:00";
        assert_eq!(short_date(odd), odd);
        assert_eq!(short_date("ééééééééééééé"), "ééééééééééééé");
    }
}
