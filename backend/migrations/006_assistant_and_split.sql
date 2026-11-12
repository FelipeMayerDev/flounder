BEGIN;
SET LOCAL search_path TO finance, public;

-- Contas fixas: repetem todo mês e alimentam a previsão dos meses futuros.
ALTER TABLE entries ADD COLUMN IF NOT EXISTS recurring boolean NOT NULL DEFAULT false;

-- Rateio desligado no domicílio: despesa compartilhada vale por inteiro para quem pagou
-- (sem divisão pela renda e sem acertos entre o casal).
ALTER TABLE households ADD COLUMN IF NOT EXISTS shared_split_off boolean NOT NULL DEFAULT false;

-- Análises do assistente (GLM via Z.ai): uma por membro (subject 0/1) e uma para o casal
-- (subject 2), regeradas quando os dados mudam (data_hash). As antigas ficam no banco para
-- dar contexto à conversa; a leitura só devolve ao membro a própria e a do casal.
CREATE TABLE IF NOT EXISTS assistant_reports (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    household_id uuid NOT NULL REFERENCES households(id),
    subject smallint NOT NULL CHECK (subject IN (0, 1, 2)),
    period_start date NOT NULL,
    period_end date NOT NULL,
    content text NOT NULL CHECK (length(trim(content)) > 0),
    data_hash text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (household_id, subject, period_start, period_end, data_hash)
);
CREATE INDEX IF NOT EXISTS assistant_reports_household ON assistant_reports(household_id, subject, created_at);

INSERT INTO schema_migrations(version) VALUES (6) ON CONFLICT DO NOTHING;
COMMIT;
