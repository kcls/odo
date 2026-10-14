#!/bin/bash
# Which project the build/deploy scripts act on. Sourced, not run.
#
# Applications keep their own service-map.yaml and k8s/services/ but use
# these scripts from an odo checkout instead of copying them, e.g. from
# an app checkout:
#
#   $ODO_HOME/scripts/build-and-deploy-service.sh ui-my-app
#
# The project root is, in order:
#   1. $ODO_PROJECT_ROOT, if set (it must hold a service-map.yaml);
#   2. the nearest directory at or above $PWD with a service-map.yaml;
#   3. the odo checkout these scripts live in, as before this existed.

# Usage: resolve_project_root <odo-root>; prints the project root.
resolve_project_root() {
    local odo_root="$1"

    if [[ -n "${ODO_PROJECT_ROOT:-}" ]]; then
        if [[ ! -f "$ODO_PROJECT_ROOT/service-map.yaml" ]]; then
            echo "Error: ODO_PROJECT_ROOT=$ODO_PROJECT_ROOT has no service-map.yaml" >&2
            return 1
        fi
        (cd "$ODO_PROJECT_ROOT" && pwd)
        return
    fi

    local dir="$PWD"
    while true; do
        if [[ -f "$dir/service-map.yaml" ]]; then
            echo "$dir"
            return
        fi
        [[ "$dir" == "/" ]] && break
        dir="$(dirname "$dir")"
    done

    echo "$odo_root"
}
