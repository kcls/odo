# PostgreSQL for Odo Development

This directory contains Kubernetes manifests for deploying PostgreSQL 17 for development purposes.

## Components

- **StatefulSet**: PostgreSQL 17 (Alpine variant) with persistent storage
- **Service**: ClusterIP service exposing port 5432
- **Secret**: Database credentials (username, password, database name)
- **PersistentVolumeClaim**: 5Gi storage for database data

## Deployment

Deploy to the `odo-core` namespace:

```bash
kubectl apply -k k8s/base/infrastructure/postgres/
```

## Configuration

The PostgreSQL instance is configured with:
- **Database**: odo
- **Username**: odo (superuser)
- **Password**: changeme-dev-password (⚠️ CHANGE IN PRODUCTION!)
- **Port**: 5432

## Accessing PostgreSQL

### From within the cluster
Services can connect using:
- **Host**: `postgres.odo-core.svc.cluster.local`
- **Port**: 5432

### Port forwarding for local development
```bash
kubectl port-forward -n odo-core svc/postgres 5432:5432
```

Then connect locally:
```bash
psql -h localhost -p 5432 -U odo -d odo
```

### Running Sqitch migrations
After the database is running, execute schema migrations:
```bash
# Port forward first
kubectl port-forward -n odo-core svc/postgres 5432:5432 &

# Run sqitch
cd sqitch
sqitch deploy --target db:pg://odo:changeme-dev-password@localhost:5432/odo
```

## Resource Limits

- **Requests**: 256Mi memory, 250m CPU
- **Limits**: 512Mi memory, 500m CPU
- **Storage**: 5Gi persistent volume

## Health Checks

The StatefulSet includes:
- **Liveness probe**: Checks if PostgreSQL is accepting connections
- **Readiness probe**: Ensures database is ready before marking pod as ready

## Security Notes

⚠️ **FOR DEVELOPMENT USE ONLY**

- Change the password in production environments
- Consider using Kubernetes secrets management solutions (e.g., Sealed Secrets, External Secrets Operator)
- Enable SSL/TLS in production
- Implement network policies to restrict access