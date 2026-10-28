import { useState, type FormEvent } from "react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"

import { describeError } from "@/api/errors"
import { orgAdminApi, type OrgOperatingHours } from "@/api/org"
import { StatusBadge } from "@/components/status-badge"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { DialogClose, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Field, FieldError, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { useFormatters } from "@/lib/format"
import { FacetSection, type FacetEditing } from "@/components/facet-section"

const DAYS = [0, 1, 2, 3, 4, 5, 6]

export function OperatingHours({
  orgUnit,
  hours,
  canWrite,
  onChanged,
}: {
  orgUnit: number
  hours: OrgOperatingHours[]
  canWrite: boolean
  onChanged: () => void
}) {
  const { t } = useTranslation("orgUnits")
  const fmt = useFormatters()
  const sorted = [...hours].sort((a, b) => a.day_of_week - b.day_of_week || a.open_time.localeCompare(b.open_time))
  const day = (h: OrgOperatingHours) => fmt.weekday(h.day_of_week)

  return (
    <FacetSection
      title={t("hours.title")}
      description={t("hours.description")}
      newLabel={t("hours.new")}
      empty={t("hours.none")}
      rows={sorted}
      canWrite={canWrite}
      rowName="hours"
      columns={[
        { id: "day", header: t("hours.columns.day"), className: "w-40", cell: (h) => day(h) },
        {
          id: "hours",
          header: t("hours.columns.hours"),
          cell: (h) =>
            h.is_closed ? (
              <StatusBadge tone="neutral">{t("hours.closed")}</StatusBadge>
            ) : (
              <span className="tabular-nums">
                {t("hours.range", { open: fmt.timeOfDay(h.open_time), close: fmt.timeOfDay(h.close_time) })}
              </span>
            ),
        },
      ]}
      editLabel={(h) => t("hours.editLabel", { day: day(h) })}
      deleteLabel={(h) => t("hours.deleteLabel", { day: day(h) })}
      deleteTitle={t("hours.deleteTitle")}
      deleteBody={(h) => t("hours.deleteBody", { day: day(h) })}
      deletedMessage={(h) => t("hours.deleted", { day: day(h) })}
      onDelete={(h) => orgAdminApi.deleteHours(h.id)}
      onChanged={onChanged}
      renderForm={(editing, done) => <HoursForm orgUnit={orgUnit} editing={editing} onDone={done} />}
    />
  )
}

function HoursForm({
  orgUnit,
  editing,
  onDone,
}: {
  orgUnit: number
  editing: FacetEditing<OrgOperatingHours>
  onDone: () => void
}) {
  const { t } = useTranslation(["orgUnits", "common"])
  const fmt = useFormatters()
  const existing = editing.mode === "edit" ? editing.row : null
  const [dayOfWeek, setDayOfWeek] = useState(String(existing?.day_of_week ?? 1))
  // Native time inputs use "HH:MM"; the API stores "HH:MM:SS".
  const [openTime, setOpenTime] = useState((existing?.open_time ?? "09:00:00").slice(0, 5))
  const [closeTime, setCloseTime] = useState((existing?.close_time ?? "17:00:00").slice(0, 5))
  const [isClosed, setIsClosed] = useState(existing?.is_closed ?? false)
  const [error, setError] = useState<string | null>(null)
  const [pending, setPending] = useState(false)

  // "HH:MM" compares correctly as a string.
  const orderValid = isClosed || (openTime !== "" && closeTime !== "" && closeTime > openTime)

  const submit = async (e: FormEvent) => {
    e.preventDefault()
    if (!orderValid) return
    setPending(true)
    setError(null)
    const day = Number(dayOfWeek)
    const fields = {
      day_of_week: day,
      // Required even when closed; the stored times are ignored then.
      open_time: `${openTime || "00:00"}:00`,
      close_time: `${closeTime || "00:00"}:00`,
      is_closed: isClosed,
    }
    try {
      if (existing) await orgAdminApi.updateHours(existing.id, fields)
      else await orgAdminApi.createHours({ org_unit: orgUnit, ...fields })
      toast.success(t("hours.saved", { day: fmt.weekday(day) }))
      onDone()
    } catch (err) {
      setError(describeError(err))
      setPending(false)
    }
  }

  return (
    <form onSubmit={submit} className="contents">
      <DialogHeader>
        <DialogTitle>{existing ? t("hours.editTitle") : t("hours.createTitle")}</DialogTitle>
      </DialogHeader>
      <FieldGroup>
        <Field>
          <FieldLabel htmlFor="hours-day">{t("hours.day")}</FieldLabel>
          <Select value={dayOfWeek} onValueChange={setDayOfWeek}>
            <SelectTrigger id="hours-day" className="w-44">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {DAYS.map((d) => (
                <SelectItem key={d} value={String(d)}>
                  {fmt.weekday(d)}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </Field>
        <Field orientation="horizontal">
          <Checkbox id="hours-closed" checked={isClosed} onCheckedChange={(v) => setIsClosed(v === true)} />
          <FieldLabel htmlFor="hours-closed" className="font-normal">
            {t("hours.isClosed")}
          </FieldLabel>
        </Field>
        {!isClosed && (
          <div className="grid grid-cols-2 gap-4 sm:max-w-sm">
            <Field>
              <FieldLabel htmlFor="hours-open">{t("hours.open")}</FieldLabel>
              <Input id="hours-open" type="time" required value={openTime} onChange={(e) => setOpenTime(e.target.value)} />
            </Field>
            <Field data-invalid={!orderValid}>
              <FieldLabel htmlFor="hours-close">{t("hours.close")}</FieldLabel>
              <Input
                id="hours-close"
                type="time"
                required
                value={closeTime}
                aria-invalid={!orderValid}
                onChange={(e) => setCloseTime(e.target.value)}
              />
            </Field>
          </div>
        )}
        {!orderValid && openTime && closeTime && <FieldError>{t("hours.closeAfterOpen")}</FieldError>}
        {error && <FieldError>{error}</FieldError>}
      </FieldGroup>
      <DialogFooter>
        <DialogClose asChild>
          <Button type="button" variant="outline">
            {t("common:actions.cancel")}
          </Button>
        </DialogClose>
        <Button type="submit" disabled={pending || !orderValid}>
          {t("common:actions.save")}
        </Button>
      </DialogFooter>
    </form>
  )
}
