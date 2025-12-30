//! Thermal Monitor GUI for Lenovo IdeaPad
//!
//! Minimal thermal monitoring application using egui/eframe.
//! Displays CPU and estimated keyboard temperatures, allows mode control.

mod app;
mod system;

use app::ThermalApp;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([800.0, 600.0])
            .with_min_inner_size([800.0, 600.0])
            .with_title("Thermal Monitor"),
        ..Default::default()
    };

    eframe::run_native(
        "Thermal Monitor",
        options,
        Box::new(|cc| Ok(Box::new(ThermalApp::new(cc)))),
    )
}
