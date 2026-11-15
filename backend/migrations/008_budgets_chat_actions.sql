BEGIN;
SET LOCAL search_path TO finance, public;

-- Orçamentos mensais por categoria de despesa, do domicílio (sem dono). Valor <= 0 não
-- existe: o app remove a linha em vez de gravar zero.
CREATE TABLE IF NOT EXISTS budgets (
    household_id uuid NOT NULL REFERENCES households(id),
    category_key text NOT NULL REFERENCES categories(key),
    monthly_cents bigint NOT NULL CHECK (monthly_cents >= 0),
    UNIQUE (household_id, category_key)
);

-- Conversa persistida com o assistente: uma linha por mensagem, por membro (o histórico de
-- um não é o do outro). `ops` guarda as ações executadas na resposta, para o selo no chat.
CREATE TABLE IF NOT EXISTS chat_messages (
    id bigserial PRIMARY KEY,
    household_id uuid NOT NULL REFERENCES households(id),
    member_position smallint NOT NULL CHECK (member_position IN (0, 1)),
    role text NOT NULL CHECK (role IN ('user', 'assistant')),
    content text NOT NULL,
    ops jsonb NOT NULL DEFAULT '[]',
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS chat_messages_member ON chat_messages(household_id, member_position, id DESC);

-- Auditoria das ações executadas pelo assistente (ferramentas da conversa e rotina do mês),
-- por membro: o que ele fez com o app, quando.
CREATE TABLE IF NOT EXISTS assistant_actions (
    id bigserial PRIMARY KEY,
    household_id uuid NOT NULL REFERENCES households(id),
    member_position smallint NOT NULL CHECK (member_position IN (0, 1)),
    tool text NOT NULL,
    label text NOT NULL,
    payload jsonb,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS assistant_actions_member ON assistant_actions(household_id, member_position, id DESC);

INSERT INTO schema_migrations(version) VALUES (8) ON CONFLICT DO NOTHING;
COMMIT;
