# Runs a debug pod with psql and other network tools.
#
# See also https://hub.docker.com/r/nicolaka/netshoot
#
# Useful for troubleshooting within the pod subnet.
#
# Once started, the user is dropped into the shell on the pod.
#
# Once exited, the pod is deleted.

kubectl run netshoot --rm -it --image nicolaka/netshoot \
    --restart=Never -- sh -c "apk add --no-cache postgresql17-client && /bin/sh"
