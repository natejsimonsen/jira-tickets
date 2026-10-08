//! Paints an `adf::Doc` with egui widgets.

use crate::adf::{self, Block, Span};
use crate::theme::{self, Palette};
use egui::text::LayoutJob;
use egui::{Frame, Margin, RichText, Stroke, TextFormat, Ui};

/// `container` is the fill the document sits on; code chips and blocks take a step off it (`chip_fill_on`).
pub fn show_value(ui: &mut Ui, v: &serde_json::Value, p: &Palette, container: egui::Color32) {
    let doc = adf::parse(v);
    if doc.blocks.is_empty() {
        ui.label(RichText::new("No description").italics().color(p.muted));
        return;
    }
    show(ui, &doc, p, theme::chip_fill_on(container, p));
}

/// Space after a paragraph (or any block).
const BLOCK_GAP: f32 = 12.0;
/// The one gap between list items.
const LIST_ITEM_GAP: f32 = 4.0;
const BODY: f32 = 14.0;
/// The bullet's slot: about a numeral's width, so bulleted and numbered text line up.
const BULLET_W: f32 = 10.0;
/// Inline code is this fraction of the surrounding size, exactly, in every context (headings included).
const CODE_SCALE: f32 = 0.9;
/// An inline code chip's padding either side (the text keeps the same room from its neighbours).
const CODE_PAD_X: f32 = 4.0;
/// An inline code chip's padding above and below its glyphs.
const CODE_PAD_Y: f32 = 1.0;
/// Issue keys are mono 12.5 at body 14, scaled with the context.
const KEY_SCALE: f32 = 12.5 / 14.0;
/// Code block line height: 1.48 × mono 12.5.
const CODE_LINE_H: f32 = 18.5;

/// Space above a heading: 24 except at the top, where the section heading already spaced it.
pub fn heading_space_above(first: bool) -> f32 {
    if first { 0.0 } else { 24.0 - BLOCK_GAP }
}

pub fn show(ui: &mut Ui, doc: &adf::Doc, p: &Palette, chip: egui::Color32) {
    ui.scope(|ui| show_blocks(ui, doc, p, chip));
}

fn show_blocks(ui: &mut Ui, doc: &adf::Doc, p: &Palette, chip: egui::Color32) {
    // Every gap below is explicit; no implicit item spacing on top of it.
    ui.spacing_mut().item_spacing.y = 0.0;
    let n = doc.blocks.len();
    for (i, b) in doc.blocks.iter().enumerate() {
        if matches!(b, Block::Heading { .. }) {
            ui.add_space(heading_space_above(i == 0));
        }
        block(ui, b, p, 14.0, chip);
        if i + 1 < n {
            ui.add_space(if matches!(b, Block::Heading { .. }) { 8.0 } else { BLOCK_GAP });
        }
    }
}

fn block(ui: &mut Ui, b: &Block, p: &Palette, size: f32, chip: egui::Color32) {
    match b {
        Block::Paragraph(s) => spans(ui, s, p, size, false, chip),
        Block::Heading { level, spans: s } => {
            let sz = match level {
                1 => 22.0,
                2 => 18.0,
                3 => 16.0,
                _ => 14.5,
            };
            spans(ui, s, p, sz, true, chip);
        }
        Block::List { ordered, items } => {
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    ui.add_space(LIST_ITEM_GAP);
                }
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    ui.add_space(8.0);
                    // The marker is body size and muted whatever the item's own font (mono items included).
                    let line_h = ui.fonts_mut(|f| f.row_height(&theme::body(BODY)));
                    if *ordered {
                        // Numbers right-aligned in one gutter as wide as the list's widest, so "2." and "10."
                        // items start on one edge.
                        let measure = |n: usize| ui.painter().layout_no_wrap(format!("{n}."), theme::body(BODY), p.muted).size().x;
                        let gutter = number_gutter((1..=items.len()).map(measure));
                        let g = ui.painter().layout_no_wrap(format!("{}.", i + 1), theme::body(BODY), p.muted);
                        let (rect, _) = ui.allocate_exact_size(egui::vec2(gutter, line_h), egui::Sense::hover());
                        ui.painter().galley(egui::pos2(rect.right() - g.size().x, rect.top()), g, p.muted);
                    } else {
                        let (rect, _) = ui.allocate_exact_size(egui::vec2(BULLET_W, line_h), egui::Sense::hover());
                        ui.painter().circle_filled(rect.center(), 2.5, p.muted);
                    }
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        for (j, ib) in item.iter().enumerate() {
                            if j > 0 {
                                ui.add_space(LIST_ITEM_GAP);
                            }
                            block(ui, ib, p, size, chip);
                        }
                    });
                });
            }
        }
        Block::Code { text, .. } => {
            Frame::new().fill(chip).corner_radius(theme::RADIUS).inner_margin(Margin::same(12)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(text).font(theme::mono(12.5)).line_height(Some(CODE_LINE_H)).color(p.text));
            });
        }
        Block::Quote(bs) => {
            ui.horizontal_top(|ui| {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(3.0, 0.0), egui::Sense::hover());
                let top = rect.top();
                let inner = ui.vertical(|ui| {
                    for ib in bs {
                        block(ui, ib, p, size, chip);
                    }
                });
                let bottom = inner.response.rect.bottom();
                ui.painter().vline(rect.left() + 1.5, top..=bottom, Stroke::new(3.0, p.border));
            });
        }
        Block::Panel { kind, blocks } => {
            let (bg, border) = match kind.as_str() {
                "warning" => (p.at_risk.gamma_multiply(0.15), p.at_risk),
                "error" => (p.off_track.gamma_multiply(0.15), p.off_track),
                "success" => (p.on_track.gamma_multiply(0.15), p.on_track),
                _ => (p.accent.gamma_multiply(0.12), p.accent),
            };
            Frame::new().fill(bg).stroke(Stroke::new(1.0, border)).corner_radius(theme::RADIUS).inner_margin(Margin::symmetric(12, 8)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                for ib in blocks {
                    block(ui, ib, p, size, chip);
                }
            });
        }
        Block::Table(rows) => {
            egui::Grid::new(ui.next_auto_id()).striped(true).spacing([12.0, 4.0]).show(ui, |ui| {
                for (ri, row) in rows.iter().enumerate() {
                    for cell in row {
                        spans(ui, cell, p, 13.0, ri == 0, chip);
                    }
                    ui.end_row();
                }
            });
        }
        Block::Rule => {
            ui.separator();
        }
    }
}

/// The text format one span is painted with. Links are accent and always underlined (hue alone is too weak
/// in dark). A smart-link issue key is mono like keys elsewhere. Inline code is mono at exactly 0.9× the
/// surrounding size; its chip is painted under it by `spans` (see `code_runs`).
fn span_format(sp: &Span, p: &Palette, size: f32, heading: bool) -> TextFormat {
    let mut fmt = TextFormat { font_id: if sp.bold || heading { theme::bold(size) } else { theme::body(size) }, color: p.text, ..Default::default() };
    if sp.code {
        fmt.font_id = theme::mono(size * CODE_SCALE);
        // Centre the smaller mono run on the surrounding text's line.
        fmt.valign = egui::Align::Center;
    }
    if is_key_link(sp) {
        fmt.font_id = theme::mono(size * KEY_SCALE);
        fmt.valign = egui::Align::Center;
    }
    if sp.italic {
        fmt.italics = true;
    }
    if sp.strike {
        fmt.strikethrough = Stroke::new(1.0, p.muted);
    }
    if sp.link.is_some() {
        fmt.color = p.accent;
        fmt.underline = Stroke::new(1.0, p.accent);
    }
    fmt
}

/// A smart link that shows an issue key (an inlineCard to /browse/KEY).
fn is_key_link(sp: &Span) -> bool {
    sp.link.as_deref().and_then(adf::browse_key) == Some(sp.text.as_str())
}

/// The ordered-list number gutter: the widest "N." in the list, at least the bullet's slot.
pub fn number_gutter(widths: impl IntoIterator<Item = f32>) -> f32 {
    widths.into_iter().fold(BULLET_W, f32::max).ceil()
}

/// One glyph on a laid-out row, for `code_runs`: (char, in inline code, left, right, top, bottom).
pub type GlyphBox = (char, bool, f32, f32, f32, f32);

/// The inline-code chips on one laid-out row, as (left, right, top, bottom) around their glyphs: one per
/// run of code glyphs, with whitespace at either end of a run left out, so a code span that wraps never
/// carries a trailing space's fill to the line end.
pub fn code_runs(glyphs: &[GlyphBox]) -> Vec<(f32, f32, f32, f32)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < glyphs.len() {
        if !glyphs[i].1 {
            i += 1;
            continue;
        }
        let start = i;
        while i < glyphs.len() && glyphs[i].1 {
            i += 1;
        }
        let run: Vec<&GlyphBox> = glyphs[start..i].iter().skip_while(|g| g.0.is_whitespace()).collect();
        let n = run.iter().rposition(|g| !g.0.is_whitespace()).map_or(0, |k| k + 1);
        if n > 0 {
            let run = &run[..n];
            let top = run.iter().map(|g| g.4).fold(f32::INFINITY, f32::min);
            let bottom = run.iter().map(|g| g.5).fold(f32::NEG_INFINITY, f32::max);
            out.push((run[0].2, run[n - 1].3, top, bottom));
        }
    }
    out
}

fn spans(ui: &mut Ui, s: &[Span], p: &Palette, size: f32, heading: bool, chip: egui::Color32) {
    let s = adf::tidy_code_spans(&adf::linkify(s));
    let has_link = s.iter().any(|sp| sp.link.is_some());
    let mut job = LayoutJob::default();
    // Each char of the job's text, and whether it is inline code.
    let mut chars: Vec<(char, bool)> = Vec::new();
    for (i, sp) in s.iter().enumerate() {
        // The chip's padding either side of inline code: before it, and before whatever follows it.
        let after_code = i > 0 && s[i - 1].code;
        let pad = if sp.code || after_code { CODE_PAD_X } else { 0.0 };
        job.append(&sp.text, pad, span_format(sp, p, size, heading));
        chars.extend(sp.text.chars().map(|c| (c, sp.code)));
    }
    // The chips go under the label: lay the job out as the label will (wrapped at the available width) and
    // paint a padded fill around each run of code glyphs, row by row.
    let chips = ui.painter().add(egui::Shape::Noop);
    let laid = chars.iter().any(|c| c.1).then(|| {
        let mut j = job.clone();
        j.wrap.max_width = ui.available_width();
        ui.fonts_mut(|f| f.layout_job(j))
    });
    let sense = if has_link { egui::Sense::click() } else { egui::Sense::hover() };
    let r = ui.add(egui::Label::new(job).wrap().sense(sense));
    if let Some(g) = laid {
        let mut shapes = Vec::new();
        // Glyphs come in text order; a row ending in a newline drops it, so walk the chars alongside.
        let mut at_char = 0;
        for row in &g.rows {
            let boxes: Vec<GlyphBox> = row
                .row
                .glyphs
                .iter()
                .map(|gl| {
                    while at_char < chars.len() && chars[at_char].0 != gl.chr {
                        at_char += 1;
                    }
                    let code = chars.get(at_char).is_some_and(|c| c.1);
                    at_char += 1;
                    let lr = gl.logical_rect();
                    (gl.chr, code, lr.left(), lr.right(), lr.top(), lr.bottom())
                })
                .collect();
            for (l, rt, t, b) in code_runs(&boxes) {
                let at = r.rect.min + row.pos.to_vec2();
                let rect = egui::Rect::from_min_max(at + egui::vec2(l, t), at + egui::vec2(rt, b)).expand2(egui::vec2(CODE_PAD_X, CODE_PAD_Y));
                shapes.push(egui::Shape::rect_filled(rect, 4.0, chip));
            }
        }
        ui.painter().set(chips, egui::Shape::Vec(shapes));
    }
    if has_link {
        r.clone().on_hover_cursor(egui::CursorIcon::PointingHand);
    }
    // One link per paragraph is the common case; open the first on click.
    if r.clicked()
        && let Some(url) = s.iter().find_map(|sp| sp.link.clone())
        && crate::app::openable(&url)
    {
        ui.ctx().open_url(egui::OpenUrl::new_tab(url));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Color32;

    fn palette() -> Palette {
        let c = |v: u8| Color32::from_gray(v);
        Palette {
            dark: false,
            bg: c(1),
            surface: c(2),
            surface_2: c(3),
            border: c(4),
            text: c(5),
            muted: c(6),
            accent: c(7),
            on_accent: c(18),
            on_track: c(8),
            at_risk: c(9),
            off_track: c(10),
            no_health: c(11),
            done: c(12),
            today: c(13),
            selected_row: c(14),
            hover_row: c(15),
            danger_subtle: c(16),
            tooltip: c(17),
            tooltip_text: c(19),
        }
    }

    fn span(text: &str) -> Span {
        Span { text: text.into(), bold: false, italic: false, code: false, strike: false, link: None }
    }

    #[test]
    fn span_format_maps_marks() {
        let p = palette();
        let plain = span_format(&span("a"), &p, 14.0, false);
        assert_eq!(plain.font_id, theme::body(14.0));
        assert_eq!(plain.color, p.text);
        assert_eq!(plain.underline, Stroke::NONE);

        let bold = span_format(&Span { bold: true, ..span("b") }, &p, 14.0, false);
        assert_eq!(bold.font_id, theme::bold(14.0));
        assert_eq!(span_format(&span("h"), &p, 18.0, true).font_id, theme::bold(18.0));

        let code = span_format(&Span { code: true, ..span("c") }, &p, 14.0, false);
        assert_eq!(code.font_id, theme::mono(14.0 * 0.9), "exactly 0.9 × 14");
        assert_eq!(code.background, Color32::TRANSPARENT, "the chip is painted under the label, not by the section");
        assert_eq!(span_format(&Span { code: true, ..span("c") }, &p, 14.5, true).font_id, theme::mono(14.5 * 0.9), "the same ratio in a heading");

        let styled = span_format(&Span { italic: true, strike: true, ..span("s") }, &p, 14.0, false);
        assert!(styled.italics);
        assert_eq!(styled.strikethrough, Stroke::new(1.0, p.muted));

        let link = Span { link: Some("https://x".into()), ..span("l") };
        assert_eq!(span_format(&link, &p, 14.0, false).color, p.accent);
        assert_eq!(span_format(&link, &p, 14.0, false).underline, Stroke::new(1.0, p.accent), "links are always underlined");

        let key = Span { link: Some("https://x.atlassian.net/browse/CI-1".into()), ..span("CI-1") };
        let k = span_format(&key, &p, 14.0, false);
        assert_eq!((k.font_id, k.color), (theme::mono(12.5), p.accent), "a smart-link key is mono 12.5 in the link colour");
        assert_eq!(span_format(&Span { text: "see CI-1".into(), ..key }, &p, 14.0, false).font_id, theme::body(14.0));
    }

    #[test]
    fn code_chips_skip_whitespace_at_run_ends() {
        let g = |c: char, code: bool, x: f32| (c, code, x, x + 5.0, 0.0, 14.0);
        // "a `b c ` d" wrapped so the code run ends in a space on this row.
        let row = [g('a', false, 0.0), g(' ', true, 5.0), g('b', true, 10.0), g(' ', true, 15.0), g('c', true, 20.0), g(' ', true, 25.0), g('d', false, 30.0)];
        assert_eq!(code_runs(&row), vec![(10.0, 25.0, 0.0, 14.0)]);
        assert!(code_runs(&[g(' ', true, 0.0)]).is_empty(), "a run of only spaces gets no chip");
        assert_eq!(code_runs(&[g('x', true, 0.0), g('y', false, 5.0), g('z', true, 10.0)]).len(), 2);
    }

    #[test]
    fn numbers_share_one_gutter() {
        assert_eq!(number_gutter([7.0, 8.2]), BULLET_W.max(8.2).ceil());
        assert_eq!(number_gutter([7.0, 8.0, 15.4]), 16.0, "the widest number sets it");
        assert_eq!(number_gutter([]), BULLET_W);
    }

    #[test]
    fn headings_get_24_above_except_first() {
        assert_eq!(heading_space_above(true), 0.0);
        assert_eq!(heading_space_above(false) + BLOCK_GAP, 24.0);
    }
}
