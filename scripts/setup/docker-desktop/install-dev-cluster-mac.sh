#!/bin/bash
# Full dev cluster setup for macOS with Docker Desktop.
#
# Prerequisites (install manually before running this script):
#   - Docker Desktop with Kubernetes enabled
#   - Homebrew (https://brew.sh)
#
# Run from the project root directory.

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

prompt_postgres_credentials() {
    print_section "Create Odo Core Database Password"

    while true; do
        read -rsp "PostgreSQL password: " POSTGRES_PASSWORD
        echo
        if [[ -z "$POSTGRES_PASSWORD" ]]; then
            echo -e "${RED}Password cannot be empty.${NC}"
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
    echo -e "${YELLOW}This will install system packages, dev tools, and initialize the cluster"
    echo -e "with all services. It will take a while.${NC}"
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
        pgtap \
        cpanminus \
        k9s \
        libpq \
        perl

    install_sqitch

    sudo cpanm --notest TAP::Parser::SourceHandler::pgTAP

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
    local db_host="postgres.odo-core.svc.cluster.local"
    local db_port="5432"
    local db_name="odo"
    local db_user="odo"
    local db_url="postgres://${db_user}:${POSTGRES_PASSWORD}@${db_host}:${db_port}/${db_name}?sslmode=disable"

    # odo namespaces use a single DATABASE_URL. This (re)writes the whole
    # secret, so it must run BEFORE the individual-field patch below for
    # odo-core, otherwise it would clobber those fields.
    local ns
    for ns in odo-core odo-pub; do
        kubectl create secret generic postgres-credentials \
            --namespace "$ns" \
            --from-literal=DATABASE_URL="$db_url" \
            --dry-run=client -o yaml | kubectl apply -f -
    done

    # odo-core also needs the individual POSTGRES_* fields: the postgres
    # StatefulSet reads USER/PASSWORD/DB to init the container, and
    # manage-database.sh reads them for the schema deploy. Merge them onto
    # the DATABASE_URL secret (these fields used to live in
    # postgres/secrets.yaml, now removed).
    kubectl patch secret postgres-credentials \
        --namespace odo-core \
        --type=merge \
        -p "{\"stringData\":{\"POSTGRES_PASSWORD\":\"${POSTGRES_PASSWORD}\",\"POSTGRES_HOST\":\"${db_host}\",\"POSTGRES_PORT\":\"${db_port}\",\"POSTGRES_USER\":\"${db_user}\",\"POSTGRES_DB\":\"${db_name}\"}}"

    echo "PostgreSQL credentials patched into secrets"
}

setup_postgres() {
    print_section "Building and deploying PostgreSQL"

    docker build -t localhost:${LOCAL_REGISTRY_PORT}/odo-postgres:latest k8s/infrastructure/postgres/
    docker push localhost:${LOCAL_REGISTRY_PORT}/odo-postgres:latest
    kubectl apply -k ./k8s/infrastructure/postgres/

    echo "Waiting for PostgreSQL to become ready..."
    kubectl wait --for=condition=Ready node --all --timeout=120s

    echo "PostgreSQL deployed"
}

deploy_database_schema() {
    print_section "Deploying database schema"

    PGHOST=localhost PGPORT=32345 ./scripts/manage-database.sh deploy

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
    print_section "Dev Cluster Setup Complete!"

    echo -e "${GREEN}The Docker Desktop dev cluster is fully initialized and all services are deployed.${NC}"
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

    # dev tools
    install_node
    install_rust
    install_sea_orm_cli

    # cluster initialization
    run_local_registry
    install_envoy_gateway
    apply_namespaces_and_secrets
    setup_postgres
    deploy_database_schema
    generate_jwt_secret
    build_and_deploy_services

    print_post_install
}

case "${1:-}" in
    --help|-h)
        echo "Usage: $0"
        echo
        echo "Full dev cluster setup for macOS with Docker Desktop."
        echo "Installs dev tools and initializes the cluster with all services."
        echo
        echo "Prerequisites:"
        echo "  - Docker Desktop with Kubernetes enabled"
        echo "  - Homebrew (https://brew.sh)"
        echo
        echo "Run from the project root directory."
        exit 0
        ;;
esac

main
