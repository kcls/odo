import { KeyRoundIcon, ShapesIcon, ShieldIcon, UsersIcon, type LucideIcon } from "lucide-react"
import type { RouteObject } from "react-router"

import { session } from "@/auth/session"
import { PERMS, type PermCode } from "@/auth/perms"

/**
 * Registry of admin tools. An entry here wires up the tool's routes, its
 * sidebar entry, and its home-page card in one place; the tool's label
 * and description are `tools.<id>.*` in locales/en/common.json.
 */
export type AdminTool = {
  id: ToolId
  /** Top-level route path segment, e.g. "users". */
  path: string
  icon: LucideIcon
  /** Read perm gating the route, nav entry, and home card. */
  readPerm: PermCode
  /** Child routes, lazily loaded; the router adds the perm guard. */
  children: RouteObject[]
}

export type ToolId = "users" | "roles" | "permissions" | "orgUnitTypes"

/** In no particular order; the nav and home page sort by label. */
export const ADMIN_TOOLS: AdminTool[] = [
  {
    id: "users",
    path: "users",
    icon: UsersIcon,
    readPerm: PERMS.USER_DETAIL_READ,
    children: [
      { index: true, lazy: () => import("@/routes/users") },
      { path: ":userId", lazy: () => import("@/routes/users/detail") },
    ],
  },
  {
    id: "roles",
    path: "roles",
    icon: ShieldIcon,
    readPerm: PERMS.ROLE_READ,
    children: [
      { index: true, lazy: () => import("@/routes/roles") },
      { path: ":code", lazy: () => import("@/routes/roles/detail") },
    ],
  },
  {
    id: "permissions",
    path: "permissions",
    icon: KeyRoundIcon,
    readPerm: PERMS.ROLE_READ,
    children: [{ index: true, lazy: () => import("@/routes/permissions") }],
  },
  {
    id: "orgUnitTypes",
    path: "org-unit-types",
    icon: ShapesIcon,
    readPerm: PERMS.ORG_UNIT_READ,
    children: [{ index: true, lazy: () => import("@/routes/org-unit-types") }],
  },
]

/** The tools the current user may open, in registry order. */
export async function visibleAdminTools(): Promise<AdminTool[]> {
  const allowed = await Promise.all(ADMIN_TOOLS.map((tool) => session.hasPerm(tool.readPerm)))
  return ADMIN_TOOLS.filter((_, i) => allowed[i])
}
