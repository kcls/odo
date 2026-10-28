import { createBrowserRouter, data } from "react-router"

import { ADMIN_TOOLS } from "@/lib/admin-tools"
import { requireAuth, requirePerm } from "@/lib/loader"
import type { RouteHandle } from "@/lib/route-handle"
import { RouteError } from "@/routes/error"
import { RootLayout } from "@/routes/root"
import * as shell from "@/routes/shell"

/** For routes that only redirect or throw from their loader. */
const Nothing = () => null

export const router = createBrowserRouter(
  [
    {
      path: "/",
      Component: RootLayout,
      HydrateFallback: Nothing,
      ErrorBoundary: RouteError,
      children: [
        { path: "login", lazy: () => import("@/routes/login") },
        {
          // Pathless and eagerly loaded: its middleware must be in place
          // before any tool's loaders can run.
          id: "shell",
          middleware: [requireAuth],
          loader: shell.loader,
          Component: shell.Component,
          children: [
            {
              // Pathless, so errors render inside the app shell.
              ErrorBoundary: RouteError,
              children: [
                { index: true, lazy: () => import("@/routes/home") },
                ...ADMIN_TOOLS.map((tool) => ({
                  path: tool.path,
                  middleware: [requirePerm(tool.readPerm)],
                  handle: { crumb: (_m, t) => t(`tools.${tool.id}.label`) } satisfies RouteHandle,
                  children: tool.children,
                })),
                {
                  path: "*",
                  Component: Nothing,
                  loader: () => {
                    throw data("Not found", { status: 404 })
                  },
                },
              ],
            },
          ],
        },
      ],
    },
  ],
  { basename: import.meta.env.BASE_URL }
)
