#!/bin/bash
# Install and configure a PostgreSQL server for Odo on this Ubuntu host.
#
# Odo runs PostgreSQL OUTSIDE the Kubernetes cluster. This script installs
# the server from the PGDG apt repo, opens it to the cluster, and creates
# the superuser role the platform connects as. It is usable for production
# as well as development: it touches nothing but PostgreSQL.
#
# Skip this script entirely if you already have a PostgreSQL server - point
# the cluster installer at it instead.
#
# Run as a normal user; the script uses sudo where needed.

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/setup-common.sh"

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

# Leave POSTGRES_VERSION empty to take the latest stable major the PGDG apt
# repo offers, or pin one: POSTGRES_VERSION=17 ./install-postgres-server-ubuntu.sh
POSTGRES_VERSION="${POSTGRES_VERSION:-}"
POSTGRES_APT_SOURCES="/etc/apt/sources.list.d/pgdg.sources"

PG_HBA_BEGIN="# >>> odo (managed by install-postgres-server-ubuntu.sh) >>>"
PG_HBA_END="# <<< odo <<<"

# Install the pgtap extension, which ./scripts/run-db-tests.sh needs.
# Set by --with-pgtap; a production server has no use for it.
WITH_PGTAP="false"

# Filled by prompt_settings.
DB_HOST=""
DB_PORT=""
DB_USER=""
DB_NAME=""
DB_PASSWORD=""
DB_CLIENT_CIDRS=""

# ---------------------------------------------------------------------------
# Prompts
# ---------------------------------------------------------------------------

prompt_settings() {
    print_section "PostgreSQL server settings"

    explain_db_address
    echo

    prompt_db_host "$(guess_host_address)"

    read -rp "PostgreSQL port [${ODO_DB_PORT_DEFAULT}]: " DB_PORT
    DB_PORT="${DB_PORT:-$ODO_DB_PORT_DEFAULT}"

    read -rp "Odo superuser role [${ODO_DB_USER_DEFAULT}]: " DB_USER
    DB_USER="${DB_USER:-$ODO_DB_USER_DEFAULT}"

    read -rp "Odo database name [${ODO_DB_NAME_DEFAULT}]: " DB_NAME
    DB_NAME="${DB_NAME:-$ODO_DB_NAME_DEFAULT}"

    prompt_db_password DB_PASSWORD "$DB_USER" true

    echo
    echo "pg_hba.conf needs to allow the clients that will connect: the"
    echo "cluster nodes (pod traffic usually arrives masqueraded onto the"
    echo "node address) and any host running the odo tooling."
    local cidr_default="${DB_HOST}/32 ${K3S_POD_CIDR}"
    read -rp "Allowed client CIDRs [${cidr_default}]: " DB_CLIENT_CIDRS
    DB_CLIENT_CIDRS="${DB_CLIENT_CIDRS:-$cidr_default}"
}

confirm() {
    echo
    echo -e "${YELLOW}About to install PostgreSQL on this host and configure it for Odo:${NC}"
    echo "  address:  ${DB_HOST}:${DB_PORT}"
    echo "  role:     ${DB_USER} (SUPERUSER)"
    echo "  database: ${DB_NAME} (created later by the schema deploy)"
    echo "  clients:  ${DB_CLIENT_CIDRS}"
    if [[ "$WITH_PGTAP" == "true" ]]; then
        echo "  pgtap:    yes (--with-pgtap)"
    fi
    echo
    echo -e "${YELLOW}This requires sudo and rewrites the odo block in pg_hba.conf.${NC}"
    echo

    read -rp "Continue? [y/N] " response
    if [[ ! "$response" =~ ^[Yy]$ ]]; then
        echo "Aborted."
        exit 0
    fi
}

# ---------------------------------------------------------------------------
# Install
# ---------------------------------------------------------------------------

setup_postgres_apt_repo() {
    print_section "Configuring the PostgreSQL (PGDG) apt repository"

    sudo apt-get update
    sudo apt-get install -y curl ca-certificates

    sudo install -m 0755 -d /etc/apt/keyrings
    sudo curl -fsSL https://www.postgresql.org/media/keys/ACCC4CF8.asc \
        -o /etc/apt/keyrings/pgdg.asc
    sudo chmod a+r /etc/apt/keyrings/pgdg.asc

    sudo tee "$POSTGRES_APT_SOURCES" > /dev/null <<EOF
Types: deb
URIs: https://apt.postgresql.org/pub/repos/apt
Suites: $(. /etc/os-release && echo "${UBUNTU_CODENAME:-$VERSION_CODENAME}")-pgdg
Components: main
Architectures: $(dpkg --print-architecture)
Signed-By: /etc/apt/keyrings/pgdg.asc
EOF

    sudo apt-get update

    if [[ -n "$POSTGRES_VERSION" ]]; then
        echo "Using pinned PostgreSQL version $POSTGRES_VERSION"
        return
    fi

    # The PGDG main suite carries released majors only - betas and RCs live
    # in the separate -testing suite - so the highest is the latest stable.
    POSTGRES_VERSION="$(apt-cache search --names-only '^postgresql-[0-9]+$' \
        | awk '{print $1}' | sed 's/^postgresql-//' | sort -n | tail -1)"

    if [[ -z "$POSTGRES_VERSION" ]]; then
        die "Could not determine the latest PostgreSQL version from the PGDG repo"
    fi

    echo "Latest stable PostgreSQL in the PGDG repo: $POSTGRES_VERSION"
}

install_postgres() {
    print_section "Installing PostgreSQL $POSTGRES_VERSION"

    sudo apt-get install -y \
        "postgresql-${POSTGRES_VERSION}" \
        "postgresql-client-${POSTGRES_VERSION}"

    configure_postgres_access
    create_odo_role
}

configure_postgres_access() {
    print_section "Opening PostgreSQL to its clients"

    local conf_dir="/etc/postgresql/${POSTGRES_VERSION}/main"

    if [[ ! -d "$conf_dir" ]]; then
        die "Expected the cluster config at ${conf_dir}, which does not exist"
    fi

    # Debian's postgresql.conf ends with an include_dir; make sure of it
    # before dropping overrides in there.
    if ! sudo grep -qE "^[[:space:]]*include_dir[[:space:]]*=[[:space:]]*'conf\.d'" \
            "${conf_dir}/postgresql.conf"; then
        echo "include_dir = 'conf.d'" | sudo tee -a "${conf_dir}/postgresql.conf" > /dev/null
    fi

    sudo mkdir -p "${conf_dir}/conf.d"

    # Bind every interface rather than pinning the advertised address: that
    # address can change, and a stale listen_addresses stops the server from
    # starting at all. pg_hba.conf below is the access control.
    sudo tee "${conf_dir}/conf.d/10-odo.conf" > /dev/null <<EOF
# Managed by scripts/setup/install-postgres-server-ubuntu.sh
listen_addresses = '*'
port = ${DB_PORT}
EOF

    # Rewritten rather than appended, so a changed client list takes. The
    # pattern is loose enough to also catch blocks left by this script's
    # predecessors (install-k3s-cluster-ubuntu.sh and its earlier names).
    sudo sed -i "\|^# >>> odo.*(managed by .*) >>>$|,\|^# <<< odo.*<<<$|d" \
        "${conf_dir}/pg_hba.conf"

    {
        echo "$PG_HBA_BEGIN"
        echo "# Cluster pods usually arrive masqueraded onto the node address;"
        echo "# the pod CIDR covers a CNI that leaves pod addresses intact."
        local cidr
        for cidr in $DB_CLIENT_CIDRS; do
            printf 'host    all             all             %-24s scram-sha-256\n' "$cidr"
        done
        echo "$PG_HBA_END"
    } | sudo tee -a "${conf_dir}/pg_hba.conf" > /dev/null

    sudo systemctl enable postgresql
    sudo systemctl restart "postgresql@${POSTGRES_VERSION}-main"

    echo "PostgreSQL ${POSTGRES_VERSION} listening on port ${DB_PORT}"
    echo "pg_hba.conf allows: ${DB_CLIENT_CIDRS}"
}

create_odo_role() {
    print_section "Creating the '${DB_USER}' superuser role"

    # Created here, over the local peer connection, rather than left to
    # manage-database.sh: the tooling reaches this server over TCP as
    # ${DB_USER}, so the role has to exist before it can do anything.
    local escaped="${DB_PASSWORD//\'/\'\'}"
    local exists
    exists="$(sudo -u postgres psql -tAc \
        "SELECT 1 FROM pg_roles WHERE rolname='${DB_USER}'" 2>/dev/null)"

    if [[ "$exists" == "1" ]]; then
        sudo -u postgres psql -c \
            "ALTER ROLE \"${DB_USER}\" WITH SUPERUSER LOGIN PASSWORD '${escaped}'" > /dev/null
        echo "Role '${DB_USER}' updated"
    else
        sudo -u postgres psql -c \
            "CREATE ROLE \"${DB_USER}\" WITH SUPERUSER LOGIN PASSWORD '${escaped}'" > /dev/null
        echo "Role '${DB_USER}' created"
    fi
}

install_pgtap() {
    if [[ "$WITH_PGTAP" != "true" ]]; then
        return
    fi

    print_section "Installing the pgtap extension"

    # Server-side half of the pgTAP rig. The client-side runner (pg_prove)
    # is installed on the development host by setup-dev-cluster-ubuntu.sh.
    sudo apt-get install -y "postgresql-${POSTGRES_VERSION}-pgtap"

    echo "pgtap available to PostgreSQL ${POSTGRES_VERSION}"
}

verify_postgres() {
    print_section "Verifying the connection over ${DB_HOST}:${DB_PORT}"

    # Deliberately dialed at the advertised address rather than over the
    # local socket: that is the path the cluster and the tooling will take.
    if check_db_connection "$DB_HOST" "$DB_PORT" "$DB_USER" "$DB_PASSWORD"; then
        echo "Connected to ${DB_HOST}:${DB_PORT} as '${DB_USER}'"
        return
    fi

    echo -e "${RED}Cannot connect to ${DB_HOST}:${DB_PORT} as '${DB_USER}'.${NC}"
    echo
    echo "Check that:"
    echo "  - ${DB_HOST} is an address of this host"
    echo "  - the pg_hba.conf block allows it (${DB_CLIENT_CIDRS})"
    echo "  - nothing between the two is filtering port ${DB_PORT}"
    exit 1
}

print_post_install() {
    print_section "PostgreSQL Setup Complete"

    echo -e "${GREEN}PostgreSQL ${POSTGRES_VERSION} is installed and open to the cluster.${NC}"
    echo
    echo "  address:  ${DB_HOST}:${DB_PORT}"
    echo "  role:     ${DB_USER} (SUPERUSER)"
    echo "  database: ${DB_NAME} (created by the schema deploy)"
    echo "  config:   /etc/postgresql/${POSTGRES_VERSION}/main"
    echo "  clients:  ${DB_CLIENT_CIDRS}"
    echo
    echo "The cluster stores this as a single DATABASE_URL:"
    echo "  $(mask_database_url "$(build_database_url \
        "$DB_USER" "$DB_PASSWORD" "$DB_HOST" "$DB_PORT" "$DB_NAME")")"
    echo
    echo "Next: install the cluster, which asks for these values (and offers"
    echo "them as defaults when run on this host):"
    echo "  ./scripts/setup/install-k3s-cluster-ubuntu.sh"
    echo
    if [[ "$WITH_PGTAP" != "true" ]]; then
        echo "The pgtap extension was NOT installed; ./scripts/run-db-tests.sh"
        echo "needs it. Re-run with --with-pgtap on a development server."
        echo
    fi
    echo "If the address ever changes, update pg_hba.conf here and re-point"
    echo "the cluster with:"
    echo "  ./scripts/manage-secrets.sh update-db-url"
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

main() {
    check_not_root
    prompt_settings
    confirm

    setup_postgres_apt_repo
    install_postgres
    install_pgtap
    verify_postgres

    save_db_state
    print_post_install
}

usage() {
    echo "Usage: $0 [--with-pgtap]"
    echo
    echo "Installs a PostgreSQL server on this Ubuntu host and configures it"
    echo "for Odo: the PGDG apt repo, listen_addresses, a pg_hba.conf block"
    echo "for the cluster, and the odo superuser role."
    echo
    echo "Odo runs PostgreSQL outside the Kubernetes cluster. Skip this"
    echo "script if you already have a server - the cluster installer asks"
    echo "for its address instead."
    echo
    echo "Options:"
    echo "  --with-pgtap   Also install the pgtap extension, which"
    echo "                 ./scripts/run-db-tests.sh needs. Development"
    echo "                 servers only."
    echo "  --help, -h     Show this message"
    echo
    echo "Environment variables:"
    echo "  POSTGRES_VERSION   PostgreSQL major to install (default: the"
    echo "                     latest stable in the PGDG repo)"
    echo "  K3S_POD_CIDR       Pod CIDR offered in the allowed-clients"
    echo "                     default (default: 10.42.0.0/16)"
    echo "  ODO_SETUP_STATE    Where the endpoint is recorded for the"
    echo "                     cluster installer (default:"
    echo "                     ~/.config/odo/setup.env)"
    echo
    echo "Run as a normal user. The script uses sudo where needed."
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --with-pgtap)
            WITH_PGTAP="true"
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
