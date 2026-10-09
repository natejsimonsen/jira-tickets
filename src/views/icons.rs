//! Small vector icons drawn with egui shapes on a 16×16 grid, so they stay
//! sharp at any size. Ported from github-rs (MIT).

use crate::theme::Palette;
use egui::{Color32, Painter, Pos2, Rect, Response, Sense, Shape, Stroke, Ui, Vec2, pos2};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Icon {
    /// Two circling arrows, for Refresh.
    Sync,
    /// A question mark, for keyboard help: painted with the sync icon's stroke so the two buttons weigh the same.
    Help,
}

/// `n + 1` points along the arc of radius `r` around `center`, from `start` to `end` radians.
fn arc_points(center: Pos2, r: f32, start: f32, end: f32, n: usize) -> Vec<Pos2> {
    (0..=n)
        .map(|i| {
            let a = start + (end - start) * i as f32 / n as f32;
            pos2(center.x + r * a.cos(), center.y + r * a.sin())
        })
        .collect()
}

pub fn paint(p: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let s = rect.width() / 16.0;
    let at = |x: f32, y: f32| pos2(rect.left() + x * s, rect.top() + y * s);
    let stroke = Stroke::new((1.5 * s).max(1.0), color);
    let line = |pts: &[(f32, f32)]| {
        p.add(Shape::line(pts.iter().map(|&(x, y)| at(x, y)).collect(), stroke));
    };
    match icon {
        Icon::Sync => {
            use std::f32::consts::PI;
            let (center, r) = (at(8.0, 8.0), 5.5 * s);
            p.add(Shape::line(arc_points(center, r, PI * 1.1, PI * 1.9, 12), stroke));
            p.add(Shape::line(arc_points(center, r, PI * 0.1, PI * 0.9, 12), stroke));
            line(&[(12.6, 1.8), (12.7, 4.7), (9.9, 4.9)]);
            line(&[(3.4, 14.2), (3.3, 11.3), (6.1, 11.1)]);
        }
        Icon::Help => {
            let (hook, dot) = help_glyph();
            p.add(Shape::line(hook.iter().map(|q| at(q.x, q.y)).collect(), stroke));
            // The dot: a disc the stroke's width across.
            p.circle_filled(at(dot.x, dot.y), stroke.width * 0.6, color);
        }
    }
}

/// The "?" on the 16 grid: its hook (an arc over the top, then the stem down the middle) and the dot's centre.
/// Its ink sits in the sync glyph's 12 px box (2..14).
pub fn help_glyph() -> (Vec<Pos2>, Pos2) {
    use std::f32::consts::PI;
    let mut hook = arc_points(pos2(8.0, 5.75), 3.0, PI, PI * 2.4, 16);
    hook.push(pos2(8.0, 9.25));
    hook.push(pos2(8.0, 10.25));
    (hook, pos2(8.0, 13.0))
}

const BUTTON: f32 = 32.0;
const GLYPH: f32 = 16.0;

/// The 32×32 button frame (hover fill when `hovered`); returns the centred 16×16 glyph rect.
fn button_frame(ui: &Ui, rect: Rect, hovered: bool, p: &Palette) -> Rect {
    // control_hover is a step off button_fill in both palettes; the header buttons share the filter row's resting fill.
    let fill = if hovered { crate::theme::control_hover(p) } else { crate::theme::button_fill(p) };
    ui.painter().rect(rect, f32::from(crate::theme::RADIUS), fill, Stroke::new(1.0, p.border), egui::StrokeKind::Inside);
    Rect::from_center_size(rect.center(), Vec2::splat(GLYPH))
}

/// A 32×32 bordered button with `icon` painted at 16 px in the middle.
pub fn icon_button(ui: &mut Ui, icon: Icon, tip: &str, p: &Palette) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(BUTTON), Sense::click());
    let glyph = button_frame(ui, rect, resp.hovered(), p);
    paint(ui.painter(), glyph, icon, p.text);
    crate::views::tooltip(resp, tip)
}

/// The same 32×32 frame with a spinner the size of the sync glyph it replaces, so nothing grows on ⌘R.
pub fn spinner_button(ui: &mut Ui, p: &Palette) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(BUTTON), Sense::hover());
    let glyph = button_frame(ui, rect, false, p);
    // `text`, like the idle sync glyph it replaces, its ring in the glyph's 12 px ink box (`spinner_arc`).
    paint_spinner(ui, Rect::from_center_size(glyph.center(), Vec2::splat(spinner_box(GLYPH))), p.text);
    resp
}

/// The square a spinner is drawn in to match the sync glyph drawn at `glyph` px: the glyph's arc radius
/// (5.5 of 16) plus the 1 px the spinner insets.
pub fn spinner_box(glyph: f32) -> f32 {
    2.0 * (glyph * 5.5 / 16.0 + 1.0)
}

/// The one spinner size: 13, the header spinner's (`spinner_box(16)`), everywhere (count row, detail,
/// composer, picker loading, sign-in). Colour rule (see theme.rs): the header spinner is `text`, as it
/// replaces the `text` sync glyph; every other spinner is `muted`, like the text beside it.
pub const INLINE_SPINNER: f32 = 13.0;

/// A `size`×`size` spinner in an exact slot, so text after it never shifts.
pub fn spinner(ui: &mut Ui, size: f32, p: &Palette) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    paint_spinner(ui, rect, p.muted);
    resp
}

/// The sync glyph's ink box at 16 px: its 5.5 arc and 1.5 stroke, 12 across.
pub const ICON_INK: f32 = 12.0;

/// Every spinner's ring as (radius, stroke): the sync glyph's 1.5 stroke, its ink exactly ICON_INK across
/// (it was 12.75 with a 1.75 stroke on a 5.5 radius), centred in whatever slot holds it.
pub fn spinner_arc() -> (f32, f32) {
    let width = 1.5;
    ((ICON_INK - width) / 2.0, width)
}

/// A three-quarter ring turning once a second, centred in `rect`, the same in both themes.
fn paint_spinner(ui: &Ui, rect: Rect, color: Color32) {
    if !ui.is_rect_visible(rect) {
        return;
    }
    ui.ctx().request_repaint();
    let t = ui.input(|i| i.time) as f32;
    let start = t * std::f32::consts::TAU;
    let (r, width) = spinner_arc();
    let pts = arc_points(rect.center(), r, start, start + std::f32::consts::TAU * 0.75, 24);
    ui.painter().add(Shape::line(pts, Stroke::new(width, color)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_arc_points_are_on_the_circle() {
        use std::f32::consts::PI;
        let c = pos2(8.0, 8.0);
        let pts = arc_points(c, 5.5, PI * 1.1, PI * 1.9, 12);
        assert_eq!(pts.len(), 13);
        for q in &pts {
            assert!(((*q - c).length() - 5.5).abs() < 1e-4, "{q:?}");
        }
        let (first, last) = (pts[0], pts[12]);
        assert!((first.x - (8.0 + 5.5 * (PI * 1.1).cos())).abs() < 1e-4 && (first.y - (8.0 + 5.5 * (PI * 1.1).sin())).abs() < 1e-4);
        assert!((last.x - (8.0 + 5.5 * (PI * 1.9).cos())).abs() < 1e-4 && (last.y - (8.0 + 5.5 * (PI * 1.9).sin())).abs() < 1e-4);
        assert!(pts.iter().all(|q| q.y < 8.0), "the 1.1π→1.9π arc is the top half");
    }

    #[test]
    fn header_spinner_matches_the_sync_glyph() {
        // The slot stays 13 (nothing moves); the ring's ink is the icon's 12 px box.
        assert_eq!(spinner_box(GLYPH), INLINE_SPINNER, "one spinner slot");
        let (r, w) = spinner_arc();
        assert_eq!(2.0 * r + w, ICON_INK, "ring ink across");
        assert!(2.0 * r + w <= INLINE_SPINNER, "inside its slot");
        assert_eq!(w, 1.5, "the icon's stroke");
    }

    #[test]
    fn help_glyph_sits_in_the_icon_box() {
        let (hook, dot) = help_glyph();
        let half = 0.75;
        let top = hook.iter().map(|q| q.y).fold(f32::INFINITY, f32::min) - half;
        let bottom = dot.y + 1.5 * 0.6;
        assert!(top >= 2.0 - 1e-3 && bottom <= 14.0, "{top} {bottom}");
        assert!(hook.iter().all(|q| (2.0..=14.0).contains(&q.x)));
        // The stem ends above the dot with a clear gap, both on the glyph's centre line.
        let stem = hook.last().unwrap();
        assert_eq!((stem.x, dot.x), (8.0, 8.0));
        assert!(dot.y - stem.y >= 2.0);
    }
}
