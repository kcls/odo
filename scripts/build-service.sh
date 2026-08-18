#!/bin/bash
set -e

# To build while offline:
#
# DOCKER_BUILDKIT=0 scripts/build-service.sh

# Default values
REGISTRY=${REGISTRY:-localhost:32000}
BUILD_ARGS=${BUILD_ARGS:-}
TAG=${TAG:-latest}
PLATFORM=${PLATFORM:-}
PLATFORM_SET=false
BUILD_ALL=false
BUILD_ALL_SERVICES=false
SERVICES=()

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --tag)
            TAG="$2"
            shift 2
            ;;
        --registry)
            REGISTRY="$2"
            shift 2
            ;;
        --build-args)
            BUILD_ARGS="$2"
            shift 2
            ;;
        --platform)
            PLATFORM="$2"
            PLATFORM_SET=true
            shift 2
            ;;
        --all)
            BUILD_ALL=true
            shift
            ;;
        --all-services)
            BUILD_ALL_SERVICES=true
            shift
            ;;
        --help|-h)
            echo "Usage: $0 [OPTIONS] [SERVICE_NAME...]"
            echo ""
            echo "Options:"
            echo "  --tag TAG              Docker image tag (default: latest)"
            echo "  --registry REGISTRY    Docker registry (default: localhost:32000)"
            echo "  --build-args ARGS      Additional docker build arguments"
            echo "  --platform PLATFORM    Target platform (default: host-detected)"
            echo "  --all                  Build all services in service-map.yaml"
            echo "  --all-services         Build all non-node services in service-map.yaml"
            echo "  --help, -h             Show this help message"
            echo ""
            echo "Examples:"
            echo "  $0 odo-auth odo-org    Build odo-auth and odo-org services"
            echo "  $0 --all               Build all services"
            echo "  $0 --all-services      Build all Rust services"
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

# Detect host architecture for default platform if not provided via CLI/env
if [[ -z "$PLATFORM" ]]; then
    HOST_ARCH=$(uname -m 2>/dev/null || echo unknown)
    case "$HOST_ARCH" in
        x86_64|amd64)
            PLATFORM="linux/amd64"
            ;;
        arm64|aarch64)
            PLATFORM="linux/arm64"
            ;;
        *)
            echo "Warning: unknown host arch '$HOST_ARCH'; defaulting to linux/amd64" >&2
            PLATFORM="linux/amd64"
            ;;
    esac
fi

# Function to get service info from service-map.yaml
get_service_info() {
    local service=$1
    local field=$2
    yq eval ".services.$service.$field" service-map.yaml
}

# Function to build a single service
build_one() {
    local service="$1"

    echo "-----------------------------------------------------------------------"
    echo "Building service: $service"
    echo "Registry: $REGISTRY"
    echo "Tag: $TAG"
    echo "Platform: $PLATFORM"
    echo "-----------------------------------------------------------------------"

    # Get dockerfile and context from service-map.yaml
    local dockerfile=$(get_service_info "$service" "build.dockerfile")
    local context=$(get_service_info "$service" "build.context")
    local language=$(get_service_info "$service" "language")

    if [[ "$dockerfile" == "null" ]] || [[ -z "$dockerfile" ]]; then
        echo "Error: No dockerfile specified for service $service in service-map.yaml"
        return 1
    fi

    if [[ "$context" == "null" ]] || [[ -z "$context" ]]; then
        context="."
    fi

    echo "Dockerfile: $dockerfile"
    echo "Context: $context"
    echo "Language: $language"

    # Build the image
    docker build ${BUILD_ARGS} \
        --build-arg TARGETPLATFORM="${PLATFORM}" \
        --build-arg BUILD_TAG="${TAG}" \
        --build-arg BUILD_DATE="$(date -u +'%Y-%m-%dT%H:%M:%SZ')" \
        --build-arg BUILD_COMMIT="$(git rev-parse HEAD 2>/dev/null || echo 'unknown')" \
        -f "$dockerfile" \
        -t "$REGISTRY/$service:$TAG" \
        "$context"

    echo "Pushing image to registry..."
    docker push "$REGISTRY/$service:$TAG"
    echo "Image $service:$TAG pushed successfully."
    echo ""
}

# Get list of services based on options
if [[ "$BUILD_ALL" == true ]]; then
    echo "Building all services from service-map.yaml..."
    SERVICES=($(yq eval '.services | keys | .[]' service-map.yaml))

elif [[ "$BUILD_ALL_SERVICES" == true ]]; then
    echo "Building all non-node services from service-map.yaml..."
    SERVICES=($(yq eval '.services | to_entries | .[] | select(.value.language != "node") | .key' service-map.yaml))

elif [[ ${#SERVICES[@]} -eq 0 ]]; then
    echo "Error: No services specified. Exiting"
    exit 0
fi

# Build each service
for service in "${SERVICES[@]}"; do
    build_one "$service"
done

echo "All builds completed successfully!"
