/**
 * Authentication API client
 * Communicates with odo-auth via HTTP/JSON.
 */

import { Subject } from 'rxjs';
import { getApiBaseUrl } from '../utils/api-config';
import { ensureTokenFresh } from '../utils/token-refresh';
import { orgUnitApi } from './org-units';
import type { User, RoleAssignment, LoginRequest as LocalLoginRequest } from '../types';
export type { User, RoleAssignment, LoginRequest as LocalLoginRequest } from '../types';

export interface TokenClaims {
  user_id: number;
  email: string;
  auth_method: string;
  issued_at: number;
  expires_at: number;
  session_id: string;
  org_unit?: number;
  display_name?: string;
}

export interface LoginResponse {
  access_token: string;
  refresh_token?: string;
  token_type: string;
  expires_in: number;
  refresh_expires_at?: number;
  user?: User;
}

export interface ValidateTokenResponse {
  valid: boolean;
  error?: string;
  claims?: TokenClaims;
}

export interface SessionInfo {
  session_id: string;
  created_at: string;
  last_activity_at: string;
  expires_at: string;
  ip_address?: string;
  user_agent?: string;
  is_active: boolean;
}

export interface UserRolesResponse {
  user_id: number;
  roles: RoleAssignment[];
}

let validationPromise: Promise<User> | null = null;
let validationToken: string | null = null;

let accessToken: string | null = null;
let refreshTokenExpiresAt: number | null = null;

function setAccessToken(token: string): void {
  accessToken = token;
}

function clearTokenState(): void {
  accessToken = null;
  refreshTokenExpiresAt = null;
}

let cachedSessionData: {
  org_unit?: number;
  user_id?: number;
  email?: string;
  session_id?: string;
} | null = null;

let cachedRoles: RoleAssignment[] | null = null;
let cachedRolesPromise: Promise<RoleAssignment[]> | null = null;
let cachedRolesGeneration = -1;
let cachedRolesPromiseGeneration = -1;
let rolesCacheGeneration = 0;

function clearRoleCache() {
  rolesCacheGeneration += 1;
  cachedRoles = null;
  cachedRolesPromise = null;
  cachedRolesGeneration = -1;
  cachedRolesPromiseGeneration = -1;
}

function decodeAndCacheToken(token: string): void {
  try {
    const parts = token.split('.');
    if (parts.length !== 3) return;
    const payload = JSON.parse(atob(parts[1]));
    cachedSessionData = {
      org_unit: payload.org_unit,
      user_id: payload.sub ? parseInt(payload.sub, 10) : undefined,
      email: payload.email,
      session_id: payload.session_id
    };
  } catch {
    cachedSessionData = null;
  }
}

/**
 * Authenticated POST to an odo-auth endpoint.
 */
async function authPost<T>(path: string, body?: Record<string, any>): Promise<T> {
  await ensureTokenFresh();
  const token = accessToken;

  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
  };
  if (token) {
    headers['Authorization'] = `Bearer ${token}`;
  }

  const response = await fetch(`${getApiBaseUrl()}/api/v1/odo/auth${path}`, {
    method: 'POST',
    credentials: 'include',
    headers,
    body: JSON.stringify(body ?? {}),
  });

  if (!response.ok) {
    const errorBody = await response.text().catch(() => '');
    if (response.status === 401) authApi.sessionExpired$.next();
    throw new Error(errorBody || `Request failed: ${response.status}`);
  }

  return response.json();
}

export const authApi = {
  userLoggedOut$: new Subject<void>(),
  sessionExpired$: new Subject<void>(),

  async loginLocal(credentials: LocalLoginRequest): Promise<LoginResponse> {
    const response = await fetch(`${getApiBaseUrl()}/api/v1/odo/auth/login`, {
      method: 'POST',
      credentials: 'include',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(credentials),
    });

    if (!response.ok) throw new Error('Auth Failed');

    const data = await response.json() as LoginResponse;
    if (!data || !data.user || !data.access_token) throw new Error('Auth Failed');

    setAccessToken(data.access_token);
    if (data.refresh_expires_at) refreshTokenExpiresAt = data.refresh_expires_at;
    decodeAndCacheToken(data.access_token);
    clearRoleCache();
    data.user.roles = await this.getUserRoles();

    return data;
  },

  async logout(): Promise<void> {
    const token = this.getToken();
    try {
      await fetch(`${getApiBaseUrl()}/api/v1/odo/auth/logout`, {
        method: 'POST',
        credentials: 'include',
        headers: {
          'Content-Type': 'application/json',
          ...(token ? { 'Authorization': `Bearer ${token}` } : {}),
        },
        body: JSON.stringify({ access_token: token }),
      });
    } catch (error) {
      console.warn('Logout API error:', error);
    } finally {
      clearTokenState();
      cachedSessionData = null;
      validationPromise = null;
      validationToken = null;
      clearRoleCache();
      this.userLoggedOut$.next();
    }
  },

  async getCurrentUser(): Promise<User> { return this.me(); },

  async me(): Promise<User> {
    const token = this.getToken();
    if (!token) throw new Error('No authentication token found');

    if (validationPromise && validationToken === token) return validationPromise;

    validationToken = token;
    validationPromise = this._performValidation(token);
    validationPromise.finally(() => {
      setTimeout(() => {
        if (validationToken === token) {
          validationPromise = null;
          validationToken = null;
        }
      }, 100);
    });

    return validationPromise;
  },

  async _performValidation(token: string): Promise<User> {
    await ensureTokenFresh();
    const currentToken = this.getToken() || token;

    const validateData = await authPost<ValidateTokenResponse>(
      '/token/validate',
      { token: currentToken }
    );

    if (!validateData || !validateData.valid) {
      console.debug('Token validation failed');
      this.sessionExpired$.next();
      await this.logout();
      return { id: 0, email: '', username: '' };
    }

    const user: User = {
      id: validateData.claims?.user_id || 0,
      email: validateData.claims?.email || '',
      username: validateData.claims?.email?.split('@')[0] || '',
      display_name: validateData.claims?.display_name,
    };

    user.roles = await this.getUserRoles();
    return user;
  },

  getAccessToken(): string | null { return accessToken; },
  getAuthToken(): string | null { return this.getAccessToken(); },

  setAuthToken(token: string) {
    setAccessToken(token);
    decodeAndCacheToken(token);
    clearRoleCache();
  },

  async refreshToken(context?: Record<string, any>, options?: { silent?: boolean }): Promise<LoginResponse> {
    const silent = options?.silent ?? false;

    if (refreshTokenExpiresAt && Date.now() >= refreshTokenExpiresAt) {
      this.sessionExpired$.next();
      throw new Error('Refresh token expired');
    }

    let response: Response;
    try {
      response = await fetch(`${getApiBaseUrl()}/api/v1/odo/auth/token/refresh`, {
        method: 'POST',
        credentials: 'include',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(context || {}),
      });
    } catch (error) {
      if (!silent) this.sessionExpired$.next();
      throw error;
    }

    if (!response.ok) {
      const deadSession = response.status === 401 && refreshTokenExpiresAt !== null;
      if (!silent || deadSession) this.sessionExpired$.next();
      throw new Error('Token refresh failed');
    }

    const data = await response.json() as LoginResponse;
    if (!data || !data.access_token) {
      if (!silent) this.sessionExpired$.next();
      throw new Error('Failed to refresh token');
    }

    setAccessToken(data.access_token);
    if (data.refresh_expires_at) refreshTokenExpiresAt = data.refresh_expires_at;
    decodeAndCacheToken(data.access_token);
    // Roles are refetched lazily by the next consumer; fetching them here
    // would re-enter ensureTokenFresh() and deadlock on the in-flight
    // refreshPromise singleton.
    clearRoleCache();

    return data;
  },

  async revokeToken(token: string, tokenType: 'access' | 'refresh' = 'access'): Promise<void> {
    const result = await authPost<{ success?: boolean; message?: string }>(
      '/token/revoke',
      { token, token_type: tokenType }
    );
    if (!result || result.success === false) {
      throw new Error(result?.message || 'Failed to revoke token');
    }
  },

  isAuthenticated(): boolean { return !!this.getToken(); },
  getToken(): string | null { return accessToken; },

  setToken(token: string): void {
    setAccessToken(token);
    decodeAndCacheToken(token);
    clearRoleCache();
  },

  setRefreshTokenExpirationTime(expiresAt: number): void {
    refreshTokenExpiresAt = expiresAt;
  },

  clearTokens(): void {
    clearTokenState();
    cachedSessionData = null;
    validationPromise = null;
    validationToken = null;
    clearRoleCache();
  },

  getSessionData(): typeof cachedSessionData {
    if (!cachedSessionData) {
      const token = this.getToken();
      if (token) decodeAndCacheToken(token);
    }
    return cachedSessionData;
  },

  getOrgUnit(): number | undefined {
    return this.getSessionData()?.org_unit;
  },

  getAccessTokenExpirationTime(): number | null {
    const token = this.getToken();
    if (!token) return null;
    try {
      const parts = token.split('.');
      if (parts.length !== 3) return null;
      const payload = JSON.parse(atob(parts[1]));
      return payload.exp ? payload.exp * 1000 : null;
    } catch {
      return null;
    }
  },

  getRefreshTokenExpirationTime(): number | null {
    return refreshTokenExpiresAt;
  },

  async hasAnyRoleAt(roles: string[], orgUnitId: number): Promise<boolean> {
    if (!roles || roles.length === 0) return false;
    try {
      const userRoles = await this.getUserRoles();
      const ancestors = await orgUnitApi.getOrgUnitAncestors(orgUnitId);
      if (!ancestors || ancestors.length === 0) {
        return this._fetchUserHasRole(roles[0], orgUnitId);
      }
      const eligibleOrgUnitIds = new Set(ancestors.map(unit => unit.id));
      return userRoles.some(assignment => {
        if (!assignment || !roles.includes(assignment.role)) return false;
        const assignedOrgUnit = Number(assignment.org_unit);
        if (Number.isNaN(assignedOrgUnit)) return false;
        return eligibleOrgUnitIds.has(assignedOrgUnit);
      });
    } catch {
      return this._fetchUserHasRole(roles[0], orgUnitId);
    }
  },

  async _fetchUserHasRole(role: string, orgUnitId: number): Promise<boolean> {
    try {
      const result = await authPost<{ has_role?: boolean }>(
        '/authz/user-has-role',
        { role, org_unit: orgUnitId }
      );
      return result?.has_role ?? false;
    } catch {
      return false;
    }
  },

  /**
   * Check whether the current user holds a permission. org_unit omitted =
   * checked at the root org unit (matches server-side permission_required).
   */
  async userHasPerm(perm: string, orgUnit?: number): Promise<boolean> {
    try {
      const result = await authPost<{ has_perm?: boolean }>(
        '/authz/user-has-perm',
        { perm, org_unit: orgUnit ?? null }
      );
      return result?.has_perm ?? false;
    } catch {
      return false;
    }
  },

  async getUserRoles(userId?: number): Promise<RoleAssignment[]> {
    if (userId === undefined) {
      if (cachedRoles && cachedRolesGeneration === rolesCacheGeneration) return cachedRoles;
      if (cachedRolesPromise && cachedRolesPromiseGeneration === rolesCacheGeneration) return cachedRolesPromise;

      const generationAtStart = rolesCacheGeneration;
      cachedRolesPromise = this._fetchUserRoles();
      cachedRolesPromiseGeneration = generationAtStart;

      try {
        const roles = await cachedRolesPromise;
        if (generationAtStart === rolesCacheGeneration) {
          cachedRoles = roles;
          cachedRolesGeneration = generationAtStart;
          cachedRolesPromise = null;
          cachedRolesPromiseGeneration = -1;
        }
        return roles;
      } catch (error) {
        if (generationAtStart === rolesCacheGeneration) {
          cachedRolesPromise = null;
          cachedRolesPromiseGeneration = -1;
        }
        throw error;
      }
    }
    return this._fetchUserRoles(userId);
  },

  async _fetchUserRoles(_userId?: number): Promise<RoleAssignment[]> {
    const result = await authPost<UserRolesResponse>('/authz/user-roles', {});
    return result?.roles || [];
  },

  async getUser(params?: { id?: number; options?: { with_working_locations?: boolean } }): Promise<any> {
    return authPost('/user/get', params ?? {});
  },

  async searchUsers(params: Record<string, any>): Promise<any[]> {
    return authPost('/user/search', params);
  },
};
