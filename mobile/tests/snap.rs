//! Capturas das telas do mobile (`cargo test -p flounder-mobile --test snap -- --ignored`).
//! As telas de conteúdo usam o modo demonstração; o Consultor usa análises falsas.

use egui_kittest::Harness;
use flounder::api::{AssistantReport, AssistantReports};
use flounder::model::Person;
use flounder_mobile::app::{Conexao, MobileApp, Tab};

fn shot(app: MobileApp, name: &str, dir: &std::path::Path) {
    // As animações de entrada deixam o primeiro quadro invisível no render de teste.
    unsafe { std::env::set_var("FLOUNDER_REDUCED_MOTION", "1"); }
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

fn demo() -> MobileApp {
    let mut app = MobileApp::load(None);
    app.demo = true;
    app.atualizar_mes();
    app
}

fn conectado() -> MobileApp {
    let mut app = MobileApp::load(None);
    app.sessao = Some(Conexao {
        me: Some("client-1".into()),
        token: "token-teste".into(),
        members: Vec::new(),
        loaded: true,
        worker: None,
        offline: None,
    });
    app.data.names = ["Felipe".into(), "Larissa".into()];
    app.data.me = Person::A;
    app.eu = 0;
    app.atualizar_mes();
    app
}

#[test]
#[ignore]
fn capturas() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/screenshots");
    std::fs::create_dir_all(&dir).unwrap();
    for (tab, nome) in [
        (Tab::Inicio, "inicio"),
        (Tab::Fluxo, "fluxo"),
        (Tab::Cartoes, "cartoes"),
        (Tab::Casa, "casa"),
    ] {
        shot(MobileApp { tab, ..demo() }, nome, &dir);
    }
    // Consultor com análises falsas (habilitado, as três prontas).
    let mut app = conectado();
    app.tab = Tab::Consultor;
    app.agent.reports = Some(AssistantReports {
        enabled: true,
        busy: false,
        reports: vec![
            AssistantReport {
                subject: 0,
                content: "Felipe, meu amigo, vou nadar reto: setembro piorou (R$ 6.907,96 contra R$ 5.142,78 de agosto), outubro melhorou (R$ 4.338,15), mas novembro já cravou R$ 6.335,92 em só 12 dias — pior começo do período 🐟".into(),
                generated_at: "2026-11-12T22:39:17Z".into(),
                period_start: "2026-08-01".into(),
                period_end: "2026-11-12".into(),
            },
            AssistantReport {
                subject: 2,
                content: "Bolha bolha, pessoal! 🐠 A maré tá bem revolta: entraram uns R$ 30 mil e saíram R$ 24 mil — o aquário tá cheio mas as contas também.".into(),
                generated_at: "2026-11-12T22:36:13Z".into(),
                period_start: "2026-08-01".into(),
                period_end: "2026-11-12".into(),
            },
        ],
    });
    app.agent.chat = vec![flounder::api::ChatMsg {
        role: "assistant".into(),
        content: "Oi! Sou o Flounder 🐠 Já li os lançamentos de vocês. Me pergunte qualquer coisa — do saldo do mês àquela conta que sempre esquece.".into(),
        ops: Vec::new(),
    }];
    shot(app, "consultor", &dir);
}
