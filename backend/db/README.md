# Banco PostgreSQL

Modelo relacional do Flounder, derivado dos dados do protótipo (`Data::demo` em
`src/model.rs`) e dos retornos da Pluggy. **Estado atual:** o backend usa o banco
(`DATABASE_URL`): aplica as migrações ao iniciar, importa da Pluggy contas, cartões, faturas
e transações (**Carregar cartões** e `POST /api/sync`), transformando transações em
lançamentos, e serve `GET /api/state`, o CRUD de `/api/entries`, acertos, reserva, nomes dos
membros e o banco de cada conexão (contrato em `shared/api.rs`, detalhes em
[../README.md](../README.md)). O desktop lê e grava só por esses endpoints, nunca direto no
PostgreSQL. Preferências ainda não têm endpoint de escrita.

## Subir

```sh
docker compose up -d postgres          # projeto "flounder", porta 127.0.0.1:5433
docker compose ps                      # aguarde "healthy"
```

- A senha fica em `.env` na raiz (permissão 600, ignorado pelo Git). Use `.env.example`
  como base numa máquina nova e gere uma senha aleatória. Não publique a saída de
  `docker compose config`, que mostra a senha.
- Os dados ficam no volume nomeado `flounder_postgres_data`. `docker compose down` preserva
  o volume; **nunca** use `down -v` sem um backup.
- **Migrações:** o backend embute `backend/migrations/*.sql` e, ao iniciar, aplica em ordem
  as versões ausentes de `finance.schema_migrations` (fonte de verdade). Cada arquivo roda
  numa transação e registra a própria versão. Num volume novo o Compose também as executa
  (`docker-entrypoint-initdb.d`). Nova mudança = novo `00N_*.sql` + entrada em
  `MIGRATIONS` (`backend/src/db.rs`); não edite migrações já aplicadas.
- `001_initial.sql`: esquema e catálogos (idempotente).
- `002_household_and_imported_cards.sql`: cria o domicílio real com Felipe (posição 0,
  `client-1`) e Larissa (posição 1, `client-2`) se nenhum membro usa esses clientes, e
  acrescenta em `cards` as colunas que faltavam para o cartão importado
  (`shared/cards.rs`): `institution_name`/`color_hex` (só quando o banco não corresponde a
  `institutions`), `due_date` (próximo vencimento) e `bills_available`. Moeda ausente ou
  fora do ISO é gravada como `XXX` e volta como "Moeda não informada".
- `003_sync_and_settlements.sql`: tabela `settlements` (acertos do casal: data,
  `amount_cents > 0`, `from_id`/`to_id` membros do mesmo domicílio e diferentes);
  `bank_accounts.institution_key/institution_name/color_hex` (banco detectado da conta
  corrente, como em `cards`); `bank_connections.connector_name`; meta de reserva única por
  domicílio (índice único em `reserve_goals.household_id`); FKs refeitas: excluir conta apaga
  as suas transações (`CASCADE`), excluir fatura, lançamento ou cartão só desfaz o vínculo
  (`SET NULL` da coluna, PostgreSQL 15+). Lançamento excluído deixa a transação com
  `entry_id` NULL, e como lançamentos só nascem na primeira vez que a transação aparece, ela
  não volta (sem coluna "dismissed"). `Entry.imported` é derivado do vínculo, sem coluna.

## Bancos

| Banco | Conteúdo |
| --- | --- |
| `flounder` | Uso real: esquema, catálogos, domicílio Felipe/Larissa e o que o app gravar. Sem dados fictícios. |
| `flounder_demo` | Dados do protótipo (2 membros, 2 cartões, 104 lançamentos, 12 faturas, reserva). |

```sh
backend/db/seed-demo.sh   # cria/atualiza flounder_demo; idempotente (ON CONFLICT DO NOTHING)
docker compose exec -T postgres psql -X -q -v ON_ERROR_STOP=1 -U flounder -d flounder_demo < backend/db/verify.sql
```

O seed é gerado por `examples/export_demo.rs` a partir do próprio `Data::demo`, então não
há uma segunda cópia dos mocks. O script aplica a 001, o seed e depois as demais migrações;
como os membros demo (Lucas/Marina) já usam `client-1`/`client-2`, a 002 não cria o
domicílio real ali, e um backend apontado para `flounder_demo` mostra os dados demo. `verify.sql` roda numa transação com `ROLLBACK`: confere os
totais do seed e tenta inserir dados inválidos (domicílio cruzado, valor manual negativo,
vencimento dia 32, final com 8 dígitos, conta BANK usada como cartão, transação externa
duplicada, conta de outro dono), que precisam falhar.

## Modelo

```mermaid
erDiagram
    households ||--|{ members : "até 2 (posição 0/1)"
    members ||--o| preferences : tema/modo/layout
    members ||--o{ bank_connections : "dono do item Pluggy"
    institutions |o--o{ bank_connections : "banco confirmado"
    bank_connections ||--|{ bank_accounts : "BANK / CREDIT"
    bank_accounts |o--o| cards : "só CREDIT do mesmo dono"
    institutions |o--o{ cards : cor
    members ||--o{ cards : titular
    cards ||--o{ card_invoices : "por vencimento"
    members ||--o{ entries : lançamentos
    categories ||--o{ entries : categoria
    cards |o--o{ entries : "compra no cartão"
    bank_accounts ||--o{ bank_transactions : importadas
    card_invoices |o--o{ bank_transactions : fatura
    entries |o--o| bank_transactions : "conciliação"
    members ||--o{ reserve_goals : "reserva (uma por domicílio)"
    members ||--o{ settlements : "acerto de/para"
```

| Protótipo (`src/model.rs`) | Tabela |
| --- | --- |
| membros Lucas/Marina | `households`, `members` (`client_user_id` = cliente do backend) |
| `Entry` (renda, despesa, compartilhada, cartão, paga) | `entries` |
| `Card` demo / cartão Pluggy | `cards` (+ `bank_accounts` quando importado) |
| `Invoice` | `card_invoices` (`source` = `manual` ou `pluggy`) |
| reserva | `reserve_goals` |
| tema, modo casal/individual, grade do dashboard | `preferences` |
| banco escolhido da conexão (antes `institutions.json`) | `bank_connections.institution_key`, precedência sobre `cards`/`bank_accounts.institution_key` na leitura |
| `ConnectedCard` (`shared/cards.rs`) | `cards` + `bank_accounts` (id/saldo/atualização) + `bank_connections` (item) + `card_invoices` |
| `Account` / `Connection` / `Institution` (`shared/api.rs`) | `bank_accounts` (`kind = 'BANK'`) / `bank_connections` / `institutions` |
| `Settlement` | `settlements` |
| `Entry.card` / `Entry.imported` | `entries.card_id` → `cards` → `bank_accounts.external_id` / existe `bank_transactions.entry_id` |
| `Reserve` (com `name`) | `reserve_goals` (uma por domicílio) |

Decisões:

- Valores em `bigint` centavos com moeda ISO de 3 letras; sem conversão de câmbio.
  Lançamentos manuais são `>= 0` (o sinal vem de `kind`); transações importadas aceitam
  negativos para estornos.
- Transações importadas (`bank_transactions`) ficam separadas dos lançamentos (`entries`);
  cada uma gera no máximo um lançamento (vínculo `entry_id`, editável pelo usuário). Pagamento
  de fatura, transferência própria, estorno e movimentos de investimento ficam sem lançamento
  (regras em [../README.md](../README.md)); `invoice_id` liga a compra à fatura pelo `billId`.
- Totais, rateio da casa, saldo previsto e gráficos **não** são persistidos: são
  calculados como hoje, a partir dos lançamentos.
- FKs compostas `(household_id, owner_id, …)` impedem referenciar membro, cartão ou conta
  de outro domicílio ou de outro dono. Não há RLS: todo endpoint filtra pelo domicílio do
  membro cujo `client_user_id` é o cliente autenticado.
- Só os 4 últimos dígitos do cartão; nada de CPF, nome do titular ou credenciais.

## Servidor

Em `/opt/docker/finance` no servidor (`ssh servidor`), com o código do repositório:

- `.env` (600): `POSTGRES_PASSWORD` próprio do servidor e `POSTGRES_PORT=5433` (só loopback).
- `backend/.env` (600): `CLIENT_ID`, `CLIENT_SECRET`, `APP_TOKEN_1`, `APP_TOKEN_2` (os mesmos do
  desktop), `DATABASE_URL=host=127.0.0.1 port=5433 …` e `BIND_ADDR=<IP Tailscale do servidor>:3021`.
- `docker compose --profile server up -d --build`: PostgreSQL e backend. O backend usa a rede do
  host e escuta só no IP Tailscale, sem exposição à internet nem à rede local.

No desktop, `FLOUNDER_BACKEND_URL=http://<IP Tailscale>:3021` no `backend/.env` local aponta o app
para o servidor (os tokens continuam sendo lidos desse arquivo).

Atualizar: enviar o código novo (`git archive HEAD | ssh servidor tar -x -C /opt/docker/finance`)
e repetir o `up -d --build`; as migrações rodam na inicialização. Backup:
`docker compose exec -T postgres pg_dump -U flounder -d flounder -n finance > backup.sql`.
