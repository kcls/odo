#!/bin/bash
# Script to manage Kubernetes secrets for the Odo platform
# Supports create, show, and update operations

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Default values
COMMAND=${1:-show}
POSTGRES_NAMESPACE=${POSTGRES_NAMESPACE:-odo-core}

# Function to show usage
usage() {
    echo -e "${BLUE}Usage: $0 [command] [options]${NC}"
    echo
    echo "Commands:"
    echo "  show-secret              Show current secret values (default)"
    echo "  show-namespace-secrets <ns>  Show all secrets in a namespace, values decoded"
    echo "  update-db-url [url]  Set the in-cluster database connection from a"
    echo "                  single URL (derives the POSTGRES_* fields the"
    echo "                  statefulset consumes; refreshes the external URL creds)"
    echo "  update-external-db-url [url]  Set EXTERNAL_DATABASE_URL (the"
    echo "                  host-reachable endpoint dev tooling resolves from"
    echo "                  the secret); prompts when the url is omitted"
    echo "  update-jwt      Update JWT secret"
    echo "  update-ghcr     Create or update GitHub Container Registry secret"
    echo "  update-smtp     Update SMTP notification settings"
    echo "  update-notify-service  Update the notify service account password"
    exit 1
}

# Function to generate random secret
generate_secret() {
    openssl rand -base64 32
}

# Function to generate hex secret (no special chars)
generate_hex_secret() {
    openssl rand -hex 32
}


# Function to create or update a secret
create_secret() {
    local namespace=$1
    local secret_name=$2
    

    echo -e "${GREEN}Creating secret $secret_name in namespace $namespace${NC}"
}

# Function to check if secret exists
secret_exists() {
    local namespace=$1
    local secret_name=$2
    kubectl get secret $secret_name -n $namespace &>/dev/null
}

# Function to get secret value
get_secret_value() {
    local namespace=$1
    local secret_name=$2
    local key=$3
    kubectl get secret $secret_name -n $namespace -o jsonpath="{.data.$key}" 2>/dev/null | base64 -d
}

# Function to show all secrets in a namespace, with values decoded.
# Service-account token secrets are auto-managed by Kubernetes and skipped.
# Output is plain (no color codes) so it can be captured/redirected cleanly.
show_namespace_secrets() {
    local namespace="$1"

    if [ -z "$namespace" ]; then
        echo "Error: namespace required."
        echo "Usage: $0 show-namespace-secrets <namespace>"
        exit 1
    fi

    if ! kubectl get namespace "$namespace" &>/dev/null; then
        echo "Error: Namespace '$namespace' not found"
        exit 1
    fi

    local names
    names=$(kubectl get secrets -n "$namespace" \
        --field-selector "type!=kubernetes.io/service-account-token" \
        -o jsonpath='{range .items[*]}{.metadata.name}{"\n"}{end}')

    if [ -z "$names" ]; then
        echo "No secrets in namespace '$namespace'."
        return
    fi

    local secret_name
    for secret_name in $names; do
        echo "=== ${namespace}/${secret_name} ==="
        # Decode every key in this secret.
        local keys
        keys=$(kubectl get secret "$secret_name" -n "$namespace" -o jsonpath='{.data}' | jq -r 'keys[]' 2>/dev/null)
        local key
        for key in $keys; do
            echo "  ${key}: $(get_secret_value "$namespace" "$secret_name" "$key")"
        done
        echo
    done
}

show_secret() {
    echo -e "${BLUE}Show Kubernetes Secret${NC}\n"

    # List available namespaces with secrets
    echo -e "${YELLOW}Available namespaces:${NC}"
    NAMESPACES=$(kubectl get namespaces -o jsonpath='{.items[*].metadata.name}' | tr ' ' '\n' | grep -E '^odo-|argocd' | sort)
    for ns in $NAMESPACES; do
        SECRET_COUNT=$(kubectl get secrets -n "$ns" --no-headers 2>/dev/null | wc -l)
        echo "  - $ns ($SECRET_COUNT secrets)"
    done
    echo

    # Get namespace from user
    read -p "Enter namespace (or press enter for odo-core): " NAMESPACE
    NAMESPACE=${NAMESPACE:-odo-core}

    # Validate namespace exists
    if ! kubectl get namespace "$NAMESPACE" &>/dev/null; then
        echo -e "${RED}Error: Namespace '$NAMESPACE' not found${NC}"
        exit 1
    fi

    # List secrets in the namespace
    echo -e "\n${YELLOW}Secrets in namespace '$NAMESPACE':${NC}"
    kubectl get secrets -n "$NAMESPACE" --no-headers | awk '{print "  - " $1 " (Type: " $2 ")"}'
    echo

    # Get secret name from user
    read -p "Enter secret name: " SECRET_NAME

    if [ -z "$SECRET_NAME" ]; then
        echo -e "${RED}Error: Secret name cannot be empty${NC}"
        exit 1
    fi

    # Check if secret exists
    if ! kubectl get secret "$SECRET_NAME" -n "$NAMESPACE" &>/dev/null; then
        echo -e "${RED}Error: Secret '$SECRET_NAME' not found in namespace '$NAMESPACE'${NC}"
        exit 1
    fi

    # Get list of keys in the secret
    echo -e "\n${YELLOW}Keys available in secret '$SECRET_NAME':${NC}"
    KEYS=$(kubectl get secret "$SECRET_NAME" -n "$NAMESPACE" -o jsonpath='{.data}' | jq -r 'keys[]' 2>/dev/null)

    if [ -z "$KEYS" ]; then
        echo -e "${RED}No data keys found in secret${NC}"
        exit 1
    fi

    for key in $KEYS; do
        echo "  - $key"
    done
    echo

    # Ask if user wants a specific key or all
    read -p "Enter specific key (or press enter to show all): " KEY

    echo
    echo -e "${BLUE}═══════════════════════════════════════════${NC}"
    echo -e "${BLUE}Secret: $SECRET_NAME${NC}"
    echo -e "${BLUE}Namespace: $NAMESPACE${NC}"
    echo -e "${BLUE}═══════════════════════════════════════════${NC}"
    echo

    if [ -n "$KEY" ]; then
        # Show specific key
        VALUE=$(kubectl get secret "$SECRET_NAME" -n "$NAMESPACE" -o jsonpath="{.data.$KEY}" 2>/dev/null)

        if [ -z "$VALUE" ]; then
            echo -e "${RED}Error: Key '$KEY' not found in secret${NC}"
            exit 1
        fi

        DECODED_VALUE=$(echo "$VALUE" | base64 -d)
        echo -e "${GREEN}$KEY:${NC} $DECODED_VALUE"
    else
        # Show all keys
        echo -e "${YELLOW}Data:${NC}"

        for key in $KEYS; do
            VALUE=$(kubectl get secret "$SECRET_NAME" -n "$NAMESPACE" -o jsonpath="{.data.$key}" | base64 -d)
            echo -e "  ${GREEN}$key:${NC} $VALUE"
        done

        # Show metadata
        echo
        echo -e "${YELLOW}Metadata:${NC}"
        CREATION_TIME=$(kubectl get secret "$SECRET_NAME" -n "$NAMESPACE" -o jsonpath='{.metadata.creationTimestamp}')
        TYPE=$(kubectl get secret "$SECRET_NAME" -n "$NAMESPACE" -o jsonpath='{.type}')
        echo -e "  ${GREEN}Type:${NC} $TYPE"
        echo -e "  ${GREEN}Created:${NC} $CREATION_TIME"

        # Show labels if any
        LABELS=$(kubectl get secret "$SECRET_NAME" -n "$NAMESPACE" -o jsonpath='{.metadata.labels}')
        if [ "$LABELS" != "{}" ] && [ -n "$LABELS" ]; then
            echo -e "  ${GREEN}Labels:${NC}"
            echo "$LABELS" | jq -r 'to_entries[] | "    \(.key): \(.value)"'
        fi
    fi

    echo
}

# Function to update PostgreSQL connection details for all services.
# Prompts once, then updates:
#   odo-core  → individual fields (POSTGRES_HOST, etc.)
#   odo-core    → single DATABASE_URL
#   odo-pub     → single DATABASE_URL
# Set the database connection from a single URL. Writes the in-cluster
# DATABASE_URL verbatim (odo-core + odo-pub), derives the POSTGRES_*
# fields the postgres statefulset consumes (odo-core), and refreshes
# EXTERNAL_DATABASE_URL with the new credentials (keeping its own
# host:port endpoint). All merge patches - nothing else in the secrets
# is disturbed.
update_db_url() {
    echo -e "${YELLOW}Updating DATABASE_URL (in-cluster)${NC}"

    local new_url="$1"
    if [ -z "$new_url" ]; then
        local current
        current=$(get_secret_value $POSTGRES_NAMESPACE postgres-credentials DATABASE_URL 2>/dev/null)
        current=${current:-postgres://odo:demo123@postgres.odo-core.svc.cluster.local:5432/odo?sslmode=disable}
        read -p "Database URL [$current]: " new_url
        new_url=${new_url:-$current}
    fi
    if [ -z "$new_url" ]; then
        echo -e "${RED}No URL provided${NC}"
        exit 1
    fi
    case "$new_url" in
        postgres://*|postgresql://*) ;;
        *)
            echo -e "${RED}URL must start with postgres:// or postgresql://${NC}"
            exit 1
            ;;
    esac

    # Parse postgres://user:pass@host:port/db?params
    local rest="${new_url#*://}"
    local userinfo="${rest%%@*}"
    local hostpart="${rest#*@}"
    local pg_user="${userinfo%%:*}"
    local pg_password="${userinfo#*:}"
    local hostport="${hostpart%%/*}"
    local pg_host="${hostport%%:*}"
    local pg_port="${hostport#*:}"
    local dbpart="${hostpart#*/}"
    local pg_db="${dbpart%%\?*}"

    if [ -z "$pg_user" ] || [ "$pg_user" = "$pg_password" ] || [ -z "$pg_host" ] || [ -z "$pg_db" ]; then
        echo -e "${RED}Could not parse user:password@host:port/db out of the URL${NC}"
        exit 1
    fi
    [ "$pg_port" = "$pg_host" ] && pg_port="5432"

    # Keep the external endpoint (host:port) but refresh its creds/db.
    local ext_url ext_hostport
    ext_url=$(get_secret_value $POSTGRES_NAMESPACE postgres-credentials EXTERNAL_DATABASE_URL 2>/dev/null)
    if [ -n "$ext_url" ]; then
        ext_hostport="${ext_url#*://}"; ext_hostport="${ext_hostport#*@}"
        ext_hostport="${ext_hostport%%/*}"
    else
        ext_hostport="localhost:5432"
    fi
    local external_url="postgres://${pg_user}:${pg_password}@${ext_hostport}/${pg_db}?sslmode=disable"

    # odo-core also carries the POSTGRES_* fields the postgres
    # statefulset consumes at pod start.
    kubectl -n "$POSTGRES_NAMESPACE" patch secret postgres-credentials --type=merge -p "{\"stringData\":{
        \"POSTGRES_HOST\": \"$pg_host\",
        \"POSTGRES_PORT\": \"$pg_port\",
        \"POSTGRES_DB\": \"$pg_db\",
        \"POSTGRES_USER\": \"$pg_user\",
        \"POSTGRES_PASSWORD\": \"$pg_password\"}}"
    echo -e "${GREEN}  ✓ postgres-credentials → ${POSTGRES_NAMESPACE} (POSTGRES_* fields)${NC}"

    for ns in odo-core odo-pub; do
        if kubectl get secret postgres-credentials -n "$ns" > /dev/null 2>&1; then
            kubectl -n "$ns" patch secret postgres-credentials --type=merge -p "{\"stringData\":{
                \"DATABASE_URL\": \"$new_url\",
                \"EXTERNAL_DATABASE_URL\": \"$external_url\"}}"
            echo -e "${GREEN}  ✓ postgres-credentials → ${ns} (DATABASE_URL + EXTERNAL_DATABASE_URL)${NC}"
        fi
    done

    echo -e "\n${YELLOW}Updated values:${NC}"
    echo "  URL:      postgres://${pg_user}:****@${pg_host}:${pg_port}/${pg_db}"
    echo "  External: postgres://${pg_user}:****@${ext_hostport}/${pg_db}"
    echo -e "\n${YELLOW}Restart the services (and the postgres statefulset if its${NC}"
    echo -e "${YELLOW}credentials changed) to pick this up.${NC}"
}

# Set EXTERNAL_DATABASE_URL: the host-reachable endpoint dev tooling
# (manage-database.sh, test runners) resolves from the secret. The
# in-cluster DATABASE_URL is untouched. Patched (merge), so nothing else
# in the secret is disturbed.
update_external_db_url() {
    echo -e "${YELLOW}Updating EXTERNAL_DATABASE_URL${NC}"

    local new_url="$1"
    if [ -z "$new_url" ]; then
        local current
        current=$(get_secret_value $POSTGRES_NAMESPACE postgres-credentials EXTERNAL_DATABASE_URL 2>/dev/null)
        if [ -z "$current" ]; then
            # Suggest the in-cluster URL with the host swapped to localhost.
            local in_cluster rest
            in_cluster=$(get_secret_value $POSTGRES_NAMESPACE postgres-credentials DATABASE_URL 2>/dev/null)
            if [ -n "$in_cluster" ]; then
                rest="${in_cluster#*://}"
                local userinfo="${rest%%@*}" tail="${rest#*@}" dbpart="${tail#*/}"
                current="postgres://${userinfo}@localhost:5432/${dbpart}"
            fi
        fi
        read -p "External database URL [$current]: " new_url
        new_url=${new_url:-$current}
    fi
    if [ -z "$new_url" ]; then
        echo -e "${RED}No URL provided${NC}"
        exit 1
    fi
    case "$new_url" in
        postgres://*|postgresql://*) ;;
        *)
            echo -e "${RED}URL must start with postgres:// or postgresql://${NC}"
            exit 1
            ;;
    esac

    for ns in odo-core odo-pub; do
        if kubectl get secret postgres-credentials -n "$ns" > /dev/null 2>&1; then
            kubectl -n "$ns" patch secret postgres-credentials --type=merge \
                -p "{\"stringData\":{\"EXTERNAL_DATABASE_URL\":\"$new_url\"}}"
            echo -e "${GREEN}  ✓ postgres-credentials → ${ns} (EXTERNAL_DATABASE_URL)${NC}"
        fi
    done

    echo -e "\n${BLUE}Dev tooling (manage-database.sh, run-tests.sh) resolves this${NC}"
    echo -e "${BLUE}endpoint automatically; no pod restarts needed.${NC}"
}

# Function to update JWT secrets
#   HS256 → auth-jwt-secret in odo-pub
#   RS256 → odo-jwt-secret in odo-pub
update_jwt_secret() {
    echo -e "${YELLOW}Updating JWT secrets${NC}"

    # --- HS256 (legacy auth) ---
    NEW_SECRET=$(generate_hex_secret)
    kubectl create secret generic auth-jwt-secret \
        --namespace=odo-pub \
        --from-literal=JWT_SECRET_KEY="$NEW_SECRET" \
        --from-literal=JWT_ALGORITHM="HS256" \
        --dry-run=client -o yaml | kubectl apply -f -
    echo -e "${GREEN}  ✓ auth-jwt-secret → odo-pub (HS256)${NC}"

    # --- RS256 (odo services) ---
    TMPDIR=$(mktemp -d)
    trap "rm -rf $TMPDIR" EXIT

    echo -e "${BLUE}Generating RSA 2048-bit key pair...${NC}"
    openssl genrsa -out "$TMPDIR/jwt-private.pem" 2048 2>/dev/null
    openssl rsa -in "$TMPDIR/jwt-private.pem" -pubout -out "$TMPDIR/jwt-public.pem" 2>/dev/null

    kubectl create secret generic odo-jwt-secret \
        --namespace=odo-pub \
        --from-file=JWT_PRIVATE_KEY="$TMPDIR/jwt-private.pem" \
        --from-file=JWT_PUBLIC_KEY="$TMPDIR/jwt-public.pem" \
        --dry-run=client -o yaml | kubectl apply -f -
    echo -e "${GREEN}  ✓ odo-jwt-secret → odo-pub (RS256)${NC}"

    echo -e "\n${YELLOW}Remember to:${NC}"
    echo "1. Restart all service pods to pick up the new secrets"
    echo "2. All existing tokens will become invalid"
}

# Function to create GitHub Container Registry secret
create_ghcr_secret() {
    echo -e "${YELLOW}Creating GitHub Container Registry Secret${NC}\n"
    
    # Get GitHub username
    read -p "Enter GitHub username: " GITHUB_USERNAME
    if [ -z "$GITHUB_USERNAME" ]; then
        echo -e "${RED}GitHub username cannot be empty${NC}"
        exit 1
    fi
    
    # Get GitHub PAT
    echo -e "${BLUE}Enter GitHub Personal Access Token (PAT)${NC}"
    echo "The PAT needs the 'read:packages' scope for pulling images"
    echo "For pushing images, it also needs 'write:packages' scope"
    read -s -p "GitHub PAT: " GITHUB_PAT
    echo
    
    if [ -z "$GITHUB_PAT" ]; then
        echo -e "${RED}GitHub PAT cannot be empty${NC}"
        exit 1
    fi
    
    # Ask which namespaces to create the secret in
    echo -e "\n${BLUE}Select namespaces to create the secret in:${NC}"
    echo "1. All namespaces (recommended for pull secrets)"
    echo "2. Specific namespaces"
    echo "3. Single namespace"
    read -p "Choose an option (1-3): " namespace_choice
    
    case $namespace_choice in
        1)
            # Create in all odo namespaces
            NAMESPACES=$(kubectl get namespaces -o jsonpath='{.items[*].metadata.name}' | tr ' ' '\n' | grep -E '^odo-' | sort)
            ;;
        2)
            # Select specific namespaces
            echo -e "\n${BLUE}Available namespaces:${NC}"
            echo "  odo-core"
            echo "  odo-pub"
            echo "  odo-monitoring"
            read -p "Enter namespaces (space-separated): " NAMESPACES
            ;;
        3)
            # Single namespace
            read -p "Enter namespace: " NAMESPACES
            ;;
        *)
            echo -e "${RED}Invalid option${NC}"
            exit 1
            ;;
    esac
    
    # Ask for secret name
    read -p "Enter secret name [ghcr-secret]: " SECRET_NAME
    SECRET_NAME=${SECRET_NAME:-ghcr-secret}
    
    # Create the secret in each namespace
    for namespace in $NAMESPACES; do
        echo -e "\n${BLUE}Creating secret in namespace: $namespace${NC}"

        # Ensure namespace exists
    

        # Create the docker-registry secret
        kubectl create secret docker-registry $SECRET_NAME \
            --namespace=$namespace \
            --docker-server=ghcr.io \
            --docker-username="$GITHUB_USERNAME" \
            --docker-password="$GITHUB_PAT" \
            --dry-run=client -o yaml | kubectl apply -f -

        if [ $? -eq 0 ]; then
            echo -e "${GREEN}✓ Secret created in $namespace${NC}"
        else
            echo -e "${RED}✗ Failed to create secret in $namespace${NC}"
        fi
    done

    # Also create in argocd namespace if it exists
    if kubectl get namespace argocd &>/dev/null; then
        echo -e "\n${BLUE}Found argocd namespace, creating secret there as well...${NC}"

        kubectl create secret docker-registry $SECRET_NAME \
            --namespace=argocd \
            --docker-server=ghcr.io \
            --docker-username="$GITHUB_USERNAME" \
            --docker-password="$GITHUB_PAT" \
            --dry-run=client -o yaml | kubectl apply -f -

        if [ $? -eq 0 ]; then
            echo -e "${GREEN}✓ Secret created in argocd namespace${NC}"
        else
            echo -e "${RED}✗ Failed to create secret in argocd namespace${NC}"
        fi
    fi

    # Always propagate to the odo namespaces so odo services can pull images.
    echo -e "\n${BLUE}Propagating secret to odo namespaces...${NC}"
    for namespace in odo-core odo-pub; do
        kubectl create secret docker-registry $SECRET_NAME \
            --namespace=$namespace \
            --docker-server=ghcr.io \
            --docker-username="$GITHUB_USERNAME" \
            --docker-password="$GITHUB_PAT" \
            --dry-run=client -o yaml | kubectl apply -f -

        if [ $? -eq 0 ]; then
            echo -e "${GREEN}✓ Secret created in $namespace${NC}"
        else
            echo -e "${RED}✗ Failed to create secret in $namespace${NC}"
        fi
    done

    echo -e "\n${GREEN}GitHub Container Registry secret created successfully!${NC}"
    echo -e "\n${YELLOW}To use this secret for pulling images:${NC}"
    echo "1. Add to deployment spec:"
    echo "   spec:"
    echo "     imagePullSecrets:"
    echo "     - name: $SECRET_NAME"
    echo ""
    echo "2. Or configure as default for service account:"
    echo "   kubectl patch serviceaccount default -n <namespace> -p '{\"imagePullSecrets\": [{\"name\": \"$SECRET_NAME\"}]}'"
}

# Function to update SMTP notification settings
update_smtp() {
    echo -e "${YELLOW}Updating SMTP notification settings${NC}"

    SMTP_NAMESPACE="odo-pub"
    SMTP_SECRET="notification-smtp"

    if ! secret_exists $SMTP_NAMESPACE $SMTP_SECRET; then
        echo -e "${RED}Error: $SMTP_SECRET secret not found in namespace $SMTP_NAMESPACE${NC}"
        echo "The secret will be created with the values you provide."
        echo
    else
        echo -e "${BLUE}Current SMTP configuration found${NC}"
        echo
    fi

    # Get current values as defaults (if secret exists)
    if secret_exists $SMTP_NAMESPACE $SMTP_SECRET; then
        CURRENT_HOST=$(get_secret_value $SMTP_NAMESPACE $SMTP_SECRET smtp-host)
        CURRENT_PORT=$(get_secret_value $SMTP_NAMESPACE $SMTP_SECRET smtp-port)
        CURRENT_USERNAME=$(get_secret_value $SMTP_NAMESPACE $SMTP_SECRET smtp-username)
        CURRENT_FROM_EMAIL=$(get_secret_value $SMTP_NAMESPACE $SMTP_SECRET smtp-from-email)
        CURRENT_FROM_NAME=$(get_secret_value $SMTP_NAMESPACE $SMTP_SECRET smtp-from-name)
        CURRENT_USE_TLS=$(get_secret_value $SMTP_NAMESPACE $SMTP_SECRET smtp-use-tls)
        CURRENT_USE_STARTTLS=$(get_secret_value $SMTP_NAMESPACE $SMTP_SECRET smtp-use-starttls)
        CURRENT_ACCEPT_INVALID_CERTS=$(get_secret_value $SMTP_NAMESPACE $SMTP_SECRET smtp-dangerous-accept-invalid-certs)
        # Default to false if not set
        CURRENT_ACCEPT_INVALID_CERTS=${CURRENT_ACCEPT_INVALID_CERTS:-false}
    else
        CURRENT_HOST="smtp.example.com"
        CURRENT_PORT="587"
        CURRENT_USERNAME=""
        CURRENT_FROM_EMAIL="noreply@example.com"
        CURRENT_FROM_NAME="Odo Notification Service"
        CURRENT_USE_TLS="false"
        CURRENT_USE_STARTTLS="true"
        CURRENT_ACCEPT_INVALID_CERTS="false"
    fi

    # Prompt for new values
    echo -e "${BLUE}Enter SMTP configuration (press enter to keep current value):${NC}\n"

    read -p "SMTP Host [$CURRENT_HOST]: " NEW_HOST
    NEW_HOST=${NEW_HOST:-$CURRENT_HOST}

    read -p "SMTP Port [$CURRENT_PORT]: " NEW_PORT
    NEW_PORT=${NEW_PORT:-$CURRENT_PORT}

    echo -e "\n${YELLOW}Note: To clear username/password, enter a single dash '-'${NC}"
    read -p "SMTP Username (leave empty for no auth, '-' to clear) [$CURRENT_USERNAME]: " NEW_USERNAME_INPUT

    # Handle special cases for username
    if [ "$NEW_USERNAME_INPUT" == "-" ]; then
        NEW_USERNAME=""
        echo -e "${BLUE}Username will be cleared${NC}"
    elif [ -z "$NEW_USERNAME_INPUT" ]; then
        NEW_USERNAME=$CURRENT_USERNAME
    else
        NEW_USERNAME=$NEW_USERNAME_INPUT
    fi

    if [ -n "$NEW_USERNAME" ]; then
        read -s -p "SMTP Password (press enter to keep current, '-' to clear): " NEW_PASSWORD_INPUT
        echo

        # Handle special cases for password
        if [ "$NEW_PASSWORD_INPUT" == "-" ]; then
            NEW_PASSWORD=""
            echo -e "${BLUE}Password will be cleared${NC}"
        elif [ -z "$NEW_PASSWORD_INPUT" ] && secret_exists $SMTP_NAMESPACE $SMTP_SECRET; then
            NEW_PASSWORD=$(get_secret_value $SMTP_NAMESPACE $SMTP_SECRET smtp-password)
            echo -e "${BLUE}Keeping existing password${NC}"
        else
            NEW_PASSWORD=$NEW_PASSWORD_INPUT
        fi
    else
        NEW_PASSWORD=""
    fi

    read -p "From Email Address [$CURRENT_FROM_EMAIL]: " NEW_FROM_EMAIL
    NEW_FROM_EMAIL=${NEW_FROM_EMAIL:-$CURRENT_FROM_EMAIL}

    read -p "From Name [$CURRENT_FROM_NAME]: " NEW_FROM_NAME
    NEW_FROM_NAME=${NEW_FROM_NAME:-$CURRENT_FROM_NAME}

    echo -e "\n${BLUE}TLS/Security Settings:${NC}"
    read -p "Use TLS (true/false) [$CURRENT_USE_TLS]: " NEW_USE_TLS
    NEW_USE_TLS=${NEW_USE_TLS:-$CURRENT_USE_TLS}

    read -p "Use STARTTLS (true/false) [$CURRENT_USE_STARTTLS]: " NEW_USE_STARTTLS
    NEW_USE_STARTTLS=${NEW_USE_STARTTLS:-$CURRENT_USE_STARTTLS}

    echo -e "\n${YELLOW}Note: Enable this for servers with self-signed or invalid certificates${NC}"
    read -p "Accept Invalid Certificates (true/false) [$CURRENT_ACCEPT_INVALID_CERTS]: " NEW_ACCEPT_INVALID_CERTS
    NEW_ACCEPT_INVALID_CERTS=${NEW_ACCEPT_INVALID_CERTS:-$CURRENT_ACCEPT_INVALID_CERTS}

    # Ensure namespace exists


    # Create or update secret
    kubectl create secret generic $SMTP_SECRET \
        --namespace=$SMTP_NAMESPACE \
        --from-literal=smtp-host="$NEW_HOST" \
        --from-literal=smtp-port="$NEW_PORT" \
        --from-literal=smtp-username="$NEW_USERNAME" \
        --from-literal=smtp-password="$NEW_PASSWORD" \
        --from-literal=smtp-from-email="$NEW_FROM_EMAIL" \
        --from-literal=smtp-from-name="$NEW_FROM_NAME" \
        --from-literal=smtp-use-tls="$NEW_USE_TLS" \
        --from-literal=smtp-use-starttls="$NEW_USE_STARTTLS" \
        --from-literal=smtp-dangerous-accept-invalid-certs="$NEW_ACCEPT_INVALID_CERTS" \
        --dry-run=client -o yaml | kubectl apply -f -

    echo -e "${GREEN}  ✓ $SMTP_SECRET → $SMTP_NAMESPACE${NC}"

    echo -e "\n${YELLOW}Updated configuration:${NC}"
    echo "Host: $NEW_HOST"
    echo "Port: $NEW_PORT"
    echo "Username: $NEW_USERNAME"
    echo "From Email: $NEW_FROM_EMAIL"
    echo "From Name: $NEW_FROM_NAME"
    echo "Use TLS: $NEW_USE_TLS"
    echo "Use STARTTLS: $NEW_USE_STARTTLS"
    echo "Accept Invalid Certs: $NEW_ACCEPT_INVALID_CERTS"

    echo -e "\n${YELLOW}Remember to restart notification service pods:${NC}"
    echo "  kubectl rollout restart deployment/odo-notify -n odo-pub"
}


# Function to update the shared notify service account secret
update_notify_service() {
    echo -e "${YELLOW}Updating notify service-account credentials${NC}"

    local NAMESPACE="odo-pub"
    local SECRET="odo-notify-service-account"
    local USERNAME="odo-notify-service"

    if secret_exists "$NAMESPACE" "$SECRET"; then
        echo -e "${BLUE}Existing $SECRET found${NC}"
        USERNAME=$(get_secret_value "$NAMESPACE" "$SECRET" username)
        USERNAME=${USERNAME:-odo-notify-service}
    fi

    read -p "Service username [$USERNAME]: " NEW_USERNAME
    NEW_USERNAME=${NEW_USERNAME:-$USERNAME}

    read -s -p "Service password (press enter to auto-generate a strong one): " NEW_PASSWORD
    echo
    if [ -z "$NEW_PASSWORD" ]; then
        NEW_PASSWORD=$(generate_hex_secret)
        echo -e "${BLUE}Generated a random password${NC}"
    fi

    kubectl create secret generic "$SECRET" \
        --namespace="$NAMESPACE" \
        --from-literal=username="$NEW_USERNAME" \
        --from-literal=password="$NEW_PASSWORD" \
        --dry-run=client -o yaml | kubectl apply -f -
    echo -e "${GREEN}  ✓ $SECRET → $NAMESPACE${NC}"

    echo -e "\n${YELLOW}IMPORTANT: update the DB password hash to match:${NC}"
    echo
    echo "  UPDATE auth.local_account"
    echo "  SET password_hash = auth.hash_password('$NEW_PASSWORD')"
    echo "  WHERE usr = (SELECT id FROM auth.usr WHERE username = '$NEW_USERNAME');"
    echo
    echo -e "${YELLOW}Then restart any application services that use this secret.${NC}"
}


# Main script logic
case "$COMMAND" in
    show-secret)
        show_secret
        ;;
    show-namespace-secrets)
        show_namespace_secrets "$2"
        ;;
    update-db-url)
        update_db_url "$2"
        ;;
    update-external-db-url)
        update_external_db_url "$2"
        ;;
    update-jwt)
        update_jwt_secret
        ;;
    update-ghcr)
        create_ghcr_secret
        ;;
    update-smtp)
        update_smtp
        ;;
    update-notify-service)
        update_notify_service
        ;;
    -h|--help|help)
        usage
        ;;
    *)
        echo -e "${RED}Error: Unknown command '$COMMAND'${NC}"
        usage
        ;;
esac
