let configuredHostPort: string | null = null;

/**
 * Configure the API host:port at runtime.
 */
export function configureApiHostPort(hostport: string): void {
    configuredHostPort = hostport;
}

/**
 * Get the API host:port for building API URLs.
 */
export function getApiHostPort(): string {
    if (configuredHostPort) return configuredHostPort;

    // @ts-ignore - Vite
    const viteHost = typeof import.meta !== 'undefined' && import.meta.env?.VITE_API_HOSTPORT;
    if (viteHost) return viteHost;

    return window.location.host;
}

/**
 * Get the full API base URL (e.g., "http://localhost:30080")
 */
export function getApiBaseUrl(): string {
    return `${window.location.protocol}//${getApiHostPort()}`;
}
