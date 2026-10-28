import { useTranslation } from "react-i18next"
import { Link, useRouteLoaderData } from "react-router"

import { PageHeader } from "@/components/page-header"
import { Card, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { loadNamespaces } from "@/i18n"
import { ADMIN_TOOLS } from "@/lib/admin-tools"
import type { loader as shellLoader } from "@/routes/shell"

export async function loader() {
  await loadNamespaces("home")
  return null
}

export function Component() {
  // The shell already resolved which tools this user may open.
  const { toolIds } = useRouteLoaderData<typeof shellLoader>("shell")!
  const { t } = useTranslation(["home", "common"])
  const cards = ADMIN_TOOLS.filter((tool) => toolIds.includes(tool.id))
    .map((tool) => ({ tool, label: t(`common:tools.${tool.id}.label`) }))
    .sort((a, b) => a.label.localeCompare(b.label))

  return (
    <div className="flex flex-col gap-6">
      <PageHeader title={t("title")} description={t("subtitle")} />
      {cards.length === 0 ? (
        <p className="text-muted-foreground">{t("noTools")}</p>
      ) : (
        <ul className="grid gap-4 sm:grid-cols-2 xl:grid-cols-3">
          {cards.map(({ tool, label }) => (
            <li key={tool.id}>
              <Link
                to={`/${tool.path}`}
                className="block h-full rounded-xl outline-none focus-visible:ring-3 focus-visible:ring-ring/50"
              >
                <Card className="h-full transition-colors hover:bg-muted/50">
                  <CardHeader>
                    <div className="mb-2 flex size-9 items-center justify-center rounded-md bg-primary/10 text-primary">
                      <tool.icon className="size-5" />
                    </div>
                    <CardTitle>{label}</CardTitle>
                    <CardDescription>{t(`common:tools.${tool.id}.description`)}</CardDescription>
                  </CardHeader>
                </Card>
              </Link>
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}
