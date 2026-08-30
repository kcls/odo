#!/bin/bash
# Shared helpers for the scripts/setup/ Ubuntu installers. Sourced, never
# executed directly.
#
#   install-postgres-server-ubuntu.sh   PostgreSQL server (prod or dev)
#   install-k3s-cluster-ubuntu.sh       k3s + Docker + cluster (prod or dev)
#   setup-dev-cluster-ubuntu.sh         dev-only tooling, schema, services

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m'

# The PostgreSQL installer records the endpoint it configured here so the
# cluster installer can offer it as a default when both run on one host.
# Never holds the password.
ODO_SETUP_STATE="${ODO_SETUP_STATE:-$HOME/.config/odo/setup.env}"

# Default database name and role. The cluster installer prompts for both.
ODO_DB_NAME_DEFAULT="odo"
ODO_DB_USER_DEFAULT="odo"
ODO_DB_PORT_DEFAULT="5432"

# k3s default cluster CIDR - the source range pods connect from when the
# CNI does not masquerade them onto the node address.
K3S_POD_CIDR="${K3S_POD_CIDR:-10.42.0.0/16}"

print_section() {
    echo
    echo -e "${YELLOW}>>> $1${NC}"
    echo
}

die() {
    echo -e "${RED}$*${NC}" >&2
    exit 1
}

check_not_root() {
    if [ "$EUID" -eq 0 ]; then
        die "Error: Run this script as a normal user, not root."
    fi
}

require_project_root() {
    if [[ ! -d ./scripts || ! -d ./k8s ]]; then
        die "Error: Run this script from the project root directory."
    fi
}

# ---------------------------------------------------------------------------
# The database address
#
# There is ONE database URL. The service pods and the host tooling both read
# it, so it has to name an address that resolves and routes the same way in
# both places: a LAN IP or a DNS name, never a loopback address.
# ---------------------------------------------------------------------------

is_loopback_host() {
    case "${1,,}" in
        ""|localhost|localhost.localdomain|::1|0.0.0.0) return 0 ;;
        127.*) return 0 ;;
    esac
    return 1
}

# Best guess at an address this machine is reachable on: the source address
# the kernel picks for the default route. Empty if it cannot be determined.
guess_host_address() {
    ip -4 route get 1.1.1.1 2>/dev/null \
        | awk '{ for (i = 1; i < NF; i++) if ($i == "src") { print $(i + 1); exit } }'
}

explain_db_address() {
    cat <<'EOF'
PostgreSQL runs outside the cluster, and a single DATABASE_URL is shared by
the service pods and by the host tooling (manage-database.sh, the test
runners). It therefore needs an address that is reachable from BOTH:

  - a LAN IP address of the database host, or
  - a DNS name that resolves to it from inside the cluster and on the host

'localhost' and 127.0.0.1 are NOT valid: inside a pod they point at the pod.
A DHCP address works but pins the cluster to that lease - prefer a static
address or a DNS name for anything long-lived.
EOF
}

# Prompt for the shared database address. Sets DB_HOST.
# Usage: prompt_db_host [default]
prompt_db_host() {
    local default="$1"
    local prompt="Database host (IP or DNS name)"
    [[ -n "$default" ]] && prompt+=" [$default]"

    while true; do
        read -rp "${prompt}: " DB_HOST
        DB_HOST="${DB_HOST:-$default}"

        if [[ -z "$DB_HOST" ]]; then
            echo -e "${RED}A host is required.${NC}"
            continue
        fi
        if is_loopback_host "$DB_HOST"; then
            echo -e "${RED}'${DB_HOST}' is a loopback address; pods would dial themselves.${NC}"
            echo "Use a LAN address or a DNS name reachable from inside the cluster."
            continue
        fi
        return 0
    done
}

# Prompt for a password into the named variable.
# Usage: prompt_db_password <varname> <label> [confirm]
prompt_db_password() {
    local varname="$1" label="$2" confirm="${3:-false}"
    local value confirmation

    while true; do
        read -rsp "PostgreSQL password for '${label}': " value
        echo
        if [[ -z "$value" ]]; then
            echo -e "${RED}Password cannot be empty.${NC}"
            continue
        fi
        # The password is carried inside a postgres:// URL that the tooling
        # parses by hand, so characters that delimit the URL are rejected
        # rather than silently mangled.
        if [[ "$value" =~ [@/?\#%[:space:]] ]]; then
            echo -e "${RED}Password cannot contain @ / ? # % or whitespace.${NC}"
            continue
        fi
        if [[ "$confirm" == "true" ]]; then
            read -rsp "Confirm password: " confirmation
            echo
            if [[ "$value" != "$confirmation" ]]; then
                echo -e "${RED}Passwords do not match. Try again.${NC}"
                continue
            fi
        fi
        printf -v "$varname" '%s' "$value"
        return 0
    done
}

build_database_url() {
    local user="$1" password="$2" host="$3" port="$4" db="$5" sslmode="${6:-disable}"
    printf 'postgres://%s:%s@%s:%s/%s?sslmode=%s' \
        "$user" "$password" "$host" "$port" "$db" "$sslmode"
}

# Parse postgres://user:pass@host:port/db?params into DB_USER, DB_PASSWORD,
# DB_HOST, DB_PORT, DB_NAME. Returns 1 if the URL is not parseable.
parse_database_url() {
    local url="$1"

    case "$url" in
        postgres://*|postgresql://*) ;;
        *) return 1 ;;
    esac

    local rest="${url#*://}"
    local userinfo="${rest%%@*}"
    local hostpart="${rest#*@}"
    local hostport="${hostpart%%/*}"
    local dbpart="${hostpart#*/}"

    DB_USER="${userinfo%%:*}"
    DB_PASSWORD="${userinfo#*:}"
    DB_HOST="${hostport%%:*}"
    DB_PORT="${hostport#*:}"
    [[ "$DB_PORT" == "$DB_HOST" ]] && DB_PORT="$ODO_DB_PORT_DEFAULT"
    DB_NAME="${dbpart%%\?*}"

    [[ -n "$DB_USER" && -n "$DB_PASSWORD" && -n "$DB_HOST" && -n "$DB_NAME" ]]
}

mask_database_url() {
    local url="$1"
    local rest="${url#*://}"
    local userinfo="${rest%%@*}"
    echo "postgres://${userinfo%%:*}:****@${rest#*@}"
}

# ---------------------------------------------------------------------------
# Installer state (host/port/user/db only - never the password)
# ---------------------------------------------------------------------------

save_db_state() {
    local dir
    dir="$(dirname "$ODO_SETUP_STATE")"
    mkdir -p "$dir"

    cat > "$ODO_SETUP_STATE" <<EOF
# Written by scripts/setup/install-postgres-server-ubuntu.sh.
# Defaults for the cluster installer; no password is stored here.
ODO_DB_HOST=${DB_HOST}
ODO_DB_PORT=${DB_PORT}
ODO_DB_USER=${DB_USER}
ODO_DB_NAME=${DB_NAME}
EOF
    chmod 600 "$ODO_SETUP_STATE"

    echo "Recorded the endpoint in ${ODO_SETUP_STATE}"
}

load_db_state() {
    [[ -r "$ODO_SETUP_STATE" ]] || return 0
    # shellcheck disable=SC1090
    source "$ODO_SETUP_STATE"
}

# Is $USER a member of a group? Reads /etc/group rather than `id`, so a
# membership granted earlier in the install counts even though it is not
# active in this login session yet (which is what `sg` works around).
user_in_group() {
    local grp="$1"

    [[ "$(id -gn)" == "$grp" ]] && return 0

    local members
    members="$(getent group "$grp" 2>/dev/null | cut -d: -f4)" || return 1

    local member
    for member in ${members//,/ }; do
        [[ "$member" == "$USER" ]] && return 0
    done
    return 1
}

# ---------------------------------------------------------------------------
# Connectivity
# ---------------------------------------------------------------------------

# Connect to the maintenance database. The odo database may not exist yet.
# Usage: check_db_connection <host> <port> <user> <password>
check_db_connection() {
    local host="$1" port="$2" user="$3" password="$4"

    command -v psql >/dev/null 2>&1 || return 2

    PGPASSWORD="$password" psql -h "$host" -p "$port" -U "$user" \
        -d postgres -c 'SELECT 1' >/dev/null 2>&1
}
