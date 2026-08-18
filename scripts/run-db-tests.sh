#!/bin/bash
# Run the pgTAP database tests (src/db-tests/) against the odo database.
#
# Connection details resolve from the postgres-credentials secret
# (EXTERNAL_DATABASE_URL, the host-reachable endpoint); any PG*
# environment variables act as overrides.
#
#   scripts/run-db-tests.sh
#   VERBOSE=1 scripts/run-db-tests.sh

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
TEST_DIR="$PROJECT_ROOT/src/db-tests"

source "$SCRIPT_DIR/common.sh"

# Fills and exports PGHOST/PGPORT/PGDATABASE/PGUSER/PGPASSWORD, so psql
# and pg_prove below need no connection flags.
init_pg_connection || exit 1

VERBOSE="${VERBOSE:-}"
[ -n "$VERBOSE" ] && VERBOSE="-v"

echo "Running pgTAP tests from $TEST_DIR"
echo "Connecting to: $PGUSER@$PGHOST:$PGPORT/$PGDATABASE"
echo

# Ensure pgTAP extension is installed
echo "Setting up pgTAP extension..."
psql -c "CREATE EXTENSION IF NOT EXISTS pgtap;" > /dev/null
echo

# Run from the test directory so \i relative paths in test files work
cd "$TEST_DIR"

pg_prove $VERBOSE odo/*.sql
