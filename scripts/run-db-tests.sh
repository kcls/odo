#!/bin/bash
# Run a project's pgTAP database tests (tests/db/) against its database.
#
# Works for odo and for app checkouts alike (see lib/project.sh): the
# connection comes from the secret named in the project's service-map.yaml
# (`database:` block; see load_db_config in common.sh), and any PG*
# environment variables act as overrides. `database.tests` is the glob of
# test files under tests/db (default: *.sql); helper files that tests pull
# in with \i must not match it.
#
#   scripts/run-db-tests.sh
#   VERBOSE=1 scripts/run-db-tests.sh

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib/project.sh"
PROJECT_ROOT="$(resolve_project_root "$(cd "$SCRIPT_DIR/.." && pwd)")"
TEST_DIR="$PROJECT_ROOT/tests/db"
echo "Project: $PROJECT_ROOT"

source "$SCRIPT_DIR/common.sh"
load_db_config "$PROJECT_ROOT" || exit 1
TESTS="$(yq eval '.database.tests // "*.sql"' "$PROJECT_ROOT/service-map.yaml")"

# Fills and exports PGHOST/PGPORT/PGDATABASE/PGUSER/PGPASSWORD, so psql
# and pg_prove below need no connection flags.
init_pg_connection || exit 1

VERBOSE="${VERBOSE:-}"
[ -n "$VERBOSE" ] && VERBOSE="-v"

echo "Running pgTAP tests from $TEST_DIR"
echo "Connecting to: $PGUSER@$PGHOST:$PGPORT/$PGDATABASE ($DB_SECRET_NAMESPACE/$DB_SECRET_NAME)"
echo

# Ensure pgTAP extension is installed
echo "Setting up pgTAP extension..."
psql -c "CREATE EXTENSION IF NOT EXISTS pgtap;" > /dev/null
echo

# Run from the test directory so \i relative paths in test files work
cd "$TEST_DIR"

# $TESTS is a glob: left unquoted on purpose so the shell expands it.
# shellcheck disable=SC2086
pg_prove $VERBOSE $TESTS
