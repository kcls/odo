#!/usr/bin/env bash
set -euo pipefail

# Generate SeaORM entities from the live database.
#
# Connection resolution, in order of precedence:
#   1. $DATABASE_URL — full libpq URL (host, port, creds, sslmode, etc.).
#      Use this for non-k8s setups, e.g.:
#        DATABASE_URL=postgres://odo:demo123@localhost:5432/odo?sslmode=disable \
#          ./scripts/generate-rust-entities.sh
#   2. The postgres-credentials secret (EXTERNAL_DATABASE_URL, the
#      host-reachable endpoint), resolved via common.sh's
#      init_pg_connection; PG* environment variables act as overrides.
#      DB_SSLMODE overrides the sslmode (default: disable).
#
# Usage:
#   ./scripts/generate-rust-entities.sh              # all schemas
#   ./scripts/generate-rust-entities.sh auth authz   # specific schemas only

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

ENTITY_DIR="src/rust/odo-entity/src"

ALL_SCHEMAS=(asset auth authz notification org)

# Schemas to generate — args or all
if [ $# -gt 0 ]; then
    SCHEMAS=("$@")
else
    SCHEMAS=("${ALL_SCHEMAS[@]}")
fi

# Check sea-orm-cli
if ! command -v sea-orm-cli &>/dev/null; then
    echo -e "${RED}sea-orm-cli not found. Install with:${NC}"
    echo "  cargo install sea-orm-cli --version 2.0.0-rc.2"
    exit 1
fi

# Resolve the connection URL.
if [ -n "${DATABASE_URL:-}" ]; then
    # User supplied a full URL — trust it as-is. Skips the k8s lookup
    # entirely so this path works without kubectl.
    echo -e "${BLUE}Using DATABASE_URL from environment.${NC}"
else
    # Fills PGHOST/PGPORT/PGDATABASE/PGUSER/PGPASSWORD from the secret
    # (PG* env vars act as overrides). sea-orm-cli takes a URL, not
    # libpq env vars, so rebuild one from the resolved pieces.
    # (Empty-init the PG* vars: init_pg_connection probes them with -n,
    # which trips this script's nounset.)
    : "${PGHOST:=}" "${PGPORT:=}" "${PGDATABASE:=}" "${PGUSER:=}" "${PGPASSWORD:=}"
    init_pg_connection || exit 1
    DB_SSLMODE="${DB_SSLMODE:-disable}"

    DATABASE_URL="postgres://${PGUSER}:${PGPASSWORD}@${PGHOST}:${PGPORT}/${PGDATABASE}?sslmode=${DB_SSLMODE}"
    echo -e "${BLUE}Database: ${PGUSER}@${PGHOST}:${PGPORT}/${PGDATABASE} (sslmode=${DB_SSLMODE})${NC}"
fi

echo -e "${BLUE}Schemas:  ${SCHEMAS[*]}${NC}"
echo

for schema in "${SCHEMAS[@]}"; do
    output_dir="${ENTITY_DIR}/${schema}"

    if [ ! -d "$output_dir" ]; then
        echo -e "${RED}Directory $output_dir does not exist — skipping $schema${NC}"
        continue
    fi

    echo -e "${BLUE}Generating entities for schema: ${schema}${NC}"

    sea-orm-cli generate entity \
        --database-url "$DATABASE_URL" \
        --database-schema "$schema" \
        --output-dir "$output_dir" \
        --with-serde both

    echo -e "${GREEN}  ✓ ${schema} → ${output_dir}${NC}"
done

echo
echo -e "${GREEN}Entity generation complete.${NC}"
echo
echo -e "Review the generated files for manual annotations that may"
echo -e "need to be re-applied (e.g. #[sea_orm(ignore)] on INET columns,"
echo -e "utoipa::ToSchema derives, etc.)."
