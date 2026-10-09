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
/// The one key style (hint-bar keys, help chips): mono 12.5; "⌘", "⇧" and arrows come from the system face
/// through the mono family's fallback.
pub fn key_font() -> FontId {
    mono(12.5)
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
    muted: hex(0x585753),
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
    tooltip: hex(0x3a3a37),
    tooltip_text: hex(0xf7f7f5),
};

pub const DARK: Palette = Palette {
    dark: true,
    bg: hex(0x171716),
    surface: hex(0x20201f),
    surface_2: hex(0x2a2a28),
    border: hex(0x3a3a37),
    text: hex(0xecebe8),
    muted: hex(0xa3a29d),
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
    tooltip: hex(0x4a4a47),
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

/// Ten epic colours per theme as (pill text, stripe and outline), perceptually spaced: OKLCH hues 1° + 36°·k at
/// chroma 0.12 (clipped to sRGB), lightness alternating between hue-adjacent slots (light 0.48 / 0.43, dark 0.81 /
/// 0.76), the stripe 0.06 toward the row (lighter in light, darker in dark). Every pair is ≥ ΔE2000 13 in each
/// theme (the old 36° HSL wheel crowded the greens to 6.3). Epic pills are outline only (status is the one
/// filled pill). Slots go to the epics on screen first, farthest free slot first (`assign_epic_slots`).
pub const EPIC_LIGHT: [(u32, u32); 10] = [
    (0x923c5a, 0xa54d6a),
    (0x843219, 0x98432b),
    (0x805300, 0x966200),
    (0x545300, 0x666500),
    (0x286f2f, 0x3b8040),
    (0x005d52, 0x007063),
    (0x00697d, 0x007c93),
    (0x12508f, 0x2662a2),
    (0x5d509c, 0x6d61af),
    (0x6f3473, 0x824585),
];
pub const EPIC_DARK: [(u32, u32); 10] = [
    (0xffa1bc, 0xec8da9),
    (0xf2957a, 0xde8268),
    (0xf0b464, 0xdca150),
    (0xb7b756, 0xa4a443),
    (0x8ed690, 0x7bc27e),
    (0x40cab5, 0x22b6a3),
    (0x52d4f3, 0x39c0df),
    (0x78b5fb, 0x66a2e7),
    (0xbeb6ff, 0xaca1f5),
    (0xd896db, 0xc483c7),
];
/// The number of epic slots.
pub const EPIC_SLOTS: usize = EPIC_LIGHT.len();

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
const DARK_WASH: u8 = 84;
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
    let mut used = [false; EPIC_SLOTS];
    let mut pending = Vec::new();
    for &k in &seen {
        match slots.get(k) {
            Some(&s) if !used[s % EPIC_SLOTS] => used[s % EPIC_SLOTS] = true,
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
                let next = EPIC_SLOT_ORDER[slots.len() % EPIC_SLOTS];
                slots.entry(k.to_string()).or_insert(next);
            }
        }
    }
    for k in rest {
        if !slots.contains_key(k) {
            let next = EPIC_SLOT_ORDER[slots.len() % EPIC_SLOTS];
            slots.insert(k.to_string(), next);
        }
    }
}

/// An epic's tint: the config override for its exact key wins, else its slot's hue (see `assign_epic_slots`).
pub fn epic_tint_in(epic_key: &str, slots: &BTreeMap<String, usize>, p: &Palette, overrides: &BTreeMap<String, String>) -> Tint {
    let slot = slots.get(epic_key).copied().unwrap_or(0);
    match overrides.get(epic_key).and_then(|v| parse_hex(v)) {
        Some(base) => tint_from_hue(base, p),
        None => slot_tint(slot, p),
    }
}

/// A slot's tint: the theme's colours for it (`EPIC_LIGHT`, `EPIC_DARK`). The wash (unused by outline epic
/// pills) is the stripe at the status wash alpha.
pub fn slot_tint(slot: usize, p: &Palette) -> Tint {
    let (fg, stripe) = if p.dark { EPIC_DARK[slot % EPIC_SLOTS] } else { EPIC_LIGHT[slot % EPIC_SLOTS] };
    let (fg, stripe) = (hex(fg), hex(stripe));
    let alpha = if p.dark { DARK_WASH } else { LIGHT_WASH };
    Tint { bg: Color32::from_rgba_unmultiplied(stripe.r(), stripe.g(), stripe.b(), alpha), fg, stripe }
}

fn visuals(p: &Palette) -> Visuals {
    let mut v = if p.dark { Visuals::dark() } else { Visuals::light() };
    v.panel_fill = p.bg;
    v.window_fill = p.surface;
    v.extreme_bg_color = p.surface;
    // Inputs on the page (the list pane's search box): one step under `bg` (`input_fill`).
    v.text_edit_bg_color = Some(input_fill(p.bg, p));
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

/// Placeholder text: a step below `muted` in both themes (light #63625e under muted #585753, 1.19:1 apart; dark
/// #979691), so an empty field never reads as filled in; light 4.79:1 on the #e4e4df input, dark ≥ 4.5:1 on the
/// dialog field.
pub fn placeholder(p: &Palette) -> Color32 {
    if p.dark { hex(0x979691) } else { hex(0x63625e) }
}

/// A hovered control (button, chip, combo) that rests on `surface_2`: in light the old #e6e6e2 step, since the
/// lighter list-row `hover_row` (#ebebe8) is only 1.05:1 off surface_2; in dark `hover_row`.
pub fn control_hover(p: &Palette) -> Color32 {
    if p.dark { p.hover_row } else { hex(0xe6e6e2) }
}

/// Muted text on a fill other than the page's (a selected or cursor row, a dialog): `muted` where it keeps
/// MUTED_MIN there, else the first of the theme's lifts that does (light #4d4c48 darker; dark #c0bfba, then
/// #cccbc6 for the dialog cursor row #4e4e4b), else the last lift.
pub fn muted_on(fill: Color32, p: &Palette) -> Color32 {
    let lifts: &[Color32] = if p.dark { &MUTED_LIFTS_DARK } else { &MUTED_LIFTS_LIGHT };
    std::iter::once(p.muted).chain(lifts.iter().copied()).find(|&c| contrast(c, fill) >= MUTED_MIN).unwrap_or(*lifts.last().unwrap_or(&p.muted))
}

const MUTED_LIFTS_LIGHT: [Color32; 1] = [hex(0x4d4c48)];
const MUTED_LIFTS_DARK: [Color32; 2] = [hex(0xc0bfba), hex(0xcccbc6)];

/// The contrast `muted` must keep on a fill before `muted_on` lifts it (round 8: 4.5 → 5.0 in both themes).
pub const MUTED_MIN: f32 = 5.0;

/// Dark surfaces' field (the sign-in panel, the composer): 1.15:1 under `surface` (it used to be the page `bg`,
/// 1.10, so a field there read as a hole to the page).
const DARK_SURFACE_FIELD: Color32 = hex(0x121211);
/// The sunken field on the dark page (the list pane's search box): the ramp's floor, 1.10:1 under `bg`
/// (#0e0e0d was 1.08). Nothing in the ramp clears INPUT_STEP under `bg` short of near-black, so the page field
/// takes the floor.
const DARK_SUNKEN: Color32 = hex(0x0a0a09);
/// The dark dialog's field: ≈1.16:1 under DARK_DIALOG, so dark inputs are one even step everywhere (the
/// dialog's field was `surface`, 1.39).
const DARK_DIALOG_FIELD: Color32 = hex(0x2e2e2c);

/// The one light page input fill, on the page and on white (sign-in, composer): 1.12:1 past the button fill
/// (`surface_2`), so a field never reads as a button (#eaeae6 was 1.06).
const LIGHT_INPUT: Color32 = hex(0xe4e4df);
/// The light field inside a modal dialog (go-to, pickers): one step toward white from LIGHT_INPUT, so it stands
/// 1.12:1 off the #dfdfda cursor row (#e4e4df was 1.05 and read as its sibling); placeholder 5.11 on it.
const LIGHT_DIALOG_INPUT: Color32 = hex(0xebebe7);
/// An input stands at least this far under its container (dark measured 1.08 page / 1.10 surface before round 10;
/// light already cleared it everywhere).
const INPUT_STEP: f32 = 1.12;

/// One fill rule for every text input on the page: the first neutral step at least INPUT_STEP darker than the
/// container it sits on (a sunken field), never the button fill (`surface_2`), else the ramp's darkest step, in
/// both themes. Light: white and bg → #e4e4df; dark: dialog → #2e2e2c, surface → #121211, bg → #0a0a09 (the
/// floor). Modal dialogs use `dialog_input_fill`.
pub fn input_fill(container: Color32, p: &Palette) -> Color32 {
    let ramp: &[Color32] = if p.dark {
        &[DARK_DIALOG, DARK_DIALOG_FIELD, p.surface_2, p.surface, p.bg, DARK_SURFACE_FIELD, DARK_SUNKEN]
    } else {
        &[p.surface, p.bg, p.surface_2, LIGHT_INPUT]
    };
    let lum = |c: Color32| contrast(c, Color32::BLACK);
    ramp.iter()
        .copied()
        .filter(|&c| c != p.surface_2)
        .find(|&c| lum(c) < lum(container) && contrast(c, container) >= INPUT_STEP)
        .unwrap_or(*ramp.last().unwrap_or(&container))
}

/// A field inside a modal dialog: light #ebebe7 (dialogs only; page fields keep #e4e4df), dark the input rule
/// under the dialog fill (#2e2e2c).
pub fn dialog_input_fill(p: &Palette) -> Color32 {
    if p.dark { input_fill(DARK_DIALOG, p) } else { LIGHT_DIALOG_INPUT }
}

/// The light-theme chip step on white (code chips and blocks, help keys): 1.29:1 off the page, near dark's 1.39
/// (#e8e8e4 was 1.23). It replaces a darker light `surface_2`, which would have met the #e4e4df input fill.
const LIGHT_CHIP: Color32 = hex(0xe3e3de);
/// The dark modal dialog's fill: one step above `surface_2`, so a dialog lifts off the dimmed page.
pub const DARK_DIALOG: Color32 = hex(0x383836);
/// The lifted dark edge: modal dialogs, fields and buttons inside them.
pub const DARK_EDGE: Color32 = hex(0x545350);
/// The dark dialog's chip step (help keys): 1.32:1 off the dialog, quieter than DARK_EDGE's 1.54.
const DARK_DIALOG_CHIP: Color32 = hex(0x454542);
/// The cursor row in a dark dialog: 1.42:1 off #333331 (selected_row was 1.16).
pub const DARK_DIALOG_CURSOR: Color32 = hex(0x4e4e4b);
/// A chip's fill must stand at least this far off its container (1.14:1 read as flat).
const CHIP_STEP: f32 = 1.17;

/// The fill for a chip (inline code, a code block, a help key) inside a `container`: the first step of the
/// theme's neutral ramp at least CHIP_STEP off it, darker in light and lighter in dark (a key reads raised).
/// On the page (surface) that is #e3e3de light, #333331 dark; in a comment card #dfdfda / #3a3a37.
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
    // Mono falls back to the system face before egui's builtins, so "⌘" in a mono key (SF Mono has no ⌘)
    // matches the text around it instead of the thin builtin glyph.
    let mono_fallback: Vec<String> = prop.iter().take(1).cloned().chain(builtin_mono.iter().cloned()).collect();
    let mono = family("system-mono", MONO, &mono_fallback);
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
        // No fill: the text sits on the row itself, and the outline and stripe are the stripe colour. Slots and a
        // config override (through `tint_from_hue`) alike.
        let override_base = hex(0x8250df);
        for p in [&LIGHT, &DARK] {
            let tints = (0..EPIC_SLOTS).map(|s| slot_tint(s, p)).chain(std::iter::once(tint_from_hue(override_base, p)));
            for (i, t) in tints.enumerate() {
                for row in [p.surface, p.selected_row, p.hover_row, p.bg] {
                    assert!(contrast(t.fg, row) >= 4.5, "dark={} slot {i} text on {row:?}: {:.2}", p.dark, contrast(t.fg, row));
                    assert!(contrast(t.stripe, row) >= 3.0, "dark={} slot {i} edge on {row:?}: {:.2}", p.dark, contrast(t.stripe, row));
                }
            }
        }
    }

    #[test]
    fn muted_text_reads_on_every_row_fill() {
        // Including the selected row and the picker cursor, so secondary text never swaps to `text` there.
        for p in [&LIGHT, &DARK] {
            for row in [p.surface, p.hover_row, p.bg, p.surface_2] {
                assert!(contrast(p.muted, row) >= 4.5, "dark={} {:?}: {:.2}", p.dark, row, contrast(p.muted, row));
            }
            // The selected row: secondary text there goes through `muted_on` (dark muted reaches 4.26).
            assert!(contrast(muted_on(p.selected_row, p), p.selected_row) >= 4.5, "dark={}", p.dark);
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
        assert_eq!(chip_fill_on(LIGHT.surface, &LIGHT), hex(0xe3e3de), "code on the light page");
        assert_eq!(chip_fill_on(DARK.surface, &DARK), DARK_DIALOG, "code on the dark page");
        assert_eq!(chip_fill_on(LIGHT.surface_2, &LIGHT), LIGHT.selected_row, "code in a light comment card");
        assert_eq!(chip_fill_on(DARK.surface_2, &DARK), DARK_DIALOG, "code in a dark comment card");
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

    #[test]
    fn epic_colours_are_perceptually_apart() {
        // Round 8: the HSL wheel's closest visible pair was ΔE2000 6.3 light / 8.1 dark (two greens).
        for p in [&LIGHT, &DARK] {
            for part in [|t: Tint| t.fg, |t: Tint| t.stripe] {
                let cols: Vec<Color32> = (0..EPIC_SLOTS).map(|s| part(slot_tint(s, p))).collect();
                let mut min = f32::INFINITY;
                for (i, &a) in cols.iter().enumerate() {
                    for &b in &cols[i + 1..] {
                        min = min.min(delta_e2000(a, b));
                    }
                }
                assert!(min >= 12.0, "dark={}: closest pair ΔE2000 {min:.1}", p.dark);
            }
            // Hue-adjacent slots (the wheel wraps) differ in OKLCH lightness by 4–6.
            for s in 0..EPIC_SLOTS {
                let (a, b) = (slot_tint(s, p).fg, slot_tint((s + 1) % EPIC_SLOTS, p).fg);
                let d = (oklab_l(a) - oklab_l(b)).abs() * 100.0;
                assert!((4.0..=6.0).contains(&d), "dark={} slots {s}/{}: ΔL {d:.1}", p.dark, (s + 1) % EPIC_SLOTS);
            }
        }
        assert_eq!(EPIC_DARK.len(), EPIC_SLOTS);
    }

    /// CIE L*a*b* (D65) of an sRGB colour.
    fn lab(c: Color32) -> (f64, f64, f64) {
        let lin = |v: u8| {
            let v = f64::from(v) / 255.0;
            if v <= 0.040_45 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
        };
        let (r, g, b) = (lin(c.r()), lin(c.g()), lin(c.b()));
        let x = (0.4124 * r + 0.3576 * g + 0.1805 * b) / 0.950_47;
        let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
        let z = (0.0193 * r + 0.1192 * g + 0.9505 * b) / 1.088_83;
        let f = |t: f64| if t > 0.008_856 { t.cbrt() } else { 7.787 * t + 16.0 / 116.0 };
        (116.0 * f(y) - 16.0, 500.0 * (f(x) - f(y)), 200.0 * (f(y) - f(z)))
    }

    /// CIEDE2000 between two L*a*b* colours (Sharma, Wu and Dalal 2005).
    fn ciede2000((l1, a1, b1): (f64, f64, f64), (l2, a2, b2): (f64, f64, f64)) -> f64 {
        let rad = f64::to_radians;
        let cb = (a1.hypot(b1) + a2.hypot(b2)) / 2.0;
        let g = 0.5 * (1.0 - (cb.powi(7) / (cb.powi(7) + 25f64.powi(7))).sqrt());
        let (a1p, a2p) = ((1.0 + g) * a1, (1.0 + g) * a2);
        let (c1p, c2p) = (a1p.hypot(b1), a2p.hypot(b2));
        let hue = |b: f64, a: f64| if b == 0.0 && a == 0.0 { 0.0 } else { b.atan2(a).to_degrees().rem_euclid(360.0) };
        let (h1p, h2p) = (hue(b1, a1p), hue(b2, a2p));
        let dhp = if c1p * c2p == 0.0 {
            0.0
        } else {
            let d = h2p - h1p;
            if d > 180.0 {
                d - 360.0
            } else if d < -180.0 {
                d + 360.0
            } else {
                d
            }
        };
        let (dl, dc, dh) = (l2 - l1, c2p - c1p, 2.0 * (c1p * c2p).sqrt() * rad(dhp / 2.0).sin());
        let (lbp, cbp) = ((l1 + l2) / 2.0, (c1p + c2p) / 2.0);
        let hbp = if c1p * c2p == 0.0 {
            h1p + h2p
        } else if (h1p - h2p).abs() <= 180.0 {
            (h1p + h2p) / 2.0
        } else if h1p + h2p < 360.0 {
            (h1p + h2p + 360.0) / 2.0
        } else {
            (h1p + h2p - 360.0) / 2.0
        };
        let t = 1.0 - 0.17 * rad(hbp - 30.0).cos() + 0.24 * rad(2.0 * hbp).cos() + 0.32 * rad(3.0 * hbp + 6.0).cos() - 0.20 * rad(4.0 * hbp - 63.0).cos();
        let dth = 30.0 * (-((hbp - 275.0) / 25.0).powi(2)).exp();
        let rc = 2.0 * (cbp.powi(7) / (cbp.powi(7) + 25f64.powi(7))).sqrt();
        let sl = 1.0 + 0.015 * (lbp - 50.0).powi(2) / (20.0 + (lbp - 50.0).powi(2)).sqrt();
        let (sc, sh) = (1.0 + 0.045 * cbp, 1.0 + 0.015 * cbp * t);
        let rt = -rad(2.0 * dth).sin() * rc;
        ((dl / sl).powi(2) + (dc / sc).powi(2) + (dh / sh).powi(2) + rt * (dc / sc) * (dh / sh)).sqrt()
    }

    fn delta_e2000(a: Color32, b: Color32) -> f32 {
        ciede2000(lab(a), lab(b)) as f32
    }

    /// OKLab lightness (0..1) of an sRGB colour.
    fn oklab_l(c: Color32) -> f32 {
        let lin = |v: u8| {
            let v = f32::from(v) / 255.0;
            if v <= 0.040_45 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
        };
        let (r, g, b) = (lin(c.r()), lin(c.g()), lin(c.b()));
        let l = (0.412_221_47 * r + 0.536_332_55 * g + 0.051_445_99 * b).cbrt();
        let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
        let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
        0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s
    }

    #[test]
    fn ciede2000_matches_the_reference_pairs() {
        // Sharma et al. test data, pairs 1, 17 and 25.
        let close = |a, b, want: f64| assert!((ciede2000(a, b) - want).abs() < 1e-3, "{:.4} vs {want}", ciede2000(a, b));
        close((50.0, 2.6772, -79.7751), (50.0, 0.0, -82.7485), 2.0425);
        close((50.0, 2.5, 0.0), (73.0, 25.0, -18.0), 27.1492);
        close((60.2574, -34.0099, 36.2677), (60.4626, -34.1751, 39.4387), 1.2644);
        assert!((oklab_l(Color32::WHITE) - 1.0).abs() < 1e-3);
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
    fn inputs_sit_one_step_under_their_container() {
        assert_eq!(input_fill(LIGHT.bg, &LIGHT), LIGHT_INPUT, "search box on the list pane: #e4e4df");
        assert_eq!(input_fill(LIGHT.surface, &LIGHT), LIGHT_INPUT, "dialog, sign-in panel, composer: the search box's fill");
        assert_eq!(input_fill(DARK_DIALOG, &DARK), DARK_DIALOG_FIELD, "one step under the dialog");
        assert_eq!(input_fill(DARK.surface, &DARK), hex(0x121211), "dark surface field (sign-in, composer)");
        assert_eq!(input_fill(DARK.bg, &DARK), hex(0x0a0a09), "dark page field: the ramp's floor");
        for p in [&LIGHT, &DARK] {
            for c in [p.bg, p.surface, if p.dark { DARK_DIALOG } else { p.surface }] {
                let f = input_fill(c, p);
                assert!(contrast(f, Color32::BLACK) < contrast(c, Color32::BLACK), "darker than its container");
                assert_ne!(f, p.surface_2, "never the button fill (dark={})", p.dark);
                assert!(contrast(p.text, f) >= 7.0 && contrast(placeholder(p), f) >= 4.5, "dark={} {c:?}", p.dark);
            }
        }
        assert!(contrast(placeholder(&DARK), input_fill(DARK_DIALOG, &DARK)) >= 4.5);
    }

    #[test]
    fn dark_input_steps_match_light() {
        // Round 10 (L1): dark fields stood 1.08 page / 1.10 surface / 1.16 dialog under their containers, light
        // 1.19 / 1.28 / 1.28.
        let step = |c: Color32| contrast(input_fill(c, &DARK), c);
        assert!((1.10..1.12).contains(&step(DARK.bg)), "page {:.3}", step(DARK.bg));
        assert!(step(DARK.surface) >= INPUT_STEP && step(DARK.surface) < 1.16, "surface {:.3}", step(DARK.surface));
        assert_eq!(dialog_input_fill(&DARK), hex(0x2e2e2c), "dialog field unchanged");
        assert!(contrast(dialog_input_fill(&DARK), DARK_DIALOG) >= INPUT_STEP, "dialog {:.3}", contrast(dialog_input_fill(&DARK), DARK_DIALOG));
        assert_ne!(input_fill(DARK.surface, &DARK), DARK.bg, "a surface field is not the page");
        for fill in [input_fill(DARK.bg, &DARK), input_fill(DARK.surface, &DARK), dialog_input_fill(&DARK)] {
            assert!(contrast(placeholder(&DARK), fill) >= 4.5, "{fill:?}: {:.2}", contrast(placeholder(&DARK), fill));
            assert!(contrast(DARK.text, fill) >= 7.0);
        }
        // Light is unchanged by the 1.12 step.
        assert_eq!(input_fill(LIGHT.bg, &LIGHT), LIGHT_INPUT);
        assert_eq!(input_fill(LIGHT.surface, &LIGHT), LIGHT_INPUT);
    }

    #[test]
    fn light_dialog_fields_step_off_the_cursor_row() {
        // L4: the light dialog field moves toward white, apart from the #dfdfda cursor row (1.05 before).
        let f = dialog_input_fill(&LIGHT);
        assert_eq!(f, hex(0xebebe7));
        assert!(contrast(f, LIGHT.selected_row) >= 1.1, "{:.3}", contrast(f, LIGHT.selected_row));
        assert!(contrast(f, LIGHT.surface) >= INPUT_STEP, "under the white dialog");
        assert!(contrast(placeholder(&LIGHT), f) >= 5.0, "{:.2}", contrast(placeholder(&LIGHT), f));
        assert!(contrast(LIGHT.text, f) >= 7.0);
        assert_ne!(f, LIGHT.surface_2);
    }

    #[test]
    fn placeholders_read_apart_from_typed_text() {
        assert_eq!(placeholder(&DARK), hex(0x979691));
        assert_eq!(placeholder(&LIGHT), hex(0x63625e));
        assert!(contrast(placeholder(&LIGHT), LIGHT_INPUT) >= 4.75, "light page input");
        assert!(contrast(LIGHT_INPUT, LIGHT.surface_2) >= 1.1, "a light field never reads as a button");
        for p in [&LIGHT, &DARK] {
            assert_ne!(placeholder(p), p.muted, "placeholders are dimmer than muted (dark={})", p.dark);
            // On every field fill, and the surfaces around them.
            for fill in [p.bg, p.surface, p.surface_2, input_fill(p.bg, p), input_fill(p.surface, p), input_fill(DARK_DIALOG, p)] {
                assert!(contrast(placeholder(p), fill) >= 4.5, "dark={}: {:.2}", p.dark, contrast(placeholder(p), fill));
            }
        }
        for p in [&LIGHT, &DARK] {
            assert!(contrast(placeholder(p), p.text) > contrast(p.muted, p.text), "dark={}", p.dark);
        }
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
        assert_eq!(muted_on(DARK_DIALOG_CURSOR, &DARK), hex(0xcccbc6), "the picker's current marker on the cursor row");
        assert!(contrast(muted_on(DARK_DIALOG_CURSOR, &DARK), DARK_DIALOG_CURSOR) >= 5.0);
        assert_eq!(muted_on(DARK.surface, &DARK), DARK.muted, "narrow: only where muted falls short");
        // Round 9: light muted #585753 clears 5.0 on the selected row (5.41), so it needs no lift there.
        assert_eq!(muted_on(LIGHT.selected_row, &LIGHT), LIGHT.muted, "light muted 5.41 on the selected row");
        assert!(contrast(placeholder(&LIGHT), LIGHT.muted) >= 1.15, "light muted and placeholder are a visible step apart");
        assert_eq!(muted_on(LIGHT.surface, &LIGHT), LIGHT.muted);
        assert_eq!(muted_on(DARK_DIALOG, &DARK), hex(0xc0bfba), "dark dialog muted 4.60");
        for p in [&LIGHT, &DARK] {
            for fill in [p.surface, p.bg, p.selected_row, p.hover_row, if p.dark { DARK_DIALOG } else { p.surface }] {
                assert!(contrast(muted_on(fill, p), fill) >= MUTED_MIN, "dark={} {fill:?}: {:.2}", p.dark, contrast(muted_on(fill, p), fill));
            }
        }
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
    fn epic_slots_read_on_selected_rows() {
        let none = BTreeMap::new();
        for p in [&LIGHT, &DARK] {
            for s in 0..EPIC_SLOTS {
                let slots = BTreeMap::from([("A".to_string(), s)]);
                let a = epic_tint_in("A", &slots, p, &none);
                assert_eq!(a, slot_tint(s, p));
                for fill in [p.surface, p.selected_row] {
                    assert!(contrast(a.fg, fill) >= 4.5, "dark={} slot {s} text on {fill:?}: {:.2}", p.dark, contrast(a.fg, fill));
                    assert!(contrast(a.stripe, fill) >= 3.0, "dark={} slot {s} stripe on {fill:?}: {:.2}", p.dark, contrast(a.stripe, fill));
                }
            }
        }
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
    fn override_stripe_keeps_the_hue() {
        // A config override's base goes through `tint_from_hue`.
        let base = hex(0xb6572e);
        assert_eq!(tint_from_hue(base, &LIGHT).stripe, mix(base, 0, 0.2));
        assert!(contrast(tint_from_hue(base, &LIGHT).stripe, LIGHT.surface) >= 4.0);
        assert_eq!(tint_from_hue(base, &DARK).stripe, mix(base, 255, DARK_STRIPE_MIX), "dark stripe 42% to white");
        assert!(contrast(tint_from_hue(base, &DARK).stripe, DARK.selected_row) > 3.6);
        assert_eq!(tint_from_hue(base, &DARK).fg, mix(base, 255, DARK_TEXT_MIX), "dark pill text a little past halfway");
    }

    #[test]
    fn epic_tint_honours_override() {
        let o = BTreeMap::from([("CI-1860".to_string(), "#8250df".to_string())]);
        let slots = BTreeMap::from([("CI-1860".to_string(), 3), ("CI-1569".to_string(), 4)]);
        assert_eq!(epic_tint_in("CI-1860", &slots, &LIGHT, &o), tint_from_hue(hex(0x8250df), &LIGHT));
        assert_eq!(epic_tint_in("CI-1569", &slots, &LIGHT, &o), slot_tint(4, &LIGHT));
    }
}
