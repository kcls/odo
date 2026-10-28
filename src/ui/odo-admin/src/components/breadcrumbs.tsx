import { Fragment } from "react"
import { useTranslation } from "react-i18next"
import { Link, useMatches, type UIMatch } from "react-router"

import {
  Breadcrumb,
  BreadcrumbItem,
  BreadcrumbLink,
  BreadcrumbList,
  BreadcrumbPage,
  BreadcrumbSeparator,
} from "@/components/ui/breadcrumb"
import type { RouteHandle } from "@/lib/route-handle"

export function Breadcrumbs() {
  const { t } = useTranslation()
  const crumbs = (useMatches() as UIMatch<unknown, RouteHandle | undefined>[]).flatMap((m) =>
    m.handle?.crumb ? [{ to: m.pathname, label: m.handle.crumb(m as UIMatch<unknown, RouteHandle>, t) }] : []
  )
  if (crumbs.length === 0) return null
  return (
    <Breadcrumb aria-label={t("breadcrumb")}>
      <BreadcrumbList>
        {crumbs.map((c, i) => (
          <Fragment key={c.to}>
            {i > 0 && <BreadcrumbSeparator />}
            <BreadcrumbItem>
              {i === crumbs.length - 1 ? (
                <BreadcrumbPage>{c.label}</BreadcrumbPage>
              ) : (
                <BreadcrumbLink asChild>
                  <Link to={c.to}>{c.label}</Link>
                </BreadcrumbLink>
              )}
            </BreadcrumbItem>
          </Fragment>
        ))}
      </BreadcrumbList>
    </Breadcrumb>
  )
}
