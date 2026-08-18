# Testing Email with Mailhog

## Run Mailhog via Docker

```
docker run -d -p 1025:1025 -p 8025:8025 --name mailhog-container mailhog/mailhog
```

## Configure the Odo Notification Service

* Set the SMTP host to the k8s host IP and SMTP port to 1025
* Restart notification service.


```
./scripts/manage-secrets.sh update-smtp
./scripts/deploy-service.sh notification
```
