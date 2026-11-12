#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use flounder::{app, widgets};

fn main() -> eframe::Result {
    flounder::model::use_real_clock();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Flounder")
            .with_app_id("flounder")
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([1024.0, 700.0]),
        ..Default::default()
    };
    eframe::run_native("Flounder", options, Box::new(|cc| {
        widgets::install(&cc.egui_ctx);
        Ok(Box::new(app::App::load(cc.storage)))
    }))
}
