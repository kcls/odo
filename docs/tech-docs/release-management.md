# Release Management

Status: phases 1 and 2 are **implemented and proven** — both public
repositories build releases and gate on CI, and images publish to the new
namespace (`ghcr.io/kcls/odo/odo-org:b0e51cf` and friends). Phase 3 is
under way: `kcls/odo-deploy` exists, its `clusters/dev` runs on pinned remote
bases, and `site-data/dev` is scaffolded but holds no data. Phase 4 — running
a real release through the runbook below — remains.

Odo and its applications are separate open-source projects with separate
release cycles — this repo, and `kcls/current` (the reference application).
They were one repository (`sjora`) until the split, and the release machinery
still reflects that: a single build workflow that wrote deployment image tags
straight into a private GitOps repository shared by every project.

That no longer works, and it was never right for a public repository.

## The organizing principle

> **Public repositories produce versioned images and manifests. A private
> repository decides what runs where.**

A public workflow must not hold a credential for a private repository, name
internal hosts, or encode one organization's deployment topology. Everything
an operator needs to deploy odo lives in this repository, at a version they
can pin; everything specific to a given installation lives in that
installation's own private repository.

This also means an outside adopter can consume odo releases the same way KCLS
does, with their own private deployment repository.

## Versioning

Each project versions independently with semver and its own tags.

| | |
| --- | --- |
| Release line branch | `release/X.Y` — cut from `main`, receives backports only |
| Release tag | `vX.Y.Z` on the release branch |
| Development | `main` and feature branches; no images published |

A release line branch exists so a patch can ship without dragging in whatever
has since landed on `main`.

### Image tags

Every service in `service-map.yaml` is built and pushed to
`ghcr.io/<owner>/<repo>/<service>` on each release build — each project owns
its own registry namespace, so two repositories shipping a service of the same
name never collide, and each project's workflow owns (and can therefore create)
its own packages.

| Tag | Published on | Meaning | Moves? |
| --- | --- | --- | --- |
| `<short-sha>` | every build | exactly this commit | never |
| `vX.Y.Z` | tag push only | this release | never |
| `vX.Y.Z-suffix` | tag push only | this prerelease | never |
| `vX.Y` | plain `vX.Y.Z` only | newest **released** patch on the line | yes |

> [!NOTE]
> The flat `ghcr.io/kcls/<service>` packages predate the split — sjora created
> and owns them, and production still pulls from them. They are frozen, not
> deleted: deleting one breaks any pod that reschedules and has to re-pull.
> Re-pointing deployments at the new namespace is part of phase 3.

Pushing a release branch publishes the sha tag only. The line tag moves only
when a release is actually tagged, so `vX.Y` never points at an untagged
commit.

Deployments pin `vX.Y.Z` or a digest. `vX.Y` is a convenience for development
targets that should track a line.

### Prereleases

A tag may carry a suffix — `v1.2.0-rc1`, `v1.2.0-kcls2` — and publishes its own
full tag plus the sha. **A suffixed tag never moves `vX.Y`**, which is the
whole reason the workflow tells them apart: deriving the line tag by trimming
the last dotted segment turns `v0.1.0-rc1` into `v0.1`, quietly pointing the
stable line at a prerelease.

Semver build metadata (`v1.0.0+build7`) is rejected outright. `+` is legal in
a git tag and in semver but not in a container image tag, so it fails fast
with an explanation instead of deep inside a registry push.

> [!IMPORTANT]
> A published `vX.Y.Z` tag is never re-pointed at a different image. A bad
> release is superseded by `vX.Y.Z+1`, never overwritten — rollback depends on
> the old tag still meaning what it meant.

### Release builds are whole

A release build always builds **every** service, so a version tag names a
coherent set of images. The `[build: …]` commit-message parsing that the old
workflow used survives only as a `workflow_dispatch` input, for re-running a
single service after an infrastructure failure.

### The release workflow

`.github/workflows/release-build.yml`, the same file in both repositories
(service-map-driven, so each one picks up its own services):

| Trigger | Effect |
| --- | --- |
| push to `release/**` | build all services, publish `:<short-sha>` |
| push of tag `vX.Y.Z` | build all services, publish sha + version + line tag |
| push of tag `vX.Y.Z-suffix` | build all services, publish sha + version, no line tag |
| `workflow_dispatch` | build all services (or a comma-separated subset), publish `:<short-sha>` |

A tag that is not `vX.Y.Z[-suffix]` fails the build rather than publishing something
ambiguous, and an unknown service name in the dispatch input fails rather than
silently building nothing.

There is deliberately no deployment step and no credential for any other
repository.

Neither copy publishes release artifacts. Deployment repositories read `k8s/`
and `openapi/` straight out of the tagged source (see below), so there is
nothing to package.

## Cross-project version coupling

Current consumes `odo-client` and `odo-service` as git dependencies pinned by
its `Cargo.lock`, and installs its platform data through odo's
`load-data-manifest.sh`. A Current release is therefore only meaningful
against some range of odo versions.

Two obligations follow, neither yet met:

* Current should pin its git dependency to a **released odo tag** rather than
  to `main`. Today `Cargo.toml` says `branch = "main"` with `Cargo.lock`
  pinning a rev — which is reproducible but says nothing about which odo
  release it corresponds to. This changes once odo has its first tag.
* Each Current release states its odo compatibility range in the release
  notes ("requires odo >= 1.2, < 2.0"). Nothing enforces this at build time,
  so it has to be written down.

## The private deployment repository

One private repository per installation, holding both the Kubernetes manifests
and the site-specific data. Layout:

```
<org>/odo-deploy (private)
  clusters/<env>/          everything ArgoCD syncs for one target
    components/            cross-cutting patches (pull secret, ClusterIP)
    infrastructure/        gateway, envoy (composed), logging, argocd
    services/<svc>/        a pinned remote base + this environment's patches
  site-data/<env>/         applied deliberately; ArgoCD never syncs it
    requires.yaml          schema changes this data assumes
    apply.sh               preflight, then manifests, then SQL
    manifests/*.json       odo-register manifests, numbered, applied in order
    sql/<database>/        sqitch project per target database
  versions.yaml            odo: 1.2.3, current: 0.9.1 — per environment
  scripts/bump-images.sh
  runbook.md
```

`versions.yaml` is the single readable answer to "what is deployed here";
`bump-images.sh` (formerly this repo's `scripts/update-image-tags.sh`) writes
those versions into the overlays.

### Manifests come from the tag, not from a copy

An environment's overlay names each project's `k8s/` tree as a **kustomize
remote base pinned to the release tag**:

```yaml
resources:
  - github.com/kcls/odo//k8s/services/odo-auth?ref=v1.2.3
  - github.com/kcls/current//k8s/services/current?ref=v0.9.1
```

ArgoCD resolves those natively, there is nothing to vendor or keep in sync,
and the version in effect is visible in the diff of any change. The overlay
then patches what is installation-specific — image tags, replicas, hostnames,
secret references — leaving the upstream manifests untouched.

This is why neither public repository publishes release artifacts: the tag
*is* the artifact.

### One repository, not two

Manifests and site data are the same kind of thing — per-environment
configuration that changes together — so a production change is one pull
request. ArgoCD only syncs the paths its `Application`s name, so `site-data/`
is invisible to it.

### Directory per environment, not branch per environment

The predecessor repository used a branch per cluster and per host. Cherry-
picking configuration between six long-lived branches is how its Envoy
configuration became a hand-merge of three projects' routes. Everything is
already kustomize and ArgoCD supports a path per environment natively.

### Composing routes

Each project ships its own Envoy routes (this repo:
`k8s/infrastructure/envoy/`; Current: `k8s/services/*/routes.yaml`). The
deployment repository **composes** them via kustomize from each project's
pinned remote base, rather than maintaining a merged copy by hand.

### Site data: manifests over SQL

Applications are forbidden from writing SQL against the odo database — they
register what they need through `odo-register` manifests. Site data earns the
same rule: upsert-only semantics, permission checks, and durable uuids, for
free.

Users, roles, grants, SAML maps, notification templates, asset directories
**and org structure** all have manifest support. Org units were the last gap,
and a sharp one: `user_role_assignments` references a unit by
`org_unit_code`, so units were a prerequisite for a surface that already
existed, and every installation's bootstrap began with raw SQL. Since
`005_registration_org_units` a manifest can carry `org_unit_types` (keyed by
label) and `org_units` (keyed by code, with the parent's code and the type's
label), which leaves the `sql/` tree with nothing it must hold.

Two constraints come with it. Parents are resolved against the live tree, so
**a manifest must list a parent before its children**. And the single root is
seeded rather than registered — `unit/create` requires a parent — so a
manifest extends the tree and never establishes it.

Granting `odo.org.unit.write` widened the `odo-registration` account, which
002's seed had deliberately kept clear of org structure. That is the one
permission it holds for an installation's own site data rather than for app
registration; if app-supplied manifests ever become less trusted, splitting
site data onto its own account is the way back.

> [!IMPORTANT]
> The manifest schema accepts `users[].password` in cleartext — odo's and
> Current's e2e fixtures use it, which is fine for throwaway accounts in a
> public repo. Site data must not: create the account with no usable password
> and set it out of band, exactly as the deployment repository's secrets work.

### Site data is per database set, not per cluster

`site-data/<env>/` corresponds to a set of databases — odo's and each
application's — rather than to a Kubernetes cluster. Two clusters pointed at
one production database share one tree.

Environments differ in **content**, not merely in version: dev's org tree is a
handful of fakes and production's is the real branch list. That is why there
is no `common/` tree — in a single-branch repository shared content is always
at HEAD for every environment, reintroducing exactly the skew the split
avoids. Promotion is a copy reviewed as a diff (`diff -r site-data/dev
site-data/prod` is the backlog).

Version skew does **not** require a sqitch project per cluster. Sqitch keeps
its registry in the target database, and the environments are separate
databases, so one plan deploys to each target independently and production
sitting several changes behind dev is ordinary operation. Each environment
still gets its own project because the *data* differs, and each carries a
distinct `%project` name (`kcls-site-odo-dev`, not `kcls-site`): registries
are already isolated per database, so unique names buy no structure — what
they buy is that pointing dev's plan at production's database fails loudly
instead of quietly deploying dev's org units into production.

### The version gate

Each environment declares, in `requires.yaml`, the schema changes its data
assumes are already deployed; `apply.sh` checks them against each target
database's sqitch registry and refuses to run if any is missing. So a
lagging cluster fails with a clear message instead of a 4xx from the API
halfway through a manifest.

The gate keys on **sqitch change names rather than release versions** because
the services expose no version endpoint — `/health` returns status only — and
because the schema change is what the data actually depends on. If a version
endpoint is ever added, the gate can check both; it does not need to.

## Deploying a release

Ordering matters, because ArgoCD auto-sync will roll out an image whenever the
manifest changes — including before anyone has migrated the database.

1. Apply schema migrations from a checkout of the release tag
   (`manage-database.sh deploy`).
2. Apply registration manifests — the platform's own, then each application's
   (`load-data-manifest.sh`). Upsert-only; re-running is safe. The script
   summarizes each manifest and the target it is pointed at, and waits for
   confirmation; `--force` skips the prompt for scripted runs.
3. Apply site data for the environment.
4. Bump `versions.yaml` and run `bump-images.sh`; commit. ArgoCD rolls out.

> [!IMPORTANT]
> Migrations run **before** the rollout, which means every migration must be
> backward compatible with the image currently serving. A migration that the
> outgoing binary cannot tolerate has to be split across two releases.

Applying application registration manifests requires an odo checkout, since
`load-data-manifest.sh` lives here — check out the odo release tag the
application declares compatibility with.

### Rollback

Re-pin the previous `vX.Y.Z` in `versions.yaml`. That covers images only; the
schema half needs a deliberate decision per release, since a sqitch revert is
destructive of anything written since. Releases that cannot be rolled back by
image alone should say so in their notes.

## Gating

A release build should not be the first time anything is checked. Both
projects run `ci.yml` and `openapi-drift.yml` on pull requests and on pushes
to `main` and `release/**`:

| | odo | Current |
| --- | --- | --- |
| Rust | clippy `-D warnings` + `cargo test`, all eight crates, plus odo-auth with `--features saml` | same, one crate |
| UI | core typecheck/lint/build, then admin tests + build | `tsc --noEmit`, `vitest` |
| Specs | `generate-openapi.sh --check` | same |

Integration, e2e, pgTAP and load suites need a live cluster and stay a runbook
step rather than a CI job.

Two ordering constraints are baked into odo's job and are easy to trip over
locally too: `src/ui/core` must be built before the admin UI will build
(`@odo/core` resolves to `./dist`), and the `saml` feature needs xmlsec1,
libxml2 and clang installed.

## Open items

* Re-pointing production at `ghcr.io/kcls/<repo>/<service>`; until then it
  runs images from the frozen sjora-owned namespace.
* Current's `odo-client`/`odo-service` pin moving from `main` to an odo
  release tag (blocked on odo having one).
* Secrets in the deployment repository: the predecessor's committed secrets
  are stubs (empty JWT/SMTP/API values, `demo123` against an in-cluster
  PostgreSQL that no longer exists), so real values are injected out of band.
  Whatever replaces that — SOPS, sealed-secrets, or staying out of band —
  should be a stated decision rather than the status quo by default.
* Branch protection on `main` and `release/**` in both public repositories.
* A CHANGELOG and GitHub Releases per project — adopters now need to know what
  changed between tags.
