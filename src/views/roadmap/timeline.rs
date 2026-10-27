//! Roadmap tab 2: the year's lanes with a bar per initiative from start to target quarter, the today line, and the
//! tray of initiatives with no target.

use super::{CARD_GAP, GAP, LINE_GAP, PAD};
use crate::roadmap::{self, Health, Horizon, Initiative, Quarter};
use crate::theme::{self, Palette};
use chrono::{Datelike, NaiveDate};
use egui::{Color32, CornerRadius, Pos2, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};

/// The lane-label column's width.
pub const LABEL_W: f32 = 170.0;
/// The lane-label column's head.
pub const LANE_HEAD: &str = "Subteam";
/// The heading over the chips of initiatives with no target.
pub const TRAY: &str = "Unscheduled";
/// A bar's height, and the gap between stacked bars.
const BAR_ROW: f32 = 24.0;
const BAR_GAP: f32 = 4.0;
/// A bar's inset from its quarter column edges.
const BAR_INSET: f32 = 4.0;
/// Above and below a lane's bars, and inside a header cell above and below.
const LANE_PAD: f32 = 8.0;
/// A P0 bar's `off_track` left edge.
const P0_EDGE: f32 = 3.0;
/// A bar's corners.
const BAR_RADIUS: u8 = 4;
/// The lane tint: the epic slot colour at this alpha, the same in both themes.
const LANE_ALPHA: f32 = 0.08;
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

/// A lane's tint: the `n`th epic slot colour, faint.
fn lane_tint(n: usize, p: &Palette) -> Color32 {
    theme::slot_tint(theme::EPIC_SLOT_ORDER[n % theme::EPIC_SLOTS], p).fg.gamma_multiply(LANE_ALPHA)
}

/// One lane laid out: its name, tint, bars with their spans and rows, and its height.
struct Lane<'a> {
    label: std::sync::Arc<egui::Galley>,
    tint: Color32,
    bars: Vec<(&'a Initiative, (u8, u8), usize)>,
    h: f32,
}

pub(super) fn show(ui: &mut Ui, p: &Palette, items: &[Initiative], today: NaiveDate) {
    let year = today.year();
    let (lanes, tray) = roadmap::timeline(items, year);
    let qw = ((ui.available_width() - LABEL_W) / 4.0).floor().max(QUARTER_MIN_W);
    egui::ScrollArea::horizontal().id_salt("roadmap-timeline").show(ui, |ui| grid(ui, p, &lanes, qw, today));
    ui.add_space(GAP);
    if !tray.is_empty() {
        ui.label(RichText::new(TRAY).font(theme::bold(12.0)).color(p.muted));
        ui.add_space(CARD_GAP - ui.spacing().item_spacing.y);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(LINE_GAP, LINE_GAP);
            let fill = theme::chip_fill_on(p.bg, p);
            for i in tray {
                crate::views::pill(ui, &format!("{} · {}", i.name, i.lane), fill, p.muted);
            }
        });
    }
}

/// The grid: the head row, then a tinted row per lane with its bars, quarter rules, and the today line on top.
fn grid(ui: &mut Ui, p: &Palette, lanes: &[(String, Vec<&Initiative>)], qw: f32, today: NaiveDate) {
    let year = today.year();
    let painter = ui.painter().clone();
    let head_font = theme::bold(12.0);
    let head_h = ui.fonts_mut(|f| f.row_height(&head_font)) + 2.0 * LANE_PAD;
    let laid: Vec<Lane> = lanes
        .iter()
        .enumerate()
        .map(|(n, (name, its))| {
            let bars: Vec<(&Initiative, (u8, u8))> = its.iter().filter_map(|i| roadmap::bar_span(i.start, i.target, year).map(|s| (*i, s))).collect();
            let rows = pack(&bars.iter().map(|b| b.1).collect::<Vec<_>>());
            let count = rows.iter().max().map_or(1, |r| r + 1) as f32;
            let label = painter.layout(name.clone(), theme::bold(13.5), p.text, LABEL_W - 2.0 * PAD as f32);
            let h = (2.0 * LANE_PAD + count * BAR_ROW + (count - 1.0) * BAR_GAP).max(label.size().y + 2.0 * PAD as f32);
            Lane { label, tint: lane_tint(n, p), bars: bars.into_iter().zip(rows).map(|((i, s), r)| (i, s, r)).collect(), h }
        })
        .collect();
    let empty_h = 2.0 * PAD as f32 + ui.fonts_mut(|f| f.row_height(&theme::body(14.0)));
    let body_h = if laid.is_empty() { empty_h } else { laid.iter().map(|l| l.h).sum() };
    let (rect, _) = ui.allocate_exact_size(vec2(LABEL_W + 4.0 * qw, head_h + body_h), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let col_x = |n: f32| rect.left() + LABEL_W + n * qw;
    painter.rect_filled(rect, theme::RADIUS, p.surface);

    // Head row: the lane column, then the four quarters, each with a rule on its left.
    let head_y = rect.top() + LANE_PAD;
    painter.text(pos2(rect.left() + PAD as f32, head_y), egui::Align2::LEFT_TOP, LANE_HEAD, head_font.clone(), p.muted);
    for (n, q) in quarter_heads(year).into_iter().enumerate() {
        painter.text(pos2(col_x(n as f32) + PAD as f32, head_y), egui::Align2::LEFT_TOP, q, head_font.clone(), p.muted);
    }
    let body_top = rect.top() + head_h;
    hline(&painter, rect, body_top, p);

    // Lanes: tint and top rule, then the quarter rules (in the head between every column, through the lanes between
    // quarters only), then labels and bars over them.
    let last = laid.len().saturating_sub(1);
    let tops: Vec<f32> = laid.iter().scan(body_top, |y, l| Some(std::mem::replace(y, *y + l.h))).collect();
    for (n, (lane, &y)) in laid.iter().zip(&tops).enumerate() {
        let row = Rect::from_min_max(pos2(rect.left(), y), pos2(rect.right(), y + lane.h));
        let r = theme::RADIUS;
        let corners = if n == last { CornerRadius { nw: 0, ne: 0, sw: r, se: r } } else { CornerRadius::ZERO };
        painter.rect_filled(row, corners, lane.tint);
        if n > 0 {
            hline(&painter, rect, y, p);
        }
    }
    for n in 0..4 {
        let x = col_x(n as f32);
        let to = if n == 0 { body_top } else { rect.bottom() };
        painter.rect_filled(Rect::from_x_y_ranges(x..=x + 1.0, rect.top()..=to), 0.0, p.border);
    }
    for (lane, &y) in laid.iter().zip(&tops) {
        painter.galley(pos2(rect.left() + PAD as f32, y + PAD as f32), lane.label.clone(), p.text);
        for &(i, (lo, hi), r) in &lane.bars {
            let top = y + LANE_PAD + r as f32 * (BAR_ROW + BAR_GAP);
            let bar = Rect::from_min_max(pos2(col_x((lo - 1) as f32) + BAR_INSET, top), pos2(col_x(hi as f32) - BAR_INSET, top + BAR_ROW));
            draw_bar(ui, &painter, p, i, bar);
        }
    }
    if laid.is_empty() {
        let at = pos2(col_x(0.0) + PAD as f32, body_top + PAD as f32);
        painter.text(at, egui::Align2::LEFT_TOP, format!("Nothing scheduled in {year}"), theme::body(14.0), p.muted);
    }

    // Today: a 2 px line through the grid, its date just under the head.
    let x = col_x(4.0 * roadmap::today_fraction(today));
    painter.rect_filled(Rect::from_x_y_ranges(x - 1.0..=x + 1.0, rect.top()..=rect.bottom()), 0.0, p.today);
    let date = painter.layout_no_wrap(today_label(today), theme::bold(12.0), p.today);
    let at = pos2(x + 1.0 + LINE_GAP, body_top + LINE_GAP);
    // Backed by the grid's surface so the date stays legible over a bar.
    painter.rect_filled(Rect::from_min_size(at, date.size()).expand2(vec2(2.0, 0.0)), 2.0, p.surface);
    painter.galley(at, date, p.today);

    painter.rect_stroke(rect, theme::RADIUS, Stroke::new(1.0, p.border), egui::StrokeKind::Inside);
}

fn hline(painter: &egui::Painter, rect: Rect, y: f32, p: &Palette) {
    painter.rect_filled(Rect::from_x_y_ranges(rect.x_range(), y..=y + 1.0), 0.0, p.border);
}

/// One bar: `accent` (Done `done` with a "✓ "), a P0's `off_track` left edge, the health dot and the name cut to fit.
/// Hovering shows the full name.
fn draw_bar(ui: &mut Ui, painter: &egui::Painter, p: &Palette, i: &Initiative, bar: Rect) {
    let done = i.horizon == Horizon::Done;
    painter.rect_filled(bar, BAR_RADIUS, if done { p.done } else { p.accent });
    if i.priority.as_deref() == Some("P0") {
        let edge = Rect::from_min_size(bar.min, vec2(P0_EDGE, bar.height()));
        painter.rect_filled(edge, CornerRadius { nw: BAR_RADIUS, sw: BAR_RADIUS, ne: 0, se: 0 }, p.off_track);
    }
    let mut x = bar.left() + CARD_GAP;
    if !done {
        bar_dot(painter, pos2(x + 4.5, bar.center().y), i.health, p);
        x += 9.0 + 6.0;
    }
    let name = if done { format!("✓ {}", i.name) } else { i.name.clone() };
    let mut job = egui::text::LayoutJob::simple_singleline(name, theme::body(12.0), p.on_accent);
    job.wrap = egui::text::TextWrapping::truncate_at_width((bar.right() - CARD_GAP - x).max(0.0));
    let galley = painter.layout_job(job);
    painter.galley(pos2(x, bar.center().y - galley.size().y / 2.0), galley, p.on_accent);
    let resp = ui.interact(bar, egui::Id::new(("roadmap-bar", &i.key)), Sense::hover());
    crate::views::tooltip(resp, i.name.clone());
}

/// The health dot on a bar: ringed in `on_accent` so it reads on the fill; a hollow ring when no health is set.
fn bar_dot(painter: &egui::Painter, c: Pos2, h: Health, p: &Palette) {
    match h {
        Health::None => {
            painter.circle_stroke(c, 3.75, Stroke::new(1.5, p.on_accent));
        }
        _ => {
            painter.circle_filled(c, 4.5, p.on_accent);
            painter.circle_filled(c, 3.5, super::health_color(h, p));
        }
    }
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
    fn pack_reuses_the_first_row_that_has_ended() {
        assert_eq!(pack(&[]), Vec::<usize>::new());
        assert_eq!(pack(&[(1, 1), (2, 3), (2, 2), (4, 4), (3, 4)]), [0, 0, 1, 0, 1]);
        assert_eq!(pack(&[(1, 4), (1, 4), (2, 2)]), [0, 1, 2]);
    }
}
