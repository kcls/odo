import { useTranslation } from "react-i18next"
import { Outlet, useLoaderData } from "react-router"

import { AppSidebar } from "@/components/app-sidebar"
import { Breadcrumbs } from "@/components/breadcrumbs"
import { Separator } from "@/components/ui/separator"
import { SidebarInset, SidebarProvider, SidebarTrigger } from "@/components/ui/sidebar"
import { ADMIN_TOOLS, visibleAdminTools } from "@/lib/admin-tools"

/** Middleware (see router.tsx) has already ensured a signed-in user. */
export async function loader() {
  // Ids, not tool objects: loader data is plain data, and tools hold
  // components and functions.
  return { toolIds: (await visibleAdminTools()).map((tool) => tool.id) }
}

/** The signed-in layout: tool sidebar, breadcrumb bar, routed page. */
export function Component() {
  const { toolIds } = useLoaderData<typeof loader>()
  const tools = ADMIN_TOOLS.filter((tool) => toolIds.includes(tool.id))
  const { t } = useTranslation()
  return (
    <SidebarProvider style={{ "--sidebar-width": "14rem" } as React.CSSProperties}>
      <AppSidebar tools={tools} />
      <SidebarInset>
        <header className="flex h-12 shrink-0 items-center gap-2 border-b px-4">
          <SidebarTrigger className="-ms-1" aria-label={t("nav.toggleSidebar")} />
          <Separator orientation="vertical" className="me-2 data-[orientation=vertical]:h-4" />
          <Breadcrumbs />
        </header>
        <main className="flex min-w-0 flex-1 flex-col p-4 md:p-6">
          <Outlet />
        </main>
      </SidebarInset>
    </SidebarProvider>
  )
}
