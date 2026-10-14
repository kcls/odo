# Odobenus (Odo)

I am the walrus.

## Common Tools, Data, and Components

* User Roles and Permission APIs
* Notification APIs
* Authentication with SSO (SAML)
* Asset storage APIs
* Common data APIs (e.g. organizational data like library branches)

## Applications

The reference application is `Current`, an incident tracker for community
libraries (the seed project from which Odo sprung), maintained at
https://github.com/kcls/current.

Applications build atop Odo by consuming the HTTP APIs
exposed by each Odo service.  OpenAPI specs are available at
[openapi](openapi) and through each API end point, for example,
`/api/v1/odo/auth/api-doc/openapi.json`.

### Building and deploying an application

An application needs no copy of Odo's build and deploy scripts. It
keeps a `service-map.yaml` and `k8s/services/<service>/` in its own
repository and runs the scripts from an Odo checkout:

```bash
cd ~/code/my-app            # any directory inside the app checkout
$ODO_HOME/scripts/build-and-deploy-service.sh ui-my-app
```

`build-service.sh`, `deploy-service.sh`, and `build-and-deploy-service.sh`
act on the nearest `service-map.yaml` at or above the current directory,
falling back to this checkout; set `ODO_PROJECT_ROOT` to choose a project
explicitly. Each run prints the project it resolved. See
[scripts/lib/project.sh](scripts/lib/project.sh).

## Quick Install Guide for Developers

Install PostgreSQL, [K3S](https://k3s.io/), Docker, dev tools, and initialize 
the cluster on Ubuntu.

### 1. Install PosgreSQL

Install and run Postgres on the host, outside of the k8s cluster.

```bash
./scripts/setup/install-postgres-server-ubuntu.sh --with-pgtap
```

### 2. Setup K3S, Docker, and Cluster Infrastructure

This will create a new `k3s` user group and add the current user to
the new group to allow access to `kubectl` for managing the cluster.

```bash
./scripts/setup/install-k3s-cluster-ubuntu.sh
```

> [!NOTE]
> Log out and back in to activate k3s and docker group memberships.

### 3. Dev toolchains, database schema, and all services.

```bash
./scripts/setup/setup-dev-cluster-ubuntu.sh
```

### Run Tests

```bash
./scripts/run-tests.sh --db --integration --e2e # --unit --load
```

### Admin UI

Navigate in your browser to http://DEV-HOST-IP:30080/odo/admin and log in
with `e2e.odo.admin` and defualt password `test123!`.

