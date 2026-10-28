#!/bin/sh
# Place the SPA at ODO_ADMIN_BASE_PATH (default /odo/admin).
#
# The image builds the app with a placeholder base path, because Vite bakes
# the base into index.html and the bundles at build time. This script runs
# before nginx starts (the nginx image runs /docker-entrypoint.d/*.sh) and:
#
#   1. copies the pristine build into the served directory, so a restarted
#      container with a different setting starts from the placeholder again
#      rather than from the previous substitution;
#   2. replaces the placeholder with the configured base;
#   3. renders the nginx config for that base.
#
# Set ODO_ADMIN_BASE_PATH=/ to serve the app at the host root.
set -eu

PLACEHOLDER=/__ODO_ADMIN_BASE_PATH__/
DIST=/usr/share/nginx/html-dist
HTML=/usr/share/nginx/html
TEMPLATE=/etc/nginx/templates-odo-admin/default.conf.template
CONF=/etc/nginx/conf.d/default.conf

raw="${ODO_ADMIN_BASE_PATH:-/odo/admin}"

# The value lands in sed replacements and nginx location blocks, so allow
# only plain path characters rather than trying to escape for both.
case "$raw" in
    /*) ;;
    *) echo "ODO_ADMIN_BASE_PATH must start with '/': $raw" >&2; exit 1 ;;
esac
if ! printf '%s' "$raw" | grep -Eq '^[A-Za-z0-9/._~-]+$'; then
    echo "ODO_ADMIN_BASE_PATH has unsupported characters: $raw" >&2
    exit 1
fi
case "$raw" in
    *//*|*/./*|*/../*|*/.|*/..)
        echo "ODO_ADMIN_BASE_PATH is not a normalized path: $raw" >&2
        exit 1 ;;
esac

# PREFIX has no trailing slash ("" at the root); BASE always has one.
prefix="$(printf '%s' "$raw" | sed 's#/*$##')"
base="$prefix/"

rm -rf "${HTML:?}"/*
cp -R "$DIST"/. "$HTML"/
find "$HTML" -type f \( -name '*.html' -o -name '*.js' -o -name '*.css' \
    -o -name '*.json' -o -name '*.map' \) \
    -exec sed -i "s#$PLACEHOLDER#$base#g" {} +

if grep -rqF "$PLACEHOLDER" "$HTML"; then
    echo "Base path placeholder still present after substitution:" >&2
    grep -rlF "$PLACEHOLDER" "$HTML" >&2
    exit 1
fi

if [ -n "$prefix" ]; then
    # Without this, a request for the bare prefix (no trailing slash) misses
    # every location and gets the 404 page instead of the app.
    redirect="    location = $prefix { return 301 $base\$is_args\$args; }"
else
    redirect=""
fi

sed -e "s#@PREFIX@#$prefix#g" \
    -e "s#@BARE_PREFIX_REDIRECT@#$redirect#" \
    "$TEMPLATE" > "$CONF"

echo "odo-admin: serving at $base"
