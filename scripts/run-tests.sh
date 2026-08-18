#!/bin/bash
# Run selected test suites against the k3s cluster. Run from the project root.
#
# Which suites run is chosen explicitly with flags — at least one is required:
#   --db           pgtap database tests
#   --integration  Rust integration tests (odo)
#   --e2e          UI e2e tests (all Playwright projects)
#   --unit         Rust unit tests
#   --load         odo API load test (read-only smoke; fails on concerns)
#   --all          every suite above
#
# Requires install-test-dependencies-ubuntu.sh (or equivalent) to have
# been run first.
#
# Usage:
#   scripts/run-tests.sh --db --integration --e2e   # the cluster suites
#   scripts/run-tests.sh --unit                     # just Rust unit tests
#   scripts/run-tests.sh --db --integration --e2e --unit   # everything
#
# Environment variables (all optional):
#   PGHOST/PGPORT  Override the database endpoint (default: resolved
#                  from the postgres-credentials secret)
#   PGPASSWORD   PostgreSQL password (default: read from k8s secrets)
#   LOAD_WORKERS   --load worker count (default: 10)
#   LOAD_DURATION  --load run length (default: 60s)
#   LOAD_ARGS      extra args appended to the load-test invocation,
#                  e.g. LOAD_ARGS="--writes --mode open --rate 100"

set -e

# Suite selection (set by flags; at least one is required).
RUN_DB=false
RUN_INTEGRATION=false
RUN_E2E=false
RUN_UNIT=false
RUN_LOAD=false

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

YELLOW='\033[1;33m'
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m'

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Shared DB connection helpers (init_pg_connection parses DATABASE_URL).
source "$SCRIPT_DIR/common.sh"

# PGHOST/PGPORT are NOT pre-defaulted here: init_pg_connection resolves
# them from the secret's EXTERNAL_DATABASE_URL, and setting them first
# would override it. Export them only to override the secret.

print_section() {
    echo
    echo -e "${YELLOW}>>> $1${NC}"
    echo
}

resolve_pg_password() {
    if [[ -n "$PGPASSWORD" && -n "$PGHOST" && -n "$PGPORT" ]]; then
        return
    fi

    print_section "Reading PostgreSQL password from k8s secrets"

    # The shared resolver fills host/port/user/db/password from the
    # secret (EXTERNAL_DATABASE_URL preferred); any PG* env vars the
    # caller exported act as overrides.
    if ! init_pg_connection odo-core; then
        echo -e "${RED}Error: could not read postgres password from k8s secrets.${NC}"
        echo "Set PGPASSWORD manually or check that the odo-core postgres-credentials"
        echo "secret exists and contains a DATABASE_URL."
        exit 1
    fi

    echo "Connection resolved from odo-core/postgres-credentials"
}

# ---------------------------------------------------------------------------
# Steps
# ---------------------------------------------------------------------------

run_cargo_tests() {
    print_section "Running Rust unit tests"

    cd "$PROJECT_ROOT/src/rust"

    for dir in */; do
        echo "Running tests in $dir"
        cargo test --manifest-path $dir/Cargo.toml
    done;

    echo "Rust unit tests passed"
}

run_pgtap_tests() {
    print_section "Running pgtap tests"

    # Resolves its own connection from the secret; the PG* variables we
    # exported above (or the caller's overrides) take precedence.
    "$SCRIPT_DIR/run-db-tests.sh"

    echo "pgtap tests passed"
}

run_odo_integration_tests() {
    print_section "Running odo integration tests"

    cd "$PROJECT_ROOT/src/integration-tests"
    cargo test

    echo "odo integration tests passed"
}

run_load_tests() {
    print_section "Running odo API load test"

    cd "$PROJECT_ROOT/src/load-tests"

    # Read-only smoke by default; tune with LOAD_WORKERS / LOAD_DURATION or
    # pass anything else through LOAD_ARGS (see src/load-tests/README.md).
    # --fail-on-concerns makes flagged concerns fail the suite.
    # shellcheck disable=SC2086
    cargo run --release -- \
        --workers "${LOAD_WORKERS:-10}" \
        --duration "${LOAD_DURATION:-60s}" \
        --fail-on-concerns \
        ${LOAD_ARGS:-}

    echo "odo API load test passed"
}

run_e2e_tests() {
    print_section "Running UI e2e tests"

    cd "$PROJECT_ROOT/src/e2e"

    # Run against the containerized/k3s UIs. All Playwright projects.
    BASE_URL=http://localhost:30080 npm run test

    echo "UI e2e tests passed"
}

usage() {
    echo "Usage: $0 [--db] [--integration] [--e2e] [--unit] [--load] [--all]"
    echo
    echo "Runs the selected test suites against the k3s cluster. Run from the"
    echo "project root. At least one suite flag is required."
    echo
    echo "Suites:"
    echo "  --db           pgtap database tests"
    echo "  --integration  Rust integration tests (odo)"
    echo "  --e2e          UI e2e tests (all Playwright projects)"
    echo "  --unit         Rust unit tests"
    echo "  --load         odo API load test (read-only smoke; fails on concerns)"
    echo "  --all          every suite above"
    echo "  --help, -h     Show this help"
    echo
    echo "Environment variables (all optional):"
    echo "  PGHOST/PGPORT  Override the endpoint (default: from the secret)"
    echo "  PGPASSWORD   PostgreSQL password (default: read from k8s secrets)"
    echo "  LOAD_WORKERS / LOAD_DURATION / LOAD_ARGS   --load tuning (see src/load-tests/README.md)"
}

print_results() {
    print_section "All Selected Tests Passed!"
    local ran=()
    [[ "$RUN_UNIT" == true ]] && ran+=("unit")
    [[ "$RUN_DB" == true ]] && ran+=("database")
    [[ "$RUN_INTEGRATION" == true ]] && ran+=("integration")
    [[ "$RUN_E2E" == true ]] && ran+=("e2e")
    [[ "$RUN_LOAD" == true ]] && ran+=("load")
    echo -e "${GREEN}Completed: ${ran[*]}${NC}"
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

main() {
    # Unit tests are the only suite that doesn't touch the cluster DB; resolve
    # the password only when a DB-touching suite is selected.
    if [[ "$RUN_DB" == true || "$RUN_INTEGRATION" == true || "$RUN_E2E" == true ]]; then
        resolve_pg_password
    fi

    [[ "$RUN_UNIT" == true ]] && run_cargo_tests
    [[ "$RUN_DB" == true ]] && run_pgtap_tests
    [[ "$RUN_INTEGRATION" == true ]] && run_odo_integration_tests
    [[ "$RUN_E2E" == true ]] && run_e2e_tests
    [[ "$RUN_LOAD" == true ]] && run_load_tests
    print_results
}

# ---------------------------------------------------------------------------
# Argument parsing
# ---------------------------------------------------------------------------

while [[ $# -gt 0 ]]; do
    case "$1" in
        --db)          RUN_DB=true; shift ;;
        --integration) RUN_INTEGRATION=true; shift ;;
        --e2e)         RUN_E2E=true; shift ;;
        --unit)        RUN_UNIT=true; shift ;;
        --load)        RUN_LOAD=true; shift ;;
        --all)
            RUN_DB=true
            RUN_INTEGRATION=true
            RUN_E2E=true
            RUN_UNIT=true
            RUN_LOAD=true
            shift
            ;;
        --help|-h)     usage; exit 0 ;;
        *)
            echo -e "${RED}Unknown option: $1${NC}" >&2
            echo "Try '$0 --help'." >&2
            exit 1
            ;;
    esac
done

# Require at least one suite.
if [[ "$RUN_DB" != true && "$RUN_INTEGRATION" != true && "$RUN_E2E" != true && "$RUN_UNIT" != true && "$RUN_LOAD" != true ]]; then
    echo -e "${RED}Error: no test suite selected.${NC}" >&2
    echo >&2
    usage >&2
    exit 1
fi

main
