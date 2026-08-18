# OpenAPI specs

These JSON files are the committed OpenAPI specifications for the odo
services, and the **source of truth** for the admin UI's generated
TypeScript API types.

| File | Service |
|------|---------|
| `odo-auth.json` | odo-auth (auth, authz/role admin, SAML admin, user admin) |
| `odo-org.json` | odo-org (org unit / type / child-data admin) |
| `odo-notify.json` | odo-notify (email group + template admin, inbox, enqueue) |
| `odo-asset.json` | odo-asset (file upload / retrieval) |

## How they're produced

Each service generates its spec at **compile time** from its `utoipa`
`ApiDoc`, via a `--dump-openapi <path>` flag. This never contacts a running
service or a database:

```
cargo run --manifest-path src/rust/odo-auth/Cargo.toml --bin odo-auth -- \
    --dump-openapi openapi/odo-auth.json
```

Don't run these by hand — use the script, which dumps every service (and,
once the admin UI exists, regenerates the TS types from these specs):

```
scripts/generate-openapi.sh
```

## Keeping them current

After changing any service API (a handler signature, a request/response
struct, a route), regenerate and commit:

```
scripts/generate-openapi.sh
git add openapi/ src/ui/odo-admin/src/app/core/api-types.gen.ts
```

CI runs `scripts/generate-openapi.sh --check`
(`.github/workflows/openapi-drift.yml`) and fails the build if the committed
specs or generated types are stale, so drift can't merge.
