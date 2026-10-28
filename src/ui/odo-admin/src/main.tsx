import { StrictMode } from "react"
import { createRoot } from "react-dom/client"
import { RouterProvider } from "react-router/dom"

import "./index.css"
import { session } from "@/auth/session"
import { ThemeProvider } from "@/components/theme-provider"
import { i18nReady, loadNamespaces } from "@/i18n"
import { router } from "@/router"

// Render once the shell's strings are in and any existing session is
// restored, so nothing flashes raw keys and guards see the final state.
await Promise.all([i18nReady.then(() => loadNamespaces("common")), session.init()])

session.onSessionEnded((expired) => {
  const { pathname, search } = router.state.location
  const params = new URLSearchParams()
  if (expired) {
    params.set("session_expired", "1")
    if (pathname !== "/" && pathname !== "/login") params.set("redirect_to", pathname + search)
  }
  const query = params.toString()
  void router.navigate(query ? `/login?${query}` : "/login")
})

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <ThemeProvider storageKey="odo-admin.theme">
      <RouterProvider router={router} />
    </ThemeProvider>
  </StrictMode>
)
