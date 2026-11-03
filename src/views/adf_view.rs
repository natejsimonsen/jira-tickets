//! Paints an `adf::Doc` with egui widgets, typeset like a document: prose capped at the density's
//! `prose_max_width`, `prose_line_height` leading, `paragraph_gap` between blocks, headings on the type scale.

use crate::adf::{self, Block, Span};
use crate::theme::{self, Palette, Spacing};
use egui::text::LayoutJob;
use egui::{Frame, Margin, RichText, Stroke, TextFormat, Ui};

/// `container` is the fill the document sits on; code chips and blocks take a step off it (`chip_fill_on`).
pub fn show_value(ui: &mut Ui, v: &serde_json::Value, p: &Palette, container: egui::Color32) {
    let doc = adf::parse(v);
    if doc.blocks.is_empty() {
        // Upright muted body, laid out as a one-paragraph description (the prose line height in the
        // document's zero-spacing scope), so the gap to Comments is a paragraph's.
        let body = theme::spacing(ui.ctx()).body;
        let lh = prose_line(ui.ctx(), body);
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.label(RichText::new(NO_DESCRIPTION).font(theme::body(body)).line_height(Some(lh)).color(p.muted));
        });
        return;
    }
    show(ui, &doc, p, theme::chip_fill_on(container, p));
}

/// How far the first block of document `v` puts its ink under its top: a code block's fill is its ink; a
/// heading's ink sits in its own line; a rule is its own ink; everything else starts with a body line.
pub fn first_ink_top(ctx: &egui::Context, v: &serde_json::Value) -> f32 {
    let sp = theme::spacing(ctx);
    let doc = adf::parse(v);
    match doc.blocks.first() {
        Some(Block::Code { .. } | Block::Rule) => 0.0,
        Some(Block::Heading { level, .. }) => {
            let size = heading_px(sp, *level);
            theme::Ink::of(ctx, &theme::bold(size), Some(heading_line(size))).top
        }
        Some(Block::Table(_)) => table_pad(sp).y + theme::Ink::of(ctx, &theme::body(sp.small), Some(prose_line(ctx, sp.small))).top,
        _ => theme::Ink::of(ctx, &theme::body(sp.body), Some(prose_line(ctx, sp.body))).top,
    }
}

/// The empty description's line.
pub const NO_DESCRIPTION: &str = "No description";

/// The bullet's slot at the right of the marker slot: about a numeral's width.
const BULLET_W: f32 = 10.0;
/// Inline code is this fraction of the surrounding size (capped at 0.9× body, so a heading's code is never the
/// largest mono on screen). Smart-link and mention chips use the same step.
const CODE_SCALE: f32 = 0.9;
/// An inline chip's padding either side, to its glyphs' ink: tight, so punctuation hugs the code.
pub const CODE_PAD_X: f32 = 2.0;
/// An inline chip's padding above and below its glyphs.
const CODE_PAD_Y: f32 = 1.0;
/// An inline chip's corner radius.
const CHIP_RADIUS: f32 = 3.0;
/// A code block's padding, measured to the ink on every side: the density's card pad (12 / 16), so a block breathes
/// like the cards around it.
fn code_pad(sp: &Spacing) -> f32 {
    sp.card_pad
}
/// Code block line height as a multiple of its mono size.
const CODE_LEADING: f32 = 1.45;
/// Headings' line height as a multiple of their size (tighter than prose).
const HEADING_LEADING: f32 = 1.3;
/// The marker gutter: list text starts this far from the list's left edge, for bullets, numbers and boxes alike.
const LIST_GUTTER: f32 = 24.0;
/// Marker → item text.
const MARKER_GAP: f32 = 6.0;
/// A task's checkbox side (BULLET_W, so boxes and bullets share one marker column), its stroke and corner.
const TASK_MARK: f32 = BULLET_W;
const TASK_STROKE: f32 = 1.25;
const TASK_RADIUS: f32 = 2.0;
/// A bullet's radius (a filled disc at the top level, a ring one level down).
const BULLET_R: f32 = 2.5;

/// The density's prose line height for a text size, on whole points.
pub fn prose_line(ctx: &egui::Context, size: f32) -> f32 {
    (size * theme::spacing(ctx).prose_line_height).round()
}

/// A heading's line height.
fn heading_line(size: f32) -> f32 {
    (size * HEADING_LEADING).round()
}

/// Heading sizes, always bold, one step under the host's chrome: a description's h1 sits between the pane's h3
/// and h2 (18 / 19, under the 22 detail title), h2 on h3 (16 / 17), h3–h6 at body size. A document heading never
/// outranks the title or the screen's headings it sits under.
pub fn heading_px(sp: &Spacing, level: u8) -> f32 {
    match level {
        0 | 1 => ((sp.h3 + sp.h2) / 2.0).round(),
        2 => sp.h3,
        _ => sp.body,
    }
}

/// The marker slot (left of MARKER_GAP): fixed so text starts LIST_GUTTER in, unless a number is wider.
pub fn marker_slot(widest_number: f32) -> f32 {
    (LIST_GUTTER - MARKER_GAP).max(widest_number.ceil())
}

/// The space between two blocks' boxes, from the density's tokens: `paragraph_gap` by default. A heading
/// attaches to what it introduces: above it two gaps (h1–h2) or one and a half (h3+), below it two line gaps, so
/// its space above reads at least twice its space below, ink to ink. A heading straight under another heading
/// takes one and a half gaps, so the subheading sits closer to its own text than to the heading above it. A rule
/// gets one gap either side, also before a heading (the rule already divides), so it sits midway.
pub fn block_gap(sp: &Spacing, prev: &Block, cur: &Block) -> f32 {
    let para = sp.paragraph_gap;
    match (prev, cur) {
        (Block::Heading { .. }, Block::Heading { .. }) => 1.5 * para,
        (Block::Rule, _) | (_, Block::Rule) => para,
        (_, Block::Heading { level, .. }) => {
            if *level <= 2 {
                2.0 * para
            } else {
                1.5 * para
            }
        }
        (Block::Heading { .. }, _) => heading_after(sp),
        _ => para,
    }
}

/// A heading's box → the next block's box: two line gaps (8 / 12).
fn heading_after(sp: &Spacing) -> f32 {
    2.0 * sp.line_gap
}

/// A table cell's padding: `card_gap` at the sides between columns (none at the table's outer edges), half of
/// it above and below.
fn table_pad(sp: &Spacing) -> egui::Vec2 {
    egui::vec2(sp.card_gap, sp.card_gap / 2.0)
}

pub fn show(ui: &mut Ui, doc: &adf::Doc, p: &Palette, chip: egui::Color32) {
    ui.scope(|ui| {
        // The column: never wider than the density's prose_max_width; left-aligned.
        let full = ui.available_width().min(theme::spacing(ui.ctx()).prose_max_width);
        // The measure: prose lines of about MEASURE_CHARS characters at the body size.
        let w = full.min(measure(ui.ctx()));
        ui.data_mut(|d| d.insert_temp(table_room_id(), full - w));
        ui.set_max_width(w);
        show_blocks(ui, &doc.blocks, p, chip);
    });
}

/// Prose runs at most this many characters of the lowercase sample to a line; running text (capitals, digits,
/// inline code) sets wider, so a line holds about 70 characters (the rubric's 70–80, at its low end).
pub const MEASURE_CHARS: f32 = 75.0;

/// The width of MEASURE_CHARS characters of the sample at the density's body size.
pub fn measure(ctx: &egui::Context) -> f32 {
    const SAMPLE: &str = "the quick brown fox jumps over the lazy dog";
    let body = theme::spacing(ctx).body;
    let w = ctx.fonts_mut(|f| f.layout_no_wrap(SAMPLE.to_string(), theme::body(body), egui::Color32::BLACK).size().x);
    (w / SAMPLE.chars().count() as f32 * MEASURE_CHARS).round()
}

fn table_room_id() -> egui::Id {
    egui::Id::new("adf_view::table_room")
}

/// How much wider than the measure a table may run: up to the document's full column.
fn table_room(ui: &Ui) -> f32 {
    ui.available_width() + ui.data(|d| d.get_temp::<f32>(table_room_id())).unwrap_or(0.0)
}

/// Table rules and the document rule: the edge that holds at least 1.5:1 on the pane in both themes.
fn rule_color(p: &Palette) -> egui::Color32 {
    p.pill_edge()
}

fn show_blocks(ui: &mut Ui, blocks: &[Block], p: &Palette, chip: egui::Color32) {
    // Every gap below is explicit; no implicit item spacing on top of it.
    ui.spacing_mut().item_spacing.y = 0.0;
    let sp = theme::spacing(ui.ctx());
    // A table may run past the measure and widen this ui; every other block keeps the measure.
    let measure = ui.available_width();
    for (i, b) in blocks.iter().enumerate() {
        if i > 0 {
            ui.add_space(block_gap(sp, &blocks[i - 1], b));
        }
        if matches!(b, Block::Table(_)) {
            block(ui, b, p, sp.body, chip);
        } else {
            ui.scope(|ui| {
                ui.set_max_width(measure);
                block(ui, b, p, sp.body, chip);
            });
        }
    }
}

/// From a line box's top to the middle of its lowercase (between the baseline and the x-height) for body text
/// of `size` in a `lh` line: where bullets and task marks centre, so they sit on the text, not the line box.
fn x_mid(ui: &Ui, size: f32, lh: f32) -> f32 {
    let mut job = LayoutJob::default();
    job.append("x", 0.0, TextFormat { font_id: theme::body(size), line_height: Some(lh), ..Default::default() });
    let g = ui.fonts_mut(|f| f.layout_job(job));
    let base = crate::views::galley_baseline(&g);
    let ink = theme::ink_rect(&g);
    if ink.is_positive() { (base + ink.top()) / 2.0 } else { lh / 2.0 }
}

/// One list-like item: a marker painted in its fixed slot by `marker`, then the item's blocks.
#[allow(clippy::too_many_arguments)]
fn list_item(ui: &mut Ui, item: &[Block], p: &Palette, size: f32, chip: egui::Color32, slot: f32, depth: usize, marker: impl FnOnce(&Ui, egui::Rect)) {
    let sp = theme::spacing(ui.ctx());
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = MARKER_GAP;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(slot, prose_line(ui.ctx(), size)), egui::Sense::hover());
        marker(ui, rect);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for (j, ib) in item.iter().enumerate() {
                if j > 0 {
                    ui.add_space(2.0 * sp.line_gap);
                }
                block_at(ui, ib, p, size, chip, depth + 1);
            }
        });
    });
}

fn block(ui: &mut Ui, b: &Block, p: &Palette, size: f32, chip: egui::Color32) {
    block_at(ui, b, p, size, chip, 0);
}

/// One block `depth` lists deep (a nested list's bullets are rings).
fn block_at(ui: &mut Ui, b: &Block, p: &Palette, size: f32, chip: egui::Color32, depth: usize) {
    let sp = theme::spacing(ui.ctx());
    match b {
        Block::Paragraph(s) => spans(ui, s, p, size, false, chip),
        Block::Heading { level, spans: s } => spans(ui, s, p, heading_px(sp, *level), true, chip),
        Block::List { ordered, items } => {
            // The marker is the item's size and muted, in a fixed slot so every list's text starts LIST_GUTTER in.
            let measure = |n: usize| ui.painter().layout_no_wrap(format!("{n}."), theme::body(size), p.muted).size().x;
            let widest = if *ordered { (1..=items.len()).map(measure).fold(0.0, f32::max) } else { 0.0 };
            let slot = marker_slot(widest);
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    ui.add_space(sp.line_gap);
                }
                list_item(ui, item, p, size, chip, slot, depth, |ui, rect| {
                    let lh = prose_line(ui.ctx(), size);
                    if *ordered {
                        // Numbers right-aligned against the gap, so "2." and "10." items start on one edge; laid out
                        // in the item's own line height from the item's top, so the number sits on its baseline.
                        let g = ui.fonts_mut(|f| f.layout_job(number_job(i + 1, size, lh, p.muted)));
                        ui.painter().galley(egui::pos2(rect.right() - g.size().x, rect.top()), g, p.muted);
                    } else {
                        let c = egui::pos2(rect.right() - BULLET_W / 2.0, rect.top() + x_mid(ui, size, lh));
                        if depth.is_multiple_of(2) {
                            ui.painter().circle_filled(c, BULLET_R, p.muted);
                        } else {
                            ui.painter().circle_stroke(c, BULLET_R - 0.5, Stroke::new(1.0, p.muted));
                        }
                    }
                });
            }
        }
        Block::Tasks(items) => {
            let slot = marker_slot(0.0);
            for (i, (done, item)) in items.iter().enumerate() {
                if i > 0 {
                    ui.add_space(sp.line_gap);
                }
                // A done task's text steps back to muted; the box carries the state.
                let q = if *done { Palette { text: p.muted, ..*p } } else { *p };
                list_item(ui, item, &q, size, chip, slot, depth, |ui, rect| paint_task_mark(ui, rect, *done, size, p));
            }
        }
        Block::Code { text, .. } => code_block(ui, text, p, chip, size),
        Block::Quote(bs) => {
            ui.horizontal_top(|ui| {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(3.0, 0.0), egui::Sense::hover());
                let top = rect.top();
                let inner = ui.vertical(|ui| show_blocks(ui, bs, p, chip));
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
            let pad = sp.card_pad as i8;
            Frame::new().fill(bg).stroke(Stroke::new(1.0, border)).corner_radius(theme::RADIUS).inner_margin(Margin::symmetric(pad, pad / 2)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                show_blocks(ui, blocks, p, chip);
            });
        }
        Block::Table(rows) => table(ui, rows, p, chip),
        Block::Rule => {
            let w = ui.available_width();
            let (rect, _) = ui.allocate_exact_size(egui::vec2(w, 1.0), egui::Sense::hover());
            ui.painter().hline(rect.x_range(), rect.center().y, Stroke::new(1.0, rule_color(p)));
        }
    }
}

/// An ordered list's number ("3.") as one row in the item's line height, so its baseline is the item's.
fn number_job(n: usize, size: f32, lh: f32, color: egui::Color32) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.append(&format!("{n}."), 0.0, TextFormat { font_id: theme::body(size), color, line_height: Some(lh), ..Default::default() });
    job
}

/// A task's state as a small square checkbox in the bullets' marker column, centred on the item's lowercase: an
/// open task is an empty muted box; a done one the box filled muted with a check knocked out of it in the
/// surface. Square, so it never reads as a nested list's ring.
fn paint_task_mark(ui: &Ui, slot: egui::Rect, done: bool, size: f32, p: &Palette) {
    let lh = prose_line(ui.ctx(), size);
    let c = egui::pos2(slot.right() - BULLET_W / 2.0, slot.top() + x_mid(ui, size, lh));
    let r = egui::Rect::from_center_size(c, egui::vec2(TASK_MARK, TASK_MARK));
    if done {
        ui.painter().rect_filled(r, TASK_RADIUS, p.muted);
        let pts = [r.lerp_inside(egui::vec2(0.22, 0.52)), r.lerp_inside(egui::vec2(0.43, 0.72)), r.lerp_inside(egui::vec2(0.78, 0.3))];
        ui.painter().line(pts.to_vec(), Stroke::new(1.5, p.surface));
    } else {
        ui.painter().rect_stroke(r.shrink(TASK_STROKE / 2.0), TASK_RADIUS, Stroke::new(TASK_STROKE, p.muted), egui::StrokeKind::Middle);
    }
}

/// A code block: long lines soft-wrap inside the block (`wrap_code`), so nothing is clipped and nothing scrolls
/// sideways; a continuation row hangs CODE_HANG columns in from its line's indent with a muted hook in the hang,
/// so indentation still reads. Laid out here so the fill sits `code_pad` from the ink on every side.
fn code_block(ui: &mut Ui, text: &str, p: &Palette, chip: egui::Color32, size: f32) {
    let w = ui.available_width();
    let pad = code_pad(theme::spacing(ui.ctx()));
    let mono = code_size(size, theme::spacing(ui.ctx()).body);
    let font = theme::mono(mono);
    let lh = (mono * CODE_LEADING).round();
    // The text's room: the fill less its pads, and a point for glyph ink past the advance.
    let room = (w - 2.0 * pad - 1.0).max(1.0);
    let rows = ui.fonts_mut(|f| wrap_code(text, room, |c| f.glyph_width(&font, c)));
    let joined = rows.iter().map(|r| r.text.as_str()).collect::<Vec<_>>().join("\n");
    let mut job = LayoutJob::simple(joined, font.clone(), p.text, f32::INFINITY);
    if let Some(s) = job.sections.first_mut() {
        s.format.line_height = Some(lh);
    }
    let g = ui.fonts_mut(|f| f.layout_job(job));
    let ink = theme::ink_rect(&g);
    let ink = if ink.is_positive() { ink } else { egui::Rect::from_min_size(egui::Pos2::ZERO, g.size()) };
    let bottom = pad - (g.size().y - ink.bottom());
    let (x_height, bearing) = ui.fonts_mut(|f| {
        let mut ink_of = |t: String| theme::ink_rect(&f.layout_no_wrap(t, font.clone(), p.text));
        let x = ink_of("x".into());
        // The left pad is to the ink of the column's first glyphs, not to an indent (a block that is all
        // indented keeps its indent inside the pad).
        let lead = rows.iter().filter_map(|r| r.text.chars().next().filter(|c| !c.is_whitespace())).map(|c| ink_of(c.to_string()).left()).reduce(f32::min);
        (x.height(), lead.unwrap_or(x.left()))
    });
    Frame::new().fill(chip).corner_radius(theme::RADIUS).show(ui, |ui| {
        ui.set_width(w);
        ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
        ui.add_space(pad - ink.top());
        ui.horizontal_top(|ui| {
            ui.add_space(pad - bearing);
            let r = ui.add(egui::Label::new(g.clone()));
            let baseline = crate::views::galley_baseline(&g);
            for (row, laid) in rows.iter().zip(&g.rows) {
                if let Some(x) = row.hang {
                    let base = r.rect.top() + laid.pos.y + baseline;
                    paint_hang_mark(ui, egui::pos2(r.rect.left() + x, base), x_height, ui.fonts_mut(|f| f.glyph_width(&font, ' ')), p);
                }
            }
        });
        ui.add_space(bottom);
    });
}

/// A wrapped code row's continuation rows start this many columns past their line's indent; the hook sits in them.
pub const CODE_HANG: usize = 2;

/// One painted row of a code block: its text (a continuation row starts with its hang's spaces) and, on a
/// continuation row, the x of the hang where the hook is painted (relative to the row's start).
#[derive(Clone, Debug, PartialEq)]
pub struct CodeRow {
    pub text: String,
    pub hang: Option<f32>,
}

/// Code text as rows no wider than `room`, glyph widths from `width`: tabs as four spaces; each line kept as typed
/// when it fits, else broken at the last space (the space goes) or after punctuation (`,;)]}>|&/.=:-`), whichever
/// comes later, else at the last glyph that fits. Continuation rows hang CODE_HANG columns past the line's own indent (the indent
/// drops when that leaves under half the room), so no glyph is ever cut.
pub fn wrap_code(text: &str, room: f32, mut width: impl FnMut(char) -> f32) -> Vec<CodeRow> {
    let space = width(' ');
    let mut out = Vec::new();
    for line in text.replace('\t', "    ").split('\n') {
        let chars: Vec<char> = line.trim_end().chars().collect();
        let indent = chars.iter().take_while(|c| **c == ' ').count();
        let total: f32 = chars.iter().map(|c| width(*c)).sum();
        if total <= room {
            out.push(CodeRow { text: line.trim_end().to_string(), hang: None });
            continue;
        }
        let lead = if (indent + CODE_HANG) as f32 * space <= room / 2.0 { indent } else { 0 };
        let hang_w = (lead + CODE_HANG) as f32 * space;
        let mut start = 0;
        let mut first = true;
        while start < chars.len() {
            let avail = if first { room } else { (room - hang_w).max(space) };
            // How many glyphs fit from `start` (always at least one).
            let mut used = 0.0;
            let mut end = start;
            while end < chars.len() && (end == start || used + width(chars[end]) <= avail) {
                used += width(chars[end]);
                end += 1;
            }
            let brk = if end == chars.len() {
                end
            } else {
                // Text past the indent, so a break never leaves an indent-only row.
                let floor = if first { indent + 1 } else { start + 1 };
                let at_space = (floor..=end).rev().find(|&i| i < chars.len() && chars[i] == ' ' && i > start);
                let at_punct = (floor..end).rev().find(|&i| ",;)]}>|&/.=:-".contains(chars[i - 1]) && i > start);
                at_space.max(at_punct).unwrap_or(end)
            };
            let piece: String = chars[start..brk].iter().collect::<String>().trim_end().to_string();
            if first {
                out.push(CodeRow { text: piece, hang: None });
            } else {
                out.push(CodeRow { text: format!("{}{}", " ".repeat(lead + CODE_HANG), piece.trim_start()), hang: Some(lead as f32 * space) });
            }
            first = false;
            start = brk;
            while start < chars.len() && chars[start] == ' ' {
                start += 1;
            }
        }
    }
    out
}

/// The continuation hook ("↪" drawn as strokes, so every mono face has it) in a hang that starts at `at.x`, on
/// the row whose baseline is `at.y`: down from the x-height, then right with an arrowhead, inside the hang's
/// first column and a half.
fn paint_hang_mark(ui: &Ui, at: egui::Pos2, x_height: f32, column: f32, p: &Palette) {
    let stroke = Stroke::new(1.0, p.muted);
    let x0 = (at.x + 0.25 * column).round() + 0.5;
    let x1 = at.x + 1.45 * column;
    let top = at.y - x_height;
    let mid = (at.y - 0.35 * x_height).round() + 0.5;
    let head = 0.3 * x_height;
    let painter = ui.painter();
    painter.line(vec![egui::pos2(x0, top), egui::pos2(x0, mid), egui::pos2(x1, mid)], stroke);
    painter.line(vec![egui::pos2(x1 - head, mid - head), egui::pos2(x1, mid), egui::pos2(x1 - head, mid + head)], stroke);
}

/// One word of a table cell as the wrap sees it: its width, its widest piece when it must break (an inline-code
/// identifier at its punctuation; a prose word is one piece), the width of the space after it (zero at a soft
/// break or the cell's end) and whether a hard line break follows it.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Word {
    w: f32,
    min: f32,
    space: f32,
    brk: bool,
}

#[cfg(test)]
impl Word {
    fn new(w: f32, space: f32) -> Self {
        Word { w, min: w, space, brk: false }
    }
}

/// How many lines `words` take wrapped greedily at `width`, as egui wraps: a word moves down when it and the
/// space after it would cross the edge; a word wider than a whole line splits over as many lines as it needs.
fn cell_lines(words: &[Word], width: f32) -> usize {
    let width = width.max(1.0);
    let mut lines = 1;
    let mut x: Option<f32> = None;
    let mut gap = 0.0;
    for (i, wd) in words.iter().enumerate() {
        let last = i + 1 == words.len() || wd.brk;
        let tail = if last { 0.0 } else { wd.space };
        match x {
            Some(at) if at + gap + wd.w + tail <= width => x = Some(at + gap + wd.w),
            _ => {
                if x.is_some() {
                    lines += 1;
                }
                let over = (wd.w / width).ceil().max(1.0) as usize - 1;
                lines += over;
                x = Some(wd.w - over as f32 * width);
            }
        }
        gap = wd.space;
        if wd.brk {
            lines += 1;
            x = None;
        }
    }
    lines
}

/// What a cell's text needs beyond its words: the chip pads `layout_paragraph` reserves at the row ends.
const CELL_SLACK: f32 = 2.0 * CODE_PAD_X + 1.0;

/// What breaking one word (an identifier split over two lines) costs the search, in lines of table height.
const BROKEN_WORD_COST: usize = 3;

/// A table's height in lines at column content widths `w`: each row as tall as its tallest cell.
#[cfg(test)]
fn table_height(cells: &[Vec<Vec<Word>>], w: &[f32]) -> usize {
    cells.iter().map(|r| r.iter().zip(w).map(|(c, w)| cell_lines(c, w - CELL_SLACK)).max().unwrap_or(1)).sum()
}

/// How many words are wider than their column at widths `w`, so they break.
fn broken_words(cells: &[Vec<Vec<Word>>], w: &[f32]) -> usize {
    cells.iter().map(|r| r.iter().zip(w).map(|(c, w)| c.iter().filter(|x| x.w > w - CELL_SLACK + 0.01).count()).sum::<usize>()).sum()
}

/// The search's score for widths `w`, lower is better: the table's height plus BROKEN_WORD_COST for each word
/// that breaks, then how unevenly the height is spread (the rows' squared heights: one ten-line row loses to two
/// five-line rows).
fn table_score(cells: &[Vec<Vec<Word>>], w: &[f32]) -> (usize, usize) {
    let (h, sq) = cells.iter().fold((0, 0), |(h, sq), r| {
        let n = r.iter().zip(w).map(|(c, w)| cell_lines(c, w - CELL_SLACK)).max().unwrap_or(1);
        (h + n, sq + n * n)
    });
    (h + BROKEN_WORD_COST * broken_words(cells, w), sq)
}

/// Column content widths for a table `avail` wide (padding already taken off), from each cell's words
/// (`cells[row][col]`). When every cell fits unwrapped, columns take their natural widths and share the rest in
/// proportion. Otherwise the widths are the split that makes the table shortest when the cells are really
/// wrapped, word by word (`cell_lines`), with each word that would break (an inline-code identifier wider than
/// its column) counted as BROKEN_WORD_COST more lines: the wordy column gets the room, so no cell runs ten lines
/// beside a two-line neighbour, and identifiers stay whole wherever the room allows. No column is narrower
/// than its widest piece. When not even the pieces fit, they are scaled down to the room, so a table is never
/// wider than its column.
fn table_widths(cells: &[Vec<Vec<Word>>], avail: f32) -> Vec<f32> {
    let cols = cells.iter().map(Vec::len).max().unwrap_or(0);
    let mut mins = vec![0.0f32; cols];
    let mut nats = vec![0.0f32; cols];
    for row in cells {
        for (c, cell) in row.iter().enumerate() {
            let piece = cell.iter().map(|x| x.min).fold(0.0, f32::max);
            mins[c] = mins[c].max((piece + CELL_SLACK).ceil());
            nats[c] = nats[c].max((natural_line(cell) + CELL_SLACK).ceil());
        }
    }
    let (sum_min, sum_nat): (f32, f32) = (mins.iter().sum(), nats.iter().sum());
    if sum_nat <= avail {
        let extra = avail - sum_nat;
        return nats.iter().map(|n| n + if sum_nat > 0.0 { extra * n / sum_nat } else { extra / cols as f32 }).collect();
    }
    if sum_min > avail {
        return mins.iter().map(|m| m * avail.max(0.0) / sum_min).collect();
    }
    let spare = avail - sum_min;
    let score = |w: &[f32]| table_score(cells, w);
    // Starts: the spare room in proportion to what each column lacks, and shared evenly.
    let lack: Vec<f32> = nats.iter().zip(&mins).map(|(n, m)| n - m).collect();
    let lack_sum: f32 = lack.iter().sum();
    let mut starts: Vec<Vec<f32>> =
        vec![mins.iter().zip(&lack).map(|(m, l)| m + spare * l / lack_sum.max(1.0)).collect(), mins.iter().map(|m| m + spare / cols as f32).collect()];
    // Up to three columns, the best split on a coarse grid as well.
    if (2..=3).contains(&cols) {
        let step = (spare / 40.0).max(2.0);
        let n = (spare / step).floor() as usize;
        let mut best: Option<((usize, usize), Vec<f32>)> = None;
        let mut consider = |w: Vec<f32>| {
            let s = score(&w);
            if best.as_ref().is_none_or(|(b, _)| s < *b) {
                best = Some((s, w));
            }
        };
        for a in 0..=n {
            let x = a as f32 * step;
            if cols == 2 {
                consider(vec![mins[0] + x, mins[1] + spare - x]);
                continue;
            }
            for b in 0..=(n - a) {
                let y = b as f32 * step;
                consider(vec![mins[0] + x, mins[1] + y, mins[2] + spare - x - y]);
            }
        }
        starts.extend(best.map(|(_, w)| w));
    }
    // Each start refined by moving width between two columns, in halving steps, while that improves the score.
    let refine = |mut w: Vec<f32>| {
        let mut s = score(&w);
        let mut d = (spare / 4.0).max(1.0);
        while d >= 1.0 {
            let mut moved = true;
            while moved {
                moved = false;
                for a in 0..cols {
                    for b in 0..cols {
                        if a == b || w[b] - d < mins[b] {
                            continue;
                        }
                        let mut t = w.clone();
                        t[a] += d;
                        t[b] -= d;
                        let ts = score(&t);
                        if ts < s {
                            (w, s, moved) = (t, ts, true);
                        }
                    }
                }
            }
            d /= 2.0;
        }
        (s, w)
    };
    starts.into_iter().map(refine).min_by(|a, b| a.0.cmp(&b.0)).map(|(_, w)| w).unwrap_or(mins)
}

/// A cell's longest unwrapped line.
fn natural_line(words: &[Word]) -> f32 {
    let (mut line, mut longest, mut gap) = (0.0f32, 0.0f32, 0.0);
    for wd in words {
        line += if line > 0.0 { gap } else { 0.0 } + wd.w;
        longest = longest.max(line);
        gap = wd.space;
        if wd.brk {
            line = 0.0;
        }
    }
    longest
}

/// Where an inline-code word may break when it is wider than its column: after any of these.
const CODE_PIECE_ENDS: [char; 6] = ['/', '.', ':', '-', '_', '='];

/// A cell's words as `cell_lines` wraps them: split at breakable whitespace (not U+00A0, so short code stays
/// whole), carried across span edges ("`x`," is one word), each measured in its span's font, a chip with its
/// pads; a code word's `min` is its widest piece between CODE_PIECE_ENDS.
fn cell_words(ui: &Ui, s: &[Span], p: &Palette, size: f32, header: bool) -> Vec<Word> {
    let s = prepare(s);
    let lh = table_line(ui.ctx());
    let body = theme::spacing(ui.ctx()).body;
    ui.fonts_mut(|f| {
        let fit = MonoFit::measure(f, size, header, lh, body);
        let mut out: Vec<Word> = Vec::new();
        // The word so far: its width, its widest piece, and the piece still open at its end.
        let (mut word, mut min, mut open) = (0.0f32, 0.0f32, 0.0f32);
        for (i, sp) in s.iter().enumerate() {
            let fmt = span_format(sp, p, size, header, &fit, lh);
            let mut width = |t: &str| f.layout_no_wrap(t.to_string(), fmt.font_id.clone(), p.text).size().x;
            let space = width(" ");
            let lead = span_lead(&s, i);
            word += lead;
            open += lead;
            let mut rest = sp.text.as_str();
            loop {
                let cut = rest.find(|c: char| (c.is_whitespace() && c != '\u{a0}') || c == '\n');
                let part = &rest[..cut.unwrap_or(rest.len())];
                if !part.is_empty() {
                    word += width(part);
                    if sp.code {
                        let mut pieces = part.split_inclusive(CODE_PIECE_ENDS);
                        if let Some(first) = pieces.next() {
                            open += width(first);
                            for piece in pieces {
                                min = min.max(open);
                                open = width(piece);
                            }
                        }
                    } else {
                        open += width(part);
                    }
                }
                let Some(at) = cut else { break };
                let c = rest[at..].chars().next().unwrap_or(' ');
                let gap = if c == SOFT_BREAK { 0.0 } else { space };
                if word > 0.0 || c == '\n' {
                    out.push(Word { w: word, min: min.max(open), space: gap, brk: c == '\n' });
                } else if let Some(last) = out.last_mut() {
                    last.space += gap;
                }
                (word, min, open) = (0.0, 0.0, 0.0);
                rest = &rest[at + c.len_utf8()..];
            }
        }
        if word > 0.0 {
            out.push(Word { w: word, min: min.max(open), space: 0.0, brk: false });
        }
        out
    })
}

/// A table cell's line height: the prose leading at the cells' size.
fn table_line(ctx: &egui::Context) -> f32 {
    prose_line(ctx, theme::spacing(ctx).small)
}

/// A table typeset to the prose column: columns sized by content (`table_widths`), cells top-aligned with
/// `table_pad` padding, a bold header row over a rule, hairlines between rows. Never wider than the column, so
/// it never scrolls sideways.
fn table(ui: &mut Ui, rows: &[Vec<Vec<Span>>], p: &Palette, chip: egui::Color32) {
    let sp = theme::spacing(ui.ctx());
    let size = sp.small;
    let pad = table_pad(sp);
    let cols = rows.iter().map(Vec::len).max().unwrap_or(0);
    if cols == 0 {
        return;
    }
    // Header cells come bold from the parser; a first row that is all bold is the header.
    let header = rows
        .first()
        .is_some_and(|r| r.iter().flatten().filter(|s| !s.text.trim().is_empty()).all(|s| s.bold) && r.iter().flatten().any(|s| !s.text.trim().is_empty()));
    // A table is not prose: it may run the full column, past the measure.
    let avail = table_room(ui);
    // Gutters between columns only: the first column's text sits on the prose edge, the last ends on the column's.
    let side = |ci: usize| (if ci == 0 { 0.0 } else { pad.x }, if ci + 1 == cols { 0.0 } else { pad.x });
    let pads = (cols - 1) as f32 * 2.0 * pad.x;
    let words: Vec<Vec<Vec<Word>>> = rows
        .iter()
        .enumerate()
        .map(|(ri, row)| (0..cols).map(|ci| row.get(ci).map_or_else(Vec::new, |c| cell_words(ui, c, p, size, header && ri == 0))).collect())
        .collect();
    let lh = table_line(ui.ctx());
    let widths = table_widths(&words, avail - pads);
    let total: f32 = widths.iter().sum::<f32>() + pads;
    let draw = |ui: &mut Ui| {
        ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
        let left = ui.cursor().left();
        for (ri, row) in rows.iter().enumerate() {
            let r = ui.horizontal_top(|ui| {
                for (ci, w) in widths.iter().enumerate() {
                    let (l, r) = side(ci);
                    ui.allocate_ui_with_layout(egui::vec2(w + l + r, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                        ui.set_width(w + l + r);
                        ui.add_space(pad.y);
                        ui.horizontal_top(|ui| {
                            ui.add_space(l);
                            ui.vertical(|ui| {
                                ui.set_width(*w);
                                if let Some(cell) = row.get(ci) {
                                    spans_in(ui, cell, p, size, header && ri == 0, chip, lh);
                                }
                            });
                        });
                        ui.add_space(pad.y);
                    });
                }
            });
            let y = r.response.rect.bottom();
            if ri + 1 < rows.len() {
                ui.painter().hline(egui::Rangef::new(left, left + total), y, Stroke::new(1.0, rule_color(p)));
            }
        }
    };
    ui.scope(draw);
}

/// The text format one span is painted with. Links are accent, underlined by `spans`. Inline code is mono at
/// 0.9× the surrounding size; a smart-link key is a mono chip and a mention a chip in body type, at the same step. Chips are
/// painted under the text by `spans`; their runs sit on the body's baseline (`fit`, bottom-aligned).
fn span_format(sp: &Span, p: &Palette, size: f32, heading: bool, fit: &MonoFit, lh: f32) -> TextFormat {
    let mut fmt = TextFormat {
        font_id: if sp.bold || heading { theme::bold(size) } else { theme::body(size) },
        color: p.text,
        line_height: Some(lh),
        ..Default::default()
    };
    if sp.code {
        fmt.font_id = theme::mono(code_size(size, fit.body));
        fmt.line_height = Some(fit.code);
        fmt.valign = egui::Align::Max;
    }
    if is_key_link(sp) {
        // An issue key on a chip is set like every issue key in the app and like code on a chip: mono.
        fmt.font_id = theme::mono(code_size(size, fit.body));
        fmt.line_height = Some(fit.code);
        fmt.valign = egui::Align::Max;
    } else if is_mention(sp) {
        fmt.font_id = theme::body(code_size(size, fit.body));
        fmt.line_height = Some(fit.key);
        fmt.valign = egui::Align::Max;
    }
    if sp.italic {
        fmt.italics = true;
    }
    if sp.strike {
        fmt.strikethrough = Stroke::new(1.0, p.muted);
    }
    if sp.placeholder {
        fmt.color = p.placeholder();
    } else if sp.link.is_some() && !is_card(sp) {
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
/// char of the paragraph and whether it is in an underlined link, in text order.
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
                // The glyph's ink, on the pixel grid as `theme::ink_rect` puts it; a space keeps its advance
                // (runs drop it at their ends).
                let lr = gl.logical_rect();
                if gl.chr.is_whitespace() || gl.uv_rect.is_nothing() || gl.uv_rect.size.x <= 0.0 {
                    (link, gl.chr.is_whitespace(), row.pos.x + lr.left(), row.pos.x + lr.right())
                } else {
                    let ppp = g.pixels_per_point.max(1.0);
                    let x = row.pos.x + ((gl.pos.x + gl.uv_rect.offset.x) * ppp).round() / ppp;
                    (link, false, x, x + gl.uv_rect.size.x)
                }
            })
            .collect();
        if let Some(b) = baseline {
            out.extend(underline_runs(&boxes).into_iter().map(|(l, r)| (egui::Rangef::new(l, r), b)));
        }
    }
    out
}

/// The line heights inline code and chips take in one paragraph so their baselines meet the body's, and the
/// body size they were measured against.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MonoFit {
    pub code: f32,
    /// Smart-link and mention chips.
    pub key: f32,
    /// The density's body size, which caps inline code (`code_size`).
    pub body: f32,
}

impl MonoFit {
    /// Measures the fonts of a context of `size` with line height `lh` in a document whose body is `body`: each
    /// run's baseline from its own row top.
    pub fn measure(fonts: &mut egui::epaint::text::FontsView<'_>, size: f32, heading: bool, lh: f32, body: f32) -> Self {
        let mut baseline = |font: egui::FontId, lh: Option<f32>| {
            let mut job = LayoutJob::default();
            job.append("x", 0.0, TextFormat { font_id: font, line_height: lh, ..Default::default() });
            let g = fonts.layout_job(job);
            g.rows.first().and_then(|r| r.row.glyphs.first()).map_or(0.0, |gl| gl.pos.y)
        };
        let small = code_size(size, body);
        let base = baseline(if heading { theme::bold(size) } else { theme::body(size) }, Some(lh));
        let code = baseline(theme::mono(small), None);
        let key = baseline(theme::body(small), None);
        MonoFit { code: mono_line_height(lh, base, code), key: mono_line_height(lh, base, key), body }
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

/// Inline code's size in a context of `size` whose body is `body`: 0.9×, at most 0.9× body.
pub fn code_size(size: f32, body: f32) -> f32 {
    CODE_SCALE * size.min(body)
}

/// A smart link that shows an issue key (an inlineCard to /browse/KEY).
fn is_key_link(sp: &Span) -> bool {
    sp.link.as_deref().and_then(adf::browse_key) == Some(sp.text.as_str())
}

/// A mention as the parser gives it: bold "@Name", no link.
fn is_mention(sp: &Span) -> bool {
    sp.bold && sp.link.is_none() && !sp.code && sp.text.starts_with('@')
}

/// A span drawn as a compact chip: a smart-link issue key or a mention.
fn is_card(sp: &Span) -> bool {
    is_key_link(sp) || is_mention(sp)
}

/// One glyph on a laid-out row, for `code_runs`: (char, in a chip, ink left, ink right, top, bottom). The
/// x edges are the glyph's ink (its advance box for whitespace); top and bottom its font box.
pub type GlyphBox = (char, bool, f32, f32, f32, f32);

/// One inline chip on a row: its glyphs' ink (left, right) and font box (top, bottom).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CodeRun {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
}

/// The inline chips on one laid-out row: one per run of chip glyphs, with whitespace at either end of
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
            out.push(CodeRun { left: run[0].2, right: run[n - 1].3, top, bottom });
        }
    }
    out
}

/// How far to move a laid-out row right so a chip that starts it keeps its pad inside the text column
/// (its fill starts at the text edge) instead of hanging into the margin.
pub fn row_inset(glyphs: &[GlyphBox]) -> f32 {
    match glyphs.first() {
        Some(&(c, true, left, ..)) if !c.is_whitespace() => (CODE_PAD_X - left).max(0.0),
        _ => 0.0,
    }
}

/// Punctuation that hugs the code after it ("(`x`").
pub fn opens(c: char) -> bool {
    matches!(c, '(' | '[' | '{' | '"' | '\'' | '“' | '‘' | '/')
}

/// Punctuation that hugs the code before it ("`x`),").
pub fn closes(c: char) -> bool {
    matches!(c, ')' | ']' | '}' | ',' | '.' | ';' | ':' | '!' | '?' | '"' | '\'' | '”' | '’' | '/')
}

/// The first-pass leading space before span `i`, at a chip junction: the chip's pad (punctuation that hugs
/// the chip gets none: the fill may touch it). `layout_paragraph` then sets each code junction from the laid-out
/// ink (`junction_shift`); the text's own whitespace is never touched.
pub fn span_lead(s: &[Span], i: usize) -> f32 {
    let chip = |q: &Span| q.code || is_card(q);
    let sp = &s[i];
    let prev = i.checked_sub(1).map(|j| &s[j]);
    if chip(sp) && !prev.is_some_and(chip) {
        let hugged = prev.is_some_and(|q| q.text.chars().last().is_some_and(opens));
        if hugged { CODE_PAD_TIGHT } else { CODE_PAD_X }
    } else if !chip(sp) && prev.is_some_and(chip) {
        if sp.text.chars().next().is_some_and(closes) { CODE_PAD_TIGHT } else { CODE_PAD_X }
    } else {
        0.0
    }
}

/// The first pass's lead where punctuation hugs the code.
const CODE_PAD_TIGHT: f32 = 1.0;

/// Ink of a neighbour that touches a chip (punctuation, a letter) → the chip's fill: none, so a comma or a
/// bracket reads as attached to the code, never as a word of its own.
pub const CHIP_ABUT: f32 = 0.0;

/// One glyph at a code junction: (whitespace, ink left, ink right, advance left, advance right), in galley x.
pub type JunctionGlyph = (bool, f32, f32, f32, f32);

/// How far to move the glyphs after a code junction so the chip's fill (CODE_PAD_X outside its ink) meets its
/// neighbour right: a space keeps its advance outside the fill, anything else sits CHIP_ABUT off it.
/// `a` is the glyph before the junction, `b` the one after; `into_code` when `b` starts the code.
pub fn junction_shift(a: JunctionGlyph, b: JunctionGlyph, into_code: bool) -> f32 {
    if into_code {
        let want = if a.0 { a.4 } else { a.2 + CHIP_ABUT };
        want - (b.1 - CODE_PAD_X)
    } else {
        // The fill ends CODE_PAD_X right of a's ink.
        let fill = a.2 + CODE_PAD_X;
        if b.0 { fill - b.3 } else { fill + CHIP_ABUT - b.1 }
    }
}

/// One paragraph's layout job, and per char whether it is in a chip (inline code or a card).
pub fn paragraph_job(s: &[Span], p: &Palette, size: f32, heading: bool, fit: &MonoFit, lh: f32) -> (LayoutJob, Vec<(char, bool)>) {
    let mut job = LayoutJob::default();
    let mut chars: Vec<(char, bool)> = Vec::new();
    for (i, sp) in s.iter().enumerate() {
        let fmt = span_format(sp, p, size, heading, fit, lh);
        let chip = sp.code || is_card(sp);
        // A soft break is a space set too small to see: the wrap's preferred break, in an otherwise unbroken token.
        let soft = TextFormat { font_id: egui::FontId::new(SOFT_BREAK_SIZE, fmt.font_id.family.clone()), ..fmt.clone() };
        for (k, piece) in sp.text.split(SOFT_BREAK).enumerate() {
            if k > 0 {
                job.append(" ", 0.0, soft.clone());
                chars.push((' ', chip));
            }
            job.append(piece, if k == 0 { span_lead(s, i) } else { 0.0 }, fmt.clone());
            chars.extend(piece.chars().map(|c| (c, chip)));
        }
    }
    (job, chars)
}

/// Marks a break opportunity inside a long inline-code token (`soft_break_long_code`); painted as a space of
/// SOFT_BREAK_SIZE.
pub const SOFT_BREAK: char = '\u{200a}';
/// A soft break's font size: its advance is a fraction of a point.
const SOFT_BREAK_SIZE: f32 = 1.0;

/// Inline-code tokens of CODE_KEEP_WHOLE characters or more may break after a `/` or `_` (there first, as at a
/// space): `spans` with a SOFT_BREAK after each such character. Shorter tokens move whole to the next line.
pub fn soft_break_long_code(spans: Vec<Span>) -> Vec<Span> {
    spans
        .into_iter()
        .map(|sp| {
            if !sp.code || !sp.text.split_whitespace().any(|t| t.chars().count() >= CODE_KEEP_WHOLE) {
                return sp;
            }
            let mut text = String::with_capacity(sp.text.len() + 8);
            for (i, tok) in sp.text.split(' ').enumerate() {
                if i > 0 {
                    text.push(' ');
                }
                let long = tok.chars().count() >= CODE_KEEP_WHOLE;
                let n = tok.chars().count();
                for (k, c) in tok.chars().enumerate() {
                    text.push(c);
                    if long && matches!(c, '/' | '_') && k + 1 < n {
                        text.push(SOFT_BREAK);
                    }
                }
            }
            Span { text, ..sp }
        })
        .collect()
}

/// Each glyph of a galley in text order: (char index, row, ink left, ink right, advance left, advance right),
/// x relative to the galley. Rows drop a wrapping newline, so the chars are walked alongside.
fn glyph_index(g: &egui::Galley, chars: &[(char, bool)]) -> Vec<(usize, usize, f32, f32, f32, f32)> {
    let mut out = Vec::new();
    let mut at = 0;
    for (ri, row) in g.rows.iter().enumerate() {
        for gl in &row.row.glyphs {
            while at < chars.len() && chars[at].0 != gl.chr {
                at += 1;
            }
            let lr = gl.logical_rect();
            let (l, r) = (row.pos.x + lr.left(), row.pos.x + lr.right());
            let (il, ir) = if gl.uv_rect.is_nothing() || gl.uv_rect.size.x <= 0.0 {
                (l, r)
            } else {
                let x = row.pos.x + gl.pos.x + gl.uv_rect.offset.x;
                (x, x + gl.uv_rect.size.x)
            };
            out.push((at, ri, il, ir, l, r));
            at += 1;
        }
    }
    out
}

/// The char under `pos` (relative to the galley), if `pos` is on a glyph.
fn char_at(g: &egui::Galley, chars: &[(char, bool)], pos: egui::Pos2) -> Option<usize> {
    let index = glyph_index(g, chars);
    let mut k = 0;
    for row in &g.rows {
        let y = row.pos.y..=row.pos.y + row.row.size.y;
        for _ in &row.row.glyphs {
            let (ci, _, _, _, l, r) = index[k];
            k += 1;
            if y.contains(&pos.y) && (l..=r).contains(&pos.x) {
                return Some(ci);
            }
        }
    }
    None
}

/// Sets every code junction's leading space from a first layout's ink (`junction_shift`), so each chip's fill
/// sits CODE_PAD_X outside its glyphs' ink on both sides.
fn fit_junctions(fonts: &mut egui::epaint::text::FontsView<'_>, job: &mut LayoutJob, chars: &[(char, bool)]) {
    let first = fonts.layout_job(job.clone());
    let glyphs = glyph_index(&first, chars);
    let text = job.text.clone();
    for sec in job.sections.iter_mut() {
        if sec.leading_space <= 0.0 {
            continue;
        }
        let k = text[..sec.byte_range.start.0].chars().count();
        let (Some(a), Some(b)) = (glyphs.iter().find(|g| g.0 + 1 == k), glyphs.iter().find(|g| g.0 == k)) else { continue };
        let (Some(ca), Some(cb)) = (chars.get(k - 1), chars.get(k)) else { continue };
        if a.1 != b.1 || ca.1 == cb.1 {
            continue;
        }
        let jg = |g: &(usize, usize, f32, f32, f32, f32), c: char| (c.is_whitespace(), g.2, g.3, g.4, g.5);
        sec.leading_space = (sec.leading_space + junction_shift(jg(a, ca.0), jg(b, cb.0), cb.1)).max(0.0);
    }
}

/// Lays `job` out in `width`: chip junctions fitted to the ink (`fit_junctions`), rows that start with a chip
/// move in by its pad, every row takes at least the line height (`row_tops`), and the chips' rects
/// (relative to the galley, CODE_PAD_X outside their ink) come back with it.
pub fn layout_paragraph(
    fonts: &mut egui::epaint::text::FontsView<'_>,
    mut job: LayoutJob,
    chars: &[(char, bool)],
    width: f32,
    line_h: f32,
) -> (egui::Galley, Vec<egui::Rect>) {
    // Narrower by the pads so a row moved in by `row_inset` (and a chip's pad at the row end) still fits.
    job.wrap.max_width = (width - 2.0 * CODE_PAD_X).max(1.0);
    fit_junctions(fonts, &mut job, chars);
    let laid = fonts.layout_job(job);
    let mut g = (*laid).clone();
    // A word longer than a whole row (a long path in code) makes egui wrap once before it, leaving an empty row,
    // then split it: drop that empty row so no blank line opens up.
    // (The rect's height is reset from the rows below.)
    // A row of only spaces (a space that alone crossed the edge after a full-width word) is blank too.
    g.rows.retain(|r| r.row.glyphs.iter().any(|gl| !gl.chr.is_whitespace()) || r.ends_with_newline);
    if g.rows.is_empty() {
        g.rows = laid.rows.clone();
    }
    let (tops, height) = row_tops(&g.rows.iter().map(|r| r.row.size.y).collect::<Vec<_>>(), line_h);
    let dy = height - g.rect.height();
    let index = glyph_index(&g, chars);
    let mut runs: Vec<(f32, CodeRun)> = Vec::new();
    let mut at = 0;
    for (ri, (row, top)) in g.rows.iter_mut().zip(tops).enumerate() {
        row.pos.y = top;
        let boxes: Vec<GlyphBox> = row
            .row
            .glyphs
            .iter()
            .map(|gl| {
                let (ci, _, il, ir, ..) = index[at];
                debug_assert_eq!(index[at].1, ri);
                at += 1;
                let chip = chars.get(ci).is_some_and(|c| c.1);
                // The run's own font box (not its fitted line height), so the chip hugs the glyphs.
                let glyph_top = gl.pos.y - gl.font_ascent;
                (gl.chr, chip, il - row.pos.x, ir - row.pos.x, glyph_top, glyph_top + gl.font_height)
            })
            .collect();
        // A row that starts with a space (the wrap broke just before it) starts at its first visible glyph.
        let lead = boxes.iter().take_while(|b| b.0.is_whitespace() && b.0 != '\u{a0}').count();
        let pull = if lead > 0 && lead < boxes.len() { row.row.glyphs[lead].pos.x - row.row.glyphs[0].pos.x } else { 0.0 };
        let visible: Vec<GlyphBox> = boxes[lead.min(boxes.len())..].iter().map(|b| (b.0, b.1, b.2 - pull, b.3 - pull, b.4, b.5)).collect();
        let shift = row_inset(&visible) - pull;
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
    let chips = runs
        .into_iter()
        .map(|(y, run)| {
            egui::Rect::from_min_max(
                egui::pos2(run.left - CODE_PAD_X, y + run.top - CODE_PAD_Y),
                egui::pos2(run.right + CODE_PAD_X, y + run.bottom + CODE_PAD_Y),
            )
        })
        .collect();
    (g, chips)
}

/// Inline code shorter than this many characters never splits across a wrap; it moves whole to the next line.
pub const CODE_KEEP_WHOLE: usize = 30;

/// `spans` with the spaces inside every short inline-code span (and every card) made no-break (U+00A0), so the
/// wrap moves the chip whole; longer code still wraps at its spaces.
pub fn keep_short_code_whole(spans: Vec<Span>) -> Vec<Span> {
    spans
        .into_iter()
        .map(|sp| if (sp.code && sp.text.chars().count() < CODE_KEEP_WHOLE) || is_card(&sp) { Span { text: sp.text.replace(' ', "\u{a0}"), ..sp } } else { sp })
        .collect()
}

/// The spans as painted: bare URLs linked, long ones labelled short and neighbours spaced (`adf::tidy_links`),
/// code tidied and short code kept whole.
fn prepare(s: &[Span]) -> Vec<Span> {
    pad_placeholders(keep_short_code_whole(soft_break_long_code(adf::tidy_code_spans(&adf::tidy_links(&adf::linkify(s))))))
}

/// A placeholder ("Image · logo.png") reads as one quiet chip: kept whole, with a no-break space inside either
/// end for its outline's padding (`spans_in` draws the outline round it).
fn pad_placeholders(spans: Vec<Span>) -> Vec<Span> {
    spans
        .into_iter()
        .map(|sp| if sp.placeholder { Span { text: format!("\u{a0}{}\u{a0}", sp.text.trim().replace(' ', "\u{a0}")), ..sp } } else { sp })
        .collect()
}

/// Each placeholder's outline in galley coordinates, one rect per row it sits on: its glyphs' advances (its
/// no-break pads included), the row's font box CODE_PAD_Y over and under.
fn placeholder_rects(g: &egui::Galley, chars: &[(char, bool)], owner: &[usize], s: &[Span]) -> Vec<egui::Rect> {
    let index = glyph_index(g, chars);
    let mut out: Vec<(usize, usize, egui::Rect)> = Vec::new();
    let mut k = 0;
    for (ri, row) in g.rows.iter().enumerate() {
        for gl in &row.row.glyphs {
            let (ci, _, _, _, l, r) = index[k];
            k += 1;
            let Some(&si) = owner.get(ci) else { continue };
            if !s[si].placeholder {
                continue;
            }
            let top = row.pos.y + gl.pos.y - gl.font_ascent - CODE_PAD_Y;
            let rect = egui::Rect::from_min_max(egui::pos2(l, top), egui::pos2(r, top + gl.font_height + 2.0 * CODE_PAD_Y));
            match out.last_mut() {
                Some((a, b, acc)) if *a == si && *b == ri => *acc = acc.union(rect),
                _ => out.push((si, ri, rect)),
            }
        }
    }
    out.into_iter().map(|(.., r)| r).collect()
}

/// Which span each char of `s` belongs to.
fn span_of_char(s: &[Span]) -> Vec<usize> {
    s.iter().enumerate().flat_map(|(i, sp)| std::iter::repeat_n(i, sp.text.chars().count())).collect()
}

fn spans(ui: &mut Ui, s: &[Span], p: &Palette, size: f32, heading: bool, chip: egui::Color32) {
    let lh = if heading { heading_line(size) } else { prose_line(ui.ctx(), size) };
    spans_in(ui, s, p, size, heading, chip, lh);
}

/// `spans` in a line height of `lh`.
fn spans_in(ui: &mut Ui, s: &[Span], p: &Palette, size: f32, heading: bool, chip: egui::Color32, lh: f32) {
    let s = prepare(s);
    let has_link = s.iter().any(|sp| sp.link.is_some());
    let body = theme::spacing(ui.ctx()).body;
    let fit = ui.fonts_mut(|f| MonoFit::measure(f, size, heading, lh, body));
    let (job, chars) = paragraph_job(&s, p, size, heading, &fit, lh);
    let sense = if has_link { egui::Sense::click() } else { egui::Sense::hover() };
    let under = ui.painter().add(egui::Shape::Noop);
    let width = ui.available_width();
    let (g, rects) = ui.fonts_mut(|f| layout_paragraph(f, job, &chars, width, lh));
    let g = std::sync::Arc::new(g);
    let r = ui.add(egui::Label::new(g.clone()).sense(sense));
    let origin = r.rect.min.to_vec2();
    // Chips under the text: code in the chip step, cards in the accent tint.
    let owner = span_of_char(&s);
    // A chip's kind from the char at its centre.
    let card_at = |rc: &egui::Rect| char_at(&g, &chars, rc.center()).and_then(|ci| owner.get(ci)).is_some_and(|&i| is_card(&s[i]));
    let mut shapes: Vec<egui::Shape> = rects
        .iter()
        .map(|rc| {
            let fill = if card_at(rc) { p.accent_subtle() } else { chip };
            egui::Shape::rect_filled(rc.translate(origin), CHIP_RADIUS, fill)
        })
        .collect();
    if s.iter().any(|sp| sp.placeholder) {
        let edge = Stroke::new(1.0, p.pill_edge());
        for rc in placeholder_rects(&g, &chars, &owner, &s) {
            shapes.push(egui::Shape::rect_stroke(rc.translate(origin), CHIP_RADIUS, edge, egui::StrokeKind::Inside));
        }
    }
    ui.painter().set(under, egui::Shape::Vec(shapes));
    if has_link {
        let links: Vec<(char, bool)> =
            s.iter().flat_map(|sp| sp.text.chars().map(move |c| (c, sp.link.is_some() && !is_card(sp) && !sp.placeholder))).collect();
        for (x, baseline) in link_underlines(&g, &links) {
            let y = underline_y(baseline + origin.y);
            ui.painter().hline(egui::Rangef::new(x.min + origin.x, x.max + origin.x), y, Stroke::new(1.0, p.accent));
        }
        // The link under the pointer: its full URL on hover, opened on click.
        let hovered = r.hover_pos().and_then(|pos| char_at(&g, &chars, pos - origin)).and_then(|ci| owner.get(ci)).and_then(|&i| s[i].link.clone());
        if let Some(url) = hovered {
            let clicked = r.clicked();
            r.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text_at_pointer(url.clone());
            if clicked && crate::app::openable(&url) {
                ui.ctx().open_url(egui::OpenUrl::new_tab(url));
            }
        }
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
            selected_row: c(14),
            hover_row: c(15),
            danger_subtle: c(16),
            tooltip: c(17),
            tooltip_text: c(19),
        }
    }

    const LH: f32 = 21.0;
    const FIT: MonoFit = MonoFit { code: 18.0, key: 18.5, body: 14.0 };

    fn span(text: &str) -> Span {
        Span { text: text.into(), ..Default::default() }
    }

    fn with_fonts<R>(f: impl FnOnce(&mut egui::epaint::text::FontsView<'_>) -> R) -> R {
        crate::testutil::with_ui(|ui| ui.fonts_mut(f))
    }

    fn walk_lines(shapes: &[egui::epaint::ClippedShape]) -> Vec<[egui::Pos2; 2]> {
        fn walk(s: &egui::Shape, out: &mut Vec<[egui::Pos2; 2]>) {
            match s {
                egui::Shape::LineSegment { points, .. } => out.push(*points),
                egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
                _ => {}
            }
        }
        let mut lines = Vec::new();
        shapes.iter().for_each(|c| walk(&c.shape, &mut lines));
        lines
    }

    #[test]
    fn code_blocks_pad_12_to_the_ink_and_wrap_long_lines() {
        const CODE_PAD: f32 = 12.0;
        assert_eq!(code_pad(&theme::COMPACT), CODE_PAD, "compact keeps 12");
        assert_eq!(code_pad(&theme::COMFORTABLE), theme::COMFORTABLE.card_pad, "comfortable follows the card pad");
        // 12 from the fill to the ink above, left and below, with the real fonts at the captures' density.
        let p = crate::theme::LIGHT;
        let chip = theme::chip_fill_on(p.surface, &p);
        for text in ["{\"matcher\": {\"id\": \"byName\"},\n \"properties\": [{\"id\": \"unit\"}]}", "make test", "HELLO world"] {
            let shapes = crate::testutil::paint_at2(egui::vec2(800.0, 400.0), |ui| {
                ui.set_width(500.0);
                block(ui, &Block::Code { text: text.into(), lang: None }, &p, 14.0, chip);
            });
            let (fill, _) = *crate::testutil::fills(&shapes).iter().find(|(_, c)| *c == chip).expect("the block's fill");
            let ink = crate::testutil::inks(&shapes).into_iter().map(|(_, r)| r).reduce(|a, b| a.union(b)).expect("ink");
            for (side, gap) in [("top", ink.top() - fill.top()), ("left", ink.left() - fill.left()), ("bottom", fill.bottom() - ink.bottom())] {
                assert!((gap - CODE_PAD).abs() <= 0.25, "{text:?} {side}: {gap}");
            }
        }
        // A line wider than the block wraps inside it: every glyph inside the fill's pads, a hook on each
        // continuation row, and no sideways scroller anywhere (its bar would float over the pane's text).
        let long = "  \"arn\": \"arn:aws:bedrock:us-west-2::foundation-model/amazon.titan-embed-text-v2:0\",";
        let shapes = crate::testutil::paint_at2(egui::vec2(800.0, 400.0), |ui| {
            ui.set_width(200.0);
            block(ui, &Block::Code { text: long.into(), lang: None }, &p, 14.0, chip);
        });
        let (fill, _) = *crate::testutil::fills(&shapes).iter().find(|(_, c)| *c == chip).expect("fill");
        assert!(fill.width() <= 200.5, "the block keeps the column: {}", fill.width());
        assert!(fill.height() > 2.0 * CODE_PAD + 2.0 * 18.0, "wrapped to several rows: {}", fill.height());
        let ink = crate::testutil::inks(&shapes).into_iter().map(|(_, r)| r).reduce(|a, b| a.union(b)).expect("ink");
        assert!(ink.right() <= fill.right() - CODE_PAD + 1.0, "no glyph in the right pad: {} vs {}", ink.right(), fill.right());
        assert!(walk_lines(&shapes).len() + shapes.iter().filter(|c| matches!(c.shape, egui::Shape::Path(_))).count() >= 2, "continuation hooks");
    }

    #[test]
    fn short_code_moves_whole_to_the_next_line() {
        // A chip under CODE_KEEP_WHOLE characters never splits at a wrap, at any width it fits in; longer code
        // still wraps at its spaces.
        let p = crate::theme::LIGHT;
        let code = Span { code: true, ..span("(rendererAvailable: false)") };
        let spans = keep_short_code_whole(vec![span("Set the renderer flag so the panel reads "), code, span(" and loads.")]);
        assert!(spans[1].text.contains('\u{a0}') && !spans[1].text.contains(' '));
        let long = Span { code: true, ..span("a much longer code span that wraps at its spaces") };
        assert_eq!(keep_short_code_whole(vec![long.clone()])[0].text, long.text);
        crate::testutil::with_ui(|ui| {
            for w in (240..=520).step_by(4) {
                let chips = ui.fonts_mut(|f| {
                    let fit = MonoFit::measure(f, 14.0, false, LH, 14.0);
                    let (job, chars) = paragraph_job(&spans, &p, 14.0, false, &fit, LH);
                    layout_paragraph(f, job, &chars, w as f32, LH).1
                });
                assert_eq!(chips.len(), 1, "width {w}: one chip");
            }
        });
    }

    #[test]
    fn link_underlines_sit_two_under_the_baseline() {
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
    fn smart_link_keys_and_mentions_are_chips_not_underlines() {
        let p = palette();
        let key = Span { link: Some("https://x.atlassian.net/browse/CI-1104".into()), ..span("CI-1104") };
        let mention = Span { bold: true, ..span("@Ada Lovelace") };
        let doc = adf::Doc { blocks: vec![Block::Paragraph(vec![span("see "), key, span(" and "), mention, span(".")])] };
        let shapes = crate::testutil::paint_at2(egui::vec2(800.0, 200.0), |ui| show(ui, &doc, &p, p.surface_2));
        let chips: Vec<egui::Rect> = crate::testutil::fills(&shapes).into_iter().filter(|(_, c)| *c == p.accent_subtle()).map(|(r, _)| r).collect();
        assert_eq!(chips.len(), 2, "{chips:?}");
        assert!(walk_lines(&shapes).is_empty(), "no underline under a chip");
        let ink = crate::testutil::ink_of(&shapes, "see CI-1104 and @Ada\u{a0}Lovelace.");
        assert!(chips.iter().all(|c| ink.intersects(*c)), "chips sit on the line");
    }

    #[test]
    fn span_format_maps_marks() {
        let p = palette();
        let plain = span_format(&span("a"), &p, 14.0, false, &FIT, LH);
        assert_eq!(plain.font_id, theme::body(14.0));
        assert_eq!(plain.color, p.text);
        assert_eq!(plain.underline, Stroke::NONE);
        assert_eq!(plain.line_height, Some(LH));

        let bold = span_format(&Span { bold: true, ..span("b") }, &p, 14.0, false, &FIT, LH);
        assert_eq!(bold.font_id, theme::bold(14.0));
        assert_eq!(span_format(&span("h"), &p, 20.0, true, &FIT, LH).font_id, theme::bold(20.0));

        let code = span_format(&Span { code: true, ..span("c") }, &p, 14.0, false, &FIT, LH);
        assert_eq!(code.font_id, theme::mono(14.0 * 0.9), "exactly 0.9 × 14");
        assert_eq!((code.line_height, code.valign), (Some(FIT.code), egui::Align::Max), "code sits on the body baseline");
        assert_eq!(code.background, Color32::TRANSPARENT, "the chip is painted under the label, not by the section");
        assert_eq!(
            span_format(&Span { code: true, ..span("c") }, &p, 20.0, true, &FIT, LH).font_id,
            theme::mono(14.0 * 0.9),
            "capped at 0.9 × body in a heading"
        );
        assert_eq!(code_size(13.0, 14.0), 13.0 * 0.9, "smaller contexts keep the ratio");

        let styled = span_format(&Span { italic: true, strike: true, ..span("s") }, &p, 14.0, false, &FIT, LH);
        assert!(styled.italics);
        assert_eq!(styled.strikethrough, Stroke::new(1.0, p.muted));

        let link = Span { link: Some("https://x".into()), ..span("l") };
        assert_eq!(span_format(&link, &p, 14.0, false, &FIT, LH).color, p.accent);
        assert_eq!(span_format(&link, &p, 14.0, false, &FIT, LH).underline, Stroke::NONE, "links are underlined by `spans`, not egui");

        let key = Span { link: Some("https://x.atlassian.net/browse/CI-1".into()), ..span("CI-1") };
        let k = span_format(&key, &p, 14.0, false, &FIT, LH);
        assert_eq!((k.font_id, k.color), (theme::mono(14.0 * 0.9), p.text), "a smart-link key is a mono chip in text colour");
        assert_eq!(span_format(&Span { text: "see CI-1".into(), ..key }, &p, 14.0, false, &FIT, LH).color, p.accent, "a named link stays a link");
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
    #[cfg_attr(not(target_os = "macos"), ignore = "glyph-ink metrics are calibrated on macOS font rasterisation")]
    fn chips_pad_tight_to_the_ink_and_neighbours_abut() {
        // The fill reaches CODE_PAD_X past the code's ink on both sides; punctuation (or a letter) that touches
        // the code sits CHIP_ABUT off the fill; a space keeps its whole advance outside it.
        let into = junction_shift((false, 0.0, 5.0, 0.0, 6.0), (false, 10.5, 15.0, 10.0, 16.0), true);
        assert_eq!(into, 5.0 + CHIP_ABUT - (10.5 - CODE_PAD_X), "( then code: the fill starts 1 past the ( ink");
        let spaced = junction_shift((true, 6.0, 9.0, 6.0, 9.0), (false, 10.5, 15.0, 10.0, 16.0), true);
        assert_eq!(spaced, 9.0 - (10.5 - CODE_PAD_X), "space then code: the fill starts at the space's advance");
        let out = junction_shift((false, 0.0, 5.0, 0.0, 6.0), (false, 6.5, 8.0, 6.0, 9.0), false);
        assert_eq!(out, 5.0 + CODE_PAD_X + CHIP_ABUT - 6.5, "code then ,: the comma's ink 1 past the fill");
        let s = |text: &str, code: bool| Span { code, ..span(text) };
        let para = [s("(", false), s("07-31", true), s(") then ", false), s("x", true), s(" y", false)];
        assert_eq!(
            [span_lead(&para, 1), span_lead(&para, 2), span_lead(&para, 3), span_lead(&para, 4)],
            [CODE_PAD_TIGHT, CODE_PAD_TIGHT, CODE_PAD_X, CODE_PAD_X],
            "first-pass leads; the laid-out ink sets the final ones"
        );
        assert_eq!(span_lead(&para, 0), 0.0);
        // With the real fonts: every chip CODE_PAD_X to its ink, the hugging marks CHIP_ABUT off the fill.
        let p = palette();
        let chip = theme::chip_fill_on(p.surface, &p);
        let doc = adf::Doc {
            blocks: vec![Block::Paragraph(vec![
                s("correct (", false),
                s("07-31", true),
                s(") and ", false),
                s("known_hosts", true),
                s(", with ", false),
                s("x", true),
                s(" y", false),
            ])],
        };
        let shapes = crate::testutil::paint_at2(egui::vec2(800.0, 200.0), |ui| show(ui, &doc, &p, chip));
        let mut chips: Vec<egui::Rect> = crate::testutil::fills(&shapes).into_iter().filter(|(_, c)| *c == chip).map(|(r, _)| r).collect();
        chips.sort_by(|a, b| a.left().total_cmp(&b.left()));
        assert_eq!(chips.len(), 3, "{chips:?}");
        let glyphs: Vec<(char, f32, f32)> = {
            let mut v = Vec::new();
            fn walk(s: &egui::Shape, v: &mut Vec<(char, f32, f32)>) {
                match s {
                    egui::Shape::Text(t) => {
                        for row in &t.galley.rows {
                            for gl in &row.row.glyphs {
                                if !gl.uv_rect.is_nothing() && gl.uv_rect.size.x > 0.0 {
                                    let x = t.pos.x + row.pos.x + gl.pos.x + gl.uv_rect.offset.x;
                                    v.push((gl.chr, x, x + gl.uv_rect.size.x));
                                }
                            }
                        }
                    }
                    egui::Shape::Vec(s) => s.iter().for_each(|s| walk(s, v)),
                    _ => {}
                }
            }
            shapes.iter().for_each(|c| walk(&c.shape, &mut v));
            v
        };
        let find = |c: char, nth: usize| *glyphs.iter().filter(|g| g.0 == c).nth(nth).unwrap_or_else(|| panic!("{c}"));
        let near = |v: f32, want: f32, what: &str| assert!((v - want).abs() <= 0.5, "{what}: {v}");
        near(find('0', 0).1 - chips[0].left(), CODE_PAD_X, "07-31 left pad");
        near(chips[0].right() - find('1', 0).2, CODE_PAD_X, "07-31 right pad");
        near(chips[0].left() - find('(', 0).2, CHIP_ABUT, "( abuts");
        near(find(')', 0).1 - chips[0].right(), CHIP_ABUT, ") abuts");
        near(find('k', 0).1 - chips[1].left(), CODE_PAD_X, "known_hosts left pad");
        near(chips[1].right() - find('s', 1).2, CODE_PAD_X, "known_hosts right pad");
        near(find(',', 0).1 - chips[1].right(), CHIP_ABUT, ", abuts");
        near(find('x', 0).1 - chips[2].left(), CODE_PAD_X, "x left pad");
        near(chips[2].right() - find('x', 0).2, CODE_PAD_X, "x right pad");
        assert!(find('y', 0).1 - chips[2].right() > 2.0, "the word after keeps its space");
        // A word space before a chip reads like one between words (its advance, 2.5–4.5 to the ink).
        let before = chips[1].left() - find('d', 0).2;
        assert!((2.5..=4.5).contains(&before), "and → known_hosts {before}");
    }

    #[test]
    fn a_chip_starting_a_row_keeps_its_pad_inside_the_column() {
        let g = |c: char, code: bool, x: f32| (c, code, x, x + 5.0, 0.0, 14.0);
        assert_eq!(row_inset(&[g('m', true, 0.0), g('o', true, 5.0)]), CODE_PAD_X, "a wrapped row starting in code moves in");
        assert_eq!(row_inset(&[g('m', true, CODE_PAD_X)]), 0.0, "the paragraph's own leading pad already insets it");
        assert_eq!(row_inset(&[g('a', false, 0.0), g('m', true, 9.0)]), 0.0);
        assert_eq!(row_inset(&[]), 0.0);
    }

    #[test]
    fn lists_share_one_24_px_gutter() {
        assert_eq!(marker_slot(0.0) + MARKER_GAP, LIST_GUTTER, "bullets and task boxes");
        assert_eq!(marker_slot(15.4) + MARKER_GAP, LIST_GUTTER, "1.–9.");
        assert_eq!(marker_slot(17.6) + MARKER_GAP, LIST_GUTTER, "10.");
        assert_eq!(marker_slot(30.2), 31.0, "only a very wide number pushes the text");
    }

    #[test]
    fn prose_rhythm_and_heading_scale() {
        assert_eq!((14.0 * theme::COMPACT.prose_line_height).round(), 21.0, "1.5 × body");
        let c = &theme::COMPACT;
        assert_eq!((heading_px(c, 1), heading_px(c, 2), heading_px(c, 3), heading_px(c, 4), heading_px(c, 6)), (18.0, 16.0, 14.0, 14.0, 14.0));
        let f = &theme::COMFORTABLE;
        assert_eq!((heading_px(f, 1), heading_px(f, 2), heading_px(f, 3), heading_px(f, 5)), (19.0, f.h3, f.body, f.body));
        for sp in [c, f] {
            assert!(heading_px(sp, 1) < sp.h2 && heading_px(sp, 2) <= sp.h3, "a document heading sits under the pane's chrome");
        }
        let para = Block::Paragraph(vec![span("x")]);
        let h = |level| Block::Heading { level, spans: vec![span("H")] };
        assert_eq!(block_gap(c, &para, &para), c.paragraph_gap);
        assert!(block_gap(c, &para, &h(2)) > block_gap(c, &h(2), &para), "more space above a heading than below");
        assert!(block_gap(c, &para, &h(1)) > block_gap(c, &para, &h(4)), "bigger headings get more room");
        assert_eq!(block_gap(c, &para, &Block::Rule), block_gap(c, &Block::Rule, &para), "a rule is centred between paragraphs");
        assert_eq!(block_gap(c, &Block::Rule, &h(2)), block_gap(c, &para, &Block::Rule), "a rule sits midway, also before a heading");
        assert!(block_gap(c, &h(1), &h(2)) < block_gap(c, &para, &h(2)), "a heading under a heading stays with it");
        assert!(block_gap(c, &h(1), &h(2)) > block_gap(c, &h(2), &para), "a subheading sits closer to its own text");
    }

    #[test]
    fn headings_sit_twice_as_far_from_the_text_above_as_below() {
        // Ink to ink with the real fonts: the space above a heading after a paragraph is at least 1.6 times the
        // space below it, at every level, so the heading groups with what it introduces. After a rule the rule
        // sits midway (within 4) between the text above it and the heading, and the heading still sits nearer its
        // own text. A subheading straight under a heading sits nearer its own text than the heading above.
        let p = crate::theme::LIGHT;
        let sp = |t: &str| vec![span(t)];
        for level in [1u8, 2, 3] {
            for rule in [false, true] {
                let mut blocks = vec![Block::Paragraph(sp("above xx"))];
                if rule {
                    blocks.push(Block::Rule);
                }
                blocks.extend([Block::Heading { level, spans: sp("Heading HH") }, Block::Paragraph(sp("Below xx"))]);
                let doc = adf::Doc { blocks };
                let shapes = crate::testutil::paint_at2(egui::vec2(800.0, 400.0), |ui| show(ui, &doc, &p, p.surface_2));
                let h = crate::testutil::ink_of(&shapes, "Heading HH");
                let b = crate::testutil::ink_of(&shapes, "Below xx");
                let a = crate::testutil::ink_of(&shapes, "above xx").bottom();
                let top = if rule { walk_lines(&shapes)[0][0].y + 0.5 } else { a };
                let (up, down) = (h.top() - top, b.top() - h.bottom());
                if rule {
                    let over = top - 1.0 - a;
                    assert!((up - over).abs() <= 4.0, "h{level}: the rule sits midway: above it {over}, below it {up}");
                    assert!(up > down, "h{level} after a rule: above {up}, below {down}");
                } else {
                    assert!(up >= 1.6 * down, "h{level}: above {up}, below {down}");
                }
            }
        }
        let doc = adf::Doc {
            blocks: vec![
                Block::Heading { level: 2, spans: sp("Target HH") },
                Block::Heading { level: 3, spans: sp("Role HH") },
                Block::Paragraph(sp("Body xx")),
            ],
        };
        let shapes = crate::testutil::paint_at2(egui::vec2(800.0, 400.0), |ui| show(ui, &doc, &p, p.surface_2));
        let (t, r, b) =
            (crate::testutil::ink_of(&shapes, "Target HH"), crate::testutil::ink_of(&shapes, "Role HH"), crate::testutil::ink_of(&shapes, "Body xx"));
        assert!(r.top() - t.bottom() > b.top() - r.bottom(), "h3 under h2: {} over, {} under", r.top() - t.bottom(), b.top() - r.bottom());
    }

    #[test]
    fn long_code_lines_wrap_with_a_hanging_indent() {
        // A 1-wide mono face: room for 20 columns.
        let w = |_: char| 1.0;
        let rows = wrap_code("  count_over_time({namespace=\"compliance\"}) > 0\nshort", 20.0, w);
        assert!(rows.iter().all(|r| r.text.chars().count() <= 20), "nothing past the room: {rows:?}");
        assert_eq!(rows[0].hang, None);
        assert!(rows[0].text.starts_with("  count_over_time("), "the line keeps its indent: {rows:?}");
        for r in &rows[1..rows.len() - 1] {
            assert_eq!(r.hang, Some(2.0), "the hook sits at the line's indent");
            assert!(r.text.starts_with("    ") && !r.text[4..].starts_with(' '), "continuations hang two past the indent: {r:?}");
        }
        assert_eq!(rows.last().unwrap(), &CodeRow { text: "short".into(), hang: None });
        let text: String = rows[..rows.len() - 1].iter().map(|r| r.text.trim()).collect::<Vec<_>>().join("");
        assert_eq!(text.replace(' ', ""), "count_over_time({namespace=\"compliance\"})>0", "no glyph lost");
        // Breaks at a space when there is one, and the space goes.
        let rows = wrap_code("alpha beta gamma delta", 14.0, w);
        assert_eq!(rows.iter().map(|r| r.text.trim()).collect::<Vec<_>>(), ["alpha beta", "gamma delta"]);
        // An unbreakable run is cut at the room, never clipped.
        let rows = wrap_code(&"x".repeat(25), 10.0, w);
        assert_eq!(rows.iter().map(|r| r.text.trim().len()).sum::<usize>(), 25);
        assert!(rows.iter().all(|r| r.text.len() <= 10));
        // A deep indent drops when it would leave under half the room.
        let rows = wrap_code(&format!("{}{}", " ".repeat(12), "y".repeat(30)), 20.0, w);
        assert_eq!(rows[1].hang, Some(0.0));
        assert_eq!(wrap_code("a\tb", 20.0, w)[0].text, "a    b", "tabs as four spaces");
    }

    #[test]
    fn tables_fill_the_column_and_size_columns_by_content() {
        let one = |w: f32| vec![Word::new(w, 0.0)];
        // Everything fits unwrapped: natural widths, the rest shared in proportion, so the table fills.
        let w = table_widths(&[vec![one(95.0), one(195.0)]], 600.0);
        assert_eq!(w, vec![200.0, 400.0]);
        // Not even the words fit: the longest words scaled to the room, so the table never runs past its column.
        assert_eq!(table_widths(&[vec![one(295.0), one(95.0)]], 200.0), vec![150.0, 50.0]);
        // The wrap model: a word and its space must fit; a hard break starts a line; a too-long word splits.
        let words = [Word::new(40.0, 5.0), Word::new(40.0, 5.0), Word::new(40.0, 0.0)];
        assert_eq!(cell_lines(&words, 85.0), 2, "40 + 5 + 40 and its space cross 85");
        assert_eq!(cell_lines(&words, 130.0), 1);
        assert_eq!(cell_lines(&[Word { brk: true, ..Word::new(10.0, 5.0) }, Word::new(10.0, 0.0)], 100.0), 2);
        assert_eq!(cell_lines(&[Word::new(250.0, 0.0)], 100.0), 3);
    }

    /// The CI-9999 permissions table, a cell per string with `code` in backticks.
    const CI_9999: [[&str; 3]; 6] = [
        ["Area", "Verbs", "Reason"],
        [
            "Everything in `ex:developers:aggregated` today except `secrets`",
            "pods get/list/watch/delete, exec, attach, port-forward, logs; deployments, replicasets and statefulsets patch/update; deployments/scale; jobs create/delete; every `ex:*:view` operator role; `ex:airflow:edit`",
            "today's developer surface",
        ],
        ["configmaps, horizontalpodautoscalers", "create, patch, update, delete", "XX-010 curated role; audit: patch configmaps 18, patch HPA 18"],
        [
            "`ex:flink:edit`, `ex:ray:edit`, `ex:ocd:edit`, `ex:prometheus:edit`, `ex:linkerd:edit`, `ex:linkerd-policy:edit`, `ex:ephemeral-containers`",
            "label `rbac.authorization.k8s.io/aggregate-to-ex-developers: \"true\"`",
            "XX-010 curated role; audit: patch flink CRDs 28",
        ],
        [
            "`applications.argoproj.io`, as a new `ex:argocd:edit`",
            "patch, update",
            "disable auto-sync (`spec.syncPolicy.automated`) and roll back (top-level `operation`) from the k8s API; Argo's own RBAC already grants this through the UI and API",
        ],
        [
            "`meshtlsauthentications`, `networkauthentications` (policy.linkerd.io); `podmonitors`, `probes`, `scrapeconfigs` (monitoring.coreos.com)",
            "get, list, watch, added to `ex:linkerd-policy:view` and `ex:prometheus:view`",
            "5 of the audit's 46 denials were these two gaps, not the namespace ceiling",
        ],
    ];

    fn ci_9999_rows() -> Vec<Vec<Vec<Span>>> {
        CI_9999
            .iter()
            .enumerate()
            .map(|(ri, row)| {
                row.iter()
                    .map(|cell| {
                        cell.split('`').enumerate().filter(|(_, t)| !t.is_empty()).map(|(k, t)| Span { code: k % 2 == 1, bold: ri == 0, ..span(t) }).collect()
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn the_ci_9244_table_gives_its_wordy_column_the_room() {
        // Round 4: columns came out near equal (207 / 158 / 155) and Verbs row 1 ran 10-13 lines beside a 2-line
        // Reason. Real fonts, comfortable, the full 520 column.
        let p = crate::theme::LIGHT;
        crate::testutil::with_ui(|ui| {
            theme::set_density(ui.ctx(), theme::Density::Comfortable);
            let sp = theme::spacing(ui.ctx());
            let rows = ci_9999_rows();
            let words: Vec<Vec<Vec<Word>>> =
                rows.iter().enumerate().map(|(ri, r)| r.iter().map(|c| cell_words(ui, c, &p, sp.small, ri == 0)).collect()).collect();
            let avail = sp.prose_max_width - 2.0 * 2.0 * table_pad(sp).x;
            let w = table_widths(&words, avail);
            assert!((w.iter().sum::<f32>() - avail).abs() < 0.5, "fills the column: {w:?}");
            let lines = |ri: usize, ci: usize| cell_lines(&words[ri][ci], w[ci] - CELL_SLACK);
            // The round-4 split (content 183 / 134 / 131) modelled the same way: Verbs row 1 at 13 lines, 40 in all,
            // as the captures showed.
            let old = [183.0, 134.0, 131.0];
            assert_eq!(cell_lines(&words[1][1], old[1] - CELL_SLACK), 13, "the model reproduces the capture");
            assert_eq!(table_height(&words, &old), 40);
            assert!(lines(1, 1) <= 9, "Verbs row 1 runs {} lines at {w:?}", lines(1, 1));
            assert!(table_height(&words, &w) <= 34, "the table is shorter: {} lines at {w:?}", table_height(&words, &w));
            // Few identifiers break, and only ones too wide for an even split.
            let wr = &w;
            let broken: Vec<f32> = words
                .iter()
                .flatten()
                .zip((0..3).cycle())
                .flat_map(|(c, ci)| c.iter().filter(move |x| x.w > wr[ci] - CELL_SLACK + 0.01).map(|x| x.w))
                .collect();
            assert!(broken.len() <= 4 && broken.iter().all(|&b| b > avail / 3.0), "breaks {broken:?} at {w:?}");
            // No split on a 2-point grid scores better.
            let best = table_score(&words, &w);
            let mut a = 40.0;
            while a <= avail - 80.0 {
                let mut b = 40.0;
                while a + b + 40.0 <= avail {
                    let t = [a, b, avail - a - b];
                    assert!(table_score(&words, &t) >= best, "{t:?} beats {w:?}");
                    b += 2.0;
                }
                a += 2.0;
            }
        });
    }

    #[test]
    fn the_table_model_matches_the_real_wrap() {
        // cell_lines against layout_paragraph's own rows, for every CI-9999 cell at three widths.
        let p = crate::theme::LIGHT;
        crate::testutil::with_ui(|ui| {
            theme::set_density(ui.ctx(), theme::Density::Comfortable);
            let sp = theme::spacing(ui.ctx());
            let lh = table_line(ui.ctx());
            for (ri, row) in ci_9999_rows().iter().enumerate() {
                for cell in row {
                    let words = cell_words(ui, cell, &p, sp.small, ri == 0);
                    for width in [130.0, 160.0, 200.0] {
                        let s = prepare(cell);
                        let real = ui.fonts_mut(|f| {
                            let fit = MonoFit::measure(f, sp.small, ri == 0, lh, sp.body);
                            let (job, chars) = paragraph_job(&s, &p, sp.small, ri == 0, &fit, lh);
                            layout_paragraph(f, job, &chars, width, lh).0.rows.len()
                        });
                        let model = cell_lines(&words, width - CELL_SLACK);
                        assert!(real.abs_diff(model) <= 1, "row {ri} at {width}: real {real}, model {model}");
                    }
                }
            }
        });
    }

    #[test]
    fn long_code_breaks_only_after_a_slash_or_underscore() {
        let code = |t: &str| Span { code: true, ..span(t) };
        let out = soft_break_long_code(vec![code("rbac.authorization.k8s.io/aggregate-to-ex-developers:"), code("ex:linkerd-policy:edit"), span("a/b_c")]);
        let b = SOFT_BREAK;
        assert_eq!(out[0].text, format!("rbac.authorization.k8s.io/{b}aggregate-to-ex-developers:"), "never at a '-', ':' or '.'");
        assert_eq!(out[1].text, "ex:linkerd-policy:edit", "a short identifier never breaks");
        assert_eq!(out[2].text, "a/b_c", "only code");
    }

    #[test]
    fn a_table_fills_the_prose_column_with_a_header_rule() {
        let p = crate::theme::LIGHT;
        let b = |t: &str| Span { bold: true, ..span(t) };
        let rows = vec![
            vec![vec![b("Area")], vec![b("Reason")], vec![b("Owner")]],
            vec![vec![span("watch")], vec![span("delete, attach, forward and a long explanation that needs to wrap at words")], vec![span("ex:flink:edit")]],
        ];
        let shapes = crate::testutil::paint_at2(egui::vec2(800.0, 600.0), |ui| {
            ui.set_width(460.0);
            block(ui, &Block::Table(rows.clone()), &p, 14.0, p.surface_2);
        });
        let inks = crate::testutil::inks(&shapes);
        let right = inks.iter().map(|(_, r)| r.right()).fold(0.0, f32::max);
        let lines = walk_lines(&shapes);
        let rule = lines.first().expect("a header rule");
        assert!((rule[1].x - rule[0].x - 460.0).abs() <= 1.0, "the table fills the column: {rule:?}");
        assert!(right <= rule[1].x, "nothing past the table");
        for word in ["Reason", "watch", "delete,", "ex:flink:edit"] {
            assert!(
                inks.iter().any(|(t, _)| t.split_whitespace().any(|w| w == word)) || inks.iter().any(|(t, _)| t.contains(word)),
                "{word} kept whole: {inks:?}"
            );
        }
        // Top-aligned: every cell of a row starts on one line.
        let (a, r) = (crate::testutil::ink_of(&shapes, "Area"), crate::testutil::ink_of(&shapes, "Reason"));
        assert!((a.top() - r.top()).abs() <= 1.0);
    }

    #[test]
    fn task_items_are_rows_with_boxes() {
        let p = crate::theme::LIGHT;
        let item = |t: &str| vec![Block::Paragraph(vec![span(t)])];
        let tasks = Block::Tasks(vec![(false, item("Find out")), (true, item("Determine it")), (false, item("Evaluate"))]);
        let shapes = crate::testutil::paint_at2(egui::vec2(800.0, 400.0), |ui| {
            ui.set_width(400.0);
            block(ui, &tasks, &p, 14.0, p.surface_2);
        });
        let (a, b, c) =
            (crate::testutil::ink_of(&shapes, "Find out"), crate::testutil::ink_of(&shapes, "Determine it"), crate::testutil::ink_of(&shapes, "Evaluate"));
        assert!(b.top() > a.bottom() && c.top() > b.bottom(), "one item per line");
        assert!((a.left() - b.left()).abs() <= 0.5, "one indent");
        // Square boxes: an open task's box is outlined, a done one's filled muted; never the accent.
        let filled: Vec<egui::Rect> = crate::testutil::fills(&shapes).into_iter().filter(|(_, col)| *col == p.muted).map(|(r, _)| r).collect();
        assert_eq!(filled.len(), 1, "only the done task's box is filled: {filled:?}");
        assert!((filled[0].width() - filled[0].height()).abs() < 0.01 && filled[0].width() >= 9.0, "a square box: {:?}", filled[0]);
        assert!(filled[0].center().y > b.top() && filled[0].center().y < b.bottom() + 1.0, "on the done item's line");
        assert_eq!(crate::testutil::fills(&shapes).into_iter().filter(|(_, col)| *col == p.accent).count(), 0);
        // The same text edge as a bullet list.
        let list = Block::List { ordered: false, items: vec![item("Find out")] };
        let shapes = crate::testutil::paint_at2(egui::vec2(800.0, 400.0), |ui| {
            ui.set_width(400.0);
            block(ui, &list, &p, 14.0, p.surface_2);
        });
        assert!((crate::testutil::ink_of(&shapes, "Find out").left() - a.left()).abs() <= 0.5);
    }

    #[test]
    fn prose_is_capped_at_the_measure() {
        let p = crate::theme::LIGHT;
        let long = "word ".repeat(200);
        let doc = adf::Doc { blocks: vec![Block::Paragraph(vec![span(&long)])] };
        let shapes = crate::testutil::paint_at2(egui::vec2(1400.0, 1200.0), |ui| {
            ui.set_width(1000.0);
            show(ui, &doc, &p, p.surface_2);
        });
        let right = crate::testutil::inks(&shapes).iter().map(|(_, r)| r.right()).fold(0.0, f32::max);
        let left = crate::testutil::inks(&shapes).iter().map(|(_, r)| r.left()).fold(f32::INFINITY, f32::min);
        assert!(right - left <= theme::COMPACT.prose_max_width, "{}", right - left);
    }

    /// (glyph left, glyph right) of every glyph, in text order, of a paragraph laid out wide.
    fn glyph_xs(s: &[Span]) -> Vec<(char, f32, f32)> {
        with_fonts(|f| {
            let p = palette();
            let fit = MonoFit::measure(f, 14.0, false, LH, 14.0);
            let s = adf::tidy_code_spans(s);
            let (job, chars) = paragraph_job(&s, &p, 14.0, false, &fit, LH);
            let (g, _) = layout_paragraph(f, job, &chars, 2000.0, LH);
            g.rows.iter().flat_map(|r| r.row.glyphs.iter().map(|gl| (gl.chr, gl.pos.x, gl.pos.x + gl.advance_width))).collect()
        })
    }

    #[test]
    fn a_word_after_a_chip_keeps_its_space() {
        let s = |text: &str, code: bool| Span { code, ..span(text) };
        let g = glyph_xs(&[s("triggers ", false), s("config.yml", true), s(" every", false)]);
        let text: String = g.iter().map(|c| c.0).collect();
        assert_eq!(text, "triggers config.yml every", "the space survives layout");
        let l = g.iter().position(|c| c.0 == 'l').unwrap();
        let (sp, e) = (g[l + 1], g[l + 2]);
        assert_eq!(sp.0, ' ');
        assert!(sp.2 - sp.1 > 2.0, "a real space glyph, {sp:?}");
        assert!(e.1 - g[l].2 >= CODE_PAD_X + 2.0, "pad and space between the chip and the word: {}", e.1 - g[l].2);
        let g = glyph_xs(&[s("config.yml ", true), s("every", false)]);
        assert_eq!(g.iter().map(|c| c.0).collect::<String>(), "config.yml every");
    }

    #[test]
    #[cfg_attr(not(target_os = "macos"), ignore = "glyph-ink metrics are calibrated on macOS font rasterisation")]
    fn punctuation_after_a_chip_hugs_it_then_keeps_its_space() {
        let s = |text: &str, code: bool| Span { code, ..span(text) };
        let g = glyph_xs(&[s("known_hosts", true), s(", with", false)]);
        assert_eq!(g.iter().map(|c| c.0).collect::<String>(), "known_hosts, with");
        let comma = g.iter().position(|c| c.0 == ',').unwrap();
        let gap = g[comma].1 - g[comma - 1].2;
        assert!((0.0..=CODE_PAD_X + CHIP_ABUT + 0.5).contains(&gap), "the comma hugs the chip: {gap}");
        assert_eq!(g[comma + 1].0, ' ');
        assert!(g[comma + 2].1 - g[comma].2 > 2.0, "the space after the comma survives");
    }

    #[test]
    #[cfg_attr(not(target_os = "macos"), ignore = "glyph-ink metrics are calibrated on macOS font rasterisation")]
    fn mono_runs_share_the_body_baseline() {
        assert_eq!(mono_line_height(20.0, 15.0, 12.0), 17.0);
        assert_eq!(mono_line_height(20.0, 12.0, 13.0), 20.0, "never taller than the row");
        let (base_body, base_code) = with_fonts(|f| {
            let p = palette();
            let fit = MonoFit::measure(f, 14.0, false, LH, 14.0);
            let s = [Span { code: true, ..span("code") }, span(" word")];
            let (job, _) = paragraph_job(&s, &p, 14.0, false, &fit, LH);
            let g = f.layout_job(job);
            let gl = &g.rows[0].row.glyphs;
            (gl.iter().find(|g| g.chr == 'w').unwrap().pos.y, gl.iter().find(|g| g.chr == 'c').unwrap().pos.y)
        });
        assert!((base_body - base_code).abs() <= 0.5, "body {base_body} vs code {base_code}");
    }

    #[test]
    fn no_description_takes_a_paragraphs_height() {
        let h = |v: serde_json::Value| {
            crate::testutil::with_ui(|ui| {
                let p = crate::theme::LIGHT;
                ui.scope(|ui| show_value(ui, &v, &p, p.surface)).response.rect.height()
            })
        };
        let para = serde_json::json!({"type": "doc", "content": [{"type": "paragraph", "content": [{"type": "text", "text": "No description"}]}]});
        assert_eq!(h(serde_json::Value::Null), h(para));
        assert_eq!(h(serde_json::Value::Null), (14.0 * crate::theme::COMPACT.prose_line_height).round());
    }

    #[test]
    fn a_path_longer_than_a_row_leaves_no_blank_line() {
        // CI-3400: "~> 1.9.0 in" then a 54-character path; egui wrapped before it, leaving an empty row.
        let c = |t: &str| Span { code: true, ..span(t) };
        let s = prepare(&[c("~> 1.9.0"), span(" in "), c("infrastructure/terraform/examples_v2/infra/_version.tf"), span(", and more")]);
        let rows = with_fonts(|f| {
            let p = palette();
            let fit = MonoFit::measure(f, 14.0, false, LH, 14.0);
            let (job, chars) = paragraph_job(&s, &p, 14.0, false, &fit, LH);
            let (g, _) = layout_paragraph(f, job, &chars, 300.0, LH);
            g.rows.iter().map(|r| r.row.glyphs.len()).collect::<Vec<_>>()
        });
        assert!(rows.iter().all(|&n| n > 0), "{rows:?}");
    }

    #[test]
    fn every_row_takes_the_body_line_height() {
        let (tops, h) = row_tops(&[20.0, 15.0, 15.0, 20.0], 20.0);
        assert_eq!(tops, [0.0, 25.0, 45.0, 60.0], "short rows sit at the bottom of a 20 slot");
        assert_eq!(h, 80.0);
        let (tops, h) = row_tops(&[24.0], 20.0);
        assert_eq!((tops, h), (vec![0.0], 24.0), "a taller row keeps its height");
        // A wrapped all-code paragraph: separate chips, one line apart, never touching.
        let chips = with_fonts(|f| {
            let p = palette();
            let fit = MonoFit::measure(f, 14.0, false, LH, 14.0);
            let s = [Span { code: true, ..span("b54249f25c21c feat(build): add workflow_pr_comment action") }];
            let (job, chars) = paragraph_job(&s, &p, 14.0, false, &fit, LH);
            layout_paragraph(f, job, &chars, 200.0, LH).1
        });
        assert!(chips.len() >= 2, "{chips:?}");
        for w in chips.windows(2) {
            assert!((w[1].top() - w[0].top() - LH).abs() < 0.01, "one line pitch: {w:?}");
            assert!(w[1].top() - w[0].bottom() >= 3.0, "fills keep a clear gap: {w:?}");
        }
    }
}
