import { useSyncExternalStore } from "react"
import {
  authApi,
  configureTokenRefresh,
  samlApi,
  type SamlSSOConfig,
  type User,
} from "@odo/core"

/**
 * The signed-in user and their permission checks, wrapping the shared
 * @odo/core auth client. The rest of the app talks to this module only;
 * @odo/core stays an implementation detail.
 */

let currentUser: User | null = null
const listeners = new Set<() => void>()
const permCache = new Map<string, Promise<boolean>>()
let sessionEndedHandler: (expired: boolean) => void = () => {}

function setUser(user: User | null) {
  permCache.clear()
  currentUser = user
  for (const listener of listeners) listener()
}

function subscribe(listener: () => void) {
  listeners.add(listener)
  return () => listeners.delete(listener)
}

/**
 * `expired` is true only when the session ended involuntarily (token or
 * refresh expiry); a deliberate sign-out passes false so the "session
 * expired" notice isn't shown.
 */
function endSession(expired: boolean) {
  const wasSignedIn = currentUser !== null
  setUser(null)
  if (wasSignedIn) sessionEndedHandler(expired)
}

async function fetchCurrentUser() {
  const user = await authApi.me()
  setUser(user && user.id !== 0 ? user : null)
}

export const session = {
  /** Call once before the router starts, so guards see the final state. */
  async init(): Promise<void> {
    configureTokenRefresh({
      getAccessTokenExpirationTime: () => authApi.getAccessTokenExpirationTime(),
      refreshToken: () => authApi.refreshToken(undefined, { silent: true }),
    })
    authApi.sessionExpired$.subscribe(() => endSession(true))
    authApi.userLoggedOut$.subscribe(() => endSession(false))

    // Restore a session from the HttpOnly refresh cookie, if present.
    try {
      await authApi.refreshToken(undefined, { silent: true })
      await fetchCurrentUser()
    } catch {
      // Not signed in; the shell's middleware routes to /login.
    }
  },

  /** Where to go when the session ends; the router registers this. */
  onSessionEnded(handler: (expired: boolean) => void) {
    sessionEndedHandler = handler
  },

  user: () => currentUser,
  isAuthenticated: () => currentUser !== null,

  async login(username: string, password: string): Promise<void> {
    const response = await authApi.loginLocal({ username, password })
    setUser(response.user ?? null)
  },

  /** Fires userLoggedOut$, which ends the session. */
  logout: () => authApi.logout(),

  /** Permission check via odo-auth; cached until the user changes. */
  hasPerm(perm: string): Promise<boolean> {
    if (currentUser === null) return Promise.resolve(false)
    let cached = permCache.get(perm)
    if (!cached) {
      cached = authApi.userHasPerm(perm)
      permCache.set(perm, cached)
    }
    return cached
  },

  listSSOConfigs(): Promise<SamlSSOConfig[]> {
    return samlApi.listSSOConfigs(window.location.origin)
  },

  async startSSOLogin(spId: number, redirectTo?: string | null): Promise<void> {
    // RelayState brings the user back to this app's login page after the
    // IdP round trip; BASE_URL carries the configured base path.
    const relayState = new URL(`${import.meta.env.BASE_URL}login`, window.location.origin)
    if (redirectTo) relayState.searchParams.set("redirect_to", redirectTo)
    const response = await samlApi.initiateSSOLogin({ spId, relayState: relayState.toString() })
    window.location.href = response.redirect_url
  },

  /** Exchange the HttpOnly cookie set by the SAML ACS for an access token. */
  async completeSamlLogin(): Promise<void> {
    await authApi.refreshToken(undefined, { silent: true })
    await fetchCurrentUser()
  },
}

export function useCurrentUser(): User | null {
  return useSyncExternalStore(subscribe, () => currentUser)
}

/** Resolve several permission checks at once, keyed like the input. */
export async function checkPerms<K extends string>(
  perms: Record<K, string>
): Promise<Record<K, boolean>> {
  const keys = Object.keys(perms) as K[]
  const results = await Promise.all(keys.map((k) => session.hasPerm(perms[k])))
  return Object.fromEntries(keys.map((k, i) => [k, results[i]])) as Record<K, boolean>
}
