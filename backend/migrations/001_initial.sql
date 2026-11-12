BEGIN;
CREATE SCHEMA IF NOT EXISTS finance;
SET LOCAL search_path TO finance, public;

CREATE TABLE IF NOT EXISTS schema_migrations (
    version integer PRIMARY KEY,
    applied_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS households (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    name text NOT NULL CHECK (length(trim(name)) > 0),
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS members (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    household_id uuid NOT NULL REFERENCES households(id),
    client_user_id text NOT NULL UNIQUE,
    display_name text NOT NULL CHECK (length(trim(display_name)) > 0),
    position smallint NOT NULL CHECK (position IN (0, 1)),
    UNIQUE (household_id, position),
    UNIQUE (household_id, id)
);

CREATE TABLE IF NOT EXISTS categories (
    key text PRIMARY KEY,
    label text NOT NULL
);
INSERT INTO categories(key, label) VALUES
    ('salary', 'Renda'), ('housing', 'Moradia'), ('utilities', 'Contas da casa'),
    ('groceries', 'Mercado'), ('transport', 'Transporte'), ('health', 'Saúde'),
    ('leisure', 'Lazer'), ('reserve', 'Reserva'), ('family', 'Família'), ('other', 'Outros')
ON CONFLICT DO NOTHING;

CREATE TABLE IF NOT EXISTS institutions (
    key text PRIMARY KEY,
    name text NOT NULL,
    color_hex text NOT NULL CHECK (color_hex ~ '^[0-9a-fA-F]{6}$')
);
INSERT INTO institutions(key, name, color_hex) VALUES
    ('nubank', 'Nubank', '820ad1'), ('inter', 'Inter', 'dc5300'),
    ('itau', 'Itaú', 'cb4900'), ('santander', 'Santander', 'be0000'),
    ('bradesco', 'Bradesco', 'b00c36'), ('c6', 'C6 Bank', '28282b'),
    ('bb', 'Banco do Brasil', '0f3189'), ('caixa', 'Caixa', '00568f'),
    ('picpay', 'PicPay', '00683e'), ('mercadopago', 'Mercado Pago', '006392')
ON CONFLICT DO NOTHING;

CREATE TABLE IF NOT EXISTS bank_connections (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    household_id uuid NOT NULL,
    owner_id uuid NOT NULL,
    external_id text NOT NULL UNIQUE,
    institution_key text REFERENCES institutions(key),
    connector_id integer,
    status text,
    synced_at timestamptz,
    FOREIGN KEY (household_id, owner_id) REFERENCES members(household_id, id),
    UNIQUE (household_id, owner_id, id)
);

CREATE TABLE IF NOT EXISTS bank_accounts (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    household_id uuid NOT NULL,
    owner_id uuid NOT NULL,
    connection_id uuid NOT NULL,
    external_id text NOT NULL UNIQUE,
    kind text NOT NULL CHECK (kind IN ('BANK', 'CREDIT')),
    name text NOT NULL,
    currency text NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    balance_cents bigint,
    updated_at timestamptz,
    FOREIGN KEY (household_id, owner_id, connection_id) REFERENCES bank_connections(household_id, owner_id, id),
    UNIQUE (household_id, id),
    UNIQUE (household_id, owner_id, id, kind)
);

CREATE TABLE IF NOT EXISTS cards (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    household_id uuid NOT NULL,
    owner_id uuid NOT NULL,
    account_id uuid UNIQUE,
    account_kind text NOT NULL DEFAULT 'CREDIT' CHECK (account_kind = 'CREDIT'),
    name text NOT NULL,
    institution_key text REFERENCES institutions(key),
    last_four text CHECK (last_four ~ '^[0-9]{4}$'),
    brand text,
    currency text NOT NULL DEFAULT 'BRL' CHECK (currency ~ '^[A-Z]{3}$'),
    due_day smallint CHECK (due_day BETWEEN 1 AND 31),
    credit_limit_cents bigint CHECK (credit_limit_cents >= 0),
    available_limit_cents bigint,
    FOREIGN KEY (household_id, owner_id) REFERENCES members(household_id, id),
    FOREIGN KEY (household_id, owner_id, account_id, account_kind) REFERENCES bank_accounts(household_id, owner_id, id, kind),
    UNIQUE (household_id, id)
);

CREATE TABLE IF NOT EXISTS card_invoices (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    household_id uuid NOT NULL,
    card_id uuid NOT NULL,
    external_id text UNIQUE,
    due_date date NOT NULL,
    closing_date date,
    currency text NOT NULL DEFAULT 'BRL' CHECK (currency ~ '^[A-Z]{3}$'),
    total_cents bigint,
    minimum_payment_cents bigint CHECK (minimum_payment_cents >= 0),
    paid boolean NOT NULL DEFAULT false,
    source text NOT NULL DEFAULT 'manual' CHECK (source IN ('manual', 'pluggy')),
    FOREIGN KEY (household_id, card_id) REFERENCES cards(household_id, id),
    UNIQUE (card_id, due_date, currency),
    UNIQUE (household_id, id)
);

CREATE TABLE IF NOT EXISTS entries (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    household_id uuid NOT NULL,
    owner_id uuid NOT NULL,
    description text NOT NULL CHECK (length(trim(description)) > 0),
    amount_cents bigint NOT NULL CHECK (amount_cents >= 0),
    currency text NOT NULL DEFAULT 'BRL' CHECK (currency ~ '^[A-Z]{3}$'),
    entry_date date NOT NULL,
    kind text NOT NULL CHECK (kind IN ('income', 'expense')),
    category_key text NOT NULL REFERENCES categories(key),
    shared boolean NOT NULL DEFAULT false,
    card_id uuid,
    paid boolean NOT NULL DEFAULT false,
    FOREIGN KEY (household_id, owner_id) REFERENCES members(household_id, id),
    FOREIGN KEY (household_id, card_id) REFERENCES cards(household_id, id),
    CHECK (kind = 'expense' OR (NOT shared AND card_id IS NULL)),
    UNIQUE (household_id, id)
);

CREATE TABLE IF NOT EXISTS bank_transactions (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    household_id uuid NOT NULL,
    account_id uuid NOT NULL,
    external_id text NOT NULL UNIQUE,
    description text NOT NULL,
    amount_cents bigint NOT NULL,
    currency text NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    transaction_date date NOT NULL,
    status text NOT NULL CHECK (status IN ('PENDING', 'POSTED')),
    credit_card_metadata jsonb NOT NULL DEFAULT '{}' CHECK (jsonb_typeof(credit_card_metadata) = 'object'),
    invoice_id uuid,
    entry_id uuid UNIQUE,
    updated_at timestamptz,
    FOREIGN KEY (household_id, account_id) REFERENCES bank_accounts(household_id, id),
    FOREIGN KEY (household_id, invoice_id) REFERENCES card_invoices(household_id, id),
    FOREIGN KEY (household_id, entry_id) REFERENCES entries(household_id, id)
);

CREATE TABLE IF NOT EXISTS reserve_goals (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    household_id uuid NOT NULL,
    owner_id uuid NOT NULL,
    name text NOT NULL,
    opening_balance_cents bigint NOT NULL DEFAULT 0 CHECK (opening_balance_cents >= 0),
    target_cents bigint NOT NULL CHECK (target_cents >= 0),
    FOREIGN KEY (household_id, owner_id) REFERENCES members(household_id, id)
);

CREATE TABLE IF NOT EXISTS preferences (
    member_id uuid PRIMARY KEY REFERENCES members(id),
    theme text NOT NULL DEFAULT 'light' CHECK (theme IN ('light', 'dark', 'pink')),
    mode text NOT NULL DEFAULT 'couple' CHECK (mode IN ('couple', 'individual')),
    dashboard_layout jsonb NOT NULL DEFAULT '{}' CHECK (jsonb_typeof(dashboard_layout) = 'object')
);

CREATE INDEX IF NOT EXISTS entries_household_date ON entries(household_id, entry_date);
CREATE INDEX IF NOT EXISTS entries_owner_date ON entries(owner_id, entry_date);
CREATE INDEX IF NOT EXISTS entries_card_date ON entries(card_id, entry_date) WHERE card_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS invoices_card_due ON card_invoices(card_id, due_date);
CREATE INDEX IF NOT EXISTS transactions_account_date ON bank_transactions(account_id, transaction_date);
CREATE INDEX IF NOT EXISTS connections_owner ON bank_connections(owner_id);
INSERT INTO schema_migrations(version) VALUES (1) ON CONFLICT DO NOTHING;
COMMIT;
