use crate::app::App;
use egui::Ui;

pub fn show(app: &mut App, ui: &mut Ui) {
    if let Some(k) = &app.selected {
        ui.label(k);
    }
}
