//! VERSIONE Studio — musician-facing creative library.
//!
//! Domain logic stays in `versione-core`. This crate only presents structured APIs.

mod app;
mod player;

use eframe::egui;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([960.0, 640.0])
            .with_title("VERSIONE"),
        ..Default::default()
    };
    eframe::run_native(
        "VERSIONE",
        options,
        Box::new(|cc| Ok(Box::new(app::VersioneApp::new(cc)))),
    )
}
