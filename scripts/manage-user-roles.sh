#!/bin/bash
# Script to manage user role-org mappings in the Odo platform database
# Lists, adds, and deletes authz.usr_role_org_map entries
#
# Usage: ./manage-users.sh <command> <username> [options]

set -e

# Get script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Source common database functions
source "$SCRIPT_DIR/common.sh"

# Function to show usage
usage() {
    echo -e "${BLUE}Usage: $0 <command> <username> [options]${NC}"
    echo
    echo "Commands:"
    echo "  list <username>                          List all role-org mappings for a user"
    echo "  add <username> <role> <org_unit_code>    Add a new role-org mapping"
    echo "  delete <mapping_id>                      Delete a mapping by ID"
    echo
    echo "Arguments:"
    echo "  username        Login username (from auth.usr.username)"
    echo "  role            Role code (e.g., 'odo-admin')"
    echo "  org_unit_code   Organization unit code (from org.unit.code)"
    echo "  mapping_id      ID of the usr_role_org_map entry"
    echo
    echo "Environment variables:"
    echo "  PGHOST=hostname           Override database host (default: localhost)"
    echo "  PGPORT=port               Override database port (default: 32345)"
    echo "  PGDATABASE=dbname         Override database name (default: from secret)"
    echo "  PGUSER=username           Override database user (default: from secret)"
    echo "  PGPASSWORD=password       Override database password (default: from secret)"
    echo "  NAMESPACE=name            Kubernetes namespace for secrets (default: odo-core)"
    echo
    echo "Examples:"
    echo "  $0 list john.doe"
    echo "  $0 add john.doe odo-admin MAIN"
    echo "  $0 delete 42"
    exit 1
}

# Function to list available roles
list_roles() {
    echo -e "${BLUE}Available roles:${NC}"
    query_psql "SELECT code || ' - ' || label FROM authz.role ORDER BY code"
}

# Function to list available org units
list_org_units() {
    echo -e "${BLUE}Available org units:${NC}"
    query_psql "SELECT code || ' - ' || label FROM org.unit WHERE deleted_at IS NULL ORDER BY code"
}

# Function to look up user ID by username
lookup_user() {
    local username="$1"
    local usr_id=$(query_psql "SELECT id FROM auth.usr WHERE username = '$username'")

    if [ -z "$usr_id" ]; then
        echo -e "${RED}Error: User not found with username: $username${NC}" >&2
        return 1
    fi

    echo "$usr_id"
}

# Function to look up org unit ID by code
lookup_org_unit() {
    local code="$1"
    local org_id=$(query_psql "SELECT id FROM org.unit WHERE code = '$code' AND deleted_at IS NULL")

    if [ -z "$org_id" ]; then
        echo -e "${RED}Error: Org unit not found with code: $code${NC}" >&2
        list_org_units
        return 1
    fi

    echo "$org_id"
}

# Function to verify role exists
verify_role() {
    local role="$1"
    local exists=$(query_psql "SELECT 1 FROM authz.role WHERE code = '$role'")

    if [ -z "$exists" ]; then
        echo -e "${RED}Error: Role not found: $role${NC}" >&2
        list_roles
        return 1
    fi

    return 0
}

# Function to verify mapping exists and return its details
verify_mapping() {
    local mapping_id="$1"
    local exists=$(query_psql "SELECT 1 FROM authz.usr_role_org_map WHERE id = $mapping_id")

    if [ -z "$exists" ]; then
        echo -e "${RED}Error: Mapping not found with ID: $mapping_id${NC}" >&2
        return 1
    fi

    return 0
}

# Command: list
cmd_list() {
    local username="$1"

    if [ -z "$username" ]; then
        echo -e "${RED}Error: Username is required${NC}"
        usage
    fi

    local usr_id=$(lookup_user "$username") || exit 1

    local results=$(query_psql "
        SELECT
            m.id,
            m.role,
            o.code || '/' || o.label as org
        FROM authz.usr_role_org_map m
        JOIN org.unit o ON o.id = m.org_unit
        WHERE m.usr = $usr_id
        ORDER BY m.role, o.code
    ")

    if [ -z "$results" ]; then
        echo -e "${YELLOW}No role-org mappings found for this user${NC}"
        return 0
    fi

    # Print header
    printf "%-12s %-25s %s\n" "Mapping ID" "Role" "Org"
    printf "%-12s %-25s %s\n" "------------" "-------------------------" "------------------------------------"

    # Print each row
    echo "$results" | while IFS='|' read -r id role org; do
        printf "%-12s %-25s %s\n" "$id" "$role" "$org"
    done
}

# Command: add
cmd_add() {
    local username="$1"
    local role="$2"
    local org_unit_code="$3"

    if [ -z "$username" ] || [ -z "$role" ] || [ -z "$org_unit_code" ]; then
        echo -e "${RED}Error: Username, role, and org_unit_code are required${NC}"
        usage
    fi

    # Look up user
    local usr_id=$(lookup_user "$username") || exit 1

    # Verify role
    verify_role "$role" || exit 1

    # Look up org unit
    local org_unit_id=$(lookup_org_unit "$org_unit_code") || exit 1

    # Check if mapping already exists
    local existing=$(query_psql "
        SELECT id FROM authz.usr_role_org_map
        WHERE usr = $usr_id AND role = '$role' AND org_unit = $org_unit_id
    ")

    if [ -n "$existing" ]; then
        echo -e "${RED}Error: Mapping already exists (ID: $existing)${NC}"
        exit 1
    fi

    local map_id=$(query_psql "
        INSERT INTO authz.usr_role_org_map (usr, role, org_unit)
        VALUES ($usr_id, '$role', $org_unit_id)
        RETURNING id
    ")

    if [ -z "$map_id" ]; then
        echo -e "${RED}Error: Failed to create role mapping${NC}"
        exit 1
    fi

    echo
    echo -e "${GREEN}Updated list: ${NC}"
    echo 

    $0 list "$username"
}

# Command: delete
cmd_delete() {
    local mapping_id="$1"

    if [ -z "$mapping_id" ]; then
        echo -e "${RED}Error: Mapping ID is required${NC}"
        usage
    fi

    # Validate mapping_id is a number
    if ! [[ "$mapping_id" =~ ^[0-9]+$ ]]; then
        echo -e "${RED}Error: Mapping ID must be a number${NC}"
        exit 1
    fi

    # Verify mapping exists and get details
    verify_mapping "$mapping_id" || exit 1

    # Get mapping details for confirmation
    local details=$(query_psql "
        SELECT
            u.username,
            m.role,
            r.label as role_label,
            o.code as org_code,
            o.label as org_label
        FROM authz.usr_role_org_map m
        JOIN auth.usr u ON u.id = m.usr
        JOIN authz.role r ON r.code = m.role
        JOIN org.unit o ON o.id = m.org_unit
        WHERE m.id = $mapping_id
    ")

    IFS='|' read -r username role role_label org_code org_label <<< "$details"
    echo -e "${YELLOW}Delete mapping: $username => $role => $org_code ${NC}"
    echo

    # Confirm deletion
    read -p "Are you sure you want to delete this mapping? (y/N): " confirm
    if [[ ! "$confirm" =~ ^[Yy]$ ]]; then
        echo -e "${YELLOW}Deletion cancelled${NC}"
        exit 0
    fi

    local _=$(execute_psql "DELETE FROM authz.usr_role_org_map WHERE id = $mapping_id")

    echo
    echo -e "${GREEN}Updated list: ${NC}"
    echo

    $0 list "$username"
}

# Main script logic
if [ $# -lt 1 ]; then
    echo -e "${RED}Error: No command specified${NC}"
    usage
fi

COMMAND="$1"
shift

# Initialize database connection
if ! init_pg_connection; then
    echo -e "${RED}Failed to initialize database connection${NC}"
    exit 1
fi

# Test connection
if ! test_connection; then
    echo -e "${RED}Failed to connect to database${NC}"
    echo "Connection details:"
    echo "  Host: $PGHOST"
    echo "  Port: $PGPORT"
    echo "  Database: $PGDATABASE"
    echo "  User: $PGUSER"
    exit 1
fi

# Execute command
case "$COMMAND" in
    list)
        cmd_list "$@"
        ;;
    add)
        cmd_add "$@"
        ;;
    delete)
        cmd_delete "$@"
        ;;
    *)
        echo -e "${RED}Error: Unknown command: $COMMAND${NC}"
        usage
        ;;
esac
