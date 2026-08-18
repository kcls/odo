# Odo — guidance for Claude

Odo is a library-domain platform: shared auth (odo-auth), org structure
(odo-org), notifications (odo-notify), and file storage (odo-asset)
behind an Envoy gateway, plus an admin SPA. Applications (e.g.
kcls/current, the incident tracker) live in their own repos, consume the
odo HTTP APIs and the `odo-client` crate, and run against their own
databases. This repo must stay platform-only: no app roles, permissions,
templates, or fixtures.

## Developer Preferences

- Do not add 'Co-authored-by' metadata to commit messages.
- Author all git commits as the human user.

## Layout

- `src/rust/` — one standalone crate per service (no workspace):
  odo-auth, odo-org, odo-notify, odo-asset; shared crates odo-client
  (HTTP clients, error, context, JWT), odo-service (server scaffold,
  middleware, `page_type!`), odo-entity (SeaORM entities, private to the
  services — apps must NOT depend on it); odo-register (CLI: applies app
  registration manifests to the APIs).
- `src/sqitch/schema/` — sqitch project: 001 baseline (squashed schema) +
  002 seed (permissions, platform roles, machine accounts, the "Odo
  Library System" demo org tree with pinned `5eed0000-…` uuids).
- `src/test-data/` — flat idempotent SQL e2e fixtures (`e2e.*` users with
  pinned `e2e00000-…` uuids); applied by `manage-database.sh deploy-test`.
- `src/integration-tests/`, `src/e2e/` (Playwright, odo-admin project),
  `src/db-tests/` (pgTAP), `src/load-tests/` (weighted API load harness).
- `src/ui/odo-admin/` — Angular admin SPA (`/odo/admin`); `src/ui/core` —
  shared UI lib it depends on.
- `k8s/` — gateway, envoy routes/security, postgres, registry. The README
  carries the gateway routing registry (claimed path prefixes).

## Build / deploy / test (dev k3s cluster)

- `cargo check` per crate from its directory (no workspace root).
  odo-auth also needs `cargo check --features saml`.
- `./scripts/build-service.sh <name>` — ONE service per invocation —
  then `./scripts/deploy-service.sh <name>` (or build-and-deploy).
  Wait ~20s after deploy before hitting the service.
- `./scripts/run-tests.sh --db --integration --e2e --unit --load`
  (dev DB password: `PGPASSWORD=demo123`, `PGHOST=localhost PGPORT=5432`
  when using the direct cluster DB).
- e2e locally: `cd src/e2e && BASE_URL=http://localhost:30080 npm test`.
  The UIs need a recent Node (the Angular CLI requires >= 20).
- OpenAPI: `./scripts/generate-openapi.sh` regenerates `openapi/*.json` +
  the admin UI's generated TS types; `--check` is the drift gate. Commit
  the results whenever handler signatures/schemas change.

## Conventions that matter

- **Durable references**: rows expose integer ids (internal) AND stable
  uuids. Anything an app stores or a JWT carries is the uuid. The JWT
  `org_unit` claim is the working org unit's uuid (string). Never add an
  API that makes an app persist an odo integer id.
- **App registration**: apps install their platform data (permissions,
  roles, grants, templates, asset directories — including their upload
  routing: each directory row may map an entity_type + optional category
  to its path — SAML maps, fixture users/assignments) via a JSON
  manifest applied by `odo-register` with the `odo-registration` machine
  account. Upsert-only; 409 = already
  registered; never deletes. New registration surface belongs behind
  perms held by that account (see the seed).
- Machine accounts (seeded, dev-only default passwords, must-change in
  prod): `odo-registration`, `odo-notify-service`.
- Paginated admin lists use `odo_service::page_type!` (a generic
  Paginated<T> produces untyped rows in the generated TS).
- Soft deletes only (`deleted_at`); a DB trigger blocks hard deletes.
- New HTTP endpoints must be routed in `k8s/infrastructure/envoy/` (and
  applied) or requests fall through with misleading errors; record new
  path prefixes in the README routing registry.

## Gotchas

- Docker/BuildKit can serve stale cargo caches (phantom old code in
  deployed binaries): `docker builder prune --force --filter
  type=exec.cachemount`, then rebuild.
- k8s state is ArgoCD-managed in spirit: express operational changes as
  git edits; prefer handing `kubectl` commands to the user. After
  changing a deployment manifest, verify the change actually landed in
  the cluster (`kubectl get deploy … -o yaml`) — a rolling update can
  leave an old pod serving while a new one crashloops, which makes stale
  config look healthy.
- kcls/current consumes odo-client/odo-service as git dependencies
  pinned by its Cargo.lock: after changing those crates, push this
  repo's `main` to the bare origin (`git push local <branch>:main`) and
  bump the pin in current (`cargo update -p odo-client -p odo-service`).
