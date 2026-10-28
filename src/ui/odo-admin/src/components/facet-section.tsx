import { PencilIcon, PlusIcon, Trash2Icon } from "lucide-react"
import { useState, type ReactNode } from "react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"

import { ConfirmDialog } from "@/components/confirm-dialog"
import { DataTable, type Column } from "@/components/data-table/data-table"
import { DisabledReason } from "@/components/disabled-reason"
import { Section } from "@/components/section"
import { Button } from "@/components/ui/button"
import { Dialog, DialogContent } from "@/components/ui/dialog"
import { useReportError } from "@/hooks/use-report-error"

/** What a section's form edits: a new row, or an existing one. */
export type FacetEditing<T> = { mode: "create" } | { mode: "edit"; row: T }

/**
 * A bounded list of a subject's parts (a unit's addresses, an IdP's
 * service providers): a titled table with create, edit, and delete. The
 * caller supplies the columns, the form, and the per-row text; this owns
 * the dialogs and their state.
 */
export function FacetSection<T extends { id: number }>({
  title,
  description,
  newLabel,
  empty,
  rows,
  columns,
  canWrite,
  rowName,
  editLabel,
  deleteLabel,
  deleteTitle,
  deleteBody,
  deletedMessage,
  onDelete,
  deleteBlockedReason = () => null,
  onChanged,
  renderForm,
}: {
  title: string
  description: string
  newLabel: string
  empty: string
  rows: readonly T[]
  columns: Column<T>[]
  canWrite: boolean
  /** A short name for log context. */
  rowName: string
  editLabel: (row: T) => string
  deleteLabel: (row: T) => string
  deleteTitle: string
  deleteBody: (row: T) => string
  deletedMessage: (row: T) => string
  onDelete: (row: T) => Promise<unknown>
  /** Why a row can't be deleted right now, or null when it can. */
  deleteBlockedReason?: (row: T) => string | null
  onChanged: () => void
  /** Rendered inside the dialog; call `done` after a successful save. */
  renderForm: (editing: FacetEditing<T>, done: () => void) => ReactNode
}) {
  const { t } = useTranslation()
  const reportError = useReportError()
  const [editing, setEditing] = useState<FacetEditing<T> | null>(null)
  const [deleting, setDeleting] = useState<T | null>(null)

  const remove = async (row: T) => {
    try {
      await onDelete(row)
      toast.success(deletedMessage(row))
      onChanged()
    } catch (err) {
      reportError(err, `Failed to delete ${rowName}`)
    }
  }

  const allColumns: Column<T>[] = canWrite
    ? [
        ...columns,
        {
          id: "actions",
          header: null,
          className: "w-1 whitespace-nowrap text-end",
          cell: (row) => (
            <div className="flex justify-end gap-1">
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label={editLabel(row)}
                onClick={() => setEditing({ mode: "edit", row })}
              >
                <PencilIcon />
              </Button>
              <DeleteButton
                label={deleteLabel(row)}
                blockedReason={deleteBlockedReason(row)}
                onClick={() => setDeleting(row)}
              />
            </div>
          ),
        },
      ]
    : columns

  return (
    <Section
      title={title}
      description={description}
      actions={
        canWrite && (
          <Button variant="outline" onClick={() => setEditing({ mode: "create" })}>
            <PlusIcon data-icon="inline-start" />
            {newLabel}
          </Button>
        )
      }
    >
      <DataTable label={title} rows={rows} columns={allColumns} rowKey={(r) => r.id} empty={empty} />

      <Dialog open={editing !== null} onOpenChange={(open) => !open && setEditing(null)}>
        <DialogContent className="sm:max-w-lg">
          {/* Remounting per open resets the form. */}
          {editing &&
            renderForm(editing, () => {
              setEditing(null)
              onChanged()
            })}
        </DialogContent>
      </Dialog>
      <ConfirmDialog
        open={deleting !== null}
        onOpenChange={(open) => !open && setDeleting(null)}
        title={deleteTitle}
        description={deleting ? deleteBody(deleting) : ""}
        confirmLabel={t("actions.delete")}
        destructive
        onConfirm={async () => {
          if (deleting) await remove(deleting)
        }}
      />
    </Section>
  )
}

function DeleteButton({
  label,
  blockedReason,
  onClick,
}: {
  label: string
  blockedReason: string | null
  onClick: () => void
}) {
  if (blockedReason) {
    return (
      <DisabledReason label={label} reason={blockedReason}>
        <Button variant="ghost" size="icon-sm" disabled aria-hidden="true" tabIndex={-1}>
          <Trash2Icon />
        </Button>
      </DisabledReason>
    )
  }
  return (
    <Button variant="ghost" size="icon-sm" aria-label={label} onClick={onClick}>
      <Trash2Icon />
    </Button>
  )
}
