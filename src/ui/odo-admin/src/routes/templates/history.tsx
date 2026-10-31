import { useTranslation } from "react-i18next"
import { Link, useLoaderData, type LoaderFunctionArgs } from "react-router"

import { templatesApi, type TemplateVersionSummary } from "@/api/notify"
import { DataTable, type Column } from "@/components/data-table/data-table"
import { Paginator } from "@/components/data-table/paginator"
import { DetailHeader } from "@/components/detail-header"
import { StatusBadge } from "@/components/status-badge"
import { useBusy } from "@/hooks/use-busy"
import { loadNamespaces } from "@/i18n"
import { useFormatters } from "@/lib/format"
import { readListState, redirectPastLastPage, useListState, type ListDefaults } from "@/lib/list-state"
import { requireIdParam } from "@/lib/loader"
import { useTemplate } from "./template-route"
import { useFieldLabel, VersionAction } from "./version-labels"

// Newest first, always; only the paging is in the URL.
const LIST: ListDefaults = { sort: { key: "version", dir: "desc" }, sortKeys: [] }

export async function loader({ params, request }: LoaderFunctionArgs) {
  const url = new URL(request.url)
  const state = readListState(url.searchParams, LIST)
  const [page] = await Promise.all([
    templatesApi.versions(requireIdParam(params.templateId), {
      limit: state.pageSize,
      offset: state.pageIndex * state.pageSize,
    }),
    loadNamespaces("templates"),
  ])
  redirectPastLastPage(url, state, page.total, LIST)
  return { page }
}

/** Every saved version of a template, newest first; each opens to its changes. */
export function Component() {
  const { page } = useLoaderData<typeof loader>()
  const template = useTemplate()
  const { t } = useTranslation("templates")
  const fmt = useFormatters()
  const fieldLabel = useFieldLabel()
  const [list, setList] = useListState(LIST)
  const busy = useBusy()
  // Versions number consecutively from 1, so the newest is the total.
  const current = page.total

  const columns: Column<TemplateVersionSummary>[] = [
    {
      id: "version",
      header: t("history.columns.version"),
      className: "whitespace-nowrap",
      cell: (v) => (
        <span className="inline-flex items-center gap-2">
          <Link to={String(v.version)} className="font-medium hover:underline focus-visible:underline">
            {t("history.versionLabel", { version: v.version })}
          </Link>
          {v.version === current && <StatusBadge tone="success">{t("history.current")}</StatusBadge>}
        </span>
      ),
    },
    {
      id: "action",
      header: t("history.columns.action"),
      cell: (v) => <VersionAction action={v.action} restoredFrom={v.restored_from} />,
    },
    {
      id: "fields",
      header: t("history.columns.fields"),
      cell: (v) =>
        v.changed_fields.length > 0 ? (
          v.changed_fields.map(fieldLabel).join(", ")
        ) : (
          <span className="text-muted-foreground">{t("history.noFieldChanges")}</span>
        ),
    },
    {
      id: "by",
      header: t("history.columns.by"),
      cell: (v) => v.changed_by_name ?? <span className="text-muted-foreground">{t("history.unknownAuthor")}</span>,
    },
    {
      id: "at",
      header: t("history.columns.at"),
      className: "whitespace-nowrap",
      cell: (v) => <span title={fmt.dateTime(v.changed_at)}>{fmt.relativeTime(v.changed_at)}</span>,
    },
  ]

  return (
    <div className="flex flex-col gap-4">
      <DetailHeader
        backTo=".."
        backLabel={template.name}
        title={t("history.title", { name: template.name })}
        summary={[t("history.subtitle")]}
      />
      <DataTable
        label={t("history.title", { name: template.name })}
        rows={page.rows}
        columns={columns}
        rowKey={(v) => v.version}
        rowHref={(v) => String(v.version)}
        empty={t("history.empty")}
        busy={busy}
      />
      <Paginator total={page.total} pageIndex={list.pageIndex} pageSize={list.pageSize} onChange={setList} />
    </div>
  )
}
