import type { RouteHandle } from "@/lib/route-handle"

// The editor, creating: an existing template's crumb comes from its route.
export { Component, loader } from "./editor"

export const handle: RouteHandle = {
  crumb: (_match, t) => t("editor.newTitle", { ns: "templates" }),
}
