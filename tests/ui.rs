//! Testes de interface (sem GPU) e gerador de capturas (`cargo test --test ui -- --ignored`, precisa de GPU/Vulkan).

use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use flounder::app::App;
use flounder::model::*;

fn harness(app: App, w: f32, h: f32, ppp: f32) -> Harness<'static, App> {
    let mut hs = Harness::builder()
        .with_size([w, h])
        .with_pixels_per_point(ppp)
        .with_max_steps(400)
        .build_eframe(|cc| {
            flounder::widgets::install(&cc.egui_ctx);
            app
        });
    hs.run();
    hs
}

fn nov() -> Month {
    Month::of(today())
}

fn connected_card() -> flounder::connected::ConnectedCard {
    flounder::connected::ConnectedCard::from_pluggy("fixture-item", &serde_json::json!({"name":"Nubank"}), &serde_json::json!({
        "id":"fixture-card","type":"CREDIT","name":"Nubank Mastercard Platinum","number":"xxxx1234","currencyCode":"BRL",
        "balance":345.67,"creditData":{"brand":"Mastercard","creditLimit":5000,"availableCreditLimit":4000,"balanceDueDate":"2026-11-25"}
    }), Some(&serde_json::json!([{"dueDate":"2026-11-25","totalAmount":123.45,"totalAmountCurrencyCode":"BRL"}]))).unwrap()
}

fn connected_app() -> App {
    let mut app = App::new();
    app.connected_cards = Some(vec![connected_card()]);
    app.dashboard_layout.order = [5, 0, 1, 2, 3, 4, 6, 7, 8];
    app
}

#[test]
fn individual_mode_shows_only_my_connected_cards() {
    let mut other = connected_card();
    other.id = "other-card".into();
    other.institution = "Inter".into();
    other.owner = 1;
    for me in [Person::A, Person::B] {
        let mut app = connected_app();
        app.connected_cards.as_mut().unwrap().push(other.clone());
        app.data.me = me;
        app.mode = Mode::Couple;
        let mut hs = harness(app, 1440.0, 1100.0, 1.0);
        assert!(hs.query_by_label_contains("Ver cartão Nubank").is_some());
        assert!(hs.query_by_label_contains("Ver cartão Inter").is_some());
        hs.get_by_label("Individual").click();
        hs.run();
        assert_eq!(hs.query_by_label_contains("Ver cartão Nubank").is_some(), me == Person::A);
        assert_eq!(hs.query_by_label_contains("Ver cartão Inter").is_some(), me == Person::B);
    }
}

/// App real com uma thread falsa: as mensagens vêm deste canal, sem rede.
fn remote_app(storage: Option<&dyn eframe::Storage>) -> (App, std::sync::mpsc::Sender<flounder::backend::Msg>) {
    let mut app = App::load(storage);
    let (tx, _commands) = std::sync::mpsc::channel();
    let (replies, rx) = std::sync::mpsc::channel();
    app.remote.as_mut().unwrap().worker = Some(flounder::backend::Worker { client: "client-1".into(), tx, rx, pending: 0 });
    fake_update_worker(&mut app);
    (app, replies)
}

/// Worker de atualização falso: a thread de verdade nunca nasce (nem rede nos testes).
/// Devolve o canal para injetar `flounder::update::Msg`.
fn fake_update_worker(app: &mut App) -> std::sync::mpsc::Sender<flounder::update::Msg> {
    let (tx, _commands) = std::sync::mpsc::channel();
    let (replies, rx) = std::sync::mpsc::channel();
    app.update.worker = Some(flounder::update::Worker { tx, rx });
    replies
}

fn fake_state() -> flounder::api::State {
    use flounder::api::Member;
    flounder::api::State {
        me: 0,
        members: vec![
            Member { position: 0, name: "Ana".into(), client: "client-1".into() },
            Member { position: 1, name: "Bruno".into(), client: "client-2".into() },
        ],
        ..Default::default()
    }
}

#[derive(Default)]
struct MemoryStorage(std::collections::HashMap<String, String>);
impl eframe::Storage for MemoryStorage {
    fn get_string(&self, key: &str) -> Option<String> { self.0.get(key).cloned() }
    fn set_string(&mut self, key: &str, value: String) { self.0.insert(key.into(), value); }
    fn remove_string(&mut self, key: &str) { self.0.remove(key); }
    fn flush(&mut self) {}
}

#[test]
fn imported_card_bill_is_a_bill_on_the_calendar_and_can_be_marked_paid() {
    let mut storage = MemoryStorage::default();
    storage.0.insert("member_client".into(), "client-1".into());
    let (app, replies) = remote_app(Some(&storage));
    let mut state = fake_state();
    let card = |owner, due: &str| {
        let mut c = flounder::connected::ConnectedCard::from_pluggy("item", &serde_json::json!({"name":"Nubank"}),
            &serde_json::json!({"id": format!("card-{owner}"), "type":"CREDIT"}),
            Some(&serde_json::json!([{"dueDate": due, "totalAmount": 123.45, "totalAmountCurrencyCode":"BRL"}]))).unwrap();
        c.owner = owner;
        c
    };
    state.cards = vec![card(0, "2026-11-25"), card(1, "2026-11-20")];
    replies.send(flounder::backend::Msg::State(Box::new(state), vec![])).unwrap();
    let mut hs = harness(app, 1440.0, 1600.0, 1.0);
    // Só a fatura de quem usa o app vira conta a pagar.
    assert_eq!(hs.state().data.summary(nov(), Mode::Couple).bills.len(), 1);
    assert!(hs.query_all_by_label_contains("Fatura Nubank").count() >= 2, "contas a pagar e detalhe do dia");
    hs.state_mut().sel_day = 25;
    hs.run();
    hs.get_all_by_label("Marcar Fatura Nubank como paga").next().unwrap().click();
    hs.run();
    assert!(hs.state().data.connected_invoices.iter().find(|i| i.card == "card-0").unwrap().paid);
}

#[test]
fn person_picker_shows_until_someone_is_chosen_and_is_remembered() {
    use flounder::backend::Msg;
    let (app, replies) = remote_app(None);
    let mut hs = harness(app, 1024.0, 700.0, 1.0);
    assert!(hs.query_by_label_contains("Carregando").is_some());
    assert!(hs.query_by_label("Quem é você?").is_none());
    replies.send(Msg::Offline("Execute backend/run-local.sh e tente novamente.".into())).unwrap();
    hs.run();
    assert!(hs.query_by_label_contains("backend/run-local.sh").is_some());
    assert!(hs.query_by_label("Tentar novamente").is_some());
    replies.send(Msg::State(Box::new(fake_state()), vec![])).unwrap();
    hs.run();
    assert!(hs.query_by_label("Quem é você?").is_some());
    assert!(hs.query_by_label("Bruno").is_some());
    assert!(hs.query_by_label("Saldo previsto").is_none());
    hs.get_by_label("Ana").click();
    hs.run();
    assert!(hs.query_by_label("Quem é você?").is_none());
    assert!(hs.query_by_label("Saldo previsto").is_some());
    assert_eq!(hs.state().me(), "Ana");
    let mut storage = MemoryStorage::default();
    eframe::App::save(hs.state_mut(), &mut storage);
    assert_eq!(storage.0.get("member_client").map(String::as_str), Some("client-1"));
    let (app, replies) = remote_app(Some(&storage));
    replies.send(Msg::State(Box::new(fake_state()), vec![])).unwrap();
    let hs = harness(app, 1024.0, 700.0, 1.0);
    assert!(hs.query_by_label("Quem é você?").is_none());
    assert!(hs.query_by_label("Saldo previsto").is_some());
}

#[test]
#[ignore]
fn person_picker_screenshot() {
    let (app, replies) = remote_app(None);
    replies.send(flounder::backend::Msg::State(Box::new(fake_state()), vec![])).unwrap();
    let mut hs = harness(app, 1024.0, 700.0, 1.0);
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/screenshots");
    hs.render().unwrap().save(dir.join("estado-quem-e-voce.png")).unwrap();
}

#[test]
fn empty_real_data_renders_every_screen() {
    use flounder::app::Screen;
    let empty = || {
        let mut app = App::new();
        app.data = Data::empty();
        app.data.names = ["Ana".into(), "Bruno".into()];
        app.connected_cards = Some(vec![]);
        app
    };
    for mode in [Mode::Couple, Mode::Individual] {
        for screen in [Screen::Home, Screen::CashFlow, Screen::Cards, Screen::House, Screen::Reserve, Screen::Settings] {
            let hs = harness(App { mode, screen, ..empty() }, 1024.0, 700.0, 1.0);
            assert!(hs.query_by_label_contains("NaN").is_none() && hs.query_by_label_contains("inf%").is_none());
        }
        let hs = harness(App { mode, ..empty() }, 1024.0, 700.0, 1.0);
        assert!(hs.query_by_label("Saldo previsto").is_some());
        // Mês sem lançamentos: o painel completo aparece com os estados vazios de cada card.
        let hs = harness(App { mode, ..empty() }, 1440.0, 1600.0, 1.0);
        for label in ["Saldo previsto", "Nenhuma conta neste mês.", "Nenhum cartão conectado.", "Nenhuma meta de reserva definida."] {
            assert!(hs.query_by_label(label).is_some(), "Missing {label} in {mode:?}");
        }
        assert!(hs.query_by_label_contains("NaN").is_none());
        assert!(hs.query_by_label_contains("4821").is_none());
    }
}

#[test]
fn connected_card_replaces_demo_and_distinguishes_monthly_bill_from_balance() {
    for (width, scale) in [(1024.0, 1.0), (1440.0, 1.5), (1024.0, 2.0)] {
        let mut hs = harness(connected_app(), width, 1100.0, scale);
        let label = "Ver cartão Nubank · Mastercard · •••• 1234 · Fatura do mês: R$ 123,45 · limite livre R$ 4.000,00";
        assert!(hs.query_by_label(label).is_some(), "Missing card at {width}/{scale}");
        assert!(hs.query_by_label("Cartão Azul").is_none());
        assert!(hs.state().invoice.is_none());
        hs.state_mut().set_month(nov().add(1)); hs.run();
        assert!(hs.query_by_label_contains("Saldo utilizado: R$ 345,67").is_some());
        assert!(hs.query_by_label("Fatura do mês").is_none());
        hs.get_by_label_contains("Ver cartão Nubank").click(); hs.run();
        assert!(hs.state().connected_invoice.is_some());
        assert!(hs.query_by_label("Limite total: R$ 5.000,00").is_some());
        hs.get_by_label("Fechar").click(); hs.run();
        assert!(hs.state().connected_invoice.is_none());
    }
}

#[test]
#[ignore]
fn connected_card_screenshots() {
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/screenshots");
    std::fs::create_dir_all(&directory).unwrap();
    for (width, theme, name) in [(1024.0, flounder::widgets::Theme::Light, "cartao-conectado-1024"), (1440.0, flounder::widgets::Theme::Dark, "cartao-conectado-dark")] {
        let mut app = connected_app(); app.theme = theme;
        let mut hs = harness(app, width, 1100.0, 1.0);
        hs.render().unwrap().save(directory.join(format!("{name}.png"))).unwrap();
    }
}

#[test]
fn dashboard_shows_balance_calendar_and_evolution() {
    for mode in [Mode::Couple, Mode::Individual] {
        for m in [nov(), nov().add(1)] {
            let mut app = app_at(m);
            app.mode = mode;
            let balance = app.data.summary(m, mode).balance;
            for (w, h) in [(1440.0, 900.0), (1024.0, 700.0)] {
                let hs = harness(App { mode, ..app_at(m) }, w, h, 1.0);
                assert!(hs.query_all_by_label(&money(balance)).count() >= 1);
                for label in ["Saldo previsto", "Calendário de vencimentos", "Evolução do saldo previsto"] {
                    assert!(hs.query_by_label(label).is_some(), "Missing {label} at {w}");
                }
                assert!(hs.query_by_label("Proposta:").is_none());
            }
        }
    }
}

fn app_at(m: Month) -> App {
    let mut a = App::new();
    a.set_month(m);
    a
}

#[test]
fn month_navigation_and_empty_state() {
    let mut hs = harness(App::new(), 1440.0, 900.0, 1.0);
    hs.get_by_label("Próximo mês").click();
    hs.run();
    assert_eq!(hs.state().month, nov().add(1));
    hs.get_by_label("Próximo mês").click();
    hs.run();
    assert!(hs.query_by_label("Saldo previsto").is_some());
    hs.get_by_label("Hoje").click();
    hs.run();
    assert_eq!(hs.state().month, nov());
}

#[test]
fn mark_and_unmark_paid_updates_summary() {
    let mut hs = harness(App::new(), 1440.0, 900.0, 1.0);
    let before = hs.state().data.summary(nov(), Mode::Couple).paid_count;
    hs.get_by_label("Marcar Plano de saúde como paga").click();
    hs.run();
    assert_eq!(hs.state().data.summary(nov(), Mode::Couple).paid_count, before + 1);
    hs.get_by_label("Marcar Plano de saúde como paga").click();
    hs.run();
    assert_eq!(hs.state().data.summary(nov(), Mode::Couple).paid_count, before);
}

#[test]
fn add_validates_then_saves() {
    let mut hs = harness(App::new(), 1440.0, 900.0, 1.0);
    let n = hs.state().data.entries.len();
    let bal = hs.state().data.summary(nov(), Mode::Couple).balance;
    hs.get_by_label("Adicionar").click();
    hs.run();
    hs.get_by_label("Salvar").click();
    hs.run();
    assert!(hs.query_by_label_contains("Informe uma descrição").is_some());
    assert!(hs.query_by_label_contains("Informe o valor").is_some());
    assert_eq!(hs.state().data.entries.len(), n);
    let f = hs.state_mut().form.as_mut().unwrap();
    f.desc = "Pizza".into();
    f.amount = "-5".into();
    hs.run();
    hs.get_by_label("Salvar").click();
    hs.run();
    assert!(hs.query_by_label_contains("1.234,56").is_some());
    hs.state_mut().form.as_mut().unwrap().amount = "80,00".into();
    hs.run();
    hs.get_by_label("Salvar").click();
    hs.run();
    assert!(hs.state().form.is_none());
    assert_eq!(hs.state().data.entries.len(), n + 1);
    assert_eq!(hs.state().data.summary(nov(), Mode::Couple).balance, bal - 8_000);
}

#[test]
fn edit_entry_and_switch_mode() {
    let mut hs = harness(App::new(), 1440.0, 900.0, 1.0);
    hs.get_by_label("5 de novembro: 1 conta(s), paga").scroll_to_me();
    hs.run();
    hs.get_by_label("5 de novembro: 1 conta(s), paga").click();
    hs.run();
    assert_eq!(hs.state().sel_day, 5);
    hs.get_by_label_contains("Abrir Academia").scroll_to_me();
    hs.run();
    hs.get_by_label_contains("Abrir Academia").click();
    hs.run();
    assert_eq!(hs.state().form.as_ref().unwrap().amount, "129,90");
    hs.state_mut().form.as_mut().unwrap().amount = "200".into();
    hs.run();
    hs.get_by_label("Salvar").click();
    hs.run();
    assert!(hs.state().data.in_month(nov()).any(|e| e.desc == "Academia" && e.amount == 20_000));
    hs.get_by_label("Individual").click();
    hs.run();
    assert_eq!(hs.state().mode, Mode::Individual);
    assert!(hs.query_by_label("Distribuição do orçamento").is_some());
    assert!(hs.query_by_label("Casal").is_some());
}

#[test]
fn keyboard_shortcuts_and_tab_focus() {
    let mut hs = harness(App::new(), 1440.0, 900.0, 1.0);
    hs.key_press_modifiers(egui::Modifiers::ALT, egui::Key::ArrowRight);
    hs.run();
    assert_eq!(hs.state().month, nov().add(1));
    hs.key_press(egui::Key::Tab);
    hs.run();
    assert!(hs.ctx.memory(|m| m.focused().is_some()));
    hs.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::N);
    hs.run();
    assert!(hs.state().form.is_some());
}

/// Capturas da janela e da bolha do agente: `cargo test --test ui -- --ignored agent_screenshots`.
#[test]
#[ignore]
fn agent_screenshots() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/screenshots");
    std::fs::create_dir_all(&dir).unwrap();
    let (state, entries) = rich_state();
    let (mut hs, _replies, _commands) = signed_in(state, entries, 1440.0, 900.0);
    hs.state_mut().agent.open = true;
    hs.state_mut().agent.reports = Some(flounder::api::AssistantReports {
        enabled: true,
        busy: false,
        reports: vec![
            flounder::api::AssistantReport {
                subject: 0,
                content: "Setembro foi de R$ 5.200 e outubro subiu para R$ 6.100 — mais mercado e duas idas ao Tempero. 🐠\nA dica: marcar o mercado como conta fixa evita susto no fim do mês. Nada de pânico: a reserva cobre o mapa inteiro.".into(),
                generated_at: "2026-11-12T18:30:00.000Z".into(),
                period_start: "2026-08-01".into(),
                period_end: "2026-11-12".into(),
            },
            flounder::api::AssistantReport {
                subject: 2,
                content: "O casal gastou R$ 8.900 em outubro e a divisão da casa segue pela renda. Continuem guardando R$ 700 por mês que a meta chega no prazo. 🐠".into(),
                generated_at: "2026-11-12T18:30:00.000Z".into(),
                period_start: "2026-08-01".into(),
                period_end: "2026-11-12".into(),
            },
        ],
    });
    hs.state_mut().agent.chat = vec![
        flounder::api::ChatMsg { role: "user".into(), content: "Como está meu mês?".into(), ops: Vec::new() },
        flounder::api::ChatMsg {
            role: "assistant".into(),
            content: "Nadando bem! As compras de cartão somam R$ 1.870 e a fatura fecha em R$ 2.558 — nada fora d'água. 🐠".into(),
            ops: vec![flounder::api::Op { tool: "buscar_lancamentos".into(), label: "buscou lançamentos (12 encontrados)".into() }],
        },
    ];
    hs.run();
    hs.render().unwrap().save(dir.join("agente-janela.png")).unwrap();
    hs.state_mut().agent.open = false;
    hs.run();
    hs.render().unwrap().save(dir.join("agente-bolha.png")).unwrap();
}

/// Capturas do painel: `cargo test --release --test ui -- --ignored`.
#[test]
#[ignore]
fn screenshots() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/screenshots");
    std::fs::create_dir_all(&dir).unwrap();
    let shot = |name: &str, app: App, w: f32, h: f32, ppp: f32| {
        let mut hs = harness(app, w, h, ppp);
        hs.run();
        hs.render().unwrap().save(dir.join(format!("{name}.png"))).unwrap();
    };
    shot("painel-1440x900", App::new(), 1440.0, 900.0, 1.0);
    shot("painel-completo", App::new(), 1440.0, 1600.0, 1.0);
    shot("painel-1024x700", App::new(), 1024.0, 700.0, 1.0);
    shot("painel-individual", App { mode: Mode::Individual, ..App::new() }, 1440.0, 900.0, 1.0);
    shot("painel-negativo", app_at(nov().add(1)), 1440.0, 900.0, 1.0);
    shot("painel-1024x700@150", App::new(), 1024.0, 700.0, 1.5);
    shot("painel-1024x700@200", App::new(), 1024.0, 700.0, 2.0);
    shot("estado-mes-vazio", app_at(nov().add(2)), 1440.0, 900.0, 1.0);
    let mut f = App::new();
    f.add();
    f.form.as_mut().unwrap().tried = true;
    shot("estado-formulario-erros", f, 1440.0, 900.0, 1.0);
    let mut inv = App::new();
    inv.invoice = Some(0);
    shot("estado-fatura", inv, 1440.0, 900.0, 1.0);
    let mut long = app_at(nov().add(1));
    long.invoice = Some(0);
    shot("estado-texto-longo-1024x700", long, 1024.0, 700.0, 1.0);
}

/// Quadros da animação de entrada e troca de mês: `cargo test --release --test ui -- --ignored animation_frames`.
#[test]
#[ignore]
fn animation_frames() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/anim");
    std::fs::create_dir_all(&dir).unwrap();
    let mut hs = Harness::builder()
        .with_size([1440.0, 900.0])
        .with_step_dt(1.0 / 30.0)
        .with_max_steps(400)
        .build_eframe(|cc| {
            flounder::widgets::install(&cc.egui_ctx);
            App::new()
        });
    let mut n = 0;
    let mut grab = |hs: &mut Harness<'static, App>, frames: usize| {
        for _ in 0..frames {
            hs.step();
            hs.render().unwrap().save(dir.join(format!("{n:04}.png"))).unwrap();
            n += 1;
        }
    };
    grab(&mut hs, 36);
    hs.get_by_label("Marcar Plano de saúde como paga").click();
    grab(&mut hs, 18);
    hs.state_mut().set_month(nov().add(1));
    grab(&mut hs, 36);
}

#[test]
fn settings_switch_the_entire_theme_without_changing_data() {
    use flounder::{app::Screen, widgets::{palette, Theme}};
    let mut hs = harness(App { screen: Screen::Settings, ..App::new() }, 1024.0, 700.0, 1.0);
    let before = hs.state().data.summary(nov(), Mode::Couple).balance;
    for (label, theme) in [("Escuro", Theme::Dark), ("Pink", Theme::Pink), ("Claro", Theme::Light)] {
        hs.get_by_label(label).click();
        hs.run();
        assert_eq!(hs.state().theme, theme);
        let colors = palette(&hs.ctx);
        assert_eq!(hs.ctx.global_style().visuals.panel_fill, colors.bg);
        assert_eq!(hs.ctx.global_style().visuals.window_fill, colors.card);
        assert_eq!(hs.ctx.global_style().visuals.dark_mode, theme == Theme::Dark);
        assert_eq!(hs.state().data.summary(nov(), Mode::Couple).balance, before);
        hs.state_mut().add();
        hs.run();
        assert!(hs.query_by_label("Novo lançamento").is_some());
        hs.get_by_label("Cancelar").click();
        hs.run();
    }
}

#[test]
fn preferences_survive_restart_and_unknown_values_fall_back() {
    use flounder::widgets::Theme;
    let mut storage = MemoryStorage::default();
    assert_eq!(App::load(None).theme, Theme::Light);
    assert_eq!(App::load(Some(&storage)).theme, Theme::Light);
    for theme in [Theme::Dark, Theme::Pink, Theme::Light] {
        let mut app = App { theme, ..App::new() };
        eframe::App::save(&mut app, &mut storage);
        assert_eq!(App::load(Some(&storage)).theme, theme);
    }
    let mut app = App::new();
    app.dashboard_layout.order.swap(0, 2);
    app.dashboard_layout.spans[2] = 10;
    app.dashboard_layout.heights[2] = 704.0;
    eframe::App::save(&mut app, &mut storage);
    assert_eq!(App::load(Some(&storage)).dashboard_layout, app.dashboard_layout);
    storage.0.insert("dashboard_layout".into(), "invalid".into());
    assert_eq!(App::load(Some(&storage)).dashboard_layout, App::new().dashboard_layout);
    storage.0.insert("theme".into(), "unknown".into());
    assert_eq!(App::load(Some(&storage)).theme, Theme::Light);
}

#[test]
fn typography_and_content_fit_at_supported_display_scales() {
    use flounder::widgets::{money_job, palette, Theme};
    for theme in [Theme::Light, Theme::Dark, Theme::Pink] {
        for scale in [1.0, 1.5, 2.0] {
            let mut hs = harness(App { theme, ..App::new() }, 1024.0, 700.0, scale);
            assert!((hs.ctx.pixels_per_point() - scale).abs() < 0.01);
            let style = hs.ctx.global_style();
            assert_eq!(style.text_styles[&egui::TextStyle::Body].size, 14.0);
            assert_eq!(style.text_styles[&egui::TextStyle::Small].size, 12.0);
            let money = money_job(&hs.ctx, 123456, 24.0, palette(&hs.ctx).text);
            assert!(money.sections.iter().all(|s| s.format.font_id.size >= 12.0));
            hs.get_by_label("Evolução do saldo previsto").scroll_to_me();
            hs.run();
            // Ignore intentionally scrolled-off content; visible text must fit horizontally.
            fn check(shape: &egui::Shape, clip: egui::Rect) {
                match shape {
                    egui::Shape::Text(t) if !t.galley.job.text.is_empty() => {
                        let rect = t.galley.rect.translate(t.pos.to_vec2());
                        if rect.intersects(clip) && rect.left() >= 256.0 {
                            assert!(rect.right() <= 1000.5, "Text exceeds page margin: {} at {rect:?}", t.galley.job.text);
                        }
                    }
                    egui::Shape::Vec(shapes) => for shape in shapes { check(shape, clip); },
                    _ => {}
                }
            }
            for shape in &hs.output().shapes { check(&shape.shape, shape.clip_rect); }
        }
    }
}

#[test]
#[ignore]
fn theme_screenshots() {
    use flounder::{app::Screen, widgets::Theme};
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/screenshots");
    for theme in [Theme::Light, Theme::Dark, Theme::Pink] {
        for (screen, name) in [(Screen::Home, "home"), (Screen::Settings, "settings")] {
            let mut hs = harness(App { theme, screen, ..App::new() }, 1024.0, 700.0, 1.0);
            hs.render().unwrap().save(dir.join(format!("theme-{}-{name}.png", theme.key()))).unwrap();
        }
    }
}

#[test]
fn header_controls_share_the_same_vertical_center() {
    for (width, height, scale) in [(1024.0, 700.0, 1.0), (1440.0, 900.0, 2.0)] {
        let hs = harness(App::new(), width, height, scale);
        let center = hs.get_by_label("Adicionar").rect().center().y;
        for label in ["Mês anterior", "Próximo mês", "Novembro 2026", "Casal", "Individual"] {
            let rect = hs.get_by_label(label).rect();
            assert!((rect.center().y - center).abs() <= 0.5, "{label}: {rect:?}, button center: {center}");
        }
    }
}

fn drag(hs: &mut Harness<'_, App>, from: egui::Pos2, to: egui::Pos2) {
    hs.event(egui::Event::PointerMoved(from));
    hs.event(egui::Event::PointerButton { pos: from, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
    hs.step();
    hs.event(egui::Event::PointerMoved(from + egui::vec2(10.0, 0.0)));
    hs.step();
    hs.event(egui::Event::PointerMoved(to));
    hs.step();
    hs.event(egui::Event::PointerButton { pos: to, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE });
    hs.run();
}

#[test]
fn dashboard_cards_can_be_dragged_resized_and_controlled_by_keyboard() {
    let mut hs = harness(App::new(), 1440.0, 1600.0, 1.0);
    let balance = hs.state().data.summary(nov(), Mode::Couple).balance;
    let handle = hs.get_by_label("Mover Saldo previsto").rect();
    let corner = hs.get_by_label("Redimensionar Saldo previsto").rect();
    assert!((handle.right() + 8.0 - corner.right()).abs() < 0.5);
    let source = hs.get_by_label("Mover Saldo previsto").rect().center();
    let target = hs.get_by_label("Mover Contas a pagar").rect().center();
    drag(&mut hs, source, target);
    assert_eq!(hs.state().dashboard_layout.order[..2], [1, 0]);
    let from = hs.get_by_label("Redimensionar Saldo previsto").rect().center();
    let height = hs.state().dashboard_layout.heights[0];
    drag(&mut hs, from, from + egui::vec2(-120.0, 80.0));
    assert!(hs.state().dashboard_layout.spans[0] < 8);
    assert!(hs.state().dashboard_layout.heights[0] > height);
    hs.get_by_label("Mover Saldo previsto").focus();
    hs.key_press(egui::Key::ArrowLeft);
    hs.run();
    assert_eq!(hs.state().dashboard_layout.order[..2], [0, 1]);
    let height = hs.state().dashboard_layout.heights[0];
    hs.get_by_label("Redimensionar Saldo previsto").focus();
    hs.key_press(egui::Key::ArrowDown);
    hs.run();
    assert!(hs.state().dashboard_layout.heights[0] > height);
    assert_eq!(hs.state().data.summary(nov(), Mode::Couple).balance, balance);
}

fn animation_harness() -> Harness<'static, App> {
    let mut hs = Harness::builder().with_size([1440.0, 1600.0])
        .with_step_dt(1.0 / 60.0).with_max_steps(400).build_eframe(|cc| {
            flounder::widgets::install(&cc.egui_ctx);
            App::new()
        });
    hs.run();
    hs
}

#[test]
fn dashboard_drop_animates_before_settling_on_grid() {
    let mut hs = animation_harness();
    hs.ctx.global_style_mut(|s| s.animation_time = 0.18);
    let before = hs.get_by_label("Mover Contas a pagar").rect().center();
    let from = hs.get_by_label("Mover Saldo previsto").rect().center();
    let to = hs.get_by_label("Mover Calendário de vencimentos").rect().center();
    hs.event(egui::Event::PointerMoved(from));
    hs.event(egui::Event::PointerButton { pos: from, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
    hs.step();
    hs.event(egui::Event::PointerMoved(from + egui::vec2(-10.0, 0.0)));
    hs.step();
    hs.event(egui::Event::PointerMoved(to));
    hs.step();
    assert!(hs.output().shapes.iter().any(|s| matches!(s.shape, egui::Shape::Mesh(_))));
    hs.event(egui::Event::PointerButton { pos: to, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE });
    hs.step();
    hs.run_steps(5);
    let during = hs.get_by_label("Mover Contas a pagar").rect().center();
    hs.run();
    let after = hs.get_by_label("Mover Contas a pagar").rect().center();
    assert!(during.distance(before) > 1.0);
    assert!(during.distance(after) > 1.0);
    assert_eq!(hs.state().dashboard_layout.order[..3], [1, 2, 0]);
}

#[test]
#[ignore]
fn dashboard_drag_screenshots() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/screenshots");
    let mut hs = animation_harness();
    let from = hs.get_by_label("Mover Saldo previsto").rect().center();
    let to = hs.get_by_label("Mover Calendário de vencimentos").rect().center();
    hs.event(egui::Event::PointerMoved(from));
    hs.event(egui::Event::PointerButton { pos: from, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
    hs.step();
    hs.event(egui::Event::PointerMoved(from + egui::vec2(-10.0, 0.0)));
    hs.step();
    hs.event(egui::Event::PointerMoved(to));
    hs.run_steps(5);
    hs.render().unwrap().save(dir.join("painel-arrastando.png")).unwrap();
    hs.run_steps(10);
    hs.render().unwrap().save(dir.join("painel-balanco.png")).unwrap();
    hs.event(egui::Event::PointerButton { pos: to, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE });
    hs.step();
    hs.run_steps(5);
    hs.render().unwrap().save(dir.join("painel-reorganizando.png")).unwrap();
    hs.run();
    hs.render().unwrap().save(dir.join("painel-reorganizado.png")).unwrap();
}

#[test]
fn compact_cards_allow_three_columns_without_a_blank_header_row() {
    let mut app = App::new();
    app.dashboard_layout.order = [0, 2, 3, 4, 1, 6, 5, 7, 8];
    app.dashboard_layout.spans = [4; 9];
    let hs = harness(app, 1256.0, 1200.0, 1.0);
    let centers: Vec<_> = ["Saldo previsto", "Calendário de vencimentos", "Evolução do saldo previsto"]
        .iter().map(|label| hs.get_by_label(&format!("Mover {label}")).rect().center()).collect();
    assert!(centers.windows(2).all(|pair| (pair[0].y - pair[1].y).abs() < 0.5 && pair[0].x < pair[1].x));
    let title = hs.get_by_label("Saldo previsto").rect();
    let handle = hs.get_by_label("Mover Saldo previsto").rect();
    assert!(title.top() - handle.top() <= 32.0);
    let top = hs.get_by_label("Mover Divisão da casa").rect().top() - 8.0;
    let bottom = hs.get_by_label("Redimensionar Divisão da casa").rect().bottom();
    assert!(bottom - top < 400.0, "Card has unnecessary empty height: {}", bottom - top);
}

#[test]
#[ignore]
fn three_column_screenshots() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/screenshots");
    let mut app = App::new();
    app.dashboard_layout.order = [0, 2, 3, 4, 1, 6, 5, 7, 8];
    app.dashboard_layout.spans = [4; 9];
    let mut hs = harness(app, 1256.0, 1400.0, 1.0);
    hs.render().unwrap().save(dir.join("painel-tres-colunas.png")).unwrap();
}

// ───────────── Fluxo de caixa, Cartões, exclusão e atualização (backend falso) ─────────────

use flounder::backend::{Cmd, Msg};

fn pluggy_card(id: &str, bank: &str, owner: u8, last: &str, limit: f64, available: f64, bills: &[(&str, f64)]) -> flounder::connected::ConnectedCard {
    let bills: Vec<_> = bills.iter().map(|(due, total)| serde_json::json!({"dueDate": due, "totalAmount": total, "totalAmountCurrencyCode": "BRL"})).collect();
    let mut c = flounder::connected::ConnectedCard::from_pluggy(&format!("item-{id}"), &serde_json::json!({"name": bank}), &serde_json::json!({
        "id": id, "type": "CREDIT", "name": format!("{bank} Mastercard"), "number": format!("xxxx{last}"), "currencyCode": "BRL",
        "balance": limit - available, "creditData": {"brand": "Mastercard", "creditLimit": limit, "availableCreditLimit": available, "balanceDueDate": bills[0]["dueDate"]}
    }), Some(&serde_json::Value::Array(bills))).unwrap();
    c.owner = owner;
    for b in &mut c.bills {
        b.paid = b.due.as_str() < "2026-11";
    }
    c
}

/// Domicílio fictício (Ana e Bruno): demonstração + compras e movimentos importados, cartões e contas.
/// Marca um checkbox da interface (clica pelo AccessKit até o estado mudar).
fn checkbox(hs: &mut Harness<'static, App>, label: &str) {
    let before = |hs: &Harness<'static, App>| match label {
        "Remover renda compartilhada" => hs.state().data.shared_split_off,
        _ => hs.state().form.as_ref().map(|f| f.parcelar).unwrap_or(false),
    };
    let initial = before(hs);
    for _ in 0..3 {
        hs.get_by_label(label).click_accesskit();
        hs.run();
        if before(hs) != initial {
            return;
        }
    }
    panic!("o checkbox \"{label}\" não mudou de estado");
}

fn rich_state() -> (flounder::api::State, Vec<Entry>) {
    use flounder::api::Account;
    let mut state = fake_state();
    state.cards = vec![
        pluggy_card("card-ana", "Nubank", 0, "1234", 8000.0, 5240.5, &[("2026-11-25", 2310.4), ("2026-10-25", 1985.1), ("2026-09-25", 2104.0)]),
        pluggy_card("card-bruno", "Banco Inter", 1, "9876", 5000.0, 4120.0, &[("2026-11-20", 880.0), ("2026-10-20", 640.3)]),
    ];
    let account = |owner, name: &str, institution: &str, currency: &str, cents| Account {
        id: format!("acc-{name}"), item_id: "item".into(), owner, name: name.into(), institution: institution.into(),
        color: [0, 0, 0], currency: currency.into(), balance: Some(cents), updated_at: None,
    };
    state.accounts = vec![
        account(0, "Conta corrente", "Nubank", "BRL", 523_410),
        account(0, "Poupança", "Nubank", "BRL", 1_200_000),
        account(1, "Conta corrente", "Banco Inter", "BRL", 318_090),
        account(0, "Conta global", "Wise", "USD", 12_000),
    ];
    let cards = ["card-ana", "card-bruno"];
    let mut entries: Vec<Entry> = Data::demo().entries.into_iter().map(|e| match e.card {
        Some(c) => Entry { card: None, connected: Some(cards[c].into()), imported: true, paid: true, ..e },
        None => e,
    }).collect();
    let nov = nov();
    let mut add = |desc: &str, cents, month: Month, day, kind, category, owner, card: Option<&str>| {
        let id = 900 + entries.len() as u32;
        entries.push(Entry { id, desc: desc.into(), amount: cents, date: month.day(day), kind, category, owner, shared: false,
            card: None, connected: card.map(Into::into), imported: true, paid: true, recurring: false });
    };
    use Category as C;
    use Kind::{Expense, Income};
    use Person::{A, B};
    add("Supermercado Bom Preço", 48_730, nov, 3, Expense, C::Groceries, A, Some("card-ana"));
    add("Notebook (2/10)", 42_990, nov, 6, Expense, C::Other, A, Some("card-ana"));
    add("Notebook (1/10)", 42_990, nov.add(-1), 6, Expense, C::Other, A, Some("card-ana"));
    add("Posto Avenida", 21_500, nov, 8, Expense, C::Transport, A, Some("card-ana"));
    add("Livraria Centro", 9_990, nov, 2, Expense, C::Other, B, Some("card-bruno"));
    add("Geladeira Frost (4/6)", 51_650, nov, 4, Expense, C::Shopping, B, Some("card-bruno"));
    add("PIX recebido — projeto extra", 150_000, nov, 7, Income, C::Salary, A, None);
    add("Conta de água", 9_870, nov, 9, Expense, C::Utilities, A, None);
    add("Tarifa bancária", 2_990, nov, 5, Expense, C::Other, B, None);
    (state, entries)
}

/// Backend falso com quem usa o app já escolhido (client-1); devolve também a fila de comandos.
fn signed_in(state: flounder::api::State, entries: Vec<Entry>, w: f32, h: f32) -> (Harness<'static, App>, std::sync::mpsc::Sender<Msg>, std::sync::mpsc::Receiver<Cmd>) {
    let mut storage = MemoryStorage::default();
    storage.0.insert("member_client".into(), "client-1".into());
    let mut app = App::load(Some(&storage));
    let (tx, commands) = std::sync::mpsc::channel();
    let (replies, rx) = std::sync::mpsc::channel();
    app.remote.as_mut().unwrap().worker = Some(flounder::backend::Worker { client: "client-1".into(), tx, rx, pending: 0 });
    fake_update_worker(&mut app);
    replies.send(Msg::State(Box::new(state), entries)).unwrap();
    (harness(app, w, h, 1.0), replies, commands)
}

#[test]
fn cards_screen_tracks_installments_and_future_commitments() {
    use flounder::app::Screen;
    let (state, entries) = rich_state();
    let (mut hs, _replies, _commands) = signed_in(state, entries, 1440.0, 2400.0);
    hs.state_mut().mode = Mode::Couple;
    hs.state_mut().screen = Screen::Cards;
    hs.run();
    assert!(hs.query_by_label("Parcelas em andamento").is_some());
    assert!(hs.query_by_label_contains("Notebook: parcela 2/10 · faltam 8 · até Jul 27").is_some());
    assert!(hs.query_by_label_contains("Geladeira Frost: parcela 4/6 · faltam 2").is_some());
    // Individual: só as parcelas de quem usa o app.
    hs.state_mut().mode = Mode::Individual;
    hs.run();
    assert!(hs.query_by_label_contains("Geladeira Frost: parcela").is_none());
    assert!(hs.query_by_label_contains("Notebook: parcela 2/10").is_some());
}

#[test]
fn sync_runs_once_after_first_load_and_on_demand() {
    let (state, entries) = rich_state();
    let (mut hs, replies, commands) = signed_in(state.clone(), entries.clone(), 1440.0, 900.0);
    let syncs = |rx: &std::sync::mpsc::Receiver<Cmd>| rx.try_iter().filter(|c| matches!(c, Cmd::Sync)).count();
    assert_eq!(syncs(&commands), 1, "uma atualização automática na abertura");
    assert!(hs.query_by_label("Atualizando contas").is_some());
    replies.send(Msg::Synced(Ok(flounder::api::SyncResult { synced: 2, failed: 0 }))).unwrap();
    replies.send(Msg::State(Box::new(state.clone()), entries.clone())).unwrap();
    hs.run();
    assert!(hs.query_by_label("2 conexões atualizadas").is_some());
    assert_eq!(syncs(&commands), 0, "a recarga seguinte não atualiza de novo");
    hs.get_by_label("Atualizar contas").click();
    hs.run();
    assert_eq!(syncs(&commands), 1);
    assert!(hs.query_by_label("2 conexões atualizadas").is_none());
    replies.send(Msg::Synced(Ok(flounder::api::SyncResult { synced: 1, failed: 1 }))).unwrap();
    hs.run();
    assert!(hs.query_by_label_contains("1 com falha").is_some());
    hs.get_by_label("Atualizar contas").click();
    hs.run();
    replies.send(Msg::Synced(Err("O backend respondeu com erro 502.".into()))).unwrap();
    hs.run();
    assert!(hs.query_by_label_contains("Não foi possível atualizar. O backend respondeu com erro 502.").is_some());
}

#[test]
fn deleting_an_entry_confirms_removes_the_row_and_sends_the_command() {
    let (state, entries) = rich_state();
    let water = entries.iter().find(|e| e.desc == "Conta de água").unwrap().clone();
    let (mut hs, replies, commands) = signed_in(state.clone(), entries.clone(), 1440.0, 900.0);
    hs.state_mut().screen = flounder::app::Screen::CashFlow;
    hs.run();
    hs.get_by_label_contains("Abrir Conta de água").scroll_to_me();
    hs.run();
    hs.get_by_label_contains("Abrir Conta de água").click();
    hs.run();
    assert!(hs.query_by_label("Importado do banco.").is_some());
    hs.get_by_label("Excluir").click();
    hs.run();
    assert!(hs.query_by_label_contains("não volta na próxima importação").is_some());
    assert!(hs.state().data.entry(water.id).is_some(), "só exclui depois de confirmar");
    hs.get_by_label("Excluir definitivamente").click();
    hs.run();
    assert!(hs.state().form.is_none());
    assert!(hs.state().data.entry(water.id).is_none());
    assert!(hs.query_by_label_contains("Abrir Conta de água").is_none());
    assert!(commands.try_iter().any(|c| matches!(c, Cmd::Delete(id) if id == water.id)));
    // Falha: aviso, e a recarga traz o lançamento de volta.
    replies.send(Msg::Done(Some("Não foi possível excluir o lançamento. O backend respondeu com erro 500. Os dados foram recarregados.".into()))).unwrap();
    replies.send(Msg::State(Box::new(state), entries)).unwrap();
    hs.run();
    assert!(hs.query_by_label_contains("Não foi possível excluir o lançamento").is_some());
    assert!(hs.query_by_label_contains("Abrir Conta de água").is_some());
    // Lançamento manual: confirmação sem o aviso de importação; "Voltar" cancela.
    hs.get_by_label_contains("Abrir Aluguel").scroll_to_me();
    hs.run();
    hs.get_by_label_contains("Abrir Aluguel").click();
    hs.run();
    hs.get_by_label("Excluir").click();
    hs.run();
    assert!(hs.query_by_label_contains("Não dá para desfazer").is_some());
    hs.get_by_label("Voltar").click();
    hs.run();
    assert!(hs.query_by_label("Salvar").is_some());
}

#[test]
fn cash_flow_lists_filters_and_copies_previous_month() {
    use flounder::app::Screen;
    let (state, entries) = rich_state();
    let (mut hs, _replies, commands) = signed_in(state, entries, 1440.0, 1600.0);
    hs.state_mut().mode = Mode::Couple;
    hs.state_mut().screen = Screen::CashFlow;
    hs.run();
    assert!(hs.query_by_label("Lançamentos de novembro").is_some());
    assert!(hs.query_by_label("Terça, 10 de novembro").is_some());
    // Casal: os dois, com quem é e a origem.
    assert_eq!(hs.query_all_by_label_contains("Abrir Salário").count(), 2);
    assert!(hs.query_by_label_contains("Abrir Supermercado Bom Preço: −R$ 487,30 · Mercado · Ana · Cartão · Na fatura").is_some());
    assert!(hs.query_by_label_contains("Abrir PIX recebido — projeto extra: +R$ 1.500,00 · Renda · Ana · Banco · Recebido").is_some());
    assert!(hs.query_by_label_contains("Saldo em conta").is_some());
    assert!(hs.query_by_label("Ana R$ 17.234,10").is_some(), "soma das contas em reais de Ana");
    assert!(hs.query_by_label_contains("USD 120,00").is_some(), "outra moeda à parte, sem conversão");
    hs.get_by_role_and_label(egui::accesskit::Role::RadioButton, "Entradas").click();
    hs.run();
    assert!(hs.query_by_label_contains("Abrir Aluguel").is_none());
    assert!(hs.query_by_label_contains("Abrir PIX recebido").is_some());
    hs.get_by_role_and_label(egui::accesskit::Role::RadioButton, "Todos").click();
    hs.get_by_role_and_label(egui::accesskit::Role::RadioButton, "Banco").click();
    hs.run();
    assert!(hs.query_by_label_contains("Abrir Supermercado").is_none());
    assert!(hs.query_by_label_contains("Abrir Conta de água").is_some());
    hs.get_by_role_and_label(egui::accesskit::Role::RadioButton, "Todas").click();
    hs.run();
    hs.state_mut().flow.category = Some(Category::Groceries);
    hs.state_mut().flow.search = "BOM".into();
    hs.run();
    assert_eq!(hs.query_all_by_label_contains("Abrir ").count(), 1);
    hs.state_mut().flow.search = "nada disso".into();
    hs.run();
    hs.get_by_label("Limpar filtros").click();
    hs.run();
    assert!(hs.query_by_label_contains("Abrir Aluguel").is_some());
    // Individual: só os de Ana.
    hs.get_by_label("Individual").click();
    hs.run();
    assert_eq!(hs.query_all_by_label_contains("Abrir Salário").count(), 1);
    assert!(hs.query_by_label_contains("Abrir Tarifa bancária").is_none());
    assert!(hs.query_by_label_contains("Bruno R$").is_none());
    // Mês vazio: estado vazio e cópia do mês anterior (só manuais sem cartão, uma vez).
    hs.get_by_label("Casal").click();
    // Janeiro tem a última parcela da viagem com os amigos; fevereiro é que está vazio.
    hs.state_mut().set_month(nov().add(3));
    hs.run();
    assert!(hs.query_by_label("Nenhum lançamento em fevereiro.").is_some());
    assert!(hs.query_by_label("Adicionar lançamento").is_some());
    let jan = nov().add(2);
    let expected = hs.state().data.in_month(jan).filter(|e| Origin::of(e) == Origin::Manual).count();
    let _ = commands.try_iter().count();
    hs.get_by_label("Copiar do mês anterior").click();
    hs.run();
    let note = if expected == 1 { "1 lançamento copiado de janeiro.".into() } else { format!("{expected} lançamentos copiados de janeiro.") };
    assert!(hs.query_by_label(&note).is_some());
    assert_eq!(commands.try_iter().filter(|c| matches!(c, Cmd::Entry(e) if e.id >= 1 << 31 && !e.paid)).count(), expected);
    assert!(hs.state().data.in_month(nov().add(3)).all(|e| Origin::of(e) == Origin::Manual && !e.paid));
    hs.get_by_label("Copiar do mês anterior").click();
    hs.run();
    assert!(hs.query_by_label_contains("Nada a copiar de janeiro").is_some());
    hs.get_all_by_label_contains("Abrir Viagem com os amigos").next().unwrap().click();
    hs.run();
    assert!(hs.query_by_label("Editar lançamento").is_some());
}

#[test]
fn cards_screen_shows_limits_invoices_and_purchases_per_owner() {
    use flounder::app::Screen;
    let (state, entries) = rich_state();
    for (width, mode) in [(1440.0, Mode::Couple), (1024.0, Mode::Couple), (1024.0, Mode::Individual)] {
        let (mut hs, _replies, commands) = signed_in(state.clone(), entries.clone(), width, 2400.0);
        hs.state_mut().screen = Screen::Cards;
        hs.state_mut().mode = mode;
        hs.run();
        assert!(hs.query_by_label_contains("Ver cartão Nubank").is_some());
        assert_eq!(hs.query_by_label_contains("Ver cartão Inter").is_some(), mode == Mode::Couple, "{width} {mode:?}");
        assert!(hs.query_by_label("R$ 2.759,50").is_some(), "limite usado do Nubank");
        assert!(hs.query_by_label_contains("Abrir Notebook (2/10)").is_some());
        assert!(hs.query_by_label_contains("Abrir Notebook (1/10)").is_none(), "só as compras do mês");
        assert!(hs.query_by_label_contains("fatura que vence em novembro: R$ 2.310,40").is_some());
        assert!(hs.query_by_label_contains("Abrir Conta de água").is_none(), "movimento do banco não é compra do cartão");
        // Marcar a fatura como paga usa o fluxo de faturas importadas.
        let _ = commands.try_iter().count();
        hs.get_by_label("Marcar Fatura Nubank · Nov 26 como paga").click();
        hs.run();
        assert!(hs.state().data.connected_invoices.iter().any(|i| i.card == "card-ana" && i.due.m == 11 && i.paid));
        assert!(commands.try_iter().any(|c| matches!(c, Cmd::BillPaid(b) if b.card == "card-ana" && b.due == "2026-11-25" && b.paid)));
        // Abrir outra fatura mostra as compras do mês dela.
        hs.get_by_label_contains("Abrir Fatura Nubank · Out 26").click();
        hs.run();
        assert_eq!(hs.state().month, nov().add(-1));
        assert!(hs.query_by_label_contains("Abrir Notebook (1/10)").is_some());
    }
}

/// Capturas das telas novas: `cargo test --release --test ui -- --ignored new_screen_screenshots`.
#[test]
#[ignore]
fn new_screen_screenshots() {
    use flounder::{app::Screen, widgets::Theme};
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/screenshots");
    for (screen, name) in [(Screen::CashFlow, "fluxo-de-caixa"), (Screen::Cards, "cartoes")] {
        for (w, h, theme, suffix) in [(1440.0, 900.0, Theme::Light, "1440"), (1024.0, 700.0, Theme::Light, "1024"), (1440.0, 900.0, Theme::Dark, "dark")] {
            let (state, entries) = rich_state();
            let (mut hs, replies, _commands) = signed_in(state, entries, w, h);
            replies.send(Msg::Synced(Ok(flounder::api::SyncResult { synced: 2, failed: 0 }))).unwrap();
            hs.state_mut().screen = screen;
            hs.state_mut().theme = theme;
            hs.run();
            hs.render().unwrap().save(dir.join(format!("{name}-{suffix}.png"))).unwrap();
        }
    }
}

// ───────────── Nossa casa, Reserva e Ajustes (backend falso) ─────────────

/// `rich_state` + um acerto, meta de reserva, conexões e catálogo de bancos.
fn household_state() -> (flounder::api::State, Vec<Entry>) {
    use flounder::api::{Connection, Institution, Reserve, Settlement};
    let (mut state, entries) = rich_state();
    state.settlements = vec![Settlement { id: "uuid-1".into(), date: "2026-10-28".into(), amount_cents: 50_000, from: 1, to: 0 }];
    state.reserve = Some(Reserve { opening_cents: 1_840_000, target_cents: 3_000_000, name: "Reserva de emergência".into() });
    state.institutions = vec![
        Institution { key: "nubank".into(), name: "Nubank".into(), color: [130, 10, 209] },
        Institution { key: "inter".into(), name: "Banco Inter".into(), color: [255, 122, 0] },
    ];
    state.connections = vec![
        Connection { item_id: "item-ana".into(), owner: 0, institution: "Nubank".into(), institution_key: Some("nubank".into()), synced_at: Some("2026-11-10T17:30:00.000Z".into()) },
        Connection { item_id: "item-bruno".into(), owner: 1, institution: "Inter Pessoa Física".into(), institution_key: None, synced_at: Some("2026-11-07T12:05:00.000Z".into()) },
    ];
    (state, entries)
}

#[test]
fn house_shows_cumulative_balance_and_registers_and_deletes_settlements() {
    use flounder::app::Screen;
    let (state, entries) = household_state();
    let (mut hs, replies, commands) = signed_in(state, entries, 1440.0, 1600.0);
    hs.state_mut().mode = Mode::Couple;
    hs.run();
    let _ = commands.try_iter().count();
    let balance = hs.state().data.house_balance(nov());
    assert!(balance > 0, "no demo, Ana paga mais da casa");
    let message = format!("Bruno deve {} a Ana", money(balance));
    // O painel e Nossa casa mostram o mesmo saldo acumulado, já com o acerto de outubro.
    assert!(hs.query_by_label(&message).is_some(), "Divisão da casa no painel");
    hs.state_mut().screen = Screen::House;
    hs.run();
    assert!(hs.query_by_label(&message).is_some());
    assert!(hs.query_by_label("Despesas da casa em novembro").is_some());
    assert!(hs.query_by_label_contains("Abrir Aluguel: −R$ 3.800,00 · Moradia · Ana pagou").is_some());
    assert!(hs.query_by_label("Excluir acerto de R$ 500,00 em 28/10/2026").is_some());
    // Registrar acerto: valor do saldo, Bruno → Ana.
    hs.get_by_label("Registrar acerto").click();
    hs.run();
    assert_eq!(hs.state().extra.settle.as_ref().unwrap().amount, input_money(balance));
    hs.get_by_label("Registrar").click();
    hs.run();
    assert!(hs.state().extra.settle.is_none());
    assert!(hs.query_by_label_contains("Contas equilibradas").is_some());
    let sent: Vec<_> = commands.try_iter().collect();
    let local = match &sent[..] {
        [Cmd::Settle(s)] => {
            assert_eq!((s.amount, s.from, s.date), (balance, Person::B, today()));
            s.id.clone()
        }
        _ => panic!("esperava um Cmd::Settle"),
    };
    // Excluir pede confirmação; "Voltar" não exclui.
    let label = format!("Excluir acerto de {} em {}", money(balance), today().fmt());
    hs.get_by_label(&label).click();
    hs.run();
    assert!(hs.query_by_label("Excluir acerto?").is_some());
    hs.get_by_label("Voltar").click();
    hs.run();
    assert_eq!(hs.state().data.settlements.len(), 2);
    hs.get_by_label(&label).click();
    hs.run();
    hs.get_by_label("Excluir acerto").click();
    hs.run();
    assert!(hs.query_by_label(&message).is_some());
    assert!(commands.try_iter().any(|c| matches!(c, Cmd::Unsettle(id) if id == local)));
    hs.get_by_label("Excluir acerto de R$ 500,00 em 28/10/2026").click();
    hs.run();
    hs.get_by_label("Excluir acerto").click();
    hs.run();
    assert!(commands.try_iter().any(|c| matches!(c, Cmd::Unsettle(id) if id == "uuid-1")));
    assert!(hs.query_by_label(&format!("Bruno deve {} a Ana", money(balance + 50_000))).is_some());
    // Falha do backend: aviso, como nos lançamentos.
    replies.send(Msg::Done(Some("Não foi possível excluir o acerto. O backend respondeu com erro 500. Os dados foram recarregados.".into()))).unwrap();
    hs.run();
    assert!(hs.query_by_label_contains("Não foi possível excluir o acerto").is_some());
    // Clicar numa despesa abre o formulário de edição.
    hs.get_by_label_contains("Abrir Aluguel").click();
    hs.run();
    assert!(hs.query_by_label("Editar lançamento").is_some());
    // Individual: "Meu orçamento", sem acerto.
    hs.state_mut().form = None;
    hs.state_mut().mode = Mode::Individual;
    hs.run();
    assert_eq!(hs.query_all_by_label("Meu orçamento").count(), 2, "menu e título");
    assert!(hs.query_by_label("Despesas por categoria").is_some());
    assert!(hs.query_by_label("Quanto você gasta para morar").is_some());
    assert!(hs.query_by_label("Registrar acerto").is_none());
}

#[test]
fn reserve_screen_projects_the_goal_and_edits_it() {
    use flounder::app::Screen;
    let (state, entries) = household_state();
    let (mut hs, _replies, commands) = signed_in(state.clone(), entries.clone(), 1440.0, 1200.0);
    let _ = commands.try_iter().count();
    hs.state_mut().screen = Screen::Reserve;
    hs.run();
    // 1.840.000 + 5 aportes pagos de 1.500 (jul–nov); faltam 4.100 a 1.500/mês → 3 meses.
    assert_eq!(hs.state().data.summary(nov(), Mode::Couple).reserve, 2_590_000);
    assert!(hs.query_by_label("Reserva de emergência").is_some());
    assert!(hs.query_by_label_contains("meta em 3 meses (fevereiro de 2027)").is_some());
    assert!(hs.query_by_label("Média de R$ 1.500,00 por mês nos últimos 3 meses.").is_some());
    assert!(hs.query_by_label_contains("Nov 26: R$ 1.500,00").is_some(), "barras acessíveis");
    hs.get_by_label("Editar meta").click();
    hs.run();
    assert_eq!(hs.state().extra.goal.as_ref().unwrap().target, "30.000,00");
    hs.state_mut().extra.goal.as_mut().unwrap().target = "abc".into();
    hs.get_by_label("Salvar").click();
    hs.run();
    assert!(hs.query_by_label_contains("Use um valor como 1.234,56.").is_some());
    hs.state_mut().extra.goal.as_mut().unwrap().target = "40.000,00".into();
    hs.run();
    hs.get_by_label("Salvar").click();
    hs.run();
    assert!(hs.state().extra.goal.is_none());
    assert!(commands.try_iter().any(|c| matches!(c, Cmd::Reserve(r) if r.target_cents == 4_000_000 && r.opening_cents == 1_840_000 && r.name == "Reserva de emergência")));
    assert!(hs.query_by_label_contains("meta em 10 meses").is_some());
    // Sem meta: estado vazio; sem aportes recentes: sem projeção.
    let mut empty = state.clone();
    empty.reserve = None;
    let (mut hs, _replies, _commands) = signed_in(empty, vec![], 1024.0, 700.0);
    hs.state_mut().screen = Screen::Reserve;
    hs.run();
    assert!(hs.query_by_label("Defina uma meta").is_some());
    hs.get_by_label("Definir meta").click();
    hs.run();
    assert!(hs.query_by_label("Meta da reserva").is_some());
    let (mut hs, _replies, _commands) = signed_in(state, vec![], 1024.0, 700.0);
    hs.state_mut().screen = Screen::Reserve;
    hs.run();
    assert!(hs.query_by_label_contains("Sem aportes recentes").is_some());
}

#[test]
fn settings_rename_members_and_choose_connection_bank() {
    use flounder::app::Screen;
    let (state, entries) = household_state();
    let (mut hs, _replies, commands) = signed_in(state, entries, 1440.0, 1600.0);
    let _ = commands.try_iter().count();
    hs.state_mut().screen = Screen::Settings;
    hs.run();
    let _ = commands.try_iter().count();
    assert!(hs.query_by_label("Ana · atualizada hoje às 14:30").is_some());
    assert!(hs.query_by_label("Bruno · atualizada há 3 dias (07/11/2026 às 09:05)").is_some());
    assert!(hs.query_by_label_contains("meu.pluggy.ai").is_some());
    assert!(hs.query_all_by_label_contains("Conectar banco ou cartão").count() >= 2, "texto e botão");
    // Nome inválido não grava.
    hs.state_mut().extra.names = Some([" ".into(), "Bruno".into()]);
    hs.run();
    hs.get_by_label("Salvar nomes").click();
    hs.run();
    assert!(hs.query_by_label_contains("Informe um nome.").is_some());
    assert_eq!(commands.try_iter().count(), 0);
    hs.state_mut().extra.names = Some(["Ana Clara".into(), "Bruno".into()]);
    hs.run();
    hs.get_by_label("Salvar nomes").click();
    hs.run();
    assert_eq!(hs.state().data.names[0], "Ana Clara");
    assert!(hs.state().extra.names.is_none());
    let sent: Vec<_> = commands.try_iter().collect();
    assert!(matches!(&sent[..], [Cmd::Member(0, n)] if n == "Ana Clara"));
    // Banco da conexão: escolhe no catálogo.
    hs.get_by_label("Banco da conexão Inter Pessoa Física de Bruno").click();
    hs.run();
    hs.get_by_label("Banco Inter").click();
    hs.run();
    assert!(commands.try_iter().any(|c| matches!(c, Cmd::Institution(item, Some(key)) if item == "item-bruno" && key == "inter")));
    assert_eq!(hs.state().connections[1].institution, "Banco Inter");
    assert_eq!(hs.query_all_by_label("Atualizando contas").count(), 2, "menu e Ajustes");
    assert!(hs.query_by_label("Como conectar").is_some());
}

#[test]
fn update_card_walks_through_download_restart_and_failure() {
    use flounder::app::Screen;
    use flounder::update::Msg as UMsg;
    let (state, entries) = rich_state();
    let mut storage = MemoryStorage::default();
    storage.0.insert("member_client".into(), "client-1".into());
    let mut app = App::load(Some(&storage));
    let (tx, _commands) = std::sync::mpsc::channel();
    let (replies, rx) = std::sync::mpsc::channel();
    app.remote.as_mut().unwrap().worker = Some(flounder::backend::Worker { client: "client-1".into(), tx, rx, pending: 0 });
    replies.send(Msg::State(Box::new(state), entries)).unwrap();
    let updates = fake_update_worker(&mut app);
    let mut hs = harness(app, 1440.0, 1600.0, 1.0);
    hs.state_mut().screen = Screen::Settings;
    hs.run();
    assert!(hs.query_by_label_contains("Versão 0.").is_some(), "card quieto antes da 1ª verificação");
    updates.send(UMsg::Downloading { version: "0.3.0".into() }).unwrap();
    updates.send(UMsg::Progress(0.4)).unwrap();
    hs.run();
    assert!(hs.query_by_label_contains("Nova versão 0.3.0 — baixando…").is_some());
    updates.send(UMsg::Ready { version: "0.3.0".into(), path: "/tmp/.flounder.part".into() }).unwrap();
    hs.run();
    assert!(hs.query_by_label_contains("Nova versão 0.3.0 baixada").is_some());
    hs.get_by_label_contains("Reiniciar agora").click();
    hs.run();
    updates.send(UMsg::UpToDate).unwrap();
    hs.run();
    assert!(hs.query_by_label_contains("Você está em dia").is_some());
    updates.send(UMsg::Failed("Não foi possível verificar atualizações. Sem conexão?".into())).unwrap();
    hs.run();
    assert!(hs.query_by_label_contains("Tentar de novo").is_some());
    updates.send(UMsg::Available { version: "0.3.0".into() }).unwrap();
    hs.run();
    assert!(hs.query_by_label_contains("Para atualizar, baixe o pacote novo").is_some(), "Linux sem AppImage: instrução manual");
}

/// Capturas: `cargo test --test ui -- --ignored house_reserve_settings_screenshots`.
#[test]
#[ignore]
fn house_reserve_settings_screenshots() {
    use flounder::{app::Screen, widgets::Theme};
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/screenshots");
    for (screen, name) in [(Screen::House, "nossa-casa"), (Screen::Reserve, "reserva"), (Screen::Settings, "ajustes")] {
        for (w, h, theme, suffix) in [(1440.0, 900.0, Theme::Light, "1440"), (1024.0, 700.0, Theme::Light, "1024"), (1440.0, 900.0, Theme::Dark, "dark")] {
            let (state, entries) = household_state();
            let (mut hs, replies, _commands) = signed_in(state, entries, w, h);
            replies.send(Msg::Synced(Ok(flounder::api::SyncResult { synced: 2, failed: 0 }))).unwrap();
            hs.state_mut().screen = screen;
            hs.state_mut().theme = theme;
            hs.run();
            hs.render().unwrap().save(dir.join(format!("{name}-{suffix}.png"))).unwrap();
        }
    }
    let (mut state, entries) = household_state();
    state.reserve = None;
    let (mut hs, _replies, _commands) = signed_in(state, entries, 1024.0, 700.0);
    hs.state_mut().screen = Screen::Reserve;
    hs.run();
    hs.render().unwrap().save(dir.join("reserva-sem-meta.png")).unwrap();
    let (state, entries) = household_state();
    let (mut hs, _replies, _commands) = signed_in(state, entries, 1440.0, 900.0);
    hs.state_mut().screen = Screen::House;
    hs.state_mut().mode = Mode::Individual;
    hs.run();
    hs.render().unwrap().save(dir.join("meu-orcamento-1440.png")).unwrap();
    let (state, entries) = household_state();
    let (mut hs, _replies, _commands) = signed_in(state, entries, 1024.0, 1400.0);
    hs.state_mut().screen = Screen::Settings;
    hs.run();
    hs.render().unwrap().save(dir.join("ajustes-completo.png")).unwrap();
}

// ───────────────────────── Parcelas, contas fixas e rateio pela renda ─────────────────────────

#[test]
fn form_creates_manual_installments_month_by_month() {
    let mut hs = harness(App::new(), 1440.0, 900.0, 1.0);
    hs.get_by_label("Adicionar").click();
    hs.run();
    let f = hs.state_mut().form.as_mut().unwrap();
    f.desc = "Viagem dos amigos".into();
    f.amount = "120".into();
    hs.run();
    // Ainda sem marcar: salvar cria um lançamento único, sem sufixo.
    hs.get_by_label("Salvar").click();
    hs.run();
    assert!(hs.state().form.is_none());
    assert!(hs.state().data.entries.iter().all(|e| !e.desc.contains("Viagem dos amigos (")));

    // De novo, agora parcelado: 4 pagamentos mensais de 120.
    hs.get_by_label("Adicionar").click();
    hs.run();
    let f = hs.state_mut().form.as_mut().unwrap();
    f.desc = "Viagem dos amigos".into();
    f.amount = "120".into();
    hs.run();
    checkbox(&mut hs, "Parcelar em vários meses");
    assert!(hs.state().form.as_ref().unwrap().parcelar);
    hs.state_mut().form.as_mut().unwrap().parcels = "4".into();
    hs.run();
    assert!(hs.query_by_label_contains("4 pagamentos mensais de R$ 120,00 · total R$ 480,00").is_some());
    hs.get_by_label("Salvar").click();
    hs.run();
    assert!(hs.state().form.is_none());
    // Uma parcela por mês, todas com o mesmo valor e só a primeira paga.
    for i in 1..=4u32 {
        let m = nov().add(i as i32 - 1);
        let e = hs.state().data.in_month(m).find(|e| e.desc == format!("Viagem dos amigos ({i}/4)"));
        let e = e.unwrap_or_else(|| panic!("falta a parcela {i}/4 em {}", m.short()));
        assert_eq!(e.amount, 12_000);
        assert!(!e.paid, "a parcela nasce a pagar; marcar é com a fatura/pagamento");
    }
}

#[test]
fn existing_payment_can_be_marked_as_an_installment() {
    let mut hs = harness(App::new(), 1440.0, 900.0, 1.0);
    hs.get_by_label_contains("Abrir Ajuda pra família").scroll_to_me();
    hs.run();
    hs.get_by_label_contains("Abrir Ajuda pra família").click();
    hs.run();
    checkbox(&mut hs, "Marcar como parcela de uma compra");
    let f = hs.state_mut().form.as_mut().unwrap();
    assert!(f.parcelar);
    f.parcels = "3".into();
    f.parcela_n = "2".into();
    hs.run();
    hs.get_by_label("Salvar").click();
    hs.run();
    assert!(hs.state().form.is_none());
    // Esta é a parcela 2 de 3; só falta a última, no mês que vem.
    assert!(hs.state().data.entries.iter().any(|e| e.desc == "Ajuda pra família (2/3)" && e.amount == 80_000));
    let last = hs.state().data.in_month(nov().add(1)).find(|e| e.desc == "Ajuda pra família (3/3)").unwrap();
    assert!(!last.paid);
    assert!(!hs.state().data.entries.iter().any(|e| e.desc.ends_with("(1/3)")));
}

#[test]
fn forecast_lists_the_fixed_bills_a_future_month_is_missing() {
    use flounder::app::Screen;
    let (state, entries) = rich_state();
    let (mut hs, _replies, commands) = signed_in(state, entries, 1440.0, 1600.0);
    // Janeiro (a última parcela da viagem é a única conta) mostra a previsão das fixas.
    hs.state_mut().set_month(nov().add(2));
    hs.state_mut().screen = Screen::CashFlow;
    hs.run();
    assert!(hs.query_by_label("Previsão das contas fixas de janeiro").is_some());
    let aluguel = hs.state().data.forecast(nov().add(2), None).iter().find(|f| f.desc == "Aluguel").unwrap().clone();
    assert_eq!((aluguel.estimate, aluguel.min, aluguel.max), (380_000, 380_000, 380_000));
    assert!(hs.query_by_label_contains("sempre R$ 3.800,00").is_some());
    // Luz oscila: a linha traz a faixa e a média.
    let luz = hs.state().data.forecast(nov().add(2), None).iter().find(|f| f.desc == "Luz").unwrap().clone();
    assert!(luz.min < luz.estimate && luz.estimate < luz.max);
    hs.get_by_label("Lançar Aluguel").click();
    hs.run();
    let made = hs.state().data.in_month(nov().add(2)).find(|e| e.desc == "Aluguel").unwrap();
    assert_eq!((made.amount, made.paid, made.recurring), (380_000, false, true));
    assert!(commands.try_iter().any(|c| matches!(c, Cmd::Entry(e) if e.desc == "Aluguel" && e.recurring)));
    // Lançada a conta, ela sai da previsão.
    assert!(hs.state().data.forecast(nov().add(2), None).iter().all(|f| f.desc != "Aluguel"));
    // No mês passado (com todas as contas já lançadas) a previsão não aparece.
    hs.state_mut().set_month(nov());
    hs.run();
    assert!(hs.query_by_label_contains("Previsão das contas fixas").is_none());
}

#[test]
fn settings_show_the_income_split_and_can_turn_shared_expenses_off() {
    use flounder::app::Screen;
    let (state, entries) = rich_state();
    let (mut hs, _replies, commands) = signed_in(state, entries, 1440.0, 1600.0);
    hs.state_mut().mode = Mode::Couple;
    hs.state_mut().screen = Screen::Settings;
    hs.run();
    // Renda registrada: Ana 57% e Bruno 43% (salários + PIX extra de Ana).
    assert!(hs.query_by_label_contains("Ana paga 57% e Bruno paga 43%").is_some());
    checkbox(&mut hs, "Remover renda compartilhada");
    assert!(hs.state().data.shared_split_off);
    assert!(commands.try_iter().any(|c| matches!(c, Cmd::Settings(s) if s.shared_split_off)));

    // Nossa casa sem rateio: despesas da casa por inteiro para quem pagou, sem acertos.
    hs.state_mut().screen = Screen::House;
    hs.run();
    assert!(hs.query_by_label("Despesas da casa").is_some());
    assert!(hs.query_by_label_contains("O rateio compartilhado está desligado").is_some());
    assert!(hs.query_by_label("Registrar acerto").is_none());

    // E o painel também explica, no bloco da divisão.
    hs.state_mut().screen = Screen::Home;
    hs.run();
    assert!(hs.query_by_label_contains("Rateio desligado").is_some());
}

#[test]
fn house_screen_splits_shared_bills_by_income() {
    use flounder::app::Screen;
    let (state, entries) = rich_state();
    let (mut hs, _replies, _commands) = signed_in(state, entries, 1440.0, 1600.0);
    hs.state_mut().mode = Mode::Couple;
    hs.state_mut().screen = Screen::House;
    hs.run();
    assert!(hs.query_by_label_contains("Rateio pela renda: Ana 57% · Bruno 43%.").is_some());
    assert!(hs.query_by_label_contains("Parte de cada um em novembro").is_some());
    // O acerto prefere quem pagou além da própria parte.
    let s = hs.state().data.summary(nov(), Mode::Couple);
    assert!(s.owed[0] > s.owed[1], "Ana ganha mais, então cabe a ela a parte maior");
    hs.get_by_label("Registrar acerto").scroll_to_me();
    hs.run();
    assert!(hs.query_by_label("Registrar acerto").is_some());
}

#[test]
fn fresh_install_pairs_with_a_code_and_never_asks_again() {
    use flounder::backend::PairMsg;
    // Sem arquivo de config nenhum — nem o backend/.env de um checkout de desenvolvimento.
    unsafe { std::env::set_var("FLOUNDER_BACKEND_ENV", "/não-existe/flounder.env"); }
    let mut app = App::load(None);
    let mut hs = harness(app, 1024.0, 700.0, 1.0);
    assert!(hs.query_by_label_contains("Conectar ao servidor").is_some(), "instalação nova pede o código");
    assert!(hs.query_by_label("Quem é você?").is_none());
    // Código vazio: erro na hora, sem chamada.
    hs.get_by_label("Conectar").click();
    hs.run();
    assert!(hs.query_by_label_contains("Informe o código").is_some());
    // Resposta do servidor falsa: pareia como client-2 e entra direto no app.
    let (tx, commands) = std::sync::mpsc::channel();
    let (replies, rx) = std::sync::mpsc::channel();
    let (pair_tx, pair_rx) = std::sync::mpsc::channel::<flounder::backend::PairMsg>();
    hs.state_mut().remote.as_mut().unwrap().worker = Some(flounder::backend::Worker { client: "client-2".into(), tx, rx, pending: 0 });
    fake_update_worker(hs.state_mut());
    replies.send(Msg::State(Box::new(fake_state()), vec![])).unwrap();
    hs.state_mut().pair.code = "MARIA-2026-FLOUNDER".into();
    hs.state_mut().pair.sending = Some(pair_rx);
    pair_tx.send(PairMsg::Paired { client: "client-2".into(), token: "token-pareado-2".into() }).unwrap();
    hs.run();
    assert!(hs.query_by_label_contains("Conectar ao servidor").is_none(), "pareou: a tela de código some");
    assert!(hs.query_by_label("Quem é você?").is_none(), "o código já diz quem é");
    assert!(hs.query_all_by_label_contains("Saldo previsto").count() >= 1);
    assert_eq!(hs.state().remote.as_ref().unwrap().member_token.as_deref(), Some("token-pareado-2"));
    let mut storage = MemoryStorage::default();
    eframe::App::save(hs.state_mut(), &mut storage);
    assert_eq!(storage.0.get("member_client").map(String::as_str), Some("client-2"));
    assert_eq!(storage.0.get("member_token").map(String::as_str), Some("token-pareado-2"), "o token fica guardado: nunca mais pede código");
    // Erro do servidor: volta para a tela de código com a mensagem.
    unsafe { std::env::remove_var("FLOUNDER_BACKEND_ENV"); }
}

#[test]
fn access_card_shows_the_fixed_codes_to_send() {
    let (state, entries) = rich_state();
    let (mut hs, _replies, _commands) = signed_in(state, entries, 1440.0, 1600.0);
    hs.state_mut().screen = flounder::app::Screen::Settings;
    hs.state_mut().pair.codes = Some([Some("CODIGO-DO-UM".into()), None]);
    hs.run();
    assert!(hs.query_by_label_contains("Código fixo para conectar").is_some());
    assert!(hs.query_by_label_contains("Ana: CODIGO-DO-UM").is_some());
    assert!(hs.query_by_label_contains("Copiar código").is_some());
    assert!(hs.query_by_label_contains("sem código no servidor").is_some(), "código ausente explica como definir");
}

// ───────────────────────── Orçamentos, painel novo e rotina ─────────────────────────

#[test]
fn budget_and_price_cards_show_only_when_there_is_data() {
    // Demonstração: sem orçamentos e sem alertas, os cards condicionais não entram no painel.
    let hs = harness(App::new(), 1440.0, 1600.0, 1.0);
    assert!(hs.query_by_label("Orçamento do mês").is_none());
    assert!(hs.query_by_label("Mudanças de preço").is_none());

    // Com orçamentos: consumo por categoria contra o teto.
    let mut app = App::new();
    app.budgets = vec![
        flounder::api::Budget { category: "groceries".into(), monthly_cents: 100_000 },
        flounder::api::Budget { category: "transport".into(), monthly_cents: 250_000 },
    ];
    let hs = harness(app, 1440.0, 1600.0, 1.0);
    assert!(hs.query_by_label("Orçamento do mês").is_some());
    assert!(hs.query_by_label_contains("de R$ 1.000,00 ·").is_some(), "linha com teto e consumo");

    // Conta fixa que escapou da média: card de mudanças de preço com o % ao lado.
    let mut app = App::new();
    let mut d = Data::empty();
    d.names = ["Ana".into(), "Bruno".into()];
    let nov = nov();
    for (k, v) in [(-3, 10_000), (-2, 10_000), (-1, 10_000), (0, 20_000)] {
        d.upsert(Entry {
            id: 0, desc: "Luz".into(), amount: v, date: nov.add(k).day(15), kind: Kind::Expense,
            category: Category::Utilities, owner: Person::A, shared: false, card: None,
            connected: None, imported: false, paid: true, recurring: true,
        });
    }
    app.data = d;
    let hs = harness(app, 1440.0, 1600.0, 1.0);
    assert!(hs.query_by_label("Mudanças de preço").is_some());
    assert!(hs.query_by_label_contains("Luz: R$ 200,00 (média R$ 133,33)").is_some());
    assert!(hs.query_by_label_contains("+50%").is_some());
}

#[test]
fn connected_card_modal_projects_next_invoices_and_warns_about_closing() {
    use flounder::app::Screen;
    let (state, entries) = rich_state();
    let mut entries = entries;
    entries.push(Entry {
        id: 999, desc: "Compra de dezembro".into(), amount: 50_000, date: nov().day(25),
        kind: Kind::Expense, category: Category::Shopping, owner: Person::A, shared: false,
        card: None, connected: Some("card-ana".into()), imported: true, paid: true, recurring: false,
    });
    let (mut hs, _replies, _commands) = signed_in(state, entries, 1440.0, 2000.0);
    hs.state_mut().mode = Mode::Couple;
    // Sem fechamento informado: não há base para projetar.
    hs.state_mut().connected_invoice = Some("card-ana".into());
    hs.run();
    assert!(hs.query_by_label("Sem compras futuras fechadas.").is_some());
    hs.get_by_label("Fechar").click();
    hs.run();
    assert!(hs.state().connected_invoice.is_none());
    // Fechamento dia 20 com vencimento 25: a compra de 25/11 fecha a fatura de dezembro.
    hs.state_mut().connected_cards.as_mut().unwrap().iter_mut().for_each(|c| {
        c.closing_day = Some(20);
        c.due_day = Some(25);
    });
    hs.state_mut().connected_invoice = Some("card-ana".into());
    hs.run();
    assert!(hs.query_by_label("Próximas faturas").is_some());
    assert!(hs.query_by_label_contains("Dez 26 · R$").is_some());
    assert!(hs.query_by_label_contains("Jan 27 · R$").is_none(), "meses sem compras fechadas ficam de fora");
    hs.get_by_label("Fechar").click();
    hs.run();

    // Fechamento iminente: chip âmbar na face do cartão do painel e na seção de compras.
    hs.state_mut().connected_cards.as_mut().unwrap().iter_mut().for_each(|c| c.closing_day = Some(today().d));
    hs.run();
    assert!(hs.query_all_by_label_contains("Fecha hoje — compras de hoje já vão para a próxima fatura.").count() >= 2, "painel: um aviso por cartão");
    hs.state_mut().screen = Screen::Cards;
    hs.run();
    assert!(hs.query_all_by_label_contains("Fecha hoje — compras de hoje já vão para a próxima fatura.").count() >= 2, "tela de cartões");
}

#[test]
fn assistant_chat_requests_history_on_open_and_clears_the_conversation() {
    let (state, entries) = rich_state();
    let (mut hs, replies, commands) = signed_in(state, entries, 1440.0, 900.0);
    hs.state_mut().agent.reports = Some(flounder::api::AssistantReports { enabled: true, busy: false, reports: vec![] });
    hs.state_mut().agent.open = true;
    hs.run();
    assert!(commands.try_iter().any(|c| matches!(c, Cmd::History)), "abertura com conversa vazia pede o histórico");
    let _ = commands.try_iter().count();

    // Histórico chega e enche a conversa.
    let history = |contents: &[&str]| flounder::api::ChatHistory {
        messages: contents.iter().map(|c| flounder::api::ChatMsg {
            role: (if c.starts_with("Q:") { "user" } else { "assistant" }).into(),
            content: c.trim_start_matches("Q:").into(), ops: vec![],
        }).collect(),
    };
    replies.send(Msg::History(Ok(history(&["Q:Como foi meu mês?", "De boa. 🐠"])))).unwrap();
    hs.run();
    assert!(hs.query_by_label_contains("De boa. 🐠").is_some());
    // Histórico que chega com a conversa já preenchida não sobrescreve nada.
    replies.send(Msg::History(Ok(history(&["Q:antiga"])))).unwrap();
    hs.run();
    assert!(hs.query_by_label_contains("De boa. 🐠").is_some());
    // Reabrir com conversa preenchida não pede o histórico de novo.
    hs.state_mut().agent.open = false;
    hs.run();
    hs.state_mut().agent.open = true;
    hs.run();
    assert!(!commands.try_iter().any(|c| matches!(c, Cmd::History)));

    // "Limpar conversa": zera a conversa local e apaga no backend, num clique.
    hs.get_by_label("Limpar conversa").click();
    hs.run();
    assert!(hs.state().agent.chat.is_empty());
    assert!(commands.try_iter().any(|c| matches!(c, Cmd::ClearHistory)));
    // Reaberta com a conversa vazia: pede o histórico outra vez.
    hs.state_mut().agent.open = false;
    hs.run();
    hs.state_mut().agent.open = true;
    hs.run();
    assert!(commands.try_iter().any(|c| matches!(c, Cmd::History)));
}

#[test]
fn settings_show_budgets_assistant_actions_and_month_routine() {
    use flounder::app::Screen;
    let (mut state, entries) = rich_state();
    state.budgets = vec![flounder::api::Budget { category: "groceries".into(), monthly_cents: 60_000 }];
    let (mut hs, replies, commands) = signed_in(state, entries, 1440.0, 2400.0);
    hs.state_mut().mode = Mode::Couple;
    let _ = commands.try_iter().count();
    hs.state_mut().screen = Screen::Settings;
    hs.run();
    // A seção de ações busca uma vez ao aparecer; drena aqui para não poluir os asserts seguintes.
    assert!(commands.try_iter().any(|c| matches!(c, Cmd::AssistantActions)));

    // Orçamentos: o valor salvo preenche a linha e o consumo do mês aparece ao lado.
    assert!(hs.query_by_label("Orçamentos").is_some());
    let spent = hs.state().data.summary(nov(), Mode::Couple).by_category.iter()
        .find(|(c, _)| c.key() == "groceries").map_or(0, |(_, v)| *v);
    let pct = (spent as f32 / 60_000.0 * 100.0).round() as i32;
    assert!(hs.query_by_label_contains(&format!("{} · {pct}%", money(spent))).is_some(), "gasto e consumo ao lado do orçamento");
    // Editar a linha de Moradia (primeira) e salvar manda o comando e aplica na hora.
    let mut drafts = vec![String::new(); 13];
    drafts[0] = "1.500,00".into();
    hs.state_mut().extra.budget_drafts = Some(drafts);
    hs.run();
    hs.get_all_by_label("Salvar").next().unwrap().click();
    hs.run();
    assert!(commands.try_iter().any(|c| matches!(c, Cmd::Budget(b) if b.category == "housing" && b.monthly_cents == 150_000)));
    assert!(hs.state().budgets.iter().any(|b| b.category == "housing"), "atualização otimista do card");

    // Ações do assistente: a resposta vira lista com hora local.
    assert!(hs.query_by_label_contains("Buscando as ações do assistente").is_some());
    replies.send(Msg::Actions(Ok(flounder::api::ActionLog { actions: vec![
        flounder::api::ActionRecord { tool: "criar_lancamento".into(), label: "Criou \"Mercado\"".into(), at: "2026-11-12T17:32:00Z".into() },
    ] }))).unwrap();
    hs.run();
    assert!(hs.query_by_label_contains("Criou \"Mercado\"").is_some());
    assert!(hs.query_by_label_contains("12/11 14:32").is_some(), "data/hora de São Paulo");
    assert!(hs.query_by_label_contains("criar_lancamento").is_some());
    // Falha: aviso na própria seção.
    replies.send(Msg::Actions(Err("O backend respondeu com erro 500.".into()))).unwrap();
    hs.run();
    assert!(hs.query_by_label("Não foi possível carregar as ações do assistente.").is_some());
    // Sem ações: estado vazio.
    replies.send(Msg::Actions(Ok(flounder::api::ActionLog { actions: vec![] }))).unwrap();
    hs.run();
    assert!(hs.query_by_label("O assistente ainda não agiu.").is_some());

    // Rotina do mês: o pedido mostra "gerando"; o relatório renderiza com tolerância ao JSON.
    let _ = commands.try_iter().count();
    hs.get_by_label_contains("Fechar o mês").scroll_to_me();
    hs.run();
    hs.get_by_label_contains("Fechar o mês").click();
    hs.run();
    assert!(commands.try_iter().any(|c| matches!(c, Cmd::Routine(m) if m.is_empty())));
    assert!(hs.query_by_label_contains("Gerando o relatório do mês").is_some());
    let report = serde_json::json!({
        "copiados": [ {"descricao": "Aluguel", "valor_centavos": 1_800_000, "tipo": "expense"}, {"descricao": "Internet", "valor_centavos": 9_990, "tipo": "expense"} ],
        "faturas": [ {"label": "Fatura Nubank", "due": "2026-11-25", "total_cents": 231_040, "paid": false} ],
        "acerto_sugerido": {"valor_cents": 50_000, "de": 1},
        "alertas_preco": [ {"desc": "Luz", "recent_cents": 19_000, "average_cents": 13_333} ],
        "fechamentos": [ {"nome": "Nubank", "dia_fechamento": 20, "dias": 3} ]
    });
    replies.send(Msg::Routine(Ok(report))).unwrap();
    hs.run();
    assert!(hs.query_by_label_contains("2 lançamentos copiados").is_some());
    assert!(hs.query_by_label_contains("Aluguel").is_some());
    assert!(hs.query_by_label_contains("Fatura Nubank · vence 25/11").is_some());
    assert!(hs.query_by_label_contains("Acerto sugerido: R$ 500,00 de Bruno para Ana").is_some());
    assert!(hs.query_by_label_contains("Luz: R$ 190,00 (média R$ 133,33)").is_some());
    assert!(hs.query_by_label_contains("Nubank fecha em 3 dias").is_some());
    // Falha: mensagem no lugar do relatório e o botão volta a funcionar.
    replies.send(Msg::Routine(Err("O backend respondeu com erro 500.".into()))).unwrap();
    hs.run();
    assert!(hs.query_by_label_contains("O backend respondeu com erro 500.").is_some());
    assert!(hs.query_by_label_contains("Fechar o mês").is_some());
}

