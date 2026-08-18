/**
 * Shared fetch helper for odo APIs: token refresh, bearer header, and
 * structured {code, message, field} error parsing.
 */
import { authApi, ensureTokenFresh, getApiBaseUrl } from '@odo/core';

/** Structured backend error: HTTP status plus the {code, message, field} body. */
export class ApiRequestError extends Error {
  constructor(
    public readonly status: number,
    public readonly code?: string,
    message?: string,
    public readonly field?: string,
  ) {
    super(message || $localize`Request failed (${status})`);
  }
}

export async function apiGet<T>(path: string): Promise<T> {
  await ensureTokenFresh();
  const token = authApi.getToken();

  const response = await fetch(`${getApiBaseUrl()}${path}`, {
    headers: token ? { Authorization: `Bearer ${token}` } : {},
  });

  if (!response.ok) {
    if (response.status === 401) authApi.sessionExpired$.next();
    let parsed: { code?: string; message?: string; field?: string } = {};
    try {
      parsed = await response.json();
    } catch {
      // Non-JSON error body; fall through to the generic message.
    }
    throw new ApiRequestError(
      response.status,
      parsed.code,
      parsed.message,
      parsed.field,
    );
  }

  return response.json();
}

export async function apiPost<T>(path: string, body: unknown): Promise<T> {
  await ensureTokenFresh();
  const token = authApi.getToken();

  const response = await fetch(`${getApiBaseUrl()}${path}`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      ...(token ? { Authorization: `Bearer ${token}` } : {}),
    },
    body: JSON.stringify(body ?? {}),
  });

  if (!response.ok) {
    if (response.status === 401) authApi.sessionExpired$.next();
    let parsed: { code?: string; message?: string; field?: string } = {};
    try {
      parsed = await response.json();
    } catch {
      // Non-JSON error body; fall through to the generic message.
    }
    throw new ApiRequestError(
      response.status,
      parsed.code,
      parsed.message,
      parsed.field,
    );
  }

  return response.json();
}
