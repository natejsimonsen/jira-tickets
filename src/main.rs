#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
#![allow(dead_code)]

mod adf;
mod app;
mod auth;
mod cache;
mod config;
mod jira;
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
}
