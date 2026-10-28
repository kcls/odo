import { PlusIcon } from "lucide-react"
import { useState } from "react"
import { useTranslation } from "react-i18next"
import { Link, useLoaderData, useNavigate, type LoaderFunctionArgs } from "react-router"

import { authzApi, type RoleRow } from "@/api/authz"
import { PERMS } from "@/auth/perms"
import { checkPerms } from "@/auth/session"
import { DataTable, type Column } from "@/components/data-table/data-table"
import { Paginator } from "@/components/data-table/paginator"
import { PageHeader } from "@/components/page-header"
import { SearchInput } from "@/components/search-input"
import { Button } from "@/components/ui/button"
import { useBusy } from "@/hooks/use-busy"
import { loadNamespaces } from "@/i18n"
import {
  listRequest,
  readListState,
  redirectPastLastPage,
  useListState,
  type ListDefaults,
} from "@/lib/list-state"
import { CreateRoleDialog } from "./create-role-dialog"
import { rolePath } from "./paths"

/** Matches the backend's sort allow-list for role/list. */
const LIST: ListDefaults = {
  sort: { key: "code", dir: "asc" },
  sortKeys: ["code", "label", "description"],
}

export async function loader({ request }: LoaderFunctionArgs) {
  const url = new URL(request.url)
  const state = readListState(url.searchParams, LIST)
  const [page, perms] = await Promise.all([
    authzApi.listRolesPage(listRequest(state)),
    checkPerms({ canWrite: PERMS.ROLE_WRITE }),
    loadNamespaces("roles"),
  ])
  redirectPastLastPage(url, state, page.total, LIST)
  return { page, ...perms }
}

export function Component() {
  const { page, canWrite } = useLoaderData<typeof loader>()
  const { t } = useTranslation("roles")
  const [list, setList] = useListState(LIST)
  const busy = useBusy()
  const navigate = useNavigate()
  const [creating, setCreating] = useState(false)

  const columns: Column<RoleRow>[] = [
    {
      id: "code",
      header: t("columns.code"),
      sortKey: "code",
      cell: (r) => (
        <Link to={rolePath(r.code)} className="font-mono text-xs hover:underline focus-visible:underline">
          {r.code}
        </Link>
      ),
    },
    { id: "label", header: t("columns.label"), sortKey: "label", cell: (r) => r.label },
    { id: "permissions", header: t("columns.permissions"), className: "w-28 tabular-nums", cell: (r) => r.perm_count },
    { id: "users", header: t("columns.users"), className: "w-24 tabular-nums", cell: (r) => r.user_count },
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
      <SearchInput value={list.q} onCommit={(q) => setList({ q })} label={t("search")} placeholder={t("search")} />
      <DataTable
        label={t("title")}
        rows={page.rows}
        columns={columns}
        rowKey={(r) => r.code}
        rowHref={(r) => rolePath(r.code)}
        sort={list.sort}
        onSortChange={(sort) => setList({ sort })}
        empty={list.q ? t("noMatches") : t("empty")}
        busy={busy}
      />
      <Paginator total={page.total} pageIndex={list.pageIndex} pageSize={list.pageSize} onChange={setList} />

      <CreateRoleDialog
        open={creating}
        onOpenChange={setCreating}
        onCreated={(role) => navigate(rolePath(role.code))}
      />
    </div>
  )
}
