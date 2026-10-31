import { Outlet } from "react-router"

import type { RouteHandle } from "@/lib/route-handle"

export const handle: RouteHandle = {
  crumb: (_match, t) => t("history.crumb", { ns: "templates" }),
}

export function Component() {
  return <Outlet />
}
