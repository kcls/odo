#!/bin/bash
# Turn an Odo k3s cluster into a development machine (Ubuntu).
#
# Installs the language toolchains and build dependencies, starts the local
# Docker registry, creates and populates the database, deploys every
# service and builds the odo-register CLI. Development only - nothing here
# belongs on a production node.
#
# Prerequisites, in order:
#   1. ./scripts/setup/install-postgres-server-ubuntu.sh --with-pgtap
#      (or an existing PostgreSQL server with the pgtap extension)
#   2. ./scripts/setup/install-k3s-cluster-ubuntu.sh
#
# The database connection comes from the postgres-credentials secret those
# steps wrote; no PG* environment variables are needed.
#
# --with-test-deps additionally installs what ./scripts/run-tests.sh needs:
# the pgTAP client runner, the Playwright system libraries, the e2e npm
# packages, and the e2e fixtures.
#
# Run as a normal user from the project root directory. The script uses
# sudo and sg where needed; no logout/login required during execution.

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/setup-common.sh"

# ---------------------------------------------------------------------------
# Version / configuration variables
# ---------------------------------------------------------------------------
KUBECONFIG_PATH="/etc/rancher/k3s/k3s.yaml"

NODE_MAJOR_VERSION="24"
NVM_VERSION="v0.40.4"

SEA_ORM_CLI_VERSION="2.0.0-rc.38"

LOCAL_REGISTRY_PORT="32000"       # host port; container listens on 5000
LOCAL_REGISTRY_NAME="local-registry"

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

kctl() {
    sudo kubectl --kubeconfig="$KUBECONFIG_PATH" "$@"
}

# Run a command with k3s and docker group access plus dev tool envs. The
# group memberships the cluster installer granted are not active in this
# login session yet, and sg picks them up without a logout - while keeping
# HOME (and so the cargo/npm caches) the user's rather than root's.
with_dev_env() {
    local setup="export KUBECONFIG=${KUBECONFIG_PATH}"
    local cargo_env="$HOME/.cargo/env"
    local nvm_env="$HOME/.nvm/nvm.sh"

    if [[ -f "$cargo_env" ]]; then
        setup="$setup; . $cargo_env"
    fi
    if [[ -f "$nvm_env" ]]; then
        setup="$setup; . $nvm_env"
    fi

    sg k3s -c "sg docker -c '${setup}; $*'"
}

require_cluster() {
    print_section "Checking the cluster"

    if ! command -v k3s >/dev/null 2>&1; then
        die "k3s is not installed. Run ./scripts/setup/install-k3s-cluster-ubuntu.sh first."
    fi

    if ! kctl get nodes --no-headers >/dev/null 2>&1; then
        die "Cannot reach the k3s API server. Is k3s running?"
    fi

    if ! kctl get secret postgres-credentials -n odo-core >/dev/null 2>&1; then
        die "The odo-core/postgres-credentials secret is missing. Run ./scripts/setup/install-k3s-cluster-ubuntu.sh first."
    fi

    # with_dev_env reaches kubectl and docker through `sg`, which needs the
    # memberships on record (they need not be active in this session).
    local grp
    for grp in k3s docker; do
        if ! user_in_group "$grp"; then
            die "User '$USER' is not in the '$grp' group. Run: sudo usermod -aG $grp $USER"
        fi
    done

    local db_url
    db_url="$(kctl get secret postgres-credentials -n odo-core \
        -o jsonpath='{.data.DATABASE_URL}' 2>/dev/null | base64 -d)"

    if ! parse_database_url "$db_url"; then
        die "The DATABASE_URL in odo-core/postgres-credentials is not usable. Fix it with ./scripts/manage-secrets.sh update-db-url."
    fi

    if is_loopback_host "$DB_HOST"; then
        die "DATABASE_URL names the loopback host '${DB_HOST}'; the pods would dial themselves. Fix it with ./scripts/manage-secrets.sh update-db-url."
    fi

    echo "Cluster ready; database endpoint ${DB_HOST}:${DB_PORT}/${DB_NAME}"
}

confirm() {
    echo
    echo -e "${YELLOW}This will install Node, Rust, and the build dependencies, create"
    echo -e "the odo database, deploy the schema, build and deploy every service,"
    echo -e "and build the odo-register CLI. It also installs the pgTAP client"
    echo -e "runner, the Playwright system libraries, the e2e npm packages, and"
    echo -e "the e2e fixtures. It will take a while and requires sudo.${NC}"
    echo

    read -rp "Continue? [y/N] " response
    if [[ ! "$response" =~ ^[Yy]$ ]]; then
        echo "Aborted."
        exit 0
    fi
}

# ---------------------------------------------------------------------------
# Dev tools
# ---------------------------------------------------------------------------

install_dev_packages() {
    print_section "Installing development packages"

    sudo apt-get update
    sudo apt-get install -y \
        build-essential \
        pkg-config \
        libssl-dev \
        sqitch \
        libdbd-pg-perl \
        postgresql-client

    # pg_prove, the client-side runner src/db-tests is executed with.
    # Deliberately not the `pgtap` metapackage: that pulls
    # postgresql-N-pgtap, which depends on the postgresql-N *server*.
    # The extension itself belongs on the database host - the
    # PostgreSQL installer's --with-pgtap puts it there.
    sudo apt-get install -y libtap-parser-sourcehandler-pgtap-perl

    echo "Development packages installed"
}

install_node() {
    print_section "Installing nvm $NVM_VERSION and Node.js $NODE_MAJOR_VERSION"

    if command -v node >/dev/null 2>&1; then
        echo "Node.js is already installed: $(node -v)"
        return
    fi

    curl -o- "https://raw.githubusercontent.com/nvm-sh/nvm/${NVM_VERSION}/install.sh" | bash

    \. "$HOME/.nvm/nvm.sh"

    nvm install "$NODE_MAJOR_VERSION"

    echo "Node.js installed: $(node -v), npm: $(npm -v)"
}

install_rust() {
    print_section "Installing Rust via rustup"

    if command -v rustc >/dev/null 2>&1; then
        echo "Rust is already installed: $(rustc --version)"
        return
    fi

    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y

    source "$HOME/.cargo/env"

    echo "Rust installed: $(rustc --version)"
}

install_sea_orm_cli() {
    print_section "Installing sea-orm-cli $SEA_ORM_CLI_VERSION"

    if command -v sea-orm-cli >/dev/null 2>&1; then
        local installed
        installed="$(sea-orm-cli --version 2>/dev/null | awk '{print $2}')"
        if [ "$installed" = "$SEA_ORM_CLI_VERSION" ]; then
            echo "sea-orm-cli $SEA_ORM_CLI_VERSION is already installed"
            return
        fi
        echo "Upgrading sea-orm-cli from $installed to $SEA_ORM_CLI_VERSION"
    fi

    cargo install sea-orm-cli --version "$SEA_ORM_CLI_VERSION"

    echo "sea-orm-cli $SEA_ORM_CLI_VERSION installed"
}

install_e2e_packages() {
    print_section "Installing e2e npm packages"

    # node/npm are on PATH from install_node above. Subshell so the cd
    # does not leak into the steps that follow, which use relative paths.
    # Playwright's system libraries (libatk, libcups, libgbm, ...) come
    # from install-deps.
    (
        cd ./src/e2e
        npm install
        npx playwright install-deps
    )

    echo "e2e packages installed"
}

# ---------------------------------------------------------------------------
# Cluster contents
# ---------------------------------------------------------------------------

run_local_registry() {
    print_section "Starting local Docker registry on port $LOCAL_REGISTRY_PORT"

    if sudo docker ps --format '{{.Names}}' | grep -q "^${LOCAL_REGISTRY_NAME}$"; then
        echo "Registry container '$LOCAL_REGISTRY_NAME' is already running"
        return
    fi

    sudo docker run -d \
        -p "${LOCAL_REGISTRY_PORT}:5000" \
        --restart=always \
        --name "$LOCAL_REGISTRY_NAME" \
        registry:2

    echo "Local registry running at localhost:$LOCAL_REGISTRY_PORT"
}

setup_database() {
    print_section "Creating the odo database"

    # Resolves the endpoint from the postgres-credentials secret. The
    # superuser role already exists (the PostgreSQL installer created it),
    # so this only creates the database and the sqitch schema.
    with_dev_env ./scripts/manage-database.sh setup

    echo "Database and sqitch schema ready"
}

deploy_database_schema() {
    print_section "Deploying database schema + seed"

    # The sqitch plan: baseline schema plus the generic platform seed
    # (permissions, platform roles, machine accounts, demo org tree).
    with_dev_env ./scripts/manage-database.sh deploy

    echo "Database schema + seed deployed"
}

deploy_test_data() {
    print_section "Deploying e2e/dev test data"

    # Flat idempotent fixtures (src/test-data): the e2e.odo.* users, the
    # login-only e2e-test-role, MockSAML config, soft-deleted rows. Safe
    # to re-run at any time.
    with_dev_env ./scripts/manage-database.sh deploy-test

    echo "Test data deployed"
}

build_and_deploy_services() {
    print_section "Building and deploying all services (this will take a while)"

    with_dev_env ./scripts/build-and-deploy-service.sh --all

    echo "All services deployed"
}

build_odo_register() {
    print_section "Building odo-register"

    # Not in service-map.yaml, so --all does not cover it: odo-register is
    # a CLI that runs on the host rather than a deployed service.
    # load-data-manifest.sh looks for it on PATH and then in this crate's
    # target/release, target/debug -- so a release build here is what it
    # picks up. Rebuilding with a plain `cargo build` later leaves the
    # older release binary winning that search; use --release to iterate.
    with_dev_env "cd src/rust/odo-register && cargo build --release"

    echo "odo-register built at src/rust/odo-register/target/release/odo-register"
}

print_post_install() {
    print_section "Dev Cluster Setup Complete!"

    echo -e "${GREEN}All services are built and deployed, and odo-register is built"
    echo -e "at src/rust/odo-register/target/release/odo-register.${NC}"
    echo
    echo "The database carries the platform seed: the demo org tree 'Odo"
    echo "Library System' and the machine accounts."
    echo
    echo "The e2e fixtures are loaded. Test users (password test123!):"
    echo "e2e.odo.staff (login-only), e2e.odo.admin (odo-admin)."
    echo
    echo "Admin UI: http://<this-host>:30080/odo/admin"
    echo
    echo "Machine accounts: odo-registration (apps register their data"
    echo "via ./scripts/load-data-manifest.sh; disabled outside dev/test data),"
    echo "odo-notify-service."
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

main() {
    check_not_root
    require_project_root
    require_cluster
    confirm

    # dev tools
    install_dev_packages
    install_node
    install_rust
    install_sea_orm_cli
    install_e2e_packages

    run_local_registry

    setup_database
    deploy_database_schema
    deploy_test_data
    build_and_deploy_services
    build_odo_register

    print_post_install
}

usage() {
    echo "Usage: $0 [--with-test-deps]"
    echo
    echo "Turns an Odo k3s cluster into a development machine: installs Node,"
    echo "Rust, and the build dependencies, starts the local Docker registry,"
    echo "creates the database, deploys the schema and platform seed, builds"
    echo "and deploys every service, and builds the odo-register CLI."
    echo
    echo "Run the PostgreSQL and cluster installers first:"
    echo "  ./scripts/setup/install-postgres-server-ubuntu.sh --with-pgtap"
    echo "  ./scripts/setup/install-k3s-cluster-ubuntu.sh"
    echo
    echo "Options:"
    echo "  --with-test-deps   Also install what ./scripts/run-tests.sh"
    echo "                     needs: the pgTAP client runner, the Playwright"
    echo "                     system libraries, the e2e npm packages, and"
    echo "                     the e2e fixtures. Omitted by default, which"
    echo "                     leaves the database carrying only the"
    echo "                     platform seed."
    echo "  --help, -h         Show this message"
    echo
    echo "Run as a normal user from the project root directory."
    echo "The script uses sudo where needed."
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --help|-h)
            usage
            exit 0
            ;;
        *)
            echo -e "${RED}Unknown option: $1${NC}" >&2
            echo >&2
            usage >&2
            exit 1
            ;;
    esac
done

main
