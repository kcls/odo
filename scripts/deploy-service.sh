#!/bin/bash
set -e

# Default values
DEPLOY_ALL=false
DEPLOY_ALL_SERVICES=false
SERVICES=()
WAIT=false
DELETE=false

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --all)
            DEPLOY_ALL=true
            shift
            ;;
        --all-services)
            DEPLOY_ALL_SERVICES=true
            shift
            ;;
        --wait)
            WAIT=true
            shift
            ;;
        --delete)
            DELETE=true
            shift
            ;;
        --help|-h)
            echo "Usage: $0 [OPTIONS] [SERVICE_NAME...]"
            echo ""
            echo "Options:"
            echo "  --wait                 Wait for the deployment to complete"
            echo "  --delete               Delete the service manifests instead of deploying"
            echo "  --all                  Deploy all services in service-map.yaml"
            echo "  --all-services         Deploy all non-node services in service-map.yaml"
            echo "  --help, -h             Show this help message"
            echo ""
            echo "Examples:"
            echo "  $0 odo-auth odo-org    Deploy odo-auth and odo-org services"
            echo "  $0 --all               Deploy all services"
            echo "  $0 --all-services      Deploy all Rust services"
            exit 0
            ;;
        *)
            SERVICES+=("$1")
            shift
            ;;
    esac
done

# Ensure we're in the project root
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_ROOT"

# Function to get service info from service-map.yaml
get_service_info() {
    local service=$1
    local field=$2
    yq eval ".services.$service.$field" service-map.yaml
}


# Function to deploy a single service
deploy_one() {
    local service="$1"

    echo "-----------------------------------------------------------------------"
    echo "Deploying service: $service"
    echo "-----------------------------------------------------------------------"

    # Get namespace from service-map.yaml
    local namespace=$(get_service_info "$service" "namespace")

    if [[ "$namespace" == "null" ]] || [[ -z "$namespace" ]]; then
        echo "Error: No namespace specified for service $service in service-map.yaml"
        return 1
    fi

    # Determine service directory
    local service_dir="k8s/services/$service"

    # Check if service directory exists
    if [[ ! -d "$service_dir" ]]; then
        echo "Error: Service directory not found for $service"
        return 1
    fi

    if [[ "$DELETE" == true ]]; then
        echo "Deleting manifest"
        kubectl delete -k "$service_dir" || true;
        return;
    fi;

    echo "Service directory: $service_dir"
    echo "Namespace: $namespace"

    # Apply the Kubernetes manifests
    echo "Applying Kubernetes manifests..."
    if kubectl apply -k "$service_dir"; then
        echo "Manifests applied successfully"
    else
        echo "Error: Failed to apply manifests for $service"
        return 1
    fi

    # Perform rolling restart to ensure latest image is pulled
    if kubectl rollout restart "deployment/$service" -n "$namespace" 2>/dev/null; then
        if [[ "$WAIT" == true ]]; then
            # Wait for rollout to complete
            echo "Waiting for rollout to complete..."
            if kubectl rollout status "deployment/$service" -n "$namespace" --timeout=300s; then
                echo "Deployment $service successfully rolled out"
            else
                echo "Warning: Rollout status check timed out for $service"
            fi
        fi
    fi

    echo "Service $service deployed successfully"
    echo ""
}

# Get list of services based on options
if [[ "$DEPLOY_ALL" == true ]]; then
    echo "Deploying all services from service-map.yaml..."
    SERVICES=($(yq eval '.services | keys | .[]' service-map.yaml))

elif [[ "$DEPLOY_ALL_SERVICES" == true ]]; then
    echo "Deploying all non-node services from service-map.yaml..."
    SERVICES=($(yq eval '.services | to_entries | .[] | select(.value.language != "node") | .key' service-map.yaml))

elif [[ ${#SERVICES[@]} -eq 0 ]]; then
    echo "Error: No services specified. Use --help for usage information."
    exit 1
fi

echo "Services to deploy: ${SERVICES[@]}"
echo ""

for service in "${SERVICES[@]}"; do
    deploy_one "$service"
done

