# Flounder

Protótipo desktop de controle financeiro individual e de casal, em Rust + egui/eframe (WGPU).
A página inicial usa o **Painel equilibrado**, com calendário de vencimentos interativo e evolução do saldo previsto.
Todas as telas funcionam: Visão geral, Fluxo de caixa, Cartões, Nossa casa, Reserva e Ajustes.

- Página inicial: [prévia completa](docs/screenshots/painel-completo.png)
- Capturas: [`docs/screenshots/`](docs/screenshots) — a janela e a bolha do agente: `agente-janela.png` / `agente-bolha.png`
- Instruções do pacote para quem só vai abrir o app: [`docs/EXECUTAR.txt`](docs/EXECUTAR.txt)

## Dados reais

O app lê e grava tudo pelo backend local (Axum + PostgreSQL); não há mais dados de demonstração
no executável (`Data::demo()` fica só para testes, capturas e `examples/export_demo.rs`).

- **Quem é você?** Na primeira abertura o app busca os membros do domicílio e pergunta quem usa o
  computador ([captura](docs/screenshots/estado-quem-e-voce.png)). A escolha (`client-N`) é salva
  e, daí em diante, as chamadas usam o `APP_TOKEN_N` dessa pessoa. Em **Ajustes → Quem usa este
  computador** dá para trocar pelo nome.
- **Token**: no modo local o app lê só as linhas `APP_TOKEN_1`/`APP_TOKEN_2` de `backend/.env`
  (outro caminho: `FLOUNDER_BACKEND_ENV`). Endereço: `FLOUNDER_BACKEND_URL`, padrão `http://127.0.0.1:3001`.
- **Sincronização**: `GET /api/state` a cada 5 s numa thread própria (contrato em
  [`shared/api.rs`](shared/api.rs)). Novo lançamento, edição e marcar como paga aparecem na hora e são
  gravados em segundo plano (`POST`/`PUT /api/entries`); se falhar, um aviso aparece e os dados são recarregados.
  Um formulário aberto não é sobrescrito pela recarga.
- **Sem backend** (ou sem `backend/.env`): o app mostra o motivo, pede para executar `backend/run-local.sh`
  e oferece **Tentar novamente**; não recorre a dados fictícios.
- **Importados**: transações de bancos e cartões conectados chegam sozinhas como lançamentos
  (`imported`, já pagos, categoria sugerida e editável). Compras no cartão (`card` = id do cartão
  conectado) pesam nas despesas, mas não viram contas a pagar: a conta é a fatura.
- **Excluir**: no formulário de edição, **Excluir** pede confirmação e chama `DELETE /api/entries/{id}`.
  A linha some na hora; se falhar, aparece o aviso e os dados são recarregados. Um importado excluído
  não volta na próxima importação.
- **Atualizar contas** (menu lateral): `POST /api/sync` reimporta todas as conexões do domicílio
  (até 90 s; as outras chamadas usam 5 s) e mostra quantas conexões foram atualizadas ou falharam.
  Roda uma vez sozinho depois da primeira carga a cada abertura — e o **backend também reimporta
  tudo sozinho a cada 6 horas** (5 minutos depois de subir): compras novas no cartão chegam sem
  abrir o app.
- **Hoje** é a data atual em São Paulo (UTC−3 fixo). Testes e capturas usam 10/11/2026.

## Fluxo de caixa e Cartões

- **Fluxo de caixa** ([captura](docs/screenshots/fluxo-de-caixa-1440.png)): lançamentos do mês agrupados
  por dia, do mais recente ao mais antigo, com categoria, pessoa (modo Casal), valor com sinal, situação
  e origem (Banco, Cartão, Manual). Filtros por tipo, origem e categoria, busca por descrição, totais de
  entradas, saídas e resultado da lista e saídas por categoria. "Saldo em conta" soma as contas em reais
  de cada membro; outras moedas aparecem à parte, sem conversão. **Copiar do mês anterior** traz os
  lançamentos manuais sem cartão do mês anterior, não pagos, no mesmo dia (limitado ao fim do mês),
  pulando os que já existem com mesma descrição, valor e pessoa (no modo Individual, só os seus).
  **Previsão das contas fixas**: em meses que ainda não têm uma conta marcada como **fixa** (aluguel,
  condomínio, luz, gás…), o Fluxo mostra a estimativa pela média das últimas três ocorrências, com a
  variação recente ("entre X e Y") e o vencimento típico — um toque em **lançar** cria a conta real.
  No mês atual a previsão também aparece, para as contas que ainda faltam.
- **Cartões** ([captura](docs/screenshots/cartoes-1440.png)): cada cartão conectado (Casal: dos dois;
  Individual: os seus) com limite total/usado/livre, histórico de faturas (marcar como paga) e as compras
  do mês com o total comparado à fatura que vence nele. Clicar numa fatura mostra as compras do mês dela.
  Simplificação: sem data de fechamento, as compras são as do mês civil. O bloco **Parcelas em
  andamento** inclui também parcelas manuais: no formulário de lançamento, marque **Parcelar em
  vários meses** (o valor informado é o de cada parcela, por exemplo um PIX mensal de uma viagem com
  amigos) e o app cria as próximas parcelas nos meses seguintes com " (i/n)" na descrição; para um
  pagamento que já existe, **Marcar como parcela de uma compra** pede o total e o número da parcela
  atual e gera as que faltam.

## Nossa casa, Reserva e Ajustes

- **Nossa casa** ([captura](docs/screenshots/nossa-casa-1440.png)), modo Casal: despesas compartilhadas do mês
  (dia, descrição, categoria, quem pagou, valor; clique para editar), quanto cada um pagou, a parte de cada um
  e o **saldo entre vocês** ("Bruno deve R$ X a Ana" / "Contas equilibradas"). A divisão é **proporcional à
  renda registrada** nos últimos 3 meses (quem ganha mais cobre mais da conta), sem perder centavo; sem
  renda registrada, vale metade para cada um. Em **Ajustes → Gastos compartilhados**, **Remover renda
  compartilhada** desliga tudo relacionado a gasto compartilhado: cada despesa vale por inteiro para quem
  pagou, e as telas de rateio e acertos saem (`PUT /api/settings`, vale para os dois computadores).
  **Registrar acerto** abre valor (já com o saldo), data e quem pagou → quem recebeu e grava em
  `POST /api/settlements`; a lista de acertos permite excluir (com confirmação, `DELETE /api/settlements/{id}`).
  Também mostra a fatia da renda que vai para moradia. No modo Individual a tela vira **Meu orçamento**
  ([captura](docs/screenshots/meu-orcamento-1440.png)): renda, despesas, sobra, despesas por categoria e moradia.
- **Reserva** ([captura](docs/screenshots/reserva-1440.png)): nome, meta, saldo inicial, quanto já foi guardado,
  barra de progresso, aportes dos últimos 12 meses e a projeção "no ritmo atual, meta em N meses" (média dos
  últimos 3 meses, incluindo o selecionado) ou "Sem aportes recentes". **Editar meta** grava em `PUT /api/reserve`.
  Sem meta: "Defina uma meta" ([captura](docs/screenshots/reserva-sem-meta.png)).
- **Ajustes** ([captura](docs/screenshots/ajustes-completo.png)): além de quem usa o computador e do tema,
  **Nomes** (`PUT /api/members/{posição}`, 1 a 40 caracteres), **Contas conectadas** (banco com a cor, dono,
  última atualização em horário de São Paulo e a escolha do banco no catálogo ou "Não informado",
  `PUT /api/connections/{item}/institution`), **Atualizar contas** e como conectar (página `/connect`, cada
  pessoa com o próprio acesso).
- Como nos lançamentos, tudo aparece na hora e é gravado em segundo plano; se falhar, aparece o aviso e os dados
  são recarregados. Acertos criados aqui recebem o UUID devolvido pelo `POST` (excluir logo depois funciona).

## Assistente (GLM via Z.ai)

O peixinho **Flounder** vira consultor financeiro: uma bolha animada (nada, pulsa e mostra um ponto
quando há análise nova) fica no canto da tela; clicar abre o painel com duas abas — **sua análise** e
**a do casal** — e uma conversa livre.

- **Análises**: o backend resume os últimos 4 meses (renda, despesas por mês e categoria, compartilhadas,
  reserva, acertos, parcelas em aberto e contas fixas) e o GLM responde em texto bem-humorado: está
  melhorando ou piorando, o que chama atenção e uma dica prática. As três versões (uma por membro e a do
  casal) são geradas **no backend** quando os dados mudam (hash dos lançamentos) ou uma vez por dia, e na
  hora que você pedir com **Analisar agora** (`POST /api/assistant/refresh`); o texto aparece sozinho quando
  fica pronto.
- **Privacidade**: quem pergunta pelo `APP_TOKEN_1` recebe só a análise de si e a do casal — a individual
  do outro nunca sai do servidor, nem na conversa (`GET /api/assistant/reports` já filtra pelo token).
- **Conversa**: escreva qualquer pergunta; o contexto enviado leva apenas os seus dados e os do casal.
  A resposta chega por `POST /api/assistant/chat` (até 12 mensagens de histórico).
- **Configuração**: `ZAI_TOKEN` no `backend/.env` (ou nas variáveis do servidor), opcional; sem o token
  o agente mostra que está desligado. `ZAI_BASE_URL` (padrão `https://api.z.ai/api/paas/v4`) e
  `ZAI_MODEL` (padrão `glm-5.3`) cobrem outros planos. O texto do modelo chega limpo (sem markdown) e é
  revelado com animação de digitação; `FLOUNDER_REDUCED_MOTION=1` mostra tudo de uma vez.

## Visual e movimento

- Em **Ajustes → Aparência**, escolha **Claro**, **Escuro** ou **Pink**. A escolha é salva localmente
  ao fechar o aplicativo e restaurada na próxima abertura. O painel e os formulários acompanham o tema.
- Arraste a alça de pontos no canto superior direito de um card para reorganizá-lo; solte sobre outro card.
  Arraste o canto inferior direito para alterar largura e altura. Os cards encaixam em uma grade
  com largura e altura em passos da grade, respeitando tamanhos mínimos; calendário e gráfico aproveitam a altura extra.
  O card inteiro acompanha o cursor com uma leve oscilação, e os cards deslizam até o novo encaixe ao soltar.
  Posição e tamanho são salvos localmente junto do tema. Com a alça em foco por Tab, as setas
  também permitem mover (←/→) ou redimensionar (←/→/↑/↓).
- A área de conteúdo mantém margens de 24 px. Os cards usam padding compacto e altura
  automática até um ajuste manual. Três cards podem ocupar uma linha com largura de 1/3,
  respeitando os mínimos dos gráficos e o espaço disponível.
- Tipografia Inter incorporada (Regular, SemiBold e Bold), em pontos lógicos que acompanham o DPI
  da tela. Corpo de 14 pt, legendas a partir de 12 pt e centavos com mínimo de 12 pt.

- Ilustrações 3D: Microsoft Fluent Emoji (MIT), decodificadas na abertura como texturas com mipmap
  (`assets/art/`). Ícones de interface (menu, status, ações) continuam Phosphor, com traço único.
- Movimento só comunica estado, com ease-out cúbico: entrada dos cartões em cascata (40 ms entre eles) a cada
  troca de mês/modo, valores que contam e deslizam, barras que crescem, linha do gráfico que se desenha,
  seleção deslizante no menu e nos seletores, check de pagamento com "pop", hover que eleva o cartão.
- `FLOUNDER_REDUCED_MOTION=1` desliga todas as animações.
- `cargo test --release --test ui -- --ignored animation_frames` grava os quadros em `target/anim/`.

## Backend Pluggy

O serviço privado em [backend/](backend/README.md) usa Rust + Axum, roda no servidor
com dois acessos Bearer e mantém as credenciais Pluggy fora do desktop. Já inclui
Connect Token, contas/cartões, transações v2 e faturas, com isolamento entre clientes.
Para desenvolver aqui, execute `backend/run-local.sh` e abra `http://127.0.0.1:3001/connect`.
As chaves Pluggy ficam no `backend/.env` privado; no servidor passam a variáveis de ambiente.
A página local permite conectar e inspecionar cartões. Os cartões chegam ao desktop pelo
`GET /api/state`, com instituição, cor, bandeira, final, limites e faturas reais: no modo Casal
aparecem os dos dois; no Individual, só os de quem usa o app. As credenciais Pluggy permanecem no backend.

O modelo PostgreSQL (Compose, porta 5433) está em [backend/db/README.md](backend/db/README.md).

## Executar

```sh
backend/run-local.sh     # em outro terminal; o desktop precisa do backend rodando
cargo run --release
```

Requer Rust estável ≥ 1.95. No Linux, para forçar X11 numa sessão Wayland: `WAYLAND_DISPLAY= cargo run --release`.

## Testes

```sh
cargo test                                            # cálculos + interface (sem GPU)
cargo test --release --test ui -- --ignored screenshots   # regera docs/screenshots (precisa de Vulkan/DX12/GL)
```

- `src/model.rs`: saldo previsto, separação pessoal/compartilhado, rateio sem perder centavo, acerto,
  fatura sem dupla contagem, datas, parsing de valores.
- `tests/ui.rs` (egui_kittest + AccessKit): saldo nos modos individual/casal, calendário, evolução, mudança de mês e mês vazio,
  marcar/desmarcar pagamento, cadastro com validação, edição, alternância individual/casal, atalhos e foco por Tab,
  "Quem é você?" (com estado falso, sem rede), cartões conectados por dono e telas com dados vazios.
- `src/backend.rs`: ida e volta `api::Entry` ↔ `Entry` (inclui `card`/`imported`/`recurring`) e leitura do token.
- Parcelas manuais (criar e marcar uma existente), previsão das contas fixas, rateio pela renda com o
  interruptor de Ajustes e telas de Nossa casa/Ajustes com o rateio desligado.
- Backend (`backend/src/tests.rs`, precisa de `TEST_DATABASE_URL`): ajustes do domicílio e contas fixas na
  ida e volta do banco e assistente com um GLM falso — geração única por hash, privacidade das análises
  (só a própria + a do casal) e conversa.
- Fluxo de caixa (filtros, cópia do mês anterior), Cartões (limites, faturas, compras, modo Individual),
  exclusão com confirmação e "Atualizar contas", com backend falso. Capturas das telas novas:
  `cargo test --release --test ui -- --ignored new_screen_screenshots`.
- Nossa casa (saldo acumulado igual no painel, registrar/excluir acerto com os comandos enviados), Reserva
  (projeção, edição da meta, estado vazio) e Ajustes (renomear, banco da conexão), com backend falso.
  Capturas: `cargo test --test ui -- --ignored house_reserve_settings_screenshots` (o perfil release pode
  faltar memória ao linkar; o debug serve).

## Pacotes

- Linux: `scripts/package-linux.sh` → `dist/flounder-0.2.0-linux-x86_64.tar.gz`.
- AppImage: `APPIMAGETOOL=/caminho/appimagetool-x86_64.AppImage scripts/package-appimage.sh` →
  `dist/Flounder-0.2.0-x86_64.AppImage` ([appimagetool 1.9.1](https://github.com/AppImage/appimagetool/releases/tag/1.9.1)).
  Herda a glibc da máquina de build. Em outra máquina, o `flounder.env` fica ao lado do `.AppImage`.
- Windows: `scripts/package-windows.ps1` → `dist/flounder-0.2.0-windows-x86_64.zip` (primeira
  instalação) e o `.exe` solto, que é o que a atualização automática baixa.
- Release: uma tag `v<versão>` dispara o workflow [`.github/workflows/release.yml`](.github/workflows/release.yml),
  que testa nos dois sistemas e publica na GitHub Release a AppImage, o TAR.GZ, o ZIP e o `.exe`.
  A tag precisa bater com a versão do `Cargo.toml`; o CI ([`ci.yml`](.github/workflows/ci.yml))
  continua empacotando a cada push.

## Atualização automática

Ao abrir (e em "Verificar novamente", em Ajustes), o app consulta a última release no GitHub
(`releases/latest`), compara com a própria versão e, se houver uma mais nova, baixa o pacote
desta instalação em segundo plano: `Flounder-<v>-x86_64.AppImage` na AppImage e
`flounder-<v>-windows-x86_64.exe` no Windows. Em "Reiniciar agora", o binário é trocado por
rename (o `flounder.env` ao lado do `.AppImage` não se mexe) e o app reabre sozinho.

Instalações que não se atualizam sozinhas (tar.gz, `cargo run`) recebem só o aviso de versão
nova, com o link da página de releases. Para testar contra um servidor próprio, use
`FLOUNDER_RELEASES_URL` apontando para um JSON no mesmo formato de `releases/latest`.

Windows a partir do Linux, sem MSVC (só DLLs do sistema; o UCRT já vem no Windows 10+):

```sh
rustup target add x86_64-pc-windows-gnu
cargo install cargo-zigbuild --locked   # e o zig 0.13 no PATH
cargo zigbuild --release --locked --target x86_64-pc-windows-gnu
```

O TAR.GZ gerado localmente herda a glibc da máquina de build; use o do CI (glibc 2.35) para distribuir.

Em outra máquina (sem o repositório), coloque ao lado do executável um `flounder.env` só com o
endereço do servidor e o token de quem vai usar — nunca o `backend/.env` inteiro, que tem as chaves
do Pluggy:

```
FLOUNDER_BACKEND_URL=https://finance.felipemayer.com.br
APP_TOKEN_1=…
```

O app procura, nesta ordem: `FLOUNDER_BACKEND_ENV`, o `backend/.env` do projeto e o `flounder.env`
ao lado do executável.

## Estrutura

| Arquivo | Papel |
|---|---|
| `src/assistant.rs` | Agente flutuante: bolha animada, análises (sua + do casal) e conversa com o GLM. |
|---|---|
| `src/model.rs` | Estado e cálculos, sem egui. Dinheiro em centavos (`i64`). Data de hoje. Dados da demonstração (testes). |
| `src/backend.rs` | Cliente HTTP do backend (ureq) numa thread: token, sincronização e conversão `api::Entry` ↔ `Entry`. |
| `src/widgets.rs` | Tema, fontes e componentes repetidos: cartões, valores, indicadores, linhas de conta, navegação, campos. |
| `src/app.rs` | Navegação: menu, cabeçalho do mês, formulário, fatura, atalhos, "Quem é você?" e avisos do backend. |
| `src/dashboard.rs` | Painel equilibrado, calendário, evolução e layout ajustável dos cards. |
| `src/screens.rs` | Telas de Fluxo de caixa, Cartões, Nossa casa e Reserva, janelas de acerto/meta e blocos novos de Ajustes. |
| `assets/fonts/` | Inter 4.1 incorporada ao executável (ver README da pasta). |
| `assets/art/` | Ilustrações 3D Fluent Emoji incorporadas ao executável. |

## Regras de cálculo

- **Casal**: saldo = renda de quem usa o app − despesas pessoais dele − metade de cada despesa compartilhada.
  **Saldo entre vocês** (`Data::house_balance`): acumulado desde o primeiro lançamento até o fim do mês
  selecionado — em cada despesa compartilhada, quem pagou adianta a metade do outro (centavo ímpar com a
  primeira pessoa, `halves`); cada acerto abate o que quem pagou devia. Acumulado, e não por mês, porque é o que
  o casal precisa transferir para zerar; o painel ("Divisão da casa") usa o mesmo valor. Despesas planejadas
  ainda não pagas também contam (o responsável já assumiu a conta).
- **Reserva**: saldo inicial + aportes pagos na categoria Reserva até o mês. No modo Casal soma os aportes dos
  dois (a meta é do domicílio); no Individual, só os seus.
- **Individual**: saldo = renda − tudo que sai da conta de quem usa o app; sem rateio.
- **Cartões**: as compras (manuais ou importadas) entram nos cálculos pela categoria; nas contas a pagar
  aparece só a fatura, uma vez. Simplificação: compras do mês caem na fatura do próprio mês (sem data de fechamento).

## Validação visual

O layout empilha os cartões em janelas menores e mantém a rolagem vertical.
As capturas do painel ficam em `docs/screenshots/painel-*.png`; os testes cobrem
1024 × 700 e 1440 × 900 pontos lógicos, temas claro/escuro/pink e escalas de 100/150/200 %.
