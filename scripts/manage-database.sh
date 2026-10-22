#!/bin/bash
# Manage a project's PostgreSQL database and Sqitch migrations: odo's own,
# or an app's when run from (or pointed at) the app's checkout (see
# lib/project.sh).
#
# Connection settings (in order of precedence):
#   1. Environment variables: PGHOST, PGPORT, PGDATABASE, PGUSER, PGPASSWORD
#   2. The Kubernetes secret named by the `database:` block of the
#      project's service-map.yaml (DATABASE_URL)
#   3. Defaults: localhost:5432 (for host/port only)
#
# The same `database:` block sets what differs between projects:
#   schemas      the application schemas purge-all drops
#   owns_server  true when `setup` may create the project's role and the
#                database it owns, and `update-password` may reset the
#                role over the local peer connection (odo). The role is
#                never a superuser; owning the database is enough.
#                Otherwise the role and an empty database it owns are
#                created on the server by hand, and the script only ever
#                acts as that role.
#
# Layout, under the project root:
#   src/sqitch/core/       the schema and seed. Every install needs it.
#   src/sqitch/demo-data/  optional sample data, its own sqitch project
#                          (named by %project in its sqitch.plan). It
#                          depends on core, so it deploys after core and
#                          reverts before it. An installation with its own
#                          data skips it.
#   tests/fixtures/        dev/CI fixtures: not a sqitch project, plain
#                          idempotent SQL applied in filename order, with
#                          no revert path.

set -e

# Get script directory and project root
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib/project.sh"
PROJECT_ROOT="$(resolve_project_root "$(cd "$SCRIPT_DIR/.." && pwd)")"
echo "Project: $PROJECT_ROOT"

# Source common database functions
source "$SCRIPT_DIR/common.sh"
load_db_config "$PROJECT_ROOT" || exit 1

SERVICE_MAP="$PROJECT_ROOT/service-map.yaml"
OWNS_SERVER="$(yq eval '.database.owns_server // false' "$SERVICE_MAP")"
mapfile -t SCHEMAS < <(yq eval '.database.schemas // [] | .[]' "$SERVICE_MAP")

# How to refer to this script in messages: a wrapper in an app checkout
# that hands off here sets ODO_INVOKED_AS to its own name.
ME="${ODO_INVOKED_AS:-$0}"

# Default values
COMMAND=${1:-help}
DRY_RUN=${DRY_RUN:-false}
NAMESPACE=$DB_SECRET_NAMESPACE

SQITCH_DIR="${SQITCH_DIR:-$PROJECT_ROOT/src/sqitch}"
SQITCH_CORE_DIR="${SQITCH_CORE_DIR:-$SQITCH_DIR/core}"
SQITCH_DEMO_DIR="${SQITCH_DEMO_DIR:-$SQITCH_DIR/demo-data}"
TEST_DATA_DIR="${TEST_DATA_DIR:-$PROJECT_ROOT/tests/fixtures}"

# Function to show usage
usage() {
    echo -e "${BLUE}Usage: $ME [command] [options]${NC}"
    echo
    echo "Database Commands:"
    if [[ "$OWNS_SERVER" == "true" ]]; then
        echo "  setup            Create database, user, and sqitch schema (requires sudo)"
    fi
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
    echo "Demo Data Commands (sample data; skip on a real installation):"
    echo "  deploy-demo      Deploy the demo data (requires the schema project)"
    echo "  revert-demo      Revert the demo data, leaving the schema in place"
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
    echo "  NAMESPACE=name            Kubernetes namespace for secrets (default: $DB_SECRET_NAMESPACE)"
    echo "  ODO_PROJECT_ROOT=/path    Project to act on (default: see lib/project.sh)"
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
        # Use the project's role
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
        echo "Usage: $ME revert <target>"
        echo "Example: $ME revert @HEAD^ (revert last change)"
        exit 1
    fi

    echo -e "\n${YELLOW}Reverting database schema changes to $target${NC}"
    run_sqitch "$SQITCH_CORE_DIR" revert $target
    echo -e "${GREEN}Schema revert completed successfully${NC}"
}

# Demo commands
#
# TODO (root_code): odo's schema project parameterizes the root org unit
# (root_code / root_label / root_uuid), and nothing here passes those
# values -- so every deploy from this script produces the demo root. That
# is right for a public checkout and wrong for an installation, which
# today has to run sqitch by hand or set the variables in sqitch.conf
# under [core "variables"] (which `sqitch verify` also reads, unlike -s on
# the command line). Decide whether this script should read them from the
# environment, from a file, or not at all.
sqitch_deploy_demo() {
    local target="${2:-HEAD}"
    echo -e "\n${YELLOW}Deploying the demo data${NC}"
    run_sqitch "$SQITCH_DEMO_DIR" deploy $target
    echo -e "${GREEN}Demo deployment completed successfully${NC}"
}

sqitch_revert_demo() {
    echo -e "\n${YELLOW}Reverting the demo data${NC}"
    run_sqitch "$SQITCH_DEMO_DIR" revert -y
    echo -e "${GREEN}Demo revert completed successfully${NC}"
}

sqitch_status_demo() {
    echo -e "\n${YELLOW}Checking demo project deployment status${NC}"
    run_sqitch "$SQITCH_DEMO_DIR" status
}

# The demo project's sqitch name, from the %project pragma of its plan.
demo_project_name() {
    sed -n 's/^%project=\([^[:space:]]*\).*/\1/p' "$SQITCH_DEMO_DIR/sqitch.plan" 2>/dev/null | head -1
}

# Revert the demo project if it has anything deployed. The demo project
# depends on the core project, so sqitch refuses to revert core out from
# under it -- correctly, but it means any whole-database revert has to
# come here first.
revert_demo_if_deployed() {
    [ -d "$SQITCH_DEMO_DIR" ] || return 0

    local project
    project="$(demo_project_name)"
    if [ -z "$project" ]; then
        echo -e "${RED}No %project in $SQITCH_DEMO_DIR/sqitch.plan${NC}" >&2
        return 1
    fi

    # Ask the registry rather than sqitch: this runs before the schema
    # revert, when the registry still exists, and it stays quiet when the
    # demo project was never deployed.
    local deployed
    deployed=$(PGPASSWORD="$PGPASSWORD" psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" \
        -d "$PGDATABASE" -tAc \
        "SELECT 1 FROM sqitch.changes c
           JOIN sqitch.projects p ON p.project = c.project
          WHERE p.project = '$project' LIMIT 1" 2>/dev/null)

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

# Demo reset: DESTRUCTIVE. The seed content is part of the core sqitch
# project, so a dev reset is: revert all schema changes, redeploy core
# (and the demo project, when there is one), then reload the idempotent
# test data (tests/fixtures/).
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
    if [ -d "$SQITCH_DEMO_DIR" ]; then
        run_sqitch "$SQITCH_DEMO_DIR" deploy
    fi
    deploy_test_data
    echo -e "${GREEN}Demo reset completed${NC}"
}

purge_all_schemas() {
    local schemas=("${SCHEMAS[@]}")
    if [ ${#schemas[@]} -eq 0 ]; then
        echo -e "${RED}No database.schemas in $SERVICE_MAP; nothing to purge.${NC}" >&2
        return 1
    fi
    # The names land in SQL below; accept plain identifiers only.
    local schema
    for schema in "${schemas[@]}"; do
        if [[ ! "$schema" =~ ^[a-z_][a-z0-9_]*$ ]]; then
            echo -e "${RED}Unsupported schema name in database.schemas: '$schema'${NC}" >&2
            return 1
        fi
    done

    echo -e "\n${RED}WARNING: This will drop all application schemas in database '${PGDATABASE}'.${NC}"
    echo -e "${RED}Everything in them is lost, including the sqitch registry.${NC}"
    echo -e "${RED}Connected services will be disconnected and will need restarting.${NC}"

    # Schema-level drops rather than DROP DATABASE: the pooled service
    # credentials cannot create a database, and pgbouncer does not pool
    # the maintenance database at all, so there is no connection from
    # here that could recreate one.
    #
    # Dropping still needs ownership. The schemas are owned by the
    # database owner. Where the secret carries that owner (as odo's
    # installers set it up), the secret's credentials are enough. Where
    # it carries a separate pooled service role, override PGUSER/
    # PGPASSWORD with an administrative account:
    #
    #   PGUSER=postgres PGPASSWORD=... PGHOST=<server> PGPORT=5432 \
    #       ./scripts/manage-database.sh purge-all
    #
    # Note the direct port: pgbouncer fronts the pooled databases, and
    # an admin role is usually not among its configured users.

    # odo-deploy's reset-site-data.sh answers this prompt and checks for
    # "cancelled" in the output: keep both wordings in step with it.
    echo -e "${RED}Schemas that will be dropped: ${schemas[*]}${NC}"
    read -r -p "Type 'purge' to confirm: " confirmation

    if [[ "$confirmation" != "purge" ]]; then
        echo -e "${YELLOW}Purge cancelled.${NC}"
        return
    fi

    # Check ownership before dropping anything: a partial purge leaves a
    # database that neither deploys nor reverts, and the bare
    # "must be owner of schema" error does not say what to do about it.
    local array_sql
    array_sql="ARRAY['$(IFS=,; echo "${schemas[*]}" | sed "s/,/','/g")']"
    local not_owned
    not_owned=$(PGPASSWORD="$PGPASSWORD" psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" \
        -d "$PGDATABASE" -tAc \
        "SELECT string_agg(nspname, ', ' ORDER BY nspname)
           FROM pg_namespace
          WHERE nspname = ANY($array_sql)
            AND pg_get_userbyid(nspowner) <> current_user
            AND NOT pg_has_role(current_user, nspowner, 'MEMBER')" 2>/dev/null)

    if [ -n "$not_owned" ]; then
        echo -e "${RED}Cannot purge: '$PGUSER' does not own: $not_owned${NC}"
        echo -e "${YELLOW}Re-run with an administrative account, e.g.${NC}"
        echo "  PGUSER=postgres PGPASSWORD=... PGPORT=5432 $ME purge-all"
        return 1
    fi

    echo -e "${YELLOW}Dropping application schemas...${NC}"
    for schema in "${schemas[@]}"; do
        echo -e "${BLUE}Dropping schema '$schema'${NC}"
        execute_psql "DROP SCHEMA IF EXISTS \"$schema\" CASCADE;" "$PGDATABASE"
    done

    echo -e "${GREEN}All application schemas dropped. Run '$ME deploy' to rebuild.${NC}"
}

# Function to setup database
setup_database() {
    if [[ "$OWNS_SERVER" != "true" ]]; then
        echo -e "${RED}setup does not apply to this project.${NC}"
        echo
        echo "Its role does not own the PostgreSQL server, so no script here"
        echo "creates it. Create the role and an empty database it owns on the"
        echo "server by hand (see the project's README), then run '$ME deploy'."
        exit 1
    fi

    echo -e "\n${YELLOW}Setting up PostgreSQL database${NC}"

    # First, test if the role already exists and can log in
    if test_maintenance_connection; then
        echo -e "${GREEN}Successfully connected as '$PGUSER'${NC}"
    else
        echo -e "${YELLOW}Cannot connect as '$PGUSER', checking if initial setup is needed...${NC}"

        # The role can only be created from here over a local peer
        # connection. PostgreSQL runs outside the cluster and DATABASE_URL
        # names it by a routable address, so that path exists only when
        # this script runs on the database host itself (PGHOST overridden
        # to localhost). Otherwise the role has to be created there.
        if [ "$PGHOST" != "localhost" ] && [ "$PGHOST" != "127.0.0.1" ]; then
            echo -e "${RED}Cannot log in to ${PGHOST}:${PGPORT} as '${PGUSER}'.${NC}"
            echo
            echo "That role, and a database it owns, must exist on the database"
            echo "server. On Ubuntu, ./scripts/setup/install-postgres-server-ubuntu.sh"
            echo "creates both; elsewhere, create them by hand:"
            echo
            echo "  CREATE ROLE \"$PGUSER\" WITH LOGIN PASSWORD '...';"
            echo "  CREATE DATABASE \"$PGDATABASE\" OWNER \"$PGUSER\";"
            echo
            echo "Then check the password in the $DB_SECRET_NAME secret"
            echo "matches (./scripts/manage-secrets.sh update-db-url)."
            exit 1
        fi

        # Check if this is initial setup (user doesn't exist)
        check_sudo  # This will exit if not running with sudo

        if user_exists "$PGUSER"; then
            echo -e "${BLUE}User '$PGUSER' exists but cannot connect${NC}"
            echo -e "${GREEN}Updating user password${NC}"
            execute_psql "ALTER USER $PGUSER WITH PASSWORD '$PGPASSWORD';" "postgres" "true"
        else
            echo -e "${GREEN}Creating user '$PGUSER'${NC}"
            execute_psql "CREATE USER $PGUSER WITH PASSWORD '$PGPASSWORD';" "postgres" "true"
        fi

        # Test connection again
        if ! test_maintenance_connection; then
            echo -e "${RED}Failed to establish connection after user setup${NC}"
            exit 1
        fi
    fi

    # Create database if doesn't exist
    if database_exists "$PGDATABASE"; then
        echo -e "${BLUE}Database '$PGDATABASE' already exists${NC}"
    else
        echo -e "${GREEN}Creating database '$PGDATABASE'${NC}"
        # The role usually holds no CREATEDB, so on the database host the
        # postgres account creates it; elsewhere this only succeeds if the
        # role was granted CREATEDB.
        if ! execute_psql "CREATE DATABASE $PGDATABASE OWNER $PGUSER;" "postgres" "true"; then
            echo -e "${RED}Could not create database '$PGDATABASE'.${NC}"
            echo
            echo "Create it on the database server, owned by the role:"
            echo "  CREATE DATABASE \"$PGDATABASE\" OWNER \"$PGUSER\";"
            exit 1
        fi
    fi

    # From here on, everything runs as the role, which owns the database

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

    # No GRANT statements needed: the role owns the database and everything
    # the deploy creates in it.

    echo -e "\n${GREEN}Database setup completed successfully!${NC}"
    echo -e "${YELLOW}Summary:${NC}"
    echo "  Database: $PGDATABASE"
    echo "  User: $PGUSER (database owner)"
    echo "  Sqitch schema: created and configured"
    echo
    echo -e "${BLUE}Next steps:${NC}"
    echo "1. Run '$ME deploy' to apply database schema migrations"
}

# Function to update password only
update_password() {
    echo -e "\n${YELLOW}Updating password for user '$PGUSER'${NC}"

    if [[ "$OWNS_SERVER" != "true" ]]; then
        # A role can change its own password, which is all a project that
        # does not own the server can do: it holds no admin account there.
        if ! test_connection; then
            echo -e "${RED}Cannot connect as '$PGUSER' with the credentials in the secret.${NC}"
            echo
            echo "This project can only change its own password, so the stored"
            echo "one has to work first. Reset it on the database server:"
            echo "  ALTER ROLE $PGUSER WITH PASSWORD '<the password in the secret>';"
            echo "or point the secret at the working password (manage-secrets.sh"
            echo "update-db-url)."
            exit 1
        fi

        echo -e "${GREEN}Successfully connected with existing credentials${NC}"
        execute_psql "ALTER ROLE $PGUSER WITH PASSWORD '$PGPASSWORD';" "$PGDATABASE"

        echo -e "\n${GREEN}Password updated!${NC}"
        echo -e "${YELLOW}Note: The password was retrieved from the Kubernetes secret${NC}"
        return
    fi

    # First try to connect with existing credentials
    if test_connection; then
        echo -e "${GREEN}Successfully connected with existing credentials${NC}"
        echo -e "${GREEN}Updating user password${NC}"
        execute_psql "ALTER USER $PGUSER WITH PASSWORD '$PGPASSWORD';"
    else
        echo -e "${YELLOW}Cannot connect with current credentials${NC}"
        echo -e "${RED}If the user doesn't exist, run '$ME setup' first${NC}"
        echo -e "${RED}If the password needs to be reset, run this command with sudo${NC}"

        # Check if running with sudo for password reset
        check_sudo

        if ! user_exists "$PGUSER"; then
            echo -e "${RED}User '$PGUSER' does not exist${NC}"
            echo "Run '$ME setup' first to create the user"
            exit 1
        fi

        echo -e "${GREEN}Resetting user password${NC}"
        execute_psql "ALTER USER $PGUSER WITH PASSWORD '$PGPASSWORD';" "postgres" "true"
    fi

    echo -e "\n${GREEN}Password updated!${NC}"
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
