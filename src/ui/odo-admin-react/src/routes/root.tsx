import { useTranslation } from "react-i18next"
import { Outlet, useNavigation } from "react-router"

import { DirectionProvider } from "@/components/ui/direction"
import { Toaster } from "@/components/ui/sonner"
import { TooltipProvider } from "@/components/ui/tooltip"

function NavigationProgress() {
  const navigation = useNavigation()
  if (navigation.state === "idle") return null
  return (
    <div aria-hidden="true" className="fixed inset-x-0 top-0 z-50 h-0.5 animate-pulse bg-primary" />
  )
}

/** App-wide providers; the login page and the signed-in shell sit below. */
export function RootLayout() {
  const { i18n } = useTranslation()
  return (
    <DirectionProvider dir={i18n.dir()}>
      <TooltipProvider>
        <NavigationProgress />
        <Outlet />
        <Toaster />
      </TooltipProvider>
    </DirectionProvider>
  )
}
