//! Colours from the roadmap mockup tokens, light and dark, and system fonts.

use egui::{Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Shadow, Stroke, Visuals};
use std::collections::BTreeMap;
use std::sync::Arc;

pub const BOLD: &str = "bold";

pub fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(BOLD.into()))
}
pub fn body(size: f32) -> FontId {
    FontId::proportional(size)
}
pub fn mono(size: f32) -> FontId {
    FontId::monospace(size)
}

#[derive(Clone, Copy)]
pub struct Palette {
    pub dark: bool,
    pub bg: Color32,
    pub surface: Color32,
    pub surface_2: Color32,
    pub border: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub accent: Color32,
    /// Text and glyphs on an `accent` fill (primary buttons, active toggles).
    pub on_accent: Color32,
    pub on_track: Color32,
    pub at_risk: Color32,
    pub off_track: Color32,
    pub no_health: Color32,
    pub done: Color32,
    pub today: Color32,
    pub selected_row: Color32,
    pub hover_row: Color32,
    pub danger_subtle: Color32,
    pub tooltip: Color32,
}

const fn hex(v: u32) -> Color32 {
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

pub const LIGHT: Palette = Palette {
    dark: false,
    bg: hex(0xf7f7f5),
    surface: hex(0xffffff),
    surface_2: hex(0xf0f0ed),
    border: hex(0xdad9d4),
    text: hex(0x1f1f1d),
    muted: hex(0x62615d),
    accent: hex(0x2f6fdb),
    on_accent: hex(0xffffff),
    on_track: hex(0x2e8b57),
    at_risk: hex(0xb07a0c),
    off_track: hex(0xb5352f),
    no_health: hex(0x8a8984),
    done: hex(0x6b6a66),
    today: hex(0xc8403a),
    selected_row: hex(0xdde7fa),
    hover_row: hex(0xf0f0ed),
    danger_subtle: hex(0xfbe9e8),
    tooltip: hex(0x25292e),
};

pub const DARK: Palette = Palette {
    dark: true,
    bg: hex(0x171716),
    surface: hex(0x20201f),
    surface_2: hex(0x2a2a28),
    border: hex(0x3a3a37),
    text: hex(0xecebe8),
    muted: hex(0xa3a29d),
    accent: hex(0x7aa5f0),
    on_accent: hex(0x171716),
    on_track: hex(0x5cc28a),
    at_risk: hex(0xe6b04a),
    off_track: hex(0xec6d66),
    no_health: hex(0x7d7b76),
    done: hex(0xa3a29d),
    today: hex(0xec6d66),
    selected_row: hex(0x26334d),
    hover_row: hex(0x343432),
    danger_subtle: hex(0x3a2422),
    tooltip: hex(0x454b54),
};

pub fn palette(ctx: &egui::Context) -> &'static Palette {
    match ctx.theme() {
        egui::Theme::Dark => &DARK,
        egui::Theme::Light => &LIGHT,
    }
}

/// Background and text colour for a coloured pill.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tint {
    pub bg: Color32,
    pub fg: Color32,
    /// The row's left edge: the hue at the pill text's tone, so stripe and pill read as one colour.
    pub stripe: Color32,
}

const GREY: u32 = 0x8a8984;
const BLUE: u32 = 0x3d7ea6;
const TEAL: u32 = 0x0f9d9a;
const ACCENT: u32 = 0x2f6fdb;
const PURPLE: u32 = 0x8250df;
const RED: u32 = 0xc8403a;
const GREEN: u32 = 0x2e8b57;

/// Distinct hues for epics, picked by a stable hash of the epic key. Every pair is at least 25° apart in
/// hue angle and each sits at least 25° from the danger red (0–15°), at-risk amber (35–45°), Done green
/// (140–150°) and the accent (~218°); two violets at most. Those bands leave about 155° of the wheel,
/// which holds seven hues at 25° spacing: 72°, 98°, 184°, 244°, 270°, 296°, 322°.
pub const EPIC_HUES: [u32; 7] = [0x7d9325, 0x4e8e29, 0x1b8f98, 0x7871d6, 0x9961d1, 0xbe53c6, 0xca499b];

/// `#rrggbb` or `rrggbb`.
pub fn parse_hex(s: &str) -> Option<Color32> {
    let h = s.trim();
    let h = h.strip_prefix('#').unwrap_or(h);
    if h.len() != 6 || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(h, 16).ok().map(hex)
}

/// FNV-1a 64 of the key, modulo the epic palette; stable across Rust versions.
pub fn stable_index(key: &str) -> usize {
    let h = key.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3));
    (h % EPIC_HUES.len() as u64) as usize
}

fn mix(c: Color32, toward: u8, t: f32) -> Color32 {
    let m = |v: u8| (f32::from(v) + (f32::from(toward) - f32::from(v)) * t).round() as u8;
    Color32::from_rgb(m(c.r()), m(c.g()), m(c.b()))
}

/// Light-theme wash alpha: heavy enough that an In Progress pill on a plain row never reads as the
/// (accent-tinted) selected-row fill.
const LIGHT_WASH: u8 = 76;
const DARK_WASH: u8 = 60;

/// A translucent wash of the hue behind text in a darker (light theme) or
/// lighter (dark theme) shade of it, for at least 4.5:1 contrast. The row's edge stripe is the
/// base hue itself in light (10% toward black) and the text tone in dark.
pub fn tint_from_hue(base: Color32, p: &Palette) -> Tint {
    let (alpha, fg, stripe) = if p.dark {
        let fg = mix(base, 255, 0.5);
        (DARK_WASH, fg, fg)
    } else {
        (LIGHT_WASH, mix(base, 0, 0.45), mix(base, 0, 0.1))
    };
    Tint { bg: Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), alpha), fg, stripe }
}

/// Override (case-insensitive name) > known status name > status category.
pub fn status_tint(status_name: &str, category: &str, p: &Palette, overrides: &BTreeMap<String, String>) -> Tint {
    let over = overrides.iter().find(|(k, _)| k.eq_ignore_ascii_case(status_name)).and_then(|(_, v)| parse_hex(v));
    let base = over.unwrap_or_else(|| {
        hex(match status_name.to_ascii_lowercase().as_str() {
            "new" => GREY,
            "planning" => BLUE,
            "ready for work" => TEAL,
            "in progress" => ACCENT,
            "in review" | "code review" => PURPLE,
            "blocked" => RED,
            "done" | "closed" | "resolved" => GREEN,
            _ => match category {
                "indeterminate" => ACCENT,
                "done" => GREEN,
                _ => GREY,
            },
        })
    });
    tint_from_hue(base, p)
}

/// Override by exact epic key, else a stable hue from the key.
pub fn epic_tint(epic_key: &str, p: &Palette, overrides: &BTreeMap<String, String>) -> Tint {
    let base = overrides.get(epic_key).and_then(|v| parse_hex(v)).unwrap_or_else(|| hex(EPIC_HUES[stable_index(epic_key)]));
    tint_from_hue(base, p)
}

fn visuals(p: &Palette) -> Visuals {
    let mut v = if p.dark { Visuals::dark() } else { Visuals::light() };
    v.panel_fill = p.bg;
    v.window_fill = p.surface;
    v.extreme_bg_color = p.surface;
    v.faint_bg_color = p.surface_2;
    v.override_text_color = Some(p.text);
    v.hyperlink_color = p.accent;
    // Hint (placeholder) text uses this; egui's default fades `text`, which drops below 4.5:1 in light.
    v.weak_text_color = Some(p.muted);
    v.selection.bg_fill = p.selected_row;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    v.widgets.inactive.bg_fill = p.surface_2;
    v.widgets.inactive.weak_bg_fill = p.surface_2;
    v.widgets.hovered.bg_fill = p.hover_row;
    v.widgets.hovered.weak_bg_fill = p.hover_row;
    v.widgets.active.bg_fill = p.selected_row;
    v.widgets.active.weak_bg_fill = p.selected_row;
    // Every control gets a 1 px edge in both themes; focus and press are the accent, never near-white.
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, p.border);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, p.border);
    v.widgets.open.bg_stroke = Stroke::new(1.0, p.border);
    v.widgets.open.bg_fill = p.surface_2;
    v.widgets.open.weak_bg_fill = p.surface_2;
    v.widgets.active.bg_stroke = Stroke::new(1.0, p.accent);
    v.selection.stroke = Stroke::new(1.0, p.accent);
    for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
        w.corner_radius = CornerRadius::same(6);
        w.expansion = 0.0;
    }
    v.popup_shadow = Shadow::NONE;
    v
}

/// WCAG 2 contrast ratio of two opaque colours (1.0 to 21.0).
pub fn contrast(a: Color32, b: Color32) -> f32 {
    let lum = |c: Color32| {
        let lin = |v: u8| {
            let v = f32::from(v) / 255.0;
            if v <= 0.039_28 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * lin(c.r()) + 0.7152 * lin(c.g()) + 0.0722 * lin(c.b())
    };
    let (x, y) = (lum(a), lum(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

/// `fg` (possibly translucent) composited over the opaque `bg`.
pub fn over(fg: Color32, bg: Color32) -> Color32 {
    let [r, g, b, a] = fg.to_srgba_unmultiplied();
    let t = f32::from(a) / 255.0;
    let m = |f: u8, b: u8| (f32::from(f) * t + f32::from(b) * (1.0 - t)).round() as u8;
    Color32::from_rgb(m(r, bg.r()), m(g, bg.g()), m(b, bg.b()))
}

pub fn apply(ctx: &egui::Context) {
    ctx.set_visuals_of(egui::Theme::Light, visuals(&LIGHT));
    ctx.set_visuals_of(egui::Theme::Dark, visuals(&DARK));
    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        s.spacing.button_padding = egui::vec2(12.0, 4.0);
        s.text_styles.insert(egui::TextStyle::Body, body(14.0));
        s.text_styles.insert(egui::TextStyle::Button, body(14.0));
        s.text_styles.insert(egui::TextStyle::Small, body(12.0));
        s.text_styles.insert(egui::TextStyle::Monospace, mono(12.5));
        s.text_styles.insert(egui::TextStyle::Heading, bold(20.0));
    });
    ctx.set_fonts(fonts());
}

struct Face {
    path: &'static str,
    index: u32,
}

#[cfg(target_os = "macos")]
const REGULAR: &[Face] = &[Face { path: "/System/Library/Fonts/SFNS.ttf", index: 0 }, Face { path: "/System/Library/Fonts/HelveticaNeue.ttc", index: 0 }];
#[cfg(target_os = "macos")]
const SEMIBOLD: &[Face] = &[Face { path: "/System/Library/Fonts/HelveticaNeue.ttc", index: 1 }];
#[cfg(target_os = "macos")]
const MONO: &[Face] = &[Face { path: "/System/Library/Fonts/SFNSMono.ttf", index: 0 }, Face { path: "/System/Library/Fonts/Menlo.ttc", index: 0 }];
#[cfg(not(target_os = "macos"))]
const REGULAR: &[Face] = &[Face { path: "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", index: 0 }];
#[cfg(not(target_os = "macos"))]
const SEMIBOLD: &[Face] = &[Face { path: "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf", index: 0 }];
#[cfg(not(target_os = "macos"))]
const MONO: &[Face] = &[Face { path: "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf", index: 0 }];

fn load(faces: &[Face]) -> Option<FontData> {
    faces.iter().find_map(|f| {
        let bytes = std::fs::read(f.path).ok()?;
        let mut data = FontData::from_owned(bytes);
        data.index = f.index;
        Some(data)
    })
}

fn fonts() -> FontDefinitions {
    let mut defs = FontDefinitions::default();
    let builtin_prop = defs.families[&FontFamily::Proportional].clone();
    let builtin_mono = defs.families[&FontFamily::Monospace].clone();
    let mut family = |name: &str, faces: &[Face], fallback: &[String]| -> Vec<String> {
        let mut list = Vec::new();
        if let Some(data) = load(faces) {
            defs.font_data.insert(name.to_string(), Arc::new(data));
            list.push(name.to_string());
        }
        list.extend(fallback.iter().cloned());
        list
    };
    let prop = family("system", REGULAR, &builtin_prop);
    let bold = family("system-bold", SEMIBOLD, &prop);
    let mono = family("system-mono", MONO, &builtin_mono);
    defs.families.insert(FontFamily::Proportional, prop);
    defs.families.insert(FontFamily::Name(BOLD.into()), bold);
    defs.families.insert(FontFamily::Monospace, mono);
    defs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn none() -> BTreeMap<String, String> {
        BTreeMap::new()
    }

    #[test]
    fn parse_hex_accepts_with_and_without_hash() {
        assert_eq!(parse_hex("#0f9d9a"), Some(Color32::from_rgb(0x0f, 0x9d, 0x9a)));
        assert_eq!(parse_hex("8250DF"), Some(Color32::from_rgb(0x82, 0x50, 0xdf)));
        assert_eq!(parse_hex("#fff"), None);
        assert_eq!(parse_hex("#gggggg"), None);
        assert_eq!(parse_hex(""), None);
        assert_eq!(parse_hex("#1234567"), None);
    }

    #[test]
    fn stable_index_is_deterministic_and_in_range() {
        assert_eq!(stable_index("CI-1860"), stable_index("CI-1860"));
        assert!(stable_index("CI-1860") < EPIC_HUES.len());
        assert!(stable_index("") < EPIC_HUES.len());
        assert_ne!(stable_index("CI-1860"), stable_index("CI-1569"));
    }

    #[test]
    fn status_tint_prefers_override_then_name_then_category() {
        for p in [&LIGHT, &DARK] {
            assert_ne!(status_tint("Blocked", "new", p, &none()).fg, status_tint("New", "new", p, &none()).fg);
            // Name wins over category: "Done" is green whatever the category says.
            assert_eq!(status_tint("done", "new", p, &none()), tint_from_hue(hex(GREEN), p));
            // Unknown name falls back to category.
            assert_eq!(status_tint("Weird", "indeterminate", p, &none()), tint_from_hue(hex(ACCENT), p));
            assert_eq!(status_tint("Weird", "new", p, &none()), tint_from_hue(hex(GREY), p));
            let o = BTreeMap::from([("blocked".to_string(), "#123456".to_string())]);
            assert_eq!(status_tint("Blocked", "new", p, &o), tint_from_hue(hex(0x123456), p));
            // An unparsable override is ignored.
            let bad = BTreeMap::from([("Blocked".to_string(), "red".to_string())]);
            assert_eq!(status_tint("Blocked", "new", p, &bad), tint_from_hue(hex(RED), p));
        }
    }

    #[test]
    fn on_accent_reads_on_the_accent() {
        assert!(contrast(LIGHT.on_accent, LIGHT.accent) >= 4.5, "{}", contrast(LIGHT.on_accent, LIGHT.accent));
        assert!(contrast(DARK.on_accent, DARK.accent) >= 7.0, "{}", contrast(DARK.on_accent, DARK.accent));
    }

    #[test]
    fn contrast_of_black_on_white_is_21() {
        assert!((contrast(Color32::BLACK, Color32::WHITE) - 21.0).abs() < 0.01);
        assert!((contrast(Color32::WHITE, Color32::WHITE) - 1.0).abs() < 1e-6);
        assert_eq!(over(Color32::from_rgba_unmultiplied(0, 0, 0, 0), Color32::WHITE), Color32::WHITE);
    }

    #[test]
    fn toast_text_reads_on_the_tooltip_fill() {
        for p in [&LIGHT, &DARK] {
            assert!(contrast(Color32::WHITE, p.tooltip) >= 7.0);
        }
    }

    #[test]
    fn every_tint_reads_on_its_wash_on_every_row_fill() {
        let statuses = ["New", "Planning", "Ready for Work", "In Progress", "In Review", "Blocked", "Done"];
        for p in [&LIGHT, &DARK] {
            let tints = EPIC_HUES.iter().map(|&h| tint_from_hue(hex(h), p)).chain(statuses.iter().map(|s| status_tint(s, "", p, &none())));
            for t in tints {
                for row in [p.surface, p.selected_row, p.hover_row, p.bg] {
                    let c = contrast(t.fg, over(t.bg, row));
                    assert!(c >= 4.5, "dark={} fg={:?} on {:?}: {c:.2}", p.dark, t.fg, row);
                }
            }
        }
    }

    #[test]
    fn muted_text_reads_on_every_row_fill() {
        for p in [&LIGHT, &DARK] {
            for row in [p.surface, p.selected_row, p.hover_row, p.bg] {
                assert!(contrast(p.muted, row) >= 4.5, "dark={} {:?}: {:.2}", p.dark, row, contrast(p.muted, row));
            }
        }
    }

    #[test]
    fn epic_hues_stay_out_of_the_status_and_danger_bands() {
        for banned in [RED, TEAL, ACCENT, GREEN, BLUE, 0xc8403a, 0x0f9d9a] {
            assert!(!EPIC_HUES.contains(&banned), "{banned:06x}");
        }
    }

    /// HSV hue angle in degrees (0..360) of an opaque colour.
    fn hue_angle(v: u32) -> f32 {
        let c = hex(v);
        let (r, g, b) = (f32::from(c.r()) / 255.0, f32::from(c.g()) / 255.0, f32::from(c.b()) / 255.0);
        let (max, min) = (r.max(g).max(b), r.min(g).min(b));
        let d = max - min;
        if d == 0.0 {
            return 0.0;
        }
        let h = if max == r {
            60.0 * ((g - b) / d).rem_euclid(6.0)
        } else if max == g {
            60.0 * ((b - r) / d + 2.0)
        } else {
            60.0 * ((r - g) / d + 4.0)
        };
        h.rem_euclid(360.0)
    }

    /// Shortest distance around the wheel between two angles.
    fn hue_dist(a: f32, b: f32) -> f32 {
        let d = (a - b).abs() % 360.0;
        d.min(360.0 - d)
    }

    /// Distance from `h` to the band `lo..=hi` (0 inside it).
    fn band_dist(h: f32, lo: f32, hi: f32) -> f32 {
        if (lo..=hi).contains(&h) { 0.0 } else { hue_dist(h, lo).min(hue_dist(h, hi)) }
    }

    #[test]
    fn hue_angle_of_primaries() {
        assert_eq!(hue_angle(0xff0000), 0.0);
        assert!((hue_angle(0x00ff00) - 120.0).abs() < 1e-3);
        assert!((hue_angle(0x0000ff) - 240.0).abs() < 1e-3);
        assert!((hue_dist(350.0, 10.0) - 20.0).abs() < 1e-3);
        assert_eq!(band_dist(10.0, 0.0, 15.0), 0.0);
    }

    #[test]
    fn epic_hues_are_spaced_and_clear_of_meaning_hues() {
        let hues: Vec<f32> = EPIC_HUES.iter().map(|&h| hue_angle(h)).collect();
        for (i, a) in hues.iter().enumerate() {
            for b in &hues[i + 1..] {
                assert!(hue_dist(*a, *b) >= 25.0, "{a:.0}° vs {b:.0}°");
            }
            for (name, lo, hi) in [("red", 0.0, 15.0), ("amber", 35.0, 45.0), ("green", 140.0, 150.0), ("accent", 218.0, 218.0)] {
                assert!(band_dist(*a, lo, hi) >= 25.0, "{a:.0}° too near {name}");
            }
        }
        assert!((hue_angle(ACCENT) - 218.0).abs() < 2.0, "the accent band tracks the accent");
        let violets = hues.iter().filter(|h| (255.0..=300.0).contains(*h)).count();
        assert!(violets <= 2, "{violets} violets");
    }

    #[test]
    fn light_in_progress_wash_is_not_the_selection_fill() {
        let t = status_tint("In Progress", "", &LIGHT, &none());
        let c = contrast(over(t.bg, LIGHT.surface), LIGHT.selected_row);
        assert!(c >= 1.2, "{c:.2}");
    }

    #[test]
    fn dark_hover_is_a_step_above_surface_2() {
        assert!(contrast(DARK.hover_row, DARK.surface_2) > 1.05);
        assert_ne!(DARK.selected_row, DARK.surface_2);
    }

    #[test]
    fn light_health_dots_read_on_surface() {
        for c in [LIGHT.at_risk, LIGHT.no_health, LIGHT.on_track, LIGHT.off_track] {
            assert!(contrast(c, LIGHT.surface) >= 3.0, "{c:?}: {:.2}", contrast(c, LIGHT.surface));
        }
    }

    #[test]
    fn light_stripe_keeps_the_hue() {
        let base = hex(EPIC_HUES[0]);
        assert_eq!(tint_from_hue(base, &LIGHT).stripe, mix(base, 0, 0.1));
        assert_eq!(tint_from_hue(base, &DARK).stripe, tint_from_hue(base, &DARK).fg);
    }

    #[test]
    fn epic_tint_honours_override() {
        let o = BTreeMap::from([("CI-1860".to_string(), "#8250df".to_string())]);
        assert_eq!(epic_tint("CI-1860", &LIGHT, &o), tint_from_hue(hex(0x8250df), &LIGHT));
        assert_eq!(epic_tint("CI-1569", &LIGHT, &o), tint_from_hue(hex(EPIC_HUES[stable_index("CI-1569")]), &LIGHT));
    }
}
