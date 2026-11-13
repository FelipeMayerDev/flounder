//! Capturas das telas do mobile (`cargo test -p flounder-mobile --test snap -- --ignored`).

use egui_kittest::Harness;
use flounder_mobile::app::{MobileApp, Tab};

fn shot(app: MobileApp, name: &str, dir: &std::path::Path) {
    let mut hs = Harness::builder()
        .with_size([390.0, 844.0])
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
}
