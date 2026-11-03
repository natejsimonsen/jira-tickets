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
    // ≥ 5.2:1 on surface_2 (round 13 L4: #267a4b and #8f6208 sat near 4.6).
    on_track: hex(0x22703f),
    at_risk: hex(0x835a07),
    off_track: hex(0xb5352f),
    no_health: hex(0x8a8984),
    done: hex(0x6b6a66),
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
    // Round 15 L3: a step quieter (#5cc28a / #e6b04a read 8–9:1 against light's 5.7); 6.1 / 7.1 on surface, 5.3 /
    // 6.3 on a card.
    on_track: hex(0x4cb07a),
    at_risk: hex(0xd9a23c),
    off_track: hex(0xf48a83),
    no_health: hex(0x7d7b76),
    done: hex(0xa3a29d),
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

/// Colour roles derived per theme. They are methods, not fields, so they come from one place (`dark`) and every
/// palette, test palettes included, gets them.
impl Palette {
    /// "Not set", "no update yet", "—", field placeholders: a value that is missing, not a fault. Never use the
    /// danger colour for it. Light #63625e (6.1:1 on white), dark #9c9b96 (6.2:1 on `surface`).
    pub fn placeholder(&self) -> Color32 {
        if self.dark { hex(0x9c9b96) } else { hex(0x63625e) }
    }

    /// The at-risk health dot: a mark, not text, so in light it is a brighter amber than `at_risk` (#835a07 reads
    /// brown at 8 px): #b26b00, 4.2:1 on white, 3.5 on `hover_row`, 3.1 on `selected_row`, every fill a roadmap card
    /// or table row takes. Dark keeps `at_risk`. Text in the at-risk colour stays `at_risk`.
    pub fn at_risk_mark(&self) -> Color32 {
        if self.dark { self.at_risk } else { hex(0xb26b00) }
    }

    /// A quiet accent tint for fills that are not the primary action: progress and timeline bars, an "on"
    /// toggle chip, a highlighted band. Only `text` goes on it (light 13.1:1, dark 9.6:1).
    pub fn accent_subtle(&self) -> Color32 {
        if self.dark { hex(0x2f3a50) } else { hex(0xdce6f7) }
    }

    /// A search match inside text (the Issues list's matched words): a firmer accent tint than `accent_subtle`, so
    /// the matched word reads at a glance on a plain row (light #bccff3 1.57:1 on white, 1.39 on `selection`; dark
    /// #34456a 1.88:1 on `bg`, 1.65 on `selection`). Only `text` goes on it (light 10.5:1, dark 8.0:1); on the
    /// selected row add a 1 px accent underline (`issues::title_job`).
    pub fn search_match(&self) -> Color32 {
        if self.dark { hex(0x34456a) } else { hex(0xbccff3) }
    }

    /// The one selected / focused fill for cards and list rows: a quiet accent tint, so it never reads as a code
    /// block, comment card, input or hover (all neutral). Pair it with `selection_edge`, which carries most of the
    /// "selected" signal. It is chosen to stay clear of every status pill fill, so a pill keeps its shape on a
    /// selected row with no edge of its own (`components::status_pill`): light #ebf1fd sits lighter and less blue than
    /// every pill (ΔE OKLab ≥ 8.5, 1.24:1; #e6edf9 was 5.4 from "New"); dark #1d2236 sits a deeper navy than the card,
    /// under every pill's lift (ΔE ≥ 9.7, 1.34:1; #283040 was 4.9 from Planning and 5.4 from In Progress).
    pub fn selection(&self) -> Color32 {
        if self.dark { hex(0x1d2236) } else { hex(0xebf1fd) }
    }

    /// The selected card's or row's 1 px ring: the accent (light 4.75:1, dark 4.57:1 on `selection`).
    pub fn selection_edge(&self) -> Color32 {
        self.accent
    }

    /// A neutral pill's 1 px outline: `border` in light (1.53:1 on white cards and dialogs), DARK_EDGE in dark,
    /// where `border` vanishes on the #383836 dialog (1.03:1); DARK_EDGE reads 1.53 there, 2.12 on cards.
    pub fn pill_edge(&self) -> Color32 {
        if self.dark { DARK_EDGE } else { self.border }
    }

    /// The toast's 1 px edge. Light: none, its inverted dark fill already stands 9:1 off the page. Dark: a lifted
    /// neutral (#6e6d69, 1.7:1 off the #4a4a47 toast, 3.1:1 off the panes), so the toast reads as an overlay with the
    /// same weight as in light, not as one more grey surface.
    pub fn toast_edge(&self) -> Option<Color32> {
        self.dark.then_some(hex(0x6e6d69))
    }

    /// The one form-field surface on the page: the filter combos and the search box share it (and the `border`
    /// edge), so a filter bar reads as one set of fields. It is the resting control fill.
    pub fn field(&self) -> Color32 {
        button_fill(self)
    }

    /// The page dimming behind a modal, as black alpha. Dark needs far more to push the page back.
    pub fn scrim_alpha(&self) -> u8 {
        if self.dark { 160 } else { 48 }
    }

    /// A modal dialog's shadow: its elevation off the dimmed page (heavier in dark, where a light one vanishes).
    pub fn dialog_shadow(&self) -> Shadow {
        if self.dark {
            Shadow { offset: [0, 8], blur: 28, spread: 0, color: Color32::from_black_alpha(180) }
        } else {
            Shadow { offset: [0, 6], blur: 20, spread: 0, color: Color32::from_black_alpha(40) }
        }
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

/// Eight epic colours per theme as (dot / pill text, stripe and outline), perceptually spaced: OKLCH hues 56°, 93°,
/// 122°, 188°, 220°, 250°, 290° and 333° at chroma 0.12 (clipped to sRGB), lightness alternating between
/// hue-adjacent slots (light 0.48 / 0.43, dark 0.81 / 0.76), the stripe 0.06 toward the row. The hues skip the
/// danger red (OKLCH ~27°) and the success green (~155°), so an epic never reads as Blocked or Done. Every pair
/// is ≥ ΔE2000 13.7 in each theme. Show an epic as neutral text with a dot in this hue (`components::epic_label`);
/// slots go to the epics on screen first, farthest free slot first (`assign_epic_slots`).
pub const EPIC_LIGHT: [(u32, u32); 8] = [
    (0x8d4903, 0xa2591a),
    (0x604e00, 0x745e01),
    (0x536701, 0x637919),
    (0x035d57, 0x056f69),
    (0x05687f, 0x027b96),
    (0x03528e, 0x1e63a1),
    (0x5e4f9c, 0x6f61af),
    (0x75326c, 0x87437d),
];
pub const EPIC_DARK: [(u32, u32); 8] = [
    (0xfcac72, 0xe8995f),
    (0xcbaf4f, 0xb89c3a),
    (0xb4cd73, 0xa1ba60),
    (0x37c9bf, 0x0fb6ac),
    (0x55d3f6, 0x3dbfe2),
    (0x73b6fa, 0x61a3e6),
    (0xc0b6ff, 0xada0f5),
    (0xdf94d2, 0xcb81be),
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
/// The grey hue's dark wash ("New" and other to-do statuses): 1.45:1 on a plain row, near light's 1.37 (84 read
/// 1.64; round 14 L3). Its text holds 4.9:1 over the wash on the selected row.
const DARK_GREY_WASH: u8 = 66;
/// The grey hue's light wash ("New" and other to-do statuses): a firmer grey than the coloured washes, so a to-do
/// pill reads as a filled status, not as an outlined label pill, and stands ≥ 1.3:1 off the selection.
const LIGHT_GREY_WASH: u8 = 92;
/// Dark stripe (and epic outline): 42% of the way to white, so it holds ≥4:1 on the selected row.
const DARK_STRIPE_MIX: f32 = 0.42;
/// Dark pill text: a little past halfway to white, so it holds 4.5:1 on the lighter selected row too.
const DARK_TEXT_MIX: f32 = 0.62;
/// The grey hue's dark pill text ("New" and other to-do statuses): less far toward white, #cccbc9, 1.36 off body
/// text instead of 1.27 (#d3d2d0 read as plain text; round 12 L1). The asked-for #c4c3c0 (1.5 off) falls to 4.2:1
/// on the grey wash over the selected row, so this was the furthest step that kept every row at 4.5:1 (4.55 at
/// DARK_WASH; 4.9 since round 14's lighter grey wash).
const DARK_GREY_TEXT_MIX: f32 = 0.56;

/// How far toward white a hue's dark pill text goes.
fn dark_text_mix(base: Color32) -> f32 {
    if base == hex(GREY) { DARK_GREY_TEXT_MIX } else { DARK_TEXT_MIX }
}

/// A translucent wash of the hue behind text in a darker (light theme) or
/// lighter (dark theme) shade of it, for at least 4.5:1 contrast. The row's edge stripe is the
/// base hue in light (20% toward black, so it holds 4.5:1 on the row) and DARK_STRIPE_MIX toward white in dark (the text's
/// halfway tint washes every hue out to the same pale stripe).
pub fn tint_from_hue(base: Color32, p: &Palette) -> Tint {
    let (alpha, fg, stripe) = if p.dark {
        (if base == hex(GREY) { DARK_GREY_WASH } else { DARK_WASH }, mix(base, 255, dark_text_mix(base)), mix(base, 255, DARK_STRIPE_MIX))
    } else {
        (if base == hex(GREY) { LIGHT_GREY_WASH } else { LIGHT_WASH }, mix(base, 0, 0.45), mix(base, 0, 0.2))
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
            "in progress" | "work in progress" => ACCENT,
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

/// The order hue slots are handed out in: farthest free hue first, so the first four epics on screen sit two or
/// more slots apart.
pub const EPIC_SLOT_ORDER: [usize; EPIC_SLOTS] = [0, 4, 2, 6, 1, 5, 3, 7];

/// Gives epics their hue slots, kept in `slots` for the session. `visible` are the epics of the rows on screen
/// (in row order, then the selected issue's), `rest` the loaded epic list. Visible epics come first: one that
/// already has a slot no other visible epic holds keeps it; a new one, or one whose slot another visible epic
/// already holds, takes the first free slot in EPIC_SLOT_ORDER. So up to EPIC_SLOTS visible epics always get
/// distinct hues, and an epic keeps its hue across lists unless keeping it would make two visible epics alike.
/// Past EPIC_SLOTS visible epics, each further one takes the least-used slot on screen (keeping its own when that
/// is one of them), so 16 epics show each hue twice, never one hue ten times.
/// The rest of the epic list then takes slots in order. Config overrides are applied on top (`epic_tint_in`).
pub fn assign_epic_slots<'a>(slots: &mut BTreeMap<String, usize>, visible: impl IntoIterator<Item = &'a str>, rest: impl IntoIterator<Item = &'a str>) {
    let mut seen: Vec<&str> = Vec::new();
    for k in visible {
        if !seen.contains(&k) {
            seen.push(k);
        }
    }
    let mut used = [0usize; EPIC_SLOTS];
    let mut pending = Vec::new();
    for &k in &seen {
        match slots.get(k) {
            Some(&s) if used[s % EPIC_SLOTS] == 0 => used[s % EPIC_SLOTS] = 1,
            _ => pending.push(k),
        }
    }
    for k in pending {
        let least = used.iter().copied().min().unwrap_or(0);
        let own = slots.get(k).map(|s| s % EPIC_SLOTS).filter(|&s| used[s] == least);
        let s = own.or_else(|| EPIC_SLOT_ORDER.iter().copied().find(|&s| used[s] == least)).unwrap_or(0);
        used[s] += 1;
        slots.insert(k.to_string(), s);
    }
    for k in rest {
        if !slots.contains_key(k) {
            let next = EPIC_SLOT_ORDER[slots.len() % EPIC_SLOTS];
            slots.insert(k.to_string(), next);
        }
    }
}

/// A slot for an epic no screen has given one yet (`assign_epic_slots`): spread by its key, so neighbouring epics
/// differ. A key ending in a number ("CI-1841") walks EPIC_SLOT_ORDER by that number, so any EPIC_SLOTS
/// consecutive numbers take every slot; any other key is hashed (FNV-1a). It used to be slot 0 for every
/// unassigned epic, which painted most epics one orange on screens that never assign slots.
pub fn epic_fallback_slot(epic_key: &str) -> usize {
    let digits: String = epic_key.chars().rev().take_while(char::is_ascii_digit).collect::<Vec<_>>().into_iter().rev().collect();
    let n = match digits.parse::<u64>() {
        Ok(n) => n,
        Err(_) => epic_key.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)),
    };
    EPIC_SLOT_ORDER[(n % EPIC_SLOTS as u64) as usize]
}

/// An epic's tint: the config override for its exact key wins, else its slot's hue (see `assign_epic_slots`), else
/// `epic_fallback_slot`.
pub fn epic_tint_in(epic_key: &str, slots: &BTreeMap<String, usize>, p: &Palette, overrides: &BTreeMap<String, String>) -> Tint {
    let slot = slots.get(epic_key).copied().unwrap_or_else(|| epic_fallback_slot(epic_key));
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

/// Every field's caret width, in points: two device pixels at 2x, in every field and state.
pub const CARET_W: f32 = 1.0;

fn visuals(p: &Palette) -> Visuals {
    let mut v = if p.dark { Visuals::dark() } else { Visuals::light() };
    v.panel_fill = p.bg;
    v.window_fill = p.surface;
    v.extreme_bg_color = p.surface;
    // Inputs on the page (the list pane's search box): the combos' field surface (`Palette::field`) with the same
    // `border` edge, so the filter bar is one set of fields. Dialogs, sign-in and the composer set their own.
    v.text_edit_bg_color = Some(p.field());
    v.faint_bg_color = p.surface_2;
    v.override_text_color = Some(p.text);
    v.hyperlink_color = p.accent;
    // One blue: the caret is the accent, not egui's default.
    v.text_cursor.stroke.color = p.accent;
    // One caret width, empty or typed (round 14 I1: egui's 2 pt read 4x the clipped trailing caret).
    v.text_cursor.stroke.width = CARET_W;
    // Hint (placeholder) text uses this; egui's default fades `text`, which drops below 4.5:1 in light.
    v.weak_text_color = Some(placeholder(p));
    v.selection.bg_fill = p.selected_row;
    v.disabled_alpha = DISABLED_ALPHA;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    v.widgets.inactive.bg_fill = button_fill(p);
    v.widgets.inactive.weak_bg_fill = button_fill(p);
    v.widgets.hovered.bg_fill = control_hover(p);
    v.widgets.hovered.weak_bg_fill = control_hover(p);
    v.widgets.active.bg_fill = p.selected_row;
    v.widgets.active.weak_bg_fill = p.selected_row;
    // Every control gets a 1 px edge in both themes; focus and press are the accent, never near-white.
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, p.border);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, p.border);
    v.widgets.open.bg_stroke = Stroke::new(1.0, p.border);
    v.widgets.open.bg_fill = button_fill(p);
    v.widgets.open.weak_bg_fill = button_fill(p);
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
/// #9c9b96, under the dialog's lifted muted #b3b2ad), so an empty field never reads as filled in; light 4.79:1 on
/// the #e4e4df input, dark 4.85:1 on the #2e2e2c dialog field (#979691 was 4.59; round 13 L2).
pub fn placeholder(p: &Palette) -> Color32 {
    p.placeholder()
}

/// A resting control's fill (button, chip, combo): `surface_2` in light (1.06:1 on the page); in dark DARK_BUTTON,
/// a step quieter than `surface_2` (#2a2a28 read 1.25:1 on the page against light's 1.06; round 15 B4).
pub fn button_fill(p: &Palette) -> Color32 {
    if p.dark { DARK_BUTTON } else { p.surface_2 }
}

/// Avatar sizes (`avatar::show`): on a pill line (Issues and Grooming rows), beside a name (detail fields,
/// pickers, the header), and before a section header (Team columns). AVATAR_GAP from the circle to the name.
pub const AVATAR_PILL: f32 = 16.0;
pub const AVATAR: f32 = 20.0;
pub const AVATAR_HEADER: f32 = 24.0;
pub const AVATAR_GAP: f32 = 8.0;

/// Dark's selected segment (`components::segment_fill`): a raised neutral one step over the DARK_BUTTON track
/// (1.51:1), ringed with DARK_EDGE (1.97:1 off the track), `text` on it (8.5:1). The light segment is the same idea
/// (white on `surface_2` with a `border` ring). The old #858480 with `bg` text read as an inverted grey block.
pub const DARK_SEGMENT: Color32 = hex(0x424240);

/// How opaque a disabled control is drawn: egui's `disabled_alpha` (every `add_enabled(false, ..)` widget) and the
/// disabled primary (`views::primary_disabled_colors`), so every disabled button is one look, the resting secondary
/// button faded. 0.7 keeps its label ≥ 5.5:1 on every page, card, selection and dialog fill in both themes (egui's
/// 0.5 read 2.2–3.2:1).
pub const DISABLED_ALPHA: f32 = 0.7;

/// The dark resting control fill: 1.18:1 on the page `bg`; `text` 12.7:1 and `muted` 5.9:1 on it.
pub const DARK_BUTTON: Color32 = hex(0x262624);

/// A hovered control (button, chip, combo) that rests on `button_fill`: in light the old #e6e6e2 step, since the
/// lighter list-row `hover_row` (#ebebe8) is only 1.05:1 off surface_2; in dark `hover_row`.
pub fn control_hover(p: &Palette) -> Color32 {
    if p.dark { p.hover_row } else { hex(0xe6e6e2) }
}

/// Muted text on a fill other than the page's (a selected or cursor row, a dialog): `muted` where it keeps
/// MUTED_MIN there, else the first of the theme's lifts that does (light #4d4c48 darker; dark #b3b2ad, then
/// #c0bfba, which the #474744 dialog cursor row stops at, then #cccbc6), else the last lift. Dark's first lift stays ≥ 1.75
/// off `text` (#c0bfba was 1.54 and read as text in dialogs and on selected rows; round 11 L1).
pub fn muted_on(fill: Color32, p: &Palette) -> Color32 {
    let lifts: &[Color32] = if p.dark { &MUTED_LIFTS_DARK } else { &MUTED_LIFTS_LIGHT };
    std::iter::once(p.muted).chain(lifts.iter().copied()).find(|&c| contrast(c, fill) >= MUTED_MIN).unwrap_or(*lifts.last().unwrap_or(&p.muted))
}

const MUTED_LIFTS_LIGHT: [Color32; 1] = [hex(0x4d4c48)];
const MUTED_LIFTS_DARK: [Color32; 3] = [hex(0xb3b2ad), hex(0xc0bfba), hex(0xcccbc6)];

/// The contrast `muted` must keep on a fill before `muted_on` lifts it (round 8: 4.5 → 5.0 in both themes).
pub const MUTED_MIN: f32 = 5.0;

/// Dark surfaces' field (the sign-in panel, the composer): 1.15:1 under `surface` (it used to be the page `bg`,
/// 1.10, so a field there read as a hole to the page).
const DARK_SURFACE_FIELD: Color32 = hex(0x121211);
/// The sunken field on the dark page (the list pane's search box): the ramp's floor, 1.09:1 under `bg`. Nothing in
/// the ramp clears INPUT_STEP under `bg` short of near-black, so the page field takes the floor; the ruled range is
/// 1.09–1.11 (round 11 L4, final: #0a0a09 at 1.104 and #0c0c0b both sit in it).
const DARK_SUNKEN: Color32 = hex(0x0c0c0b);
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
/// both themes. Light: white and bg → #e4e4df; dark: dialog → #2e2e2c, surface → #121211, bg → #0c0c0b (the
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

/// The light-theme chip step on white (code chips and blocks, help keys): 1.20:1 off the page and 1.07 off the
/// #e4e4df page input, so code never reads as a field (#e3e3de was 1.01 off it; round 11 L2). Chips inside a card
/// step on to #dfdfda (`chip_fill_on`).
const LIGHT_CHIP: Color32 = hex(0xebebe7);
/// The dark page's code chip: the first step past `surface_2`, 1.29:1 off the page (light's #ebebe7 is 1.20),
/// so page code is quieter than the card chip and the dialog fill it used to share (#383836, 1.39; round 14 L1).
const DARK_PAGE_CHIP: Color32 = hex(0x333331);
/// The dark modal dialog's fill: one step above `surface_2`, so a dialog lifts off the dimmed page.
pub const DARK_DIALOG: Color32 = hex(0x383836);
/// The lifted dark edge: modal dialogs, fields and buttons inside them.
pub const DARK_EDGE: Color32 = hex(0x545350);
/// The dark dialog's chip step (help keys): 1.32:1 off the dialog (#454542 read 1.22 and the keys floated),
/// quieter than DARK_EDGE's 1.53; `text` 7.5:1 on it.
const DARK_DIALOG_CHIP: Color32 = hex(0x4a4a47);
/// A chip's fill must stand at least this far off its container (1.14:1 read as flat).
const CHIP_STEP: f32 = 1.17;

/// The fill for a chip (inline code, a code block, a help key) inside a `container`: the first step of the
/// theme's neutral ramp at least CHIP_STEP off it, darker in light and lighter in dark (a key reads raised).
/// On the page (surface) that is #ebebe7 light, #333331 dark; in a comment card #dfdfda / #383836; on the dark
/// dialog (help keys) #454542.
pub fn chip_fill_on(container: Color32, p: &Palette) -> Color32 {
    let ramp: &[Color32] = if p.dark {
        &[p.surface_2, DARK_PAGE_CHIP, DARK_DIALOG, p.border, p.selected_row, DARK_DIALOG_CHIP, DARK_EDGE]
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

/// Visuals, fonts and the Compact spacing. `set_density` switches the spacing later.
pub fn apply(ctx: &egui::Context) {
    ctx.set_visuals_of(egui::Theme::Light, visuals(&LIGHT));
    ctx.set_visuals_of(egui::Theme::Dark, visuals(&DARK));
    set_density(ctx, Density::Compact);
    ctx.set_fonts(fonts());
}

/// Which palette the app draws with: the macOS appearance, or one fixed theme.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scheme {
    #[default]
    System,
    Light,
    Dark,
}

impl Scheme {
    pub fn parse(s: &str) -> Option<Scheme> {
        match s.trim().to_ascii_lowercase().as_str() {
            "system" => Some(Scheme::System),
            "light" => Some(Scheme::Light),
            "dark" => Some(Scheme::Dark),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Scheme::System => "System",
            Scheme::Light => "Light",
            Scheme::Dark => "Dark",
        }
    }

    /// The theme as a toast or menu says it: "System (dark)" names what System resolved to (`system` is the OS
    /// appearance, if known); Light and Dark are their names.
    pub fn label(self, system: Option<egui::Theme>) -> String {
        match (self, system) {
            (Scheme::System, Some(t)) => format!("System ({})", if t == egui::Theme::Dark { "dark" } else { "light" }),
            _ => self.name().to_string(),
        }
    }

    /// System, then Light, then Dark, then System again.
    pub fn next(self) -> Scheme {
        match self {
            Scheme::System => Scheme::Light,
            Scheme::Light => Scheme::Dark,
            Scheme::Dark => Scheme::System,
        }
    }

    pub fn preference(self) -> egui::ThemePreference {
        match self {
            Scheme::System => egui::ThemePreference::System,
            Scheme::Light => egui::ThemePreference::Light,
            Scheme::Dark => egui::ThemePreference::Dark,
        }
    }
}

/// How much room rows, cards and controls get. Compact is the tuned layout every metric test measures.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Density {
    #[default]
    Compact,
    Comfortable,
}

impl Density {
    pub fn parse(s: &str) -> Option<Density> {
        match s.trim().to_ascii_lowercase().as_str() {
            "compact" => Some(Density::Compact),
            "comfortable" => Some(Density::Comfortable),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Density::Compact => "Compact",
            Density::Comfortable => "Comfortable",
        }
    }

    pub fn toggled(self) -> Density {
        match self {
            Density::Compact => Density::Comfortable,
            Density::Comfortable => Density::Compact,
        }
    }

    pub fn spacing(self) -> &'static Spacing {
        match self {
            Density::Compact => &COMPACT,
            Density::Comfortable => &COMFORTABLE,
        }
    }
}

/// Every density-dependent size in one place, in points. Views read it through `spacing(ctx)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spacing {
    /// egui's gap between widgets.
    pub item: egui::Vec2,
    /// egui's button padding.
    pub button_padding: egui::Vec2,
    /// The Body and Button text styles.
    pub body: f32,
    /// Added above and below the content of every list row (Issues, Grooming, the roadmap table).
    pub row_extra: f32,
    /// A card's padding (Team and Roadmap cards, Grooming rows' sides, detail comment cards).
    pub card_pad: f32,
    /// Between cards in a column or list.
    pub card_gap: f32,
    /// Between a card's or row's lines.
    pub line_gap: f32,
    /// The detail pane's ink gap between sections (pills → Description → Comments).
    pub section_gap: f32,
    // The type scale. `body` above is the body step; use `theme::bold` for headings.
    /// Captions, pills, hint-bar words, timestamps.
    pub caption: f32,
    /// Secondary lines: meta rows, helper text, empty-state hints.
    pub small: f32,
    /// Section headers (`components::section_header`).
    pub h3: f32,
    /// Ticket titles on rows and cards (Issues, Grooming, Team, Me, Roadmap, Standup), always `theme::bold`
    /// (`components::card_title_font`).
    pub card_title: f32,
    /// Screen and panel titles.
    pub h2: f32,
    /// The one large title on a screen (the detail pane's issue title).
    pub h1: f32,
    // Prose (descriptions, comments, suggestion text).
    /// The widest a block of prose runs: about 70 characters at `body`.
    pub prose_max_width: f32,
    /// Prose line height as a multiple of its font size.
    pub prose_line_height: f32,
    /// Between paragraphs of prose, ink to ink.
    pub paragraph_gap: f32,
    /// The filter bar's controls (combos, toggle chips, the search box): `views::BUTTON_H` in Compact, taller in
    /// Comfortable so the bar breathes with the rows.
    pub control_h: f32,
}

/// The layout as tuned and measured; must not change (the glyph metric tests pin it).
pub const COMPACT: Spacing = Spacing {
    item: egui::vec2(8.0, 6.0),
    button_padding: egui::vec2(12.0, 4.0),
    body: 14.0,
    row_extra: 0.0,
    card_pad: 12.0,
    card_gap: 8.0,
    line_gap: 4.0,
    section_gap: 32.0,
    caption: 12.0,
    small: 13.0,
    h3: 16.0,
    card_title: 13.5,
    h2: 20.0,
    h1: 24.0,
    prose_max_width: 480.0,
    prose_line_height: 1.5,
    paragraph_gap: 12.0,
    control_h: 28.0,
};

/// More air: taller rows, roomier cards, a point more body text.
pub const COMFORTABLE: Spacing = Spacing {
    item: egui::vec2(10.0, 8.0),
    button_padding: egui::vec2(14.0, 6.0),
    body: 15.0,
    row_extra: 4.0,
    card_pad: 16.0,
    card_gap: 12.0,
    line_gap: 6.0,
    section_gap: 40.0,
    caption: 12.0,
    small: 13.5,
    h3: 17.0,
    // A step over body, so a title reads above the prose under it on a roomy card.
    card_title: 16.0,
    h2: 21.0,
    h1: 26.0,
    prose_max_width: 520.0,
    prose_line_height: 1.55,
    paragraph_gap: 16.0,
    control_h: 32.0,
};

fn density_id() -> egui::Id {
    egui::Id::new("jira-tickets-density")
}

/// The density last set on `ctx`; Compact until `set_density` says otherwise.
pub fn density(ctx: &egui::Context) -> Density {
    ctx.data(|d| d.get_temp::<Density>(density_id())).unwrap_or_default()
}

/// The spacing of `ctx`'s density.
pub fn spacing(ctx: &egui::Context) -> &'static Spacing {
    density(ctx).spacing()
}

/// Stores `d` on `ctx` and sets egui's spacing and text styles to match.
pub fn set_density(ctx: &egui::Context, d: Density) {
    ctx.data_mut(|m| m.insert_temp(density_id(), d));
    let sp = d.spacing();
    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = sp.item;
        s.spacing.button_padding = sp.button_padding;
        s.text_styles.insert(egui::TextStyle::Body, body(sp.body));
        s.text_styles.insert(egui::TextStyle::Button, body(sp.body));
        s.text_styles.insert(egui::TextStyle::Small, body(sp.caption));
        s.text_styles.insert(egui::TextStyle::Monospace, mono(12.5));
        s.text_styles.insert(egui::TextStyle::Heading, bold(sp.h2));
    });
}

/// The appearance the user asked for: config's `[appearance]` table, or a choice made in the app.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Appearance {
    pub theme: Scheme,
    pub density: Density,
}

/// A choice made in the app, saved with the config value it replaced.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SavedAppearance {
    /// The config's `[appearance]` when the choice was made.
    pub config: Appearance,
    pub chosen: Appearance,
}

/// What a launch uses: the saved choice while the config still says what it said when the choice was made;
/// once the config's `[appearance]` is edited, the config wins.
pub fn effective_appearance(config: Appearance, saved: Option<SavedAppearance>) -> Appearance {
    match saved {
        Some(s) if s.config == config => s.chosen,
        _ => config,
    }
}

/// The painted ink box of a galley, relative to its origin: the union of its glyphs' raster boxes (what a
/// rater measures; egui's margins and rects are to the line box). Empty galleys give `Rect::NOTHING`.
pub fn ink_rect(g: &egui::Galley) -> egui::Rect {
    let ppp = g.pixels_per_point.max(1.0);
    // Each glyph's raster lands on a whole pixel inside its row, as the tessellator places it, so ink edges (and
    // the spaces derived from them) stay on the pixel grid.
    let snap = |v: f32| (v * ppp).round() / ppp;
    let mut r = egui::Rect::NOTHING;
    for row in &g.rows {
        for gl in &row.glyphs {
            if gl.uv_rect.is_nothing() || gl.uv_rect.size.x <= 0.0 {
                continue;
            }
            let at = gl.pos + gl.uv_rect.offset;
            let min = row.pos + egui::vec2(snap(at.x), snap(at.y));
            r = r.union(egui::Rect::from_min_size(min, gl.uv_rect.size));
        }
    }
    r
}

/// A glyph raster's coverage below this reads as background: `visible_ink` trims raster columns that faint
/// (the antialiasing margin a glyph's bitmap carries, up to a device pixel each side).
const VISIBLE_ALPHA: u8 = 26;

/// `ink_rect`'s x edges trimmed to the glyphs' visible coverage in the font atlas: a raster's outer columns can be
/// near-empty, so gaps set from the raster box read up to a device pixel wide (round 15 H1: 4.5 after "J/K", "c").
/// Cached per galley text and fonts in `ctx`'s memory; the atlas is read only on a miss.
pub fn visible_ink(ctx: &egui::Context, g: &egui::Galley) -> egui::Rect {
    visible_ink_at(ctx, g, VISIBLE_ALPHA)
}

/// `visible_ink` with its coverage threshold.
fn visible_ink_at(ctx: &egui::Context, g: &egui::Galley, alpha: u8) -> egui::Rect {
    let r = ink_rect(g);
    if !r.is_positive() {
        return r;
    }
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    g.job.text.hash(&mut h);
    for s in &g.job.sections {
        s.format.font_id.hash(&mut h);
        s.leading_space.to_bits().hash(&mut h);
    }
    g.pixels_per_point.to_bits().hash(&mut h);
    alpha.hash(&mut h);
    // The rasters themselves too: the first frame's fonts can land their glyphs in the very cells the app's fonts
    // take once loaded (a rebuilt atlas fills in the same order), so the advances and raster boxes join the key.
    for row in &g.rows {
        for gl in &row.glyphs {
            (gl.uv_rect.min, gl.uv_rect.max).hash(&mut h);
            for v in [gl.advance_width, gl.uv_rect.offset.x, gl.uv_rect.offset.y, gl.uv_rect.size.x, gl.uv_rect.size.y, gl.pos.x] {
                v.to_bits().hash(&mut h);
            }
        }
    }
    let id = egui::Id::new(("visible-ink", h.finish()));
    let trim = ctx.data(|d| d.get_temp::<(f32, f32)>(id)).unwrap_or_else(|| {
        let atlas = ctx.fonts(|f| f.image());
        let ppp = g.pixels_per_point.max(1.0);
        let snap = |v: f32| (v * ppp).round() / ppp;
        // Each glyph's visible columns, as (left, right) in galley points.
        let (mut left, mut right) = (f32::INFINITY, f32::NEG_INFINITY);
        for row in &g.rows {
            for gl in &row.glyphs {
                let uv = gl.uv_rect;
                if uv.is_nothing() || uv.size.x <= 0.0 {
                    continue;
                }
                let x0 = row.pos.x + snap(gl.pos.x + uv.offset.x);
                let cols = usize::from(uv.max[0].saturating_sub(uv.min[0]));
                let seen = |c: usize| {
                    (usize::from(uv.min[1])..usize::from(uv.max[1])).any(|y| {
                        let x = usize::from(uv.min[0]) + c;
                        y < atlas.size[1] && x < atlas.size[0] && atlas.pixels[y * atlas.size[0] + x].a() >= alpha
                    })
                };
                let Some(first) = (0..cols).find(|&c| seen(c)) else { continue };
                let last = (0..cols).rev().find(|&c| seen(c)).unwrap_or(first);
                left = left.min(x0 + first as f32 / ppp);
                right = right.max(x0 + (last + 1) as f32 / ppp);
            }
        }
        let t = if left.is_finite() { (left - r.left(), r.right() - right) } else { (0.0, 0.0) };
        ctx.data_mut(|d| d.insert_temp(id, t));
        t
    });
    egui::Rect::from_x_y_ranges(r.left() + trim.0..=r.right() - trim.1, r.y_range())
}

/// One font's ink offsets inside its line box, measured on reference glyphs with the loaded fonts at the
/// current pixel density. "Spacing is measured to the ink": a container's margin is the ruled ink gap minus
/// these (`pad_*`), so the ink, not the line box, lands on the 4 / 8 / 12 / 16 / 24 scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ink {
    /// Line-box top → the top of the tallest ascender / capital ("Hdlk").
    pub top: f32,
    /// Line-box top → the baseline.
    pub base: f32,
    /// Baseline → the bottom of the descenders ("gpqy").
    pub descent: f32,
    /// The line box's height.
    pub line: f32,
}

/// The item spacing that puts `target` between the ink of `left` and `right` laid side by side: the target less
/// `left`'s right bearing (its advance past its ink) and `right`'s left bearing.
pub fn ink_gap(left: &egui::Galley, right: &egui::Galley, target: f32) -> f32 {
    let (l, r) = (ink_rect(left), ink_rect(right));
    let rb = if l.is_positive() { left.size().x - l.right() } else { 0.0 };
    let lb = if r.is_positive() { r.left() } else { 0.0 };
    target - rb - lb
}

/// Reference glyphs for the ascender / cap line and for the descender line.
const INK_TALL: &str = "Hdlk";
const INK_DEEP: &str = "gpqy";

impl Ink {
    /// Measures `font` (with `line_height`, or the font's own) in `ctx`'s fonts.
    pub fn of(ctx: &egui::Context, font: &FontId, line_height: Option<f32>) -> Ink {
        let lay = |t: &str| {
            let mut job = egui::text::LayoutJob::simple_singleline(t.to_string(), font.clone(), Color32::WHITE);
            if let Some(s) = job.sections.first_mut() {
                s.format.line_height = line_height;
            }
            ctx.fonts_mut(|f| f.layout_job(job))
        };
        let tall = lay(INK_TALL);
        let deep = lay(INK_DEEP);
        let base = tall.rows.first().and_then(|r| r.glyphs.first().map(|g| r.pos.y + g.pos.y)).unwrap_or(0.0);
        Ink { top: ink_rect(&tall).top(), base, descent: ink_rect(&deep).bottom() - base, line: tall.size().y }
    }
    /// The margin above a line box for `target` to the cap / ascender ink.
    pub fn pad_top(&self, target: f32) -> f32 {
        target - self.top
    }
    /// The margin below a line box for `target` to the descender ink.
    pub fn pad_deep(&self, target: f32) -> f32 {
        target - (self.line - self.base - self.descent)
    }
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

    #[test]
    fn pill_edges_show_on_cards_and_dialogs() {
        for (p, dialog) in [(&LIGHT, LIGHT.surface), (&DARK, DARK_DIALOG)] {
            for fill in [dialog, p.surface, p.selection()] {
                let c = contrast(p.pill_edge(), fill);
                assert!(c >= 1.3, "dark={} on {fill:?}: {c:.2}", p.dark);
            }
        }
    }

    fn none() -> BTreeMap<String, String> {
        BTreeMap::new()
    }

    #[test]
    fn scheme_and_density_parse_and_cycle() {
        assert_eq!(Scheme::parse(" Dark "), Some(Scheme::Dark));
        assert_eq!(Scheme::parse("system"), Some(Scheme::System));
        assert_eq!(Scheme::parse("sepia"), None);
        assert_eq!(Scheme::System.next().next().next(), Scheme::System);
        assert_eq!(Scheme::Light.next(), Scheme::Dark);
        assert_eq!(Density::parse("COMFORTABLE"), Some(Density::Comfortable));
        assert_eq!(Density::parse("cozy"), None);
        assert_eq!(Density::Compact.toggled(), Density::Comfortable);
        assert_eq!(Density::Comfortable.toggled(), Density::Compact);
    }

    #[test]
    fn compact_is_the_tuned_layout_and_comfortable_is_roomier() {
        let (c, m) = (Density::Compact.spacing(), Density::Comfortable.spacing());
        assert_eq!((c.item, c.button_padding, c.body), (egui::vec2(8.0, 6.0), egui::vec2(12.0, 4.0), 14.0));
        assert_eq!((c.row_extra, c.card_pad, c.card_gap, c.line_gap, c.section_gap), (0.0, 12.0, 8.0, 4.0, 32.0));
        assert!(m.item.x > c.item.x && m.item.y > c.item.y && m.button_padding.y > c.button_padding.y);
        assert!(m.body > c.body && m.row_extra > 0.0 && m.card_pad > c.card_pad);
        assert!(m.card_gap > c.card_gap && m.line_gap > c.line_gap && m.section_gap > c.section_gap);
    }

    #[test]
    fn density_lives_on_the_context() {
        let ctx = egui::Context::default();
        assert_eq!(density(&ctx), Density::Compact, "Compact until set");
        set_density(&ctx, Density::Comfortable);
        assert_eq!(spacing(&ctx), &COMFORTABLE);
        assert_eq!(ctx.style_of(egui::Theme::Dark).spacing.item_spacing, COMFORTABLE.item);
        set_density(&ctx, Density::Compact);
        assert_eq!(ctx.style_of(egui::Theme::Dark).spacing.item_spacing, egui::vec2(8.0, 6.0));
    }

    #[test]
    fn a_saved_choice_holds_until_the_config_changes() {
        let config = Appearance::default();
        let chosen = Appearance { theme: Scheme::Dark, density: Density::Comfortable };
        assert_eq!(effective_appearance(config, None), config);
        assert_eq!(effective_appearance(config, Some(SavedAppearance { config, chosen })), chosen);
        let edited = Appearance { theme: Scheme::Light, ..config };
        assert_eq!(effective_appearance(edited, Some(SavedAppearance { config, chosen })), edited, "an edited config wins");
    }

    #[test]
    fn ink_offsets_invert_with_the_real_fonts() {
        // Spacing is measured to the ink: a line `pad_top(12)` under a fill's edge inks 12 under it, and
        // `pad_deep(12)` leaves 12 under its descenders, for every font the containers use (captures' 2x). Glyph
        // rasters shift by half a pixel with their subpixel x, so the tolerance is one device pixel.
        let marker = Color32::from_rgb(1, 2, 3);
        let fonts = [
            (body(14.0), None),
            (body(14.0), Some(20.0)),
            (body(12.0), None),
            (bold(13.5), None),
            (bold(15.0), None),
            (bold(22.0), None),
            (mono(12.5), Some(18.5)),
        ];
        for (font, lh) in fonts {
            let text = format!("{INK_TALL} {INK_DEEP}");
            let shapes = crate::testutil::paint_at2(egui::vec2(400.0, 200.0), |ui| {
                let ink = Ink::of(ui.ctx(), &font, lh);
                assert!(ink.top > 0.0 && ink.base > ink.top && ink.descent > 0.0 && ink.line >= ink.base + ink.descent, "{font:?}: {ink:?}");
                egui::Frame::new().fill(marker).show(ui, |ui| {
                    ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                    ui.add_space(ink.pad_top(12.0));
                    ui.label(egui::RichText::new(&text).font(font.clone()).line_height(lh));
                    ui.add_space(ink.pad_deep(12.0));
                });
            });
            let (fill, _) = *crate::testutil::fills(&shapes).iter().find(|(_, c)| *c == marker).expect("fill");
            let ink = crate::testutil::ink_of(&shapes, &text);
            assert!((ink.top() - fill.top() - 12.0).abs() <= 0.5, "{font:?} {lh:?} top {}", ink.top() - fill.top());
            assert!((fill.bottom() - ink.bottom() - 12.0).abs() <= 0.5, "{font:?} {lh:?} bottom {}", fill.bottom() - ink.bottom());
        }
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
    fn work_in_progress_reads_as_in_progress() {
        for p in [&LIGHT, &DARK] {
            assert_eq!(status_tint("Work in Progress", "new", p, &none()), status_tint("In Progress", "indeterminate", p, &none()));
        }
    }

    #[test]
    fn dark_new_text_steps_off_body_text() {
        // L1: the grey hue's dark text is #cccbc9, further off `text` than the other hues' 1.27, ≥ 4.5 on its wash
        // over every row fill, the selection included.
        let t = tint_from_hue(hex(GREY), &DARK);
        assert_eq!(t.fg, Color32::from_rgb(0xcc, 0xcb, 0xc9));
        let off = contrast(t.fg, DARK.text);
        assert!(off >= 1.35, "{off:.2}");
        for row in [DARK.surface, DARK.selected_row, DARK.hover_row, DARK.bg] {
            let c = contrast(t.fg, over(t.bg, row));
            assert!(c >= 4.5, "{row:?}: {c:.2}");
        }
        assert_eq!(tint_from_hue(hex(BLUE), &DARK).fg, mix(hex(BLUE), 255, DARK_TEXT_MIX), "other hues keep theirs");
        // Round 14 L3: the grey wash on a plain dark row ≈ 1.45, near light's; other hues keep DARK_WASH.
        let wash = contrast(over(t.bg, DARK.surface), DARK.surface);
        let light = contrast(over(tint_from_hue(hex(GREY), &LIGHT).bg, LIGHT.surface), LIGHT.surface);
        assert!((1.42..=1.48).contains(&wash) && wash - light < 0.12, "dark {wash:.3}, light {light:.3}");
        assert_eq!(tint_from_hue(hex(BLUE), &DARK).bg.a(), DARK_WASH);
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
        assert_eq!(chip_fill_on(LIGHT.surface, &LIGHT), hex(0xebebe7), "code on the light page");
        assert!(contrast(LIGHT_CHIP, LIGHT.surface) >= 1.19, "{:.3} off white", contrast(LIGHT_CHIP, LIGHT.surface));
        assert!(contrast(LIGHT_CHIP, LIGHT_INPUT) >= 1.06, "{:.3} off the input", contrast(LIGHT_CHIP, LIGHT_INPUT));
        for p in [&LIGHT, &DARK] {
            for container in [p.surface, p.surface_2] {
                assert!(contrast(p.text, chip_fill_on(container, p)) >= 7.0, "code text on its chip (dark={})", p.dark);
            }
        }
        assert_eq!(chip_fill_on(DARK.surface, &DARK), hex(0x333331), "code on the dark page");
        let (dark_page, light_page) = (contrast(DARK_PAGE_CHIP, DARK.surface), contrast(LIGHT_CHIP, LIGHT.surface));
        assert!((1.28..1.30).contains(&dark_page), "L1: {dark_page:.3}");
        assert!(dark_page - light_page < 0.1, "dark page chips no louder than light by much");
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
        assert_eq!(input_fill(DARK.bg, &DARK), hex(0x0c0c0b), "dark page field: the ramp's floor");
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
        assert!((1.09..=1.11).contains(&step(DARK.bg)), "page {:.3}: the ruled 1.09–1.11", step(DARK.bg));
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
    fn light_health_colours_read_on_surface_2() {
        // L3 (round 13 L4): ≥ 5.2:1 on surface_2, the lightest fill they sit on besides white.
        for c in [LIGHT.on_track, LIGHT.at_risk] {
            assert!(contrast(c, LIGHT.surface_2) >= 5.2, "{c:?}: {:.2}", contrast(c, LIGHT.surface_2));
        }
    }

    #[test]
    fn placeholders_read_apart_from_typed_text() {
        assert_eq!(placeholder(&DARK), hex(0x9c9b96));
        // L2: 4.85 on the dark dialog field, and still a step under the dialog's muted.
        assert!(contrast(placeholder(&DARK), dialog_input_fill(&DARK)) >= 4.8, "{:.2}", contrast(placeholder(&DARK), dialog_input_fill(&DARK)));
        assert!(contrast(placeholder(&DARK), DARK.text) > contrast(muted_on(DARK_DIALOG, &DARK), DARK.text), "under the dialog muted");
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
            for fill in [p.selected_row, p.hover_row, p.selection()] {
                assert!(contrast(muted_on(fill, p), fill) >= 4.5, "dark={} {fill:?}: {:.2}", p.dark, contrast(muted_on(fill, p), fill));
            }
        }
        for p in [&LIGHT, &DARK] {
            assert!(contrast(muted_on(p.selection(), p), p.selection()) >= 5.0, "picker cursor rows: dark={}", p.dark);
            assert!(contrast(p.text, muted_on(p.selection(), p)) >= 1.5, "apart from text: dark={}", p.dark);
        }
        assert_eq!(muted_on(DARK.surface, &DARK), DARK.muted, "narrow: only where muted falls short");
        // Round 9: light muted #585753 clears 5.0 on the selected row (5.41), so it needs no lift there.
        assert_eq!(muted_on(LIGHT.selected_row, &LIGHT), LIGHT.muted, "light muted 5.41 on the selected row");
        assert!(contrast(placeholder(&LIGHT), LIGHT.muted) >= 1.15, "light muted and placeholder are a visible step apart");
        assert_eq!(muted_on(LIGHT.surface, &LIGHT), LIGHT.muted);
        assert_eq!(muted_on(DARK_DIALOG, &DARK), hex(0xb3b2ad), "dark dialog: the first lift");
        assert_eq!(muted_on(DARK.selected_row, &DARK), hex(0xb3b2ad), "dark selected row: the first lift");
        for fill in [DARK_DIALOG, DARK.selected_row] {
            let m = muted_on(fill, &DARK);
            assert!(contrast(m, fill) >= MUTED_MIN, "{:.2}", contrast(m, fill));
            assert!(contrast(DARK.text, m) >= 1.75, "lifted muted stays apart from text: {:.2}", contrast(DARK.text, m));
        }
        for p in [&LIGHT, &DARK] {
            for fill in [p.surface, p.bg, p.selected_row, p.hover_row, if p.dark { DARK_DIALOG } else { p.surface }] {
                assert!(contrast(muted_on(fill, p), fill) >= MUTED_MIN, "dark={} {fill:?}: {:.2}", p.dark, contrast(muted_on(fill, p), fill));
            }
        }
    }

    #[test]
    fn dark_buttons_sit_quiet_with_readable_labels() {
        // Round 15 B4: the dark resting control 1.18:1 on the page (was 1.25); its labels keep 4.5:1.
        let c = contrast(button_fill(&DARK), DARK.bg);
        assert!((1.17..=1.19).contains(&c), "{c:.3}");
        for t in [DARK.text, DARK.muted] {
            assert!(contrast(t, button_fill(&DARK)) >= 4.5);
        }
        assert_eq!(button_fill(&LIGHT), LIGHT.surface_2, "light unchanged");
        // L3: the dark health colours ≥ 4.5 on the surface and on a card.
        for h in [DARK.on_track, DARK.at_risk] {
            assert!(contrast(h, DARK.surface) >= 4.5 && contrast(h, DARK.surface_2) >= 4.5, "{h:?}");
        }
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
        // The full 34-epic list arrives first (nothing on screen yet): slots wrap mod EPIC_SLOTS.
        assign_epic_slots(&mut slots, [], all.iter().copied());
        // Then rows show seven epics whose list slots collide (CI-100, CI-108 and CI-116 share slot 0, ...).
        let visible = ["CI-100", "CI-108", "CI-116", "CI-101", "CI-109", "CI-102", "CI-110"];
        assign_epic_slots(&mut slots, visible, all.iter().copied());
        let hues: BTreeSet<usize> = visible.iter().map(|k| slots[*k]).collect();
        assert_eq!(hues.len(), visible.len(), "≤EPIC_SLOTS distinct visible epics, distinct hues: {slots:?}");
        // A full screen: every hue.
        let ten: Vec<&str> = all[20..20 + EPIC_SLOTS].to_vec();
        assign_epic_slots(&mut slots, ten.iter().copied(), all.iter().copied());
        assert_eq!(ten.iter().map(|k| slots[*k]).collect::<BTreeSet<_>>().len(), EPIC_SLOTS);
        // Stable: running again (every frame) moves nothing.
        let before = slots.clone();
        assign_epic_slots(&mut slots, ten.iter().copied(), all.iter().copied());
        assert_eq!(slots, before);
    }

    #[test]
    fn epic_hues_go_farthest_first_and_stay_across_lists() {
        let mut slots = BTreeMap::new();
        assign_epic_slots(&mut slots, ["CI-9", "CI-3", "CI-7", "CI-9"], ["CI-1", "CI-2"]);
        assert_eq!((slots["CI-9"], slots["CI-3"], slots["CI-7"]), (0, 4, 2), "farthest free hue first, row order");
        assert_eq!((slots["CI-1"], slots["CI-2"]), (EPIC_SLOT_ORDER[3], EPIC_SLOT_ORDER[4]), "then the rest of the list");
        // The first four slots are two or more slots apart.
        let n = EPIC_SLOTS as i32;
        for (i, a) in EPIC_SLOT_ORDER[..4].iter().enumerate() {
            for b in &EPIC_SLOT_ORDER[i + 1..4] {
                let d = (*a as i32 - *b as i32).rem_euclid(n).min((*b as i32 - *a as i32).rem_euclid(n));
                assert!(d >= 2, "{a} {b}");
            }
        }
        // Another list: a known key keeps its hue; a new one takes a slot free on screen.
        assign_epic_slots(&mut slots, ["CI-7", "CI-4", "CI-3"], []);
        assert_eq!((slots["CI-7"], slots["CI-3"]), (2, 4), "same key, same hue in every list");
        assert!(![2, 4].contains(&slots["CI-4"]));
        assert_eq!(EPIC_SLOT_ORDER.iter().collect::<BTreeSet<_>>().len(), EPIC_SLOTS);
    }

    #[test]
    fn unassigned_epics_spread_by_key() {
        // Round 3: every epic without a slot fell back to slot 0, so 10 of 16 Grooming epics were one orange.
        let keys: Vec<String> = (1841..1841 + EPIC_SLOTS as u32).map(|n| format!("CI-{n}")).collect();
        let hues: BTreeSet<usize> = keys.iter().map(|k| epic_fallback_slot(k)).collect();
        assert_eq!(hues.len(), EPIC_SLOTS, "any EPIC_SLOTS consecutive keys take every slot");
        assert_ne!(epic_fallback_slot("CI-1841"), epic_fallback_slot("CI-1842"), "neighbours differ");
        assert_eq!(epic_fallback_slot("CI-9"), epic_fallback_slot("CI-9"), "stable");
        assert!(epic_fallback_slot("no-number") < EPIC_SLOTS);
        let none = BTreeMap::new();
        assert_eq!(epic_tint_in("CI-1842", &BTreeMap::new(), &LIGHT, &none), slot_tint(epic_fallback_slot("CI-1842"), &LIGHT));
    }

    #[test]
    fn more_visible_epics_than_slots_share_hues_evenly() {
        let keys: Vec<String> = (0..16).map(|n| format!("E-{}", n * 8)).collect();
        let all: Vec<&str> = keys.iter().map(String::as_str).collect();
        // The epic list first: these keys all land on slots in list order.
        let mut slots = BTreeMap::new();
        assign_epic_slots(&mut slots, [], all.iter().copied());
        for _ in 0..2 {
            assign_epic_slots(&mut slots, all.iter().copied(), []);
            let mut count = [0; EPIC_SLOTS];
            for k in &all {
                count[slots[*k]] += 1;
            }
            assert!(count.iter().all(|&c| c == 2), "16 visible epics, each hue twice: {count:?}");
        }
        // Stable once given.
        let before = slots.clone();
        assign_epic_slots(&mut slots, all.iter().copied(), []);
        assert_eq!(slots, before);
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
    fn the_at_risk_dot_passes_as_a_mark_on_every_row_fill() {
        for p in [LIGHT, DARK] {
            for fill in [p.surface, p.bg, p.surface_2, p.selection(), p.hover_row, p.selected_row] {
                assert!(contrast(p.at_risk_mark(), fill) >= 3.0, "dark={} on {fill:?}: {:.2}", p.dark, contrast(p.at_risk_mark(), fill));
            }
        }
        assert_ne!(LIGHT.at_risk_mark(), LIGHT.at_risk, "light: amber, not the brown text step");
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

    #[test]
    fn selection_is_accent_tinted_and_apart_from_neutral_surfaces() {
        for p in [&LIGHT, &DARK] {
            let sel = p.selection();
            // A blue cast: the accent's hue, not another grey.
            assert!(i32::from(sel.b()) - i32::from(sel.r()) >= 15, "dark={}: {sel:?}", p.dark);
            for neutral in [p.surface, p.surface_2, p.hover_row, p.selected_row, chip_fill_on(p.surface, p), input_fill(p.surface, p)] {
                assert_ne!(sel, neutral, "dark={}", p.dark);
            }
            // A quiet tint: the accent ring carries "selected"; the fill only has to read as a different surface.
            assert!(delta_e2000(sel, p.surface) >= SEL_OFF_SURFACE, "dark={}: ΔE {:.1} off the card", p.dark, delta_e2000(sel, p.surface));
            assert!(delta_e2000(sel, p.hover_row) >= SEL_OFF_SURFACE, "dark={}: ΔE {:.1} off hover", p.dark, delta_e2000(sel, p.hover_row));
            assert!(contrast(p.selection_edge(), sel) >= 3.0, "dark={}: ring {:.2}", p.dark, contrast(p.selection_edge(), sel));
            assert!(contrast(p.selection_edge(), p.surface) >= 3.0, "dark={}", p.dark);
            assert!(contrast(p.text, sel) >= 7.0, "dark={}", p.dark);
            assert!(contrast(muted_on(sel, p), sel) >= 4.5, "dark={}", p.dark);
            assert!(contrast(p.placeholder(), sel) >= 4.5, "dark={}", p.dark);
            // Pills keep reading on it: status text over its wash, epic text and dots.
            for (name, cat) in [("New", "new"), ("In Progress", "indeterminate"), ("Blocked", "new"), ("Done", "done"), ("Ready for Work", "new")] {
                let t = status_tint(name, cat, p, &none());
                let c = contrast(t.fg, over(t.bg, sel));
                assert!(c >= 4.5, "dark={} {name}: {c:.2}", p.dark);
            }
            for s in 0..EPIC_SLOTS {
                let t = slot_tint(s, p);
                assert!(contrast(t.fg, sel) >= 4.5 && contrast(t.stripe, sel) >= 3.0, "dark={} slot {s}", p.dark);
            }
        }
    }

    /// The selection's least ΔE2000 off the card and the hover fill.
    const SEL_OFF_SURFACE: f32 = 3.0;

    #[test]
    fn status_pill_fills_stay_apart_from_the_selection() {
        // One pill rule in both themes: an opaque fill (the wash over `surface`), no edge. So the selection, not a
        // stroke, keeps every pill's shape on a selected row: every fill stands off it.
        let names = [("New", "new"), ("Planning", "new"), ("Ready for Work", "new"), ("Work in Progress", "indeterminate")];
        let more = [("In Review", "indeterminate"), ("Blocked", "new"), ("Done", "done"), ("Waiting for support", "new")];
        for p in [&LIGHT, &DARK] {
            let sel = p.selection();
            for (name, cat) in names.iter().chain(&more) {
                let t = status_tint(name, cat, p, &none());
                let fill = over(t.bg, p.surface);
                let (de, c) = (delta_e2000(fill, sel), contrast(fill, sel));
                eprintln!("dark={} {name}: ΔE {de:.1} {c:.2}:1  (on card ΔE {:.1})", p.dark, delta_e2000(fill, p.surface));
                assert!(de >= PILL_OFF_SELECTION && c >= 1.2, "dark={} {name}: ΔE {de:.1}, {c:.2}:1 off the selection", p.dark);
                assert!(contrast(t.fg, fill) >= 4.5, "dark={} {name}", p.dark);
            }
        }
    }

    /// A status pill's fill's least ΔE2000 off the selection.
    const PILL_OFF_SELECTION: f32 = 6.0;

    #[test]
    fn colour_roles_pass_aa() {
        for p in [&LIGHT, &DARK] {
            for fill in [p.surface, p.bg, p.surface_2] {
                assert!(contrast(p.placeholder(), fill) >= 4.5, "dark={} placeholder {:.2}", p.dark, contrast(p.placeholder(), fill));
                // Issue keys and other secondary text are `muted`.
                assert!(contrast(p.muted, fill) >= 4.5, "dark={} muted", p.dark);
            }
            assert_ne!(p.placeholder(), p.off_track, "missing is not an error");
            assert!(contrast(p.text, p.accent_subtle()) >= 7.0, "dark={}", p.dark);
            assert!(contrast(p.accent_subtle(), p.surface) >= 1.2, "dark={}: a bar shows on a card", p.dark);
            assert!(contrast(p.on_accent, p.accent) >= 4.5, "dark={}: primary text {:.2}", p.dark, contrast(p.on_accent, p.accent));
            assert_eq!(placeholder(p), p.placeholder());
        }
        assert!(DARK.scrim_alpha() > LIGHT.scrim_alpha());
        for p in [&LIGHT, &DARK] {
            // A search match reads at a glance on a plain and a selected row, with body text on it.
            assert!(contrast(p.search_match(), p.surface) >= 1.5 && contrast(p.search_match(), p.selection()) >= 1.35, "dark={}", p.dark);
            assert!(contrast(p.text, p.search_match()) >= 7.0, "dark={}", p.dark);
        }
        assert!(DARK.dialog_shadow().color.a() > LIGHT.dialog_shadow().color.a());
    }

    #[test]
    fn epic_hues_skip_danger_red_and_success_green() {
        // OKLCH hue of an sRGB colour, in degrees.
        let hue = |c: Color32| {
            let lin = |v: u8| {
                let v = f64::from(v) / 255.0;
                if v <= 0.040_45 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
            };
            let (r, g, b) = (lin(c.r()), lin(c.g()), lin(c.b()));
            let l = (0.412_221_470_8 * r + 0.536_332_536_3 * g + 0.051_445_992_9 * b).cbrt();
            let m = (0.211_903_498_2 * r + 0.680_699_545_1 * g + 0.107_396_956_6 * b).cbrt();
            let s = (0.088_302_461_9 * r + 0.281_718_837_6 * g + 0.629_978_700_5 * b).cbrt();
            let a = 1.977_998_495_1 * l - 2.428_592_205 * m + 0.450_593_709_9 * s;
            let bb = 0.025_904_037_1 * l + 0.782_771_766_2 * m - 0.808_675_766 * s;
            bb.atan2(a).to_degrees().rem_euclid(360.0)
        };
        let apart = |a: f64, b: f64| {
            let d = (a - b).rem_euclid(360.0);
            d.min(360.0 - d)
        };
        for p in [&LIGHT, &DARK] {
            let danger = hue(p.off_track);
            let success = hue(p.on_track);
            for s in 0..EPIC_SLOTS {
                let h = hue(slot_tint(s, p).stripe);
                assert!(apart(h, danger) >= 25.0, "dark={} slot {s}: {h:.0}° vs danger {danger:.0}°", p.dark);
                assert!(apart(h, success) >= 25.0, "dark={} slot {s}: {h:.0}° vs success {success:.0}°", p.dark);
            }
        }
    }

    #[test]
    fn system_theme_says_what_it_resolved_to() {
        assert_eq!(Scheme::System.label(Some(egui::Theme::Dark)), "System (dark)");
        assert_eq!(Scheme::System.label(Some(egui::Theme::Light)), "System (light)");
        assert_eq!(Scheme::System.label(None), "System");
        assert_eq!(Scheme::Dark.label(Some(egui::Theme::Light)), "Dark");
    }

    #[test]
    fn type_scale_steps_and_compact_keeps_its_layout() {
        for sp in [&COMPACT, &COMFORTABLE] {
            assert!(sp.caption < sp.small && sp.small < sp.body && sp.body < sp.h3 && sp.h3 < sp.h2 && sp.h2 < sp.h1);
            assert!(sp.h3 - sp.body >= 2.0, "a section header is a clear step over body text");
            assert!((1.4..=1.6).contains(&sp.prose_line_height));
        }
        assert_eq!((COMPACT.caption, COMPACT.small, COMPACT.h3, COMPACT.h2, COMPACT.h1), (12.0, 13.0, 16.0, 20.0, 24.0));
        let (c, m) = (&COMPACT, &COMFORTABLE);
        assert!(m.small >= c.small && m.h3 > c.h3 && m.h1 > c.h1 && m.prose_max_width > c.prose_max_width && m.paragraph_gap > c.paragraph_gap);
    }

    #[test]
    #[cfg_attr(not(target_os = "macos"), ignore = "glyph-ink metrics are calibrated on macOS font rasterisation")]
    fn the_prose_column_holds_about_70_characters() {
        // About 70 characters of real prose at body size, measured with the app's fonts.
        for sp in [&COMPACT, &COMFORTABLE] {
            let chars = crate::testutil::with_ui(|ui| {
                let sample = "Done means both are merged and the nightly sweep passes without skipped tests, and the runbook names the new path.";
                let w = ui.painter().layout_no_wrap(sample.into(), body(sp.body), Color32::WHITE).size().x;
                sp.prose_max_width / (w / sample.chars().count() as f32)
            });
            assert!((65.0..=82.0).contains(&chars), "{chars:.0} characters a line");
        }
    }

    #[test]
    fn a_disabled_button_label_still_reads() {
        // The resting secondary button (button_fill, `text`) faded to DISABLED_ALPHA over every fill a button sits on.
        let fade = |c: Color32, under: Color32| over(c.gamma_multiply(DISABLED_ALPHA), under);
        for p in [&LIGHT, &DARK] {
            for under in [p.bg, p.surface, p.surface_2, p.selection(), if p.dark { DARK_DIALOG } else { LIGHT_INPUT }] {
                let fill = fade(button_fill(p), under);
                let label = fade(p.text, fill);
                assert!(contrast(label, fill) >= 5.0, "dark={} on {under:?}: {:.2}", p.dark, contrast(label, fill));
                assert!(contrast(label, fill) < contrast(p.text, button_fill(p)), "still quieter than enabled");
            }
        }
    }
}
