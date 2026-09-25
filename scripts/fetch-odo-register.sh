#!/bin/bash
# Fetch a released odo-register binary instead of building one.
#
# The release workflow attaches odo-register to every GitHub Release as a
# static binary per architecture, with a SHA256SUMS file. This downloads
# the one matching this checkout's tag and this machine's architecture,
# verifies the checksum, confirms the binary reports that tag, and
# installs it. load-data-manifest.sh then finds it on PATH (or through
# ODO_REGISTER) and skips its cargo build, so a host that only runs k3s
# needs no Rust toolchain.
#
# Usage:
#   scripts/fetch-odo-register.sh [--version vX.Y.Z] [--dest DIR] [--repo OWNER/REPO]
#
#   --version  release tag to fetch (default: the tag this checkout is at;
#              fails if HEAD is not exactly on one)
#   --dest     install directory (default: ~/.local/bin)
#   --repo     GitHub repository publishing the releases (default: kcls/odo)
#
# Environment:
#   GITHUB_TOKEN   sent on the download requests, for a private fork.
#                  The public repository needs none.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

VERSION=""
DEST="${HOME}/.local/bin"
REPO="kcls/odo"

usage() {
    sed -n '2,/^$/p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
    exit "${1:-0}"
}

while [ $# -gt 0 ]; do
    case "$1" in
        --version) [ $# -ge 2 ] || usage 1; VERSION="$2"; shift 2 ;;
        --dest)    [ $# -ge 2 ] || usage 1; DEST="$2"; shift 2 ;;
        --repo)    [ $# -ge 2 ] || usage 1; REPO="$2"; shift 2 ;;
        --help|-h) usage ;;
        *) echo "Unknown option: $1" >&2; usage 1 ;;
    esac
done

die() { echo "ERROR: $*" >&2; exit 1; }

for tool in curl sha256sum install; do
    command -v "$tool" >/dev/null || die "$tool is required"
done

# The binary and the manifests it applies must come from the same
# release: a manifest key the checkout supports is rejected as invalid by
# a binary that predates it, which reads as a broken manifest. So the
# default is the checkout's own tag, and nothing else is guessed at.
if [ -z "$VERSION" ]; then
    VERSION="$(git -C "$PROJECT_ROOT" describe --tags --exact-match HEAD 2>/dev/null)" \
        || die "HEAD is not on a release tag; pass --version vX.Y.Z (odo checkout: $PROJECT_ROOT)"
fi
[[ "$VERSION" =~ ^v[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$ ]] \
    || die "'$VERSION' is not a release tag (vX.Y.Z[-suffix])"

case "$(uname -s)-$(uname -m)" in
    Linux-x86_64)  TARGET="x86_64-unknown-linux-musl" ;;
    Linux-aarch64) TARGET="aarch64-unknown-linux-musl" ;;
    *) die "no released odo-register for $(uname -s) $(uname -m)" ;;
esac

ASSET="odo-register-${TARGET}"
BASE_URL="https://github.com/${REPO}/releases/download/${VERSION}"

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

# The token goes in via a config file on stdin: as -H it would sit in the
# process table for the life of the transfer.
fetch() {  # fetch <url> <out>
    local -a auth=()
    if [ -n "${GITHUB_TOKEN:-}" ]; then
        auth=(--config -)
        printf 'header = "Authorization: Bearer %s"\n' "$GITHUB_TOKEN" \
            | curl -fsSL "${auth[@]}" -o "$2" "$1"
    else
        curl -fsSL -o "$2" "$1"
    fi
}

echo "Fetching odo-register $VERSION ($TARGET) from $REPO"
fetch "$BASE_URL/SHA256SUMS" "$TMP/SHA256SUMS" \
    || die "no SHA256SUMS at $BASE_URL -- is $VERSION a release that shipped binaries?"
fetch "$BASE_URL/$ASSET" "$TMP/$ASSET" \
    || die "could not download $BASE_URL/$ASSET"

# --ignore-missing: the sums file lists every architecture; only one was
# downloaded. A missing or altered file still fails.
( cd "$TMP" && sha256sum --check --ignore-missing --strict SHA256SUMS ) \
    || die "checksum mismatch for $ASSET"
grep -q " $ASSET\$" "$TMP/SHA256SUMS" || die "$ASSET is not listed in SHA256SUMS"

chmod 0755 "$TMP/$ASSET"
reported="$("$TMP/$ASSET" --version 2>/dev/null || true)"
[ "$reported" = "odo-register $VERSION" ] \
    || die "downloaded binary reports '${reported:-nothing}', expected 'odo-register $VERSION'"

mkdir -p "$DEST"
install -m 0755 "$TMP/$ASSET" "$DEST/odo-register"
echo "Installed $DEST/odo-register ($reported)"

case ":$PATH:" in
    *":$DEST:"*) ;;
    *)
        echo
        echo "$DEST is not on PATH. Either add it, or point the loader at the binary:"
        echo "  export ODO_REGISTER=$DEST/odo-register"
        ;;
esac
