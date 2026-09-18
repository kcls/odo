# Database PGTap Unit Tests

## Basics

* Tests are developed using <https://pgtap.org/> and exectuted via pg\_prove.
* Every test creates the data it needs and completes with a ROLLBACK, leaving
  the database is it was found.

## Setup

pgTAP has two halves: `pg_prove` on this host, and the `pgtap` extension on
the database server. PostgreSQL runs outside the cluster, so the two are
installed by different scripts.

On Ubuntu:

```bash
./scripts/setup/install-postgres-server-ubuntu.sh --with-pgtap  # the extension
./scripts/setup/setup-dev-cluster-ubuntu.sh --with-test-deps    # pg_prove
```

When the database lives on a host the installer never touches, install the
extension there by hand (`postgresql-<major>-pgtap`, or its equivalent).

On macOS, `scripts/setup/install-docker-desktop-cluster-mac.sh
--with-test-deps` installs both.

## Testing

Connection details resolve from the cluster's `postgres-credentials`
secret (PG\* environment variables act as overrides).

```
scripts/run-db-tests.sh

# OR

VERBOSE=1 scripts/run-db-tests.sh
```

