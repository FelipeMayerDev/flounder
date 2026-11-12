#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
# Os mocks ficam em outro banco; este comando não preenche o banco de dados reais.
if [[ $(docker compose exec -T postgres psql -U flounder -d postgres -Atc "SELECT 1 FROM pg_database WHERE datname='flounder_demo'") != 1 ]]; then
  docker compose exec -T postgres createdb -U flounder flounder_demo
fi
psql() { docker compose exec -T postgres psql -X -q -v ON_ERROR_STOP=1 -U flounder -d flounder_demo "$@"; }
psql < backend/migrations/001_initial.sql
cargo run --quiet --locked --example export_demo | psql
# Depois do seed: os membros demo já usam client-1/client-2, então a 002 não cria o domicílio real.
for migration in backend/migrations/0*.sql; do
  version=$((10#$(basename "$migration" | cut -d_ -f1)))
  if [[ $version != 1 && -z $(psql -Atc "SELECT 1 FROM finance.schema_migrations WHERE version=$version") ]]; then
    psql < "$migration"
  fi
done
