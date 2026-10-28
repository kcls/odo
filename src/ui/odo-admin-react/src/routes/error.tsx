import { TriangleAlertIcon } from "lucide-react"
import { useTranslation } from "react-i18next"
import { isRouteErrorResponse, Link, useRouteError } from "react-router"

import { Button } from "@/components/ui/button"
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty"

export function RouteError() {
  const error = useRouteError()
  const { t } = useTranslation()
  const notFound = isRouteErrorResponse(error) && error.status === 404
  if (!notFound) console.error(error)
  return (
    <Empty>
      <EmptyHeader>
        <EmptyMedia variant="icon">
          <TriangleAlertIcon />
        </EmptyMedia>
        <EmptyTitle>{notFound ? t("errors.notFoundTitle") : t("errors.genericTitle")}</EmptyTitle>
        <EmptyDescription>
          {notFound ? t("errors.notFoundBody") : t("errors.genericBody")}
        </EmptyDescription>
      </EmptyHeader>
      <EmptyContent>
        <Button asChild variant="outline">
          <Link to="/">{t("errors.backHome")}</Link>
        </Button>
      </EmptyContent>
    </Empty>
  )
}
