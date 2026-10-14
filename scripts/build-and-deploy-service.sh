#!/bin/bash
#
# Combo script to Build and Deploy a service
set -e

# Call the sibling scripts by path rather than ./scripts/: when run from an
# app checkout, the project root has no scripts/ of its own. Each resolves
# the project root itself (see lib/project.sh).
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

"$SCRIPT_DIR/build-service.sh" "$@" && "$SCRIPT_DIR/deploy-service.sh" "$@"
