/** The user page's tabs, in display order. Overview is the default. */
export const USER_TABS = ["overview", "permissions", "sessions", "saml"] as const
export type UserTab = (typeof USER_TABS)[number]

/**
 * The tab named by `?tab=`, if it's one this user's page shows; anything
 * else (absent, unknown, or a tab hidden for this user) is the overview.
 */
export function readTab(params: URLSearchParams, shown: readonly UserTab[]): UserTab {
  const tab = params.get("tab")
  return shown.find((t) => t === tab) ?? "overview"
}

/** Search params for a tab, leaving the default out of the URL. */
export function tabParams(params: URLSearchParams, tab: UserTab): URLSearchParams {
  const next = new URLSearchParams(params)
  if (tab === "overview") next.delete("tab")
  else next.set("tab", tab)
  return next
}
