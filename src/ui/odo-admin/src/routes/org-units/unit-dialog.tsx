import { useState, type FormEvent } from "react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"

import { ApiRequestError } from "@/api/client"
import { describeError } from "@/api/errors"
import { orgAdminApi, subtreeIds, type Flattened, type OrgUnitNode, type UnitRow, type UnitType } from "@/api/org"
import { Combobox, type ComboboxOption } from "@/components/combobox"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Field, FieldDescription, FieldError, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"

export type UnitEditing =
  | { mode: "create"; parent?: number }
  | { mode: "edit"; unit: Flattened<OrgUnitNode> }

type Units = readonly Flattened<OrgUnitNode>[]

export function UnitDialog({
  editing,
  units,
  unitTypes,
  onOpenChange,
  onSaved,
}: {
  editing: UnitEditing | null
  units: Units
  unitTypes: UnitType[]
  onOpenChange: (open: boolean) => void
  onSaved: (unit: UnitRow) => void
}) {
  return (
    <Dialog open={editing !== null} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        {editing && (
          <UnitForm
            editing={editing}
            units={units}
            unitTypes={unitTypes}
            onDone={(unit) => {
              onOpenChange(false)
              onSaved(unit)
            }}
          />
        )}
      </DialogContent>
    </Dialog>
  )
}

/** IANA zones the browser knows, plus the current value if it's not one. */
function timeZoneOptions(current: string, noneLabel: string): ComboboxOption[] {
  const zones = Intl.supportedValuesOf("timeZone")
  if (current && !zones.includes(current)) zones.unshift(current)
  return [{ value: "", label: noneLabel }, ...zones.map((z) => ({ value: z, label: z }))]
}

function UnitForm({
  editing,
  units,
  unitTypes,
  onDone,
}: {
  editing: UnitEditing
  units: Units
  unitTypes: UnitType[]
  onDone: (unit: UnitRow) => void
}) {
  const { t } = useTranslation(["orgUnits", "common"])
  const existing = editing.mode === "edit" ? editing.unit : null
  const isRoot = existing !== null && existing.parent == null
  const initialParent = existing?.parent ?? (editing.mode === "create" ? editing.parent : undefined)

  const [label, setLabel] = useState(existing?.label ?? "")
  const [code, setCode] = useState(existing?.code ?? "")
  const [parent, setParent] = useState(initialParent != null ? String(initialParent) : "")
  const [unitType, setUnitType] = useState(existing ? String(existing.unit_type.id) : "")
  const [timezone, setTimezone] = useState(existing?.timezone ?? "")
  const [labelError, setLabelError] = useState<string | null>(null)
  const [codeError, setCodeError] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [pending, setPending] = useState(false)

  // A unit can't move under itself or its descendants.
  const excluded = existing ? subtreeIds(units, existing.id) : new Set<number>()
  const parentOptions = units
    .filter((u) => !excluded.has(u.id))
    .map((u) => ({ value: String(u.id), label: u.label, detail: u.code, depth: u.depth }))

  const submit = async (e: FormEvent) => {
    e.preventDefault()
    setPending(true)
    setLabelError(null)
    setCodeError(null)
    setError(null)
    const fields = {
      label: label.trim(),
      code: code.trim(),
      unit_type: Number(unitType),
      timezone,
    }
    try {
      const saved = existing
        ? await orgAdminApi.updateUnit(existing.id, {
            ...fields,
            // The root has no parent and can't be given one.
            ...(isRoot ? {} : { parent: Number(parent) }),
          })
        : await orgAdminApi.createUnit({ ...fields, parent: Number(parent) })
      toast.success(t(existing ? "unit.updated" : "unit.created", { label: saved.label }))
      onDone(saved)
    } catch (err) {
      if (err instanceof ApiRequestError && err.code === "UNIT_CODE_TAKEN") {
        setCodeError(t("common:errors.codes.UNIT_CODE_TAKEN"))
      } else if (err instanceof ApiRequestError && err.code === "UNIT_LABEL_TAKEN") {
        setLabelError(t("common:errors.codes.UNIT_LABEL_TAKEN"))
      } else {
        setError(describeError(err))
      }
      setPending(false)
    }
  }

  const ready = label.trim() && code.trim() && unitType && (isRoot || parent)

  return (
    <form onSubmit={submit} className="contents">
      <DialogHeader>
        <DialogTitle>{existing ? t("unit.editTitle") : t("unit.createTitle")}</DialogTitle>
      </DialogHeader>
      <FieldGroup>
        <Field data-invalid={labelError !== null}>
          <FieldLabel htmlFor="unit-label">{t("unit.label")}</FieldLabel>
          <Input
            id="unit-label"
            required
            autoFocus
            value={label}
            aria-invalid={labelError !== null}
            onChange={(e) => setLabel(e.target.value)}
          />
          {labelError && <FieldError>{labelError}</FieldError>}
        </Field>
        <Field data-invalid={codeError !== null}>
          <FieldLabel htmlFor="unit-code">{t("unit.code")}</FieldLabel>
          <Input
            id="unit-code"
            className="font-mono"
            required
            value={code}
            aria-invalid={codeError !== null}
            onChange={(e) => setCode(e.target.value)}
          />
          {codeError && <FieldError>{codeError}</FieldError>}
        </Field>
        <Field>
          <FieldLabel htmlFor="unit-parent">{t("unit.parent")}</FieldLabel>
          <Combobox
            id="unit-parent"
            options={parentOptions}
            value={parent}
            onValueChange={setParent}
            disabled={isRoot}
            placeholder={t("unit.parentPlaceholder")}
            searchPlaceholder={t("unit.parentSearch")}
          />
          {isRoot && <FieldDescription>{t("unit.rootFixed")}</FieldDescription>}
        </Field>
        <Field>
          <FieldLabel htmlFor="unit-type">{t("unit.type")}</FieldLabel>
          <Select value={unitType} onValueChange={setUnitType}>
            <SelectTrigger id="unit-type" className="w-full">
              <SelectValue placeholder={t("unit.typePlaceholder")} />
            </SelectTrigger>
            <SelectContent>
              {unitTypes.map((u) => (
                <SelectItem key={u.id} value={String(u.id)}>
                  {u.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </Field>
        <Field>
          <FieldLabel htmlFor="unit-timezone">{t("unit.timezone")}</FieldLabel>
          <Combobox
            id="unit-timezone"
            options={timeZoneOptions(existing?.timezone ?? "", t("unit.timezoneNone"))}
            value={timezone}
            onValueChange={setTimezone}
            placeholder={t("unit.timezonePlaceholder")}
            searchPlaceholder={t("unit.timezoneSearch")}
          />
        </Field>
        {error && <FieldError>{error}</FieldError>}
      </FieldGroup>
      <DialogFooter>
        <DialogClose asChild>
          <Button type="button" variant="outline">
            {t("common:actions.cancel")}
          </Button>
        </DialogClose>
        <Button type="submit" disabled={pending || !ready}>
          {t("common:actions.save")}
        </Button>
      </DialogFooter>
    </form>
  )
}
