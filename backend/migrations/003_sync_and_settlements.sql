BEGIN;
SET LOCAL search_path TO finance, public;

-- Acertos entre o casal: quem transferiu (from) para quem (to), para zerar a divisão da casa.
CREATE TABLE IF NOT EXISTS settlements (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    household_id uuid NOT NULL REFERENCES households(id),
    from_id uuid NOT NULL,
    to_id uuid NOT NULL,
    settled_on date NOT NULL,
    amount_cents bigint NOT NULL CHECK (amount_cents > 0),
    FOREIGN KEY (household_id, from_id) REFERENCES members(household_id, id),
    FOREIGN KEY (household_id, to_id) REFERENCES members(household_id, id),
    CHECK (from_id <> to_id)
);
CREATE INDEX IF NOT EXISTS settlements_household_date ON settlements(household_id, settled_on);

-- Conta corrente importada: banco detectado como em `cards`. O banco escolhido pelo usuário
-- fica em bank_connections.institution_key e tem precedência na leitura.
ALTER TABLE bank_accounts
    ADD COLUMN IF NOT EXISTS institution_key text REFERENCES institutions(key),
    ADD COLUMN IF NOT EXISTS institution_name text,
    ADD COLUMN IF NOT EXISTS color_hex text CHECK (color_hex ~ '^[0-9a-fA-F]{6}$');
-- Nome do conector Pluggy, mostrado enquanto o banco não é escolhido.
ALTER TABLE bank_connections ADD COLUMN IF NOT EXISTS connector_name text;

-- Conta que some da Pluggy leva suas transações; fatura, lançamento ou cartão removidos só
-- desfazem o vínculo. Excluir o lançamento de uma transação mantém a transação (entry_id NULL),
-- o que impede que ela seja recriada nas próximas importações.
ALTER TABLE bank_transactions
    DROP CONSTRAINT IF EXISTS bank_transactions_household_id_account_id_fkey,
    ADD CONSTRAINT bank_transactions_household_id_account_id_fkey FOREIGN KEY (household_id, account_id)
        REFERENCES bank_accounts(household_id, id) ON DELETE CASCADE,
    DROP CONSTRAINT IF EXISTS bank_transactions_household_id_invoice_id_fkey,
    ADD CONSTRAINT bank_transactions_household_id_invoice_id_fkey FOREIGN KEY (household_id, invoice_id)
        REFERENCES card_invoices(household_id, id) ON DELETE SET NULL (invoice_id),
    DROP CONSTRAINT IF EXISTS bank_transactions_household_id_entry_id_fkey,
    ADD CONSTRAINT bank_transactions_household_id_entry_id_fkey FOREIGN KEY (household_id, entry_id)
        REFERENCES entries(household_id, id) ON DELETE SET NULL (entry_id);
ALTER TABLE entries
    DROP CONSTRAINT IF EXISTS entries_household_id_card_id_fkey,
    ADD CONSTRAINT entries_household_id_card_id_fkey FOREIGN KEY (household_id, card_id)
        REFERENCES cards(household_id, id) ON DELETE SET NULL (card_id);

-- Uma meta de reserva por domicílio (PUT /api/reserve faz upsert).
CREATE UNIQUE INDEX IF NOT EXISTS reserve_goals_household ON reserve_goals(household_id);

INSERT INTO schema_migrations(version) VALUES (3) ON CONFLICT DO NOTHING;
COMMIT;
