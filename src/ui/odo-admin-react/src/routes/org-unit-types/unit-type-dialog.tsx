import { useState, type FormEvent } from "react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"

import { ApiRequestError } from "@/api/client"
import { describeError } from "@/api/errors"
import { orgAdminApi, type UnitType } from "@/api/org"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Field, FieldError, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"

export type Editing = { mode: "create" } | { mode: "edit"; unitType: UnitType }

/** Select can't hold an empty value, so "no parent" gets a sentinel. */
const NO_PARENT = "none"

export function UnitTypeDialog({
  editing,
  unitTypes,
  onOpenChange,
  onSaved,
}: {
  editing: Editing | null
  unitTypes: UnitType[]
  onOpenChange: (open: boolean) => void
  onSaved: () => void
}) {
  return (
    <Dialog open={editing !== null} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        {editing && (
          <UnitTypeForm
            editing={editing}
            unitTypes={unitTypes}
            onDone={() => {
              onOpenChange(false)
              onSaved()
            }}
          />
        )}
      </DialogContent>
    </Dialog>
  )
}

function UnitTypeForm({
  editing,
  unitTypes,
  onDone,
}: {
  editing: Editing
  unitTypes: UnitType[]
  onDone: () => void
}) {
  const { t } = useTranslation(["unitTypes", "common"])
  const existing = editing.mode === "edit" ? editing.unitType : null
  const [label, setLabel] = useState(existing?.label ?? "")
  const [parent, setParent] = useState(existing?.parent != null ? String(existing.parent) : NO_PARENT)
  const [canHaveStaff, setCanHaveStaff] = useState(existing?.can_have_staff ?? false)
  const [canHavePatrons, setCanHavePatrons] = useState(existing?.can_have_patrons ?? false)
  const [labelError, setLabelError] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [pending, setPending] = useState(false)

  // A type can't be its own parent; deeper cycles are refused server-side.
  const parentOptions = unitTypes.filter((u) => u.id !== existing?.id)

  const submit = async (e: FormEvent) => {
    e.preventDefault()
    setPending(true)
    setLabelError(null)
    setError(null)
    const fields = {
      label: label.trim(),
      can_have_staff: canHaveStaff,
      can_have_patrons: canHavePatrons,
    }
    const parentId = parent === NO_PARENT ? null : Number(parent)
    try {
      const saved = existing
        ? await orgAdminApi.updateUnitType(existing.id, {
            ...fields,
            // A null parent means "unchanged" to the API; removing one is
            // its own flag.
            ...(parentId !== null
              ? { parent: parentId }
              : { clear_parent: existing.parent != null }),
          })
        : await orgAdminApi.createUnitType({ ...fields, parent: parentId })
      toast.success(t(existing ? "dialog.updated" : "dialog.created", { label: saved.label }))
      onDone()
    } catch (err) {
      if (err instanceof ApiRequestError && err.code === "TYPE_LABEL_TAKEN") {
        setLabelError(t("common:errors.codes.TYPE_LABEL_TAKEN"))
      } else {
        setError(describeError(err))
      }
      setPending(false)
    }
  }

  return (
    <form onSubmit={submit} className="contents">
      <DialogHeader>
        <DialogTitle>{existing ? t("dialog.editTitle") : t("dialog.createTitle")}</DialogTitle>
      </DialogHeader>
      <FieldGroup>
        <Field data-invalid={labelError !== null}>
          <FieldLabel htmlFor="unit-type-label">{t("dialog.label")}</FieldLabel>
          <Input
            id="unit-type-label"
            required
            autoFocus
            value={label}
            aria-invalid={labelError !== null}
            onChange={(e) => setLabel(e.target.value)}
          />
          {labelError && <FieldError>{labelError}</FieldError>}
        </Field>
        <Field>
          <FieldLabel htmlFor="unit-type-parent">{t("dialog.parent")}</FieldLabel>
          <Select value={parent} onValueChange={setParent}>
            <SelectTrigger id="unit-type-parent" className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value={NO_PARENT}>{t("dialog.noParent")}</SelectItem>
              {parentOptions.map((u) => (
                <SelectItem key={u.id} value={String(u.id)}>
                  {u.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </Field>
        <Field orientation="horizontal">
          <Checkbox
            id="unit-type-staff"
            checked={canHaveStaff}
            onCheckedChange={(v) => setCanHaveStaff(v === true)}
          />
          <FieldLabel htmlFor="unit-type-staff" className="font-normal">
            {t("dialog.canHaveStaff")}
          </FieldLabel>
        </Field>
        <Field orientation="horizontal">
          <Checkbox
            id="unit-type-patrons"
            checked={canHavePatrons}
            onCheckedChange={(v) => setCanHavePatrons(v === true)}
          />
          <FieldLabel htmlFor="unit-type-patrons" className="font-normal">
            {t("dialog.canHavePatrons")}
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
        <Button type="submit" disabled={pending || !label.trim()}>
          {t("common:actions.save")}
        </Button>
      </DialogFooter>
    </form>
  )
}
