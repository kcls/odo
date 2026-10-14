#!/bin/bash
#
# Combo script to Build and Deploy a service
set -e

# Call the sibling scripts by path rather than ./scripts/: when run from an
# app checkout, the project root has no scripts/ of its own. Each resolves
# the project root itself (see lib/project.sh).
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Each script takes any argument it does not recognize as a service name,
# so route each one's own options to it alone; everything else (service
# names, --all, --all-services, --help) goes to both.
build_argv=()
deploy_argv=()
while [[ $# -gt 0 ]]; do
    case $1 in
        --tag|--registry|--build-args|--platform)
            if [[ $# -lt 2 ]]; then
                echo "Error: $1 needs a value" >&2
                exit 1
            fi
            build_argv+=("$1" "$2")
            shift 2
            ;;
        --wait|--delete)
            deploy_argv+=("$1")
            shift
            ;;
        *)
            build_argv+=("$1")
            deploy_argv+=("$1")
            shift
            ;;
    esac
done

"$SCRIPT_DIR/build-service.sh" "${build_argv[@]}" && "$SCRIPT_DIR/deploy-service.sh" "${deploy_argv[@]}"
