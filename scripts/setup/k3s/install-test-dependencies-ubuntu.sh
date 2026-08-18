#!/bin/bash
# Install test dependencies on Ubuntu.
#
# Installs system packages needed by Playwright/e2e tests and deploys
# test data to the database. Run from the project root directory as a
# normal user; the script uses sudo where needed.

set -e

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

YELLOW='\033[1;33m'
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m'

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

# The test-data deploy resolves its database endpoint from the
# postgres-credentials secret (EXTERNAL_DATABASE_URL); export PGHOST/
# PGPORT only to override it, e.g.
#   PGHOST=db.example.org PGPORT=5432 ./install-test-dependencies-ubuntu.sh

print_section() {
    echo
    echo -e "${YELLOW}>>> $1${NC}"
    echo
}

check_not_root() {
    if [ "$EUID" -eq 0 ]; then
        echo -e "${RED}Error: Run this script as a normal user, not root.${NC}"
        exit 1
    fi
}

confirm() {
    echo
    echo -e "${YELLOW}This script will:${NC}"
    echo
    echo "  1. Install Playwright system dependencies (libatk, libcups, libgbm, ...)"
    echo "  2. Install e2e npm packages"
    echo "  3. Deploy test data to the database"
    echo

    read -rp "Continue? [y/N] " response
    if [[ ! "$response" =~ ^[Yy]$ ]]; then
        echo "Aborted."
        exit 0
    fi
}

# ---------------------------------------------------------------------------
# Steps
# ---------------------------------------------------------------------------

install_e2e_packages() {
    print_section "Installing e2e npm packages"

    cd "$PROJECT_ROOT/src/e2e"
    npm install
    npx playwright install-deps

    echo "e2e packages installed"
}

deploy_test_data() {
    print_section "Deploying test data to database"

    "$PROJECT_ROOT/scripts/manage-database.sh" deploy-test

    echo "Test data deployed"
}

print_post_install() {
    print_section "Test Dependencies Installed!"

    echo -e "${GREEN}Test dependencies and test data are ready.${NC}"
    echo
    echo "Run tests with:"
    echo "  ./scripts/run-tests.sh"
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

main() {
    check_not_root
    confirm

    install_e2e_packages
    deploy_test_data

    print_post_install
}

case "${1:-}" in
    --help|-h)
        echo "Usage: $0"
        echo
        echo "Installs test dependencies (Playwright libs, e2e packages) and"
        echo "deploys test data to the database."
        echo "Run as a normal user from the project root directory."
        exit 0
        ;;
esac

main
