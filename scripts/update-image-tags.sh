#!/bin/bash
# Script to update image tags in deployment manifests
# Usage: ./update-image-tags.sh <tag> <service1> [<service2> ...]
# Usage: echo '["service1","service2"]' | ./update-image-tags.sh <tag> --from-json --manifests-dir <path>

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Default values
MANIFESTS_DIR=""
REGISTRY="ghcr.io"
IMAGE_PREFIX="kcls"

# Function to show usage
usage() {
    echo -e "${BLUE}Usage: $0 <tag> <service1> [<service2> ...]${NC}"
    echo -e "${BLUE}   or: echo '[\"service1\",\"service2\"]' | $0 <tag> --from-json [--manifests-dir <path>]${NC}"
    echo
    echo "Updates image tags in deployment.yaml files for specified services."
    echo
    echo "Arguments:"
    echo "  tag              The image tag to use (e.g., git commit short SHA)"
    echo "  service(s)       List of services to update"
    echo
    echo "Options:"
    echo "  --from-json      Read services as JSON array from stdin"
    echo "  --manifests-dir  Path to manifests repository (default: current directory)"
    echo "  --registry       Container registry (default: ghcr.io)"
    echo "  --prefix         Image prefix/owner (default: kcls)"
    echo
    echo "Examples:"
    echo "  $0 abc1234 odo-auth odo-org"
    echo "  echo '[\"odo-auth\",\"odo-org\"]' | $0 abc1234 --from-json --manifests-dir ./manifests"
    echo
    exit 1
}

# Check for required tools
check_requirements() {
    local missing=()

    if ! command -v yq &> /dev/null; then
        missing+=("yq")
    fi

    if ! command -v jq &> /dev/null; then
        missing+=("jq")
    fi

    if [ ${#missing[@]} -gt 0 ]; then
        echo -e "${RED}Error: Missing required tools: ${missing[*]}${NC}"
        echo "Install with:"
        echo "  yq: https://github.com/mikefarah/yq/releases"
        echo "  jq: apt-get install jq or brew install jq"
        exit 1
    fi
}

# Parse arguments
parse_args() {
    if [ $# -lt 1 ]; then
        usage
    fi

    TAG="$1"
    shift

    SERVICES=()
    FROM_JSON=false

    while [ $# -gt 0 ]; do
        case "$1" in
            --from-json)
                FROM_JSON=true
                shift
                ;;
            --manifests-dir)
                MANIFESTS_DIR="$2"
                shift 2
                ;;
            --registry)
                REGISTRY="$2"
                shift 2
                ;;
            --prefix)
                IMAGE_PREFIX="$2"
                shift 2
                ;;
            --help|-h)
                usage
                ;;
            *)
                SERVICES+=("$1")
                shift
                ;;
        esac
    done

    # If --from-json, read services from stdin
    if [ "$FROM_JSON" = true ]; then
        if [ -t 0 ]; then
            echo -e "${RED}Error: No JSON input provided on stdin${NC}"
            exit 1
        fi

        JSON_INPUT=$(cat)
        if ! echo "$JSON_INPUT" | jq -e . >/dev/null 2>&1; then
            echo -e "${RED}Error: Invalid JSON input${NC}"
            exit 1
        fi

        while IFS= read -r service; do
            SERVICES+=("$service")
        done < <(echo "$JSON_INPUT" | jq -r '.[]')
    fi

    # Default manifests dir to current directory
    if [ -z "$MANIFESTS_DIR" ]; then
        MANIFESTS_DIR="."
    fi

    # Validate
    if [ -z "$TAG" ]; then
        echo -e "${RED}Error: Tag is required${NC}"
        usage
    fi

    if [ ${#SERVICES[@]} -eq 0 ]; then
        echo -e "${RED}Error: No services specified${NC}"
        usage
    fi

    if [ ! -d "$MANIFESTS_DIR" ]; then
        echo -e "${RED}Error: Manifests directory not found: $MANIFESTS_DIR${NC}"
        exit 1
    fi
}

# Update image tag in a deployment file
update_deployment() {
    local service="$1"
    local deployment_file="$MANIFESTS_DIR/services/$service/deployment.yaml"

    if [ ! -f "$deployment_file" ]; then
        echo -e "${YELLOW}  Warning: deployment.yaml not found for $service${NC}"
        echo -e "${YELLOW}  Looked in: $deployment_file${NC}"
        return 1
    fi

    # Build the new image reference
    local new_image="${REGISTRY}/${IMAGE_PREFIX}/${service}:${TAG}"

    # Get current image for display
    local current_image=$(yq eval '.spec.template.spec.containers[0].image' "$deployment_file")

    # Update the image field using yq
    # This updates the first container's image - adjust if needed for multiple containers
    yq eval -i ".spec.template.spec.containers[0].image = \"$new_image\"" "$deployment_file"

    echo -e "${GREEN}  ✓ Updated: $deployment_file${NC}"
    echo -e "${BLUE}    $current_image -> $new_image${NC}"

    return 0
}

# Main function
main() {
    check_requirements
    parse_args "$@"

    echo -e "${BLUE}=== Image Tag Update ===${NC}"
    echo -e "${BLUE}Tag: ${TAG}${NC}"
    echo -e "${BLUE}Services: ${SERVICES[*]}${NC}"
    echo -e "${BLUE}Manifests directory: ${MANIFESTS_DIR}${NC}"
    echo -e "${BLUE}Registry: ${REGISTRY}/${IMAGE_PREFIX}${NC}"
    echo

    local updated=0
    local failed=0

    for service in "${SERVICES[@]}"; do
        echo -e "${BLUE}Updating ${service}...${NC}"
        if update_deployment "$service"; then
            updated=$((updated + 1))
        else
            failed=$((failed + 1))
        fi
    done

    echo
    echo -e "${BLUE}=== Summary ===${NC}"
    echo -e "${GREEN}Updated: $updated services${NC}"
    if [ $failed -gt 0 ]; then
        echo -e "${YELLOW}Failed/Skipped: $failed services${NC}"
    fi

    # Show git diff summary if in a git repo
    if [ -d "$MANIFESTS_DIR/.git" ] || git -C "$MANIFESTS_DIR" rev-parse --git-dir >/dev/null 2>&1; then
        echo
        echo -e "${BLUE}=== Modified files ===${NC}"
        git -C "$MANIFESTS_DIR" diff --name-only 2>/dev/null || echo "Unable to show git diff"
    fi
}

main "$@"
