import { PencilIcon, PlusIcon, Trash2Icon } from "lucide-react"
import { useState } from "react"
import { useTranslation } from "react-i18next"
import { useLoaderData, useRevalidator } from "react-router"
import { toast } from "sonner"

import { orgAdminApi, type UnitType } from "@/api/org"
import { PERMS } from "@/auth/perms"
import { checkPerms } from "@/auth/session"
import { ConfirmDialog } from "@/components/confirm-dialog"
import { DataTable, type Column } from "@/components/data-table/data-table"
import { DisabledReason } from "@/components/disabled-reason"
import { PageHeader } from "@/components/page-header"
import { StatusBadge } from "@/components/status-badge"
import { Button } from "@/components/ui/button"
import { useReportError } from "@/hooks/use-report-error"
import { loadNamespaces } from "@/i18n"
import { UnitTypeDialog, type Editing } from "./unit-type-dialog"

export async function loader() {
  const [unitTypes, perms] = await Promise.all([
    orgAdminApi.listAllUnitTypes(),
    checkPerms({ canWrite: PERMS.ORG_UNIT_WRITE }),
    loadNamespaces("unitTypes"),
  ])
  return { unitTypes, ...perms }
}

/** Reference data for the org tree: a small list, loaded whole. */
export function Component() {
  const { unitTypes, canWrite } = useLoaderData<typeof loader>()
  const { t } = useTranslation(["unitTypes", "common"])
  const revalidator = useRevalidator()
  const reportError = useReportError()
  const [editing, setEditing] = useState<Editing | null>(null)
  const [deleting, setDeleting] = useState<UnitType | null>(null)

  const deleteType = async (unitType: UnitType) => {
    try {
      await orgAdminApi.deleteUnitType(unitType.id)
      toast.success(t("delete.done", { label: unitType.label }))
      await revalidator.revalidate()
    } catch (err) {
      reportError(err, "Failed to delete unit type")
    }
  }

  const columns: Column<UnitType>[] = [
    { id: "label", header: t("columns.label"), cell: (u) => <span className="font-medium">{u.label}</span> },
    { id: "parent", header: t("columns.parent"), cell: (u) => u.parent_label ?? t("common:none") },
    {
      id: "contains",
      header: t("columns.contains"),
      cell: (u) => (
        <div className="flex flex-wrap gap-1">
          {u.can_have_staff && <StatusBadge tone="info">{t("staff")}</StatusBadge>}
          {u.can_have_patrons && <StatusBadge tone="info">{t("patrons")}</StatusBadge>}
          {!u.can_have_staff && !u.can_have_patrons && t("common:none")}
        </div>
      ),
    },
    { id: "units", header: t("columns.units"), className: "w-24 tabular-nums", cell: (u) => u.unit_count },
  ]
  if (canWrite) {
    columns.push({
      id: "actions",
      header: null,
      className: "w-1 whitespace-nowrap text-end",
      cell: (u) => (
        <div className="flex justify-end gap-1">
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label={t("editLabel", { label: u.label })}
            onClick={() => setEditing({ mode: "edit", unitType: u })}
          >
            <PencilIcon />
          </Button>
          {u.unit_count > 0 ? (
            <DisabledReason label={t("deleteLabel", { label: u.label })} reason={t("inUse", { count: u.unit_count })}>
              <Button variant="ghost" size="icon-sm" disabled aria-hidden="true" tabIndex={-1}>
                <Trash2Icon />
              </Button>
            </DisabledReason>
          ) : (
            <Button
              variant="ghost"
              size="icon-sm"
              aria-label={t("deleteLabel", { label: u.label })}
              onClick={() => setDeleting(u)}
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
      <DataTable label={t("title")} rows={unitTypes} columns={columns} rowKey={(u) => u.id} empty={t("empty")} />

      <UnitTypeDialog
        editing={editing}
        unitTypes={unitTypes}
        onOpenChange={(open) => !open && setEditing(null)}
        onSaved={() => void revalidator.revalidate()}
      />
      <ConfirmDialog
        open={deleting !== null}
        onOpenChange={(open) => !open && setDeleting(null)}
        title={t("delete.title")}
        description={t("delete.body", { label: deleting?.label ?? "" })}
        confirmLabel={t("common:actions.delete")}
        destructive
        onConfirm={async () => {
          if (deleting) await deleteType(deleting)
        }}
      />
    </div>
  )
}
