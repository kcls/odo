import { PlusIcon, Trash2Icon } from "lucide-react"
import { useEffect, useState, type FormEvent } from "react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"

import { authzApi, type AssignmentRow, type RoleRow } from "@/api/authz"
import { ApiRequestError } from "@/api/client"
import { describeError } from "@/api/errors"
import { orgOptions, type OrgOption } from "@/api/org"
import type { UserAccount } from "@/api/users"
import { Combobox } from "@/components/combobox"
import { ConfirmDialog } from "@/components/confirm-dialog"
import { DataTable, type Column } from "@/components/data-table/data-table"
import { Section } from "@/components/section"
import { StatusBadge } from "@/components/status-badge"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Field, FieldError, FieldGroup, FieldLabel } from "@/components/ui/field"
import { useReportError } from "@/hooks/use-report-error"
import { useFormatters } from "@/lib/format"

/**
 * A user's role grants at org units, managed in place. SAML-managed
 * assignments are owned by the IdP mapping and are read-only here. The
 * server checks the write perm at the assignment's org unit, so a 403 is
 * possible even when the button shows.
 */
export function RoleAssignments({
  user,
  assignments,
  canAssign,
  onChanged,
  onViewPermissions,
}: {
  user: UserAccount
  assignments: AssignmentRow[]
  canAssign: boolean
  onChanged: () => void
  /** Offered on the success toast, as the permissions are on another tab. */
  onViewPermissions?: () => void
}) {
  const { t } = useTranslation(["users", "common"])
  const fmt = useFormatters()
  const reportError = useReportError()
  const [assigning, setAssigning] = useState(false)
  const [removing, setRemoving] = useState<AssignmentRow | null>(null)
  const viewPermissions = onViewPermissions && { label: t("roles.viewPermissions"), onClick: onViewPermissions }

  const remove = async (a: AssignmentRow) => {
    const names = { role: a.role_label, orgUnit: a.org_unit_label }
    try {
      await authzApi.deleteAssignment(a.id)
      toast.success(t("roles.removed", names), { action: viewPermissions })
      onChanged()
    } catch (err) {
      if (err instanceof ApiRequestError && err.status === 403) toast.error(t("roles.forbiddenRemove"))
      else reportError(err, "Failed to remove role assignment")
    }
  }

  const columns: Column<AssignmentRow>[] = [
    {
      id: "role",
      header: t("roles.columns.role"),
      cell: (a) => (
        <div className="flex flex-col">
          <span>{a.role_label}</span>
          <span className="font-mono text-xs text-muted-foreground">{a.role}</span>
        </div>
      ),
    },
    { id: "orgUnit", header: t("roles.columns.orgUnit"), cell: (a) => a.org_unit_label },
    {
      id: "source",
      header: t("roles.columns.source"),
      cell: (a) =>
        a.is_managed_by_saml ? (
          <StatusBadge tone="info">{t("roles.sourceSaml")}</StatusBadge>
        ) : (
          <StatusBadge tone="neutral">{t("roles.sourceManual")}</StatusBadge>
        ),
    },
    { id: "added", header: t("roles.columns.added"), cell: (a) => fmt.date(a.created_at) },
  ]
  if (canAssign) {
    columns.push({
      id: "actions",
      header: null,
      className: "w-1 text-end",
      cell: (a) =>
        !a.is_managed_by_saml && (
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label={t("roles.removeLabel", { role: a.role_label, orgUnit: a.org_unit_label })}
            onClick={() => setRemoving(a)}
          >
            <Trash2Icon />
          </Button>
        ),
    })
  }

  return (
    <Section
      title={t("roles.titleCount", { count: assignments.length })}
      description={t("roles.description")}
      actions={
        canAssign && (
          <Button variant="outline" onClick={() => setAssigning(true)}>
            <PlusIcon data-icon="inline-start" />
            {t("roles.assign")}
          </Button>
        )
      }
    >
      <DataTable
        label={t("roles.title")}
        rows={assignments}
        columns={columns}
        rowKey={(a) => a.id}
        empty={t("roles.none")}
      />

      <Dialog open={assigning} onOpenChange={setAssigning}>
        <DialogContent className="sm:max-w-lg">
          {assigning && (
            <AssignForm
              userId={user.id}
              toastAction={viewPermissions}
              onDone={() => {
                setAssigning(false)
                onChanged()
              }}
            />
          )}
        </DialogContent>
      </Dialog>
      <ConfirmDialog
        open={removing !== null}
        onOpenChange={(open) => !open && setRemoving(null)}
        title={t("roles.removeTitle")}
        description={t("roles.removeBody", {
          role: removing?.role_label ?? "",
          orgUnit: removing?.org_unit_label ?? "",
          name: user.display_name,
        })}
        confirmLabel={t("common:actions.remove")}
        destructive
        onConfirm={async () => {
          if (removing) await remove(removing)
        }}
      />
    </Section>
  )
}

type Pickers = { roles: RoleRow[]; units: OrgOption[] }

function AssignForm({
  userId,
  toastAction,
  onDone,
}: {
  userId: number
  toastAction?: { label: string; onClick: () => void }
  onDone: () => void
}) {
  const { t } = useTranslation(["users", "common"])
  const [pickers, setPickers] = useState<Pickers | "failed" | null>(null)
  const [role, setRole] = useState("")
  const [orgUnit, setOrgUnit] = useState("")
  const [error, setError] = useState<string | null>(null)
  const [pending, setPending] = useState(false)

  // Loaded when the dialog opens: only people who assign roles need them.
  useEffect(() => {
    let cancelled = false
    Promise.all([authzApi.listAllRoles(), orgOptions()]).then(
      ([roles, units]) => !cancelled && setPickers({ roles, units }),
      (err) => {
        console.error("Failed to load role/org pickers", err)
        if (!cancelled) setPickers("failed")
      }
    )
    return () => {
      cancelled = true
    }
  }, [])

  const submit = async (e: FormEvent) => {
    e.preventDefault()
    if (!role || !orgUnit || pickers === null || pickers === "failed") return
    setPending(true)
    setError(null)
    try {
      const created = await authzApi.createAssignment({ usr: userId, role, org_unit: Number(orgUnit) })
      toast.success(t("roles.assigned", { role: created.role_label, orgUnit: created.org_unit_label }), {
        action: toastAction,
      })
      onDone()
    } catch (err) {
      setError(
        err instanceof ApiRequestError && err.status === 403
          ? t("roles.forbiddenAssign")
          : describeError(err)
      )
      setPending(false)
    }
  }

  const ready = pickers !== null && pickers !== "failed"

  return (
    <form onSubmit={submit} className="contents">
      <DialogHeader>
        <DialogTitle>{t("roles.assign")}</DialogTitle>
      </DialogHeader>
      <FieldGroup>
        <Field>
          <FieldLabel htmlFor="assign-role">{t("roles.role")}</FieldLabel>
          <Combobox
            id="assign-role"
            options={ready ? pickers.roles.map((r) => ({ value: r.code, label: r.label, detail: r.code })) : []}
            value={role}
            onValueChange={setRole}
            disabled={!ready}
            placeholder={t("roles.rolePlaceholder")}
            searchPlaceholder={t("roles.roleSearch")}
          />
        </Field>
        <Field>
          <FieldLabel htmlFor="assign-org-unit">{t("roles.orgUnit")}</FieldLabel>
          <Combobox
            id="assign-org-unit"
            options={
              ready ? pickers.units.map((u) => ({ value: String(u.id), label: u.label, depth: u.depth })) : []
            }
            value={orgUnit}
            onValueChange={setOrgUnit}
            disabled={!ready}
            placeholder={t("roles.orgUnitPlaceholder")}
            searchPlaceholder={t("roles.orgUnitSearch")}
          />
        </Field>
        {pickers === "failed" && <FieldError>{t("roles.pickersFailed")}</FieldError>}
        {error && <FieldError>{error}</FieldError>}
      </FieldGroup>
      <DialogFooter>
        <DialogClose asChild>
          <Button type="button" variant="outline">
            {t("common:actions.cancel")}
          </Button>
        </DialogClose>
        <Button type="submit" disabled={pending || !ready || !role || !orgUnit}>
          {t("roles.assignSubmit")}
        </Button>
      </DialogFooter>
    </form>
  )
}
