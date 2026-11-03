//! Round Jira user avatars. Images load on background threads, are cached on disk under the app cache dir
//! (`avatars/`), and show as initials on a tinted circle until they arrive or when they fail.
//!
//! Views call `avatar::show(ui, Some(&user.account_id), &user.display_name, 20.0)`. The store lives in the egui
//! context, so views need no plumbing; the App hands it the client once with `avatar::set_client`.

use crate::jira::Client;
use egui::{Color32, Rect, Sense, TextureHandle, pos2, vec2};
use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

/// Worker threads fetching avatars; a team page asks for a dozen at once.
const WORKERS: usize = 4;
/// Decoded images larger than this are scaled down; 48 px avatars at 2x fit.
const MAX_SIDE: u32 = 96;

/// Shared avatar store, kept in the egui context's temp data.
#[derive(Default)]
pub struct Avatars {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    client: Option<Arc<Client>>,
    slots: HashMap<String, Slot>,
    jobs: Option<Sender<Job>>,
    done: Option<(Sender<Done>, Receiver<Done>)>,
}

enum Slot {
    Loading,
    Ready(TextureHandle),
    Failed,
}

struct Job {
    account_id: String,
    client: Arc<Client>,
    ctx: egui::Context,
    done: Sender<Done>,
}

struct Done {
    account_id: String,
    image: Option<egui::ColorImage>,
}

fn handle(ctx: &egui::Context) -> Arc<Avatars> {
    ctx.data_mut(|d| d.get_temp_mut_or_default::<Arc<Avatars>>(egui::Id::new("avatars")).clone())
}

/// Gives the store the Jira client (call whenever the App's client changes). Without one, avatars show initials.
pub fn set_client(ctx: &egui::Context, client: Option<Arc<Client>>) {
    let store = handle(ctx);
    let mut inner = store.inner.lock().unwrap_or_else(|e| e.into_inner());
    // A new login may see avatars the old one could not, so failures get another try.
    inner.slots.retain(|_, s| !matches!(s, Slot::Failed));
    inner.client = client;
}

/// A round avatar `size` points wide with a tooltip of `name`. Shows initials while loading, on failure, or
/// without an account id.
pub fn show(ui: &mut egui::Ui, account_id: Option<&str>, name: &str, size: f32) -> egui::Response {
    inline(ui, account_id, name, size, size)
}

/// `show` beside text: a slot `size` wide and `line_h` tall (the text's line box, or a pill's height) with the
/// circle centred on it (reaching past it when taller), so the row and its text keep their place and the avatar
/// sits on the text's middle in a top-aligned row.
pub fn inline(ui: &mut egui::Ui, account_id: Option<&str>, name: &str, size: f32, line_h: f32) -> egui::Response {
    let (slot, resp) = ui.allocate_exact_size(vec2(size, line_h), Sense::hover());
    let rect = Rect::from_min_size((slot.center() - vec2(size, size) / 2.0).round(), vec2(size, size));
    if ui.is_rect_visible(rect) {
        let account_id = account_id.filter(|a| !a.is_empty());
        let texture = account_id.and_then(|a| handle(ui.ctx()).texture(ui.ctx(), a));
        let painter = ui.painter();
        if let Some(id) = texture {
            let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
            painter.add(egui::epaint::RectShape::filled(rect, size / 2.0, Color32::WHITE).with_texture(id, uv));
        } else {
            let (fill, ink) = colors(name, ui.visuals().dark_mode);
            painter.circle_filled(rect.center(), size / 2.0, fill);
            let galley = painter.layout_no_wrap(initials(name), egui::FontId::proportional((size * 0.42).max(7.0)), ink);
            // Centre the glyphs' ink, not the line box, so caps sit in the middle of the circle.
            let ink_rect = crate::theme::ink_rect(&galley);
            let at = if ink_rect.is_positive() { rect.center() - ink_rect.center().to_vec2() } else { rect.center() - galley.size() / 2.0 };
            painter.galley(at, galley, ink);
        }
    }
    resp.on_hover_text(name)
}

impl Avatars {
    /// The loaded texture for `account_id`, queueing a fetch the first time it is asked for.
    fn texture(&self, ctx: &egui::Context, account_id: &str) -> Option<egui::TextureId> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((_, rx)) = &inner.done {
            let finished: Vec<Done> = rx.try_iter().collect();
            for d in finished {
                let slot = match d.image {
                    Some(img) => Slot::Ready(ctx.load_texture(format!("avatar-{}", d.account_id), img, egui::TextureOptions::LINEAR)),
                    None => Slot::Failed,
                };
                inner.slots.insert(d.account_id, slot);
            }
        }
        match inner.slots.get(account_id) {
            Some(Slot::Ready(t)) => return Some(t.id()),
            Some(_) => return None,
            None => {}
        }
        let client = inner.client.clone()?;
        let done = inner.done.get_or_insert_with(channel).0.clone();
        let jobs = inner.jobs.get_or_insert_with(start_workers);
        let job = Job { account_id: account_id.to_string(), client, ctx: ctx.clone(), done };
        if jobs.send(job).is_ok() {
            inner.slots.insert(account_id.to_string(), Slot::Loading);
        }
        None
    }
}

/// Starts the fetch workers; they end when the store (the only job sender) is dropped.
fn start_workers() -> Sender<Job> {
    let (tx, rx) = channel::<Job>();
    let rx = Arc::new(Mutex::new(rx));
    for _ in 0..WORKERS {
        let rx = rx.clone();
        std::thread::spawn(move || {
            loop {
                let job = match rx.lock().unwrap_or_else(|e| e.into_inner()).recv() {
                    Ok(j) => j,
                    Err(_) => return,
                };
                let image = load(&job.client, &job.account_id);
                let _ = job.done.send(Done { account_id: job.account_id, image });
                job.ctx.request_repaint();
            }
        });
    }
    tx
}

/// Disk cache names: the image bytes and the URL they came from.
fn cache_keys(account_id: &str) -> (String, String) {
    (account_id.to_string(), format!("{account_id}.url"))
}

/// Whether the cached bytes can be used without a fetch: they exist and either no fresher URL is known or the
/// known URL is the one they came from.
fn use_disk(known_url: Option<&str>, cached_url: Option<&str>, have_bytes: bool) -> bool {
    have_bytes && known_url.is_none_or(|k| Some(k) == cached_url)
}

/// Blocking: the avatar for `account_id` from disk or Jira, decoded. Runs on a worker thread.
fn load(client: &Client, account_id: &str) -> Option<egui::ColorImage> {
    let (bytes_key, url_key) = cache_keys(account_id);
    let cached = crate::cache::load_bytes("avatars", &bytes_key).filter(|b| decode(b).is_some());
    let cached_url = crate::cache::load_bytes("avatars", &url_key).and_then(|b| String::from_utf8(b).ok());
    let known = crate::jira::known_avatar_url(account_id);
    if use_disk(known.as_deref(), cached_url.as_deref(), cached.is_some()) {
        return cached.as_deref().and_then(decode);
    }
    let fetched = known.or_else(|| client.user_avatar_url(account_id).ok().flatten()).and_then(|url| {
        let bytes = client.fetch_bytes(&url).ok()?;
        let img = decode(&bytes)?;
        crate::cache::store_bytes("avatars", &bytes_key, &bytes);
        crate::cache::store_bytes("avatars", &url_key, url.as_bytes());
        Some(img)
    });
    // A failed refresh still shows the old picture.
    fetched.or_else(|| cached.as_deref().and_then(decode))
}

fn decode(bytes: &[u8]) -> Option<egui::ColorImage> {
    let mut img = image::load_from_memory(bytes).ok()?;
    if img.width() > MAX_SIDE || img.height() > MAX_SIDE {
        img = img.thumbnail(MAX_SIDE, MAX_SIDE);
    }
    let rgba = img.to_rgba8();
    Some(egui::ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw()))
}

/// Up to two initials: the first letters of the first and last words. "?" when the name has no letters.
fn initials(name: &str) -> String {
    let firsts: Vec<char> = name.split_whitespace().filter_map(|w| w.chars().next().filter(|c| c.is_alphanumeric())).collect();
    let picked: Vec<char> = match firsts.as_slice() {
        [] => return "?".into(),
        [one] => vec![*one],
        [first, .., last] => vec![*first, *last],
    };
    picked.into_iter().flat_map(char::to_uppercase).collect()
}

/// Stable hue (0..360) from a name, so a person keeps their colour across runs.
fn hue(name: &str) -> f32 {
    let h = name.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3));
    (h % 360) as f32
}

/// (fill, text) for the initials circle: a muted tint of the name's hue with text that reads on it.
fn colors(name: &str, dark: bool) -> (Color32, Color32) {
    let h = hue(name);
    if dark { (hsl(h, 0.28, 0.32), hsl(h, 0.20, 0.93)) } else { (hsl(h, 0.40, 0.86), hsl(h, 0.45, 0.22)) }
}

fn hsl(h: f32, s: f32, l: f32) -> Color32 {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let (r, g, b) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    let u = |v: f32| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgb(u(r), u(g), u(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contrast(a: Color32, b: Color32) -> f32 {
        let lum = |c: Color32| {
            let ch = |v: u8| {
                let v = v as f32 / 255.0;
                if v <= 0.03928 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
            };
            0.2126 * ch(c.r()) + 0.7152 * ch(c.g()) + 0.0722 * ch(c.b())
        };
        let (x, y) = (lum(a), lum(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    #[test]
    fn initials_take_first_and_last_words() {
        assert_eq!(initials("Ada Lovelace"), "AL");
        assert_eq!(initials("grace brewster murray hopper"), "GH");
        assert_eq!(initials("Shawn"), "S");
        assert_eq!(initials("  "), "?");
        assert_eq!(initials("Mary-Jane Watson (Contractor)"), "MW");
        assert_eq!(initials("élodie ünal"), "ÉÜ");
    }

    #[test]
    fn colours_are_stable_muted_and_readable() {
        assert_eq!(colors("Ada Lovelace", true), colors("Ada Lovelace", true));
        assert_ne!(hue("Ada Lovelace"), hue("Grace Hopper"));
        for h in 0..360 {
            for (fill, ink) in [(hsl(h as f32, 0.28, 0.32), hsl(h as f32, 0.20, 0.93)), (hsl(h as f32, 0.40, 0.86), hsl(h as f32, 0.45, 0.22))] {
                assert!(contrast(fill, ink) >= 4.5, "hue {h}: {:.2}", contrast(fill, ink));
            }
        }
        assert_eq!(hsl(0.0, 1.0, 0.5), Color32::from_rgb(255, 0, 0));
        assert_eq!(hsl(240.0, 1.0, 0.5), Color32::from_rgb(0, 0, 255));
    }

    #[test]
    fn disk_cache_is_used_unless_a_newer_url_is_known() {
        assert!(use_disk(None, None, true));
        assert!(use_disk(Some("u1"), Some("u1"), true));
        assert!(!use_disk(Some("u2"), Some("u1"), true));
        assert!(!use_disk(None, None, false));
        assert_eq!(cache_keys("557058:a"), ("557058:a".to_string(), "557058:a.url".to_string()));
    }

    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut out = std::io::Cursor::new(Vec::new());
        image::RgbaImage::from_pixel(w, h, image::Rgba([10, 20, 30, 255])).write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test]
    fn decodes_and_shrinks_large_images() {
        assert_eq!(decode(&png(48, 48)).unwrap().size, [48, 48]);
        assert_eq!(decode(&png(512, 256)).unwrap().size, [96, 48]);
        assert!(decode(b"not an image").is_none());
    }

    fn client(base: &str) -> Client {
        Client::new(base, &crate::auth::Login { email: "me@x.com".into(), token: "tok".into(), source: crate::auth::Source::Env })
    }

    #[test]
    fn load_prefers_disk_and_falls_back_quietly() {
        let _g = crate::testutil::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var("JIRA_TICKETS_CACHE_DIR", dir.path()) };
        // No URL known and none on disk: asks Jira for the user, which has none, so it fails without caching.
        let (base, rx) =
            crate::jira::tests::stub_server(vec![("GET", "/rest/api/3/user", serde_json::json!({ "accountId": "x", "displayName": "X", "avatarUrls": {} }))]);
        assert!(load(&client(&base), "avatar-test-none").is_none());
        assert!(rx.recv().unwrap().path.starts_with("/rest/api/3/user?accountId=avatar-test-none"));
        // Cached bytes with no newer URL known: no network at all.
        crate::cache::store_bytes("avatars", "avatar-test-disk", &png(48, 48));
        assert_eq!(load(&client(&base), "avatar-test-disk").unwrap().size, [48, 48]);
        assert!(rx.try_recv().is_err());
        assert!(crate::cache::load_bytes("avatars", "avatar-test-none").is_none());
        unsafe { std::env::remove_var("JIRA_TICKETS_CACHE_DIR") };
    }

    #[test]
    fn a_queued_avatar_becomes_a_texture_once() {
        let _g = crate::testutil::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var("JIRA_TICKETS_CACHE_DIR", dir.path()) };
        crate::cache::store_bytes("avatars", "avatar-test-tex", &png(48, 48));
        let (base, rx) = crate::jira::tests::stub_server(vec![]);
        let ctx = egui::Context::default();
        let store = handle(&ctx);
        assert!(store.texture(&ctx, "avatar-test-tex").is_none(), "no client: initials, nothing queued");
        assert!(store.inner.lock().unwrap().slots.is_empty());
        set_client(&ctx, Some(Arc::new(client(&base))));
        let start = std::time::Instant::now();
        while store.texture(&ctx, "avatar-test-tex").is_none() {
            assert!(start.elapsed() < std::time::Duration::from_secs(5), "never loaded");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(store.inner.lock().unwrap().slots.len(), 1, "asked many times, queued once");
        assert!(rx.try_recv().is_err(), "served from disk");
        unsafe { std::env::remove_var("JIRA_TICKETS_CACHE_DIR") };
    }

    #[test]
    fn shows_initials_without_a_client() {
        let shapes = crate::testutil::paint_at2(vec2(200.0, 100.0), |ui| {
            show(ui, Some("someone"), "Ada Lovelace", 24.0);
        });
        let ink = crate::testutil::ink_of(&shapes, "AL");
        assert!(ink.width() < 24.0 && ink.height() < 24.0);
        // The ink sits in the middle of the 24 pt circle.
        assert!((ink.center().y - 12.0).abs() <= 1.0, "{ink:?}");
    }
}
