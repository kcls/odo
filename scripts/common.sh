#!/bin/bash
# Common functions and utilities for Odo database scripts
# This file is meant to be sourced by other scripts

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Default namespace for Kubernetes secrets
DEFAULT_NAMESPACE=${NAMESPACE:-odo-core}

# Function to get secret value from Kubernetes
get_secret_value() {
    local namespace=$1
    local secret_name=$2
    local key=$3
    kubectl get secret $secret_name -n $namespace -o jsonpath="{.data.$key}" 2>/dev/null | base64 -d
}

# Function to initialize PostgreSQL connection parameters
# Sets: PGHOST, PGPORT, PGDATABASE, PGUSER, PGPASSWORD
# Returns: 0 on success, 1 on failure
# Only prints output on error
init_pg_connection() {
    local namespace=${1:-$DEFAULT_NAMESPACE}
    local errors=""

    # Check if kubectl is available
    if ! command -v kubectl &> /dev/null; then
        echo -e "${RED}kubectl command not found${NC}" >&2
        return 1
    fi

    # The postgres-credentials secret stores connection URLs
    # (postgres://user:pass@host:port/db?params): EXTERNAL_DATABASE_URL is
    # the host-reachable endpoint for tooling like this; DATABASE_URL is
    # the in-cluster endpoint the services use. Prefer the external one,
    # fall back to the in-cluster one (overridable via PG* env vars).
    local db_url=$(get_secret_value $namespace postgres-credentials EXTERNAL_DATABASE_URL)
    if [ -z "$db_url" ]; then
        db_url=$(get_secret_value $namespace postgres-credentials DATABASE_URL)
    fi

    local SECRET_PGHOST="" SECRET_PGPORT="" SECRET_PGDATABASE="" \
          SECRET_PGUSER="" SECRET_PGPASSWORD=""
    if [ -n "$db_url" ]; then
        # Strip scheme -> user:pass@host:port/db?params
        local rest="${db_url#*://}"
        local userinfo="${rest%%@*}"      # user:pass
        local hostpart="${rest#*@}"       # host:port/db?params
        SECRET_PGUSER="${userinfo%%:*}"
        SECRET_PGPASSWORD="${userinfo#*:}"
        local hostport="${hostpart%%/*}"  # host:port
        SECRET_PGHOST="${hostport%%:*}"
        SECRET_PGPORT="${hostport#*:}"
        local dbpart="${hostpart#*/}"     # db?params
        SECRET_PGDATABASE="${dbpart%%\?*}"
    fi

    # Use environment variables for host and port if provided, otherwise use secret values, otherwise use defaults
    if [ -n "$PGHOST" ]; then
        : # Using PGHOST from environment
    elif [ -n "$SECRET_PGHOST" ]; then
        PGHOST="$SECRET_PGHOST"
    else
        PGHOST="localhost"
    fi

    if [ -n "$PGPORT" ]; then
        : # Using PGPORT from environment
    elif [ -n "$SECRET_PGPORT" ]; then
        PGPORT="$SECRET_PGPORT"
    else
        PGPORT="5432"
    fi

    # Use environment variables for database, user, and password if provided, otherwise use secret values
    if [ -n "$PGDATABASE" ]; then
        : # Using PGDATABASE from environment
    elif [ -n "$SECRET_PGDATABASE" ]; then
        PGDATABASE="$SECRET_PGDATABASE"
    else
        errors+="PGDATABASE not set in environment or secret\n"
    fi

    if [ -n "$PGUSER" ]; then
        : # Using PGUSER from environment
    elif [ -n "$SECRET_PGUSER" ]; then
        PGUSER="$SECRET_PGUSER"
    else
        errors+="PGUSER not set in environment or secret\n"
    fi

    if [ -n "$PGPASSWORD" ]; then
        : # Using PGPASSWORD from environment
    elif [ -n "$SECRET_PGPASSWORD" ]; then
        PGPASSWORD="$SECRET_PGPASSWORD"
    else
        errors+="PGPASSWORD not set in environment or secret\n"
    fi

    if [ -z "$PGDATABASE" ] || [ -z "$PGUSER" ] || [ -z "$PGPASSWORD" ]; then
        echo -e "${RED}Failed to retrieve PostgreSQL credentials:${NC}" >&2
        echo -e "${RED}${errors}${NC}" >&2
        echo "Ensure postgres-credentials secret exists in $namespace namespace" >&2
        echo "Or provide PGDATABASE, PGUSER, and PGPASSWORD environment variables" >&2
        echo >&2
        echo "Connection details:" >&2
        echo "  Host: ${PGHOST:-<not set>}" >&2
        echo "  Port: ${PGPORT:-<not set>}" >&2
        echo "  Database: ${PGDATABASE:-<not set>}" >&2
        echo "  User: ${PGUSER:-<not set>}" >&2
        return 1
    fi

    # Export variables so they're available to psql and other commands
    export PGHOST
    export PGPORT
    export PGDATABASE
    export PGUSER
    export PGPASSWORD

    return 0
}

# Function to execute PostgreSQL command
# Usage: execute_psql "SQL command" [database] [use_sudo]
execute_psql() {
    local sql="$1"
    local db="${2:-$PGDATABASE}"
    local use_sudo="${3:-false}"

    if [ "$use_sudo" == "true" ] && ([ "$PGHOST" == "localhost" ] || [ "$PGHOST" == "127.0.0.1" ]); then
        # Use sudo only for initial user creation on local database
        sudo -u postgres psql -d "$db" -c "$sql"
    else
        # Use the credentials from environment
        PGPASSWORD="$PGPASSWORD" psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" -d "$db" -c "$sql"
    fi
}

# Function to execute PostgreSQL query and return result
# Usage: query_psql "SQL query" [database]
query_psql() {
    local sql="$1"
    local db="${2:-$PGDATABASE}"

    PGPASSWORD="$PGPASSWORD" psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" -d "$db" -qtAc "$sql"
}

# Function to connect interactively to the database
connect_psql() {
    PGPASSWORD="$PGPASSWORD" psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" -d "$PGDATABASE"
}

# Function to test database connection
# Returns: 0 on success, 1 on failure
test_connection() {
    local test_query="SELECT 1"

    if PGPASSWORD="$PGPASSWORD" psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" -d "$PGDATABASE" -c "$test_query" &>/dev/null; then
        return 0
    else
        return 1
    fi
}
