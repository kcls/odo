# Database PGTap Unit Tests

## Basics

* Tests are developed using <https://pgtap.org/> and exectuted via pg\_prove.
* Every test creates the data it needs and completes with a ROLLBACK, leaving
  the database is it was found.

## Testing

Connection details resolve from the cluster's `postgres-credentials`
secret (PG\* environment variables act as overrides).

```
scripts/run-db-tests.sh

# OR

VERBOSE=1 scripts/run-db-tests.sh
```

