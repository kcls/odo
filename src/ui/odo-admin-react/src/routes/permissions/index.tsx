import { PencilIcon, PlusIcon, Trash2Icon } from "lucide-react"
import { useState } from "react"
import { useTranslation } from "react-i18next"
import { useLoaderData, useRevalidator, type LoaderFunctionArgs } from "react-router"
import { toast } from "sonner"

import { authzApi, type PermissionRow } from "@/api/authz"
import { PERMS } from "@/auth/perms"
import { checkPerms } from "@/auth/session"
import { ConfirmDialog } from "@/components/confirm-dialog"
import { DataTable, type Column } from "@/components/data-table/data-table"
import { Paginator } from "@/components/data-table/paginator"
import { DisabledReason } from "@/components/disabled-reason"
import { PageHeader } from "@/components/page-header"
import { SearchInput } from "@/components/search-input"
import { Button } from "@/components/ui/button"
import { useBusy } from "@/hooks/use-busy"
import { useReportError } from "@/hooks/use-report-error"
import { loadNamespaces } from "@/i18n"
import {
  listRequest,
  readListState,
  redirectPastLastPage,
  useListState,
  type ListDefaults,
} from "@/lib/list-state"
import { PermissionDialog } from "./permission-dialog"

/** Matches the backend's sort allow-list for permission/list. */
const LIST: ListDefaults = {
  sort: { key: "code", dir: "asc" },
  sortKeys: ["code", "description"],
}

export async function loader({ request }: LoaderFunctionArgs) {
  const url = new URL(request.url)
  const state = readListState(url.searchParams, LIST)
  const [page, perms] = await Promise.all([
    authzApi.listPermissionsPage(listRequest(state)),
    checkPerms({ canWrite: PERMS.ROLE_WRITE }),
    loadNamespaces("permissions"),
  ])
  redirectPastLastPage(url, state, page.total, LIST)
  return { page, ...perms }
}

type Editing = { mode: "create" } | { mode: "edit"; permission: PermissionRow }

export function Component() {
  const { page, canWrite } = useLoaderData<typeof loader>()
  const { t } = useTranslation(["permissions", "common"])
  const [list, setList] = useListState(LIST)
  const busy = useBusy()
  const revalidator = useRevalidator()
  const reportError = useReportError()
  const [editing, setEditing] = useState<Editing | null>(null)
  const [deleting, setDeleting] = useState<PermissionRow | null>(null)

  const deletePermission = async (permission: PermissionRow) => {
    try {
      await authzApi.deletePermission(permission.code)
      toast.success(t("delete.done", { code: permission.code }))
      await revalidator.revalidate()
    } catch (err) {
      reportError(err, "Failed to delete permission")
    }
  }

  const columns: Column<PermissionRow>[] = [
    {
      id: "code",
      header: t("columns.code"),
      sortKey: "code",
      cell: (p) => <span className="font-mono text-xs">{p.code}</span>,
    },
    {
      id: "description",
      header: t("columns.description"),
      sortKey: "description",
      cell: (p) => p.description,
    },
    {
      id: "roles",
      header: t("columns.roles"),
      className: "w-24 tabular-nums",
      cell: (p) => p.role_count,
    },
  ]
  if (canWrite) {
    columns.push({
      id: "actions",
      header: null,
      className: "w-1 whitespace-nowrap text-end",
      cell: (p) => (
        <div className="flex justify-end gap-1">
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label={t("editLabel", { code: p.code })}
            onClick={() => setEditing({ mode: "edit", permission: p })}
          >
            <PencilIcon />
          </Button>
          {p.role_count > 0 ? (
            <DisabledReason label={t("deleteLabel", { code: p.code })} reason={t("inUse", { count: p.role_count })}>
              <Button variant="ghost" size="icon-sm" disabled aria-hidden="true" tabIndex={-1}>
                <Trash2Icon />
              </Button>
            </DisabledReason>
          ) : (
            <Button
              variant="ghost"
              size="icon-sm"
              aria-label={t("deleteLabel", { code: p.code })}
              onClick={() => setDeleting(p)}
            >
              <Trash2Icon />
            </Button>
          )}
        </div>
      ),
    })
  }

  return (
    <div className="flex flex-col gap-4">
      <PageHeader
        title={t("title")}
        description={t("subtitle")}
        actions={
          canWrite && (
            <Button onClick={() => setEditing({ mode: "create" })}>
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
        rowKey={(p) => p.code}
        sort={list.sort}
        onSortChange={(sort) => setList({ sort })}
        empty={list.q ? t("noMatches") : t("empty")}
        busy={busy}
      />
      <Paginator
        total={page.total}
        pageIndex={list.pageIndex}
        pageSize={list.pageSize}
        onChange={(p) => setList(p)}
      />

      <PermissionDialog
        editing={editing}
        onOpenChange={(open) => !open && setEditing(null)}
        onSaved={() => void revalidator.revalidate()}
      />
      <ConfirmDialog
        open={deleting !== null}
        onOpenChange={(open) => !open && setDeleting(null)}
        title={t("delete.title")}
        description={t("delete.body", { code: deleting?.code ?? "" })}
        confirmLabel={t("common:actions.delete")}
        destructive
        onConfirm={async () => {
          if (deleting) await deletePermission(deleting)
        }}
      />
    </div>
  )
}
