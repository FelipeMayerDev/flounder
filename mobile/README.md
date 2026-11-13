# Flounder mobile

O Flounder de celular: **o mesmo motor nativo do desktop** (Rust + egui/eframe + wgpu), em
janela de celular. Fase 1 = interface com dados de demonstração (`src/mock.rs`, os números do
mockup); fase 2 = API real do backend reutilizando `flounder::model`.

## Rodar (janela de celular no desktop)

```sh
cargo run --manifest-path mobile/Cargo.toml
```

Capturas de todas as telas (sem GPU não roda; precisa de Vulkan/software):

```sh
cargo test --manifest-path mobile/Cargo.toml --test snap -- --ignored
# → mobile/docs/screenshots/{inicio,fluxo,cartoes,casa}.png
```

## Estrutura

| Arquivo | Papel |
|---|---|
| `src/app.rs` | Casca: topo, barra do mês, tab bar flutuante + FAB, folha de novo lançamento, despacho das telas. |
| `src/theme.rs` | Paleta do mockup e helpers (`card`, `money` com centavos menores, `chip`, barras, `avatar`). |
| `src/mock.rs` | Dados de demonstração (Dezembro 2026 + Novembro). Fase 2: `flounder::model::Data` da API. |
| `src/screens/` | As 4 telas: `home` (Início), `flow` (Fluxo), `cards` (Cartões), `house` (Casa). |

## Caminho para o celular (Android)

O app é o mesmo binário nativo; o empacotamento Android usa `cargo-ndk` + o template de
projeto do eframe ([egui mobile](https://github.com/rust-mobile/egui)):

1. `rustup target add aarch64-linux-android`
2. `cargo install cargo-ndk` + Android SDK/NDK (`sdkmanager "ndk;27.0.11718014"`)
3. `cargo ndk -t arm64-v8a -o ./jniLibs build --release --manifest-path mobile/Cargo.toml`
4. Embraçar o `.so` num template Gradle (o repositório rust-mobile/egui tem o `apk.template`)

iOS exige Xcode/macOS (fase futura, em uma máquina Apple — o código egui é o mesmo).

## Estado (fase 1)

- Início: saldo previsto com barra empilhada, repartição, salário, contas a pagar com tira de dias.
- Fluxo: resumo do mês (entradas/saídas/saldo) e lançamentos por dia, com cartão e compartilhadas.
- Cartões: total das faturas e um bloco por cartão (fatura, compras, disponível com barra).
- Casa: filtro Todas/Pendentes/Pagas, contas com checkbox (marca paga e recalcula), meio a meio.
- FAB: folha de novo lançamento gravando no mock do mês aberto (sem persistência).
- Menu: troca quem está no celular ("Você" nas listas).
