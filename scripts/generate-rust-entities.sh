#!/usr/bin/env bash
set -euo pipefail

# Generate SeaORM entities from the live database.
#
# Connection resolution, in order of precedence:
#   1. $DATABASE_URL — full libpq URL (host, port, creds, sslmode, etc.).
#      Use this for non-k8s setups, e.g.:
#        DATABASE_URL=postgres://odo:demo123@localhost:5432/odo?sslmode=disable \
#          ./scripts/generate-rust-entities.sh
#   2. Per-piece overrides: $DB_HOST, $DB_PORT, $DB_USER, $DB_PASS, $DB_NAME,
#      $DB_SSLMODE. Any value not set falls back to either the
#      built-in default (host/port/sslmode) or the k8s secret
#      (user/pass/db).
#   3. k8s postgres-credentials secret in $POSTGRES_NAMESPACE
#      (default: odo-core). Connects via the default host:port below
#      (the NodePort exposing the in-cluster postgres).
#
# Usage:
#   ./scripts/generate-rust-entities.sh              # all schemas
#   ./scripts/generate-rust-entities.sh auth authz   # specific schemas only

RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[0;33m'
NC='\033[0m'

ENTITY_DIR="src/rust/odo-entity/src"
NAMESPACE="${POSTGRES_NAMESPACE:-odo-core}"
SECRET_NAME="postgres-credentials"

# Connection defaults — only used when neither $DATABASE_URL nor the
# corresponding per-piece env var is set.
DEFAULT_DB_HOST="localhost"
DEFAULT_DB_PORT="32345"
DEFAULT_DB_SSLMODE="disable"

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
    # User supplied a full URL — trust it as-is. Skip the k8s lookup
    # entirely so this path works without kubectl.
    echo -e "${BLUE}Using DATABASE_URL from environment.${NC}"
else
    # Read missing pieces from the k8s secret. Skip the kubectl call
    # if everything is already set by per-piece env vars so the script
    # also works in environments without kubectl.
    needs_secret=false
    for var in DB_USER DB_PASS DB_NAME; do
        if [ -z "${!var:-}" ]; then
            needs_secret=true
            break
        fi
    done

    if $needs_secret; then
        if ! command -v kubectl &>/dev/null; then
            echo -e "${RED}DB_USER / DB_PASS / DB_NAME not set and kubectl not available.${NC}"
            echo "Either export DATABASE_URL or the per-piece env vars, or install kubectl."
            exit 1
        fi
        get_secret() {
            kubectl get secret "$SECRET_NAME" -n "$NAMESPACE" \
                -o jsonpath="{.data.$1}" 2>/dev/null | base64 -d
        }
        DB_USER="${DB_USER:-$(get_secret POSTGRES_USER)}"
        DB_PASS="${DB_PASS:-$(get_secret POSTGRES_PASSWORD)}"
        DB_NAME="${DB_NAME:-$(get_secret POSTGRES_DB)}"

        if [ -z "$DB_USER" ] || [ -z "$DB_PASS" ] || [ -z "$DB_NAME" ]; then
            echo -e "${RED}Failed to read postgres-credentials from k8s secret${NC}"
            echo "Namespace: $NAMESPACE, Secret: $SECRET_NAME"
            echo "Alternative: export DATABASE_URL=postgres://user:pass@host:port/db?sslmode=..."
            exit 1
        fi
    fi

    DB_HOST="${DB_HOST:-$DEFAULT_DB_HOST}"
    DB_PORT="${DB_PORT:-$DEFAULT_DB_PORT}"
    DB_SSLMODE="${DB_SSLMODE:-$DEFAULT_DB_SSLMODE}"

    DATABASE_URL="postgres://${DB_USER}:${DB_PASS}@${DB_HOST}:${DB_PORT}/${DB_NAME}?sslmode=${DB_SSLMODE}"
    echo -e "${BLUE}Database: ${DB_USER}@${DB_HOST}:${DB_PORT}/${DB_NAME} (sslmode=${DB_SSLMODE})${NC}"
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
