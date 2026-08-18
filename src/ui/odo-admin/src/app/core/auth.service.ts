import { Injectable, computed, inject, signal } from '@angular/core';
import { Router } from '@angular/router';
import {
  authApi,
  samlApi,
  configureTokenRefresh,
  type SamlSSOConfig,
  type User,
} from '@odo/core';

/**
 * Signal-based wrapper around the shared @odo/core auth client.
 * The rest of the app talks to this service only; @odo/core stays an
 * implementation detail.
 */
@Injectable({ providedIn: 'root' })
export class AuthService {
  private readonly router = inject(Router);

  readonly user = signal<User | null>(null);
  readonly isAuthenticated = computed(() => this.user() !== null);

  private permCache = new Map<string, Promise<boolean>>();

  /** Called once at app startup (provideAppInitializer). */
  async init(): Promise<void> {
    configureTokenRefresh({
      getAccessTokenExpirationTime: () => authApi.getAccessTokenExpirationTime(),
      refreshToken: () => authApi.refreshToken(undefined, { silent: true }),
    });

    // Session expiry shows the "your session expired" notice; a deliberate
    // logout must not (it would be misleading).
    authApi.sessionExpired$.subscribe(() => this.onSessionEnded(true));
    authApi.userLoggedOut$.subscribe(() => this.onSessionEnded(false));

    // Restore a session from the HttpOnly refresh cookie, if present.
    try {
      await authApi.refreshToken(undefined, { silent: true });
      await this.fetchCurrentUser();
    } catch {
      // Not logged in; the auth guard will route to /login.
    }
  }

  async login(username: string, password: string): Promise<void> {
    const response = await authApi.loginLocal({ username, password });
    this.permCache.clear();
    this.user.set(response.user ?? null);
  }

  async logout(): Promise<void> {
    await authApi.logout(); // fires userLoggedOut$ -> onSessionEnded()
  }

  async fetchCurrentUser(): Promise<void> {
    const user = await authApi.me();
    this.permCache.clear();
    this.user.set(user && user.id !== 0 ? user : null);
  }

  /** Permission check via odo-auth; cached per session. */
  hasPerm(perm: string): Promise<boolean> {
    if (!this.isAuthenticated()) return Promise.resolve(false);
    let cached = this.permCache.get(perm);
    if (!cached) {
      cached = authApi.userHasPerm(perm);
      this.permCache.set(perm, cached);
    }
    return cached;
  }

  listSSOConfigs(): Promise<SamlSSOConfig[]> {
    return samlApi.listSSOConfigs(window.location.origin);
  }

  /** Exchange the HttpOnly cookie set by the SAML ACS for an access token. */
  async completeSamlLogin(): Promise<void> {
    await authApi.refreshToken(undefined, { silent: true });
    await this.fetchCurrentUser();
  }

  async startSSOLogin(spId: number, redirectTo?: string | null): Promise<void> {
    // RelayState returns the user to this app's login page after the IdP
    // round-trip; document.baseURI carries the /odo/admin/ base href.
    let relayState = new URL('login', document.baseURI).toString();
    if (redirectTo) {
      relayState += `?redirect_to=${encodeURIComponent(redirectTo)}`;
    }
    const response = await samlApi.initiateSSOLogin({ spId, relayState });
    window.location.href = response.redirect_url;
  }

  /**
   * Clear auth state and return to the login page. `expired` is true only when
   * the session ended involuntarily (token/refresh expiry) — a deliberate
   * logout passes false so the "session expired" notice isn't shown.
   */
  private onSessionEnded(expired: boolean): void {
    const wasAuthenticated = this.user() !== null;
    this.user.set(null);
    this.permCache.clear();
    if (wasAuthenticated) {
      this.router.navigate(['/login'], {
        queryParams: expired ? { session_expired: 1 } : {},
      });
    }
  }
}
