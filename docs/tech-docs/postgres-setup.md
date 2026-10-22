# PostgreSQL Setup

PostgreSQL runs **outside** the Kubernetes cluster.

## One connection URL

The `postgres-credentials` secret (in `odo-core` and `odo-pub`) carries a
single key:

| Key | Who reads it |
| --- | --- |
| `DATABASE_URL` | the service pods, **and** the host tooling — `manage-database.sh`, `run-tests.sh`, `run-db-tests.sh`, `generate-rust-entities.sh` |

Because both audiences read the same value, the host in it has to resolve and
route the same way from inside a pod and from the host:

* a LAN IP address of the database host, or
* a DNS name that resolves to it in both places.

> [!IMPORTANT]
> `localhost` and `127.0.0.1` are **not** valid. Inside a pod they point at the
> pod. The installers and `manage-secrets.sh update-db-url` reject them.

The tooling needs no `PG*` environment variables — it resolves the endpoint
from the secret. `PG*` variables, when set, override it.

To change the URL later:

```bash
./scripts/manage-secrets.sh update-db-url
```

That patches both namespaces. The host tooling picks it up immediately;
restart the service pods for them to see it.

> [!NOTE]
> A DHCP address works but pins the cluster to that lease. If it changes,
> re-run `update-db-url` and update the `odo` block in `pg_hba.conf` on the
> database host.

Passwords may not contain `@ / ? # %` or whitespace: the URL is parsed by hand
in shell, and those characters would be mangled rather than escaped.

## Installing a server (Ubuntu)

```bash
./scripts/setup/install-postgres-server-ubuntu.sh [--with-pgtap]
```

This script is production-safe — it touches nothing but PostgreSQL. It adds the
[PGDG apt repository](https://www.postgresql.org/download/linux/ubuntu/),
installs the latest stable major, then:

* sets `listen_addresses = '*'` and the port in
  `/etc/postgresql/<major>/main/conf.d/10-odo.conf`
* rewrites a delimited `odo` block in `pg_hba.conf` allowing the client CIDRs
  you confirm (default: the advertised address plus the k3s pod CIDR)
* creates the `odo` role (not a superuser) and the `odo` database it owns,
  over the local peer connection
* verifies the connection over TCP at the advertised address
* records host/port/role/database in `~/.config/odo/setup.env`, so the cluster
  installer can offer them as defaults (never the password)

`--with-pgtap` adds the server-side `pgtap` extension that
`./scripts/run-db-tests.sh` needs, and creates it in the database as
`postgres`: unlike `pg_trgm` and `pgcrypto`, which the schema deploy creates
as the database owner, it is not a trusted extension. Development servers
only.

Pin a major version with `POSTGRES_VERSION`, and override the pod CIDR offered
in the allowed-clients default with `K3S_POD_CIDR`:

```bash
POSTGRES_VERSION=17 ./scripts/setup/install-postgres-server-ubuntu.sh
```

## Using a server you already have

Skip the installer above. `install-k3s-cluster-ubuntu.sh` prompts for the
address, port, role, database, and password, then verifies the connection
before writing the secret. The server needs a login role and a database that
role owns; the role does not need to be a superuser:

```sql
CREATE ROLE odo WITH LOGIN PASSWORD '...';
CREATE DATABASE odo OWNER odo;
-- development servers, for ./scripts/run-db-tests.sh (as a superuser):
\c odo
CREATE EXTENSION IF NOT EXISTS pgtap;
```

`--database-url` skips the prompts:

```bash
./scripts/setup/install-k3s-cluster-ubuntu.sh \
  --database-url postgres://odo:secret@db.example.org:5432/odo
```
