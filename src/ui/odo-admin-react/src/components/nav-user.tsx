import { ChevronsUpDownIcon, LanguagesIcon, LogOutIcon, SunMoonIcon } from "lucide-react"
import { useTranslation } from "react-i18next"

import { session, useCurrentUser } from "@/auth/session"
import { UI_LANGUAGES } from "@/i18n"
import { THEMES, useTheme } from "@/components/theme-provider"
import { Avatar, AvatarFallback } from "@/components/ui/avatar"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { SidebarMenu, SidebarMenuButton, SidebarMenuItem, useSidebar } from "@/components/ui/sidebar"

function initials(name: string): string {
  return name
    .split(/\s+/)
    .map((part) => part[0])
    .join("")
    .slice(0, 2)
    .toUpperCase()
}

type Identity = { name: string; email: string }

function UserIdentity({ user }: { user: Identity }) {
  return (
    <>
      <Avatar className="size-8 rounded-md">
        <AvatarFallback className="rounded-md">{initials(user.name)}</AvatarFallback>
      </Avatar>
      <div className="grid flex-1 text-start text-sm leading-tight">
        <span className="truncate font-medium">{user.name}</span>
        <span className="truncate text-xs text-muted-foreground">{user.email}</span>
      </div>
    </>
  )
}

export function NavUser() {
  const { t, i18n } = useTranslation()
  const { theme, setTheme } = useTheme()
  const { isMobile } = useSidebar()
  const current = useCurrentUser()
  // The shell only renders signed in; null is the instant after sign-out.
  if (!current) return null
  const user: Identity = {
    name: current.display_name || current.username || current.email,
    email: current.email,
  }

  return (
    <SidebarMenu>
      <SidebarMenuItem>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <SidebarMenuButton
              size="lg"
              aria-label={t("user.menu")}
              className="data-[state=open]:bg-sidebar-accent data-[state=open]:text-sidebar-accent-foreground"
            >
              <UserIdentity user={user} />
              <ChevronsUpDownIcon className="ms-auto size-4" />
            </SidebarMenuButton>
          </DropdownMenuTrigger>
          <DropdownMenuContent
            className="min-w-56"
            side={isMobile ? "bottom" : "right"}
            align="end"
            sideOffset={4}
          >
            <DropdownMenuLabel className="flex items-center gap-2 p-1 font-normal">
              <UserIdentity user={user} />
            </DropdownMenuLabel>
            <DropdownMenuSeparator />
            <DropdownMenuSub>
              <DropdownMenuSubTrigger>
                <SunMoonIcon />
                {t("user.theme")}
              </DropdownMenuSubTrigger>
              <DropdownMenuSubContent>
                <DropdownMenuRadioGroup
                  value={theme}
                  onValueChange={(v) => setTheme(v as (typeof THEMES)[number])}
                >
                  {THEMES.map((value) => (
                    <DropdownMenuRadioItem key={value} value={value}>
                      {t(`user.themes.${value}`)}
                    </DropdownMenuRadioItem>
                  ))}
                </DropdownMenuRadioGroup>
              </DropdownMenuSubContent>
            </DropdownMenuSub>
            <DropdownMenuSub>
              <DropdownMenuSubTrigger>
                <LanguagesIcon />
                {t("user.language")}
              </DropdownMenuSubTrigger>
              <DropdownMenuSubContent>
                <DropdownMenuRadioGroup
                  value={i18n.resolvedLanguage}
                  onValueChange={(lng) => void i18n.changeLanguage(lng)}
                >
                  {UI_LANGUAGES.map((lng) => (
                    <DropdownMenuRadioItem key={lng} value={lng}>
                      {t(`uiLanguages.${lng}`)}
                    </DropdownMenuRadioItem>
                  ))}
                </DropdownMenuRadioGroup>
              </DropdownMenuSubContent>
            </DropdownMenuSub>
            <DropdownMenuSeparator />
            <DropdownMenuItem onSelect={() => void session.logout()}>
              <LogOutIcon />
              {t("user.signOut")}
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </SidebarMenuItem>
    </SidebarMenu>
  )
}
