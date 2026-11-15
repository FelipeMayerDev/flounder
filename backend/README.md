# Backend privado do Flounder

Serviço Rust + Axum para desenvolvimento local e para rodar no servidor da casa. São dois acessos fixos,
identificados por `client-1` e `client-2`, cada um com seu próprio token Bearer.
O desktop continua sendo Rust/egui; este serviço é um executável separado.

## Rodar aqui na máquina

```sh
backend/run-local.sh
```

Abra **http://127.0.0.1:3001/connect**. O modo local cria `backend/.env` com
permissões privadas e dois tokens aleatórios de cliente quando o arquivo ainda
não existe. Arquivos existentes são preservados. Preencha `CLIENT_ID` e
`CLIENT_SECRET` com suas credenciais Pluggy e `DATABASE_URL` com o PostgreSQL do
Compose (`host=127.0.0.1 port=5433 user=flounder dbname=flounder password=…`, a senha é
`POSTGRES_PASSWORD` do `.env` da raiz; veja [db/README.md](db/README.md)); não envie
esses valores no chat. Sem `DATABASE_URL` válida o backend não inicia.

Na página, use o **valor** de `APP_TOKEN_1` ou `APP_TOKEN_2` para identificar o
cliente. Eles são chaves privadas do Flounder, diferentes da API Key da Pluggy.
A API Key da Pluggy é gerada e renovada pelo backend, não precisa entrar no `.env`.

Sem as credenciais, o backend inicia e mostra `pluggyConfigured: false` em `/health`;
as consultas Pluggy retornam 503. Nesse estado não há dados falsos de banco.
Após preencher o arquivo, reinicie o serviço para carregar a configuração.

### Importação para o PostgreSQL

**Carregar cartões** (`GET /api/items/{itemId}/accounts`) e **Sincronizar** (`POST /api/sync`)
usam a mesma importação: consultam contas, faturas e transações (API v2, por cursor) e gravam
tudo numa única transação, como do membro dono da conexão (`client-1` → Felipe, `client-2` →
Larissa): `bank_connections` (o item), `bank_accounts` (`BANK` e `CREDIT`), `cards`,
`card_invoices` (`source = 'pluggy'`, com o id da fatura) e `bank_transactions`. Apenas o
final do número do cartão é salvo; não são salvos CPF, nome do titular, credenciais nem o
número do cartão das transações. Consultar outra instituição acrescenta as suas contas;
recarregar a mesma conexão atualiza sem duplicar e remove contas, cartões e faturas que a
Pluggy não devolve mais (se a Pluggy não devolver conta nenhuma, nada é removido). Um item
já gravado para o outro membro é recusado (409), nunca reatribuído.

#### Transações → lançamentos

Só entram transações dos últimos 12 meses, sem duplicar (id externo da Pluggy). Na
**primeira vez** que uma transação aparece ela vira um lançamento (`Entry`) realizado:
`paid = true`, `imported = true`, dono = dono da conexão, não compartilhado, compras do cartão
com `card` (pesam nas despesas; a conta a pagar continua sendo a fatura). Parcelas ganham
" (n/N)" na descrição quando ela ainda não traz. Nas importações seguintes só a linha de
`bank_transactions` é atualizada: categoria, valor ou descrição editados no lançamento são
preservados, e um importado excluído não volta (a transação fica sem vínculo). Limitações:
mudança de valor de uma transação pendente que é lançada com o mesmo id não chega ao
lançamento; uma pendente que some da Pluggy (cancelada ou relançada com outro id) é apagada
com o seu lançamento, mesmo editado. A direção vem de `type` (`DEBIT` = saída, `CREDIT` =
entrada), não do sinal, que no cartão é invertido (compra positiva, pagamento negativo).

Não viram lançamento (ficam só em `bank_transactions`), pelo `categoryId` da Pluggy:

| Transação | Regra |
|---|---|
| Crédito no cartão (pagamento da fatura, estorno, crédito contratado) | conta `CREDIT` com `type = CREDIT` |
| Pagamento da fatura na conta corrente | `05100000`, ou categoria `05…` com "fatura" na descrição (o Nubank usa `05000000` "Pagamento de fatura") |
| Transferência entre contas do mesmo titular | `04…` |
| Resgate, rendimento ou ajuste de margem de investimento | `03…` de entrada, `03050000`, `03060000` |
| Empréstimo ou financiamento recebido | `02…` de entrada |

Aplicação em investimento (`03…` de saída) vira despesa na categoria `reserve`; resgates não
abatem a reserva. Estornos não abatem a compra original. Transferências entre o casal entram
como despesa de um e renda do outro (registre o acerto em `/api/settlements` ou exclua).

| Pluggy (`categoryId`, saída) | Categoria |
|---|---|
| `17020…` água/luz/gás, `07010…` telecom | `utilities` |
| `17…` moradia, `20020…` seguro residencial | `housing` |
| `10…` supermercado | `groceries` |
| `19…` transporte, `20040…` seguro de veículo | `transport` |
| `18…` saúde, `20030…` plano de saúde, `07030…` bem-estar | `health` |
| `11…` restaurantes/delivery, `07040…` ingressos, `09…` digitais, `12…` viagens, `14…` apostas, `21…` lazer | `leisure` |
| `06020…` pensão alimentícia, `08050…` artigos infantis | `family` |
| demais | `other` |

Entradas: `01…` → `salary` (Renda); demais → `other`.

#### Sincronizar

`POST /api/sync` reimporta todas as conexões do domicílio, dos dois membros (o backend guarda
as credenciais da Pluggy; o escopo vem só do banco, nunca da requisição). Cada item é
consultado como o seu dono. Uma conexão que falha é contada em `failed` e não interrompe as
outras; responde `{"synced": n, "failed": m}`. Uma sincronização por vez: chamadas
simultâneas esperam a anterior. Pode levar dezenas de segundos.

O desktop lê os cartões dos dois membros por `GET /api/state` (campo `owner`). O antigo
resumo `.local/cards-client-N.json` não é mais gravado; arquivos existentes podem ser
apagados à mão. Os testes usam apenas dados fictícios e bancos descartáveis.

A instituição e a cor vêm do conector ou dos nomes informados na conta: Nubank usa
roxo, Inter laranja etc. Nomes desconhecidos mantêm a cor do conector ou uma cor neutra.
O MeuPluggy genérico não é tratado como se fosse o banco emissor. Quando o retorno não
traz o emissor (ex.: conta `platinum` via MeuPluggy), escolha o banco da conexão no app
(`PUT /api/connections/{itemId}/institution`): fica em `bank_connections.institution_key`,
tem precedência sobre o detectado para cartões e contas da conexão e vale na hora.
O antigo `.local/institutions.json` (id do item → nome do banco; só no modo `--local`) é
copiado para a conexão na importação quando ela ainda não tem banco escolhido; depois disso
pode ser apagado (se continuar lá, volta a preencher uma conexão cujo banco foi limpo).
O nome é associado sem diferenciar maiúsculas a `finance.institutions`; sem correspondência,
o cartão guarda o próprio nome e cor. Campos ausentes
aparecem como não informados, e o saldo utilizado não é apresentado como uma fatura.
Faturas são selecionadas pelo mês de vencimento; moedas não são convertidas para reais.

Os dados bancários só mudam após **Carregar cartões** ou `POST /api/sync`; ler
`/api/state` não força sincronização com a Pluggy.

### Contas reais para uso pessoal: Meu Pluggy

Este projeto é pessoal. Cadastre-se em [meu.pluggy.ai](https://meu.pluggy.ai) e conecte
seus bancos ali. Depois use **Autorizar Meu Pluggy** na página local: ela seleciona
o conector **MeuPluggy (200)**, com `clientUserId` vinculado ao cliente autenticado.
Esse conector é um proxy dos consentimentos que você já deu no portal pessoal.

Se não aparecer, entre no Dashboard com a mesma conta e confira
**Customização → Conectores → Pessoal → Direct Connectors → MeuPluggy**.
Não selecione o banco diretamente no Dashboard para esse fluxo pessoal.

A documentação oficial informa que o acesso aos próprios dados por esse caminho é
gratuito, com até cinco conexões e sem a aprovação comercial exigida para empresas.
[Guia de uso pessoal](https://docs.pluggy.ai/pt/docs/guides/meu-pluggy-personal-use).
A opção sandbox permanece disponível para testes e não representa contas reais.

Guarde os `itemId`s: a listagem `GET /v2/items` não está disponível nesse plano.
A sincronização acontece no Meu Pluggy a cada 24 horas; os itens proxy refletem as
atualizações e não aceitam atualização manual. A autorização do aplicativo expira
após um ano e deve ser renovada pelo conector MeuPluggy.

Itens criados pelo Demo do Dashboard sem `clientUserId` continuam sem acesso pelo
backend para preservar o isolamento dos dois clientes. Prefira autorizar pelo
widget local, que já vincula o identificador do cliente ao Connect Token.

O Pluggy Connect abre pelo widget oficial no navegador. Ao concluir o consentimento,
a página recebe `itemId`, lista os cartões e permite consultar limites, transações e
faturas em JSON. É possível informar um `itemId` existente do mesmo cliente. O botão
de próximas transações usa o cursor da API v2. O JSON completo fica na memória da página;
somente o resumo dos cartões e faturas é gravado no banco. Dados financeiros não são
enviados para logs (erros do PostgreSQL registram só o código SQLSTATE). O token do cliente não é guardado
em `localStorage` e é removido ao fechar/recarregar a página.

O arquivo aceita `NOME=valor` ou valores entre aspas, literalmente, sem executar
comandos ou expandir variáveis. Variáveis de ambiente do processo têm prioridade.
Variáveis extras do arquivo são ignoradas. O modo `--local` só aceita loopback.

## Portar para o servidor

O mesmo executável roda sem `--local`, lendo somente as variáveis de ambiente e
exigindo credenciais Pluggy e `DATABASE_URL` válidas na configuração. Ao iniciar, aplica
as migrações pendentes de `backend/migrations/` (embutidas no executável). Configure o serviço e o proxy
HTTPS; não é preciso mudar as rotas ou trocar Rust por outra linguagem.

## Configuração

Preencha `CLIENT_ID` e `CLIENT_SECRET` reais **somente nas variáveis de ambiente do servidor**.
Use `backend/.env.example` como referência; não envie os valores no chat nem os inclua
no executável desktop, no navegador ou no Git. No modo de servidor, o programa lê apenas `std::env`. O arquivo local só é lido com `--local`.

| Variável | Uso |
|---|---|
| `CLIENT_ID` | Credencial da aplicação Pluggy |
| `CLIENT_SECRET` | Segredo da aplicação Pluggy |
| `APP_TOKEN_1` | Token aleatório exclusivo do primeiro cliente |
| `APP_TOKEN_2` | Token aleatório exclusivo do segundo cliente |
| `BIND_ADDR` | Endereço HTTP interno; padrão `127.0.0.1:3001` |
| `DATABASE_URL` | PostgreSQL, no formato `host=… port=… user=… password=… dbname=…` ou `postgres://…`; sem TLS, use loopback ou rede privada |

Gere **dois** tokens distintos com pelo menos 32 caracteres. Por exemplo, execute
`openssl rand -hex 32` para cada um. Cada instalação recebe apenas seu próprio
`APP_TOKEN`, e nunca as credenciais da Pluggy ou o token do outro cliente.

Configure essas variáveis pelo gerenciador de serviços/containers do servidor.
Um `EnvironmentFile` do systemd deve ser legível apenas pelo administrador/usuário do serviço.
Coloque o HTTP interno atrás do proxy HTTPS do servidor; o endereço loopback padrão
impede exposição direta pela rede.

```sh
cargo test --manifest-path backend/Cargo.toml --locked
# Testes com PostgreSQL: cada execução cria e apaga bancos flounder_test_*; sem a
# variável eles passam sem rodar (aviso visível com -- --nocapture).
TEST_DATABASE_URL="host=127.0.0.1 port=5433 user=flounder dbname=postgres password=$(grep ^POSTGRES_PASSWORD= .env | cut -d= -f2-)" \
  cargo test --manifest-path backend/Cargo.toml --locked
cargo build --manifest-path backend/Cargo.toml --release --locked
# Após configurar o ambiente do processo:
./backend/target/release/flounder-backend
```

O backend não precisa das bibliotecas gráficas do desktop. A API Key da Pluggy é reutilizada em memória,
renovada antes da expiração e novamente autenticada se a Pluggy responder 401.

## Endpoints

Todas as rotas `/api/` exigem `Authorization: Bearer <APP_TOKEN_DO_CLIENTE>`.
Respostas usam `Cache-Control: no-store`. `GET /health` só informa se o serviço está ativo.

| Método e rota | Retorno |
|---|---|
| `POST /api/connect-token` | `{ "accessToken": "..." }` para abrir Pluggy Connect |
| `GET /api/items/{itemId}/accounts` | Contas da conexão; importa contas, cartões, faturas e transações para o banco |
| `GET /api/accounts/{accountId}` | Detalhes da conta/cartão, incluindo `creditData` |
| `GET /api/accounts/{accountId}/transactions` | Página de transações da API v2, com `results` e `next` |
| `GET /api/accounts/{accountId}/bills` | Faturas, quando disponíveis para o conector |
| `GET /api/state` | `State` de `shared/api.rs`: `me`, membros, lançamentos, reserva, cartões, contas, conexões, catálogo de bancos e acertos do domicílio |
| `POST /api/sync` | Reimporta todas as conexões do domicílio; `SyncResult` (200). 503 sem credenciais Pluggy |
| `POST /api/entries` | Cria um lançamento (`Entry` sem `id`); devolve o salvo (200) |
| `PUT /api/entries/{id}` | Substitui um lançamento do domicílio; devolve o salvo (200), 404 fora do domicílio |
| `DELETE /api/entries/{id}` | Remove; 204. Importado excluído não volta |
| `PUT /api/card-bills/paid` | Marca/desmarca a fatura importada (`BillPaid`: cartão, vencimento, paga); 204. Preservado ao recarregar |
| `POST /api/settlements` | Registra um acerto (`Settlement` sem `id`); devolve o salvo (200) |
| `DELETE /api/settlements/{id}` | Remove o acerto; 204, 404 fora do domicílio |
| `PUT /api/reserve` | Cria ou substitui a meta única de reserva (`Reserve`); dono = quem salvou; 204 |
| `PUT /api/members/{position}` | Renomeia o membro 0/1 (`MemberName`); 204, 404 para posição inexistente |
| `PUT /api/connections/{itemId}/institution` | Escolhe o banco da conexão (`ConnectionInstitution`, chave de `institutions` ou `null`); 204, 404 fora do domicílio |

### Domicílio e lançamentos

O contrato está em `shared/api.rs`. Tudo é limitado ao domicílio do membro autenticado:
os dois membros veem e editam os mesmos lançamentos e cartões. Validações (400, mensagem
em `error`): descrição não vazia de até 200 caracteres, `amount_cents >= 0`, data
`AAAA-MM-DD` válida, `kind` `income` ou `expense`, categoria existente em
`finance.categories`, receita não compartilhada e `owner` com posição de um membro do
domicílio, `card` (id de um cartão do domicílio, só em despesas). `imported` do corpo é
ignorado. `id` que não seja UUID de um lançamento do domicílio responde 404, sem revelar
se existe em outro. Um token sem membro cadastrado no banco recebe 403.

Outras validações (400): acerto com data válida, `amount_cents > 0` e `from`/`to` membros
diferentes do domicílio; reserva com valores `>= 0` e nome de até 60 caracteres; nome de
membro com 1 a 40 caracteres; banco da conexão existente em `institutions`.

### Connect Token

```sh
curl -X POST 'https://SEU_BACKEND/api/connect-token' \
  -H "Authorization: Bearer $APP_TOKEN_1" \
  -H 'Content-Type: application/json' \
  -d '{}'
```

O backend envia à Pluggy `{"options":{"clientUserId":"client-1"}}` ou `client-2`,
conforme o token autenticado. `clientUserId` é opcional na requisição; quando enviado,
precisa corresponder ao cliente autenticado. O servidor impede a troca de identidade.
A resposta pública contém somente `accessToken`, sem `apiKey`, `CLIENT_ID` ou `CLIENT_SECRET`.

Use o token curto no Pluggy Connect. Após o consentimento do titular,
o widget devolve `itemId` no `onSuccess`; use esse ID para consultar as contas.

```sh
curl 'https://SEU_BACKEND/api/items/SEU_ITEM_ID/accounts' \
  -H "Authorization: Bearer $APP_TOKEN_1"
```

Antes de retornar informações, o servidor consulta o item na Pluggy e verifica
`clientUserId`. Para contas/transações/faturas, também verifica a ligação `account.itemId`.
IDs de outro cliente ou de conexões antigas sem `clientUserId` não dão acesso aos dados.
O compartilhamento financeiro entre os dois clientes não é automático.

### Paginação

Faturas aceitam `?page=1&pageSize=500`, com `page >= 1` e `1 <= pageSize <= 500`.
Transações usam `/v2/transactions` na Pluggy, por cursor. Continue anexando o `next`
recebido à rota local de transações, ou envie apenas seu parâmetro `after` decodificado.
Não use a string inteira de `next` como valor de `after`.

O endpoint antigo `/transactions` da Pluggy está descontinuado; esta integração já usa v2.
Os campos e respostas de paginação são preservados no JSON para investigar os retornos, sem converter
valores, sinais ou moedas para o modelo financeiro da demonstração.

## O que observar nos retornos de cartão

| Campo | O que investigar |
|---|---|
| `type: CREDIT`, `subtype: CREDIT_CARD` | Identificação de conta de cartão |
| `creditData.creditLimit`, `availableCreditLimit` | Limites total e disponível |
| `creditData.balanceDueDate`, `balanceCloseDate` | Vencimento e fechamento, quando informados; o dia do mês de ambos já é lido e guardado no cartão (`due_day` e `closing_day`), e a fatura guarda a data de fechamento (`closeDate`/`summary`) |
| `balance` | Semântica depende do conector; em Open Finance pode representar limite utilizado |
| `bills.results[].totalAmount`, `dueDate`, `payments` | Valores, vencimentos e pagamentos das faturas |
| `transactions.results[].creditCardMetadata` | Parcelas e metadados específicos do cartão |
| `currencyCode` / `totalAmountCurrencyCode` | Moeda de cada valor |

Consulte primeiro o Sandbox. A disponibilidade das faturas e de alguns campos depende
do conector e do consentimento. As transações devem ser importadas/sincronizadas;
estes endpoints são para exploração inicial, não para consulta a cada frame do desktop.

A página `/connect` já permite abrir o widget e inspecionar dados reais após o consentimento.
Os testes automatizados usam uma API Pluggy simulada, sem acessar suas credenciais reais.

## Referências oficiais

- [Autenticação e options.clientUserId](https://v2.docs.pluggy.ai/en/docs/authentication)
- [Contas e creditData](https://v2.docs.pluggy.ai/en/docs/products/accounts)
- [Transações v2 e cursor](https://v2.docs.pluggy.ai/en/reference/transaction/transactions-list-by-cursor)
- [Faturas](https://v2.docs.pluggy.ai/en/reference/bill/bills-list)
