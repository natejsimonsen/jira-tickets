#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
#![allow(dead_code)]

mod adf;
mod app;
mod auth;
mod cache;
mod config;
mod filter;
mod jira;
mod keys;
#[cfg(feature = "snapshot")]
mod snapshot;
mod store;
mod theme;
mod views;

fn main() -> eframe::Result {
    let path = config::path();
    let cfg = match config::load(&path) {
        Ok(c) => c,
        Err(e) if !path.exists() => {
            if let Err(w) = config::write_example(&path) {
                eprintln!("Error writing example config: {w}");
                std::process::exit(1);
            }
            println!("Welcome to Jira Tickets!\n\nA config file has been created at:\n  {}\n", path.display());
            println!("Edit it: set site and email, add an api_token (plain, op://…, or $(cmd)) or export JIRA_API_TOKEN,");
            println!("then run jira-tickets again.\n\n  $EDITOR {}\n\n({e})", path.display());
            return Ok(());
        }
        Err(e) => {
            eprintln!("Error loading config: {e}\n\nConfig file: {}", path.display());
            std::process::exit(1);
        }
    };
    #[cfg(feature = "snapshot")]
    {
        let out = std::env::var("JIRA_TICKETS_SCREENSHOT").ok();
        if out.is_some() || std::env::var_os("JIRA_TICKETS_SCRIPT").is_some() {
            snapshot::run(out.as_deref(), cfg);
            return Ok(());
        }
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Jira Tickets")
            .with_app_id("jira-tickets")
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([900.0, 560.0]),
        run_and_return: false,
        ..Default::default()
    };
    eframe::run_native("jira-tickets", options, Box::new(|cc| Ok(Box::new(app::App::new(cc, cfg)))))
}

#[cfg(test)]
pub mod testutil {
    /// Tests that set process env vars lock this so they never interleave.
    pub static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Runs `f` in a pass of a headless context with the app's theme and fonts (they load on the pass after
    /// `theme::apply`, so one empty pass goes first).
    pub fn with_ui<R>(f: impl FnOnce(&mut egui::Ui) -> R) -> R {
        let ctx = egui::Context::default();
        crate::theme::apply(&ctx);
        let input = || egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1440.0, 900.0))), ..Default::default() };
        ctx.run_ui(input(), |_| {}).textures_delta.clear();
        let mut f = Some(f);
        let mut out = None;
        ctx.run_ui(input(), |ui| {
            if let Some(f) = f.take() {
                out = Some(f(ui));
            }
        })
        .textures_delta
        .clear();
        out.expect("one pass")
    }
}
