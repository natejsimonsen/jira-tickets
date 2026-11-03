//! Shared building blocks every screen uses for the same job: cards and selected rows, section headers, empty
//! states, pills, the segmented sub-view switcher, toggle chips, the quiet button, epic labels, missing values,
//! dialogs and key chips. API notes for screen authors: `.ui-polish/components.md`.

use crate::theme::{self, Palette, Tint};
use crate::views::BUTTON_H;
use egui::{Color32, CornerRadius, Frame, Margin, Painter, Rect, Response, RichText, Sense, Stroke, StrokeKind, Ui, vec2};

/// The corner radius of a selected list row (as picker rows).
pub const ROW_RADIUS: u8 = 4;

/// A card's fill: `surface`, or the accent-tinted `selection` when it is the selected or focused one.
pub fn card_fill(p: &Palette, selected: bool) -> Color32 {
    if selected { p.selection() } else { p.surface }
}

/// A card's 1 px edge: `border`, or the accent `selection_edge` when selected. One width in both states, so
/// selecting never moves the layout.
pub fn card_stroke(p: &Palette, selected: bool) -> Stroke {
    Stroke::new(1.0, if selected { p.selection_edge() } else { p.border })
}

/// The one card frame (Team, Roadmap, Standup, Grooming cards): `card_fill`, `card_stroke`, RADIUS, the
/// density's `card_pad` inside.
pub fn card_frame(ui: &Ui, p: &Palette, selected: bool) -> Frame {
    let pad = theme::spacing(ui.ctx()).card_pad;
    Frame::new().fill(card_fill(p, selected)).stroke(card_stroke(p, selected)).corner_radius(theme::RADIUS).inner_margin(Margin::same(pad as i8))
}

/// A list row's fill when not selected: `hover_row` while hovered, else none.
pub fn row_hover_fill(hovered: bool, p: &Palette) -> Color32 {
    if hovered { p.hover_row } else { Color32::TRANSPARENT }
}

/// Between a section header's title and its count, ink to ink.
pub const HEADER_COUNT_GAP: f32 = 8.0;

/// A section header's count as shown: just the number ("3"), never "(3)" or "3 items".
pub fn count_label(n: usize) -> String {
    n.to_string()
}

/// The one section header ("In progress  3"): the title in bold `h3` `text`, the count (if any) regular `h3`
/// `muted` on the same baseline, HEADER_COUNT_GAP after it. It adds no space around itself; put the density's
/// `section_gap` above and `line_gap` below.
pub fn section_header(ui: &mut Ui, p: &Palette, title: &str, count: Option<usize>) -> Response {
    let size = theme::spacing(ui.ctx()).h3;
    let title_g = ui.painter().layout_no_wrap(title.to_string(), theme::bold(size), p.text);
    let count_g = count.map(|n| ui.painter().layout_no_wrap(count_label(n), theme::body(size), p.muted));
    let gap = count_g.as_ref().map_or(0.0, |c| theme::ink_gap(&title_g, c, HEADER_COUNT_GAP));
    let w = title_g.size().x + count_g.as_ref().map_or(0.0, |c| gap + c.size().x);
    let h = title_g.size().y.max(count_g.as_ref().map_or(0.0, |c| c.size().y));
    let (rect, resp) = ui.allocate_exact_size(vec2(w.ceil(), h), Sense::hover());
    if ui.is_rect_visible(rect) {
        let line = rect.top() + crate::views::galley_baseline(&title_g);
        let title_w = title_g.size().x;
        ui.painter().galley(rect.min, title_g, p.text);
        if let Some(c) = count_g {
            let top = crate::views::top_for_baseline(line, crate::views::galley_baseline(&c));
            ui.painter().galley(egui::pos2(rect.left() + title_w + gap, top), c, p.muted);
        }
    }
    resp
}

/// The detail panes' labels (Issues detail sections and fields, Grooming's "Description"): bold caption in `muted`.
pub fn pane_label_font(ui: &Ui) -> egui::FontId {
    theme::bold(theme::spacing(ui.ctx()).caption)
}

/// A pane label: chrome, not content, so it is small and muted and a description's own headings (text colour,
/// 18 and down) read as the document under it. A count follows in regular muted on the same baseline,
/// `components::HEADER_COUNT_GAP` after the title's ink, as in every section header.
pub fn pane_label(ui: &mut Ui, p: &Palette, title: &str, count: Option<usize>) -> egui::Response {
    let font = pane_label_font(ui);
    let title_g = ui.painter().layout_no_wrap(title.to_string(), font.clone(), p.muted);
    let count_g = count.map(|n| ui.painter().layout_no_wrap(count_label(n), theme::body(font.size), p.muted));
    let gap = count_g.as_ref().map_or(0.0, |cg| theme::ink_gap(&title_g, cg, HEADER_COUNT_GAP));
    let w = title_g.size().x + count_g.as_ref().map_or(0.0, |cg| gap + cg.size().x);
    let (rect, resp) = ui.allocate_exact_size(vec2(w.ceil(), title_g.size().y), egui::Sense::hover());
    if ui.is_rect_visible(rect) {
        let title_w = title_g.size().x;
        ui.painter().galley(rect.min, title_g, p.muted);
        if let Some(cg) = count_g {
            ui.painter().galley(egui::pos2(rect.left() + title_w + gap, rect.top()), cg, p.muted);
        }
    }
    resp
}

/// The one empty state: what is (not) here in `text` at body size (the heading of the pair), then optionally what
/// to do next in `small` `muted` text, `line_gap` apart. Left-aligned; centre it with the caller's layout. No
/// trailing period on the message ("Nothing in Now"); the hint is a sentence ("Press r to run it now.").
pub fn empty_state(ui: &mut Ui, p: &Palette, message: &str, hint: Option<&str>) {
    let sp = theme::spacing(ui.ctx());
    ui.label(RichText::new(message).color(p.text).size(sp.body));
    if let Some(hint) = hint {
        ui.add_space(sp.line_gap - ui.spacing().item_spacing.y);
        ui.label(RichText::new(hint).color(p.muted).size(sp.small));
    }
}

/// A missing value ("not set", "no update yet", "—") in the `placeholder` role at `small` size. Never the
/// danger colour: missing is not an error.
pub fn missing_value(ui: &mut Ui, p: &Palette, text: &str) -> Response {
    let size = theme::spacing(ui.ctx()).small;
    ui.label(RichText::new(text).color(p.placeholder()).size(size))
}

/// The one neutral pill (issue type, labels, team, assignee, source, confidence, "No lane"): outlined, 1 px
/// `pill_edge` (it holds on dialog and card fills), `muted` caption text, radius 10. Filled pills mean status
/// only (`status_pill`).
pub fn neutral_pill(ui: &mut Ui, p: &Palette, text: &str) -> Response {
    crate::views::pill_sized(ui, text, Color32::TRANSPARENT, p.muted, Stroke::new(1.0, p.pill_edge()), crate::views::PILL_MARGIN)
}

/// A status pill ("Work in Progress", "Blocked"): the only filled pill, in its `theme::status_tint`. One rule in
/// both themes and on every fill: the wash resolved over `surface` (`views::pill_fill`), no edge. The selection
/// tint is kept clear of every status fill (`Palette::selection`), so a pill keeps its shape on selected rows
/// without a stroke, and a to-do ("New") pill is a grey fill, never an outline like the `neutral_pill` labels.
pub fn status_pill(ui: &mut Ui, text: &str, tint: Tint) -> Response {
    crate::views::pill_outlined(ui, text, tint.bg, tint.fg)
}

/// A filled pill's edge: none, in both themes (round 3: the solid edge some fills took near the selection drew a
/// bright outline on every dark row). Kept so callers that pass a stroke stay on the one rule.
pub fn status_pill_edge(_bg: Color32, _fg: Color32, _p: &Palette) -> Stroke {
    Stroke::NONE
}

/// The least contrast a status pill's fill keeps off the selection (`theme` tests measure every status).
#[cfg_attr(not(test), allow(dead_code))]
pub const PILL_FILL_STEP: f32 = 1.2;

/// A ticket title on a row or card (Issues, Grooming, Team, Me, Roadmap, Standup): `theme::bold` (the semibold
/// face) at the density's `card_title` (13.5 compact, 16 comfortable). Every ticket title takes this, so titles
/// are one weight on every screen.
pub fn card_title_font(ui: &Ui) -> egui::FontId {
    theme::bold(theme::spacing(ui.ctx()).card_title)
}

/// How far a scroll-edge fade reaches into its viewport: 12, about the bottom padding of a row, so a row that
/// fits keeps its text clear of the fade and only the strip that is actually cut fades out (24 faded the last
/// readable row to 3–4:1).
pub const EDGE_FADE: f32 = 12.0;
/// The fade's opacity halfway in (`paint_edge_fades`): the inner half stays mostly clear, so a descender or pill
/// edge that reaches into it keeps AA contrast; the outer half carries the "more this way" cue.
pub const EDGE_FADE_MID: f32 = 0.3;

/// Which edges of a scroll viewport cut content off, as [top, bottom, left, right], for a scroll `offset`, the
/// `viewport` size and the `content` size (half a point of slack, so a list scrolled to its end shows none).
pub fn clipped_edges(offset: egui::Vec2, viewport: egui::Vec2, content: egui::Vec2) -> [bool; 4] {
    const SLACK: f32 = 0.5;
    [offset.y > SLACK, offset.y + viewport.y < content.y - SLACK, offset.x > SLACK, offset.x + viewport.x < content.x - SLACK]
}

/// The scroll-edge fade for a clipped list or column (Team columns, Roadmap's Later, Grooming's lists): call it
/// right after `ScrollArea::show`, with the area's output and the `fill` the content sits on. Every edge that
/// cuts content off (`clipped_edges`) fades the last EDGE_FADE of the viewport into `fill`, so a half-cut row reads
/// as "more this way" rather than as a broken layout. Edges with nothing past them get nothing. Use it instead of
/// egui's own fade (the views turn that off, `views::no_scroll_fade`), so every clipped list shows the same cue.
pub fn scroll_edge_fade<R>(ui: &Ui, out: &egui::scroll_area::ScrollAreaOutput<R>, fill: Color32) {
    let view = out.inner_rect;
    let edges = clipped_edges(out.state.offset, view.size(), out.content_size);
    paint_edge_fades(ui.painter(), view, fill, edges);
}

/// Paints `scroll_edge_fade`'s gradients inside `view` on the [top, bottom, left, right] edges set in `edges`: one
/// mesh per edge, clear at its inner end, EDGE_FADE_MID of `fill` halfway, and `fill` at the viewport's edge.
pub fn paint_edge_fades(painter: &Painter, view: Rect, fill: Color32, edges: [bool; 4]) {
    let painter = painter.with_clip_rect(painter.clip_rect().intersect(view));
    let h = EDGE_FADE.min(view.height() / 2.0);
    let w = EDGE_FADE.min(view.width() / 2.0);
    // The opacity of `fill` at the band's inner end, its middle and the viewport edge.
    let ramp = [Color32::TRANSPARENT, fill.gamma_multiply(EDGE_FADE_MID), fill];
    // Each band as (the edge it fades into, the inward offset, whether it runs across x).
    let (l, r, t, b) = (view.left(), view.right(), view.top(), view.bottom());
    let bands: [(f32, f32, bool); 4] = [(t, h, false), (b, -h, false), (l, w, true), (r, -w, true)];
    for ((edge, depth, vertical), on) in bands.into_iter().zip(edges) {
        if !on || depth == 0.0 {
            continue;
        }
        let mut mesh = egui::Mesh::default();
        // Three lines of two vertices: the viewport edge (`fill`), halfway (MID), the inner end (clear).
        for (i, k) in [(2usize, 0.0f32), (1, 0.5), (0, 1.0)] {
            let at = edge + depth * k;
            let (a, c) = if vertical { (egui::pos2(at, t), egui::pos2(at, b)) } else { (egui::pos2(l, at), egui::pos2(r, at)) };
            mesh.colored_vertex(a, ramp[i]);
            mesh.colored_vertex(c, ramp[i]);
        }
        for q in [0u32, 2] {
            mesh.add_triangle(q, q + 1, q + 3);
            mesh.add_triangle(q, q + 3, q + 2);
        }
        painter.add(egui::Shape::mesh(mesh));
    }
}

/// The epic dot's diameter.
pub const EPIC_DOT: f32 = 8.0;
/// From the epic dot to its name's ink.
pub const EPIC_DOT_GAP: f32 = 6.0;

/// An epic as neutral text with a small dot in its hue: the dot carries the epic's identity (`tint.stripe` from
/// `theme::epic_tint_in`), the name stays secondary text (`muted`, lifted on `on_fill` where needed), so epic
/// colours never compete with status colours. `on_fill` is what the label sits on (the row or card fill).
pub fn epic_label(ui: &mut Ui, p: &Palette, name: &str, tint: Tint, on_fill: Color32) -> Response {
    let size = theme::spacing(ui.ctx()).caption;
    let fg = theme::muted_on(on_fill, p);
    let g = ui.painter().layout_no_wrap(name.to_string(), theme::body(size), fg);
    let ink = theme::ink_rect(&g);
    let lead = if ink.is_positive() { ink.left() } else { 0.0 };
    let (rect, resp) = ui.allocate_exact_size(vec2(EPIC_DOT + EPIC_DOT_GAP - lead + g.size().x, g.size().y.max(EPIC_DOT)), Sense::hover());
    if ui.is_rect_visible(rect) {
        let text_top = rect.center().y - g.size().y / 2.0;
        // The dot centred on the lowercase body (between the baseline and the x-height), not the line box.
        let x_mid = if ink.is_positive() { text_top + (crate::views::galley_baseline(&g) + ink.top().max(0.0)) / 2.0 } else { rect.center().y };
        ui.painter().circle_filled(egui::pos2(rect.left() + EPIC_DOT / 2.0, x_mid.max(rect.top() + EPIC_DOT / 2.0)), EPIC_DOT / 2.0, tint.stripe);
        ui.painter().galley(egui::pos2(rect.left() + EPIC_DOT + EPIC_DOT_GAP - lead, text_top), g, fg);
    }
    resp
}

/// A segment's side padding, label to edge.
pub const SEGMENT_PAD: f32 = 12.0;
/// The selected segment's inset from the track's edge.
pub const SEGMENT_INSET: f32 = 2.0;

/// The selected segment's raised fill, one idea in both themes: a surface a step lighter than the track with a
/// 1 px ring (`segment_edge`): white in light, DARK_SEGMENT in dark.
pub fn segment_fill(p: &Palette) -> Color32 {
    if p.dark { theme::DARK_SEGMENT } else { p.surface }
}

/// The selected segment's ring: `border` in light, DARK_EDGE in dark (where `border` vanishes on the track).
pub fn segment_edge(p: &Palette) -> Color32 {
    p.pill_edge()
}

/// The selected segment's label: `text` in both themes (light 16.5:1, dark 8.5:1), the strongest label in the
/// control; the others are `muted`.
pub fn segment_text(p: &Palette) -> Color32 {
    p.text
}

/// The one sub-view switcher (Grooming's Pre-check / Session, Roadmap's views): a quiet track (`button_fill`,
/// 1 px `border`, BUTTON_H) with the current segment raised on it in `text`, the others `muted`. Quieter than the
/// header's tab bar and never the filled primary look. Returns the index clicked when it is not `selected`.
pub fn segmented(ui: &mut Ui, p: &Palette, options: &[&str], selected: usize) -> Option<usize> {
    let size = theme::spacing(ui.ctx()).body;
    let labels: Vec<_> = options.iter().map(|o| ui.painter().layout_no_wrap(o.to_string(), theme::body(size), p.text)).collect();
    let widths: Vec<f32> = labels.iter().map(|g| (g.size().x + 2.0 * SEGMENT_PAD).ceil()).collect();
    let (track, _) = ui.allocate_exact_size(vec2(widths.iter().sum::<f32>() + 2.0 * SEGMENT_INSET, BUTTON_H), Sense::hover());
    let track = crate::views::whole_points(track);
    let mut clicked = None;
    if ui.is_rect_visible(track) {
        ui.painter().rect(track, f32::from(theme::RADIUS), theme::button_fill(p), Stroke::new(1.0, p.border), StrokeKind::Inside);
    }
    let mut x = track.left() + SEGMENT_INSET;
    for (i, (g, w)) in labels.into_iter().zip(widths).enumerate() {
        let seg = Rect::from_min_size(egui::pos2(x, track.top() + SEGMENT_INSET), vec2(w, track.height() - 2.0 * SEGMENT_INSET));
        x += w;
        let resp = ui.interact(seg, ui.id().with(("segmented", options[i])), Sense::click());
        let on = i == selected;
        resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, on, options[i]));
        if on {
            let edge = Stroke::new(1.0, segment_edge(p));
            ui.painter().rect(seg, CornerRadius::same(theme::RADIUS - SEGMENT_INSET as u8), segment_fill(p), edge, StrokeKind::Inside);
        }
        let fg = if on {
            segment_text(p)
        } else if resp.hovered() {
            p.text
        } else {
            p.muted
        };
        let g = if fg == p.text { g } else { ui.painter().layout_no_wrap(options[i].to_string(), theme::body(size), fg) };
        let y = (seg.center().y - g.size().y / 2.0).round();
        ui.painter().galley(egui::pos2(seg.left() + SEGMENT_PAD, y), g, fg);
        if resp.clicked() && !on {
            clicked = Some(i);
        }
    }
    clicked
}

/// A toggle chip's fill and edge: on is the `accent_subtle` tint with an accent edge; off the resting control.
pub fn toggle_chip_colors(p: &Palette, on: bool, hovered: bool) -> (Color32, Color32) {
    match (on, hovered) {
        (true, _) => (p.accent_subtle(), p.accent),
        (false, true) => (theme::control_hover(p), p.border),
        (false, false) => (theme::button_fill(p), p.border),
    }
}

/// The check an on chip shows before its label: its width and the gap to the label's box.
pub const CHIP_CHECK_W: f32 = 10.0;
pub const CHIP_CHECK_GAP: f32 = 6.0;

/// A toggle chip's width for a label `label_w` wide: room for the check in both states, so turning a chip on
/// never moves its neighbours.
pub fn toggle_chip_width(label_w: f32) -> f32 {
    crate::views::chip_width(label_w + CHIP_CHECK_W + CHIP_CHECK_GAP)
}

/// Where the label starts inside a chip `w` wide: off, centred; on, after the check, the pair centred.
pub fn toggle_chip_label_x(w: f32, label_w: f32, on: bool) -> f32 {
    let content = if on { CHIP_CHECK_W + CHIP_CHECK_GAP + label_w } else { label_w };
    let left = ((w - content) / 2.0).round();
    if on { left + CHIP_CHECK_W + CHIP_CHECK_GAP } else { left }
}

/// A filter toggle chip ("Only opened", "Mine"): the density's `control_h`, RADIUS, `text` label. On reads three ways at once in
/// both themes: the `accent_subtle` fill, a 1 px accent edge and an accent check before the label, so it reads
/// as "on" without looking like the primary action. The width never changes with state (`toggle_chip_width`).
pub fn toggle_chip(ui: &mut Ui, p: &Palette, label: &str, on: bool) -> Response {
    let g = ui.painter().layout_no_wrap(label.to_string(), theme::body(14.0), p.text);
    let w = toggle_chip_width(g.size().x);
    let (rect, resp) = ui.allocate_exact_size(vec2(w, theme::spacing(ui.ctx()).control_h), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, on, label));
    let rect = crate::views::whole_points(rect);
    if ui.is_rect_visible(rect) {
        let (fill, edge) = toggle_chip_colors(p, on, resp.hovered());
        ui.painter().rect(rect, f32::from(theme::RADIUS), fill, Stroke::new(1.0, edge), StrokeKind::Inside);
        let x = rect.left() + toggle_chip_label_x(w, g.size().x, on);
        if on {
            paint_check(ui.painter(), egui::pos2(x - CHIP_CHECK_GAP - CHIP_CHECK_W, rect.center().y), p.accent);
        }
        // One device pixel up, like the combos' text (round 15 B1).
        let y = (rect.center().y - g.size().y / 2.0).round() - 1.0 / ui.ctx().pixels_per_point();
        ui.painter().galley(egui::pos2(x, y), g, p.text);
    }
    resp
}

/// A CHIP_CHECK_W wide check mark, its left end at `left` and centred on `left.y`.
fn paint_check(painter: &Painter, left: egui::Pos2, color: Color32) {
    let at = |x: f32, y: f32| egui::pos2(left.x + x, left.y + y);
    painter.add(egui::Shape::line(vec![at(0.5, 0.0), at(3.5, 3.0), at(9.5, -3.5)], Stroke::new(1.5, color)));
}

/// The quiet (ghost) button's side padding, from its text to the hover fill: the density's `button_padding.x`,
/// so a hovered ghost is exactly a secondary button's box.
fn ghost_pad(ui: &Ui) -> f32 {
    ui.spacing().button_padding.x
}

/// Whether the next widget is the first one in `ui`'s row: its text then starts on the content edge (left in a
/// left-to-right row, right in a right-to-left one, and always in a column).
fn first_in_row(ui: &Ui) -> bool {
    let (cursor, max) = (ui.cursor(), ui.max_rect());
    match ui.layout().main_dir() {
        egui::Direction::LeftToRight => (cursor.left() - max.left()).abs() < 0.5,
        egui::Direction::RightToLeft => (cursor.right() - max.right()).abs() < 0.5,
        _ => true,
    }
}

/// A ghost button's allocated width and its text's x inside it, for text `text_w` wide: no padding on the side
/// that meets the content edge (`first`), so its text sits on the grid like a label; the pad between it and any
/// neighbour, so the hovered box keeps the same item spacing as every bordered button.
pub fn ghost_layout(text_w: f32, pad: f32, first: bool, rtl: bool) -> (f32, f32) {
    let lead = if first { 0.0 } else { pad };
    let w = lead + text_w + pad;
    (w, if rtl { pad } else { lead })
}

/// The quiet (ghost) button for secondary and tertiary actions beside a primary ("Dismiss", "Backlog", "Retry"):
/// `muted` 14 text, no fill or edge at rest; the control hover fill and `border` edge on hover. BUTTON_H tall.
/// Order of weight: `primary_button` > `button` (secondary) > `ghost_button`.
/// Its text sits `button_padding.x` inside its box at rest too, so where the text must sit on the content grid
/// (a row's first or last action, a header action beside a bordered button) use `ghost` instead; keep this one for
/// egui Button features (`right_text`, `atom_ui`).
#[allow(dead_code)] // Kept stable for other screens; not called from one right now.
pub fn ghost_button(text: &str, p: &Palette) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text.to_string()).color(p.muted).size(14.0))
        .frame_when_inactive(false)
        .corner_radius(theme::RADIUS)
        .min_size(vec2(0.0, BUTTON_H))
}

/// The grid-aligned ghost button (`ui.add(c::ghost("Dismiss", p))`): `ghost_button`'s look, but at rest its text
/// sits on the content grid. The first widget in a row has no hidden inset before its text; after a neighbour it
/// keeps `button_padding.x` toward it, so hovered boxes are item spacing apart like bordered buttons. Hovered, the
/// box is `button_padding.x` either side of the text (a first ghost's box reaches past the content edge).
pub fn ghost(text: &str, p: &Palette) -> Ghost {
    Ghost { text: text.to_string(), muted: p.muted, hover_text: p.text, min_size: vec2(0.0, BUTTON_H) }
}

/// See `ghost`.
#[must_use = "add it with `ui.add`"]
pub struct Ghost {
    text: String,
    muted: Color32,
    hover_text: Color32,
    min_size: egui::Vec2,
}

impl Ghost {
    #[allow(dead_code)] // Kept stable for callers.
    /// The least size of its box (BUTTON_H tall by default; never under the interact height).
    pub fn min_size(mut self, min_size: egui::Vec2) -> Self {
        self.min_size = min_size;
        self
    }
}

impl egui::Widget for Ghost {
    fn ui(self, ui: &mut Ui) -> Response {
        let p = theme::palette(ui.ctx());
        let g = ui.painter().layout_no_wrap(self.text.clone(), theme::body(14.0), self.muted);
        let pad = ghost_pad(ui);
        let rtl = ui.layout().main_dir() == egui::Direction::RightToLeft;
        let (w, text_x) = ghost_layout(g.size().x, pad, first_in_row(ui), rtl);
        // Like egui's Button: `min_size` (BUTTON_H unless set), never under the interact height.
        let size = vec2(w.max(self.min_size.x).ceil(), self.min_size.y.max(ui.spacing().interact_size.y));
        let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), &self.text));
        if ui.is_rect_visible(rect) {
            // Right to left, the text keeps its offset from the box's right end (flush when first), however wide `min_size` makes it.
            let text_left = if rtl { rect.right() - (w - text_x) } else { rect.left() + text_x };
            let hot = resp.hovered() || resp.has_focus() || resp.is_pointer_button_down_on();
            if hot {
                let box_ = Rect::from_x_y_ranges((text_left - pad)..=(text_left + g.size().x + pad), rect.y_range());
                let fill = if resp.is_pointer_button_down_on() { p.selected_row } else { theme::control_hover(p) };
                ui.painter().rect(crate::views::whole_points(box_), f32::from(theme::RADIUS), fill, Stroke::new(1.0, p.border), StrokeKind::Inside);
            }
            let color = if hot { self.hover_text } else { self.muted };
            let y = (rect.center().y - g.size().y / 2.0).round();
            ui.painter().galley_with_override_text_color(egui::pos2(text_left, y), g, color);
        }
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        resp
    }
}

/// The one modal dialog frame: `dialog_fill`, the dialog edge, RADIUS and the theme's `dialog_shadow` (its
/// elevation over the `scrim_alpha` backdrop). The caller sets the inner margin.
pub fn dialog_frame(p: &Palette) -> Frame {
    Frame::new().fill(crate::views::dialog_fill(p)).stroke(crate::views::dialog_stroke(p)).corner_radius(theme::RADIUS).shadow(p.dialog_shadow())
}

/// A key chip's padding around the key's ink.
pub const KEY_CHIP_PAD: egui::Vec2 = vec2(4.0, 2.0);
/// A key chip's corner radius.
pub const KEY_CHIP_RADIUS: u8 = 3;

/// The chip behind a key (hint bar, tooltips, inline legends), for a key galley whose ink is `ink`: the ink
/// grown by KEY_CHIP_PAD. Space the next item KEY_CHIP_PAD.x further off so the gap holds to the chip.
pub fn key_chip_rect(ink: Rect) -> Rect {
    ink.expand2(KEY_CHIP_PAD)
}

/// Paints the key chip for a key galley painted at `pos` (draw the galley after it): the neutral chip step off
/// `container` (`theme::chip_fill_on`) with a quiet 1 px `key_chip_edge`, so every key, one letter or "⌘Enter",
/// reads as a key in one style on every fill, the white and the dark dialogs included.
pub fn paint_key_chip(painter: &Painter, pos: egui::Pos2, galley: &egui::Galley, container: Color32, p: &Palette) {
    let ink = theme::ink_rect(galley);
    let ink = if ink.is_positive() { ink } else { Rect::from_min_size(egui::Pos2::ZERO, galley.size()) };
    let r = key_chip_rect(ink.translate(pos.to_vec2()));
    painter.rect(r, f32::from(KEY_CHIP_RADIUS), theme::chip_fill_on(container, p), key_chip_edge(p), StrokeKind::Inside);
}

/// Every key chip's edge: the neutral `pill_edge`, drawn inside the chip so it never changes the chip's size.
pub fn key_chip_edge(p: &Palette) -> Stroke {
    Stroke::new(1.0, p.pill_edge())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_cards_and_rows_share_one_accent_treatment() {
        for p in [&theme::LIGHT, &theme::DARK] {
            assert_eq!(card_fill(p, true), p.selection());
            assert_eq!(card_fill(p, false), p.surface);
            assert_eq!(card_stroke(p, true).color, p.selection_edge());
            assert_eq!(card_stroke(p, true).width, card_stroke(p, false).width, "selecting never moves the layout");
            assert!(theme::contrast(card_stroke(p, true).color, card_fill(p, true)) >= 3.0, "dark={}", p.dark);
            assert_eq!(row_hover_fill(false, p), Color32::TRANSPARENT);
        }
    }

    #[test]
    fn counts_are_bare_numbers() {
        assert_eq!(count_label(3), "3");
        assert_eq!(count_label(0), "0");
    }

    #[test]
    fn toggle_chip_states_read_apart_and_quieter_than_primary() {
        for p in [&theme::LIGHT, &theme::DARK] {
            let (on, on_edge) = toggle_chip_colors(p, true, false);
            let (off, _) = toggle_chip_colors(p, false, false);
            assert_ne!(on, off);
            assert_ne!(on, p.accent, "on is a tint, not the primary fill");
            assert!(theme::contrast(on_edge, on) >= 3.0, "dark={}", p.dark);
            assert!(theme::contrast(p.text, on) >= 7.0, "dark={}", p.dark);
            assert_eq!(toggle_chip_colors(p, true, true), toggle_chip_colors(p, true, false), "hover keeps on");
        }
    }

    #[test]
    fn toggle_chip_keeps_its_width_and_shows_a_check_when_on() {
        let w = toggle_chip_width(40.0);
        assert_eq!(w, crate::views::chip_width(40.0 + CHIP_CHECK_W + CHIP_CHECK_GAP));
        let off = toggle_chip_label_x(w, 40.0, false);
        assert_eq!(off, ((w - 40.0) / 2.0).round(), "off: centred");
        let on = toggle_chip_label_x(w, 40.0, true);
        assert!(on - CHIP_CHECK_W - CHIP_CHECK_GAP >= crate::views::CHIP_PAD, "the check stays inside the pad");
        assert!(on + 40.0 <= w - crate::views::CHIP_PAD + 0.5);
        for p in [&theme::LIGHT, &theme::DARK] {
            let (on_fill, _) = toggle_chip_colors(p, true, false);
            assert!(theme::contrast(p.accent, on_fill) >= 3.0, "the check reads on the tint, dark={}", p.dark);
        }
    }

    #[test]
    fn ghost_text_sits_on_the_grid_when_first_and_pads_toward_neighbours() {
        assert_eq!(ghost_layout(50.0, 12.0, true, false), (62.0, 0.0), "first, left to right: text at the edge");
        assert_eq!(ghost_layout(50.0, 12.0, false, false), (74.0, 12.0), "after a neighbour: the pad between");
        assert_eq!(ghost_layout(50.0, 12.0, true, true), (62.0, 12.0), "first, right to left: text ends at the edge");
        assert_eq!(ghost_layout(50.0, 12.0, false, true), (74.0, 12.0));
    }

    #[test]
    fn status_pills_hold_their_shape_on_the_selection() {
        let overrides = std::collections::BTreeMap::new();
        for p in [&theme::LIGHT, &theme::DARK] {
            for (name, cat) in [
                ("Work in Progress", "indeterminate"),
                ("In Review", "indeterminate"),
                ("Planning", "new"),
                ("Blocked", "new"),
                ("New", "new"),
                ("Done", "done"),
            ] {
                let t = theme::status_tint(name, cat, p, &overrides);
                assert_eq!(status_pill_edge(t.bg, t.fg, p), Stroke::NONE, "one rule: no edge in either theme");
                let fill = crate::views::pill_fill(t.bg, p);
                assert_eq!(fill.a(), 255, "opaque, so the same colour on every fill");
                let shape = theme::contrast(fill, p.selection());
                assert!(shape >= PILL_FILL_STEP, "dark={} {name}: {shape:.2}", p.dark);
                assert!(theme::contrast(t.fg, fill) >= 4.5, "dark={} {name}", p.dark);
            }
            // A to-do pill is filled, so it never reads as an outlined label pill.
            let new = theme::status_tint("New", "new", p, &overrides);
            assert!(theme::contrast(crate::views::pill_fill(new.bg, p), p.surface) >= 1.3, "dark={}", p.dark);
        }
    }

    #[test]
    fn key_chips_read_on_the_dialogs() {
        for p in [&theme::LIGHT, &theme::DARK] {
            let dialog = crate::views::dialog_fill(p);
            let fill = theme::chip_fill_on(dialog, p);
            assert!(theme::contrast(fill, dialog) >= 1.17 || theme::contrast(key_chip_edge(p).color, dialog) >= 1.5, "dark={}", p.dark);
            assert!(theme::contrast(key_chip_edge(p).color, dialog) >= 1.5, "dark={}", p.dark);
            assert!(theme::contrast(p.text, fill) >= 7.0, "dark={}", p.dark);
        }
        assert!(theme::contrast(theme::chip_fill_on(theme::DARK_DIALOG, &theme::DARK), theme::DARK_DIALOG) >= 1.3);
    }

    #[test]
    fn segmented_selection_is_raised_not_primary() {
        for p in [&theme::LIGHT, &theme::DARK] {
            let f = segment_fill(p);
            assert_ne!(f, p.accent);
            assert!(theme::contrast(f, theme::button_fill(p)) >= 1.05, "dark={}", p.dark);
            assert!(theme::contrast(segment_text(p), f) >= 4.5 && theme::contrast(p.muted, theme::button_fill(p)) >= 4.5, "dark={}", p.dark);
        }
        // One idea in both themes: a quiet lift ringed by an edge that stands off the track, primary text on it,
        // and never an inverted fill (the label is the light `text` in dark, not the page colour).
        for p in [&theme::LIGHT, &theme::DARK] {
            let lift = theme::contrast(segment_fill(p), theme::button_fill(p));
            assert!((1.05..2.0).contains(&lift), "dark={}: {lift:.2}", p.dark);
            assert!(theme::contrast(segment_edge(p), theme::button_fill(p)) >= 1.3, "dark={}", p.dark);
            assert!(theme::contrast(segment_text(p), segment_fill(p)) >= 7.0, "dark={}", p.dark);
            assert!(theme::contrast(segment_text(p), segment_fill(p)) > theme::contrast(p.muted, theme::button_fill(p)), "dark={}", p.dark);
        }
    }

    #[test]
    fn epic_dots_show_on_rows_and_selection() {
        for p in [&theme::LIGHT, &theme::DARK] {
            for s in 0..theme::EPIC_SLOTS {
                let dot = theme::slot_tint(s, p).stripe;
                for fill in [p.surface, p.bg, p.selection(), p.hover_row] {
                    assert!(theme::contrast(dot, fill) >= 3.0, "dark={} slot {s}: {:.2}", p.dark, theme::contrast(dot, fill));
                }
            }
        }
    }

    #[test]
    fn edge_fades_show_only_where_content_is_cut() {
        let (view, content) = (vec2(300.0, 400.0), vec2(300.0, 1000.0));
        assert_eq!(clipped_edges(vec2(0.0, 0.0), view, content), [false, true, false, false], "at the top: more below");
        assert_eq!(clipped_edges(vec2(0.0, 300.0), view, content), [true, true, false, false]);
        assert_eq!(clipped_edges(vec2(0.0, 600.0), view, content), [true, false, false, false], "at the end: none below");
        assert_eq!(clipped_edges(vec2(0.0, 0.0), view, vec2(300.0, 400.2)), [false; 4], "fits: no fade");
        assert_eq!(clipped_edges(vec2(0.0, 0.0), vec2(1000.0, 400.0), vec2(1600.0, 400.0)), [false, false, false, true], "columns off to the right");
        let shapes = crate::testutil::paint_at2(vec2(400.0, 400.0), |ui| {
            let r = Rect::from_min_size(egui::pos2(10.0, 10.0), vec2(200.0, 300.0));
            paint_edge_fades(ui.painter(), r, theme::LIGHT.bg, [false, true, false, true]);
        });
        let meshes = shapes.iter().filter(|c| matches!(c.shape, egui::Shape::Mesh(_))).count();
        assert_eq!(meshes, 2, "one band per cut edge");
    }

    #[test]
    fn card_titles_are_one_bold_step() {
        crate::testutil::with_ui(|ui| {
            let f = card_title_font(ui);
            assert_eq!(f, theme::bold(theme::spacing(ui.ctx()).card_title));
        });
        let (c, m) = (theme::Density::Compact.spacing(), theme::Density::Comfortable.spacing());
        assert_eq!(c.card_title, 13.5, "the Issues rows' title size");
        assert!(m.card_title > c.card_title && c.card_title < c.h3);
        assert!(m.card_title >= m.body + 1.0 && m.card_title < m.h3, "comfortable: a title one step over body, under h3");
    }

    #[test]
    fn key_chip_pads_the_ink() {
        let ink = Rect::from_min_max(egui::pos2(10.0, 5.0), egui::pos2(18.0, 14.0));
        let r = key_chip_rect(ink);
        assert_eq!((r.left(), r.right(), r.top(), r.bottom()), (6.0, 22.0, 3.0, 16.0));
        for p in [&theme::LIGHT, &theme::DARK] {
            let fill = theme::chip_fill_on(p.surface_2, p);
            assert!(theme::contrast(p.text, fill) >= 7.0, "dark={}", p.dark);
        }
    }

    #[test]
    fn components_draw_in_a_real_pass() {
        crate::testutil::with_ui(|ui| {
            let p = theme::palette(ui.ctx());
            let h = section_header(ui, p, "In progress", Some(3));
            assert!(h.rect.height() > 0.0 && h.rect.width() > 0.0);
            empty_state(ui, p, "Nothing in Now", Some("Move an initiative here from Next."));
            assert_eq!(segmented(ui, p, &["Pre-check", "Session"], 0), None);
            let c = toggle_chip(ui, p, "Mine", true);
            assert_eq!(c.rect.height(), BUTTON_H);
            let e = epic_label(ui, p, "Blue Rollouts", theme::slot_tint(0, p), p.surface);
            assert!(e.rect.width() > EPIC_DOT + EPIC_DOT_GAP);
            assert!(ui.add(ghost_button("Dismiss", p)).rect.height() >= BUTTON_H);
            assert!(ui.add(ghost("Clear", p).min_size(vec2(0.0, 24.0))).rect.height() >= 24.0);
            let first = ui.horizontal(|ui| ui.add(ghost("Dismiss", p)).rect.left() - ui.max_rect().left()).inner;
            assert_eq!(first, 0.0, "a first ghost's box starts at the content edge, its text with it");
            status_pill(ui, "Work in Progress", theme::slot_tint(0, p));
            missing_value(ui, p, "not set");
            neutral_pill(ui, p, "Story");
        });
    }
}
