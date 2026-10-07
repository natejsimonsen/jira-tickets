#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
#![allow(dead_code)]

mod auth;
mod config;

#[cfg(test)]
pub mod testutil {
    /// Tests that set process env vars lock this so they never interleave.
    pub static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Jira Tickets")
            .with_app_id("jira-tickets")
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([900.0, 560.0]),
        run_and_return: false,
        ..Default::default()
    };
    eframe::run_native("jira-tickets", options, Box::new(|_cc| Ok(Box::new(Hello))))
}

struct Hello;

impl eframe::App for Hello {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.label("Jira Tickets");
        });
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn builds() {}
}
