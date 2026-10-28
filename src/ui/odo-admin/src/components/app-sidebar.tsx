import { HomeIcon, ShieldCheckIcon } from "lucide-react"
import { useTranslation } from "react-i18next"
import { NavLink, useMatch } from "react-router"

import { NavUser } from "@/components/nav-user"
import { useDirection } from "@/components/ui/direction"
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupContent,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  useSidebar,
} from "@/components/ui/sidebar"
import type { AdminTool } from "@/lib/admin-tools"

function NavItem({
  to,
  icon,
  label,
  end = false,
}: {
  to: string
  icon: React.ReactNode
  label: string
  end?: boolean
}) {
  const isActive = useMatch({ path: to, end }) !== null
  const { isMobile, setOpenMobile } = useSidebar()
  return (
    <SidebarMenuItem>
      <SidebarMenuButton asChild isActive={isActive} tooltip={label}>
        <NavLink to={to} end={end} onClick={() => isMobile && setOpenMobile(false)}>
          {icon}
          <span>{label}</span>
        </NavLink>
      </SidebarMenuButton>
    </SidebarMenuItem>
  )
}

export function AppSidebar({ tools }: { tools: AdminTool[] }) {
  const { t } = useTranslation()
  // The sidebar positions itself physically, so flip it for RTL.
  const side = useDirection() === "rtl" ? "right" : "left"
  const sorted = tools
    .map((tool) => ({ tool, label: t(`tools.${tool.id}.label`) }))
    .sort((a, b) => a.label.localeCompare(b.label))
  return (
    <Sidebar collapsible="icon" side={side}>
      <SidebarHeader>
        <SidebarMenu>
          <SidebarMenuItem>
            <SidebarMenuButton size="lg" asChild>
              <NavLink to="/">
                <div className="flex aspect-square size-8 items-center justify-center rounded-md bg-sidebar-primary text-sidebar-primary-foreground">
                  <ShieldCheckIcon className="size-4" />
                </div>
                <span className="truncate font-semibold">{t("app.name")}</span>
              </NavLink>
            </SidebarMenuButton>
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarHeader>
      <SidebarContent>
        <SidebarGroup>
          <SidebarGroupContent>
            <SidebarMenu aria-label={t("nav.main")}>
              <NavItem to="/" end icon={<HomeIcon />} label={t("nav.home")} />
              {sorted.map(({ tool, label }) => (
                <NavItem key={tool.id} to={`/${tool.path}`} icon={<tool.icon />} label={label} />
              ))}
            </SidebarMenu>
          </SidebarGroupContent>
        </SidebarGroup>
      </SidebarContent>
      <SidebarFooter>
        <NavUser />
      </SidebarFooter>
    </Sidebar>
  )
}
