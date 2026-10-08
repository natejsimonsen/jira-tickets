//! Small vector icons drawn with egui shapes on a 16×16 grid, so they stay
//! sharp at any size. Ported from github-rs (MIT).

use crate::theme::Palette;
use egui::{Color32, Painter, Pos2, Rect, Response, Sense, Shape, Stroke, Ui, Vec2, pos2};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Icon {
    /// Two circling arrows, for Refresh.
    Sync,
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
    }
}

const BUTTON: f32 = 32.0;
const GLYPH: f32 = 16.0;

/// The 32×32 button frame (hover fill when `hovered`); returns the centred 16×16 glyph rect.
fn button_frame(ui: &Ui, rect: Rect, hovered: bool, p: &Palette) -> Rect {
    let fill = if hovered { p.hover_row } else { p.surface_2 };
    ui.painter().rect(rect, 6.0, fill, Stroke::new(1.0, p.border), egui::StrokeKind::Inside);
    Rect::from_center_size(rect.center(), Vec2::splat(GLYPH))
}

/// A 32×32 bordered button with `icon` painted at 16 px in the middle.
pub fn icon_button(ui: &mut Ui, icon: Icon, tip: &str, p: &Palette) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(BUTTON), Sense::click());
    let glyph = button_frame(ui, rect, resp.hovered(), p);
    paint(ui.painter(), glyph, icon, p.text);
    resp.on_hover_text(tip)
}

/// The same 32×32 frame with a 16 px spinner, so the slot does not jump while loading.
pub fn spinner_button(ui: &mut Ui, p: &Palette) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(BUTTON), Sense::hover());
    let glyph = button_frame(ui, rect, false, p);
    ui.put(glyph, egui::Spinner::new().size(GLYPH).color(p.muted));
    resp
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
}
