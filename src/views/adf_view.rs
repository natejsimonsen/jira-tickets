//! Paints an `adf::Doc` with egui widgets.

use crate::adf::{self, Block, Span};
use crate::theme::{self, Palette};
use egui::text::LayoutJob;
use egui::{Frame, Margin, RichText, Stroke, TextFormat, Ui};

/// `container` is the fill the document sits on; code chips and blocks take a step off it (`chip_fill_on`).
pub fn show_value(ui: &mut Ui, v: &serde_json::Value, p: &Palette, container: egui::Color32) {
    let doc = adf::parse(v);
    if doc.blocks.is_empty() {
        // Upright muted 14, like every other empty value (it was the only italic in the app), laid out as a
        // one-paragraph description: the body line height in the document's zero-spacing scope, so the gap to
        // Comments is a paragraph's (round 11 E1; the 17 label box sat 3.5 tighter).
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.label(RichText::new(NO_DESCRIPTION).font(theme::body(BODY)).line_height(Some(line_height(BODY))).color(p.muted));
        });
        return;
    }
    show(ui, &doc, p, theme::chip_fill_on(container, p));
}

/// The empty description's line.
pub const NO_DESCRIPTION: &str = "No description";

/// Space after a paragraph (or any block).
const BLOCK_GAP: f32 = 12.0;
/// The one gap between list items.
const LIST_ITEM_GAP: f32 = 4.0;
/// Above a later block (a second paragraph, a nested list) inside one list item.
const LIST_BLOCK_GAP: f32 = 8.0;
const BODY: f32 = 14.0;
/// The bullet's slot at the right of the marker slot: about a numeral's width.
const BULLET_W: f32 = 10.0;
/// Inline code is this fraction of the surrounding size, exactly, capped at 0.9× body (so a heading's code
/// is never the largest mono on screen).
const CODE_SCALE: f32 = 0.9;
/// An inline code chip's padding either side (the text keeps the same room from its neighbours).
const CODE_PAD_X: f32 = 4.0;
/// An inline code chip's padding above and below its glyphs.
const CODE_PAD_Y: f32 = 1.0;
/// Issue keys are mono 12.5 at body 14, scaled with the context.
const KEY_SCALE: f32 = 12.5 / 14.0;
/// Code block line height: 1.48 × mono 12.5.
const CODE_LINE_H: f32 = 18.5;
/// A code block's inner margin: 12 on every side, counting the line box's leading (round 12 E2: the 10 / 8 that
/// put 12 to the ink read as ~8 to the lines' boxes).
const CODE_MARGIN: Margin = Margin::same(12);
/// The marker gutter: list text starts this far from the list's left edge, for bullets and numbers alike.
const LIST_GUTTER: f32 = 24.0;
/// Marker → item text.
const MARKER_GAP: f32 = 6.0;

/// Body line height for a text size: 20 at body 14 (≈1.43, so wrapped lines and their code chips breathe);
/// the same ratio for headings and table text.
pub fn line_height(size: f32) -> f32 {
    (size * 20.0 / BODY).round()
}

/// The heading sizes: h1–h2 15 bold, h3–h6 14 bold, on the type scale (no 18–22 headings under the 22 title).
pub fn heading_size(level: u8) -> f32 {
    if level <= 2 { 15.0 } else { 14.0 }
}

/// The marker slot (left of MARKER_GAP): fixed so text starts LIST_GUTTER in, unless a number is wider.
pub fn marker_slot(widest_number: f32) -> f32 {
    (LIST_GUTTER - MARKER_GAP).max(widest_number.ceil())
}

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
            spans(ui, s, p, heading_size(*level), true, chip);
        }
        Block::List { ordered, items } => {
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    ui.add_space(LIST_ITEM_GAP);
                }
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = MARKER_GAP;
                    // The marker is body size and muted whatever the item's own font (mono items included),
                    // in a fixed slot so every list's text starts LIST_GUTTER in.
                    let line_h = line_height(BODY);
                    let measure = |n: usize| ui.painter().layout_no_wrap(format!("{n}."), theme::body(BODY), p.muted).size().x;
                    let widest = if *ordered { (1..=items.len()).map(measure).fold(0.0, f32::max) } else { 0.0 };
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(marker_slot(widest), line_h), egui::Sense::hover());
                    if *ordered {
                        // Numbers right-aligned against the gap, so "2." and "10." items start on one edge.
                        let g = ui.painter().layout_no_wrap(format!("{}.", i + 1), theme::body(BODY), p.muted);
                        ui.painter().galley(egui::pos2(rect.right() - g.size().x, (rect.center().y - g.size().y / 2.0).round()), g, p.muted);
                    } else {
                        ui.painter().circle_filled(egui::pos2(rect.right() - BULLET_W / 2.0, rect.center().y), 2.5, p.muted);
                    }
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        for (j, ib) in item.iter().enumerate() {
                            if j > 0 {
                                ui.add_space(LIST_BLOCK_GAP);
                            }
                            block(ui, ib, p, size, chip);
                        }
                    });
                });
            }
        }
        Block::Code { text, .. } => {
            Frame::new().fill(chip).corner_radius(theme::RADIUS).inner_margin(CODE_MARGIN).show(ui, |ui| {
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
/// surrounding size; its chip is painted under it by `spans` (see `code_runs`). Mono runs sit on the body's
/// baseline: `fit` gives each its line height for that (`mono_line_height`), bottom-aligned.
fn span_format(sp: &Span, p: &Palette, size: f32, heading: bool, fit: &MonoFit) -> TextFormat {
    let mut fmt = TextFormat {
        font_id: if sp.bold || heading { theme::bold(size) } else { theme::body(size) },
        color: p.text,
        line_height: Some(line_height(size)),
        ..Default::default()
    };
    if sp.code {
        fmt.font_id = theme::mono(code_size(size));
        fmt.line_height = Some(fit.code);
        fmt.valign = egui::Align::Max;
    }
    if is_key_link(sp) {
        fmt.font_id = theme::mono(size * KEY_SCALE);
        fmt.line_height = Some(fit.key);
        fmt.valign = egui::Align::Max;
    }
    if sp.italic {
        fmt.italics = true;
    }
    if sp.strike {
        fmt.strikethrough = Stroke::new(1.0, p.muted);
    }
    if sp.link.is_some() {
        // Underlined by `spans` (`link_underlines`), LINK_UNDERLINE_DROP under the baseline: egui's own
        // underline sits on the line box's bottom, 3–4 px down.
        fmt.color = p.accent;
    }
    fmt
}

/// A link's underline: its 1 pt line's top this far under the baseline.
pub const LINK_UNDERLINE_DROP: f32 = 2.0;

/// The y of a link underline's centre for a run whose baseline is `baseline`: its 1 pt line starts
/// LINK_UNDERLINE_DROP under the baseline, on whole points so it stays one crisp line.
pub fn underline_y(baseline: f32) -> f32 {
    (baseline + LINK_UNDERLINE_DROP).round() + 0.5
}

/// The underlined spans of one row: runs of link glyphs given as (is link, left, right), with whitespace at
/// either end of a run left out (the space after a link is not underlined).
pub fn underline_runs(glyphs: &[(bool, bool, f32, f32)]) -> Vec<(f32, f32)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < glyphs.len() {
        if !glyphs[i].0 {
            i += 1;
            continue;
        }
        let start = i;
        while i < glyphs.len() && glyphs[i].0 {
            i += 1;
        }
        let run = &glyphs[start..i];
        let (Some(a), Some(b)) = (run.iter().position(|g| !g.1), run.iter().rposition(|g| !g.1)) else { continue };
        out.push((run[a].2, run[b].3));
    }
    out
}

/// Every link underline in a laid-out paragraph, relative to the galley: (x range, the run's baseline). `links` is each
/// char of the paragraph and whether it is in a link, in text order.
fn link_underlines(g: &egui::Galley, links: &[(char, bool)]) -> Vec<(egui::Rangef, f32)> {
    let mut out = Vec::new();
    let mut at = 0;
    for row in &g.rows {
        let mut baseline = None;
        let boxes: Vec<(bool, bool, f32, f32)> = row
            .row
            .glyphs
            .iter()
            .map(|gl| {
                while at < links.len() && links[at].0 != gl.chr {
                    at += 1;
                }
                let link = links.get(at).is_some_and(|c| c.1);
                at += 1;
                if link && baseline.is_none() {
                    baseline = Some(row.pos.y + gl.pos.y);
                }
                let lr = gl.logical_rect();
                (link, gl.chr.is_whitespace(), row.pos.x + lr.left(), row.pos.x + lr.right())
            })
            .collect();
        if let Some(b) = baseline {
            out.extend(underline_runs(&boxes).into_iter().map(|(l, r)| (egui::Rangef::new(l, r), b)));
        }
    }
    out
}

/// The line heights inline code and key links take in one paragraph so their baselines meet the body's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MonoFit {
    pub code: f32,
    pub key: f32,
}

impl MonoFit {
    /// Measures the fonts of a context of `size`: each run's baseline from its own row top.
    pub fn measure(fonts: &mut egui::epaint::text::FontsView<'_>, size: f32, heading: bool) -> Self {
        let mut baseline = |font: egui::FontId, lh: Option<f32>| {
            let mut job = LayoutJob::default();
            job.append("x", 0.0, TextFormat { font_id: font, line_height: lh, ..Default::default() });
            let g = fonts.layout_job(job);
            g.rows.first().and_then(|r| r.row.glyphs.first()).map_or(0.0, |gl| gl.pos.y)
        };
        let body = baseline(if heading { theme::bold(size) } else { theme::body(size) }, Some(line_height(size)));
        let code = baseline(theme::mono(code_size(size)), None);
        let key = baseline(theme::mono(size * KEY_SCALE), None);
        let row = line_height(size);
        MonoFit { code: mono_line_height(row, body, code), key: mono_line_height(row, body, key) }
    }
}

/// A bottom-aligned mono run in a `row_h` row is drawn at its ascent + (row_h − its line height) (egui's
/// valign), and a body run at the body's ascent; this line height makes the two baselines meet. Never taller
/// than the row, so a mono run never grows a body line.
pub fn mono_line_height(row_h: f32, body_baseline: f32, mono_baseline: f32) -> f32 {
    (row_h - (body_baseline - mono_baseline)).min(row_h)
}

/// Each laid-out row's top and the paragraph's height when every row takes at least `line_h` (rows of only
/// mono text are shorter): the row sits at the bottom of its slot so its baseline lands where a body row's
/// would.
pub fn row_tops(heights: &[f32], line_h: f32) -> (Vec<f32>, f32) {
    let mut y = 0.0;
    let tops = heights
        .iter()
        .map(|&h| {
            let pitch = h.max(line_h);
            let top = y + (pitch - h);
            y += pitch;
            top
        })
        .collect();
    (tops, y)
}

/// Inline code's size in a context of `size`: 0.9×, at most 0.9× body.
pub fn code_size(size: f32) -> f32 {
    CODE_SCALE * size.min(BODY)
}

/// A smart link that shows an issue key (an inlineCard to /browse/KEY).
fn is_key_link(sp: &Span) -> bool {
    sp.link.as_deref().and_then(adf::browse_key) == Some(sp.text.as_str())
}

/// One glyph on a laid-out row, for `code_runs`: (char, in inline code, left, right, top, bottom).
pub type GlyphBox = (char, bool, f32, f32, f32, f32);

/// One inline-code chip on a row: its glyphs' (left, right, top, bottom), and whether each side hugs
/// punctuation (an opening mark just before it, a closing one just after), where the chip pads less.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CodeRun {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
    pub tight_left: bool,
    pub tight_right: bool,
}

/// Punctuation that hugs the code after it ("(`x`").
pub fn opens(c: char) -> bool {
    matches!(c, '(' | '[' | '{' | '"' | '\'' | '“' | '‘')
}

/// Punctuation that hugs the code before it ("`x`),").
pub fn closes(c: char) -> bool {
    matches!(c, ')' | ']' | '}' | ',' | '.' | ';' | ':' | '!' | '?' | '"' | '\'' | '”' | '’')
}

/// The inline-code chips on one laid-out row: one per run of code glyphs, with whitespace at either end of
/// a run left out, so a code span that wraps never carries a trailing space's fill to the line end.
pub fn code_runs(glyphs: &[GlyphBox]) -> Vec<CodeRun> {
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
        let lead = glyphs[start..i].iter().take_while(|g| g.0.is_whitespace()).count();
        let run = &glyphs[start + lead..i];
        let n = run.iter().rposition(|g| !g.0.is_whitespace()).map_or(0, |k| k + 1);
        if n > 0 {
            let run = &run[..n];
            let top = run.iter().map(|g| g.4).fold(f32::INFINITY, f32::min);
            let bottom = run.iter().map(|g| g.5).fold(f32::NEG_INFINITY, f32::max);
            // Hugging only when the neighbour touches the run's glyphs, with no space in between.
            let before = (lead == 0 && start > 0).then(|| glyphs[start - 1].0);
            let after = (start + lead + n == i && i < glyphs.len()).then(|| glyphs[i].0);
            out.push(CodeRun {
                left: run[0].2,
                right: run[n - 1].3,
                top,
                bottom,
                tight_left: before.is_some_and(opens),
                tight_right: after.is_some_and(closes),
            });
        }
    }
    out
}

/// How far to move a laid-out row right so a chip that starts it keeps its 4 px pad inside the text column
/// (its fill starts at the text edge) instead of hanging into the margin.
pub fn row_inset(glyphs: &[GlyphBox]) -> f32 {
    match glyphs.first() {
        Some(&(c, true, left, ..)) if !c.is_whitespace() => (CODE_PAD_X - left).max(0.0),
        _ => 0.0,
    }
}

/// The leading space before span `i`: the chip's pad before inline code and before what follows it. Only
/// where punctuation hugs the code ("(`07-31`)", "`x`, then") is the gap the chip's tight reach instead, so
/// the mark sits just outside the fill. The text's own whitespace is never touched: " every" after a chip
/// keeps its space on top of the pad.
pub fn span_lead(s: &[Span], i: usize) -> f32 {
    let sp = &s[i];
    let prev = i.checked_sub(1).map(|j| &s[j]);
    if sp.code {
        let hugged = prev.is_some_and(|q| !q.code && q.text.chars().last().is_some_and(opens));
        if hugged { CODE_PAD_TIGHT } else { CODE_PAD_X }
    } else if prev.is_some_and(|q| q.code) {
        if sp.text.chars().next().is_some_and(closes) { CODE_PAD_TIGHT } else { CODE_PAD_X }
    } else {
        0.0
    }
}

/// A chip's fill reach past its glyphs on a side that hugs punctuation.
const CODE_PAD_TIGHT: f32 = 2.0;

/// One paragraph's layout job, and per char whether it is inline code.
pub fn paragraph_job(s: &[Span], p: &Palette, size: f32, heading: bool, fit: &MonoFit) -> (LayoutJob, Vec<(char, bool)>) {
    let mut job = LayoutJob::default();
    let mut chars: Vec<(char, bool)> = Vec::new();
    for (i, sp) in s.iter().enumerate() {
        job.append(&sp.text, span_lead(s, i), span_format(sp, p, size, heading, fit));
        chars.extend(sp.text.chars().map(|c| (c, sp.code)));
    }
    (job, chars)
}

/// Lays `job` out in `width`: rows that start with a chip move in by its pad, every row takes at least the
/// body line height (`row_tops`), and the chips' rects (relative to the galley) come back with it.
pub fn layout_paragraph(
    fonts: &mut egui::epaint::text::FontsView<'_>,
    mut job: LayoutJob,
    chars: &[(char, bool)],
    width: f32,
    line_h: f32,
) -> (egui::Galley, Vec<egui::Rect>) {
    // 8 px narrower so a row moved in by `row_inset` (and a chip's pad at the row end) still fits.
    job.wrap.max_width = (width - 2.0 * CODE_PAD_X).max(1.0);
    let laid = fonts.layout_job(job);
    let mut g = (*laid).clone();
    let (tops, height) = row_tops(&g.rows.iter().map(|r| r.row.size.y).collect::<Vec<_>>(), line_h);
    let dy = height - g.rect.height();
    let mut runs: Vec<(f32, CodeRun)> = Vec::new();
    // Glyphs come in text order; a row ending in a newline drops it, so walk the chars alongside.
    let mut at_char = 0;
    for (row, top) in g.rows.iter_mut().zip(tops) {
        row.pos.y = top;
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
                // The run's own font box (not its fitted line height), so the chip hugs the glyphs.
                let glyph_top = gl.pos.y - gl.font_ascent;
                (gl.chr, code, lr.left(), lr.right(), glyph_top, glyph_top + gl.font_height)
            })
            .collect();
        let shift = row_inset(&boxes);
        row.pos.x += shift;
        for run in code_runs(&boxes) {
            runs.push((row.pos.y, CodeRun { left: run.left + row.pos.x, right: run.right + row.pos.x, ..run }));
        }
    }
    let widest = g.rows.iter().map(|r| r.pos.x + r.row.size.x).fold(0.0, f32::max);
    g.rect.max.x = g.rect.max.x.max(g.rect.min.x + widest);
    g.rect.max.y += dy;
    g.mesh_bounds.max.x = g.mesh_bounds.max.x.max(g.mesh_bounds.min.x + widest);
    g.mesh_bounds.max.y += dy;
    let pad = |tight: bool| if tight { CODE_PAD_TIGHT } else { CODE_PAD_X };
    let chips = runs
        .into_iter()
        .map(|(y, run)| {
            egui::Rect::from_min_max(
                egui::pos2(run.left - pad(run.tight_left), y + run.top - CODE_PAD_Y),
                egui::pos2(run.right + pad(run.tight_right), y + run.bottom + CODE_PAD_Y),
            )
        })
        .collect();
    (g, chips)
}

fn spans(ui: &mut Ui, s: &[Span], p: &Palette, size: f32, heading: bool, chip: egui::Color32) {
    let s = adf::tidy_code_spans(&adf::linkify(s));
    let has_link = s.iter().any(|sp| sp.link.is_some());
    let fit = ui.fonts_mut(|f| MonoFit::measure(f, size, heading));
    let (job, chars) = paragraph_job(&s, p, size, heading, &fit);
    let sense = if has_link { egui::Sense::click() } else { egui::Sense::hover() };
    let chips = ui.painter().add(egui::Shape::Noop);
    let links: Vec<(char, bool)> = s.iter().flat_map(|sp| sp.text.chars().map(move |c| (c, sp.link.is_some()))).collect();
    let (r, galley) = if chars.iter().any(|c| c.1) || s.iter().any(is_key_link) {
        // Lay the paragraph out ourselves (see `layout_paragraph`), paint the chips under it, then show that
        // exact galley.
        let width = ui.available_width();
        let (g, rects) = ui.fonts_mut(|f| layout_paragraph(f, job, &chars, width, line_height(size)));
        let g = std::sync::Arc::new(g);
        let r = ui.add(egui::Label::new(g.clone()).sense(sense));
        let shapes = rects.into_iter().map(|rc| egui::Shape::rect_filled(rc.translate(r.rect.min.to_vec2()), 4.0, chip)).collect();
        ui.painter().set(chips, egui::Shape::Vec(shapes));
        (r, Some(g))
    } else if has_link {
        // Laid out here (as `Label::wrap` would) so the underlines can be placed on its glyphs.
        let mut job = job;
        job.wrap.max_width = ui.available_width();
        let g = ui.fonts_mut(|f| f.layout_job(job));
        (ui.add(egui::Label::new(g.clone()).sense(sense)), Some(g))
    } else {
        (ui.add(egui::Label::new(job).wrap().sense(sense)), None)
    };
    if has_link {
        if let Some(g) = &galley {
            let origin = r.rect.min.to_vec2();
            for (x, baseline) in link_underlines(g, &links) {
                let y = underline_y(baseline + origin.y);
                ui.painter().hline(egui::Rangef::new(x.min + origin.x, x.max + origin.x), y, Stroke::new(1.0, p.accent));
            }
        }
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

    const FIT: MonoFit = MonoFit { code: 18.0, key: 18.5 };

    fn span(text: &str) -> Span {
        Span { text: text.into(), bold: false, italic: false, code: false, strike: false, link: None }
    }

    #[test]
    fn code_blocks_pad_12_all_round() {
        // E2: 12 on every side to the line boxes, leading counted.
        assert_eq!(CODE_MARGIN, Margin::same(12));
    }

    #[test]
    fn link_underlines_sit_two_under_the_baseline() {
        // E2: egui's underline sat on the line box's bottom, 3–4 px down; ours starts 2 under the baseline.
        assert_eq!(underline_y(15.0), 17.5, "a 1 pt line from 17 to 18");
        assert_eq!(underline_y(15.3), 17.5, "on whole points");
        assert!(underline_y(15.0) - 0.5 - 15.0 == LINK_UNDERLINE_DROP);
        let g = |link: bool, c: char, l: f32| (link, c.is_whitespace(), l, l + 5.0);
        let row = [g(false, 'a', 0.0), g(true, 'x', 5.0), g(true, ' ', 10.0), g(true, 'y', 15.0), g(true, ' ', 20.0), g(false, 'b', 25.0)];
        assert_eq!(underline_runs(&row), [(5.0, 20.0)], "one run, its trailing space left out");
        assert!(underline_runs(&[g(true, ' ', 0.0)]).is_empty());
        assert_eq!(underline_runs(&[g(true, 'x', 0.0), g(false, 'y', 5.0), g(true, 'z', 10.0)]).len(), 2);
    }

    #[test]
    fn span_format_maps_marks() {
        let p = palette();
        let plain = span_format(&span("a"), &p, 14.0, false, &FIT);
        assert_eq!(plain.font_id, theme::body(14.0));
        assert_eq!(plain.color, p.text);
        assert_eq!(plain.underline, Stroke::NONE);

        let bold = span_format(&Span { bold: true, ..span("b") }, &p, 14.0, false, &FIT);
        assert_eq!(bold.font_id, theme::bold(14.0));
        assert_eq!(span_format(&span("h"), &p, 18.0, true, &FIT).font_id, theme::bold(18.0));

        let code = span_format(&Span { code: true, ..span("c") }, &p, 14.0, false, &FIT);
        assert_eq!(code.font_id, theme::mono(14.0 * 0.9), "exactly 0.9 × 14");
        assert_eq!(code.background, Color32::TRANSPARENT, "the chip is painted under the label, not by the section");
        assert_eq!(span_format(&Span { code: true, ..span("c") }, &p, 18.0, true, &FIT).font_id, theme::mono(14.0 * 0.9), "capped at 0.9 × body in a heading");
        assert_eq!(code_size(13.0), 13.0 * 0.9, "smaller contexts keep the ratio");

        let styled = span_format(&Span { italic: true, strike: true, ..span("s") }, &p, 14.0, false, &FIT);
        assert!(styled.italics);
        assert_eq!(styled.strikethrough, Stroke::new(1.0, p.muted));

        let link = Span { link: Some("https://x".into()), ..span("l") };
        assert_eq!(span_format(&link, &p, 14.0, false, &FIT).color, p.accent);
        assert_eq!(span_format(&link, &p, 14.0, false, &FIT).underline, Stroke::NONE, "links are underlined by `spans`, not egui");

        let key = Span { link: Some("https://x.atlassian.net/browse/CI-1".into()), ..span("CI-1") };
        let k = span_format(&key, &p, 14.0, false, &FIT);
        assert_eq!((k.font_id, k.color), (theme::mono(12.5), p.accent), "a smart-link key is mono 12.5 in the link colour");
        assert_eq!(span_format(&Span { text: "see CI-1".into(), ..key }, &p, 14.0, false, &FIT).font_id, theme::body(14.0));
    }

    #[test]
    fn code_chips_skip_whitespace_at_run_ends() {
        let g = |c: char, code: bool, x: f32| (c, code, x, x + 5.0, 0.0, 14.0);
        // "a `b c ` d" wrapped so the code run ends in a space on this row.
        let row = [g('a', false, 0.0), g(' ', true, 5.0), g('b', true, 10.0), g(' ', true, 15.0), g('c', true, 20.0), g(' ', true, 25.0), g('d', false, 30.0)];
        let r = code_runs(&row);
        assert_eq!(r.len(), 1);
        assert_eq!((r[0].left, r[0].right, r[0].top, r[0].bottom), (10.0, 25.0, 0.0, 14.0));
        assert!(code_runs(&[g(' ', true, 0.0)]).is_empty(), "a run of only spaces gets no chip");
        assert_eq!(code_runs(&[g('x', true, 0.0), g('y', false, 5.0), g('z', true, 10.0)]).len(), 2);
    }

    #[test]
    fn chips_hug_punctuation() {
        let g = |c: char, code: bool, x: f32| (c, code, x, x + 5.0, 0.0, 14.0);
        let row =
            [g('(', false, 0.0), g('0', true, 5.0), g('7', true, 10.0), g(')', false, 15.0), g(' ', false, 20.0), g('x', true, 25.0), g(' ', false, 30.0)];
        let r = code_runs(&row);
        assert!(r[0].tight_left && r[0].tight_right, "( 07 ): both sides hug");
        assert!(!r[1].tight_left && !r[1].tight_right, "spaces either side");
        let s = |text: &str, code: bool| Span { code, ..span(text) };
        let para = [s("(", false), s("07-31", true), s(") then ", false), s("x", true), s(" y", false)];
        assert_eq!(
            [span_lead(&para, 1), span_lead(&para, 2), span_lead(&para, 3), span_lead(&para, 4)],
            [CODE_PAD_TIGHT, CODE_PAD_TIGHT, CODE_PAD_X, CODE_PAD_X],
            "punctuation sits at the fill's tight reach; words keep the full pad"
        );
        assert_eq!(span_lead(&para, 0), 0.0);
    }

    #[test]
    fn a_chip_starting_a_row_keeps_its_pad_inside_the_column() {
        let g = |c: char, code: bool, x: f32| (c, code, x, x + 5.0, 0.0, 14.0);
        assert_eq!(row_inset(&[g('m', true, 0.0), g('o', true, 5.0)]), CODE_PAD_X, "a wrapped row starting in code moves in 4");
        assert_eq!(row_inset(&[g('m', true, CODE_PAD_X)]), 0.0, "the paragraph's own leading pad already insets it");
        assert_eq!(row_inset(&[g('a', false, 0.0), g('m', true, 9.0)]), 0.0);
        assert_eq!(row_inset(&[]), 0.0);
    }

    #[test]
    fn lists_share_one_24_px_gutter() {
        assert_eq!(marker_slot(0.0) + MARKER_GAP, LIST_GUTTER, "bullets");
        assert_eq!(marker_slot(15.4) + MARKER_GAP, LIST_GUTTER, "1.–9.");
        assert_eq!(marker_slot(17.6) + MARKER_GAP, LIST_GUTTER, "10.");
        assert_eq!(marker_slot(30.2), 31.0, "only a very wide number pushes the text");
    }

    #[test]
    fn body_rhythm_and_heading_sizes() {
        assert_eq!(line_height(14.0), 20.0);
        assert_eq!(line_height(15.0), 21.0);
        assert_eq!((heading_size(1), heading_size(2), heading_size(3), heading_size(6)), (15.0, 15.0, 14.0, 14.0));
        let sp = |code: bool| Span { text: "x".into(), bold: false, italic: false, code, strike: false, link: None };
        let p = palette();
        assert_eq!(span_format(&sp(false), &p, 14.0, false, &FIT).line_height, Some(20.0));
        let code = span_format(&sp(true), &p, 14.0, false, &FIT);
        assert_eq!((code.line_height, code.valign), (Some(FIT.code), egui::Align::Max), "code sits on the body baseline");
    }

    #[test]
    fn headings_get_24_above_except_first() {
        assert_eq!(heading_space_above(true), 0.0);
        assert_eq!(heading_space_above(false) + BLOCK_GAP, 24.0);
    }

    fn with_fonts<R>(f: impl FnOnce(&mut egui::epaint::text::FontsView<'_>) -> R) -> R {
        crate::testutil::with_ui(|ui| ui.fonts_mut(f))
    }

    /// (glyph left, glyph right) of every glyph, in text order, of a paragraph laid out wide.
    fn glyph_xs(s: &[Span]) -> Vec<(char, f32, f32)> {
        with_fonts(|f| {
            let p = palette();
            let fit = MonoFit::measure(f, 14.0, false);
            let s = adf::tidy_code_spans(s);
            let (job, chars) = paragraph_job(&s, &p, 14.0, false, &fit);
            let (g, _) = layout_paragraph(f, job, &chars, 2000.0, 20.0);
            g.rows.iter().flat_map(|r| r.row.glyphs.iter().map(|gl| (gl.chr, gl.pos.x, gl.pos.x + gl.advance_width))).collect()
        })
    }

    #[test]
    fn a_word_after_a_chip_keeps_its_space() {
        // Regression (round 7): "`base-workloads.yml` every" read "base-workloads.yml,every".
        let s = |text: &str, code: bool| Span { code, ..span(text) };
        let g = glyph_xs(&[s("triggers ", false), s("config.yml", true), s(" every", false)]);
        let text: String = g.iter().map(|c| c.0).collect();
        assert_eq!(text, "triggers config.yml every", "the space survives layout");
        let l = g.iter().position(|c| c.0 == 'l').unwrap();
        let (sp, e) = (g[l + 1], g[l + 2]);
        assert_eq!(sp.0, ' ');
        assert!(sp.2 - sp.1 > 2.0, "a real space glyph, {sp:?}");
        assert!(e.1 - g[l].2 >= CODE_PAD_X + 2.0, "pad and space between the chip and the word: {}", e.1 - g[l].2);
        // A space inside the code mark moves out and still separates the word.
        let g = glyph_xs(&[s("config.yml ", true), s("every", false)]);
        assert_eq!(g.iter().map(|c| c.0).collect::<String>(), "config.yml every");
    }

    #[test]
    fn punctuation_after_a_chip_hugs_it_then_keeps_its_space() {
        let s = |text: &str, code: bool| Span { code, ..span(text) };
        let g = glyph_xs(&[s("known_hosts", true), s(", with", false)]);
        assert_eq!(g.iter().map(|c| c.0).collect::<String>(), "known_hosts, with");
        let comma = g.iter().position(|c| c.0 == ',').unwrap();
        let gap = g[comma].1 - g[comma - 1].2;
        assert!((1.0..CODE_PAD_X).contains(&gap), "the comma sits at the chip's tight reach (2, pixel-snapped): {gap}");
        assert_eq!(g[comma + 1].0, ' ');
        assert!(g[comma + 2].1 - g[comma].2 > 2.0, "the space after the comma survives");
    }

    #[test]
    fn mono_runs_share_the_body_baseline() {
        assert_eq!(mono_line_height(20.0, 15.0, 12.0), 17.0);
        assert_eq!(mono_line_height(20.0, 12.0, 13.0), 20.0, "never taller than the row");
        let (base_body, base_code) = with_fonts(|f| {
            let p = palette();
            let fit = MonoFit::measure(f, 14.0, false);
            let s = [Span { code: true, ..span("code") }, span(" word")];
            let (job, _) = paragraph_job(&s, &p, 14.0, false, &fit);
            let g = f.layout_job(job);
            let gl = &g.rows[0].row.glyphs;
            (gl.iter().find(|g| g.chr == 'w').unwrap().pos.y, gl.iter().find(|g| g.chr == 'c').unwrap().pos.y)
        });
        assert!((base_body - base_code).abs() <= 0.5, "body {base_body} vs code {base_code}");
    }

    #[test]
    fn no_description_takes_a_paragraphs_height() {
        // E1: the placeholder's block is as tall as a one-paragraph description's, so Comments sits as far below.
        let h = |v: serde_json::Value| {
            crate::testutil::with_ui(|ui| {
                let p = crate::theme::LIGHT;
                ui.scope(|ui| show_value(ui, &v, &p, p.surface)).response.rect.height()
            })
        };
        let para = serde_json::json!({"type": "doc", "content": [{"type": "paragraph", "content": [{"type": "text", "text": "No description"}]}]});
        assert_eq!(h(serde_json::Value::Null), h(para));
        assert_eq!(h(serde_json::Value::Null), line_height(BODY));
    }

    #[test]
    fn every_row_takes_the_body_line_height() {
        let (tops, h) = row_tops(&[20.0, 15.0, 15.0, 20.0], 20.0);
        assert_eq!(tops, [0.0, 25.0, 45.0, 60.0], "short rows sit at the bottom of a 20 slot");
        assert_eq!(h, 80.0);
        let (tops, h) = row_tops(&[24.0], 20.0);
        assert_eq!((tops, h), (vec![0.0], 24.0), "a taller row keeps its height");
        // A wrapped all-code paragraph: separate chips, 20 apart.
        let chips = with_fonts(|f| {
            let p = palette();
            let fit = MonoFit::measure(f, 14.0, false);
            let s = [Span { code: true, ..span("b54249f25c21c feat(build): add workflow_pr_comment action") }];
            let (job, chars) = paragraph_job(&s, &p, 14.0, false, &fit);
            layout_paragraph(f, job, &chars, 200.0, 20.0).1
        });
        assert!(chips.len() >= 2, "{chips:?}");
        for w in chips.windows(2) {
            assert!((w[1].top() - w[0].top() - 20.0).abs() < 0.01, "20 px pitch: {w:?}");
            assert!(w[1].top() > w[0].bottom(), "fills never merge: {w:?}");
        }
    }
}
