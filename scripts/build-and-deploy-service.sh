#!/bin/bash
# 
# Combo script to Build and Deploy a service
set -e

# Ensure we're in the project root
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_ROOT"

./scripts/build-service.sh $@ && ./scripts/deploy-service.sh $@



