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
        // Centred by its ink on the pane's own rect (round 14 K2: centred on the content column it read 8 right).
        let content = ui.max_rect();
        let (left, right) = app.detail_x.unwrap_or((content.left(), content.right()));
        let g = ui.painter().layout_no_wrap(EMPTY_DETAIL.to_string(), theme::body(crate::views::EMPTY_STATE_SIZE), p.muted);
        let x = crate::views::issues::snap_px(empty_detail_x(left, right, theme::ink_rect(&g).center().x), ui.ctx().pixels_per_point());
        ui.painter().galley(egui::pos2(x, content.top() + EMPTY_DETAIL_TOP), g, p.muted);
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
    let track = crate::views::thumb_track(ui.available_rect_before_wrap());
    let out = egui::ScrollArea::vertical().auto_shrink([false, false]).scroll_bar_rect(track).show(ui, |ui| {
        crate::views::restore_fills(ui, saved);
        // The pane has no right margin: the bar sits in this inset, clear of the content.
        ui.set_max_width(ui.available_width() - DETAIL_RIGHT);
        // One row the buttons' height, everything centred on it, so the key line sits level with the buttons.
        let key_ink = ui.allocate_ui_with_layout(vec2(ui.available_width(), crate::views::BUTTON_H), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.set_min_height(crate::views::BUTTON_H);
            let r = ui.label(RichText::new(&issue.key).font(theme::mono(12.5)).color(p.muted));
            let mut lowest = ink_bottom_at(ui, r.rect.top(), &issue.key, &theme::mono(12.5));
            if let (Some(pk), Some(ps)) = (&issue.parent_key, &issue.parent_summary) {
                ui.label(RichText::new("·").color(p.muted));
                let epic = theme::epic_tint_in(pk, &app.epic_slots, p, &app.cfg.issues.epic_colors);
                // In the epic's text colour, underlined on hover like every other link.
                let tip = super::issues::epic_tip(pk, ps);
                let text = format!("{pk} {ps}");
                let link = crate::views::text_link(ui, &text, theme::body(13.0), epic.fg);
                lowest = lowest.max(ink_bottom_at(ui, link.rect.top(), &text, &theme::body(13.0)));
                if crate::views::tooltip(link, tip).clicked() {
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
            lowest
        });
        title_block(ui, p, key_ink.inner, &issue.summary);
        let pills = ui.horizontal_wrapped(|ui| {
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
        // Only a pill row inside the viewport anchors a picker (a scrolled-away one would lift it off the window).
        let pills_top = pills.response.rect.top();
        if ui.clip_rect().y_range().contains(pills_top) {
            app.detail_pills_top = Some(pills_top);
        }
        // The pills' edge is their ink.
        ui.add_space(section_space(ui, "Description", 0.0));
        let window_w = ui.ctx().content_rect().width();
        ui.horizontal_top(|ui| {
            // The gutter is FIELDS_GUTTER exactly: no item spacing either side of it.
            ui.spacing_mut().item_spacing.x = 0.0;
            let (main_w, fields_w) = detail_columns(window_w, ui.available_width());
            ui.allocate_ui_with_layout(vec2(main_w, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.set_width(main_w);
                let below = section_heading(ui, p, "Description");
                let first = issue
                    .description
                    .as_ref()
                    .map_or_else(|| adf_view::first_ink_top(ui.ctx(), &serde_json::Value::Null), |d| adf_view::first_ink_top(ui.ctx(), d));
                ui.add_space(label_space(ui, below, first));
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
                let below = section_heading(ui, p, &comments);
                let composing = composer_shown(crate::jira::READ_ONLY.load(std::sync::atomic::Ordering::Relaxed), snapshot_composer());
                // A card's or the composer's edge is its ink; the read-only note's is its text's.
                let first = if !issue.comments.is_empty() || composing { 0.0 } else { theme::Ink::of(ui.ctx(), &theme::body(14.0), None).top };
                ui.add_space(label_space(ui, below, first));
                for c in &issue.comments {
                    comment_card(ui, p, &c.author, &short_date(&c.created), &c.body);
                    // Between comment cards: the cards' own padding.
                    ui.add_space(theme::spacing(ui.ctx()).card_pad - ui.spacing().item_spacing.y);
                }
                // After the cards, 16 to the composer (12 + 4); with no cards it sits 16 below the label like one.
                if !issue.comments.is_empty() {
                    ui.add_space(4.0);
                }
                if composing {
                    composer(app, ui, p, &key);
                } else {
                    // Read-only: one muted line where the box would be; nothing to type into or press. Its ink
                    // starts where a card's edge would.
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

/// The detail pane's empty state, EMPTY_DETAIL_TOP under the pane's content top.
pub const EMPTY_DETAIL: &str = "Select an issue";
const EMPTY_DETAIL_TOP: f32 = 120.0;

/// The empty state's galley x for its ink centre (`ink_centre`, from the galley's origin) on the pane's centre.
pub fn empty_detail_x(pane_left: f32, pane_right: f32, ink_centre: f32) -> f32 {
    (pane_left + pane_right) / 2.0 - ink_centre
}

/// The key row's lowest ink (the epic link's descenders when it shows, else the key) → the title's ink (round 15
/// E2: from the key's baseline it read 20.5 under a link and 23.5 under a bare key).
const KEY_TO_TITLE: f32 = 24.0;
/// The title's last line's ink → the pills' edge, the same with or without descenders (round 15 E7: 12 from the
/// line box read 19.5 and 23). 24, not 16: a picker's dialog sits 8 over the pills and must stay 12 under the
/// title's descenders (16 left it 8).
const TITLE_TO_PILLS: f32 = 24.0;

/// `v` on the device-pixel grid, as the tessellator places a galley.
fn snap(ui: &Ui, v: f32) -> f32 {
    let ppp = ui.ctx().pixels_per_point();
    (v * ppp).round() / ppp
}

/// The ink bottom of `text` in `font` painted with its line box at `top`.
fn ink_bottom_at(ui: &Ui, top: f32, text: &str, font: &egui::FontId) -> f32 {
    let g = ui.painter().layout_no_wrap(text.to_string(), font.clone(), egui::Color32::WHITE);
    snap(ui, top) + theme::ink_rect(&g).bottom()
}

/// The ink bottom of a galley's last row, from the galley's origin (pixel-snapped like `theme::ink_rect`).
pub fn last_row_ink_bottom(g: &egui::Galley) -> f32 {
    let ppp = g.pixels_per_point.max(1.0);
    let Some(row) = g.rows.last() else { return 0.0 };
    row.glyphs
        .iter()
        .filter(|gl| !gl.uv_rect.is_nothing() && gl.uv_rect.size.y > 0.0)
        .map(|gl| row.pos.y + ((gl.pos.y + gl.uv_rect.offset.y) * ppp).round() / ppp + gl.uv_rect.size.y)
        .fold(0.0, f32::max)
}

/// The 22 pt title KEY_TO_TITLE under the key row's lowest ink (`key_ink`), then the space for TITLE_TO_PILLS
/// from its last line's ink to the pill row. Both are set from the laid-out ink, so every issue measures the same.
fn title_block(ui: &mut Ui, p: &Palette, key_ink: f32, summary: &str) {
    let g = ui.painter().layout(summary.to_string(), theme::bold(22.0), p.text, ui.available_width());
    let top = snap(ui, key_ink + KEY_TO_TITLE - theme::ink_rect(&g).top());
    ui.add_space(top - ui.cursor().top());
    let foot = last_row_ink_bottom(&g);
    let r = ui.add(egui::Label::new(g));
    let pills = snap(ui, snap(ui, r.rect.top()) + foot + TITLE_TO_PILLS);
    ui.add_space(pills - ui.cursor().top());
}
/// The space before a section heading `text` for the density's `section_gap` (pills → Description and the
/// description's last line → Comments, measured to the ink; 32 in Compact, round 13 E5: 24 to the boxes read 34
/// and 37) between the ink of the block above (ending `below_ink` over its box's bottom) and the heading's ink;
/// the layout's item spacing already follows the block.
fn section_space(ui: &Ui, text: &str, below_ink: f32) -> f32 {
    let top = theme::ink_rect(&ui.painter().layout_no_wrap(text.to_string(), theme::bold(12.0), egui::Color32::WHITE)).top();
    section_gap_space(theme::spacing(ui.ctx()).section_gap, below_ink, top, ui.spacing().item_spacing.y)
}

/// The section `gap` less the ink's leading either side and the item spacing.
pub fn section_gap_space(gap: f32, below_ink: f32, heading_ink_top: f32, item_spacing_y: f32) -> f32 {
    gap - below_ink - heading_ink_top - item_spacing_y
}
/// A section label's ink → the first ink under it: the description's first line, a comment card's or the
/// composer's edge, the read-only note (round 15 E6/F2: 8 to the label's box read 18.5 and 17).
const LABEL_INK_GAP: f32 = 16.0;

/// The space after a section label whose box runs `below` past its ink for LABEL_INK_GAP to content whose ink
/// starts `first` into its box; the layout's item spacing comes on top.
fn label_space(ui: &Ui, below: f32, first: f32) -> f32 {
    LABEL_INK_GAP - below - first - ui.spacing().item_spacing.y
}
/// The composer's fixed height.
const COMPOSER_H: f32 = 72.0;
/// The composer's inner margin: with its 1 px stroke, text sits 12 in, on a comment card's text edge.
const COMPOSER_PAD: f32 = 11.0;
/// The composer's stroke.
const COMPOSER_STROKE: f32 = 1.0;
/// The composer box's height as drawn: COMPOSER_H holds the field's four-row minimum, so the box is 90 (round 14
/// F1 keeps it).
#[cfg(test)]
const COMPOSER_BOX_H: f32 = 90.0;
/// The composer's border to its text's ink, above: a Compact comment card's padding (round 14 F1: 16.5 read to the line
/// box's 12).
const COMPOSER_INK_TOP: f32 = 12.0;

/// The composer's inner margin and the extra space above its text for a 14 pt line whose ink starts `ink_top`
/// into its line box: COMPOSER_INK_TOP from the border's outer edge to the ink above, COMPOSER_PAD at the sides,
/// the box's height unchanged (the bottom margin takes what the top gave up). Margins are whole points, so the
/// fraction is returned as space to add before the text; one point of the bottom margin is space after it, so the
/// two spaces make up one whole point.
pub fn composer_margin(ink_top: f32) -> (Margin, f32) {
    let pad = COMPOSER_PAD as i8;
    let top = (COMPOSER_INK_TOP - COMPOSER_STROKE - ink_top).max(0.0);
    let whole = top.floor();
    let bottom = (2.0 * COMPOSER_PAD - whole - 1.0) as i8;
    (Margin { left: pad, right: pad, top: whole as i8, bottom }, top - whole)
}
/// Comment author → date.
const COMMENT_META_GAP: f32 = 8.0;
/// The detail content's right inset, inside the scroll area: the pane's 16 px gutter, which holds the floating
/// scrollbar (its width + 4 at least).
pub const DETAIL_RIGHT: f32 = 16.0;

/// What read-only mode shows instead of the composer.
pub const READ_ONLY_NOTE: &str = "Read-only, comments can't be posted";

/// One comment: surface_2, fill only (like code blocks), in both themes; code inside steps off it
/// (`chip_fill_on`). The density's `card_pad` (12 in Compact), measured to the ink on every side (round 13 E2: a
/// 12 margin read 16–16.5 to the ink above and below): to the header's ink above and to the body's descenders
/// below; the sides keep the margin (the text's own bearing reads 12, round 13).
pub fn comment_card(ui: &mut Ui, p: &Palette, author: &str, date: &str, body: &serde_json::Value) {
    let card_pad = theme::spacing(ui.ctx()).card_pad;
    let below = theme::Ink::of(ui.ctx(), &theme::body(14.0), Some(adf_view::line_height(14.0))).pad_deep(card_pad);
    let pad = card_pad as i8;
    Frame::new().fill(p.surface_2).corner_radius(theme::RADIUS).inner_margin(Margin { left: pad, right: pad, top: 0, bottom: 0 }).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let header = header_galleys(ui, p, author, date);
        ui.add_space(card_pad - header.ink_top);
        let under = header.below_ink();
        comment_header_row(ui, header);
        // HEADER_TO_BODY from the header's lowest ink to the body's first ink.
        let body_top = adf_view::first_ink_top(ui.ctx(), body);
        ui.add_space(HEADER_TO_BODY - under - body_top - ui.spacing().item_spacing.y);
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
#[cfg(test)]
pub fn comment_header(ui: &mut Ui, p: &Palette, author: &str, date: &str) -> (egui::Rect, egui::Rect) {
    let header = header_galleys(ui, p, author, date);
    comment_header_row(ui, header)
}

impl HeaderGalleys {
    /// How far the header row's box runs below its lowest ink (the author's descenders, else the baseline).
    fn below_ink(&self) -> f32 {
        let row = self.author.size().y.max(self.drop + self.date.size().y);
        row - theme::ink_rect(&self.author).bottom().max(self.drop + theme::ink_rect(&self.date).bottom())
    }
}

/// A comment header's lowest ink → the body's first ink (round 15 E3).
const HEADER_TO_BODY: f32 = 12.0;

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

/// A section heading; the space above it comes from `section_space`, the space below from `label_space`.
/// Returns how far its box runs below its ink (its descenders or parentheses).
fn section_heading(ui: &mut Ui, p: &Palette, text: &str) -> f32 {
    let g = ui.painter().layout_no_wrap(text.to_string(), theme::bold(12.0), p.muted);
    let below = g.size().y - theme::ink_rect(&g).bottom();
    ui.add(egui::Label::new(g));
    below
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
    let resp = composer_box(ui, p, draft, id, focused);
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

/// The composer's box: the input fill, its border (accent when `focused`), COMPOSER_H tall, the field inside.
pub fn composer_box(ui: &mut Ui, p: &Palette, draft: &mut String, id: egui::Id, focused: bool) -> egui::Response {
    let (margin, lead) = composer_margin(theme::Ink::of(ui.ctx(), &theme::body(14.0), None).top);
    Frame::new()
        // The one input rule: a step under the detail pane's surface.
        .fill(theme::input_fill(p.surface, p))
        .stroke(Stroke::new(COMPOSER_STROKE, if focused { p.accent } else { p.border }))
        .corner_radius(theme::RADIUS)
        // 11 + the 1 px stroke: the comment cards' 12 to the text at the sides; 12 to the ink above.
        .inner_margin(margin)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.add_space(lead);
            let size = vec2(ui.available_width(), COMPOSER_H - 2.0 * (COMPOSER_PAD + COMPOSER_STROKE) - 1.0);
            let r = crate::views::add_field(ui, Some(size), draft, |t| composer_edit(t, p).id(id));
            ui.add_space(1.0 - lead);
            r
        })
        .inner
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

/// The narrowest the detail pane gets beside a dragged list: its 16 + 16 margins and `detail_columns`' floor (a
/// 300 description column, the gutter and the fields column's minimum), so neither column overflows the pane.
pub fn min_pane_width(window_w: f32) -> f32 {
    let fields_min = if window_w < 1200.0 { NARROW_FIELDS } else { 220.0 };
    32.0 + 300.0 + FIELDS_GUTTER + fields_min
}

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
    fn min_pane_holds_both_columns() {
        for w in [1000.0, 1440.0] {
            let (main, fields) = detail_columns(w, min_pane_width(w) - 32.0);
            assert_eq!(main + FIELDS_GUTTER + fields, min_pane_width(w) - 32.0, "window {w}");
        }
    }

    #[test]
    fn read_only_draws_no_composer() {
        assert!(!composer_shown(true, false), "read-only: only the note");
        assert!(composer_shown(false, false));
        assert!(composer_shown(true, true), "the snapshot override draws the writable UI");
        assert!(!snapshot_composer() || cfg!(feature = "snapshot"), "never on outside snapshot builds");
        assert_eq!(READ_ONLY_NOTE, "Read-only, comments can't be posted");
        let mut bar = egui::style::ScrollStyle::floating();
        crate::views::floating_thumb(&mut bar);
        assert!(DETAIL_RIGHT >= bar.bar_width + bar.bar_outer_margin + 4.0, "the scrollbar sits clear of the content");
        assert_eq!((COMMENT_TIP, COMPOSER_HINT), ("⌘Enter", "Add a comment (⌘Enter to post)"));
        assert_eq!(COMPOSER_PAD + COMPOSER_STROKE, 12.0, "text on the comment cards' 12 px edge");
        let (m, lead) = composer_margin(3.5);
        assert_eq!((m.top, lead, m.bottom), (7, 0.5, 14), "F1: 12 to the ink above, the box height kept");
        assert_eq!(super::super::issues::PILL_GAP, 8.0, "the detail pill row takes the list rows' gap (E3)");
    }

    #[test]
    #[cfg_attr(not(target_os = "macos"), ignore = "glyph-ink metrics are calibrated on macOS font rasterisation")]
    fn the_composer_insets_its_text_like_a_card() {
        // Round 14 F1: 12 from the box's outer edge to the first line's ink, typed or placeholder; the box keeps
        // COMPOSER_H.
        for p in [crate::theme::LIGHT, crate::theme::DARK] {
            for draft in ["Looks good, shipping it today", ""] {
                let mut d = draft.to_string();
                let shapes = crate::testutil::paint_at2(egui::vec2(800.0, 400.0), |ui| {
                    ui.set_width(400.0);
                    composer_box(ui, &p, &mut d, egui::Id::new("composer-test"), false);
                });
                let fill = crate::theme::input_fill(p.surface, &p);
                let (r, _) = *crate::testutil::fills(&shapes).iter().find(|(_, c)| *c == fill).expect("the box");
                let text = if draft.is_empty() { COMPOSER_HINT } else { draft };
                let ink = crate::testutil::ink_of(&shapes, text);
                // The frame paints its stroke inside its rect, so the rect's top is the border's outer edge.
                assert!((ink.top() - r.top() - COMPOSER_INK_TOP).abs() < 0.01, "{draft:?}: {}", ink.top() - r.top());
                assert_eq!(r.height(), COMPOSER_BOX_H, "the box keeps its height");
                assert!((ink.left() - r.left() - 12.0).abs() <= 0.5, "left {}", ink.left() - r.left());
            }
        }
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
        assert!(near(a.top().min(d.top()) - card.top(), theme::COMPACT.card_pad), "top {}", a.top().min(d.top()) - card.top());
        assert!(near(card.bottom() - b.bottom(), theme::COMPACT.card_pad), "bottom {}", card.bottom() - b.bottom());
        assert!(near(a.left() - card.left(), theme::COMPACT.card_pad), "left {}", a.left() - card.left());
        assert!(near(d.left() - a.right(), COMMENT_META_GAP), "author · date {}", d.left() - a.right());
        // Neither has descenders, so the ink bottoms are the baselines.
        assert!(near(a.bottom(), d.bottom()), "one baseline: {} vs {}", a.bottom(), d.bottom());
    }

    #[test]
    fn the_empty_detail_centres_on_the_pane() {
        assert_eq!(empty_detail_x(604.0, 1440.0, 50.0), 972.0, "the ink's centre on the pane's");
    }

    #[test]
    fn comment_bodies_sit_12_under_their_header() {
        // Round 14 E2: the header's lowest ink → the body's first ink, 12.
        let p = crate::theme::LIGHT;
        let body = serde_json::json!({"type": "doc", "content": [{"type": "paragraph", "content": [{"type": "text", "text": "Looks good"}]}]});
        let shapes = crate::testutil::paint_at2(egui::vec2(800.0, 400.0), |ui| {
            ui.set_width(400.0);
            comment_card(ui, &p, "Ada Lovelace", "2026-07-29 13:28", &body);
        });
        let (a, b) = (crate::testutil::ink_of(&shapes, "Ada Lovelace"), crate::testutil::ink_of(&shapes, "Looks good"));
        assert!((b.top() - a.bottom() - HEADER_TO_BODY).abs() <= 0.5, "header → body {}", b.top() - a.bottom());
        // Round 15 E3: an author with descenders measures from them.
        let shapes = crate::testutil::paint_at2(egui::vec2(800.0, 400.0), |ui| {
            ui.set_width(400.0);
            comment_card(ui, &p, "Alan Turing", "2026-07-29 13:28", &body);
        });
        let (a, b) = (crate::testutil::ink_of(&shapes, "Alan Turing"), crate::testutil::ink_of(&shapes, "Looks good"));
        assert!((b.top() - a.bottom() - HEADER_TO_BODY).abs() <= 0.5, "descenders → body {}", b.top() - a.bottom());
    }

    #[test]
    #[cfg_attr(not(target_os = "macos"), ignore = "glyph-ink metrics are calibrated on macOS font rasterisation")]
    fn the_title_sits_24_under_the_key_row_and_24_over_the_pills() {
        // Round 15 E2 / E7: the key row's lowest ink (the link's descenders, else the key) → the title's ink 24;
        // the title's last line's ink → the pills 24, with or without descenders. Real fonts, 2x.
        let p = crate::theme::LIGHT;
        let marker = egui::Color32::from_rgb(1, 2, 3);
        for (link, title) in
            [(None, "Dashboards"), (Some("CI-404 Build System (ongoing)"), "Verify the MM/DD x-axis label on the autoscaler daily spend bar chart")]
        {
            let shapes = crate::testutil::paint_at2(egui::vec2(800.0, 400.0), |ui| {
                let lowest = ui
                    .allocate_ui_with_layout(vec2(ui.available_width(), crate::views::BUTTON_H), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.set_min_height(crate::views::BUTTON_H);
                        let r = ui.label(RichText::new("CI-3778").font(theme::mono(12.5)).color(p.muted));
                        let mut lowest = ink_bottom_at(ui, r.rect.top(), "CI-3778", &theme::mono(12.5));
                        if let Some(l) = link {
                            let r = crate::views::text_link(ui, l, theme::body(13.0), p.accent);
                            lowest = lowest.max(ink_bottom_at(ui, r.rect.top(), l, &theme::body(13.0)));
                        }
                        lowest
                    })
                    .inner;
                title_block(ui, &p, lowest, title);
                crate::views::pill(ui, "New", marker, p.text);
            });
            let k = crate::testutil::ink_of(&shapes, "CI-3778");
            let low = link.map_or(k.bottom(), |l| k.bottom().max(crate::testutil::ink_of(&shapes, l).bottom()));
            let t = crate::testutil::ink_of(&shapes, title);
            assert!((t.top() - low - KEY_TO_TITLE).abs() <= 0.25, "{link:?}: key row → title {}", t.top() - low);
            let pill = crate::testutil::fills(&shapes).iter().find(|(_, c)| *c == marker).expect("pill").0;
            assert!((pill.top() - t.bottom() - TITLE_TO_PILLS).abs() <= 0.25, "{title}: title → pills {}", pill.top() - t.bottom());
        }
    }

    #[test]
    fn section_labels_sit_16_over_their_content() {
        // Round 15 E6 / F2: "Description" → its first line, "Comments" → the first card, 16 to the ink.
        let p = crate::theme::LIGHT;
        let v = serde_json::json!({"type": "doc", "content": [{"type": "paragraph", "content": [{"type": "text", "text": "Confirm the bar"}]}]});
        let shapes = crate::testutil::paint_at2(egui::vec2(800.0, 600.0), |ui| {
            ui.set_width(400.0);
            let below = section_heading(ui, &p, "Description");
            ui.add_space(label_space(ui, below, adf_view::first_ink_top(ui.ctx(), &v)));
            adf_view::show_value(ui, &v, &p, p.surface);
            ui.add_space(24.0);
            let below = section_heading(ui, &p, "Comments (1)");
            ui.add_space(label_space(ui, below, 0.0));
            comment_card(ui, &p, "Alan Turing", "2026-07-29 13:28", &v);
        });
        let gap = crate::testutil::ink_of(&shapes, "Confirm the bar").top() - crate::testutil::ink_of(&shapes, "Description").bottom();
        assert!((gap - LABEL_INK_GAP).abs() <= 0.5, "Description → text {gap}");
        let card = crate::testutil::fills(&shapes).iter().find(|(_, c)| *c == p.surface_2).expect("card").0;
        let gap = card.top() - crate::testutil::ink_of(&shapes, "Comments (1)").bottom();
        assert!((gap - LABEL_INK_GAP).abs() <= 0.5, "Comments → card {gap}");
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
            assert!((gap - theme::COMPACT.section_gap).abs() <= 0.5, "{below}: {gap}");
        }
    }

    #[test]
    fn comment_author_and_date_stand_apart() {
        // Regression (round 7): "Ada Lovelace2026-07-29 13:28" — the pane's zero item spacing leaked in.
        let (a, d) = crate::testutil::with_ui(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            comment_header(ui, &crate::theme::LIGHT, "Ada Lovelace", "2026-07-29 13:28")
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
