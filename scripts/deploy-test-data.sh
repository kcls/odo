#!/bin/bash
# Deploy e2e/dev test data (odo platform fixtures, src/test-data/) to the
# database.
#
# Test data is plain SQL, applied in filename order (numeric prefixes);
# every file is idempotent, so re-running is always safe. There is no
# revert: reloading test data pairs with a full database rebuild
# (manage-database.sh). Sqitch is deliberately not involved.
#
# App test fixtures are not deployed here: apps own their fixtures (and
# any grants of app-registered roles).
#
# Connection details resolve from the postgres-credentials secret
# (EXTERNAL_DATABASE_URL, the host-reachable endpoint); any PG*
# environment variables act as overrides.

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
DATA_DIR="$PROJECT_ROOT/src/test-data"

source "$SCRIPT_DIR/common.sh"

# Fills and exports PGHOST/PGPORT/PGDATABASE/PGUSER/PGPASSWORD.
init_pg_connection || exit 1

echo "Deploying test data to: $PGUSER@$PGHOST:$PGPORT/$PGDATABASE"

cd "$DATA_DIR"

for f in [0-9]*.sql; do
    echo "  applying $f"
    psql -q -v ON_ERROR_STOP=1 -f "$f"
done

echo "Test data deployed."
