import { useState, type FormEvent } from "react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"

import { describeError } from "@/api/errors"
import { orgAdminApi, type OrgClosure } from "@/api/org"
import { StatusBadge } from "@/components/status-badge"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { DialogClose, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Field, FieldError, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { useFormatters } from "@/lib/format"
import { FacetSection, type FacetEditing } from "./facet-section"

/** Today's local calendar date as "YYYY-MM-DD", to compare with closure dates. */
function todayYmd(): string {
  const d = new Date()
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`
}

export function Closures({
  orgUnit,
  closures,
  canWrite,
  onChanged,
}: {
  orgUnit: number
  closures: OrgClosure[]
  canWrite: boolean
  onChanged: () => void
}) {
  const { t } = useTranslation("orgUnits")
  const fmt = useFormatters()
  const today = todayYmd()
  // ISO dates sort as strings.
  const sorted = [...closures].sort((a, b) => b.closure_date.localeCompare(a.closure_date))
  const date = (c: OrgClosure) => fmt.calendarDate(c.closure_date)

  return (
    <FacetSection
      title={t("closures.title")}
      description={t("closures.description")}
      newLabel={t("closures.new")}
      empty={t("closures.none")}
      rows={sorted}
      canWrite={canWrite}
      rowName="closure"
      columns={[
        {
          id: "date",
          header: t("closures.columns.date"),
          className: "whitespace-nowrap",
          cell: (c) => <span className={c.closure_date < today ? "text-muted-foreground" : undefined}>{date(c)}</span>,
        },
        { id: "reason", header: t("closures.columns.reason"), cell: (c) => c.reason },
        {
          id: "type",
          header: t("closures.columns.type"),
          cell: (c) => (
            <div className="flex flex-wrap gap-1">
              {c.is_emergency && <StatusBadge tone="danger">{t("closures.emergency")}</StatusBadge>}
              {c.closure_date < today && <StatusBadge tone="neutral">{t("closures.past")}</StatusBadge>}
            </div>
          ),
        },
      ]}
      editLabel={(c) => t("closures.editLabel", { date: date(c) })}
      deleteLabel={(c) => t("closures.deleteLabel", { date: date(c) })}
      deleteTitle={t("closures.deleteTitle")}
      deleteBody={(c) => t("closures.deleteBody", { date: date(c), reason: c.reason })}
      deletedMessage={(c) => t("closures.deleted", { date: date(c) })}
      onDelete={(c) => orgAdminApi.deleteClosure(c.id)}
      onChanged={onChanged}
      renderForm={(editing, done) => <ClosureForm orgUnit={orgUnit} editing={editing} onDone={done} />}
    />
  )
}

function ClosureForm({
  orgUnit,
  editing,
  onDone,
}: {
  orgUnit: number
  editing: FacetEditing<OrgClosure>
  onDone: () => void
}) {
  const { t } = useTranslation(["orgUnits", "common"])
  const fmt = useFormatters()
  const existing = editing.mode === "edit" ? editing.row : null
  // A native date input works in "YYYY-MM-DD", the API's format, so no
  // timezone conversion can shift the day.
  const [closureDate, setClosureDate] = useState(existing?.closure_date ?? "")
  const [reason, setReason] = useState(existing?.reason ?? "")
  const [isEmergency, setIsEmergency] = useState(existing?.is_emergency ?? false)
  const [error, setError] = useState<string | null>(null)
  const [pending, setPending] = useState(false)

  const submit = async (e: FormEvent) => {
    e.preventDefault()
    setPending(true)
    setError(null)
    const fields = { closure_date: closureDate, reason: reason.trim(), is_emergency: isEmergency }
    try {
      const saved = existing
        ? await orgAdminApi.updateClosure(existing.id, fields)
        : await orgAdminApi.createClosure({ org_unit: orgUnit, ...fields })
      toast.success(t("closures.saved", { date: fmt.calendarDate(saved.closure_date) }))
      onDone()
    } catch (err) {
      setError(describeError(err))
      setPending(false)
    }
  }

  return (
    <form onSubmit={submit} className="contents">
      <DialogHeader>
        <DialogTitle>{existing ? t("closures.editTitle") : t("closures.createTitle")}</DialogTitle>
      </DialogHeader>
      <FieldGroup>
        <Field>
          <FieldLabel htmlFor="closure-date">{t("closures.date")}</FieldLabel>
          <Input
            id="closure-date"
            type="date"
            required
            autoFocus
            className="w-44"
            value={closureDate}
            onChange={(e) => setClosureDate(e.target.value)}
          />
        </Field>
        <Field>
          <FieldLabel htmlFor="closure-reason">{t("closures.reason")}</FieldLabel>
          <Input id="closure-reason" required value={reason} onChange={(e) => setReason(e.target.value)} />
        </Field>
        <Field orientation="horizontal">
          <Checkbox
            id="closure-emergency"
            checked={isEmergency}
            onCheckedChange={(v) => setIsEmergency(v === true)}
          />
          <FieldLabel htmlFor="closure-emergency" className="font-normal">
            {t("closures.isEmergency")}
          </FieldLabel>
        </Field>
        {error && <FieldError>{error}</FieldError>}
      </FieldGroup>
      <DialogFooter>
        <DialogClose asChild>
          <Button type="button" variant="outline">
            {t("common:actions.cancel")}
          </Button>
        </DialogClose>
        <Button type="submit" disabled={pending || !closureDate || !reason.trim()}>
          {t("common:actions.save")}
        </Button>
      </DialogFooter>
    </form>
  )
}
