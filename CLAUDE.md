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
- Keep commit message bodies concise, limited to 2 paragraphs; no need to
  fully document features in commit messages.

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
- `k8s/` — gateway, envoy routes/security, registry. The README
  carries the gateway routing registry (claimed path prefixes).
  PostgreSQL is NOT in the cluster: it runs on the host (or an external
  server). The installers write ONE `DATABASE_URL` into the
  `postgres-credentials` secret, read by the pods and by the host
  tooling alike — so its host must be reachable from both (a LAN
  address or DNS name; `localhost` is rejected).
- `scripts/setup/` — Ubuntu installs split three ways:
  `install-postgres-server-ubuntu.sh` (PG only),
  `install-k3s-cluster-ubuntu.sh` (k3s/Docker/infra), both
  production-usable, then `setup-dev-cluster-ubuntu.sh` (toolchains,
  schema, test data, services — dev only). `setup-common.sh` holds
  their shared helpers. macOS remains a single script.

## Build / deploy / test (dev k3s cluster)

- `cargo check` per crate from its directory (no workspace root).
  odo-auth also needs `cargo check --features saml`.
- `./scripts/build-service.sh <name>` — ONE service per invocation —
  then `./scripts/deploy-service.sh <name>` (or build-and-deploy).
  Wait ~20s after deploy before hitting the service.
- `./scripts/run-tests.sh --db --integration --e2e --unit --load`
  (the DB endpoint resolves from the secret's `DATABASE_URL`; `PG*` env
  vars only override it).
- e2e locally: `cd src/e2e && BASE_URL=http://localhost:30080 npm test`.
  The UIs need a recent Node (the Angular CLI requires >= 20).
- OpenAPI: `./scripts/generate-openapi.sh` regenerates `openapi/*.json` +
  the admin UI's generated TS types; `--check` is the drift gate. Commit
  the results whenever handler signatures/schemas change.
- CI (`.github/workflows/ci.yml`, on PRs and pushes to main/release):
  clippy `-D warnings` + `cargo test` for all eight crates (odo-auth also
  with `--features saml`), then core typecheck/lint/build and the admin UI's
  tests and build. `openapi-drift.yml` is the spec gate and is enabled.
- Releases: `.github/workflows/release-build.yml` publishes images to
  `ghcr.io/<owner>/<repo>/<service>` from `release/**` pushes (`:<short-sha>`)
  and `vX.Y.Z` tags (`:<short-sha>`, `:vX.Y.Z`, `:vX.Y`). This repo publishes
  only — it holds no credential for, and never writes to, any deployment
  repository, and ships no release artifacts: deployment repos consume `k8s/`
  as a kustomize remote base pinned to the tag. See
  `docs/tech-docs/release-management.md`.

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
  perms held by that account (see the seed). Manifests live in the app's
  own repo; run `./scripts/load-data-manifest.sh <manifest.json>` from here —
  it summarizes what each manifest installs and against which target, waits
  for confirmation (`--force` skips it; a non-TTY without `--force` refuses
  rather than hangs), then activates the account, applies the manifests and
  disables it again, so hosted apps never hold registration credentials.
- **Org structure via manifest**: `org_unit_types` (by `label`) and
  `org_units` (by `code`, with `parent` code and `unit_type` label) are
  manifest keys too, applied before everything else so later
  `user_role_assignments` resolve. Natural keys only — odo-register maps
  them to ids against the live tree, so **a parent must be listed before
  its children**, and the single root is seeded, never registered.
  `005_registration_org_units` grants the account
  `odo.org.unit.read/write` for this; it is the one grant that exists for
  an installation's own site data rather than for app registration, so
  split it into a separate account if app manifests ever become less
  trusted.
- Machine accounts: `odo-registration` ships disabled with an unknowable
  password (the seed plus `004_registration_account_lockdown`); only
  `load-data-manifest.sh` enables it, and `src/test-data/` restores a known
  dev password for dev/CI. `odo-notify-service` still has a dev-only
  seeded password that must be changed in prod.
- Paginated admin lists use `odo_service::page_type!` (a generic
  Paginated<T> produces untyped rows in the generated TS).
- Soft deletes only (`deleted_at`); a DB trigger blocks hard deletes.
- A sqitch **verify** script must only assert what holds on every install.
  `src/test-data/` deliberately reverses some schema effects for dev/CI (it
  reactivates `odo-registration` and restores its published password), so a
  verify that asserts account status or a password hash fails permanently on
  any box that has run `deploy-test` — which silently costs you `verify` as a
  gate everywhere. Assert structure; leave environment-dependent state to the
  deploy script.
- New HTTP endpoints must be routed in `k8s/infrastructure/envoy/` (and
  applied) or requests fall through with misleading errors; record new
  path prefixes in the README routing registry.

## Gotchas

- `src/ui/core` must be BUILT before `src/ui/odo-admin` will build:
  `@odo/core` resolves to `./dist/index.js`, so `ng build` fails with
  "Cannot find module '@odo/core'" until `npm run build` has run in core.
- `--features saml` builds samael, which links xmlsec1/libxml2 and needs
  clang for bindgen: `pkg-config libssl-dev libxml2-dev libxmlsec1-dev
  libxmlsec1-openssl libclang-dev clang` (the list the odo-auth Dockerfile
  installs). Without them the build fails in samael's build script.
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
