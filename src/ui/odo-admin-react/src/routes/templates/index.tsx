import { PlusIcon } from "lucide-react"
import { useTranslation } from "react-i18next"
import { Link, useLoaderData, type LoaderFunctionArgs } from "react-router"

import { templatesApi, type TemplateRow } from "@/api/notify"
import { PERMS } from "@/auth/perms"
import { checkPerms } from "@/auth/session"
import { DataTable, type Column } from "@/components/data-table/data-table"
import { Paginator } from "@/components/data-table/paginator"
import { PageHeader } from "@/components/page-header"
import { SearchInput } from "@/components/search-input"
import { StatusBadge } from "@/components/status-badge"
import { Button } from "@/components/ui/button"
import { useBusy } from "@/hooks/use-busy"
import { loadNamespaces } from "@/i18n"
import { useFormatters } from "@/lib/format"
import {
  listRequest,
  readListState,
  redirectPastLastPage,
  useListState,
  type ListDefaults,
} from "@/lib/list-state"

/** Matches the backend's sort allow-list for template/list. */
const LIST: ListDefaults = {
  sort: { key: "code", dir: "asc" },
  sortKeys: ["code", "name", "active", "created", "updated"],
}

export async function loader({ request }: LoaderFunctionArgs) {
  const url = new URL(request.url)
  const state = readListState(url.searchParams, LIST)
  const [page, perms] = await Promise.all([
    templatesApi.listPage(listRequest(state)),
    checkPerms({ canWrite: PERMS.TEMPLATE_WRITE }),
    loadNamespaces("templates"),
  ])
  redirectPastLastPage(url, state, page.total, LIST)
  return { page, ...perms }
}

export function Component() {
  const { page, canWrite } = useLoaderData<typeof loader>()
  const { t } = useTranslation("templates")
  const fmt = useFormatters()
  const [list, setList] = useListState(LIST)
  const busy = useBusy()

  const columns: Column<TemplateRow>[] = [
    {
      id: "code",
      header: t("columns.code"),
      sortKey: "code",
      cell: (r) => (
        <Link to={String(r.id)} className="font-mono text-xs hover:underline focus-visible:underline">
          {r.code}
        </Link>
      ),
    },
    { id: "name", header: t("columns.name"), sortKey: "name", cell: (r) => r.name },
    {
      id: "status",
      header: t("columns.status"),
      sortKey: "active",
      cell: (r) => (
        <div className="flex flex-wrap gap-1">
          {r.is_active ? (
            <StatusBadge tone="success">{t("active")}</StatusBadge>
          ) : (
            <StatusBadge tone="neutral">{t("inactive")}</StatusBadge>
          )}
          {r.body_template_html && <StatusBadge tone="info">{t("html")}</StatusBadge>}
        </div>
      ),
    },
    {
      id: "updated",
      header: t("columns.updated"),
      sortKey: "updated",
      className: "whitespace-nowrap",
      cell: (r) => <span title={fmt.dateTime(r.updated_at)}>{fmt.relativeTime(r.updated_at)}</span>,
    },
  ]

  return (
    <div className="flex flex-col gap-4">
      <PageHeader
        title={t("title")}
        description={t("subtitle")}
        actions={
          canWrite && (
            <Button asChild>
              <Link to="new">
                <PlusIcon data-icon="inline-start" />
                {t("new")}
              </Link>
            </Button>
          )
        }
      />
      <SearchInput value={list.q} onCommit={(q) => setList({ q })} label={t("search")} placeholder={t("search")} />
      <DataTable
        label={t("title")}
        rows={page.rows}
        columns={columns}
        rowKey={(r) => r.id}
        rowHref={(r) => String(r.id)}
        sort={list.sort}
        onSortChange={(sort) => setList({ sort })}
        empty={list.q ? t("noMatches") : t("empty")}
        busy={busy}
      />
      <Paginator total={page.total} pageIndex={list.pageIndex} pageSize={list.pageSize} onChange={setList} />
    </div>
  )
}
