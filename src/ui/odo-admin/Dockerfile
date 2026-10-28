# Build context: the repository root (see service-map.yaml).

# Stage 1: build the SPA
# Node 24: React Router 8 needs Node >= 22.22, Vite 8 >= 22.12.
FROM node:24-alpine AS builder

# Substituted with ODO_ADMIN_BASE_PATH when the container starts;
# see docker-entrypoint.d/40-base-path.sh.
ENV VITE_BASE_PATH=/__ODO_ADMIN_BASE_PATH__/

# @odo/core is a local dependency (file:../core) that resolves to its
# built dist/, so build it first, at the same relative location.
WORKDIR /ui/core
COPY src/ui/core/ ./
RUN npm ci && npm run build

WORKDIR /ui/odo-admin
COPY src/ui/odo-admin/package*.json ./
RUN npm ci

COPY src/ui/odo-admin/ ./
RUN npm run build

# Stage 2: serve with nginx
FROM nginx:alpine

# nginx config template and the startup script that renders it and places
# the app at ODO_ADMIN_BASE_PATH (default /odo/admin).
COPY src/ui/odo-admin/nginx.conf.template /etc/nginx/templates-odo-admin/default.conf.template
COPY --chmod=755 src/ui/odo-admin/docker-entrypoint.d/40-base-path.sh /docker-entrypoint.d/40-base-path.sh

# The pristine build stays root-owned; the startup script copies it into
# the served directory and substitutes the base path there.
COPY --from=builder /ui/odo-admin/dist /usr/share/nginx/html-dist

# Run as a non-root user.
RUN addgroup -g 1010 -S nginx-user && \
    adduser -S nginx-user -u 1010 -G nginx-user && \
    chown -R nginx-user:nginx-user /usr/share/nginx/html && \
    chown -R nginx-user:nginx-user /var/cache/nginx && \
    chown -R nginx-user:nginx-user /var/log/nginx && \
    chown -R nginx-user:nginx-user /etc/nginx/conf.d && \
    touch /var/run/nginx.pid && \
    chown -R nginx-user:nginx-user /var/run/nginx.pid

USER nginx-user

EXPOSE 8080

CMD ["nginx", "-g", "daemon off;"]
