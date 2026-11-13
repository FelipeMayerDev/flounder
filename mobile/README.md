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

## Caminho para o celular (Android) — APK

O mesmo binário nativo empacotado como APK pelo [xbuild](https://github.com/rust-mobile/xbuild),
sem Gradle. Requisitos (um só, na primeira vez): JDK 17, Android SDK + NDK (via `sdkmanager`)
e `rustup target add aarch64-linux-android`.

```sh
export JAVA_HOME=~/tools/jdk-17.0.21+9
export ANDROID_HOME=~/Android/Sdk
export NDK_HOME=$ANDROID_HOME/ndk/27.2.12479018
export PATH="$JAVA_HOME/bin:$PATH"
x build --release --platform android --arch arm64 --format apk
# → mobile/target/x/release/android/flounder-mobile.apk
#   (config do APK: mobile/manifest.yaml; ícone ≥512px em mobile/assets/)
```

`android_main` (NativeActivity) vive em `mobile/src/lib.rs`; o `Cargo.toml` do mobile tem os
metadados do APK (pacote `br.com.felipemayer.flounder`, minSdk 26). Instalar: copie o APK pro
celular e abra, ou `adb install`. O APK 0.2.1 também fica em
`https://finance.felipemayer.com.br/releases/Flounder-0.2.1-arm64.apk`.

iOS exige Xcode/macOS (fase futura, em uma máquina Apple — o código egui é o mesmo).

## Estado (fase 1)

- Início: saldo previsto com barra empilhada, repartição, salário, contas a pagar com tira de dias.
- Fluxo: resumo do mês (entradas/saídas/saldo) e lançamentos por dia, com cartão e compartilhadas.
- Cartões: total das faturas e um bloco por cartão (fatura, compras, disponível com barra).
- Casa: filtro Todas/Pendentes/Pagas, contas com checkbox (marca paga e recalcula), meio a meio.
- FAB: folha de novo lançamento gravando no mock do mês aberto (sem persistência).
- Menu: troca quem está no celular ("Você" nas listas).
