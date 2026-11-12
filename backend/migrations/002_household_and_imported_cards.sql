BEGIN;
SET LOCAL search_path TO finance, public;

-- Domicílio real: Felipe (client-1) e Larissa (client-2). Não recria nem altera membros
-- existentes (o banco de demonstração já usa client-1/client-2 com outros nomes).
DO $seed$
DECLARE
    home uuid;
BEGIN
    IF NOT EXISTS (SELECT 1 FROM members WHERE client_user_id IN ('client-1', 'client-2')) THEN
        INSERT INTO households(name) VALUES ('Felipe e Larissa') RETURNING id INTO home;
        INSERT INTO members(household_id, client_user_id, display_name, position) VALUES
            (home, 'client-1', 'Felipe', 0),
            (home, 'client-2', 'Larissa', 1);
    END IF;
END;
$seed$;

-- Campos do cartão importado (shared/cards.rs) que ainda não tinham coluna.
-- Nome/cor só são gravados quando o banco não corresponde a nenhuma `institutions.key`.
ALTER TABLE cards
    ADD COLUMN IF NOT EXISTS institution_name text,
    ADD COLUMN IF NOT EXISTS color_hex text CHECK (color_hex ~ '^[0-9a-fA-F]{6}$'),
    ADD COLUMN IF NOT EXISTS due_date date,
    ADD COLUMN IF NOT EXISTS bills_available boolean NOT NULL DEFAULT false;

INSERT INTO schema_migrations(version) VALUES (2) ON CONFLICT DO NOTHING;
COMMIT;
