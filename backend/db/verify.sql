BEGIN;
SET LOCAL search_path TO finance, public;
DO $test$
DECLARE
    balance bigint;
BEGIN
    IF (SELECT count(*) FROM entries) <> 104 OR (SELECT count(*) FROM cards) <> 2
        OR (SELECT count(*) FROM card_invoices) <> 12
        OR (SELECT count(*) FROM card_invoices WHERE paid) <> 8 THEN
        RAISE EXCEPTION 'O seed divergiu do mock';
    END IF;
    SELECT sum(CASE
        WHEN kind='income' AND owner_id='00000000-0000-4000-8000-000000000064' THEN amount_cents
        WHEN kind='expense' AND shared THEN -((amount_cents + 1) / 2)
        WHEN kind='expense' AND owner_id='00000000-0000-4000-8000-000000000064' THEN -amount_cents
        ELSE 0 END)
    INTO balance FROM entries WHERE entry_date >= DATE '2026-11-01' AND entry_date < DATE '2026-12-01';
    IF balance <> 528070 THEN RAISE EXCEPTION 'Saldo do mock incorreto ou fatura contada em duplicidade'; END IF;
    IF EXISTS (
        SELECT 1 FROM card_invoices i
        WHERE i.total_cents <> (SELECT sum(e.amount_cents) FROM entries e WHERE e.card_id=i.card_id
            AND date_trunc('month',e.entry_date)=date_trunc('month',i.due_date))
    ) THEN RAISE EXCEPTION 'As faturas não correspondem às compras do mock'; END IF;

    INSERT INTO households(id,name) VALUES ('10000000-0000-4000-8000-000000000001','Teste isolado');
    INSERT INTO members(id,household_id,client_user_id,display_name,position)
    VALUES ('10000000-0000-4000-8000-000000000002','10000000-0000-4000-8000-000000000001','test-isolated','Teste',0);
    BEGIN
        INSERT INTO entries(household_id,owner_id,description,amount_cents,entry_date,kind,category_key)
        VALUES ('10000000-0000-4000-8000-000000000001','00000000-0000-4000-8000-000000000064','Outro domicílio',100,current_date,'expense','other');
        RAISE EXCEPTION 'Aceitou referência cruzada entre domicílios';
    EXCEPTION WHEN foreign_key_violation THEN NULL; END;
    BEGIN
        INSERT INTO members(household_id,client_user_id,display_name,position)
        VALUES ('00000000-0000-4000-8000-000000000001','third-person','Teste',2);
        RAISE EXCEPTION 'Aceitou terceira posição';
    EXCEPTION WHEN check_violation THEN NULL; END;
    BEGIN
        UPDATE entries SET amount_cents=-1 WHERE id='00000000-0000-4000-8000-0000000003e9';
        RAISE EXCEPTION 'Aceitou valor negativo em lançamento manual';
    EXCEPTION WHEN check_violation THEN NULL; END;
    BEGIN
        UPDATE cards SET due_day=32;
        RAISE EXCEPTION 'Aceitou dia de vencimento inválido';
    EXCEPTION WHEN check_violation THEN NULL; END;
    BEGIN
        UPDATE cards SET last_four='12345678';
        RAISE EXCEPTION 'Aceitou número com mais de quatro dígitos';
    EXCEPTION WHEN check_violation THEN NULL; END;
    BEGIN
        UPDATE entries SET kind='income',shared=true WHERE id='00000000-0000-4000-8000-0000000003e9';
        RAISE EXCEPTION 'Aceitou receita compartilhada';
    EXCEPTION WHEN check_violation THEN NULL; END;

    INSERT INTO bank_connections(id,household_id,owner_id,external_id) VALUES
        ('10000000-0000-4000-8000-000000000003','00000000-0000-4000-8000-000000000001','00000000-0000-4000-8000-000000000064','fixture-connection');
    BEGIN
        INSERT INTO bank_accounts(household_id,owner_id,connection_id,external_id,kind,name,currency)
        VALUES ('00000000-0000-4000-8000-000000000001','00000000-0000-4000-8000-000000000065','10000000-0000-4000-8000-000000000003','fixture-wrong-owner','CREDIT','Teste','BRL');
        RAISE EXCEPTION 'Aceitou conta em conexão de outro proprietário';
    EXCEPTION WHEN foreign_key_violation THEN NULL; END;
    INSERT INTO bank_accounts(id,household_id,owner_id,connection_id,external_id,kind,name,currency) VALUES
        ('10000000-0000-4000-8000-000000000004','00000000-0000-4000-8000-000000000001','00000000-0000-4000-8000-000000000064','10000000-0000-4000-8000-000000000003','fixture-bank','BANK','Teste','USD');
    BEGIN
        INSERT INTO cards(household_id,owner_id,account_id,name)
        VALUES ('00000000-0000-4000-8000-000000000001','00000000-0000-4000-8000-000000000064','10000000-0000-4000-8000-000000000004','Teste');
        RAISE EXCEPTION 'Aceitou conta corrente como cartão';
    EXCEPTION WHEN foreign_key_violation THEN NULL; END;
    INSERT INTO bank_transactions(household_id,account_id,external_id,description,amount_cents,currency,transaction_date,status)
    VALUES ('00000000-0000-4000-8000-000000000001','10000000-0000-4000-8000-000000000004','fixture-transaction','Estorno',-29,'USD',current_date,'POSTED');
    BEGIN
        INSERT INTO bank_transactions(household_id,account_id,external_id,description,amount_cents,currency,transaction_date,status)
        VALUES ('00000000-0000-4000-8000-000000000001','10000000-0000-4000-8000-000000000004','fixture-transaction','Duplicado',100,'USD',current_date,'POSTED');
        RAISE EXCEPTION 'Aceitou transação importada duplicada';
    EXCEPTION WHEN unique_violation THEN NULL; END;
END;
$test$;
ROLLBACK;
SELECT 'Modelo validado: seed, saldo, faturas, centavos, moedas, deduplicação e integridade.' AS result;
