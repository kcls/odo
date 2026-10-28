import { PlusIcon } from "lucide-react"
import { useState } from "react"
import { useTranslation } from "react-i18next"
import { Link, useLoaderData, useNavigate, useSearchParams, type LoaderFunctionArgs } from "react-router"

import { emailGroupsApi, type EmailGroupRow } from "@/api/notify"
import { PERMS } from "@/auth/perms"
import { checkPerms } from "@/auth/session"
import { DataTable, type Column } from "@/components/data-table/data-table"
import { Paginator } from "@/components/data-table/paginator"
import { PageHeader } from "@/components/page-header"
import { SearchInput } from "@/components/search-input"
import { StatusBadge } from "@/components/status-badge"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Label } from "@/components/ui/label"
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
import { GroupDialog } from "./group-dialog"

/** Matches the backend's sort allow-list for email-group/list. */
const LIST: ListDefaults = {
  sort: { key: "code", dir: "asc" },
  sortKeys: ["code", "label", "active", "created", "updated"],
}

/** `?inactive=1` includes deactivated groups. */
const INACTIVE_PARAM = "inactive"

export async function loader({ request }: LoaderFunctionArgs) {
  const url = new URL(request.url)
  const state = readListState(url.searchParams, LIST)
  const [page, perms] = await Promise.all([
    emailGroupsApi.listPage({
      ...listRequest(state),
      include_inactive: url.searchParams.get(INACTIVE_PARAM) === "1",
    }),
    checkPerms({ canWrite: PERMS.EMAIL_GROUP_WRITE }),
    loadNamespaces("emailGroups"),
  ])
  redirectPastLastPage(url, state, page.total, LIST)
  return { page, ...perms }
}

export function Component() {
  const { page, canWrite } = useLoaderData<typeof loader>()
  const { t } = useTranslation("emailGroups")
  const fmt = useFormatters()
  const navigate = useNavigate()
  const [list, setList] = useListState(LIST)
  const [params, setParams] = useSearchParams()
  const busy = useBusy()
  const [creating, setCreating] = useState(false)
  const includeInactive = params.get(INACTIVE_PARAM) === "1"

  const setIncludeInactive = (on: boolean) =>
    setParams((prev) => {
      if (on) prev.set(INACTIVE_PARAM, "1")
      else prev.delete(INACTIVE_PARAM)
      // The filter changes what each page holds.
      prev.delete("page")
      return prev
    })

  const columns: Column<EmailGroupRow>[] = [
    {
      id: "code",
      header: t("columns.code"),
      sortKey: "code",
      cell: (g) => (
        <Link to={String(g.id)} className="font-mono text-xs hover:underline focus-visible:underline">
          {g.code}
        </Link>
      ),
    },
    { id: "label", header: t("columns.label"), sortKey: "label", cell: (g) => g.label },
    {
      id: "status",
      header: t("columns.status"),
      sortKey: "active",
      cell: (g) =>
        g.is_active ? (
          <StatusBadge tone="success">{t("active")}</StatusBadge>
        ) : (
          <StatusBadge tone="neutral">{t("inactive")}</StatusBadge>
        ),
    },
    {
      id: "members",
      header: t("columns.members"),
      className: "whitespace-nowrap tabular-nums",
      cell: (g) => t("memberCounts", { active: g.active_member_count, total: g.member_count }),
    },
    {
      id: "updated",
      header: t("columns.updated"),
      sortKey: "updated",
      className: "whitespace-nowrap",
      cell: (g) => <span title={fmt.dateTime(g.updated_at)}>{fmt.relativeTime(g.updated_at)}</span>,
    },
  ]

  return (
    <div className="flex flex-col gap-4">
      <PageHeader
        title={t("title")}
        description={t("subtitle")}
        actions={
          canWrite && (
            <Button onClick={() => setCreating(true)}>
              <PlusIcon data-icon="inline-start" />
              {t("new")}
            </Button>
          )
        }
      />
      <div className="flex flex-wrap items-center gap-x-6 gap-y-2">
        <SearchInput value={list.q} onCommit={(q) => setList({ q })} label={t("search")} placeholder={t("search")} />
        <div className="flex items-center gap-2">
          <Checkbox
            id="groups-inactive"
            checked={includeInactive}
            onCheckedChange={(v) => setIncludeInactive(v === true)}
          />
          <Label htmlFor="groups-inactive" className="font-normal">
            {t("showInactive")}
          </Label>
        </div>
      </div>
      <DataTable
        label={t("title")}
        rows={page.rows}
        columns={columns}
        rowKey={(g) => g.id}
        rowHref={(g) => String(g.id)}
        sort={list.sort}
        onSortChange={(sort) => setList({ sort })}
        empty={list.q ? t("noMatches") : t("empty")}
        busy={busy}
      />
      <Paginator total={page.total} pageIndex={list.pageIndex} pageSize={list.pageSize} onChange={setList} />

      <GroupDialog
        group={creating ? null : undefined}
        onOpenChange={setCreating}
        onSaved={(group) => navigate(String(group.id))}
      />
    </div>
  )
}
