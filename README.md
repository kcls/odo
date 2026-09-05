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

## Addtional Documentation

* [Release Management](docs/tech-docs/release-management.md)
* [Advanced PostgreSQL Setup](docs/tech-docs/postgres-setup.md)
* [Mac Setup (Docker Desktop)](docs/tech-docs/mac-setup.md)
