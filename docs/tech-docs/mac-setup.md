# Mac Setup (Docker Desktop)

> [!WARNING]
> **Work in progress.** The macOS installer has had far less exercise than the
> Ubuntu path and needs additional testing. Expect rough edges, and prefer
> Ubuntu + k3s for a cluster anyone else depends on.

Unlike Ubuntu — where setup is split into three scripts, two of which are
usable in production — macOS is a single development-only script.

## Prerequisites

Install these by hand first:

* [Docker Desktop](https://www.docker.com/products/docker-desktop/)
* Kubernetes, enabled in the Docker Desktop settings
  * Choose **Kubeadm** as the *Cluster provisioning method*
* [Homebrew](https://brew.sh)

## Install the cluster

```bash
./scripts/setup/install-docker-desktop-cluster-mac.sh --with-test-deps
```

The script installs the system packages and dev toolchains (sqitch, yq, k9s,
libpq, Node, Rust, sea-orm-cli), PostgreSQL via Homebrew, then initializes the
cluster: Envoy Gateway, namespaces, secrets, the JWT keypair, the database
schema and platform seed, and every service.

`--with-test-deps` also installs the e2e npm packages, the Playwright browsers,
pgTAP, and the e2e fixtures — everything `./scripts/run-tests.sh` needs.

## The database address

PostgreSQL runs **outside** the cluster, and the `postgres-credentials` secret
carries a single `DATABASE_URL` read by the service pods *and* by the host
tooling. It therefore needs an address that works in both places.

On a Mac that means the machine's **LAN address**. `host.docker.internal`
resolves inside a pod but not on the host, and `localhost` inside a pod points
at the pod — neither can serve as the one shared URL. The installer guesses the
address of the interface carrying the default route and asks you to confirm it.

`pg_hba.conf` is opened to Docker Desktop's internal network
(`DOCKER_DESKTOP_CIDR`, default `192.168.65.0/24`) plus that address.

> [!NOTE]
> A DHCP address pins the cluster to that lease. If it changes, re-run
> `./scripts/manage-secrets.sh update-db-url` and update the `odo` block in
> `pg_hba.conf`.

Pin a PostgreSQL major version with `POSTGRES_VERSION`:

```bash
POSTGRES_VERSION=17 ./scripts/setup/install-docker-desktop-cluster-mac.sh
```

See [Advanced PostgreSQL Setup](postgres-setup.md) for the full picture,
including pointing the cluster at a server you already have.

## Known rough edges

* The `pgtap` Homebrew formula pins its own `postgresql@N`. If that differs
  from the version serving this host, the extension lands in the other
  installation and `./scripts/run-db-tests.sh` will not find it.
* `sqitch` occasionally needs relinking against the current Homebrew perl; the
  installer retries a `brew reinstall` once before giving up.
* Whether a pod's connection to the Mac's LAN address arrives from the Docker
  Desktop network or from the address itself depends on how Docker Desktop
  routes it, so `pg_hba.conf` allows both. Verify with
  `./scripts/manage-database.sh status` if connections are refused.
