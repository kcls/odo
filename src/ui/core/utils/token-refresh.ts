/**
 * Token Refresh Utility
 *
 * Provides a reusable mechanism for token refresh that can be used
 * by different API clients (WebSocket, REST/axios, fetch, etc.)
 *
 * Usage:
 *   // For REST/axios interceptor:
 *   axios.interceptors.request.use(async (config) => {
 *     await ensureTokenFresh();
 *     config.headers.Authorization = `Bearer ${authApi.getAccessToken()}`;
 *     return config;
 *   });
 *
 *   // For fetch wrapper:
 *   async function apiFetch(url, options) {
 *     await ensureTokenFresh();
 *     return fetch(url, { ...options, headers: { ...options.headers, Authorization: `Bearer ${authApi.getAccessToken()}` } });
 *   }
 */

export interface TokenRefreshConfig {
  /** Get the access token expiration time in milliseconds */
  getAccessTokenExpirationTime: () => number | null;
  /** Refresh the access token using the refresh token */
  refreshToken: () => Promise<any>;
}

let refreshPromise: Promise<void> | null = null;
let config: TokenRefreshConfig | null = null;

/**
 * Configure the token refresh utility.
 * Must be called once during app initialization.
 *
 * @example
 * // In your app's entry point:
 * import { authApi } from '@odo/core';
 * import { configureTokenRefresh } from '@odo/core';
 *
 * configureTokenRefresh({
 *   getAccessTokenExpirationTime: () => authApi.getAccessTokenExpirationTime(),
 *   refreshToken: () => authApi.refreshToken(),
 * });
 */
export function configureTokenRefresh(cfg: TokenRefreshConfig): void {
  config = cfg;
}

/**
 * Ensure the access token is fresh before making an API request.
 * If the token is expired, it will be refreshed automatically using the refresh token.
 *
 * This function is safe to call multiple times concurrently - only one
 * refresh will be performed at a time.
 *
 * @returns Promise that resolves when token is ready (refreshed if needed)
 */
export async function ensureTokenFresh(): Promise<void> {
  if (!config) {
    // Not configured - silently skip (for apps that don't need token refresh)
    return;
  }

  // If a refresh is already in progress, wait for it
  if (refreshPromise) {
    await refreshPromise;
    return;
  }

  try {
    const expiresAt = config.getAccessTokenExpirationTime();
    if (!expiresAt) return;

    const now = Date.now();

    if (now >= expiresAt) {
      console.debug('Access token expired, refreshing...');

      // Create singleton promise to prevent concurrent refreshes
      refreshPromise = config.refreshToken()
        .then(() => {
          console.debug('Token refreshed successfully');
        })
        .finally(() => {
          refreshPromise = null;
        });

      await refreshPromise;
    }
  } catch (error) {
    console.warn('Token refresh failed:', error);
  }
}

/**
 * Check if an access token refresh is needed (without performing it).
 * Useful for debugging or conditional logic.
 */
export function isAccessTokenRefreshNeeded(): boolean {
  if (!config) return false;

  const expiresAt = config.getAccessTokenExpirationTime();
  if (!expiresAt) return false;

  return Date.now() >= expiresAt;
}

/**
 * Get the time remaining until access token expires (in milliseconds).
 * Returns null if no token or cannot be determined.
 */
export function getAccessTokenTimeRemaining(): number | null {
  if (!config) return null;

  const expiresAt = config.getAccessTokenExpirationTime();
  if (!expiresAt) return null;

  return Math.max(0, expiresAt - Date.now());
}
