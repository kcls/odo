#!/bin/bash
set -e

# Regenerate the committed OpenAPI specs for the odo services, and (when the
# admin UI is present) the TypeScript types generated from them.
#
# The specs are the source of truth for the generated client types. Each
# service produces its spec at compile time from its utoipa ApiDoc via
# `--dump-openapi`; this never contacts a running service or a database.
#
# Usage:
#   scripts/generate-openapi.sh          # regenerate specs (and TS types if UI present)
#   scripts/generate-openapi.sh --check  # fail if regenerating would change anything (CI)

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

OPENAPI_DIR="openapi"
# Generated TS types go to each admin UI. Both exist while the React
# rewrite (odo-admin-react) replaces the Angular app; drop the Angular
# entry at cutover.
TS_OUT_DIRS=(
    "src/ui/odo-admin/src/app/core/api-types"
    "src/ui/odo-admin-react/src/api/types"
)

# service name -> cargo manifest
SERVICES=(odo-auth odo-org odo-notify odo-asset)

CHECK_MODE=0
[ "${1:-}" = "--check" ] && CHECK_MODE=1

mkdir -p "$OPENAPI_DIR"

echo "Dumping OpenAPI specs..."
for svc in "${SERVICES[@]}"; do
    cargo run --quiet --manifest-path "src/rust/$svc/Cargo.toml" --bin "$svc" -- \
        --dump-openapi "$OPENAPI_DIR/$svc.json"
done


# Generate TypeScript types for each admin UI that is present.
# openapi-typescript emits one file per spec (it does not merge inputs), and
# the services are distinct APIs anyway, so we keep one types module each.
for out_dir in "${TS_OUT_DIRS[@]}"; do
    ui_dir="${out_dir%%/src/*}"
    if [ ! -f "$ui_dir/package.json" ]; then
        echo "$ui_dir not present; skipping its TypeScript generation."
        continue
    fi
    echo "Generating TypeScript types for $ui_dir..."
    mkdir -p "$out_dir"
    for svc in "${SERVICES[@]}"; do
        npx --yes openapi-typescript "$OPENAPI_DIR/$svc.json" \
            -o "$out_dir/$svc.ts" || {
            echo "TypeScript generation failed for $svc" >&2
            exit 1
        }
    done
done

if [ "$CHECK_MODE" = "1" ]; then
    if ! git diff --quiet -- "$OPENAPI_DIR" "${TS_OUT_DIRS[@]}" 2>/dev/null; then
        echo >&2
        echo "ERROR: committed OpenAPI specs / generated types are out of date." >&2
        echo "Run scripts/generate-openapi.sh and commit the result." >&2
        git --no-pager diff --stat -- "$OPENAPI_DIR" "${TS_OUT_DIRS[@]}" >&2
        exit 1
    fi
    echo "OpenAPI specs and generated types are up to date."
fi
