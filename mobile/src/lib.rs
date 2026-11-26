//! Flounder mobile: casca egui para celular (fase 1 com mock; fase 2 na API do backend).

pub mod app;
pub mod converter;
pub mod mock;
pub mod screens;
pub mod theme;

/// Cria o app (compartilhado entre desktop e Android).
pub fn criar(cc: &eframe::CreationContext<'_>) -> Box<dyn eframe::App> {
    flounder::widgets::install(&cc.egui_ctx);
    Box::new(app::MobileApp::load(cc.storage))
}

/// Android: NativeActivity chama este símbolo no `.so` (crate-type cdylib).
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(android_app: winit::platform::android::activity::AndroidApp) {
    // unsafe: antes de qualquer thread nascer, no entry do NativeActivity.
    unsafe { std::env::set_var("RUST_BACKTRACE", "1"); }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_title("Flounder"),
        android_app: Some(android_app.clone()),
        ..Default::default()
    };
    eframe::run_native("Flounder", options, Box::new(move |cc| {
        flounder::widgets::install(&cc.egui_ctx);
        let mut app = app::MobileApp::load(cc.storage);
        app.android_app = Some(android_app);
        Ok(Box::new(app))
    })).expect("Flounder não abriu no Android");
}
