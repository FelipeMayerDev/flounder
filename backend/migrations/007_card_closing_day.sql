BEGIN;
SET LOCAL search_path TO finance, public;

-- Dia do mês do fechamento da fatura, vindo da Pluggy (`creditData.balanceCloseDate`);
-- `manual_closing_day` é o ajuste do usuário e vence o automático. `cards.due_day` guarda o
-- vencimento do dia do mês; `card_invoices.closing_date` já existia e passa a ser preenchida.
ALTER TABLE cards ADD COLUMN IF NOT EXISTS closing_day smallint CHECK (closing_day BETWEEN 1 AND 31);
ALTER TABLE cards ADD COLUMN IF NOT EXISTS manual_closing_day smallint CHECK (manual_closing_day BETWEEN 1 AND 31);

INSERT INTO schema_migrations(version) VALUES (7) ON CONFLICT DO NOTHING;
COMMIT;
