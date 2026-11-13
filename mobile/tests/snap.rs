//! Capturas das telas do mobile (`cargo test -p flounder-mobile --test snap -- --ignored`).

use egui_kittest::Harness;
use flounder_mobile::app::{MobileApp, Tab};

fn shot(app: MobileApp, name: &str, dir: &std::path::Path) {
    let mut hs = Harness::builder()
        .with_size([390.0, 844.0])
        .with_pixels_per_point(2.0)
        .build_eframe(|cc| {
            flounder::widgets::install(&cc.egui_ctx);
            app
        });
    hs.run();
    hs.render().unwrap().save(dir.join(format!("{name}.png"))).unwrap();
}

#[test]
#[ignore]
fn capturas() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/screenshots");
    std::fs::create_dir_all(&dir).unwrap();
    shot(MobileApp::new(), "inicio", &dir);
    shot(MobileApp { tab: Tab::Fluxo, ..MobileApp::new() }, "fluxo", &dir);
    shot(MobileApp { tab: Tab::Cartoes, ..MobileApp::new() }, "cartoes", &dir);
    shot(MobileApp { tab: Tab::Casa, ..MobileApp::new() }, "casa", &dir);
    // Folha de novo cartão, preenchida para a captura.
    let mut app = MobileApp { tab: Tab::Cartoes, ..MobileApp::new() };
    app.sheet = Some(flounder_mobile::app::Sheet::Cartao(flounder_mobile::app::SheetCartao {
        banco: "Inter".into(),
        limite: "5.000,00".into(),
        fecha: "25".into(),
        venc: "10".into(),
        ..Default::default()
    }));
    shot(app, "cartao-novo", &dir);
}
