#!/bin/bash
# Apply one or more app registration manifests to the odo platform.
#
# The odo-registration machine account holds write permissions across
# auth, notify and asset, so it ships disabled with an unknowable
# password (002_odo_seed / 004_registration_account_lockdown). This
# script is the supported way to use it: it activates the account, sets a
# password it generates here, runs odo-register, and switches the account
# back off -- including on failure or interrupt.
#
# Hosted apps therefore never hold registration credentials. Manifests
# still ship with the app that owns them (this repo stays platform-only);
# point this script at the app's manifest, as a local path or an https
# URL. A GitHub blob URL (the one the browser shows) is rewritten to its
# raw equivalent, since the blob URL serves an HTML page rather than the
# file.
#
# Usage: ./load-data-manifest.sh <manifest.json|url> [more...]
#
# Environment variables:
#   ODO_URL=url               Gateway base URL (default: http://localhost:30080)
#   ODO_REGISTER=path         odo-register binary (default: looked up on PATH,
#                             then src/rust/odo-register/target/release|debug)
#   GITHUB_TOKEN=token        Sent as a bearer token when fetching from
#                             github.com / raw.githubusercontent.com, for
#                             manifests in a private repo
#   ODO_ALLOW_INSECURE_URL=1  Permit plaintext http:// manifest URLs
#   PGHOST/PGPORT/PGDATABASE/PGUSER/PGPASSWORD
#                             Override the database connection (default: from
#                             the postgres-credentials secret)
#   NAMESPACE=name            Kubernetes namespace for secrets (default: odo-core)
#
# On a dev box: this leaves the account disabled when it finishes, which
# is what the integration and e2e suites do not expect. Re-run
# ./scripts/manage-database.sh deploy-test to put the dev password back.
#
# A note on the exposure window: odo-auth does not re-check account status
# when validating a token (odo-service require_auth verifies the
# signature only), so the access token minted during the run stays valid
# until it expires -- ACCESS_TOKEN_EXPIRE_MINUTES, 10 by default -- even
# after this script disables the account again. Disabling closes the
# login path, not outstanding tokens.

# No -u: common.sh reads optional PG* overrides that are normally unset.
set -eo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

source "$SCRIPT_DIR/common.sh"

REGISTRATION_USER="odo-registration"

usage() {
    echo -e "${BLUE}Usage: $0 <manifest.json> [more-manifests...]${NC}"
    echo
    echo "Applies app registration manifests using the odo-registration machine"
    echo "account, which is activated for the duration of the run and disabled"
    echo "again afterwards."
    echo
    echo "Environment variables:"
    echo "  ODO_URL=url               Gateway base URL (default: http://localhost:30080)"
    echo "  ODO_REGISTER=path         odo-register binary to run"
    echo "  PGHOST/PGPORT/PGDATABASE/PGUSER/PGPASSWORD"
    echo "                            Override the database connection"
    echo "  NAMESPACE=name            Kubernetes namespace for secrets (default: odo-core)"
    echo
    echo "Examples:"
    echo "  $0 ../current/src/registration/current-manifest.json"
    echo "  $0 https://github.com/kcls/current/blob/main/src/registration/current-manifest.json"
    exit 1
}

[ $# -ge 1 ] || usage

# Locate odo-register: an explicit override, then PATH, then a local build.
find_odo_register() {
    if [ -n "${ODO_REGISTER:-}" ]; then
        echo "$ODO_REGISTER"; return 0
    fi
    if command -v odo-register &>/dev/null; then
        command -v odo-register; return 0
    fi
    local target="$PROJECT_ROOT/src/rust/odo-register/target"
    for build in release debug; do
        if [ -x "$target/$build/odo-register" ]; then
            echo "$target/$build/odo-register"; return 0
        fi
    done
    return 1
}

if ! ODO_REGISTER_BIN="$(find_odo_register)"; then
    echo -e "${RED}odo-register not found.${NC}" >&2
    echo "Build it with: (cd src/rust/odo-register && cargo build --release)" >&2
    echo "or set ODO_REGISTER=/path/to/odo-register" >&2
    exit 1
fi

# Anything downloaded lands here and is removed on the way out. Created
# lazily so a run with only local paths touches no temp storage.
FETCH_DIR=""
# Tracks whether the machine account still needs switching back off, so
# one trap can cover both jobs (bash allows only one handler per signal).
ACCOUNT_ACTIVATED=0

# Fetch a remote manifest, echoing the local path it landed at.
fetch_manifest() {
    local url="$1"

    # The URL a browser shows for a file on GitHub renders an HTML page;
    # the file itself lives on raw.githubusercontent.com.
    if [[ "$url" =~ ^https://github\.com/([^/]+)/([^/]+)/blob/(.+)$ ]]; then
        url="https://raw.githubusercontent.com/${BASH_REMATCH[1]}/${BASH_REMATCH[2]}/${BASH_REMATCH[3]}"
        echo -e "${BLUE}  -> $url${NC}" >&2
    fi

    # A manifest defines permissions and roles and is applied with a
    # privileged account, so it is not something to accept over a channel
    # anyone can rewrite in flight.
    if [[ "$url" != https://* ]] && [ "${ODO_ALLOW_INSECURE_URL:-}" != "1" ]; then
        echo -e "${RED}Refusing to fetch a manifest over plaintext http: $url${NC}" >&2
        echo "Manifests grant permissions and roles; use https." >&2
        echo "Set ODO_ALLOW_INSECURE_URL=1 if you really mean it." >&2
        return 1
    fi

    local out="$FETCH_DIR/$(printf '%s' "$url" | md5sum | cut -c1-12).json"

    local ok=0
    if [ -n "${GITHUB_TOKEN:-}" ] && [[ "$url" == https://raw.githubusercontent.com/* ]]; then
        # The token goes in via a config file on stdin: as -H it would sit
        # in the process table for the life of the transfer.
        printf 'header = "Authorization: Bearer %s"\n' "$GITHUB_TOKEN" \
            | curl -fsSL --max-time 60 --config - -o "$out" "$url" || ok=$?
    else
        curl -fsSL --max-time 60 -o "$out" "$url" || ok=$?
    fi
    if [ "$ok" -ne 0 ]; then
        echo -e "${RED}Could not fetch $url (curl exit $ok)${NC}" >&2
        [ -n "${GITHUB_TOKEN:-}" ] || echo "Private repo? Set GITHUB_TOKEN." >&2
        return 1
    fi

    # Catches the usual mistake of fetching an HTML login or 404 page that
    # came back with a 200, which would otherwise surface as an opaque
    # JSON parse error from odo-register.
    if [ "$(tr -d '[:space:]' < "$out" | head -c 1)" != "{" ]; then
        echo -e "${RED}$url did not return a JSON object.${NC}" >&2
        echo "First bytes: $(head -c 120 "$out" | tr -d '\n')" >&2
        return 1
    fi

    echo "$out"
}

init_pg_connection || exit 1

# Send SQL over stdin rather than psql -c, so nothing we run shows up in
# the process table. run_sql_query passes the result rows back on stdout;
# run_sql is the same thing where the rows are of no interest.
run_sql_query() {
    PGPASSWORD="$PGPASSWORD" psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" \
        -d "$PGDATABASE" -v ON_ERROR_STOP=1 -qtA --no-psqlrc -f -
}

run_sql() {
    run_sql_query >/dev/null
}

if [ "$(PGPASSWORD="$PGPASSWORD" psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" \
        -d "$PGDATABASE" -qtA --no-psqlrc \
        -c "SELECT COUNT(*) FROM auth.usr WHERE username = '$REGISTRATION_USER'")" != "1" ]; then
    echo -e "${RED}No '$REGISTRATION_USER' account in $PGDATABASE.${NC}" >&2
    echo "Deploy the schema first: ./scripts/manage-database.sh deploy" >&2
    exit 1
fi

# Leave the account disabled with a password nobody holds, and drop any
# fetched manifests, whatever happens from here -- success, a failed
# manifest, or Ctrl-C.
cleanup() {
    local rc=$?
    trap - EXIT INT TERM

    [ -z "$FETCH_DIR" ] || rm -rf "$FETCH_DIR"

    if [ "$ACCOUNT_ACTIVATED" -eq 0 ]; then
        exit $rc
    fi

    if ! run_sql <<'SQL'
BEGIN;
UPDATE auth.usr
   SET status = 'inactive', updated_at = CURRENT_TIMESTAMP
 WHERE username = 'odo-registration';
UPDATE auth.local_account
   SET password_hash = crypt(gen_random_uuid()::text || gen_random_uuid()::text,
                             gen_salt('bf', 10)),
       failed_login_attempts = 0,
       locked_until = NULL,
       updated_at = CURRENT_TIMESTAMP
 WHERE usr = (SELECT id FROM auth.usr WHERE username = 'odo-registration');
COMMIT;
SQL
    then
        echo -e "${RED}WARNING: failed to disable $REGISTRATION_USER.${NC}" >&2
        echo -e "${RED}Disable it by hand before leaving this install:${NC}" >&2
        echo "  UPDATE auth.usr SET status = 'inactive' WHERE username = '$REGISTRATION_USER';" >&2
        exit 1
    fi
    echo -e "${GREEN}$REGISTRATION_USER disabled${NC}"
    exit $rc
}
trap cleanup EXIT INT TERM

# Resolve every argument to a readable local path before the account is
# activated, so a bad path or URL fails while it is still switched off.
# After the trap, so a fetch that fails partway still cleans up.
MANIFESTS=()
for arg in "$@"; do
    case "$arg" in
        http://*|https://*)
            echo -e "${YELLOW}Fetching $arg${NC}"
            # Created here rather than in fetch_manifest: that runs inside
            # a command substitution, so an assignment there would be lost
            # with the subshell and cleanup would never remove the dir.
            [ -n "$FETCH_DIR" ] || FETCH_DIR="$(mktemp -d)"
            resolved="$(fetch_manifest "$arg")" || exit 1
            MANIFESTS+=("$resolved")
            ;;
        *)
            if [ ! -r "$arg" ]; then
                echo -e "${RED}Manifest not readable: $arg${NC}" >&2
                exit 1
            fi
            MANIFESTS+=("$arg")
            ;;
    esac
done

echo -e "${YELLOW}Activating $REGISTRATION_USER${NC}"
ACCOUNT_ACTIVATED=1
# The password is generated inside the database and comes back in the
# result set, so it appears in no statement text: a server running with
# log_statement = 'all' records this SQL and no secret.
# auth.update_user_password() hashes it in place, so the client never
# handles a hash either.
#
# It used to travel the other way, as a PGOPTIONS custom GUC. That fails
# through a connection pooler -- pgbouncer rejects any startup parameter
# in `options` outside its known set ("unsupported startup parameter in
# options: odo.reg_pw") -- and DATABASE_URL may well point at one.
#
# MATERIALIZED keeps the planner from evaluating gen_random_bytes() once
# per reference, which would hash one string and hand us another. Hex so
# the value needs no quoting as a shell word. No row comes back if the
# account has no auth.local_account row, which the length check catches.
REGISTRATION_PASSWORD="$(run_sql_query <<SQL
BEGIN;
UPDATE auth.usr
   SET status = 'active', updated_at = CURRENT_TIMESTAMP
 WHERE username = '${REGISTRATION_USER}';
WITH pw AS MATERIALIZED (
    SELECT encode(public.gen_random_bytes(32), 'hex') AS secret
)
SELECT pw.secret
  FROM pw
 WHERE auth.update_user_password(
           (SELECT id FROM auth.usr WHERE username = '${REGISTRATION_USER}'),
           pw.secret);
COMMIT;
SQL
)"

if [ "${#REGISTRATION_PASSWORD}" -ne 64 ]; then
    echo -e "${RED}Could not set a password for $REGISTRATION_USER.${NC}" >&2
    echo "The account has no auth.local_account row; re-deploy the schema." >&2
    exit 1
fi
export REGISTRATION_PASSWORD

echo -e "${YELLOW}Applying ${#MANIFESTS[@]} manifest(s) via ${ODO_REGISTER_BIN}${NC}"
REGISTRATION_USERNAME="$REGISTRATION_USER" \
ODO_URL="${ODO_URL:-http://localhost:30080}" \
    "$ODO_REGISTER_BIN" "${MANIFESTS[@]}"

echo -e "${GREEN}Registration complete${NC}"
