//! Flounder mobile — o mesmo motor do desktop (egui/eframe), em janela de celular.
//! Fase 1: interface com mock (mock.rs). Android nativo depois via cargo ndk.

use flounder_mobile::criar;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Flounder")
            .with_inner_size([390.0, 844.0])
            .with_min_inner_size([360.0, 640.0]),
        ..Default::default()
    };
    eframe::run_native("Flounder", options, Box::new(|cc| Ok(criar(cc))))
}
