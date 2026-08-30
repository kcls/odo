#!/bin/bash
# Full cluster setup for macOS with Docker Desktop.
#
# Prerequisites (install manually before running this script):
#   - Docker Desktop with Kubernetes enabled
#   - Homebrew (https://brew.sh)
#
# Run from the project root directory.
#
# PostgreSQL runs outside the cluster. By default it is installed on this
# host with Homebrew (latest stable major; override with POSTGRES_VERSION);
# alternatively the cluster is pointed at a PostgreSQL server that already
# exists.
#
# --with-test-deps additionally installs what ./scripts/run-tests.sh
# needs: the e2e npm packages, the Playwright browsers, pgTAP, and the
# e2e fixtures.

set -e

# ---------------------------------------------------------------------------
# Version / configuration variables
# ---------------------------------------------------------------------------
NODE_MAJOR_VERSION="24"
NVM_VERSION="v0.40.4"

SEA_ORM_CLI_VERSION="2.0.0-rc.38"

ENVOY_GATEWAY_VERSION="v1.7.2"

LOCAL_REGISTRY_PORT="32000"       # host port; container listens on 5000
LOCAL_REGISTRY_NAME="local-registry"

# Leave POSTGRES_VERSION empty to take the latest stable postgresql@N
# formula Homebrew offers, or pin one: POSTGRES_VERSION=17 ./install-...
POSTGRES_VERSION="${POSTGRES_VERSION:-}"

PG_HBA_BEGIN="# >>> odo cluster (managed by install-docker-desktop-cluster-mac.sh) >>>"
PG_HBA_END="# <<< odo cluster <<<"

# Docker Desktop's internal network - the source range pods' connections
# to this Mac can arrive from.
DOCKER_DESKTOP_CIDR="${DOCKER_DESKTOP_CIDR:-192.168.65.0/24}"

# Install the test rig (e2e packages, Playwright browsers, pgTAP,
# fixtures)? Set by --with-test-deps.
WITH_TEST_DEPS="false"

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

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

check_prerequisites() {
    local failed=0

    if ! command -v brew >/dev/null 2>&1; then
        echo -e "${RED}Error: Homebrew is not installed.${NC}"
        echo "Install it from https://brew.sh"
        failed=1
    fi

    if ! command -v docker >/dev/null 2>&1; then
        echo -e "${RED}Error: Docker is not installed.${NC}"
        echo "Install Docker Desktop and enable Kubernetes."
        failed=1
    elif ! docker info >/dev/null 2>&1; then
        echo -e "${RED}Error: Docker is not running.${NC}"
        echo "Start Docker Desktop before running this script."
        failed=1
    fi

    if ! command -v kubectl >/dev/null 2>&1; then
        echo -e "${RED}Error: kubectl is not available.${NC}"
        echo "Enable Kubernetes in Docker Desktop settings."
        failed=1
    elif ! kubectl get nodes >/dev/null 2>&1; then
        echo -e "${RED}Error: Cannot reach the Kubernetes API server.${NC}"
        echo "Enable Kubernetes in Docker Desktop settings and wait for it to start."
        failed=1
    fi

    if [ "$failed" -eq 1 ]; then
        exit 1
    fi
}

# Best guess at an address this Mac is reachable on from the Docker Desktop
# VM as well as locally: the address of the interface carrying the default
# route. Empty if it cannot be determined.
guess_host_address() {
    local iface
    iface="$(route -n get default 2>/dev/null | awk '/interface:/ {print $2; exit}')"
    [[ -n "$iface" ]] && ipconfig getifaddr "$iface" 2>/dev/null
}

is_loopback_host() {
    case "$(echo "$1" | tr '[:upper:]' '[:lower:]')" in
        ""|localhost|localhost.localdomain|::1|0.0.0.0) return 0 ;;
        127.*) return 0 ;;
    esac
    return 1
}

prompt_postgres_credentials() {
    print_section "PostgreSQL Setup"

    # PostgreSQL always runs outside the cluster: either installed on this
    # host with Homebrew, or an instance that already exists.
    echo "How should PostgreSQL be provided?"
    echo "  1) Install PostgreSQL on this host with Homebrew (default)"
    echo "  2) Use an existing PostgreSQL server"
    local pg_choice
    read -rp "Select [1/2]: " pg_choice

    if [[ "$pg_choice" == "2" ]]; then
        POSTGRES_INSTALL_LOCAL="false"
    else
        POSTGRES_INSTALL_LOCAL="true"
    fi

    # One DATABASE_URL serves the service pods and the host tooling, so the
    # address has to work in both places. host.docker.internal is out: it
    # resolves inside a pod but not on the host. For a Homebrew install
    # that means this Mac's LAN address.
    cat <<'EOF'

The cluster stores a single DATABASE_URL, read by the service pods and by
the host tooling alike. It needs an address reachable from BOTH: a LAN IP
of the database host, or a DNS name that resolves in the Docker Desktop VM
and on this Mac. 'localhost' and 'host.docker.internal' are not options.
EOF
    echo

    local host_default=""
    if [[ "$POSTGRES_INSTALL_LOCAL" == "true" ]]; then
        host_default="$(guess_host_address)"
    fi

    local host_prompt="Database host (IP or DNS name)"
    [[ -n "$host_default" ]] && host_prompt+=" [$host_default]"

    while true; do
        read -rp "${host_prompt}: " POSTGRES_HOST
        POSTGRES_HOST="${POSTGRES_HOST:-$host_default}"
        if [[ -z "$POSTGRES_HOST" ]]; then
            echo -e "${RED}A host is required.${NC}"
            continue
        fi
        if is_loopback_host "$POSTGRES_HOST"; then
            echo -e "${RED}'${POSTGRES_HOST}' is a loopback address; pods would dial themselves.${NC}"
            continue
        fi
        break
    done

    read -rp "PostgreSQL port [5432]: " POSTGRES_PORT
    POSTGRES_PORT="${POSTGRES_PORT:-5432}"

    read -rp "PostgreSQL superuser [odo]: " POSTGRES_USER
    POSTGRES_USER="${POSTGRES_USER:-odo}"

    if [[ "$POSTGRES_INSTALL_LOCAL" != "true" ]]; then
        echo
        echo "That role must already exist and be a superuser. The schema"
        echo "deploy creates the 'odo' database."
    fi

    while true; do
        read -rsp "PostgreSQL password for '${POSTGRES_USER}': " POSTGRES_PASSWORD
        echo
        if [[ -z "$POSTGRES_PASSWORD" ]]; then
            echo -e "${RED}Password cannot be empty.${NC}"
            continue
        fi
        # The password is carried inside a postgres:// URL that the tooling
        # parses by hand, so characters that delimit the URL are rejected
        # rather than silently mangled.
        if [[ "$POSTGRES_PASSWORD" =~ [@/?\#%[:space:]] ]]; then
            echo -e "${RED}Password cannot contain @ / ? # % or whitespace.${NC}"
            continue
        fi
        read -rsp "Confirm password: " pg_confirm
        echo
        if [[ "$POSTGRES_PASSWORD" == "$pg_confirm" ]]; then
            break
        fi
        echo -e "${RED}Passwords do not match. Try again.${NC}"
    done
}

confirm() {
    echo
    echo -e "${YELLOW}This will install PostgreSQL, system packages, and dev tools, then"
    echo -e "initialize the cluster with all services. It will take a while.${NC}"
    if [[ "$WITH_TEST_DEPS" == "true" ]]; then
        echo
        echo -e "${YELLOW}--with-test-deps: also installs the e2e npm packages, the"
        echo -e "Playwright browsers, pgTAP, and the e2e fixtures.${NC}"
    fi
    echo

    read -rp "Continue? [y/N] " response
    if [[ ! "$response" =~ ^[Yy]$ ]]; then
        echo "Aborted."
        exit 0
    fi
}

# ---------------------------------------------------------------------------
# System Packages
# ---------------------------------------------------------------------------

install_system_packages() {
    print_section "Installing system packages via Homebrew"

    # https://github.com/bayandin/homebrew-tap
    brew trust bayandin/tap
    brew tap bayandin/tap

    # https://github.com/sqitchers/homebrew-sqitch
    brew trust sqitchers/sqitch
    brew tap sqitchers/sqitch

    brew update
    brew install \
        openssl \
        pkg-config \
        make \
        gcc \
        yq \
        cpanminus \
        k9s \
        libpq \
        perl

    install_sqitch

    # pgTAP is not here: it is only needed to run src/db-tests, so
    # install_pgtap handles it under --with-test-deps (cpanminus above is
    # what it uses for pg_prove).

    echo "System packages installed"
}

install_sqitch() {
    local flags="--with-postgres-support --with-sqlite-support"

    brew install sqitch $flags
    sqitch --version >/dev/null 2>&1 && return

    # Reinstall to relink sqitch against the current perl.
    brew reinstall sqitch $flags
    sqitch --version >/dev/null 2>&1 && return

    echo -e "${RED}Error: sqitch won't run; try reinstalling perl first.${NC}"
    exit 1
}

# ---------------------------------------------------------------------------
# Dev Tools
# ---------------------------------------------------------------------------

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

# ---------------------------------------------------------------------------
# Cluster Initialization
# ---------------------------------------------------------------------------

run_local_registry() {
    print_section "Starting local Docker registry on port $LOCAL_REGISTRY_PORT"

    if docker ps --format '{{.Names}}' | grep -q "^${LOCAL_REGISTRY_NAME}$"; then
        echo "Registry container '$LOCAL_REGISTRY_NAME' is already running"
        return
    fi

    docker run -d \
        -p "${LOCAL_REGISTRY_PORT}:5000" \
        --restart=always \
        --name "$LOCAL_REGISTRY_NAME" \
        registry:2

    echo "Local registry running at localhost:$LOCAL_REGISTRY_PORT"
}

install_envoy_gateway() {
    print_section "Installing Envoy Gateway $ENVOY_GATEWAY_VERSION"

    kubectl apply --server-side \
        -f "https://github.com/envoyproxy/gateway/releases/download/${ENVOY_GATEWAY_VERSION}/install.yaml"

    echo "Envoy Gateway applied"
}

apply_namespaces_and_secrets() {
    print_section "Applying namespaces and secrets"

    kubectl apply -f ./k8s/namespaces.yaml
    kubectl apply -f ./k8s/odo-secrets.yaml
    kubectl apply -k ./k8s/infrastructure/envoy

    patch_postgres_secret

    echo "Namespaces and secrets applied"
}

patch_postgres_secret() {
    # One DATABASE_URL, read by the service pods and by the host tooling
    # (manage-database.sh, run-tests.sh) alike - which is why the host in
    # it has to resolve the same way in both places.
    local db_name="odo"
    local db_user="${POSTGRES_USER}"
    local db_url="postgres://${db_user}:${POSTGRES_PASSWORD}@${POSTGRES_HOST}:${POSTGRES_PORT}/${db_name}?sslmode=disable"

    local ns
    for ns in odo-core odo-pub; do
        kubectl create secret generic postgres-credentials \
            --namespace "$ns" \
            --from-literal=DATABASE_URL="$db_url" \
            --dry-run=client -o yaml | kubectl apply -f -
    done

    echo "DATABASE_URL patched into postgres-credentials (odo-core, odo-pub)"
    echo "  ${POSTGRES_HOST}:${POSTGRES_PORT}/${db_name}"
}

# ---------------------------------------------------------------------------
# PostgreSQL (external to the cluster)
# ---------------------------------------------------------------------------

resolve_postgres_version() {
    if [[ "$POSTGRES_INSTALL_LOCAL" != "true" ]]; then
        return
    fi

    if [[ -n "$POSTGRES_VERSION" ]]; then
        echo "Using pinned PostgreSQL version $POSTGRES_VERSION"
        return
    fi

    POSTGRES_VERSION="$(brew formulae | grep -E '^postgresql@[0-9]+$' \
        | sed 's/^postgresql@//' | sort -n | tail -1)"

    if [[ -z "$POSTGRES_VERSION" ]]; then
        echo -e "${RED}Could not determine the latest postgresql@N formula${NC}"
        exit 1
    fi

    echo "Latest stable PostgreSQL formula: postgresql@${POSTGRES_VERSION}"
}

install_postgres() {
    if [[ "$POSTGRES_INSTALL_LOCAL" != "true" ]]; then
        # libpq (installed with the system packages) already provides the
        # psql the dev tooling shells out to.
        return
    fi

    print_section "Installing PostgreSQL $POSTGRES_VERSION"

    brew install "postgresql@${POSTGRES_VERSION}"
    brew services start "postgresql@${POSTGRES_VERSION}"

    echo "Waiting for PostgreSQL to accept connections..."
    local deadline=$((SECONDS + 60))
    until pg_isready -h localhost -p "$POSTGRES_PORT" >/dev/null 2>&1; do
        if [ $SECONDS -ge $deadline ]; then
            echo -e "${RED}Timed out waiting for PostgreSQL to start${NC}"
            exit 1
        fi
        sleep 2
    done

    configure_postgres_access
    create_postgres_role
}

configure_postgres_access() {
    print_section "Opening PostgreSQL to the cluster"

    local data_dir
    data_dir="$(brew --prefix)/var/postgresql@${POSTGRES_VERSION}"

    # Homebrew's PostgreSQL listens on localhost only; Docker Desktop pods
    # arrive over the VM network, so it has to bind more than loopback.
    # pg_hba.conf below is the access control.
    if ! grep -qE "^[[:space:]]*listen_addresses[[:space:]]*=[[:space:]]*'\*'" \
            "${data_dir}/postgresql.conf"; then
        printf "\n# Managed by scripts/setup/install-docker-desktop-cluster-mac.sh\nlisten_addresses = '*'\n" \
            >> "${data_dir}/postgresql.conf"
    fi

    # Rewritten rather than appended, so a changed CIDR takes. The pattern
    # is loose enough to also catch a block left by the script's former
    # names (install-dev-cluster-mac.sh, install-cluster-mac.sh).
    sed -i '' "\|^# >>> odo .*cluster (managed by .*) >>>$|,\|^# <<< odo .*cluster <<<$|d" \
        "${data_dir}/pg_hba.conf"

    cat >> "${data_dir}/pg_hba.conf" <<EOF
${PG_HBA_BEGIN}
# Service pods dial this Mac's LAN address; depending on how Docker Desktop
# routes that, the connection arrives either from its internal network
# (override with DOCKER_DESKTOP_CIDR) or from the address itself. The host
# tooling arrives from the same address.
host    all             all             ${DOCKER_DESKTOP_CIDR}  scram-sha-256
host    all             all             ${POSTGRES_HOST}/32     scram-sha-256
${PG_HBA_END}
EOF

    brew services restart "postgresql@${POSTGRES_VERSION}"

    echo "PostgreSQL ${POSTGRES_VERSION} listening on port ${POSTGRES_PORT}"
    echo "pg_hba.conf allows ${DOCKER_DESKTOP_CIDR} and ${POSTGRES_HOST}/32"
}

create_postgres_role() {
    print_section "Creating the '${POSTGRES_USER}' role"

    # Homebrew's PostgreSQL makes the installing account the superuser;
    # there is no `postgres` OS user to sudo to, so create the role here
    # rather than leaving it to manage-database.sh.
    local existing
    existing="$(psql -d postgres -tAc \
        "SELECT 1 FROM pg_roles WHERE rolname='${POSTGRES_USER}'" 2>/dev/null)"

    if [[ "$existing" == "1" ]]; then
        psql -d postgres -c \
            "ALTER ROLE ${POSTGRES_USER} WITH SUPERUSER LOGIN PASSWORD '${POSTGRES_PASSWORD}'"
        echo "Role '${POSTGRES_USER}' updated"
    else
        psql -d postgres -c \
            "CREATE ROLE ${POSTGRES_USER} WITH SUPERUSER LOGIN PASSWORD '${POSTGRES_PASSWORD}'"
        echo "Role '${POSTGRES_USER}' created"
    fi
}

install_pgtap() {
    if [[ "$WITH_TEST_DEPS" != "true" ]]; then
        return
    fi

    print_section "Installing pgTAP"

    # pg_prove, the client-side runner src/db-tests is executed with
    # (cpanminus comes with the system packages above).
    sudo cpanm --notest TAP::Parser::SourceHandler::pgTAP

    # The extension itself lives on the database server, so it can only be
    # installed here when that server is this host.
    if [[ "$POSTGRES_INSTALL_LOCAL" != "true" ]]; then
        echo "Database server is ${POSTGRES_HOST}; install the pgtap"
        echo "extension there for ./scripts/run-db-tests.sh."
        return
    fi

    # The formula pins its own postgresql@N. If that differs from the
    # version serving this host, the extension lands in the other
    # installation and run-db-tests.sh will not find it.
    brew install pgtap

    echo "pgTAP installed"
}

install_e2e_packages() {
    if [[ "$WITH_TEST_DEPS" != "true" ]]; then
        return
    fi

    print_section "Installing e2e npm packages and Playwright browsers"

    # node/npm are on PATH from install_node above. Subshell so the cd
    # does not leak into the steps that follow, which use relative paths.
    (
        cd ./src/e2e
        npm install
        npx playwright install
    )

    echo "e2e packages and browsers installed"
}

deploy_test_data() {
    if [[ "$WITH_TEST_DEPS" != "true" ]]; then
        return
    fi

    print_section "Deploying e2e/dev test data"

    # Flat idempotent fixtures (src/test-data): the e2e.odo.* users, the
    # login-only e2e-test-role, MockSAML config, soft-deleted rows. Safe
    # to re-run at any time. Resolves the endpoint from the secret's
    # DATABASE_URL.
    ./scripts/manage-database.sh deploy-test

    echo "Test data deployed"
}

verify_postgres_connection() {
    print_section "Verifying the PostgreSQL connection"

    if PGPASSWORD="$POSTGRES_PASSWORD" psql -h "$POSTGRES_HOST" -p "$POSTGRES_PORT" \
            -U "$POSTGRES_USER" -d postgres -c 'SELECT 1' > /dev/null 2>&1; then
        echo "Connected to ${POSTGRES_HOST}:${POSTGRES_PORT} as '${POSTGRES_USER}'"
        return
    fi

    echo -e "${RED}Cannot connect to ${POSTGRES_HOST}:${POSTGRES_PORT} as '${POSTGRES_USER}'.${NC}"
    echo
    echo "The server needs:"
    echo "  - a superuser role '${POSTGRES_USER}' with the password entered above"
    echo "  - listen_addresses and pg_hba.conf allowing this host and the"
    echo "    pods' source network (${DOCKER_DESKTOP_CIDR} for Docker Desktop)"
    exit 1
}

setup_database() {
    print_section "Creating the odo database"

    # Resolves the endpoint from the secret patched above and creates the
    # database plus the sqitch schema with the role verified above.
    ./scripts/manage-database.sh setup

    echo "Database and sqitch schema ready"
}

deploy_database_schema() {
    print_section "Deploying database schema"

    # Resolves the endpoint from the secret's DATABASE_URL.
    ./scripts/manage-database.sh deploy

    echo "Database schema deployed"
}

generate_jwt_secret() {
    print_section "Generating JWT secret"

    ./scripts/manage-secrets.sh update-jwt

    echo "JWT secret generated"
}

build_and_deploy_services() {
    print_section "Building and deploying all services (this will take a while)"

    ./scripts/build-and-deploy-service.sh --all

    echo "All services deployed"
}

print_post_install() {
    print_section "Cluster Setup Complete!"

    echo -e "${GREEN}The Docker Desktop cluster is fully initialized and all services are deployed.${NC}"
    echo
    echo "PostgreSQL runs outside the cluster at ${POSTGRES_HOST}:${POSTGRES_PORT}"
    if [[ "$POSTGRES_INSTALL_LOCAL" == "true" ]]; then
        echo "  conf:  $(brew --prefix)/var/postgresql@${POSTGRES_VERSION}"
    fi
    echo
    echo "The database carries the platform seed: the demo org tree 'Odo"
    echo "Library System' and the machine accounts."
    if [[ "$WITH_TEST_DEPS" == "true" ]]; then
        echo
        echo "The e2e fixtures are loaded. Test users (password test123!):"
        echo "e2e.odo.staff (login-only), e2e.odo.admin (odo-admin)."
    else
        echo "The e2e fixtures are NOT loaded - re-run with --with-test-deps"
        echo "to install them along with pgTAP and the e2e packages."
    fi
    echo
    echo "Pods and tooling share the one DATABASE_URL in the secret, so no"
    echo "PG* environment variables are needed."
    if [[ "$WITH_TEST_DEPS" == "true" ]]; then
        echo "  ./scripts/run-tests.sh --db --integration --e2e --unit"
    fi
    echo
    echo "Verify:"
    echo "  kubectl get pods -A"
    echo "  k9s"
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

main() {
    check_not_root
    check_prerequisites
    confirm
    prompt_postgres_credentials

    # system packages
    install_system_packages

    # postgresql (external to the cluster)
    resolve_postgres_version
    install_postgres
    install_pgtap
    verify_postgres_connection

    # dev tools
    install_node
    install_rust
    install_sea_orm_cli
    install_e2e_packages

    # cluster initialization
    run_local_registry
    install_envoy_gateway
    apply_namespaces_and_secrets
    setup_database
    deploy_database_schema
    deploy_test_data
    generate_jwt_secret
    build_and_deploy_services

    print_post_install
}

usage() {
    echo "Usage: $0 [--with-test-deps]"
    echo
    echo "Full cluster setup for macOS with Docker Desktop. Installs"
    echo "PostgreSQL and dev tools, and initializes the cluster with all"
    echo "services."
    echo
    echo "PostgreSQL runs outside the cluster - installed on this host with"
    echo "Homebrew, or an existing server you point the cluster at."
    echo
    echo "Options:"
    echo "  --with-test-deps      Also install what ./scripts/run-tests.sh"
    echo "                        needs: the e2e npm packages, the Playwright"
    echo "                        browsers, pgTAP, and the e2e fixtures."
    echo "                        Omitted by default, which leaves the"
    echo "                        database carrying only the platform seed."
    echo "  --help, -h            Show this message"
    echo
    echo "Environment variables:"
    echo "  POSTGRES_VERSION      PostgreSQL major to install (default: the"
    echo "                        latest stable postgresql@N formula)"
    echo "  DOCKER_DESKTOP_CIDR   Pod source range allowed in pg_hba.conf"
    echo "                        (default: 192.168.65.0/24)"
    echo
    echo "Prerequisites:"
    echo "  - Docker Desktop with Kubernetes enabled"
    echo "  - Homebrew (https://brew.sh)"
    echo
    echo "Run from the project root directory."
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --with-test-deps)
            WITH_TEST_DEPS="true"
            shift
            ;;
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
