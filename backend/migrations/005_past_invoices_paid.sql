BEGIN;
SET LOCAL search_path TO finance, public;

-- Faturas importadas que venceram antes do mês atual (horário de Brasília) foram pagas fora do
-- app: a partir daqui a importação já as grava pagas; esta migração acerta as que já existem.
UPDATE card_invoices SET paid = true
WHERE source = 'pluggy' AND NOT paid
    AND due_date < date_trunc('month', now() AT TIME ZONE 'America/Sao_Paulo')::date;

INSERT INTO schema_migrations(version) VALUES (5) ON CONFLICT DO NOTHING;
COMMIT;
