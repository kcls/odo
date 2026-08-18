# Stage 1: Builder
FROM node:24-alpine AS builder

ARG VERSION
ENV APP_VERSION=$VERSION

WORKDIR /app

# Copy ui/core as a local dependency (required by @odo/core)
COPY src/ui/core /core

# Build @odo/core first
WORKDIR /core
RUN npm ci && npm run build

# Switch back to app directory
WORKDIR /app

# Copy package files
COPY src/ui/odo-admin/package*.json ./

# Install all dependencies including devDependencies for build
RUN npm ci

# Copy application source
COPY src/ui/odo-admin/ ./

# Build the Angular application
RUN npm run build

# Stage 2: Production
FROM nginx:alpine

# Copy custom nginx configuration
COPY src/ui/odo-admin/nginx.conf /etc/nginx/conf.d/default.conf

# Copy built Angular app from builder stage
COPY --from=builder /app/dist/odo-admin/browser /usr/share/nginx/html

# Create a non-root user
RUN addgroup -g 1010 -S nginx-user && \
    adduser -S nginx-user -u 1010 -G nginx-user && \
    chown -R nginx-user:nginx-user /usr/share/nginx/html && \
    chown -R nginx-user:nginx-user /var/cache/nginx && \
    chown -R nginx-user:nginx-user /var/log/nginx && \
    chown -R nginx-user:nginx-user /etc/nginx/conf.d && \
    touch /var/run/nginx.pid && \
    chown -R nginx-user:nginx-user /var/run/nginx.pid

# Switch to non-root user
USER nginx-user

EXPOSE 8080

CMD ["nginx", "-g", "daemon off;"]
