import { data, redirect, type MiddlewareFunction } from "react-router"

import { ApiRequestError } from "@/api/client"
import { session } from "@/auth/session"

/** Turn an API 404 into a 404 route error response. */
export async function orNotFound<T>(promise: Promise<T>): Promise<T> {
  try {
    return await promise
  } catch (e) {
    if (e instanceof ApiRequestError && e.status === 404) {
      throw data(e.serverMessage ?? "Not found", { status: 404 })
    }
    throw e
  }
}

export function requireParam(value: string | undefined): string {
  if (value === undefined) throw data("Missing route parameter", { status: 404 })
  return value
}

/** A numeric id route param; anything else is a 404. */
export function requireIdParam(value: string | undefined): number {
  const id = Number(requireParam(value))
  if (!Number.isInteger(id) || id <= 0) throw data("Invalid id", { status: 404 })
  return id
}

/**
 * A URL's pathname relative to the app's base path, the form router
 * paths and redirects use. Request URLs in loaders carry the base.
 */
export function appPath(url: URL): string {
  const base = import.meta.env.BASE_URL.replace(/\/$/, "")
  return url.pathname.startsWith(base) ? url.pathname.slice(base.length) || "/" : url.pathname
}

export function loginRedirect(request: Request, expired = false) {
  const params = new URLSearchParams()
  const url = new URL(request.url)
  const to = appPath(url) + url.search + url.hash
  if (to !== "/") params.set("redirect_to", to)
  if (expired) params.set("session_expired", "1")
  const query = params.toString()
  return redirect(query ? `/login?${query}` : "/login")
}

/**
 * Middleware runs before any loader below it, so a guarded route's
 * loaders never fire API calls the user isn't allowed to make.
 */
export const requireAuth: MiddlewareFunction = ({ request }) => {
  if (!session.isAuthenticated()) throw loginRedirect(request)
}

/** Signed-in users without the perm land on the home page. */
export function requirePerm(perm: string): MiddlewareFunction {
  return async ({ request }) => {
    if (!session.isAuthenticated()) throw loginRedirect(request)
    if (!(await session.hasPerm(perm))) throw redirect("/")
  }
}

/**
 * Only accept same-app paths from `redirect_to`, so a crafted login link
 * cannot bounce the user to another site after signing in.
 */
export function safeRedirectTarget(value: string | null): string {
  return value && value.startsWith("/") && !value.startsWith("//") && !value.startsWith("/\\") ? value : "/"
}
