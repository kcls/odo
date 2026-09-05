#!/bin/bash
# Install a k3s cluster for Odo on Debian or Ubuntu.
#
# Installs k3s and Docker, deploys the cluster infrastructure (Envoy
# Gateway, Fluent Bit, namespaces, secrets), and points the platform at an
# external PostgreSQL server. Usable for production as well as development:
# it installs no language toolchains, no test rig, and deploys no services.
#
# PostgreSQL runs OUTSIDE the cluster. Install it first with
# ./scripts/setup/install-postgres-server-ubuntu.sh, or bring your own
# server; either way this script asks for its address.
#
# For a development machine, follow this with
# ./scripts/setup/setup-dev-cluster-ubuntu.sh, which installs the
# toolchains and builds and deploys the services.
#
# Run as a normal user from the project root directory. The script uses
# sudo and sg where needed; no logout/login required during execution. Log
# out and back in afterward for interactive use.

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/setup-common.sh"

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
# Detect distribution
# ---------------------------------------------------------------------------
# Debian and Ubuntu differ here in exactly one place: which Docker apt
# repository to pull from. Everything else is plain apt or a distro-agnostic
# upstream installer. Read the fields in a subshell so /etc/os-release
# cannot clobber a variable this script defines (it sets NAME, VERSION and
# friends unconditionally).
os_release() {
    (. /etc/os-release 2>/dev/null && printf '%s' "${!1}")
}

# ID_LIKE is what catches the derivatives -- Linux Mint and Pop!_OS report
# ID_LIKE=ubuntu, Raspberry Pi OS reports ID_LIKE=debian -- and Docker
# publishes for the parent distribution, never the derivative. Ubuntu is
# tested first because Pop!_OS lists both.
case " $(os_release ID) $(os_release ID_LIKE) " in
    *" ubuntu "*) DOCKER_DISTRO="ubuntu" ;;
    *" debian "*) DOCKER_DISTRO="debian" ;;
    *)
        echo "Unsupported distribution: $(os_release PRETTY_NAME)"
        echo "This installer expects Debian or Ubuntu, or a derivative of either."
        exit 1
        ;;
esac

# Same reason: an Ubuntu derivative puts its own codename in
# VERSION_CODENAME (Mint 22 says 'wilma'), which Docker's repository does
# not carry, and names the Ubuntu release it tracks in UBUNTU_CODENAME.
# Debian has no such split, so VERSION_CODENAME is the codename there.
DOCKER_SUITE="$(os_release UBUNTU_CODENAME)"
DOCKER_SUITE="${DOCKER_SUITE:-$(os_release VERSION_CODENAME)}"

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

ENVOY_GATEWAY_VERSION="v1.7.2"

LOCAL_REGISTRY_PORT="32000"       # host port; container listens on 5000

# The database endpoint written into the postgres-credentials secret.
# Populated by prompt_database_connection, or parsed from --database-url.
DB_HOST=""
DB_PORT=""
DB_USER=""
DB_NAME=""
DB_PASSWORD=""
DATABASE_URL_ARG=""

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

kctl() {
    sudo kubectl --kubeconfig="$KUBECONFIG_PATH" "$@"
}

prompt_database_connection() {
    print_section "PostgreSQL connection"

    if [[ -n "$DATABASE_URL_ARG" ]]; then
        parse_database_url "$DATABASE_URL_ARG" \
            || die "Could not parse user:password@host:port/db out of --database-url"
        if is_loopback_host "$DB_HOST"; then
            die "--database-url names a loopback host; pods would dial themselves."
        fi
        echo "Using $(mask_database_url "$DATABASE_URL_ARG")"
        return
    fi

    explain_db_address
    echo

    # install-postgres-server-ubuntu.sh records what it configured, so a
    # single-host install can just press enter through these.
    load_db_state

    prompt_db_host "$ODO_DB_HOST"

    read -rp "PostgreSQL port [${ODO_DB_PORT:-$ODO_DB_PORT_DEFAULT}]: " DB_PORT
    DB_PORT="${DB_PORT:-${ODO_DB_PORT:-$ODO_DB_PORT_DEFAULT}}"

    read -rp "Odo superuser role [${ODO_DB_USER:-$ODO_DB_USER_DEFAULT}]: " DB_USER
    DB_USER="${DB_USER:-${ODO_DB_USER:-$ODO_DB_USER_DEFAULT}}"

    read -rp "Odo database name [${ODO_DB_NAME:-$ODO_DB_NAME_DEFAULT}]: " DB_NAME
    DB_NAME="${DB_NAME:-${ODO_DB_NAME:-$ODO_DB_NAME_DEFAULT}}"

    prompt_db_password DB_PASSWORD "$DB_USER" false

    echo
    echo "That role must exist on the server and be a superuser; the"
    echo "database itself is created later by the schema deploy."
}

# The secret is only as good as the endpoint in it, so prove it before the
# rest of the install builds on it.
verify_database_connection() {
    print_section "Verifying the PostgreSQL connection"

    local status=0
    check_db_connection "$DB_HOST" "$DB_PORT" "$DB_USER" "$DB_PASSWORD" || status=$?

    if [[ $status -eq 0 ]]; then
        echo "Connected to ${DB_HOST}:${DB_PORT} as '${DB_USER}'"
        return
    fi

    if [[ $status -eq 2 ]]; then
        echo -e "${YELLOW}psql is not installed yet; skipping the check.${NC}"
        echo "It runs again from ./scripts/setup/setup-dev-cluster-ubuntu.sh."
        return
    fi

    echo -e "${RED}Cannot connect to ${DB_HOST}:${DB_PORT} as '${DB_USER}'.${NC}"
    echo
    echo "The server needs:"
    echo "  - a superuser role '${DB_USER}' with the password entered above"
    echo "  - listen_addresses and a pg_hba.conf entry covering this host"
    echo "    and the cluster pod network (${K3S_POD_CIDR})"
    echo
    echo "./scripts/setup/install-postgres-server-ubuntu.sh sets all of that up."
    exit 1
}

confirm() {
    echo
    echo -e "${YELLOW}This will install k3s and Docker and initialize the cluster"
    echo -e "infrastructure. It takes a while and requires sudo.${NC}"
    echo
    echo -e "${YELLOW}No application services are built or deployed. On a development"
    echo -e "machine, run ./scripts/setup/setup-dev-cluster-ubuntu.sh next.${NC}"
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
    # postgresql-client is here rather than with the rest of the system
    # packages so the database check below can run before the long installs.
    sudo apt-get install -y curl wget ca-certificates postgresql-client
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
            die "Timed out waiting for k3s node to register"
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
        wget \
        jq \
        ca-certificates

    # Build toolchains, sqitch and the test rig are development concerns:
    # setup-dev-cluster-ubuntu.sh installs those.
    echo "System packages installed"
}

install_docker() {
    print_section "Installing Docker"

    if command -v docker >/dev/null 2>&1; then
        echo "Docker is already installed"
        add_user_to_docker_group
        return
    fi

    # Only reached when Docker is not installed, so a codename we cannot
    # resolve is fatal here rather than at the top of the script: a host
    # that already has Docker never needs it.
    if [[ -z "$DOCKER_SUITE" ]]; then
        echo "Could not read a release codename from /etc/os-release, so there"
        echo "is no way to tell which Docker repository suite to use."
        echo "Install Docker by hand, then re-run this script."
        exit 1
    fi

    echo "Using Docker's $DOCKER_DISTRO repository, suite '$DOCKER_SUITE'"

    sudo apt-get update
    sudo apt-get install -y ca-certificates curl

    sudo install -m 0755 -d /etc/apt/keyrings
    sudo curl -fsSL "https://download.docker.com/linux/${DOCKER_DISTRO}/gpg" \
        -o /etc/apt/keyrings/docker.asc
    sudo chmod a+r /etc/apt/keyrings/docker.asc

    sudo tee /etc/apt/sources.list.d/docker.sources > /dev/null <<EOF
Types: deb
URIs: https://download.docker.com/linux/${DOCKER_DISTRO}
Suites: ${DOCKER_SUITE}
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

    add_user_to_docker_group
    echo "Docker installed successfully"
}

# setup-dev-cluster-ubuntu.sh reaches the daemon through `sg docker`, which
# needs the membership on record even when Docker was already installed and
# the branch above returned early.
add_user_to_docker_group() {
    if user_in_group docker; then
        echo "User '$USER' is already in the docker group"
        return
    fi

    sudo usermod -aG docker "$USER"
    echo "User '$USER' added to group 'docker'"
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
            die "Timed out waiting for k3s node to register"
        fi
        sleep 5
    done

    kctl wait --for=condition=Ready node --all --timeout=120s
    echo "k3s restarted and ready"
}

# ---------------------------------------------------------------------------
# Cluster tooling
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

# ---------------------------------------------------------------------------
# Cluster Initialization
# ---------------------------------------------------------------------------

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
    # One DATABASE_URL, read by the service pods and by the host tooling
    # (manage-database.sh, the test runners) alike - which is why the host
    # in it has to resolve the same way in both places.
    local db_url
    db_url="$(build_database_url "$DB_USER" "$DB_PASSWORD" "$DB_HOST" "$DB_PORT" "$DB_NAME")"

    local ns
    for ns in odo-core odo-pub; do
        kctl create secret generic postgres-credentials \
            --namespace "$ns" \
            --from-literal=DATABASE_URL="$db_url" \
            --dry-run=client -o yaml | kctl apply -f -
    done

    echo "DATABASE_URL patched into postgres-credentials (odo-core, odo-pub)"
    echo "  $(mask_database_url "$db_url")"
}

generate_jwt_secret() {
    print_section "Generating JWT secret"

    sudo env KUBECONFIG="$KUBECONFIG_PATH" ./scripts/manage-secrets.sh update-jwt

    echo "JWT secret generated"
}

print_post_install() {
    print_section "Cluster Setup Complete"

    echo -e "${GREEN}k3s is running and the cluster infrastructure is deployed.${NC}"
    echo
    echo -e "${YELLOW}Log out and back in to activate the k3s and docker group${NC}"
    echo -e "${YELLOW}memberships for interactive use.${NC}"
    echo
    echo "Once logged back in, verify access to to the cluster: "
    echo "  kubectl get pods -A"
    echo
    echo "On a development machine, continue with:"
    echo "  ./scripts/setup/setup-dev-cluster-ubuntu.sh --with-test-deps"
    echo
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

main() {
    check_not_root
    require_project_root
    confirm
    prompt_database_connection

    # k3s cluster (the database is checked first: everything below is
    # built on that endpoint, and the installs are long)
    install_prerequisites
    verify_database_connection
    setup_k3s_group
    install_k3s
    setup_kubeconfig_env

    # system packages & docker
    install_system_packages
    install_docker
    setup_local_registry_config
    restart_k3s

    # these are generally useful
    install_k9s
    install_yq

    # cluster initialization
    setup_logging
    install_envoy_gateway
    apply_namespaces_and_secrets
    generate_jwt_secret

    print_post_install
}

usage() {
    echo "Usage: $0 [--database-url <url>]"
    echo
    echo "Installs k3s and Docker on this Debian or Ubuntu host and"
    echo "initializes the Odo cluster infrastructure: Envoy Gateway, Fluent"
    echo "Bit, namespaces, secrets, and the JWT keypair."
    echo
    echo "PostgreSQL runs outside the cluster; the script asks for its"
    echo "address and stores a single DATABASE_URL in the"
    echo "postgres-credentials secret. That address must be reachable both"
    echo "from inside the cluster and from this host, so 'localhost' is not"
    echo "an option. Install a server first with"
    echo "./scripts/setup/install-postgres-server-ubuntu.sh, or bring your own."
    echo
    echo "No language toolchains, no test rig, and no application services:"
    echo "./scripts/setup/setup-dev-cluster-ubuntu.sh adds those."
    echo
    echo "Options:"
    echo "  --database-url <url>  postgres://user:pass@host:port/db - skips"
    echo "                        the interactive connection prompts"
    echo "  --help, -h            Show this message"
    echo
    echo "Environment variables:"
    echo "  K3S_POD_CIDR       Pod CIDR quoted in the connection diagnostics"
    echo "                     (default: 10.42.0.0/16)"
    echo "  ODO_SETUP_STATE    Endpoint recorded by the PostgreSQL"
    echo "                     installer, used for prompt defaults"
    echo "                     (default: ~/.config/odo/setup.env)"
    echo
    echo "Run as a normal user from the project root directory."
    echo "The script uses sudo where needed."
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --database-url)
            DATABASE_URL_ARG="$2"
            [[ -z "$DATABASE_URL_ARG" ]] && { echo "--database-url requires a value" >&2; exit 1; }
            shift 2
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
