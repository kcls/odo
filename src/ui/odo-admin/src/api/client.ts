/**
 * Shared fetch helper for odo APIs: token refresh, bearer header, and
 * structured {code, message, field} error parsing.
 */
import { authApi, ensureTokenFresh, getApiBaseUrl } from "@odo/core"

/** Structured backend error: HTTP status plus the {code, message, field} body. */
export class ApiRequestError extends Error {
  readonly status: number
  readonly code?: string
  readonly field?: string
  /** The body's message, when the server sent one. */
  readonly serverMessage?: string

  constructor(status: number, body: { code?: string; message?: string; field?: string } = {}) {
    // describeError() supplies the user-facing text; this is for logs.
    super(body.message || `Request failed (${status})`)
    this.name = "ApiRequestError"
    this.status = status
    this.code = body.code
    this.field = body.field
    this.serverMessage = body.message || undefined
  }
}

async function request<T>(path: string, init: RequestInit = {}): Promise<T> {
  await ensureTokenFresh()
  const token = authApi.getToken()
  const headers = new Headers(init.headers)
  if (token) headers.set("Authorization", `Bearer ${token}`)

  const response = await fetch(`${getApiBaseUrl()}${path}`, { ...init, headers })

  if (!response.ok) {
    if (response.status === 401) authApi.sessionExpired$.next()
    let parsed: { code?: string; message?: string; field?: string } = {}
    try {
      parsed = await response.json()
    } catch {
      // Non-JSON error body; fall through to the generic message.
    }
    throw new ApiRequestError(response.status, parsed)
  }

  return response.json()
}

export function apiGet<T>(path: string): Promise<T> {
  return request(path)
}

export function apiPost<T>(path: string, body: unknown): Promise<T> {
  return request(path, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body ?? {}),
  })
}
