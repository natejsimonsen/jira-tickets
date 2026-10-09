//! Colours from the roadmap mockup tokens, light and dark, and system fonts.

use egui::{Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Shadow, Stroke, Visuals};
use std::collections::BTreeMap;
use std::sync::Arc;

pub const BOLD: &str = "bold";

/// The one corner radius for buttons, combos, chips, cards, dialogs, popups and the toast (pills are 10,
/// picker rows 4).
pub const RADIUS: u8 = 6;

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
    /// Text on a `tooltip` fill (the toast): the page `bg` in light, `text` in dark, never pure white.
    pub tooltip_text: Color32,
}

const fn hex(v: u32) -> Color32 {
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

pub const LIGHT: Palette = Palette {
    dark: false,
    bg: hex(0xf7f7f5),
    surface: hex(0xffffff),
    surface_2: hex(0xf0f0ed),
    border: hex(0xd2d1cb),
    text: hex(0x1f1f1d),
    muted: hex(0x62615d),
    accent: hex(0x2a64c8),
    on_accent: hex(0xffffff),
    on_track: hex(0x267a4b),
    at_risk: hex(0x8f6208),
    off_track: hex(0xb5352f),
    no_health: hex(0x8a8984),
    done: hex(0x6b6a66),
    today: hex(0xc8403a),
    selected_row: hex(0xdfdfda),
    hover_row: hex(0xebebe8),
    danger_subtle: hex(0xfbe9e8),
    tooltip: hex(0x1f1f1d),
    tooltip_text: hex(0xf7f7f5),
};

pub const DARK: Palette = Palette {
    dark: true,
    bg: hex(0x171716),
    surface: hex(0x20201f),
    surface_2: hex(0x2a2a28),
    border: hex(0x3a3a37),
    text: hex(0xecebe8),
    muted: hex(0xabaaa5),
    accent: hex(0x6b98e6),
    on_accent: hex(0x171716),
    on_track: hex(0x5cc28a),
    at_risk: hex(0xe6b04a),
    off_track: hex(0xf48a83),
    no_health: hex(0x7d7b76),
    done: hex(0xa3a29d),
    today: hex(0xec6d66),
    selected_row: hex(0x3d3d3a),
    hover_row: hex(0x343432),
    danger_subtle: hex(0x3a2422),
    tooltip: hex(0x57564f),
    tooltip_text: hex(0xecebe8),
};

// Spinners: one size (`icons::INLINE_SPINNER`, 13). The header spinner is `text` because it replaces the
// `text` sync glyph; every inline spinner (count row, detail, composer, pickers, sign-in) is `muted`.

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
/// Waiting statuses ("Waiting for External Party"): amber, apart from the In Progress blue.
const AMBER: u32 = 0xb07a0c;

/// Ten epic hues, evenly spaced 36° apart from 18°, at one luminance so all ten read alike. Epic pills are
/// outline-only (status is the one filled pill), so the palette needs no room around the status hues.
/// Hues go to the epics on screen first, farthest free hue first (`assign_epic_slots`).
pub const EPIC_HUES: [u32; 10] = [0xb6572e, 0x7d741f, 0x507f20, 0x21842b, 0x208264, 0x287b9e, 0x4053ff, 0x9a3ef6, 0xc231b4, 0xd23464];

/// `#rrggbb` or `rrggbb`.
pub fn parse_hex(s: &str) -> Option<Color32> {
    let h = s.trim();
    let h = h.strip_prefix('#').unwrap_or(h);
    if h.len() != 6 || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(h, 16).ok().map(hex)
}

fn mix(c: Color32, toward: u8, t: f32) -> Color32 {
    let m = |v: u8| (f32::from(v) + (f32::from(toward) - f32::from(v)) * t).round() as u8;
    Color32::from_rgb(m(c.r()), m(c.g()), m(c.b()))
}

/// Light-theme wash alpha: heavy enough that an In Progress pill on a plain row never reads as the
/// (accent-tinted) selected-row fill.
const LIGHT_WASH: u8 = 76;
const DARK_WASH: u8 = 72;
/// Dark stripe (and epic outline): 42% of the way to white, so it holds ≥4:1 on the selected row.
const DARK_STRIPE_MIX: f32 = 0.42;
/// Dark pill text: a little past halfway to white, so it holds 4.5:1 on the lighter selected row too.
const DARK_TEXT_MIX: f32 = 0.62;

/// A translucent wash of the hue behind text in a darker (light theme) or
/// lighter (dark theme) shade of it, for at least 4.5:1 contrast. The row's edge stripe is the
/// base hue in light (20% toward black, so it holds 4.5:1 on the row) and DARK_STRIPE_MIX toward white in dark (the text's
/// halfway tint washes every hue out to the same pale stripe).
pub fn tint_from_hue(base: Color32, p: &Palette) -> Tint {
    let (alpha, fg, stripe) =
        if p.dark { (DARK_WASH, mix(base, 255, DARK_TEXT_MIX), mix(base, 255, DARK_STRIPE_MIX)) } else { (LIGHT_WASH, mix(base, 0, 0.45), mix(base, 0, 0.2)) };
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
            n if n.starts_with("waiting") => AMBER,
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

/// The order hue slots are handed out in: farthest free hue first, so the first epics on screen sit ≥72° apart.
pub const EPIC_SLOT_ORDER: [usize; 10] = [0, 5, 2, 7, 1, 6, 3, 8, 4, 9];

/// Gives epics their hue slots, kept in `slots` for the session. `visible` are the epics of the rows on screen
/// (in row order, then the selected issue's), `rest` the loaded epic list. Visible epics come first: one that
/// already has a slot no other visible epic holds keeps it; a new one, or one whose slot another visible epic
/// already holds, takes the first free slot in EPIC_SLOT_ORDER. So up to ten visible epics always get ten
/// distinct hues, and an epic keeps its hue across lists unless keeping it would make two visible epics alike.
/// The rest of the epic list then takes slots in order. Config overrides are applied on top (`epic_tint_in`).
pub fn assign_epic_slots<'a>(slots: &mut BTreeMap<String, usize>, visible: impl IntoIterator<Item = &'a str>, rest: impl IntoIterator<Item = &'a str>) {
    let mut seen: Vec<&str> = Vec::new();
    for k in visible {
        if !seen.contains(&k) {
            seen.push(k);
        }
    }
    let mut used = [false; EPIC_HUES.len()];
    let mut pending = Vec::new();
    for &k in &seen {
        match slots.get(k) {
            Some(&s) if !used[s % EPIC_HUES.len()] => used[s % EPIC_HUES.len()] = true,
            _ => pending.push(k),
        }
    }
    for k in pending {
        match EPIC_SLOT_ORDER.iter().find(|&&s| !used[s]) {
            Some(&s) => {
                used[s] = true;
                slots.insert(k.to_string(), s);
            }
            // More than ten epics on screen: a known epic keeps its hue, a new one takes the next in order.
            None => {
                let next = EPIC_SLOT_ORDER[slots.len() % EPIC_HUES.len()];
                slots.entry(k.to_string()).or_insert(next);
            }
        }
    }
    for k in rest {
        if !slots.contains_key(k) {
            let next = EPIC_SLOT_ORDER[slots.len() % EPIC_HUES.len()];
            slots.insert(k.to_string(), next);
        }
    }
}

/// An epic's tint: the config override for its exact key wins, else its slot's hue (see `assign_epic_slots`).
pub fn epic_tint_in(epic_key: &str, slots: &BTreeMap<String, usize>, p: &Palette, overrides: &BTreeMap<String, String>) -> Tint {
    let slot = slots.get(epic_key).copied().unwrap_or(0);
    let base = overrides.get(epic_key).and_then(|v| parse_hex(v)).unwrap_or_else(|| hex(EPIC_HUES[slot % EPIC_HUES.len()]));
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
    // One blue: the caret is the accent, not egui's default.
    v.text_cursor.stroke.color = p.accent;
    // Hint (placeholder) text uses this; egui's default fades `text`, which drops below 4.5:1 in light.
    v.weak_text_color = Some(placeholder(p));
    v.selection.bg_fill = p.selected_row;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    v.widgets.inactive.bg_fill = p.surface_2;
    v.widgets.inactive.weak_bg_fill = p.surface_2;
    v.widgets.hovered.bg_fill = control_hover(p);
    v.widgets.hovered.weak_bg_fill = control_hover(p);
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
    v.window_corner_radius = CornerRadius::same(RADIUS);
    v.menu_corner_radius = CornerRadius::same(RADIUS);
    for w in [&mut v.widgets.noninteractive, &mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
        w.corner_radius = CornerRadius::same(RADIUS);
        w.expansion = 0.0;
    }
    v.popup_shadow = Shadow::NONE;
    // Popup and tooltip frames take the one `border`, not egui's grey.
    v.window_stroke = Stroke::new(1.0, p.border);
    v
}

/// Placeholder text: `muted` in light; in dark a step below it (#93928d), so an empty field never reads as
/// filled in (muted was 1.95:1 from typed text), still ≥4.5:1 on the dark field fills.
pub fn placeholder(p: &Palette) -> Color32 {
    if p.dark { hex(0x93928d) } else { p.muted }
}

/// A hovered control (button, chip, combo) that rests on `surface_2`: in light the old #e6e6e2 step, since the
/// lighter list-row `hover_row` (#ebebe8) is only 1.05:1 off surface_2; in dark `hover_row`.
pub fn control_hover(p: &Palette) -> Color32 {
    if p.dark { p.hover_row } else { hex(0xe6e6e2) }
}

/// Muted text on a cursor or selected fill: in dark lifted to #bcbbb6, so it holds 4.5:1 on the dark dialog's
/// #4a4a47 cursor row (muted reaches 3.8 there); `muted` everywhere else.
pub fn muted_on(fill: Color32, p: &Palette) -> Color32 {
    if p.dark && contrast(p.muted, fill) < 4.5 { hex(0xbcbbb6) } else { p.muted }
}

/// The light-theme neutral step between `surface_2` and `hover_row`, for code chips on white.
const LIGHT_CHIP: Color32 = hex(0xe8e8e4);
/// The dark modal dialog's fill: one step above `surface_2`, so a dialog lifts off the dimmed page.
pub const DARK_DIALOG: Color32 = hex(0x333331);
/// The lifted dark edge: modal dialogs, fields and buttons inside them.
pub const DARK_EDGE: Color32 = hex(0x504f4b);
/// The dark dialog's chip step (help keys): 1.32:1 off the dialog, quieter than DARK_EDGE's 1.54.
const DARK_DIALOG_CHIP: Color32 = hex(0x454542);
/// The cursor row in a dark dialog: 1.42:1 off #333331 (selected_row was 1.16).
pub const DARK_DIALOG_CURSOR: Color32 = hex(0x4a4a47);
/// A chip's fill must stand at least this far off its container (1.14:1 read as flat).
const CHIP_STEP: f32 = 1.17;

/// The fill for a chip (inline code, a code block, a help key) inside a `container`: the first step of the
/// theme's neutral ramp at least CHIP_STEP off it, darker in light and lighter in dark (a key reads raised).
/// On the page (surface) that is #e8e8e4 light, #333331 dark; in a comment card #dfdfda / #3a3a37.
pub fn chip_fill_on(container: Color32, p: &Palette) -> Color32 {
    let ramp: &[Color32] = if p.dark {
        &[p.surface_2, DARK_DIALOG, p.border, p.selected_row, DARK_DIALOG_CHIP, DARK_EDGE]
    } else {
        &[p.surface_2, LIGHT_CHIP, p.selected_row, p.border]
    };
    let lum = |c: Color32| contrast(c, Color32::BLACK);
    let ahead = |c: Color32| if p.dark { lum(c) > lum(container) } else { lum(c) < lum(container) };
    ramp.iter().copied().find(|&c| ahead(c) && contrast(c, container) >= CHIP_STEP).unwrap_or(p.border)
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
    use std::collections::BTreeSet;

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
        assert!(contrast(DARK.on_accent, DARK.accent) >= 6.0, "{}", contrast(DARK.on_accent, DARK.accent));
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
            assert!(contrast(p.tooltip_text, p.tooltip) >= 4.5, "dark={}: {:.2}", p.dark, contrast(p.tooltip_text, p.tooltip));
        }
        assert_eq!(LIGHT.tooltip_text, LIGHT.bg);
        assert_eq!(DARK.tooltip_text, DARK.text);
    }

    #[test]
    fn every_status_tint_reads_on_its_wash_on_every_row_fill() {
        let statuses = ["New", "Planning", "Ready for Work", "In Progress", "In Review", "Blocked", "Done", "Waiting for support"];
        for p in [&LIGHT, &DARK] {
            for t in statuses.iter().map(|s| status_tint(s, "", p, &none())) {
                for row in [p.surface, p.selected_row, p.hover_row, p.bg] {
                    let c = contrast(t.fg, over(t.bg, row));
                    assert!(c >= 4.5, "dark={} fg={:?} on {:?}: {c:.2}", p.dark, t.fg, row);
                }
            }
        }
    }

    #[test]
    fn epic_outline_pills_read_on_every_row_fill() {
        // No fill: the text sits on the row itself, and the outline and stripe are the stripe colour.
        for p in [&LIGHT, &DARK] {
            for &h in &EPIC_HUES {
                let t = tint_from_hue(hex(h), p);
                for row in [p.surface, p.selected_row, p.hover_row, p.bg] {
                    assert!(contrast(t.fg, row) >= 4.5, "dark={} {h:06x} text on {row:?}: {:.2}", p.dark, contrast(t.fg, row));
                    assert!(contrast(t.stripe, row) >= 3.0, "dark={} {h:06x} edge on {row:?}: {:.2}", p.dark, contrast(t.stripe, row));
                }
            }
        }
    }

    #[test]
    fn muted_text_reads_on_every_row_fill() {
        // Including the selected row and the picker cursor, so secondary text never swaps to `text` there.
        for p in [&LIGHT, &DARK] {
            for row in [p.surface, p.hover_row, p.bg, p.selected_row, p.surface_2] {
                assert!(contrast(p.muted, row) >= 4.5, "dark={} {:?}: {:.2}", p.dark, row, contrast(p.muted, row));
            }
        }
        assert!(contrast(DARK.muted, DARK_DIALOG) >= 4.5);
    }

    #[test]
    fn chips_step_off_their_container() {
        for p in [&LIGHT, &DARK] {
            for container in [p.surface, p.surface_2] {
                let c = chip_fill_on(container, p);
                assert!(contrast(c, container) >= CHIP_STEP, "dark={} {container:?}: {:.2}", p.dark, contrast(c, container));
            }
        }
        assert_eq!(chip_fill_on(LIGHT.surface, &LIGHT), hex(0xe8e8e4), "code on the light page");
        assert_eq!(chip_fill_on(DARK.surface, &DARK), DARK_DIALOG, "code on the dark page");
        assert_eq!(chip_fill_on(LIGHT.surface_2, &LIGHT), LIGHT.selected_row, "code in a light comment card");
        assert_eq!(chip_fill_on(DARK.surface_2, &DARK), DARK.border, "code in a dark comment card");
        assert_eq!(chip_fill_on(DARK_DIALOG, &DARK), DARK_DIALOG_CHIP, "help keys in the dark dialog");
    }

    #[test]
    fn links_read_on_every_surface() {
        for p in [&LIGHT, &DARK] {
            for fill in [p.bg, p.surface, p.surface_2] {
                assert!(contrast(p.accent, fill) >= 4.5, "dark={} {fill:?}: {:.2}", p.dark, contrast(p.accent, fill));
            }
        }
    }

    #[test]
    fn one_corner_radius() {
        for p in [&LIGHT, &DARK] {
            let v = visuals(p);
            for w in [v.widgets.noninteractive, v.widgets.inactive, v.widgets.hovered, v.widgets.active, v.widgets.open] {
                assert_eq!(w.corner_radius, CornerRadius::same(RADIUS));
            }
            assert_eq!((v.menu_corner_radius, v.window_corner_radius), (CornerRadius::same(RADIUS), CornerRadius::same(RADIUS)));
        }
    }

    #[test]
    fn caret_is_the_accent() {
        for p in [&LIGHT, &DARK] {
            assert_eq!(visuals(p).text_cursor.stroke.color, p.accent);
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

    #[test]
    fn hue_angle_of_primaries() {
        assert_eq!(hue_angle(0xff0000), 0.0);
        assert!((hue_angle(0x00ff00) - 120.0).abs() < 1e-3);
        assert!((hue_angle(0x0000ff) - 240.0).abs() < 1e-3);
        assert!((hue_dist(350.0, 10.0) - 20.0).abs() < 1e-3);
    }

    #[test]
    fn ten_epic_hues_evenly_spaced() {
        assert_eq!(EPIC_HUES.len(), 10);
        let hues: Vec<f32> = EPIC_HUES.iter().map(|&h| hue_angle(h)).collect();
        for (i, a) in hues.iter().enumerate() {
            for b in &hues[i + 1..] {
                assert!(hue_dist(*a, *b) >= 35.0, "{a:.0}° vs {b:.0}°");
            }
        }
    }

    #[test]
    fn in_progress_pill_shows_on_a_selected_row() {
        // The selection is neutral, so a blue pill on it keeps its hue; and its wash still steps off the fill.
        for p in [&LIGHT, &DARK] {
            let t = status_tint("In Progress", "", p, &none());
            let c = contrast(over(t.bg, p.selected_row), p.selected_row);
            assert!(c >= 1.1, "dark={}: {c:.2}", p.dark);
        }
    }

    #[test]
    fn placeholders_read_apart_from_typed_text() {
        assert_eq!(placeholder(&DARK), hex(0x93928d));
        assert_eq!(placeholder(&LIGHT), LIGHT.muted);
        for p in [&LIGHT, &DARK] {
            // On the field fills (page bg in dialogs, surface_2 / surface elsewhere).
            for fill in [p.bg, p.surface, p.surface_2] {
                assert!(contrast(placeholder(p), fill) >= 4.5, "dark={}: {:.2}", p.dark, contrast(placeholder(p), fill));
            }
        }
        assert!(contrast(placeholder(&DARK), DARK.text) > contrast(DARK.muted, DARK.text));
    }

    #[test]
    fn muted_stays_readable_on_cursor_rows() {
        for p in [&LIGHT, &DARK] {
            for fill in [p.selected_row, p.hover_row, DARK_DIALOG_CURSOR] {
                if !p.dark && fill == DARK_DIALOG_CURSOR {
                    continue;
                }
                assert!(contrast(muted_on(fill, p), fill) >= 4.5, "dark={} {fill:?}: {:.2}", p.dark, contrast(muted_on(fill, p), fill));
            }
        }
        assert_eq!(muted_on(DARK_DIALOG_CURSOR, &DARK), hex(0xbcbbb6));
        assert_eq!(muted_on(DARK.surface, &DARK), DARK.muted, "narrow: only where muted falls short");
        assert!(contrast(DARK_DIALOG_CURSOR, DARK_DIALOG) >= 1.4);
    }

    #[test]
    fn hover_is_a_step_off_surface_2_in_both_themes() {
        for p in [&LIGHT, &DARK] {
            assert!(contrast(control_hover(p), p.surface_2) > 1.05, "dark={}", p.dark);
            assert!(contrast(p.hover_row, p.surface) > 1.1, "dark={}: a list row's hover", p.dark);
            assert_ne!(p.selected_row, p.surface_2);
        }
    }

    #[test]
    fn selection_is_neutral_and_a_step_past_hover() {
        for p in [&LIGHT, &DARK] {
            let c = p.selected_row;
            let spread = c.r().max(c.g()).max(c.b()) - c.r().min(c.g()).min(c.b());
            assert!(spread <= 6, "dark={} {c:?} is not neutral", p.dark);
            assert!(contrast(c, p.hover_row) > 1.05, "dark={}", p.dark);
            assert!(contrast(c, p.surface) > contrast(p.hover_row, p.surface), "dark={}: selection outranks hover", p.dark);
        }
    }

    #[test]
    fn waiting_statuses_are_amber_not_in_progress_blue() {
        for p in [&LIGHT, &DARK] {
            let w = status_tint("Waiting for External Party", "indeterminate", p, &none());
            assert_eq!(w, tint_from_hue(hex(AMBER), p));
            assert_ne!(w, status_tint("Work in Progress", "indeterminate", p, &none()));
        }
    }

    #[test]
    fn visible_epics_get_distinct_hues() {
        let keys: Vec<String> = (100..134).map(|n| format!("CI-{n}")).collect();
        let all: Vec<&str> = keys.iter().map(String::as_str).collect();
        let mut slots = BTreeMap::new();
        // The full 34-epic list arrives first (nothing on screen yet): slots wrap mod 10.
        assign_epic_slots(&mut slots, [], all.iter().copied());
        // Then rows show nine epics whose list slots collide (CI-100, CI-110 and CI-120 share slot 0, ...).
        let visible = ["CI-100", "CI-110", "CI-120", "CI-101", "CI-111", "CI-102", "CI-112", "CI-103", "CI-113"];
        assign_epic_slots(&mut slots, visible, all.iter().copied());
        let hues: BTreeSet<usize> = visible.iter().map(|k| slots[*k]).collect();
        assert_eq!(hues.len(), visible.len(), "≤10 distinct visible epics, distinct hues: {slots:?}");
        // Ten on screen: all ten hues.
        let ten: Vec<&str> = all[20..30].to_vec();
        assign_epic_slots(&mut slots, ten.iter().copied(), all.iter().copied());
        assert_eq!(ten.iter().map(|k| slots[*k]).collect::<BTreeSet<_>>().len(), 10);
        // Stable: running again (every frame) moves nothing.
        let before = slots.clone();
        assign_epic_slots(&mut slots, ten.iter().copied(), all.iter().copied());
        assert_eq!(slots, before);
    }

    #[test]
    fn epic_hues_go_farthest_first_and_stay_across_lists() {
        let mut slots = BTreeMap::new();
        assign_epic_slots(&mut slots, ["CI-9", "CI-3", "CI-7", "CI-9"], ["CI-1", "CI-2"]);
        assert_eq!((slots["CI-9"], slots["CI-3"], slots["CI-7"]), (0, 5, 2), "farthest free hue first, row order");
        assert_eq!((slots["CI-1"], slots["CI-2"]), (EPIC_SLOT_ORDER[3], EPIC_SLOT_ORDER[4]), "then the rest of the list");
        // The first four slots are ≥72° apart (36° a slot).
        for (i, a) in EPIC_SLOT_ORDER[..4].iter().enumerate() {
            for b in &EPIC_SLOT_ORDER[i + 1..4] {
                let d = (*a as i32 - *b as i32).rem_euclid(10).min((*b as i32 - *a as i32).rem_euclid(10));
                assert!(d * 36 >= 72, "{a} {b}");
            }
        }
        // Another list: a known key keeps its hue; a new one takes a slot free on screen.
        assign_epic_slots(&mut slots, ["CI-7", "CI-4", "CI-3"], []);
        assert_eq!((slots["CI-7"], slots["CI-3"]), (2, 5), "same key, same hue in every list");
        assert!(![2, 5].contains(&slots["CI-4"]));
        assert_eq!(EPIC_SLOT_ORDER.iter().collect::<BTreeSet<_>>().len(), 10);
    }

    #[test]
    fn light_health_dots_read_on_surface() {
        for c in [LIGHT.at_risk, LIGHT.no_health, LIGHT.on_track, LIGHT.off_track] {
            assert!(contrast(c, LIGHT.surface) >= 3.0, "{c:?}: {:.2}", contrast(c, LIGHT.surface));
        }
    }

    #[test]
    fn light_health_colours_read_as_text() {
        // ADF warning and success panels set text in them.
        for c in [LIGHT.at_risk, LIGHT.on_track, LIGHT.off_track] {
            assert!(contrast(c, LIGHT.surface) >= 4.5, "{c:?}: {:.2}", contrast(c, LIGHT.surface));
        }
    }

    #[test]
    fn light_stripe_keeps_the_hue() {
        let base = hex(EPIC_HUES[0]);
        assert_eq!(tint_from_hue(base, &LIGHT).stripe, mix(base, 0, 0.2));
        for &h in &EPIC_HUES {
            let s = tint_from_hue(hex(h), &LIGHT).stripe;
            assert!(contrast(s, LIGHT.surface) >= 4.0, "{h:06x}: {:.2}", contrast(s, LIGHT.surface));
        }
        assert_eq!(tint_from_hue(base, &DARK).stripe, mix(base, 255, DARK_STRIPE_MIX), "dark stripe 42% to white");
        for &h in &EPIC_HUES {
            let s = tint_from_hue(hex(h), &DARK).stripe;
            assert!(contrast(s, DARK.selected_row) > 3.6, "{h:06x}: {:.2}", contrast(s, DARK.selected_row));
        }
        assert_eq!(tint_from_hue(base, &DARK).fg, mix(base, 255, DARK_TEXT_MIX), "dark pill text a little past halfway");
    }

    #[test]
    fn epic_tint_honours_override() {
        let o = BTreeMap::from([("CI-1860".to_string(), "#8250df".to_string())]);
        let slots = BTreeMap::from([("CI-1860".to_string(), 3), ("CI-1569".to_string(), 4)]);
        assert_eq!(epic_tint_in("CI-1860", &slots, &LIGHT, &o), tint_from_hue(hex(0x8250df), &LIGHT));
        assert_eq!(epic_tint_in("CI-1569", &slots, &LIGHT, &o), tint_from_hue(hex(EPIC_HUES[4]), &LIGHT));
    }
}
