//! Roadmap tab 2: the year's lanes with a bar per initiative from start to target quarter, the today line, and the
//! tray of initiatives with no target.

use super::{CARD_GAP, GAP, LINE_GAP, PAD, is_p0};
use crate::roadmap::{self, Health, Horizon, Initiative, Quarter};
use crate::theme::{self, Palette};
use crate::views::components as c;
use chrono::{Datelike, NaiveDate};
use egui::{CornerRadius, Pos2, Rect, Sense, Stroke, Ui, pos2, vec2};

/// The lane-label column's width when there is more than the fallback lane to name.
pub const LABEL_W: f32 = 170.0;
/// The lane-label column's head.
pub const LANE_HEAD: &str = "Subteam";
/// The heading over the chips of initiatives with no target.
pub const TRAY: &str = "Unscheduled";
/// A bar's height, and the gap between stacked bars (the cards' gap, so rows read apart).
const BAR_ROW: f32 = 24.0;
const BAR_GAP: f32 = CARD_GAP;
/// The progress strip along a bar's bottom edge, inside its frame.
const STRIP_H: f32 = 3.0;
/// A bar's inset from its quarter column edges.
const BAR_INSET: f32 = 4.0;
/// Above and below a lane's bars, and inside a header cell above and below.
const LANE_PAD: f32 = 8.0;
/// A bar's corners.
const BAR_RADIUS: u8 = 4;
/// A bar's health glyph slot: the glyph and the gap after it.
const DOT_SLOT: f32 = super::HEALTH_DOT + 6.0;
/// The continuation chevron's width and half height, inside a bar's open end.
const CHEVRON: egui::Vec2 = vec2(4.0, 4.0);
/// The narrowest a quarter column gets before the grid scrolls sideways.
const QUARTER_MIN_W: f32 = 120.0;

/// The quarter heads for `year`: "Q1 2026" … "Q4 2026".
pub fn quarter_heads(year: i32) -> [String; 4] {
    [1, 2, 3, 4].map(|q| Quarter { year, q }.label())
}

/// The today line's label: "Oct 27".
pub fn today_label(today: NaiveDate) -> String {
    today.format("%b %-d").to_string()
}

/// Where the today label starts in the head row: `gap` right of the line, pushed past the quarter heading it
/// would cover (which ends at `heading_right`), and flipped to the line's left when it would run past `right`.
pub fn today_label_left(line_x: f32, label_w: f32, heading_right: f32, right: f32, gap: f32) -> f32 {
    let x = (line_x + gap).max(heading_right + gap);
    if x + label_w <= right { x } else { line_x - gap - label_w }
}

/// The initiatives that get a bar in `year`'s grid (the legend lists the marks these use).
pub fn bars(items: &[Initiative], year: i32) -> Vec<&Initiative> {
    items.iter().filter(|i| roadmap::bar_span(i.start, i.target, year).is_some()).collect()
}

/// Whether a bar runs on before and after `year`'s edges: (starts in an earlier year, ends in a later one).
pub fn continues(i: &Initiative, year: i32) -> (bool, bool) {
    let target = i.target;
    let start = i.start.or(target);
    (start.is_some_and(|s| s.year < year), target.is_some_and(|t| t.year > year))
}

/// The lane-label column's width: none when the only lane is the fallback ("No subteam"), which names nothing, so
/// the quarters get the room.
pub fn label_width<T>(lanes: &[(String, T)]) -> f32 {
    if lanes.iter().all(|(name, _)| name == roadmap::NO_LANE) { 0.0 } else { LABEL_W }
}

/// The five quarter-column edges from `left` across `w`: whole-point quarters, the last taking the remainder, so
/// the grid ends exactly on the page's right edge.
pub fn quarter_edges(left: f32, w: f32) -> [f32; 5] {
    let q = (w / 4.0).floor();
    [left, left + q, left + 2.0 * q, left + 3.0 * q, left + w]
}

/// A bar's hover text: name and lane, its quarters, and its progress.
pub fn bar_tip(i: &Initiative) -> String {
    let target = i.target.map(|t| t.label());
    let span = match (i.start.map(|s| s.label()), target) {
        (Some(s), Some(t)) if s != t => format!("{s} to {t}"),
        (_, Some(t)) => t,
        _ => "No target quarter".into(),
    };
    format!("{} · {}\n{span} · {} done", i.name, i.lane, super::progress_text(&i.progress))
}

/// A row index per span (1-based inclusive quarters), first fit: a bar goes on the first row whose last bar ends
/// before it starts, so a lane is as short as its overlaps allow.
pub fn pack(spans: &[(u8, u8)]) -> Vec<usize> {
    let mut ends: Vec<u8> = Vec::new();
    spans
        .iter()
        .map(|&(lo, hi)| {
            let row = ends.iter().position(|&e| e < lo).unwrap_or_else(|| {
                ends.push(0);
                ends.len() - 1
            });
            ends[row] = hi;
            row
        })
        .collect()
}

/// One lane laid out: its name, bars with their spans and rows, and its height.
struct Lane<'a> {
    label: std::sync::Arc<egui::Galley>,
    bars: Vec<(&'a Initiative, (u8, u8), usize)>,
    h: f32,
}

pub(super) fn show(ui: &mut Ui, p: &Palette, items: &[Initiative], today: NaiveDate) {
    let year = today.year();
    let (lanes, tray) = roadmap::timeline(items, year);
    let label_w = label_width(&lanes);
    let qw = (ui.available_width() - label_w).max(4.0 * QUARTER_MIN_W);
    egui::ScrollArea::horizontal().id_salt("roadmap-timeline").show(ui, |ui| grid(ui, p, &lanes, label_w, qw, today));
    if !tray.is_empty() {
        ui.add_space(GAP);
        c::section_header(ui, p, TRAY, Some(tray.len()));
        ui.add_space(CARD_GAP - ui.spacing().item_spacing.y);
        // The same bars as the grid's, sized to their content, so an initiative looks the same scheduled or not.
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(BAR_GAP, BAR_GAP);
            let font = theme::body(theme::spacing(ui.ctx()).caption);
            for i in tray {
                let name = ui.painter().layout_no_wrap(i.name.clone(), font.clone(), p.text).size().x;
                let pill = if is_p0(i) { p0_width(ui, &font) + LINE_GAP } else { 0.0 };
                let w = (2.0 * CARD_GAP + DOT_SLOT + pill + name).ceil().min(ui.available_width());
                let (bar, _) = ui.allocate_exact_size(vec2(w, BAR_ROW), Sense::hover());
                let painter = ui.painter().clone();
                draw_bar(ui, &painter, p, i, bar, (false, false));
            }
        });
    }
}

/// The grid: the head row with the today label, then a row per lane on the neutral surface with its bars, quarter
/// rules, and the today line on top. `label_w` is the lane column (0: none), `qw` the four quarters' width.
fn grid(ui: &mut Ui, p: &Palette, lanes: &[(String, Vec<&Initiative>)], label_w: f32, qw: f32, today: NaiveDate) {
    let year = today.year();
    let painter = ui.painter().clone();
    let head_font = theme::bold(theme::spacing(ui.ctx()).caption);
    let head_h = ui.fonts_mut(|f| f.row_height(&head_font)) + 2.0 * LANE_PAD;
    let laid: Vec<Lane> = lanes
        .iter()
        .map(|(name, its)| {
            let bars: Vec<(&Initiative, (u8, u8))> = its.iter().filter_map(|i| roadmap::bar_span(i.start, i.target, year).map(|s| (*i, s))).collect();
            let rows = pack(&bars.iter().map(|b| b.1).collect::<Vec<_>>());
            let count = rows.iter().max().map_or(1, |r| r + 1) as f32;
            // The fallback lane is not a name: quiet, so it never is the grid's strongest text.
            let font = if name == roadmap::NO_LANE { theme::body(13.5) } else { theme::bold(13.5) };
            let label = painter.layout(name.clone(), font, if name == roadmap::NO_LANE { p.muted } else { p.text }, (label_w - 2.0 * PAD as f32).max(0.0));
            let label_h = if label_w > 0.0 { label.size().y + 2.0 * PAD as f32 } else { 0.0 };
            let h = (2.0 * LANE_PAD + count * BAR_ROW + (count - 1.0) * BAR_GAP).max(label_h);
            Lane { label, bars: bars.into_iter().zip(rows).map(|((i, s), r)| (i, s, r)).collect(), h }
        })
        .collect();
    let empty_h = 2.0 * PAD as f32 + ui.fonts_mut(|f| f.row_height(&theme::body(14.0)));
    let body_h = if laid.is_empty() { empty_h } else { laid.iter().map(|l| l.h).sum() };
    let (rect, _) = ui.allocate_exact_size(vec2(label_w + qw, head_h + body_h), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let edges = quarter_edges(rect.left() + label_w, qw);
    let col_x = |n: f32| {
        let k = (n.floor() as usize).min(3);
        edges[k] + (n - k as f32) * (edges[k + 1] - edges[k])
    };
    painter.rect_filled(rect, theme::RADIUS, p.surface);

    // Head row: the lane column, then the four quarters, each with a rule on its left.
    let head_y = rect.top() + LANE_PAD;
    if label_w > 0.0 {
        painter.text(pos2(rect.left() + PAD as f32, head_y), egui::Align2::LEFT_TOP, LANE_HEAD, head_font.clone(), p.muted);
    }
    let mut heading_right = [0.0; 4];
    for (n, q) in quarter_heads(year).into_iter().enumerate() {
        let r = painter.text(pos2(col_x(n as f32) + PAD as f32, head_y), egui::Align2::LEFT_TOP, q, head_font.clone(), p.muted);
        heading_right[n] = r.right();
    }
    let body_top = rect.top() + head_h;
    hline(&painter, rect, body_top, p);

    // Lane rules, then the quarter rules (in the head between every column, through the lanes between quarters
    // only), then labels and bars over them.
    let tops: Vec<f32> = laid.iter().scan(body_top, |y, l| Some(std::mem::replace(y, *y + l.h))).collect();
    for &y in tops.iter().skip(1) {
        hline(&painter, rect, y, p);
    }
    // With no lane column, Q1's left rule would sit on the frame's edge.
    for n in usize::from(label_w == 0.0)..4 {
        let x = col_x(n as f32);
        let to = if n == 0 { body_top } else { rect.bottom() };
        painter.rect_filled(Rect::from_x_y_ranges(x..=x + 1.0, rect.top()..=to), 0.0, p.border);
    }
    for (lane, &y) in laid.iter().zip(&tops) {
        if label_w > 0.0 {
            painter.galley(pos2(rect.left() + PAD as f32, y + PAD as f32), lane.label.clone(), p.text);
        }
        for &(i, (lo, hi), r) in &lane.bars {
            let top = y + LANE_PAD + r as f32 * (BAR_ROW + BAR_GAP);
            let runs = continues(i, year);
            // A bar that ends in the year stops BAR_INSET inside its quarter with a rounded end; one that runs on
            // goes to the grid's edge, square, with a chevron, so the two never look alike.
            let left = if runs.0 { col_x(0.0) + 1.0 } else { col_x((lo - 1) as f32) + BAR_INSET };
            let right = if runs.1 { rect.right() - 1.0 } else { col_x(hi as f32) - BAR_INSET };
            draw_bar(ui, &painter, p, i, Rect::from_min_max(pos2(left, top), pos2(right, top + BAR_ROW)), runs);
        }
    }
    if laid.is_empty() {
        let at = pos2(col_x(0.0) + PAD as f32, body_top + PAD as f32);
        painter.text(at, egui::Align2::LEFT_TOP, format!("Nothing scheduled in {year}"), theme::body(14.0), p.muted);
    }

    // Today: a 2 px accent line through the lanes (red stays for off track), its date in the head row beside it,
    // clear of the quarter heading.
    let frac = roadmap::today_fraction(today);
    let x = col_x(4.0 * frac);
    painter.rect_filled(Rect::from_x_y_ranges(x - 1.0..=x + 1.0, head_y..=rect.bottom()), 0.0, p.accent);
    let date = painter.layout_no_wrap(today_label(today), head_font, p.accent);
    let q = ((4.0 * frac).floor() as usize).min(3);
    let left = today_label_left(x + 1.0, date.size().x, heading_right[q], rect.right() - PAD as f32, LINE_GAP);
    painter.galley(pos2(left, head_y), date, p.accent);

    painter.rect_stroke(rect, theme::RADIUS, Stroke::new(1.0, p.border), egui::StrokeKind::Inside);
}

fn hline(painter: &egui::Painter, rect: Rect, y: f32, p: &Palette) {
    painter.rect_filled(Rect::from_x_y_ranges(rect.x_range(), y..=y + 1.0), 0.0, p.border);
}

/// One bar: a resting neutral (`surface_2`; Done: the grid's `surface` with a "✓ " and `done` text) framed in the
/// cards' `border` edge, never the accent selection look; the
/// progress bars' done and in-progress segments as a strip along its bottom; the health dot, the P0 pill (the
/// cards' one P0 marker), the name cut to fit, and a chevron at an end that runs past the year. Hovering shows
/// the full name, subteam, quarters and progress.
fn draw_bar(ui: &mut Ui, painter: &egui::Painter, p: &Palette, i: &Initiative, bar: Rect, (before, after): (bool, bool)) {
    let done = i.horizon == Horizon::Done;
    let r = BAR_RADIUS;
    let corners = CornerRadius { nw: if before { 0 } else { r }, sw: if before { 0 } else { r }, ne: if after { 0 } else { r }, se: if after { 0 } else { r } };
    let (fill, fg) = bar_colors(p, done);
    painter.rect_filled(bar, corners, fill);
    progress_strip(painter, p, &i.progress, bar.shrink(1.0), corners);
    painter.rect_stroke(bar, corners, Stroke::new(1.0, bar_edge(p)), egui::StrokeKind::Inside);
    let mut x = bar.left() + CARD_GAP;
    if before {
        chevron(painter, pos2(x, bar.center().y), false, p);
        x += CHEVRON.x + LINE_GAP;
    }
    if !done {
        bar_dot(painter, pos2(x + super::HEALTH_DOT / 2.0, bar.center().y), i.health, p);
        x += DOT_SLOT;
    }
    let mut end = bar.right() - CARD_GAP;
    if after {
        chevron(painter, pos2(end, bar.center().y), true, p);
        end -= CHEVRON.x + LINE_GAP;
    }
    let font = theme::body(theme::spacing(ui.ctx()).caption);
    if is_p0(i) && p0_width(ui, &font) <= end - x {
        let slot = Rect::from_x_y_ranges(x..=end, bar.y_range());
        let layout = egui::Layout::left_to_right(egui::Align::Center);
        let pill = ui.scope_builder(egui::UiBuilder::new().max_rect(slot).layout(layout), |ui| super::p0_chip(ui, p)).inner;
        x = pill.rect.right() + LINE_GAP;
    }
    let name = if done { format!("✓ {}", i.name) } else { i.name.clone() };
    let mut job = egui::text::LayoutJob::simple_singleline(name, font, fg);
    job.wrap = egui::text::TextWrapping::truncate_at_width((end - x).max(0.0));
    let galley = painter.layout_job(job);
    painter.galley(pos2(x, bar.center().y - galley.size().y / 2.0), galley, fg);
    let resp = ui.interact(bar, egui::Id::new(("roadmap-bar", &i.key)), Sense::hover());
    crate::views::tooltip(resp, bar_tip(i));
}

/// The progress bars' done (`on_track`, solid) and in-progress (striped, `paint_doing`) segments as a STRIP_H strip on `inner`'s bottom
/// edge; the to-do rest is the bar's own fill. Its bottom corners follow the bar's.
fn progress_strip(painter: &egui::Painter, p: &Palette, pr: &roadmap::Progress, inner: Rect, corners: CornerRadius) {
    let total = (pr.todo + pr.doing + pr.done) as f32;
    if total == 0.0 {
        return;
    }
    let strip = Rect::from_x_y_ranges(inner.x_range(), inner.bottom() - STRIP_H..=inner.bottom());
    let clip = painter.with_clip_rect(strip.intersect(painter.clip_rect()));
    // Each segment from the left edge with the bar's bottom-left corner; the bottom-right one only when it reaches
    // the bar's end, so the joins stay square.
    let round =
        |n: usize| CornerRadius { nw: 0, ne: 0, sw: corners.sw.saturating_sub(1), se: if n as f32 >= total { corners.se.saturating_sub(1) } else { 0 } };
    let seg = |n: usize, color| {
        clip.rect_filled(Rect::from_min_size(strip.min, vec2(strip.width() * n as f32 / total, STRIP_H)), round(n), color);
    };
    let doing = pr.done + pr.doing;
    if doing > pr.done {
        let w = strip.width() * doing as f32 / total;
        // Stripes clear of the bar's rounded bottom corners, as on the cards' bars.
        let lo = if pr.done > 0 { strip.left() + strip.width() * pr.done as f32 / total } else { strip.left() + f32::from(corners.sw) };
        let hi = (strip.left() + w).min(strip.right() - f32::from(corners.se));
        super::paint_doing(&clip, Rect::from_min_size(strip.min, vec2(w, STRIP_H)), round(doing), egui::Rangef::new(lo, hi), p);
    }
    seg(pr.done, p.on_track);
}

/// A bar's frame: the cards' resting edge (`components::card_stroke`), so bars weigh no more than the cards and
/// tiles on the other tabs; the `surface_2` fill already sets the span off the lane.
pub(super) fn bar_edge(p: &Palette) -> egui::Color32 {
    c::card_stroke(p, false).color
}

/// The P0 pill's width at the pill `font` (its text plus the pill's side margins).
fn p0_width(ui: &Ui, font: &egui::FontId) -> f32 {
    ui.painter().layout_no_wrap(super::P0_LABEL.into(), font.clone(), egui::Color32::PLACEHOLDER).size().x + 2.0 * crate::views::PILL_MARGIN.x
}

/// A bar's (fill, text): the resting neutral, or Done's quieter grid surface and `done` text.
pub(super) fn bar_colors(p: &Palette, done: bool) -> (egui::Color32, egui::Color32) {
    if done { (p.surface, p.done) } else { (p.surface_2, p.text) }
}

/// The continuation chevron: a `muted` "›" (or "‹") whose tip is at `tip`.
pub(super) fn chevron(painter: &egui::Painter, tip: Pos2, right: bool, p: &Palette) {
    let back = if right { -CHEVRON.x } else { CHEVRON.x };
    let pts = vec![pos2(tip.x + back, tip.y - CHEVRON.y), tip, pos2(tip.x + back, tip.y + CHEVRON.y)];
    painter.add(egui::Shape::line(pts, Stroke::new(1.5, p.muted)));
}

/// The health dot on a bar: the roadmap's one health glyph.
fn bar_dot(painter: &egui::Painter, c: Pos2, h: Health, p: &Palette) {
    super::paint_health(painter, c, h, p);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wording() {
        assert_eq!(quarter_heads(2026), ["Q1 2026", "Q2 2026", "Q3 2026", "Q4 2026"]);
        assert_eq!(today_label(NaiveDate::from_ymd_opt(2026, 10, 6).unwrap()), "Oct 6");
        assert_eq!((LANE_HEAD, TRAY), ("Subteam", "Unscheduled"));
    }

    #[test]
    fn the_today_label_clears_the_heading_and_the_edge() {
        assert_eq!(today_label_left(200.0, 40.0, 150.0, 400.0, 4.0), 204.0, "beside the line");
        assert_eq!(today_label_left(120.0, 40.0, 160.0, 400.0, 4.0), 164.0, "past the quarter heading it would cover");
        assert_eq!(today_label_left(380.0, 40.0, 150.0, 400.0, 4.0), 336.0, "flipped left at the grid's edge");
    }

    #[test]
    fn the_lane_column_and_quarters_fill_the_grid() {
        let lane = |n: &str| (n.to_string(), ());
        assert_eq!(label_width(&[lane(roadmap::NO_LANE)]), 0.0, "only the fallback lane: no lane column");
        assert_eq!(label_width(&[lane("Platform"), lane(roadmap::NO_LANE)]), LABEL_W);
        assert_eq!(label_width::<()>(&[]), 0.0);
        let e = quarter_edges(10.0, 1001.0);
        assert_eq!(e, [10.0, 260.0, 510.0, 760.0, 1011.0], "whole quarters, the last takes the remainder");
    }

    #[test]
    fn bar_edges_are_the_card_edge() {
        for p in [&theme::LIGHT, &theme::DARK] {
            assert_eq!(bar_edge(p), p.border, "one edge role for card-like surfaces, dark={}", p.dark);
            assert_ne!(bar_colors(p, false).0, p.surface, "the fill, not the edge, sets a bar off the lane");
        }
    }

    #[test]
    fn pack_reuses_the_first_row_that_has_ended() {
        assert_eq!(pack(&[]), Vec::<usize>::new());
        assert_eq!(pack(&[(1, 1), (2, 3), (2, 2), (4, 4), (3, 4)]), [0, 0, 1, 0, 1]);
        assert_eq!(pack(&[(1, 4), (1, 4), (2, 2)]), [0, 1, 2]);
    }
}
