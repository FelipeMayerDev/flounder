BEGIN;
SET LOCAL search_path TO finance, public;

INSERT INTO categories(key, label) VALUES
    ('shopping', 'Compras'), ('services', 'Serviços'), ('education', 'Educação'), ('pets', 'Pets')
ON CONFLICT DO NOTHING;

-- Categoria sugerida pela importação: a reimportação só reclassifica o lançamento enquanto
-- ele ainda estiver com a sugestão anterior (o usuário não mudou).
ALTER TABLE bank_transactions ADD COLUMN IF NOT EXISTS suggested_category text REFERENCES categories(key);
UPDATE bank_transactions t SET suggested_category = e.category_key
FROM entries e WHERE e.id = t.entry_id AND t.suggested_category IS NULL;

INSERT INTO schema_migrations(version) VALUES (4) ON CONFLICT DO NOTHING;
COMMIT;
