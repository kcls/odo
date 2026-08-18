#!/bin/bash
#
# Set (or unset) an environment variable on every running Rust service
# deployment and roll it out.
#
# Rust services are discovered from service-map.yaml (language: rust). For each
# one, this applies the variable to the running Deployment via
# `kubectl set env`, which patches the pod template and triggers a rolling
# restart, then waits for the rollout to finish.
#
# This changes only the *running* deployments in the cluster; it does not edit
# the k8s manifests under k8s/, so the change is reverted the next time a
# service is redeployed from its manifest (deploy-service.sh). It is intended
# for temporary, runtime toggles (e.g. turning on verbose logging to debug a
# live issue).
#
# Usage:
#   scripts/set-service-env.sh KEY=VALUE [options]
#   scripts/set-service-env.sh --unset KEY [options]
#
# Options:
#   --service NAME    Target only this service (repeatable). Default: all Rust
#                     services in service-map.yaml.
#   --unset KEY       Remove the variable KEY instead of setting it.
#   --no-wait         Don't wait for each rollout to complete.
#   --help, -h        Show this help.
#
# Examples:
#   # Enable HTTP request-body logging on every Rust service and roll out:
#   scripts/set-service-env.sh ODO_LOG_HTTP_REQUEST_BODY=true
#
#   # Same, but only for odo-auth:
#   scripts/set-service-env.sh ODO_LOG_HTTP_REQUEST_BODY=true --service odo-auth
#
#   # Turn it back off (remove the variable) everywhere:
#   scripts/set-service-env.sh --unset ODO_LOG_HTTP_REQUEST_BODY

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

WAIT=true
UNSET_KEY=""
ENV_ASSIGNMENT=""
SERVICES=()

usage() {
    # Print the leading comment block (the lines starting with '#') as help.
    sed -n '3,36p' "$0" | sed 's/^# \{0,1\}//'
    exit "${1:-0}"
}

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --unset)
            UNSET_KEY="$2"
            shift 2
            ;;
        --service)
            SERVICES+=("$2")
            shift 2
            ;;
        --no-wait)
            WAIT=false
            shift
            ;;
        --help|-h)
            usage 0
            ;;
        -*)
            echo -e "${RED}Unknown option: $1${NC}" >&2
            usage 1
            ;;
        *)
            if [[ -n "$ENV_ASSIGNMENT" ]]; then
                echo -e "${RED}Only one KEY=VALUE assignment is supported per run.${NC}" >&2
                usage 1
            fi
            ENV_ASSIGNMENT="$1"
            shift
            ;;
    esac
done

# Validate: exactly one of KEY=VALUE or --unset KEY
if [[ -n "$ENV_ASSIGNMENT" && -n "$UNSET_KEY" ]]; then
    echo -e "${RED}Provide either KEY=VALUE or --unset KEY, not both.${NC}" >&2
    usage 1
fi
if [[ -z "$ENV_ASSIGNMENT" && -z "$UNSET_KEY" ]]; then
    echo -e "${RED}Provide a KEY=VALUE assignment (or --unset KEY).${NC}" >&2
    usage 1
fi
if [[ -n "$ENV_ASSIGNMENT" && "$ENV_ASSIGNMENT" != *=* ]]; then
    echo -e "${RED}Expected KEY=VALUE, got: $ENV_ASSIGNMENT${NC}" >&2
    usage 1
fi

# Run from the project root so service-map.yaml resolves.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_ROOT"

if ! command -v yq &> /dev/null; then
    echo -e "${RED}yq is required but not found.${NC}" >&2
    exit 1
fi
if ! command -v kubectl &> /dev/null; then
    echo -e "${RED}kubectl is required but not found.${NC}" >&2
    exit 1
fi

get_service_info() {
    local service=$1
    local field=$2
    yq eval ".services.$service.$field" service-map.yaml
}

# Default to every Rust service in the service map.
if [[ ${#SERVICES[@]} -eq 0 ]]; then
    SERVICES=($(yq eval '.services | to_entries | .[] | select(.value.language == "rust") | .key' service-map.yaml))
fi

if [[ ${#SERVICES[@]} -eq 0 ]]; then
    echo -e "${RED}No Rust services found in service-map.yaml.${NC}" >&2
    exit 1
fi

# kubectl set env's env spec: "KEY=VALUE" to set, "KEY-" to unset.
if [[ -n "$UNSET_KEY" ]]; then
    ENV_SPEC="${UNSET_KEY}-"
    ACTION="Unsetting ${UNSET_KEY} on"
else
    ENV_SPEC="$ENV_ASSIGNMENT"
    ACTION="Setting ${ENV_ASSIGNMENT} on"
fi

echo -e "${BLUE}${ACTION} Rust services: ${SERVICES[*]}${NC}"
echo

failed=()
for service in "${SERVICES[@]}"; do
    namespace=$(get_service_info "$service" "namespace")
    if [[ "$namespace" == "null" || -z "$namespace" ]]; then
        echo -e "${RED}✗ $service: no namespace in service-map.yaml, skipping${NC}"
        failed+=("$service")
        continue
    fi

    echo -e "${YELLOW}▶ $service (namespace: $namespace)${NC}"

    if ! kubectl get "deployment/$service" -n "$namespace" &> /dev/null; then
        echo -e "${RED}  ✗ deployment/$service not found in $namespace, skipping${NC}"
        failed+=("$service")
        continue
    fi

    # `kubectl set env` patches the pod template, which triggers a rolling
    # restart on its own.
    if ! kubectl set env "deployment/$service" -n "$namespace" "$ENV_SPEC"; then
        echo -e "${RED}  ✗ failed to update env for $service${NC}"
        failed+=("$service")
        continue
    fi

    if [[ "$WAIT" == true ]]; then
        if kubectl rollout status "deployment/$service" -n "$namespace" --timeout=180s; then
            echo -e "${GREEN}  ✓ $service rolled out${NC}"
        else
            echo -e "${RED}  ✗ $service rollout did not complete in time${NC}"
            failed+=("$service")
        fi
    else
        echo -e "${GREEN}  ✓ $service updated (rollout not awaited)${NC}"
    fi
    echo
done

if [[ ${#failed[@]} -gt 0 ]]; then
    echo -e "${RED}Completed with failures: ${failed[*]}${NC}" >&2
    exit 1
fi

echo -e "${GREEN}All Rust services updated successfully.${NC}"
