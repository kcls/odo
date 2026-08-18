# Odobenus (Odo)

## Goals

Kubernetes cluster hosting a mix of applications with common tools for shared
data and actions.

## Common Tools, Data, and Components

* Authentication with SSO (SAML)
* User Roles and Permission APIs
* Notification APIs
* Common data APIs (e.g. organizational data like library branches)

## Installation

### Ubuntu (k3s)

Run from the project root directory.

The installer adds the current user ($USER) to the k3s group to allow access
to `kubectl` for managing the cluster.

#### Install Dev Cluster

Install k3s, Docker, dev tools, and initialize the cluster.

```bash
./scripts/setup/k3s/install-dev-cluster-ubuntu.sh
```

> [!NOTE]
> Log out and back in to activate k3s and docker group memberships.

#### Optional: Install test dependencies and run tests

```bash
./scripts/setup/k3s/install-test-dependencies-ubuntu.sh
./scripts/run-tests.sh --db --integration --e2e
```

### Mac (Docker Desktop)

#### Prerequisites

* Install [Docker Desktop](https://www.docker.com/products/docker-desktop/)
* Enable Kubernetes in the Docker Desktop settings.
  * Choose 'Kubeadm' as the 'Cluster provisioning method'
* Install [Homebrew](https://brew.sh).

#### Install Dev Cluster

```bash
./scripts/setup/docker-desktop/install-dev-cluster-mac.sh
```

#### Optional: Install test dependencies and run tests

```bash
./scripts/setup/docker-desktop/install-test-dependencies-mac.sh
./scripts/run-tests.sh --db --integration --e2e
```

## Applications

Applications build atop Odo by consuming the odo-* HTTP APIs and the
`odo-client` crate, running against their own databases, and owning their
own gateway routes and JWT policy — see `docs/app-repo-structure.md`.

The reference application is `Current`, an incident tracker for community
libraries (the seed project from which Odo sprung), maintained at
https://github.com/kcls/current.
