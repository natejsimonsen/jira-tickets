//! The keyboard help overlay (`?`), drawn from `keys::BINDINGS`. Its keys
//! (`?`, Esc) are handled in `App::shortcuts`; this only draws (and its Close
//! button and backdrop push `ToggleHelp`).

use crate::app::{Action, App, Screen};
use crate::keys::{self, BINDINGS, Group};
use crate::theme::{self, Palette, Spacing};
use crate::views::components as c;
use egui::{Galley, Margin, Sense, vec2};
use std::sync::Arc;

pub fn show(app: &mut App, ctx: &egui::Context) {
    if !app.help_open {
        return;
    }
    let p = theme::palette(ctx);
    let sp = theme::spacing(ctx);
    // Above the pickers' Foreground dialog: the backdrop at Foreground (shown after the dialog, so on top of it)
    // and the panel at Tooltip. The backdrop stops at the hint bar, like every dialog's, so the bar's "? or Esc
    // close" stays readable instead of a dimmed strip under the scrim.
    if crate::views::backdrop(ctx, "help-backdrop", egui::Order::Foreground, crate::views::hint_bar_height(app), p).clicked() {
        app.actions.push(Action::ToggleHelp);
    }
    let current = screen_group(app.screen);
    let order = section_order(current);
    let m = Metrics::of(sp);
    let layer = egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("help"));
    let measure = ctx.layer_painter(layer);
    // The panel may use the window between the top bar and the hint bar, `margin` off each. Its height hugs the
    // sheet, and it sits in the middle of that space, so the margins above and below it match.
    let space = panel_rect(ctx.content_rect(), crate::views::HEADER_H, crate::views::hint_bar_height(app).unwrap_or(0.0), m.margin);
    let edge = crate::views::dialog_stroke(p).width;
    let frame_pad = 2.0 * (m.pad + edge);
    let inner_w = (space.width() - frame_pad).max(1.0);
    let title = measure.layout_no_wrap("Keyboard shortcuts".to_string(), theme::bold(sp.h2), p.text);
    // The title's line sits centred in the BUTTON_H row, a little below the top padding; the same slack under the
    // last row makes the bottom padding read like the top.
    let foot = ((crate::views::BUTTON_H - title.size().y) / 2.0).max(0.0);
    let max_view_h = (space.height() - frame_pad - crate::views::BUTTON_H - m.title_gap - foot).max(0.0);
    let sheet = Sheet::fit(&measure, p, sp, &m, &order, inner_w, max_view_h);
    let view_h = sheet.height.min(max_view_h);
    let inner = vec2(inner_w, crate::views::BUTTON_H + m.title_gap + view_h + foot);
    let top = ((space.height() - inner.y - frame_pad) / 2.0).max(0.0).round();
    let mut close = false;
    egui::Area::new(layer.id).order(layer.order).fixed_pos(space.min + vec2(0.0, top)).show(ctx, |ui| {
        c::dialog_frame(p).inner_margin(Margin::same(m.pad as i8)).show(ui, |ui| {
            ui.set_width(inner.x);
            ui.set_height(inner.y);
            ui.spacing_mut().item_spacing.y = 0.0;
            let (row, _) = ui.allocate_exact_size(vec2(inner.x, crate::views::BUTTON_H), Sense::hover());
            ui.painter().galley(egui::pos2(row.left(), row.center().y - title.size().y / 2.0), title, p.text);
            // A framed button whose right edge is the content's right edge. A child ui, so the panel keeps its size.
            let mut bar = ui.new_child(egui::UiBuilder::new().max_rect(row).layout(egui::Layout::right_to_left(egui::Align::Center)));
            close = close_button(&mut bar, p, sp).clicked();
            ui.add_space(m.title_gap);
            let view_top = ui.cursor().min.y;
            // The viewport runs on into the right padding, where the thumb sits in the middle, clear of the text,
            // and shows at rest whenever the sheet is taller than the panel. A child ui: the panel keeps its size.
            let thumb_x = row.right() + m.pad / 2.0 + crate::views::THUMB_W / 2.0 + crate::views::THUMB_INSET;
            let viewport = egui::Rect::from_x_y_ranges(row.left()..=thumb_x, view_top..=view_top + view_h);
            let mut body = ui.new_child(egui::UiBuilder::new().max_rect(viewport).layout(egui::Layout::top_down(egui::Align::Min)));
            let saved = crate::views::set_thumb_fills(&mut body, p);
            // The thumb is the sheet's "more below" cue, so it rests at its hover tone (muted, AA on the panel).
            body.visuals_mut().widgets.inactive.bg_fill = crate::views::scroll_thumb(p).1;
            crate::views::floating_thumb(&mut body.spacing_mut().scroll);
            // Shared thumbs hide at rest; this one stays, since it is the sheet's only cue when no row is cut.
            body.spacing_mut().scroll.dormant_handle_opacity = 1.0;
            crate::views::no_scroll_fade(&mut body);
            egui::ScrollArea::vertical()
                .id_salt("help-body")
                .min_scrolled_height(view_h)
                .max_height(view_h)
                .auto_shrink([false, false])
                .scroll_bar_rect(crate::views::thumb_track(viewport))
                .show(&mut body, |ui| {
                    crate::views::restore_fills(ui, saved);
                    ui.set_width(inner.x);
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = m.gutter;
                        for (col, &width) in sheet.cols.iter().zip(&sheet.widths) {
                            ui.allocate_ui_with_layout(vec2(width, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                                ui.set_width(width);
                                ui.spacing_mut().item_spacing.y = 0.0;
                                for (i, &s) in col.iter().enumerate() {
                                    if i > 0 {
                                        ui.add_space(sheet.section_gap);
                                    }
                                    let g = order[s];
                                    let look = Look { row_gap: sheet.row_gap, key_w: sheet.key_w, width };
                                    section(ui, p, sp, &m, g, g == current, &look);
                                }
                            });
                        }
                    });
                });
        });
    });
    if close {
        app.actions.push(Action::ToggleHelp);
    }
}

/// The space the panel may use: the window between the top bar (`header`) and the hint bar (`hint`), `margin` off
/// all four sides.
pub fn panel_rect(window: egui::Rect, header: f32, hint: f32, margin: f32) -> egui::Rect {
    let r = egui::Rect::from_min_max(egui::pos2(window.left(), window.top() + header), egui::pos2(window.right(), window.bottom() - hint)).shrink(margin);
    if r.is_positive() { r } else { egui::Rect::from_min_size(r.min, egui::Vec2::ZERO) }
}

/// The help's spacing, all from the density's tokens.
struct Metrics {
    /// Window edge (or bar) → panel, on every side: a card's padding, so the sheet gets the height.
    margin: f32,
    /// Panel edge → content, on every side.
    pad: f32,
    /// Between columns: clearly wider than `key_gap`, so each description reads with its own key.
    gutter: f32,
    /// Key column → description.
    key_gap: f32,
    /// Between rows, and from a section header to its first row.
    row_gap: f32,
    /// The help-only tighter row gap, used when the sheet would not fit the panel at `row_gap`.
    tight_gap: f32,
    /// Between sections in a column at the tighter spacing (`section_gap` otherwise).
    tight_section_gap: f32,
    /// Title row → the first sections.
    title_gap: f32,
}

impl Metrics {
    fn of(sp: &Spacing) -> Self {
        Metrics {
            margin: sp.card_pad,
            pad: sp.section_gap,
            gutter: sp.section_gap,
            key_gap: sp.card_pad,
            row_gap: 2.0 * sp.line_gap,
            tight_gap: sp.line_gap,
            tight_section_gap: 2.0 * sp.card_pad,
            title_gap: 2.0 * sp.card_pad,
        }
    }
}

/// The group of keys that belongs to a screen.
pub fn screen_group(s: Screen) -> Group {
    match s {
        Screen::Issues => Group::Issues,
        Screen::Grooming => Group::Grooming,
        Screen::Team => Group::Team,
        Screen::Roadmap => Group::Roadmap,
        Screen::Me => Group::Me,
        Screen::Standup => Group::Standup,
    }
}

/// Every group in tab order, with Global first and Pickers and Edit last.
const ALL: [Group; 8] = [Group::Global, Group::Issues, Group::Grooming, Group::Team, Group::Roadmap, Group::Me, Group::Standup, Group::Pickers];

/// Reading order: the current screen's keys, then Global, then the other screens in tab order, then Pickers and
/// Edit.
pub fn section_order(current: Group) -> Vec<Group> {
    let mut out = vec![current];
    out.extend(ALL.iter().copied().filter(|g| *g != current));
    out
}

/// A column's height: its sections' heights, `gap` between them.
pub fn column_height(heights: &[f32], gap: f32) -> f32 {
    heights.iter().sum::<f32>() + gap * heights.len().saturating_sub(1) as f32
}

/// Places whole sections (never split) into at most `n` columns. Section 0 (the screen help was opened on) heads
/// the first column, and each column keeps reading order down it. Of the layouts no taller than `limit`, the one
/// closest to reading order (fewest sections that sit in an earlier column than a section before them), then the
/// shortest. When none fits (the sheet scrolls), the shortest, then the closest to reading order.
pub fn arrange(heights: &[f32], gap: f32, n: usize, limit: f32) -> Vec<Vec<usize>> {
    let len = heights.len();
    if len == 0 {
        return Vec::new();
    }
    let n = n.max(1);
    let mut assign = vec![0usize; len];
    // (inversions, tallest, assignment) for the best layout that fits, and for the shortest of all.
    let mut fit: Option<(usize, f32, Vec<usize>)> = None;
    let mut short: Option<(usize, f32, Vec<usize>)> = None;
    let mut sums = vec![0.0f32; n];
    let mut counts = vec![0usize; n];
    loop {
        sums.iter_mut().for_each(|s| *s = 0.0);
        counts.iter_mut().for_each(|c| *c = 0);
        for (s, &col) in assign.iter().enumerate() {
            sums[col] += heights[s];
            counts[col] += 1;
        }
        let tallest = sums.iter().zip(&counts).map(|(s, &k)| s + gap * k.saturating_sub(1) as f32).fold(0.0, f32::max);
        let inv = (0..len).map(|i| (i + 1..len).filter(|&j| assign[j] < assign[i]).count()).sum::<usize>();
        const EPS: f32 = 0.5;
        if tallest <= limit + EPS && fit.as_ref().is_none_or(|(bi, bt, _)| inv < *bi || (inv == *bi && tallest < bt - EPS)) {
            fit = Some((inv, tallest, assign.clone()));
        }
        if short.as_ref().is_none_or(|(bi, bt, _)| tallest < bt - EPS || ((tallest - bt).abs() <= EPS && inv < *bi)) {
            short = Some((inv, tallest, assign.clone()));
        }
        // The next assignment (section 0 stays in column 0).
        let mut i = len - 1;
        loop {
            if i == 0 {
                let (_, _, best) = fit.or(short).expect("one layout");
                let mut cols: Vec<Vec<usize>> = vec![Vec::new(); n];
                for (s, &col) in best.iter().enumerate() {
                    cols[col].push(s);
                }
                cols.retain(|c| !c.is_empty());
                return cols;
            }
            assign[i] += 1;
            if assign[i] < n {
                break;
            }
            assign[i] = 0;
            i -= 1;
        }
    }
}

/// Column widths that fill `total`: a column that needs less than an equal share gets what it needs, the rest
/// share what is left; when every column has what it needs, the leftover is spread evenly. Floored to a point.
pub fn share(needs: &[f32], total: f32) -> Vec<f32> {
    let mut out = vec![0.0f32; needs.len()];
    let mut open: Vec<usize> = (0..needs.len()).collect();
    let mut left = total;
    while !open.is_empty() {
        let each = left / open.len() as f32;
        let (met, rest): (Vec<usize>, Vec<usize>) = open.iter().partition(|&&i| needs[i] <= each);
        if met.is_empty() {
            for i in rest {
                out[i] = each;
            }
            left = 0.0;
            break;
        }
        for i in met {
            out[i] = needs[i];
            left -= needs[i];
        }
        open = rest;
    }
    let extra = if needs.is_empty() { 0.0 } else { left.max(0.0) / needs.len() as f32 };
    out.into_iter().map(|w| (w + extra).floor()).collect()
}

/// The laid-out sheet: each column's sections and width, the one key column width, the gaps it uses and its
/// tallest column.
struct Sheet {
    cols: Vec<Vec<usize>>,
    widths: Vec<f32>,
    key_w: f32,
    row_gap: f32,
    section_gap: f32,
    height: f32,
}

impl Sheet {
    /// The sheet that fits `max_h`: the most columns the width allows (each description keeps at least half the
    /// prose measure), at the density's row and section gaps, else at the help's tighter ones. When neither
    /// fits, the shorter. One key column width, the widest key cell, for every column.
    fn fit(pt: &egui::Painter, p: &Palette, sp: &Spacing, m: &Metrics, order: &[Group], avail: f32, max_h: f32) -> Sheet {
        let key_w = BINDINGS.iter().map(|k| cell(pt, k.keys, p, sp, p.text).1.ceil()).fold(0.0, f32::max);
        let (n, col_w) = grid(avail, key_w, m, sp.prose_max_width / 2.0);
        let mut best: Option<Sheet> = None;
        for gaps in [(m.row_gap, sp.section_gap), (m.tight_gap, m.tight_section_gap)] {
            let sheet = Sheet::build(pt, p, m, sp, order, key_w, (n, col_w), avail, gaps, max_h);
            let fits = sheet.height <= max_h;
            if best.as_ref().is_none_or(|b| sheet.height < b.height) {
                best = Some(sheet);
            }
            if fits {
                break;
            }
        }
        best.expect("one candidate")
    }

    /// Places the sections measured at equal columns, then widens each column toward its longest description
    /// (`share`). Wider columns only lose wraps, so the placement still holds.
    #[allow(clippy::too_many_arguments)]
    fn build(
        pt: &egui::Painter,
        p: &Palette,
        m: &Metrics,
        sp: &Spacing,
        order: &[Group],
        key_w: f32,
        grid: (usize, f32),
        avail: f32,
        gaps: (f32, f32),
        max_h: f32,
    ) -> Sheet {
        let ((n, col_w), (row_gap, section_gap)) = (grid, gaps);
        let heights: Vec<f32> = order.iter().map(|g| block_height(pt, p, sp, m, *g, row_gap, key_w, col_w)).collect();
        let cols = arrange(&heights, section_gap, n, max_h);
        let font = theme::body(sp.body);
        let needs: Vec<f32> = cols
            .iter()
            .map(|col| {
                let widest = col
                    .iter()
                    .flat_map(|s| BINDINGS.iter().filter(|k| k.group == order[*s]))
                    .map(|k| pt.layout_no_wrap(k.what.to_string(), font.clone(), p.text).size().x)
                    .fold(0.0, f32::max);
                (key_w + m.key_gap + widest).ceil()
            })
            .collect();
        let widths = share(&needs, avail - m.gutter * cols.len().saturating_sub(1) as f32);
        let height = cols
            .iter()
            .zip(&widths)
            .map(|(col, w)| {
                let hs: Vec<f32> = col.iter().map(|s| block_height(pt, p, sp, m, order[*s], row_gap, key_w, *w)).collect();
                column_height(&hs, section_gap)
            })
            .fold(0.0, f32::max)
            .ceil();
        Sheet { cols, widths, key_w, row_gap, section_gap, height }
    }
}

/// A section's height in a column `col_w` wide: its header, then each row with `row_gap` above it.
#[allow(clippy::too_many_arguments)]
fn block_height(pt: &egui::Painter, p: &Palette, sp: &Spacing, m: &Metrics, g: Group, row_gap: f32, key_w: f32, col_w: f32) -> f32 {
    let font = theme::body(sp.body);
    let line_h = pt.fonts_mut(|f| f.row_height(&font));
    let desc_w = (col_w - key_w - m.key_gap).max(1.0);
    let rows: f32 = BINDINGS.iter().filter(|k| k.group == g).map(|k| row_gap + description(pt, k.what, &font, p.text, desc_w).size().y.max(line_h)).sum();
    header_height(pt, sp) + rows
}

/// A section header's row: the h3 title's line.
fn header_height(pt: &egui::Painter, sp: &Spacing) -> f32 {
    pt.fonts_mut(|f| f.row_height(&theme::bold(sp.h3)))
}

/// A description wrapped to `width`. When it needs more than one line, it wraps at the narrowest width that keeps
/// the same number of lines, so its lines come out about even and no lone word is left on the last one.
fn description(pt: &egui::Painter, text: &str, font: &egui::FontId, color: egui::Color32, width: f32) -> Arc<Galley> {
    let lay = |w: f32| pt.layout(text.to_string(), font.clone(), color, w);
    let galley = lay(width);
    let lines = galley.rows.len();
    if lines < 2 {
        return galley;
    }
    let (mut lo, mut hi) = (width / lines as f32, width);
    for _ in 0..10 {
        let mid = (lo + hi) / 2.0;
        if lay(mid).rows.len() <= lines { hi = mid } else { lo = mid }
    }
    lay(hi.ceil().min(width))
}

/// The most equal columns (up to four) that fit `avail` with at least `min_desc` for descriptions beside the
/// widest key cell, and their width (to the floor of a point). It depends on the window and density only, never
/// on the screen, so the columns sit at the same x whichever screen opened the help.
fn grid(avail: f32, key_w: f32, m: &Metrics, min_desc: f32) -> (usize, f32) {
    let width = |n: usize| ((avail - m.gutter * (n - 1) as f32) / n as f32).floor();
    let n = (2..=4).rev().find(|n| width(*n) >= key_w + m.key_gap + min_desc).unwrap_or(1);
    (n, width(n).max(key_w + m.key_gap + 1.0))
}

/// One piece of a key cell: a key chip, or a muted joining word ("or", "to").
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Part<'a> {
    Key(&'a str),
    Word(&'static str),
}

/// A binding's key cell, left to right. Alternatives are separate chips joined by "or", a range's ends by "to".
/// A pair ("j/k", "Tab/⇧Tab") stays one chip; three or more keys in one ("w/n/e") are separate choices, so they
/// split into chips joined by "or" like any other alternatives.
pub fn parts(keys: &str) -> Vec<Part<'_>> {
    let mut out = Vec::new();
    for alt in keys::chips(keys) {
        let split: Vec<&str> = alt.split('/').collect();
        let choices = if split.len() > 2 && split.iter().all(|s| !s.is_empty()) { split } else { vec![alt] };
        for choice in choices {
            if !out.is_empty() {
                out.push(Part::Word("or"));
            }
            for (j, k) in keys::range(choice).into_iter().enumerate() {
                if j > 0 {
                    out.push(Part::Word("to"));
                }
                out.push(Part::Key(k));
            }
        }
    }
    out
}

/// Between a chip's edge and a joining word's ink.
const CHIP_GAP: f32 = 4.0;

/// Places a key cell's parts by their ink: each is (ink left, ink right, is a chip). A chip's box is its ink
/// plus KEY_CHIP_PAD.x either side; boxes sit CHIP_GAP apart. Returns each part's origin x and the cell's width.
pub fn place(items: &[(f32, f32, bool)]) -> (Vec<f32>, f32) {
    let mut cursor = 0.0;
    let mut xs = Vec::with_capacity(items.len());
    for (i, &(l, r, key)) in items.iter().enumerate() {
        let pad = if key { c::KEY_CHIP_PAD.x } else { 0.0 };
        if i > 0 {
            cursor += CHIP_GAP;
        }
        let x = cursor - (l - pad);
        xs.push(x);
        cursor = x + r + pad;
    }
    (xs, cursor)
}

/// The joining words' colour: the dialog's muted, which keeps AA (MUTED_MIN) on the panel in both themes.
pub fn word_color(p: &Palette) -> egui::Color32 {
    crate::views::dialog_muted(p)
}

/// A key cell's galleys (with whether each is a chip and its origin x) and its width. Chips' keys in `ink`.
fn cell(pt: &egui::Painter, keys: &str, p: &Palette, sp: &Spacing, ink: egui::Color32) -> (Vec<(Arc<Galley>, bool, f32)>, f32) {
    let galleys: Vec<(Arc<Galley>, bool)> = parts(keys)
        .into_iter()
        .map(|part| match part {
            Part::Key(k) => (pt.layout_job(crate::views::issues::key_job(k, ink)), true),
            Part::Word(w) => (pt.layout_no_wrap(w.to_string(), theme::body(sp.small), word_color(p)), false),
        })
        .collect();
    let items: Vec<(f32, f32, bool)> = galleys
        .iter()
        .map(|(g, key)| {
            let ink = theme::ink_rect(g);
            if ink.is_positive() { (ink.left(), ink.right(), *key) } else { (0.0, g.size().x, *key) }
        })
        .collect();
    let (xs, w) = place(&items);
    (galleys.into_iter().zip(xs).map(|((g, k), x)| (g, k, x)).collect(), w)
}

/// How a column draws its rows: the row gap, the sheet's key column width and the column's own width.
struct Look {
    row_gap: f32,
    key_w: f32,
    width: f32,
}

/// A row's text colour: full text while the row is wholly in view; a row the viewport cuts through is drawn in
/// the placeholder tone, the "more this way" cue (it cannot be read either way), so every readable row keeps
/// its full contrast.
fn row_ink(row: egui::Rect, clip: egui::Rect, p: &Palette) -> (egui::Color32, egui::Color32) {
    let whole = row.top() >= clip.top() - 0.5 && row.bottom() <= clip.bottom() + 0.5;
    if whole { (p.text, word_color(p)) } else { (p.placeholder(), p.placeholder()) }
}

/// One whole section in a column: its header (marked when it is the screen help was opened on), then a row per
/// binding.
fn section(ui: &mut egui::Ui, p: &Palette, sp: &Spacing, m: &Metrics, g: Group, current: bool, look: &Look) {
    let (head, _) = ui.allocate_exact_size(vec2(look.width, header_height(ui.painter(), sp)), Sense::hover());
    let mut bar = ui.new_child(egui::UiBuilder::new().max_rect(head).layout(egui::Layout::left_to_right(egui::Align::Center)));
    bar.spacing_mut().item_spacing.x = sp.card_gap;
    c::section_header(&mut bar, p, g.name(), None);
    if current {
        c::neutral_pill(&mut bar, p, "On this screen");
    }
    let font = theme::body(sp.body);
    let line_h = ui.fonts_mut(|f| f.row_height(&font));
    let desc_w = (look.width - look.key_w - m.key_gap).max(1.0);
    let container = crate::views::dialog_fill(p);
    let ppp = ui.ctx().pixels_per_point();
    let clip = ui.clip_rect();
    for k in BINDINGS.iter().filter(|k| k.group == g) {
        ui.add_space(look.row_gap);
        let size = description(ui.painter(), k.what, &font, p.text, desc_w).size();
        let (rect, _) = ui.allocate_exact_size(vec2(look.width, size.y.max(line_h)), Sense::hover());
        if !ui.is_rect_visible(rect) {
            continue;
        }
        let (ink, word) = row_ink(rect, clip, p);
        let desc = description(ui.painter(), k.what, &font, ink, desc_w);
        // The keys centre on the description's first line.
        let (pieces, _) = cell(ui.painter(), k.keys, p, sp, ink);
        for (galley, key, x) in pieces {
            let pos = egui::pos2(
                crate::views::issues::snap_px(rect.left() + x, ppp),
                crate::views::issues::snap_px(rect.top() + (line_h - galley.size().y) / 2.0, ppp),
            );
            if key {
                c::paint_key_chip(ui.painter(), pos, &galley, container, p);
            }
            ui.painter().galley(pos, galley, if key { ink } else { word });
        }
        ui.painter().galley(egui::pos2(rect.left() + look.key_w + m.key_gap, rect.top()), desc, ink);
    }
}

/// The panel's Close: the shared framed (secondary) button, BUTTON_H tall, with an Esc chip after the word.
fn close_button(ui: &mut egui::Ui, p: &Palette, sp: &Spacing) -> egui::Response {
    let key = ui.painter().layout_job(crate::views::issues::key_job("Esc", p.text));
    let ink = theme::ink_rect(&key);
    let ink = if ink.is_positive() { ink } else { egui::Rect::from_min_size(egui::Pos2::ZERO, key.size()) };
    let chip = c::key_chip_rect(ink);
    let id = egui::Id::new("help-close-esc");
    // `right_text` puts a zero-wide grow atom between the word and the chip, so the gap counts twice.
    let out = crate::views::button("Close", p).right_text(egui::Atom::custom(id, chip.size())).gap(sp.card_gap / 2.0).atom_ui(ui);
    let resp = out.response.clone().on_hover_cursor(egui::CursorIcon::PointingHand);
    if let Some(r) = out.rect(id) {
        let ppp = ui.ctx().pixels_per_point();
        // The galley goes where its chip box (ink plus KEY_CHIP_PAD) fills the atom's rect.
        let pos = egui::pos2(crate::views::issues::snap_px(r.left() - chip.left(), ppp), crate::views::issues::snap_px(r.center().y - chip.center().y, ppp));
        // The button keeps its fill when hovered, so the chip sits on it in every state.
        c::paint_key_chip(ui.painter(), pos, &key, theme::button_fill(p), p);
        ui.painter().galley(pos, key, p.text);
    }
    resp
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_screen_comes_first_then_global_and_every_group_once() {
        for s in [Screen::Issues, Screen::Grooming, Screen::Team, Screen::Roadmap, Screen::Me, Screen::Standup] {
            let order = section_order(screen_group(s));
            assert_eq!(order[0], screen_group(s));
            assert_eq!(order[1], Group::Global);
            assert_eq!(*order.last().unwrap(), Group::Pickers);
            for g in ALL {
                assert_eq!(order.iter().filter(|x| **x == g).count(), 1, "{}", g.name());
            }
        }
    }

    /// Every section exactly once, each column in reading order, the current screen's first.
    fn valid(cols: &[Vec<usize>], len: usize) -> bool {
        let mut seen: Vec<usize> = cols.iter().flatten().copied().collect();
        let ordered = cols.iter().all(|c| !c.is_empty() && c.windows(2).all(|w| w[0] < w[1]));
        seen.sort();
        ordered && seen == (0..len).collect::<Vec<_>>() && cols[0][0] == 0
    }

    fn tallest(cols: &[Vec<usize>], heights: &[f32], gap: f32) -> f32 {
        cols.iter().map(|c| column_height(&c.iter().map(|s| heights[*s]).collect::<Vec<_>>(), gap)).fold(0.0, f32::max)
    }

    #[test]
    fn arrange_keeps_reading_order_when_it_fits() {
        let heights = [50.0, 40.0, 60.0, 30.0];
        let cols = arrange(&heights, 10.0, 2, 1000.0);
        assert!(valid(&cols, 4), "{cols:?}");
        assert_eq!(cols, vec![vec![0, 1], vec![2, 3]], "in order, and as even as order allows");
        assert_eq!(arrange(&heights[..1], 10.0, 3, 1000.0), vec![vec![0]], "no empty columns");
    }

    #[test]
    fn arrange_moves_whole_sections_out_of_order_to_fit() {
        // In order, the best three columns are 290 tall ([0,1] [2] [3..]); moving a short section back fits 250.
        let heights = [90.0, 70.0, 130.0, 150.0, 40.0, 30.0, 30.0, 30.0];
        let cols = arrange(&heights, 10.0, 3, 250.0);
        assert!(valid(&cols, heights.len()), "{cols:?}");
        assert!(tallest(&cols, &heights, 10.0) <= 250.0, "{cols:?}");
        // When nothing fits, the shortest layout.
        let scrolls = arrange(&heights, 10.0, 2, 10.0);
        assert!(valid(&scrolls, heights.len()));
        assert!(tallest(&scrolls, &heights, 10.0) <= 320.0, "{scrolls:?}");
    }

    #[test]
    fn share_gives_each_column_what_it_needs_then_splits_the_rest() {
        assert_eq!(share(&[100.0, 200.0, 300.0], 900.0), vec![200.0, 300.0, 400.0], "leftover spread evenly");
        assert_eq!(share(&[100.0, 500.0, 500.0], 900.0), vec![100.0, 400.0, 400.0], "the narrow one keeps its need");
        assert_eq!(share(&[500.0, 500.0], 600.0), vec![300.0, 300.0]);
        assert!(share(&[], 600.0).is_empty());
    }

    #[test]
    fn cut_rows_are_quiet_and_whole_rows_keep_full_text() {
        let clip = egui::Rect::from_min_max(egui::pos2(0.0, 100.0), egui::pos2(400.0, 300.0));
        let row = |y: f32| egui::Rect::from_min_size(egui::pos2(0.0, y), vec2(400.0, 20.0));
        for p in [&theme::LIGHT, &theme::DARK] {
            assert_eq!(row_ink(row(280.0), clip, p).0, p.text, "touches the bottom edge, still whole");
            assert_eq!(row_ink(row(290.0), clip, p).0, p.placeholder());
            assert_eq!(row_ink(row(90.0), clip, p).0, p.placeholder());
        }
    }

    #[test]
    fn long_descriptions_wrap_evenly() {
        crate::testutil::with_ui(|ui| {
            let font = theme::body(14.0);
            let text = "Accept into Ready for Work (↑/↓ place, Enter commits)";
            let full = ui.painter().layout_no_wrap(text.to_string(), font.clone(), egui::Color32::WHITE).size().x;
            let g = description(ui.painter(), text, &font, egui::Color32::WHITE, full * 0.9);
            assert_eq!(g.rows.len(), 2);
            let (a, b) = (g.rows[0].rect().width(), g.rows[1].rect().width());
            assert!(b > a * 0.6, "no lone word on the last line: {a} {b}");
            let one = description(ui.painter(), "Refresh", &font, egui::Color32::WHITE, full);
            assert_eq!(one.rows.len(), 1);
        });
    }

    #[test]
    fn alternatives_are_separate_chips_joined_by_or() {
        use Part::{Key, Word};
        assert_eq!(parts("j/k, ↑/↓"), [Key("j/k"), Word("or"), Key("↑/↓")]);
        assert_eq!(parts("w/n/e"), [Key("w"), Word("or"), Key("n"), Word("or"), Key("e")]);
        assert_eq!(parts("⌘1 to ⌘6"), [Key("⌘1"), Word("to"), Key("⌘6")]);
        assert_eq!(parts("⌘F, /"), [Key("⌘F"), Word("or"), Key("/")]);
        assert_eq!(parts("Tab/⇧Tab"), [Key("Tab/⇧Tab")], "a pair stays one chip, like j/k");
    }

    #[test]
    fn words_sit_chip_gap_from_chip_edges() {
        // A chip with ink 2..10, "or" with ink 1..13, a chip with ink 0..8.
        let (xs, w) = place(&[(2.0, 10.0, true), (1.0, 13.0, false), (0.0, 8.0, true)]);
        let pad = c::KEY_CHIP_PAD.x;
        let chip1_right = xs[0] + 10.0 + pad;
        assert_eq!(xs[0] + 2.0 - pad, 0.0, "the first chip's edge starts the cell");
        assert_eq!(xs[1] + 1.0 - chip1_right, CHIP_GAP);
        assert_eq!((xs[2] - pad) - (xs[1] + 13.0), CHIP_GAP);
        assert_eq!(w, xs[2] + 8.0 + pad);
    }

    #[test]
    fn joining_words_pass_aa() {
        for p in [&theme::LIGHT, &theme::DARK] {
            assert!(theme::contrast(word_color(p), crate::views::dialog_fill(p)) >= 4.5, "dark={}", p.dark);
        }
    }

    #[test]
    fn spacing_follows_density_and_groups_read_apart() {
        for sp in [&theme::COMPACT, &theme::COMFORTABLE] {
            let m = Metrics::of(sp);
            assert!(m.gutter >= 2.0 * m.key_gap, "the column gutter is clearly wider than key → description");
            assert!(sp.section_gap > m.row_gap, "more space above a section than between its rows");
            assert!(m.margin < m.pad, "the panel's own padding is wider than its margin off the bars");
            assert!(m.tight_gap < m.row_gap);
            assert!(m.tight_section_gap < sp.section_gap && m.tight_section_gap > m.row_gap);
        }
        assert!(Metrics::of(&theme::COMFORTABLE).row_gap > Metrics::of(&theme::COMPACT).row_gap);
    }

    #[test]
    fn grid_is_equal_columns_that_fill_the_width() {
        let m = Metrics::of(&theme::COMPACT);
        let (n, w) = grid(1312.0, 110.0, &m, 240.0);
        assert_eq!(n, 3);
        let used = n as f32 * w + m.gutter * (n - 1) as f32;
        assert!(used <= 1312.0 && used > 1312.0 - 3.0, "the columns span the panel: {used}");
        assert_eq!(grid(800.0, 110.0, &m, 240.0).0, 2);
        assert_eq!(grid(300.0, 110.0, &m, 240.0).0, 1);
        assert_eq!(grid(2400.0, 110.0, &m, 240.0).0, 4);
    }

    #[test]
    fn panel_sits_between_the_bars_with_equal_margins() {
        for (w, h) in [(1440.0, 900.0), (1200.0, 800.0)] {
            let window = egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(w, h));
            for sp in [&theme::COMPACT, &theme::COMFORTABLE] {
                let m = Metrics::of(sp).margin;
                let r = panel_rect(window, 49.0, 24.0, m);
                assert_eq!(r.top() - 49.0, m, "clears the top bar by the margin");
                assert_eq!(r.left(), m);
                assert_eq!(w - r.right(), m);
                assert_eq!((h - 24.0) - r.bottom(), m);
            }
        }
        let tiny = panel_rect(egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(50.0, 50.0)), 49.0, 24.0, 32.0);
        assert_eq!(tiny.size(), egui::Vec2::ZERO);
    }

    #[test]
    fn sections_draw_in_a_real_pass() {
        crate::testutil::with_ui(|ui| {
            let p = theme::palette(ui.ctx());
            let sp = theme::spacing(ui.ctx());
            let key_w = BINDINGS.iter().map(|k| cell(ui.painter(), k.keys, p, sp, p.text).1).fold(0.0, f32::max);
            assert!(key_w > 0.0);
            let m = Metrics::of(sp);
            let look = Look { row_gap: m.row_gap, key_w, width: 400.0 };
            section(ui, p, sp, &m, Group::Grooming, true, &look);
            let order = section_order(Group::Standup);
            let roomy = Sheet::fit(ui.painter(), p, sp, &m, &order, 1300.0, 10_000.0);
            assert_eq!(roomy.row_gap, m.row_gap, "the density's row gap when there is room");
            assert!(valid(&roomy.cols, order.len()));
            let used: f32 = roomy.widths.iter().sum::<f32>() + m.gutter * (roomy.cols.len() - 1) as f32;
            assert!(used <= 1300.0 && used > 1300.0 - 4.0, "the columns span the panel: {used}");
            let tight = Sheet::fit(ui.painter(), p, sp, &m, &order, 1300.0, 1.0);
            assert_eq!(tight.row_gap, m.tight_gap, "the tighter gap when nothing fits");
            assert!(tight.height < roomy.height);
            assert!(close_button(ui, p, sp).rect.height() >= 24.0);
        });
    }
}
