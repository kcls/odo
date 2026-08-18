# Local Docker Registry for Odo

This directory contains Kubernetes manifests for deploying a local Docker registry for development purposes.

NOTE: this is only needed when deploying to k8s and no local registry is 
available, e.g. microk8s regristry.

## Components

- **Deployment**: Docker Registry v2 container
- **Service**: NodePort service exposing port 5000 on node port 32000
- **PersistentVolumeClaim**: 20Gi storage for Docker images
- **ConfigMap**: Registry configuration with cleanup policies

## Deployment

Deploy to the `odo-core` namespace:

```bash
kubectl apply -k k8s/base/infrastructure/registry/
```

## Configuration

The registry is configured with:
- **Port**: 5000 (internal), 32000 (NodePort)
- **Storage**: 20Gi persistent volume
- **Delete enabled**: Allows image deletion via API
- **Automatic cleanup**: Removes uploads older than 7 days
- **Metrics**: Prometheus metrics on port 5001

## Usage

### Configure Docker to use the registry

For insecure registry access (development only):

1. **Edit Docker daemon configuration**:
   ```bash
   # For Docker Desktop or standard Docker
   sudo nano /etc/docker/daemon.json

   # For MicroK8s
   sudo nano /var/snap/microk8s/current/args/containerd-template.toml
   ```

2. **Add insecure registry** (for Docker):
   ```json
   {
     "insecure-registries": ["localhost:32000"]
   }
   ```

   For MicroK8s, find the `[plugins."io.containerd.grpc.v1.cri".registry.mirrors]` section and add:
   ```toml
   [plugins."io.containerd.grpc.v1.cri".registry.mirrors."localhost:32000"]
     endpoint = ["http://localhost:32000"]
   ```

3. **Restart Docker/MicroK8s**:
   ```bash
   # For Docker
   sudo systemctl restart docker

   # For MicroK8s
   microk8s stop && microk8s start
   ```

### Push an image to the registry

```bash
# Tag your image for the local registry
docker tag myapp:latest localhost:32000/myapp:latest

# Push to registry
docker push localhost:32000/myapp:latest
```

### Pull an image from the registry

```bash
docker pull localhost:32000/myapp:latest
```

### Use in Kubernetes deployments

Reference images in your deployments:
```yaml
apiVersion: apps/v1
kind: Deployment
spec:
  template:
    spec:
      containers:
      - name: myapp
        image: localhost:32000/myapp:latest
        imagePullPolicy: Always
```

### Registry API Operations

#### List all repositories
```bash
curl http://localhost:32000/v2/_catalog
```

#### List tags for a repository
```bash
curl http://localhost:32000/v2/myapp/tags/list
```

#### Delete an image (requires digest)
```bash
# Get digest
digest=$(curl -I -H "Accept: application/vnd.docker.distribution.manifest.v2+json" \
  http://localhost:32000/v2/myapp/manifests/latest 2>/dev/null | grep Docker-Content-Digest | awk '{print $2}' | tr -d '\r')

# Delete by digest
curl -X DELETE http://localhost:32000/v2/myapp/manifests/$digest
```

### Access from within cluster

Services in the cluster can use:
- **Host**: `docker-registry.odo-core.svc.cluster.local:5000`
- **Image**: `docker-registry.odo-core.svc.cluster.local:5000/myapp:latest`

## Monitoring

### Check registry health
```bash
curl http://localhost:32000/v2/
# Should return: {}
```

### View metrics
```bash
kubectl port-forward -n odo-core deployment/docker-registry 5001:5001
curl http://localhost:5001/metrics
```

### Check logs
```bash
kubectl logs -n odo-core deployment/docker-registry
```

## Resource Limits

- **Requests**: 256Mi memory, 200m CPU
- **Limits**: 512Mi memory, 500m CPU
- **Storage**: 20Gi persistent volume

## Security Notes

⚠️ **FOR DEVELOPMENT USE ONLY**

This setup uses an insecure registry without TLS. For production:
- Enable TLS with proper certificates
- Implement authentication (basic auth or token-based)
- Use network policies to restrict access
- Consider using Harbor or other production-grade registry solutions

## Troubleshooting

### Registry not accessible
```bash
# Check if pod is running
kubectl get pods -n odo-core -l app=docker-registry

# Check service endpoints
kubectl get endpoints -n odo-core docker-registry

# Test connectivity
curl -v http://localhost:32000/v2/
```

### Push fails with "server gave HTTP response to HTTPS client"
- Ensure the registry is configured as insecure in Docker daemon
- Restart Docker daemon after configuration changes

### Storage issues
```bash
# Check PVC status
kubectl get pvc -n odo-core registry-pvc

# Check available storage in pod
kubectl exec -n odo-core deployment/docker-registry -- df -h /var/lib/registry
```
