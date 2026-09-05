# Release Management

Status: **in progress**. Phase 1 (this repo's release build) is being
implemented; phases 2–4 are the agreed plan, not yet built.

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
`ghcr.io/<owner>/<service>` on each release build.

| Tag | Published on | Meaning | Moves? |
| --- | --- | --- | --- |
| `<short-sha>` | every build | exactly this commit | never |
| `vX.Y.Z` | tag push only | this release | never |
| `vX.Y` | tag push only | newest patch on the release line | yes |

Pushing a release branch publishes the sha tag only. The line tag moves only
when a release is actually tagged, so `vX.Y` never points at an untagged
commit.

Deployments pin `vX.Y.Z` or a digest. `vX.Y` is a convenience for development
targets that should track a line.

> [!IMPORTANT]
> A published `vX.Y.Z` tag is never re-pointed at a different image. A bad
> release is superseded by `vX.Y.Z+1`, never overwritten — rollback depends on
> the old tag still meaning what it meant.

### Release builds are whole

A release build always builds **every** service, so a version tag names a
coherent set of images. The `[build: …]` commit-message parsing that the old
workflow used survives only as a `workflow_dispatch` input, for re-running a
single service after an infrastructure failure.

### This repository's release workflow

`.github/workflows/release-build.yml`:

| Trigger | Effect |
| --- | --- |
| push to `release/**` | build all services, publish `:<short-sha>` |
| push of tag `v*` | build all services, publish all three tags |
| `workflow_dispatch` | build all services (or a comma-separated subset), publish `:<short-sha>` |

A tag that is not `vX.Y.Z` fails the build rather than publishing something
ambiguous, and an unknown service name in the dispatch input fails rather than
silently building nothing.

There is deliberately no deployment step and no credential for any other
repository.

The workflow carries a **commented-out `release-artifacts` job** that would
attach `k8s/` + `openapi/` to the GitHub release as a tarball. It is parked
pending the decision below — it is one worked-out answer to "how does a
deployment repository obtain manifests at a known version", not the chosen
one.

## Cross-project version coupling

Current consumes `odo-client` and `odo-service` as git dependencies pinned by
its `Cargo.lock`, and installs its platform data through odo's
`load-data-manifest.sh`. A Current release is therefore only meaningful
against some range of odo versions.

Two obligations follow:

* Current pins its git dependency to a **released odo tag**, not to whatever
  is on `main`.
* Each Current release states its odo compatibility range in the release
  notes ("requires odo >= 1.2, < 2.0"). Nothing enforces this at build time,
  so it has to be written down.

## The private deployment repository

One private repository per installation, holding both the Kubernetes manifests
and the site-specific data. Layout:

```
<org>/odo-deploy (private)
  clusters/<env>/          kustomize overlay per target
  services/<svc>/          per-service manifests at a known version
  infrastructure/          gateway, envoy (composed), logging, argocd
  site-data/<env>/
    manifests/*.json       odo-register manifests (org units, users, SAML maps)
    sql/                   sqitch project for what has no API
  versions.yaml            odo: 1.2.3, current: 0.9.1 — per environment
  scripts/bump-images.sh
  runbook.md
```

`versions.yaml` is the single readable answer to "what is deployed here";
`bump-images.sh` (formerly this repo's `scripts/update-image-tags.sh`) writes
those versions into the deployment manifests.

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
deployment repository **composes** them via kustomize, rather than maintaining
a merged copy by hand — however it ends up obtaining them.

### Site data: manifests over SQL

Applications are forbidden from writing SQL against the odo database — they
register what they need through `odo-register` manifests. Site data earns the
same rule: upsert-only semantics, permission checks, and durable uuids, for
free.

Users, roles, grants, SAML maps, notification templates and asset directories
already have manifest support. **Org units do not** — extending odo's
registration surface to cover them is the prerequisite for keeping a site's
local branch structure out of raw SQL.

The `sql/` tree is reserved for what genuinely has no API, and is a sqitch
project so it is versioned and repeatable rather than ad-hoc `psql`.

## Deploying a release

Ordering matters, because ArgoCD auto-sync will roll out an image whenever the
manifest changes — including before anyone has migrated the database.

1. Apply schema migrations from a checkout of the release tag
   (`manage-database.sh deploy`).
2. Apply registration manifests — the platform's own, then each application's
   (`load-data-manifest.sh`). Upsert-only; re-running is safe.
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
projects run, on pull requests to `main` and on release branches:

* `cargo clippy` / `cargo test` per crate (odo-auth additionally with
  `--features saml`)
* `tsc --noEmit` and `vitest` for the UIs
* the OpenAPI drift check

Integration and e2e suites need a live cluster and stay a runbook step rather
than a CI job.

## Open items

* **How a deployment repository obtains `k8s/` and `openapi/` at a known
  version.** Candidates: tarballs attached to the GitHub release (written and
  commented out in the workflow), a kustomize remote base pinned to the release
  tag, a git submodule, or a vendoring script in the deployment repository.
  Undecided; nothing else in this document depends on which one wins.
* Org-unit support in the registration manifest surface (see above).
* Secrets in the deployment repository: verify whether any committed secret
  carries a live credential, and move to SOPS or sealed-secrets before the
  repository's access boundary changes.
* Branch protection on `main` and `release/**` in both public repositories.
* A CHANGELOG and GitHub Releases per project — adopters now need to know what
  changed between tags.
