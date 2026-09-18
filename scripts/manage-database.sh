#!/bin/bash
# Script to manage PostgreSQL database and Sqitch migrations for the Odo platform
# Requires sudo for database setup commands
#
# Connection settings (in order of precedence):
#   1. Environment variables: PGHOST, PGPORT, PGDATABASE, PGUSER, PGPASSWORD
#   2. Kubernetes secret: postgres-credentials in odo-core namespace
#   3. Defaults: localhost:5432 (for host/port only)

set -e

# Get script directory and project root
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Source common database functions
source "$SCRIPT_DIR/common.sh"

# Default values
COMMAND=${1:-help}
DRY_RUN=${DRY_RUN:-false}
NAMESPACE=${NAMESPACE:-odo-core}

# Sqitch directories
# Three trees under src/sqitch, deployed in this order:
#
# Two sqitch projects under src/sqitch, deployed in this order:
#
#   core/       the schema and platform seed. Every install needs it.
#   demo-data/  a separate sqitch project (%project=odo-demo) holding the
#               sample org tree. Depends on odo:002_odo_seed, so it
#               deploys after core and reverts before it. An installation
#               with its own org structure skips it.
#
# Then the dev/CI fixtures, which are not a sqitch project -- plain
# idempotent SQL applied in filename order, with no revert path. They
# live with the suites that consume them, not with the schema.
SQITCH_DIR="${SQITCH_DIR:-$PROJECT_ROOT/src/sqitch}"
SQITCH_CORE_DIR="${SQITCH_CORE_DIR:-$SQITCH_DIR/core}"
SQITCH_DEMO_DIR="${SQITCH_DEMO_DIR:-$SQITCH_DIR/demo-data}"
TEST_DATA_DIR="${TEST_DATA_DIR:-$PROJECT_ROOT/tests/fixtures}"

# Function to show usage
usage() {
    echo -e "${BLUE}Usage: $0 [command] [options]${NC}"
    echo
    echo "Database Commands (require sudo):"
    echo "  setup            Create database, user, and sqitch schema"
    echo "  update-password  Update database user password only"
    echo
    echo "Schema Commands:"
    echo "  deploy [target]  Deploy database schema changes to target (or HEAD)"
    echo "  revert-to [target] Revert database schema changes to target"
    echo "  revert-all       Revert all database schema changes"
    echo "  revert-last      Revert the most recent update"
    echo "  status           Show current schema deployment status"
    echo "  verify           Verify deployed schema changes"
    echo "  log              Show schema deployment history"
    echo
    echo "Demo Data Commands (the sample org tree; skip on a real installation):"
    echo "  deploy-demo      Deploy the demo org tree (requires the schema project)"
    echo "  revert-demo      Revert the demo org tree, leaving the schema in place"
    echo "  status-demo      Show demo project deployment status"
    echo
    echo "Test Data Commands:"
    echo "  deploy-test      Deploy test data (idempotent SQL, tests/fixtures/)"
    echo "  reset-demo       DESTRUCTIVE: revert + redeploy everything (schema + demo), then reload test data"
    echo
    echo "Database Admin Commands:"
    echo "  purge-all        Drop all application schemas (prompts for confirmation)"
    echo
    echo "Environment variables:"
    echo "  PGHOST=hostname           Override database host (default: from the secret)"
    echo "  PGPORT=port               Override database port (default: from the secret)"
    echo "  PGDATABASE=dbname         Override database name (default: from secret)"
    echo "  PGUSER=username           Override database user (default: from secret)"
    echo "  PGPASSWORD=password       Override database password (default: from secret)"
    echo "  DRY_RUN=true              Show what would be done without making changes"
    echo "  NAMESPACE=name            Kubernetes namespace for secrets (default: odo-core)"
    echo "  SQITCH_CORE_DIR=/path     Override core directory (default: $SQITCH_CORE_DIR)"
    echo "  SQITCH_DEMO_DIR=/path     Override demo-data directory (default: $SQITCH_DEMO_DIR)"
    echo "  TEST_DATA_DIR=/path       Override fixtures directory (default: $TEST_DATA_DIR)"
    echo
    echo "Note: Database credentials are retrieved from Kubernetes secret by default"
    echo "      but can be overridden with environment variables"
    # Caller decides the exit status: asking for help succeeds, being
    # shown it after a bad command does not.
    exit "${1:-0}"
}

# Check if running with sudo (only for initial user creation)
check_sudo() {
    if [ "$EUID" -ne 0 ]; then 
        echo -e "${RED}Please run with sudo for initial user creation${NC}"
        exit 1
    fi
}

# Initialize PostgreSQL connection parameters
if ! init_pg_connection "$NAMESPACE"; then
    exit 1
fi

# Alias for backward compatibility
connect() {
    connect_psql
}

# Function to execute PostgreSQL command
execute_psql() {
    local sql="$1"
    local db="${2:-postgres}"
    local use_sudo="${3:-false}"

    if [[ "$DRY_RUN" == "true" ]]; then
        echo -e "${YELLOW}[DRY RUN] Would execute:${NC}"
        echo "$sql"
        return
    fi

    if [ "$use_sudo" == "true" ] && ([ "$PGHOST" == "localhost" ] || [ "$PGHOST" == "127.0.0.1" ]); then
        # Use sudo only for initial user creation on local database
        sudo -u postgres psql -d "$db" -c "$sql"
    else
        # Use the superuser account credentials
        PGPASSWORD="$PGPASSWORD" psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" -d "$db" -c "$sql"
    fi
}

# Connectivity check against the maintenance database. `setup` runs before
# $PGDATABASE exists, so test_connection - which dials $PGDATABASE - cannot
# answer "can this role log in?" on a server that has never been set up.
test_maintenance_connection() {
    PGPASSWORD="$PGPASSWORD" psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" \
        -d postgres -c "SELECT 1" &>/dev/null
}

# Function to check if database exists
database_exists() {
    local db=$1
    local result
    local use_sudo="${2:-false}"

    if [ "$use_sudo" == "true" ] && ([ "$PGHOST" == "localhost" ] || [ "$PGHOST" == "127.0.0.1" ]); then
        result=$(sudo -u postgres psql -tAc "SELECT 1 FROM pg_database WHERE datname='$db'" 2>/dev/null)
    else
        result=$(PGPASSWORD="$PGPASSWORD" psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" -tAc "SELECT 1 FROM pg_database WHERE datname='$db'" 2>/dev/null)
    fi

    [ "$result" == "1" ]
}

# Function to check if user exists (only used during initial setup with sudo)
user_exists() {
    local user=$1
    local result

    # This is only called during setup with sudo
    if [ "$PGHOST" == "localhost" ] || [ "$PGHOST" == "127.0.0.1" ]; then
        result=$(sudo -u postgres psql -tAc "SELECT 1 FROM pg_user WHERE usename='$user'" 2>/dev/null)
    else
        # For remote hosts, we try with default postgres superuser
        result=$(PGPASSWORD="$PGPASSWORD" psql -h "$PGHOST" -p "$PGPORT" -U postgres -tAc "SELECT 1 FROM pg_user WHERE usename='$user'" 2>/dev/null)
    fi

    [ "$result" == "1" ]
}

# Function to check if schema exists
schema_exists() {
    local schema=$1
    local result

    # Use superuser account to check
    result=$(PGPASSWORD="$PGPASSWORD" psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" -d "$PGDATABASE" -tAc "SELECT 1 FROM pg_namespace WHERE nspname='$schema'" 2>/dev/null)

    [ "$result" == "1" ]
}

# Function to run sqitch command
run_sqitch() {
    local sqitch_dir="$1"
    local sqitch_command="$2"
    shift 2
    local sqitch_args="$@"
    
    # Check if sqitch is available
    if ! command -v sqitch &> /dev/null; then
        echo -e "${RED}sqitch command not found${NC}"
        echo "Please install sqitch: https://sqitch.org"
        exit 1
    fi
    
    # Check if sqitch directory exists
    if [ ! -d "$sqitch_dir" ]; then
        echo -e "${RED}Sqitch directory not found: $sqitch_dir${NC}"
        exit 1
    fi
    
    # Build database URI
    local db_uri="db:pg://${PGUSER}:${PGPASSWORD}@${PGHOST}:${PGPORT}/${PGDATABASE}"

    echo -e "${BLUE}Running sqitch $sqitch_command $sqitch_args in $sqitch_dir${NC}"

    if [[ "$DRY_RUN" == "true" ]]; then
        echo -e "${YELLOW}[DRY RUN] Would execute:${NC}"
        echo "cd $sqitch_dir && sqitch $sqitch_command --target $db_uri $sqitch_args"
        return
    fi

    # Run sqitch command
    cd "$sqitch_dir"
    sqitch $sqitch_command --target "$db_uri" $sqitch_args
}

# Schema commands
sqitch_deploy() {
    local target="${2:-HEAD}"
    echo -e "\n${YELLOW}Deploying database schema changes${NC}"
    run_sqitch "$SQITCH_CORE_DIR" deploy $target
    echo -e "${GREEN}Schema deployment completed successfully${NC}"
}

sqitch_revert() {
    local target="$2"
    if [ -z "$target" ]; then
        echo -e "${RED}Error: revert command requires a target${NC}"
        echo "Usage: $0 revert <target>"
        echo "Example: $0 revert @HEAD^ (revert last change)"
        exit 1
    fi
    
    echo -e "\n${YELLOW}Reverting database schema changes to $target${NC}"
    run_sqitch "$SQITCH_CORE_DIR" revert $target
    echo -e "${GREEN}Schema revert completed successfully${NC}"
}

# Demo commands
#
# TODO (root_code): the schema project parameterizes the root org unit
# (root_code / root_label / root_uuid), and nothing here passes those
# values -- so every deploy from this script produces the demo root. That
# is right for a public checkout and wrong for an installation, which
# today has to run sqitch by hand or set the variables in sqitch.conf
# under [core "variables"] (which `sqitch verify` also reads, unlike -s on
# the command line). Decide whether this script should read them from the
# environment, from a file, or not at all.
sqitch_deploy_demo() {
    local target="${2:-HEAD}"
    echo -e "\n${YELLOW}Deploying the demo org tree${NC}"
    run_sqitch "$SQITCH_DEMO_DIR" deploy $target
    echo -e "${GREEN}Demo deployment completed successfully${NC}"
}

sqitch_revert_demo() {
    echo -e "\n${YELLOW}Reverting the demo org tree${NC}"
    run_sqitch "$SQITCH_DEMO_DIR" revert -y
    echo -e "${GREEN}Demo revert completed successfully${NC}"
}

sqitch_status_demo() {
    echo -e "\n${YELLOW}Checking demo project deployment status${NC}"
    run_sqitch "$SQITCH_DEMO_DIR" status
}

# Revert the demo project if it has anything deployed. The demo change
# declares a dependency on odo:002_odo_seed, so sqitch refuses to revert
# the schema project out from under it -- correctly, but it means any
# whole-database revert has to come here first.
revert_demo_if_deployed() {
    [ -d "$SQITCH_DEMO_DIR" ] || return 0

    # Ask the registry rather than sqitch: this runs before the schema
    # revert, when the registry still exists, and it stays quiet when the
    # demo project was never deployed.
    local deployed
    deployed=$(PGPASSWORD="$PGPASSWORD" psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" \
        -d "$PGDATABASE" -tAc \
        "SELECT 1 FROM sqitch.changes c
           JOIN sqitch.projects p ON p.project = c.project
          WHERE p.project = 'odo-demo' LIMIT 1" 2>/dev/null)

    if [ "$deployed" == "1" ]; then
        echo -e "${BLUE}Reverting the demo project first: the schema project cannot revert while it depends on it${NC}"
        run_sqitch "$SQITCH_DEMO_DIR" revert -y
    fi
}

sqitch_revert_all() {
    echo -e "\n${YELLOW}Reverting all database schema changes${NC}"
    echo -e "${RED}WARNING: This will remove all deployed schema changes!${NC}"

    revert_demo_if_deployed
    run_sqitch "$SQITCH_CORE_DIR" revert
    echo -e "${GREEN}All schema changes reverted successfully${NC}"
}

sqitch_status() {
    echo -e "\n${YELLOW}Checking schema deployment status${NC}"
    run_sqitch "$SQITCH_CORE_DIR" status
}

sqitch_verify() {
    echo -e "\n${YELLOW}Verifying deployed schema changes${NC}"
    run_sqitch "$SQITCH_CORE_DIR" verify
    echo -e "${GREEN}Schema verification completed${NC}"
}

sqitch_log() {
    echo -e "\n${YELLOW}Schema deployment history${NC}"
    run_sqitch "$SQITCH_CORE_DIR" log
}

# Test data: plain idempotent SQL files applied in filename order. Not a
# sqitch project -- there is no revert path, and reloading pairs with a
# full database rebuild rather than with a revert.
deploy_test_data() {
    echo -e "\n${YELLOW}Deploying test data${NC}"

    if [ ! -d "$TEST_DATA_DIR" ]; then
        echo -e "${RED}Test data directory not found: $TEST_DATA_DIR${NC}"
        return 1
    fi

    echo -e "${BLUE}Applying test data to $PGUSER@$PGHOST:$PGPORT/$PGDATABASE${NC}"
    local f
    for f in "$TEST_DATA_DIR"/[0-9]*.sql; do
        [ -e "$f" ] || { echo -e "${YELLOW}  no test data files${NC}"; return 0; }
        echo -e "${BLUE}  applying $(basename "$f")${NC}"
        if [[ "$DRY_RUN" == "true" ]]; then
            continue
        fi
        PGPASSWORD="$PGPASSWORD" psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" \
            -d "$PGDATABASE" -q -v ON_ERROR_STOP=1 -f "$f" || return 1
    done

    echo -e "${GREEN}Test data deployment completed successfully${NC}"
}

# Demo/platform reset: DESTRUCTIVE. The platform seed content is the sqitch
# change 002_odo_seed, so a dev reset is: revert all schema changes,
# redeploy (baseline + seed), then reload the idempotent test data
# (tests/fixtures/).
reset_demo_data() {
    echo -e "\n${RED}WARNING: this reverts and redeploys ALL schema changes in database '${PGDATABASE}', wiping all data.${NC}"
    read -r -p "Type 'reset' to confirm: " confirmation
    if [[ "$confirmation" != "reset" ]]; then
        echo "Aborted."
        exit 1
    fi
    revert_demo_if_deployed
    run_sqitch "$SQITCH_CORE_DIR" revert -y
    run_sqitch "$SQITCH_CORE_DIR" deploy
    run_sqitch "$SQITCH_DEMO_DIR" deploy
    deploy_test_data
    echo -e "${GREEN}Demo reset completed${NC}"
}

purge_all_schemas() {
    echo -e "\n${RED}WARNING: This will drop all application schemas in database '${PGDATABASE}'.${NC}"
    echo -e "${RED}Everything in them is lost, including both sqitch registries.${NC}"

    # Schema-level drops rather than DROP DATABASE: the pooled service
    # credentials cannot create a database, and pgbouncer does not pool
    # the maintenance database at all, so there is no connection from
    # here that could recreate one.
    #
    # Dropping still needs ownership. The schemas are owned by the
    # database owner, not by the service role the secret carries, so
    # this generally wants PGUSER/PGPASSWORD overridden with an
    # administrative account:
    #
    #   PGUSER=postgres PGPASSWORD=... PGHOST=<server> PGPORT=5432 \
    #       ./scripts/manage-database.sh purge-all
    #
    # Note the direct port: pgbouncer fronts the pooled databases, and
    # an admin role is usually not among its configured users.
    local schemas=(notification asset auth audit authz org sqitch)

    echo -e "${RED}Schemas that will be dropped: ${schemas[*]}${NC}"
    read -r -p "Type 'purge' to confirm: " confirmation

    if [[ "$confirmation" != "purge" ]]; then
        echo -e "${YELLOW}Purge cancelled.${NC}"
        return
    fi

    # Check ownership before dropping anything: a partial purge leaves a
    # database that neither deploys nor reverts, and the bare
    # "must be owner of schema" error does not say what to do about it.
    local not_owned
    not_owned=$(PGPASSWORD="$PGPASSWORD" psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" \
        -d "$PGDATABASE" -tAc \
        "SELECT string_agg(nspname, ', ' ORDER BY nspname)
           FROM pg_namespace
          WHERE nspname = ANY(ARRAY['notification','asset','auth','audit','authz','org','sqitch'])
            AND pg_get_userbyid(nspowner) <> current_user
            AND NOT pg_has_role(current_user, nspowner, 'MEMBER')" 2>/dev/null)

    if [ -n "$not_owned" ]; then
        echo -e "${RED}Cannot purge: '$PGUSER' does not own: $not_owned${NC}"
        echo -e "${YELLOW}Re-run with an administrative account, e.g.${NC}"
        echo "  PGUSER=postgres PGPASSWORD=... PGPORT=5432 $0 purge-all"
        return 1
    fi

    echo -e "${YELLOW}Dropping application schemas...${NC}"
    for schema in "${schemas[@]}"; do
        echo -e "${BLUE}Dropping schema '$schema'${NC}"
        execute_psql "DROP SCHEMA IF EXISTS \"$schema\" CASCADE;" "$PGDATABASE"
    done

    echo -e "${GREEN}All application schemas dropped. Run '$0 deploy' to rebuild.${NC}"
}

# Function to setup database
setup_database() {
    echo -e "\n${YELLOW}Setting up PostgreSQL database for Odo${NC}"

    # First, test if we can connect with the superuser account
    if test_maintenance_connection; then
        echo -e "${GREEN}Successfully connected with existing superuser account${NC}"

        # User already exists and we can connect - no sudo needed
        # Just ensure password and superuser status are current
        echo -e "${GREEN}Updating user password and ensuring SUPERUSER status${NC}"
        execute_psql "ALTER USER $PGUSER WITH SUPERUSER PASSWORD '$PGPASSWORD';"

    else
        echo -e "${YELLOW}Cannot connect with superuser account, checking if initial setup is needed...${NC}"

        # The role can only be created from here over a local peer
        # connection. PostgreSQL runs outside the cluster and DATABASE_URL
        # names it by a routable address, so that path exists only when
        # this script runs on the database host itself (PGHOST overridden
        # to localhost). Otherwise the role has to be created there.
        if [ "$PGHOST" != "localhost" ] && [ "$PGHOST" != "127.0.0.1" ]; then
            echo -e "${RED}Cannot log in to ${PGHOST}:${PGPORT} as '${PGUSER}'.${NC}"
            echo
            echo "That role must exist on the database server and be a superuser."
            echo "On Ubuntu, ./scripts/setup/install-postgres-server-ubuntu.sh"
            echo "creates it; elsewhere, create it by hand:"
            echo
            echo "  CREATE ROLE \"$PGUSER\" WITH SUPERUSER LOGIN PASSWORD '...';"
            echo
            echo "Then check the password in the postgres-credentials secret"
            echo "matches (./scripts/manage-secrets.sh update-db-url)."
            exit 1
        fi

        # Check if this is initial setup (user doesn't exist)
        check_sudo  # This will exit if not running with sudo

        if user_exists "$PGUSER"; then
            echo -e "${BLUE}User '$PGUSER' exists but cannot connect${NC}"
            echo -e "${GREEN}Updating user password and ensuring SUPERUSER status${NC}"
            execute_psql "ALTER USER $PGUSER WITH SUPERUSER PASSWORD '$PGPASSWORD';" "postgres" "true"
        else
            echo -e "${GREEN}Creating user '$PGUSER' as SUPERUSER${NC}"
            execute_psql "CREATE USER $PGUSER WITH SUPERUSER PASSWORD '$PGPASSWORD';" "postgres" "true"
        fi

        # Test connection again
        if ! test_maintenance_connection; then
            echo -e "${RED}Failed to establish connection after user setup${NC}"
            exit 1
        fi
    fi

    # From here on, we use the superuser account (no sudo needed)

    # Create database if doesn't exist
    if database_exists "$PGDATABASE"; then
        echo -e "${BLUE}Database '$PGDATABASE' already exists${NC}"
    else
        echo -e "${GREEN}Creating database '$PGDATABASE'${NC}"
        execute_psql "CREATE DATABASE $PGDATABASE OWNER $PGUSER;"
    fi

    # Create sqitch schema
    echo -e "${GREEN}Setting up sqitch schema${NC}"

    # Create sqitch schema if doesn't exist
    if schema_exists "sqitch"; then
        echo -e "${BLUE}Schema 'sqitch' already exists${NC}"
        # Make sure user owns the schema
        echo -e "${GREEN}Ensuring user owns sqitch schema${NC}"
        execute_psql "ALTER SCHEMA sqitch OWNER TO $PGUSER;" "$PGDATABASE"
    else
        echo -e "${GREEN}Creating schema 'sqitch'${NC}"
        execute_psql "CREATE SCHEMA sqitch AUTHORIZATION $PGUSER;" "$PGDATABASE"
    fi

    # Note: No GRANT statements needed since user is SUPERUSER

    echo -e "\n${GREEN}Database setup completed successfully!${NC}"
    echo -e "${YELLOW}Summary:${NC}"
    echo "  Database: $PGDATABASE"
    echo "  User: $PGUSER (SUPERUSER)"
    echo "  Sqitch schema: created and configured"
    echo
    echo -e "${BLUE}Next steps:${NC}"
    echo "1. Run '$0 deploy' to apply database schema migrations"
}

# Function to update password only
update_password() {
    echo -e "\n${YELLOW}Updating password for user '$PGUSER'${NC}"

    # First try to connect with existing credentials
    if test_connection; then
        echo -e "${GREEN}Successfully connected with existing credentials${NC}"
        echo -e "${GREEN}Updating user password and ensuring SUPERUSER status${NC}"
        execute_psql "ALTER USER $PGUSER WITH SUPERUSER PASSWORD '$PGPASSWORD';"
    else
        echo -e "${YELLOW}Cannot connect with current credentials${NC}"
        echo -e "${RED}If the user doesn't exist, run '$0 setup' first${NC}"
        echo -e "${RED}If the password needs to be reset, run this command with sudo${NC}"

        # Check if running with sudo for password reset
        check_sudo

        if ! user_exists "$PGUSER"; then
            echo -e "${RED}User '$PGUSER' does not exist${NC}"
            echo "Run '$0 setup' first to create the user"
            exit 1
        fi

        echo -e "${GREEN}Resetting user password and ensuring SUPERUSER status${NC}"
        execute_psql "ALTER USER $PGUSER WITH SUPERUSER PASSWORD '$PGPASSWORD';" "postgres" "true"
    fi

    echo -e "\n${GREEN}Password updated and SUPERUSER status ensured!${NC}"
    echo -e "${YELLOW}Note: The password was retrieved from the Kubernetes secret${NC}"
}

# Main script logic
case "$COMMAND" in
    connect)
        connect
        ;;
    setup)
        setup_database
        ;;
    update-password)
        update_password
        ;;
    deploy)
        sqitch_deploy "$@"
        ;;
    revert-to)
        sqitch_revert "$@"
        ;;
    revert-last)
        sqitch_revert "" "@HEAD^"
        ;;
    revert-all)
        sqitch_revert_all
        ;;
    status)
        sqitch_status
        ;;
    verify)
        sqitch_verify
        ;;
    log)
        sqitch_log
        ;;
    deploy-demo)
        sqitch_deploy_demo "$@"
        ;;
    revert-demo)
        sqitch_revert_demo
        ;;
    status-demo)
        sqitch_status_demo
        ;;
    deploy-test)
        deploy_test_data
        ;;
    reset-demo)
        reset_demo_data
        ;;
    purge-all)
        purge_all_schemas
        ;;
    -h|--help|help)
        usage
        ;;
    *)
        echo -e "${RED}Error: Unknown command '$COMMAND'${NC}"
        usage 1
        ;;
esac
