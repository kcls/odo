#!/bin/bash
# Full dev cluster setup for Ubuntu.
#
# Installs k3s, Docker, development tools, and initializes the cluster
# with all services. Run as a normal user from the project root directory.
# The script uses sudo and sg where needed; no logout/login required
# during execution. Log out and back in afterward for interactive use.

set -e

# ---------------------------------------------------------------------------
# Detect architecture
# ---------------------------------------------------------------------------
case "$(uname -m)" in
    x86_64)  ARCH="amd64" ;;
    aarch64) ARCH="arm64" ;;
    *)
        echo "Unsupported architecture: $(uname -m)"
        exit 1
        ;;
esac

# ---------------------------------------------------------------------------
# Version / configuration variables
# ---------------------------------------------------------------------------
K3S_GROUP="k3s"
KUBECONFIG_PATH="/etc/rancher/k3s/k3s.yaml"
REGISTRIES_CONF="/etc/rancher/k3s/registries.yaml"

K9S_VERSION="v0.50.18"
K9S_ARCH="linux_${ARCH}"

YQ_VERSION="v4.53.2"
YQ_PLATFORM="linux_${ARCH}"

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

kctl() {
    sudo kubectl --kubeconfig="$KUBECONFIG_PATH" "$@"
}

# Run a command with k3s and docker group access plus dev tool envs.
# Used for helper scripts that call kubectl/docker internally, before
# the user has logged out and back in to pick up group memberships.
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

prompt_postgres_credentials() {
    print_section "PostgreSQL Setup"

    # Deployment mode: deploy a containerized PG in-cluster, or point the
    # cluster at an existing/external PostgreSQL instance.
    echo "How should PostgreSQL be provided?"
    echo "  1) Deploy a containerized PostgreSQL in the cluster (default)"
    echo "  2) Use an existing/external PostgreSQL instance"
    local pg_choice
    read -rp "Select [1/2]: " pg_choice

    if [[ "$pg_choice" == "2" ]]; then
        POSTGRES_MODE="external"

        read -rp "PostgreSQL host: " POSTGRES_HOST
        while [[ -z "$POSTGRES_HOST" ]]; do
            echo -e "${RED}Host cannot be empty.${NC}"
            read -rp "PostgreSQL host: " POSTGRES_HOST
        done

        read -rp "PostgreSQL port [5432]: " POSTGRES_PORT
        POSTGRES_PORT="${POSTGRES_PORT:-5432}"

        read -rp "PostgreSQL username [odo]: " POSTGRES_USER
        POSTGRES_USER="${POSTGRES_USER:-odo}"
    else
        POSTGRES_MODE="containerized"
        # In-cluster Service FQDN and its NodePort (see
        # k8s/infrastructure/postgres/service.yaml).
        POSTGRES_HOST="postgres.odo-core.svc.cluster.local"
        POSTGRES_PORT="5432"
        POSTGRES_USER="odo"
    fi

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
    echo -e "${YELLOW}This will install k3s, Docker, and dev tools, then initialize the cluster"
    echo -e "with all services. It will take a while and requires sudo.${NC}"
    echo

    read -rp "Continue? [y/N] " response
    if [[ ! "$response" =~ ^[Yy]$ ]]; then
        echo "Aborted."
        exit 0
    fi
}

# ---------------------------------------------------------------------------
# k3s Cluster
# ---------------------------------------------------------------------------

install_prerequisites() {
    print_section "Installing prerequisite packages"
    sudo apt-get update
    sudo apt-get install -y curl wget
    echo "Prerequisites installed"
}

setup_k3s_group() {
    print_section "Creating k3s group and adding $USER"

    if ! getent group "$K3S_GROUP" >/dev/null 2>&1; then
        sudo groupadd "$K3S_GROUP"
        echo "Group '$K3S_GROUP' created"
    else
        echo "Group '$K3S_GROUP' already exists"
    fi

    sudo usermod -aG "$K3S_GROUP" "$USER"
    echo "User '$USER' added to group '$K3S_GROUP'"
}

install_k3s() {
    print_section "Installing k3s (without Traefik ingress)"

    if command -v k3s >/dev/null 2>&1; then
        echo "k3s is already installed"
        return
    fi

    curl -sfL https://get.k3s.io | sudo sh -s - server \
        --disable=traefik \
        --write-kubeconfig-mode "640" \
        --write-kubeconfig-group "$K3S_GROUP"

    echo "Waiting for k3s API server to become ready"
    echo "This might take a few minutes..."

    local deadline=$((SECONDS + 300))
    until kctl get nodes --no-headers 2>/dev/null | grep -q .; do
        if [ $SECONDS -ge $deadline ]; then
            echo -e "${RED}Timed out waiting for k3s node to register${NC}"
            exit 1
        fi
        sleep 5
    done

    kctl wait --for=condition=Ready node --all --timeout=300s

    echo "k3s installed successfully"
}

setup_kubeconfig_env() {
    print_section "Adding KUBECONFIG to .bashrc"

    local export_line="export KUBECONFIG=$KUBECONFIG_PATH"

    if grep -qF "$export_line" "$HOME/.bashrc" 2>/dev/null; then
        echo "KUBECONFIG already set in .bashrc"
    else
        echo "" >> "$HOME/.bashrc"
        echo "# k3s kubeconfig" >> "$HOME/.bashrc"
        echo "$export_line" >> "$HOME/.bashrc"
        echo "Added KUBECONFIG export to .bashrc"
    fi
}

# ---------------------------------------------------------------------------
# System Packages & Docker
# ---------------------------------------------------------------------------

install_system_packages() {
    print_section "Installing system packages"

    sudo apt-get update
    sudo apt-get install -y \
        curl \
        jq \
        sqitch \
        libdbd-pg-perl \
        ca-certificates \
        build-essential \
        pkg-config \
        libssl-dev \
        protobuf-compiler \
        pgtap

    echo "System packages installed"
}

install_docker() {
    print_section "Installing Docker"

    if command -v docker >/dev/null 2>&1; then
        echo "Docker is already installed"
        return
    fi

    sudo apt-get update
    sudo apt-get install -y ca-certificates curl

    sudo install -m 0755 -d /etc/apt/keyrings
    sudo curl -fsSL https://download.docker.com/linux/ubuntu/gpg \
        -o /etc/apt/keyrings/docker.asc
    sudo chmod a+r /etc/apt/keyrings/docker.asc

    sudo tee /etc/apt/sources.list.d/docker.sources > /dev/null <<EOF
Types: deb
URIs: https://download.docker.com/linux/ubuntu
Suites: $(. /etc/os-release && echo "${UBUNTU_CODENAME:-$VERSION_CODENAME}")
Components: stable
Architectures: $(dpkg --print-architecture)
Signed-By: /etc/apt/keyrings/docker.asc
EOF

    sudo apt-get update
    sudo apt-get install -y \
        docker-ce \
        docker-ce-cli \
        containerd.io \
        docker-buildx-plugin \
        docker-compose-plugin

    sudo usermod -aG docker "$USER"
    echo "Docker installed successfully"
}

setup_local_registry_config() {
    print_section "Configuring k3s local registry mirror"

    if sudo grep -q "localhost:${LOCAL_REGISTRY_PORT}" "$REGISTRIES_CONF" 2>/dev/null; then
        echo "Local registry mirror already configured in $REGISTRIES_CONF"
        return
    fi

    sudo tee -a "$REGISTRIES_CONF" > /dev/null <<EOF
mirrors:
  "localhost:${LOCAL_REGISTRY_PORT}":
    endpoint:
      - "http://localhost:${LOCAL_REGISTRY_PORT}"
EOF

    echo "Local registry mirror added to $REGISTRIES_CONF"
}

restart_k3s() {
    print_section "Restarting k3s"
    sudo systemctl restart k3s
    echo "Waiting for k3s API server to become ready..."

    local deadline=$((SECONDS + 120))
    until kctl get nodes --no-headers 2>/dev/null | grep -q .; do
        if [ $SECONDS -ge $deadline ]; then
            echo -e "${RED}Timed out waiting for k3s node to register${NC}"
            exit 1
        fi
        sleep 5
    done

    kctl wait --for=condition=Ready node --all --timeout=120s
    echo "k3s restarted and ready"
}

# ---------------------------------------------------------------------------
# Dev Tools
# ---------------------------------------------------------------------------

install_k9s() {
    print_section "Installing k9s $K9S_VERSION"

    if command -v k9s >/dev/null 2>&1; then
        echo "k9s is already installed"
        return
    fi

    local deb_file="k9s_${K9S_ARCH}.deb"
    local url="https://github.com/derailed/k9s/releases/download/${K9S_VERSION}/k9s_${K9S_ARCH}.deb"

    wget -O "/tmp/$deb_file" "$url"
    sudo dpkg -i "/tmp/$deb_file"
    rm -f "/tmp/$deb_file"
    echo "k9s installed successfully"
}

install_yq() {
    print_section "Installing yq $YQ_VERSION"

    if command -v yq >/dev/null 2>&1; then
        echo "yq is already installed"
        return
    fi

    local tmpdir
    tmpdir="$(mktemp -d)"
    wget "https://github.com/mikefarah/yq/releases/download/${YQ_VERSION}/yq_${YQ_PLATFORM}.tar.gz" \
        -O - | tar xz -C "$tmpdir"
    sudo mv "$tmpdir/yq_${YQ_PLATFORM}" /usr/local/bin/yq
    rm -rf "$tmpdir"
    echo "yq installed successfully"
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

# ---------------------------------------------------------------------------
# Cluster Initialization
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

install_envoy_gateway() {
    print_section "Installing Envoy Gateway $ENVOY_GATEWAY_VERSION"

    kctl apply --server-side \
        -f "https://github.com/envoyproxy/gateway/releases/download/${ENVOY_GATEWAY_VERSION}/install.yaml"

    echo "Envoy Gateway applied"
}

apply_namespaces_and_secrets() {
    print_section "Applying namespaces and secrets"

    kctl apply -f ./k8s/namespaces.yaml
    kctl apply -f ./k8s/odo-secrets.yaml
    kctl apply -k ./k8s/infrastructure/envoy

    patch_postgres_secret

    echo "Namespaces and secrets applied"
}

patch_postgres_secret() {
    # odo namespaces carry two URLs with the same credentials:
    # DATABASE_URL (in-cluster, consumed by the services) and
    # EXTERNAL_DATABASE_URL (host-reachable, resolved by dev tooling -
    # manage-database.sh, run-tests.sh - so no PG* env vars are needed).
    local db_host="${POSTGRES_HOST}"
    local db_port="${POSTGRES_PORT}"
    local db_name="odo"
    local db_user="${POSTGRES_USER}"
    local db_url="postgres://${db_user}:${POSTGRES_PASSWORD}@${db_host}:${db_port}/${db_name}?sslmode=disable"

    # Containerized PG is host-reachable at its NodePort (see
    # k8s/infrastructure/postgres/service.yaml); an external PG is
    # host-reachable at the address the operator supplied.
    local ext_url
    if [[ "$POSTGRES_MODE" == "external" ]]; then
        ext_url="$db_url"
    else
        ext_url="postgres://${db_user}:${POSTGRES_PASSWORD}@localhost:32345/${db_name}?sslmode=disable"
    fi

    # This (re)writes the whole secret, so it must run BEFORE the
    # individual-field patch below for odo-core, otherwise it would clobber
    # those fields.
    local ns
    for ns in odo-core odo-pub; do
        kctl create secret generic postgres-credentials \
            --namespace "$ns" \
            --from-literal=DATABASE_URL="$db_url" \
            --from-literal=EXTERNAL_DATABASE_URL="$ext_url" \
            --dry-run=client -o yaml | kctl apply -f -
    done

    # odo-core also needs the individual POSTGRES_* fields: the postgres
    # StatefulSet reads USER/PASSWORD/DB to init the container, and
    # manage-database.sh reads them for the schema deploy. Merge them onto
    # the DATABASE_URL secret (these fields used to live in
    # postgres/secrets.yaml, now removed).
    kctl patch secret postgres-credentials \
        --namespace odo-core \
        --type=merge \
        -p "{\"stringData\":{\"POSTGRES_PASSWORD\":\"${POSTGRES_PASSWORD}\",\"POSTGRES_HOST\":\"${db_host}\",\"POSTGRES_PORT\":\"${db_port}\",\"POSTGRES_USER\":\"${db_user}\",\"POSTGRES_DB\":\"${db_name}\"}}"

    echo "PostgreSQL credentials patched into secrets"
}

setup_postgres() {
    if [[ "$POSTGRES_MODE" == "external" ]]; then
        print_section "Using external PostgreSQL"
        echo "Skipping containerized PostgreSQL; using ${POSTGRES_HOST}:${POSTGRES_PORT}"
        return
    fi

    print_section "Building and deploying PostgreSQL"

    sudo docker build -t localhost:${LOCAL_REGISTRY_PORT}/odo-postgres:latest k8s/infrastructure/postgres/
    sudo docker push localhost:${LOCAL_REGISTRY_PORT}/odo-postgres:latest
    kctl apply -k ./k8s/infrastructure/postgres/

    echo "Waiting for PostgreSQL to become ready..."

    local deadline=$((SECONDS + 120))
    until kctl get pod -l app=postgres --namespace odo-core --no-headers 2>/dev/null | grep -q .; do
        if [ $SECONDS -ge $deadline ]; then
            echo -e "${RED}Timed out waiting for PostgreSQL pod to be created${NC}"
            exit 1
        fi
        sleep 5
    done

    kctl wait --for=condition=Ready pod -l app=postgres --namespace odo-core --timeout=120s

    echo "PostgreSQL deployed"
}

deploy_database_schema() {
    print_section "Deploying database schema + seed"

    # manage-database.sh resolves the endpoint from the secret's
    # EXTERNAL_DATABASE_URL (patched above) - no PG* env vars needed.
    # Deploys the sqitch plan: baseline schema + the generic platform
    # seed (permissions, platform roles, machine accounts, demo org tree).
    sudo env KUBECONFIG="$KUBECONFIG_PATH" ./scripts/manage-database.sh deploy

    echo "Database schema + seed deployed"
}

deploy_test_data() {
    print_section "Deploying e2e/dev test data"

    # Flat idempotent fixtures (src/test-data): the e2e.odo.* users,
    # the login-only e2e-test-role, MockSAML config, soft-deleted rows.
    # Safe to re-run at any time.
    sudo env KUBECONFIG="$KUBECONFIG_PATH" ./scripts/manage-database.sh deploy-test

    echo "Test data deployed"
}

generate_jwt_secret() {
    print_section "Generating JWT secret"

    sudo env KUBECONFIG="$KUBECONFIG_PATH" ./scripts/manage-secrets.sh update-jwt

    echo "JWT secret generated"
}

build_and_deploy_services() {
    print_section "Building and deploying all services (this will take a while)"

    sudo env KUBECONFIG="$KUBECONFIG_PATH" ./scripts/build-and-deploy-service.sh --all

    echo "All services deployed"
}

setup_logging() {
    print_section "Setting up logging (Fluent Bit)"

    sudo mkdir -p /var/log/odo
    sudo chown root:"$K3S_GROUP" /var/log/odo
    sudo chmod 750 /var/log/odo

    sudo tee /etc/logrotate.d/odo > /dev/null <<'EOF'
/var/log/odo/services.log {
    daily
    rotate 5
    compress
    delaycompress
    missingok
    notifempty
    create 644 root root
}
EOF

    sudo systemctl restart logrotate

    kctl apply -k ./k8s/infrastructure/fluent-bit/

    echo "Fluent Bit deployed, logs at /var/log/odo"
}

print_post_install() {
    print_section "Dev Cluster Setup Complete!"

    echo -e "${GREEN}The k3s dev cluster is fully initialized and all services are deployed.${NC}"
    echo
    echo -e "${YELLOW}Log out and back in to activate k3s and docker group memberships${NC}"
    echo -e "${YELLOW}for interactive use.${NC}"
    echo
    echo "The database carries the platform seed (demo org tree 'Odo"
    echo "Library System', machine accounts) and the e2e fixtures."
    echo
    echo "Dev tooling resolves its DB connection from the secrets"
    echo "(EXTERNAL_DATABASE_URL) - no PG* environment variables needed:"
    echo "  ./scripts/run-tests.sh --db --integration --e2e --unit"
    echo "  ./scripts/manage-database.sh status"
    echo "  ./scripts/manage-secrets.sh show-secret"
    echo
    echo "Test users (password test123!): e2e.odo.staff (login-only),"
    echo "e2e.odo.admin (odo-admin). Machine accounts: odo-registration"
    echo "(apps register their data with it via the odo-register tool)"
    echo "and odo-notify-service - rotate their dev-default passwords"
    echo "for anything beyond a local dev cluster."
    echo
    echo "Applications (e.g. kcls/current) install their own services,"
    echo "routes, and registration on top - see docs/app-repo-structure.md."
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
    confirm
    prompt_postgres_credentials

    # k3s cluster
    install_prerequisites
    setup_k3s_group
    install_k3s
    setup_kubeconfig_env

    # system packages & docker
    install_system_packages
    install_docker
    setup_local_registry_config
    restart_k3s

    # dev tools
    install_k9s
    install_yq
    install_node
    install_rust
    install_sea_orm_cli

    # cluster initialization
    setup_logging
    run_local_registry
    install_envoy_gateway
    apply_namespaces_and_secrets
    setup_postgres
    deploy_database_schema
    deploy_test_data
    generate_jwt_secret
    build_and_deploy_services

    print_post_install
}

case "${1:-}" in
    --help|-h)
        echo "Usage: $0"
        echo
        echo "Full dev cluster setup for Ubuntu. Installs k3s, Docker, dev tools,"
        echo "and initializes the cluster with all services."
        echo
        echo "Run as a normal user from the project root directory."
        echo "The script uses sudo where needed."
        exit 0
        ;;
esac

main
