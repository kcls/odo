import { Outlet, useRouteLoaderData, type LoaderFunctionArgs } from "react-router"

import { templatesApi, type TemplateRow } from "@/api/notify"
import { orNotFound, requireIdParam } from "@/lib/loader"
import type { RouteHandle } from "@/lib/route-handle"

/** The template behind the editor and its history pages, and their crumb. */
export async function loader({ params }: LoaderFunctionArgs) {
  const template = await orNotFound(templatesApi.get(requireIdParam(params.templateId)))
  return { template }
}

export const handle: RouteHandle = {
  crumb: (match) => (match.loaderData as Awaited<ReturnType<typeof loader>> | undefined)?.template.name ?? "",
}

/** The template loaded by this route, from any route beneath it. */
export function useTemplate(): TemplateRow {
  const data = useRouteLoaderData("template") as Awaited<ReturnType<typeof loader>> | undefined
  if (!data) throw new Error("useTemplate outside the :templateId route")
  return data.template
}

export function Component() {
  return <Outlet />
}
