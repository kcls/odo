import { useState, type FormEvent } from "react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"

import { ApiRequestError } from "@/api/client"
import { describeError } from "@/api/errors"
import { orgAdminApi, type OrgAddress } from "@/api/org"
import { Button } from "@/components/ui/button"
import { DialogClose, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Field, FieldError, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { FacetSection, type FacetEditing } from "./facet-section"

const ADDRESS_TYPES = ["physical", "mailing"] as const
type AddressType = (typeof ADDRESS_TYPES)[number]

/** Rows may hold a type this UI doesn't know; show it raw. */
function useAddressTypeLabel() {
  const { t } = useTranslation("orgUnits")
  return (type: string) =>
    (ADDRESS_TYPES as readonly string[]).includes(type) ? t(`addresses.types.${type as AddressType}`) : type
}

export function Addresses({
  orgUnit,
  addresses,
  canWrite,
  onChanged,
}: {
  orgUnit: number
  addresses: OrgAddress[]
  canWrite: boolean
  onChanged: () => void
}) {
  const { t } = useTranslation("orgUnits")
  const typeLabel = useAddressTypeLabel()

  return (
    <FacetSection
      title={t("addresses.title")}
      description={t("addresses.description")}
      newLabel={t("addresses.new")}
      empty={t("addresses.none")}
      rows={addresses}
      canWrite={canWrite}
      rowName="address"
      columns={[
        { id: "label", header: t("addresses.columns.label"), cell: (a) => <span className="font-medium">{a.label}</span> },
        { id: "type", header: t("addresses.columns.type"), cell: (a) => typeLabel(a.address_type) },
        {
          id: "address",
          header: t("addresses.columns.address"),
          cell: (a) => (
            <address className="not-italic">
              {[a.address_line1, a.address_line2].filter(Boolean).join(", ")}
              <br />
              {[a.city, a.state_province, a.postal_code].filter(Boolean).join(" ")}
            </address>
          ),
        },
      ]}
      editLabel={(a) => t("addresses.editLabel", { label: a.label })}
      deleteLabel={(a) => t("addresses.deleteLabel", { label: a.label })}
      deleteTitle={t("addresses.deleteTitle")}
      deleteBody={(a) => t("addresses.deleteBody", { label: a.label })}
      deletedMessage={(a) => t("addresses.deleted", { label: a.label })}
      onDelete={(a) => orgAdminApi.deleteAddress(a.id)}
      onChanged={onChanged}
      renderForm={(editing, done) => <AddressForm orgUnit={orgUnit} editing={editing} onDone={done} />}
    />
  )
}

function AddressForm({
  orgUnit,
  editing,
  onDone,
}: {
  orgUnit: number
  editing: FacetEditing<OrgAddress>
  onDone: () => void
}) {
  const { t } = useTranslation(["orgUnits", "common"])
  const typeLabel = useAddressTypeLabel()
  const existing = editing.mode === "edit" ? editing.row : null
  const [label, setLabel] = useState(existing?.label ?? "")
  const [addressType, setAddressType] = useState(existing?.address_type ?? "physical")
  const [line1, setLine1] = useState(existing?.address_line1 ?? "")
  const [line2, setLine2] = useState(existing?.address_line2 ?? "")
  const [city, setCity] = useState(existing?.city ?? "")
  const [stateProvince, setStateProvince] = useState(existing?.state_province ?? "")
  const [postalCode, setPostalCode] = useState(existing?.postal_code ?? "")
  const [labelError, setLabelError] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [pending, setPending] = useState(false)

  // Keep an unknown existing type selectable rather than silently changing it.
  const typeOptions: string[] = [...ADDRESS_TYPES]
  if (!typeOptions.includes(addressType)) typeOptions.push(addressType)

  const submit = async (e: FormEvent) => {
    e.preventDefault()
    setPending(true)
    setLabelError(null)
    setError(null)
    const fields = {
      label: label.trim(),
      address_type: addressType,
      address_line1: line1.trim(),
      address_line2: line2.trim(),
      city: city.trim(),
      state_province: stateProvince.trim(),
      postal_code: postalCode.trim(),
    }
    try {
      const saved = existing
        ? await orgAdminApi.updateAddress(existing.id, fields)
        : await orgAdminApi.createAddress({ org_unit: orgUnit, ...fields })
      toast.success(t("addresses.saved", { label: saved.label }))
      onDone()
    } catch (err) {
      if (err instanceof ApiRequestError && err.code === "ADDRESS_LABEL_TAKEN") {
        setLabelError(t("addresses.labelTaken"))
      } else {
        setError(describeError(err))
      }
      setPending(false)
    }
  }

  const text = (id: string, label: string, value: string, set: (v: string) => void, autoComplete: string) => (
    <Field>
      <FieldLabel htmlFor={id}>{label}</FieldLabel>
      <Input id={id} value={value} autoComplete={autoComplete} onChange={(e) => set(e.target.value)} />
    </Field>
  )

  return (
    <form onSubmit={submit} className="contents">
      <DialogHeader>
        <DialogTitle>{existing ? t("addresses.editTitle") : t("addresses.createTitle")}</DialogTitle>
      </DialogHeader>
      <FieldGroup>
        <div className="grid gap-4 sm:grid-cols-[1fr_10rem]">
          <Field data-invalid={labelError !== null}>
            <FieldLabel htmlFor="address-label">{t("addresses.label")}</FieldLabel>
            <Input
              id="address-label"
              required
              autoFocus
              value={label}
              placeholder={t("addresses.labelPlaceholder")}
              aria-invalid={labelError !== null}
              onChange={(e) => setLabel(e.target.value)}
            />
            {labelError && <FieldError>{labelError}</FieldError>}
          </Field>
          <Field>
            <FieldLabel htmlFor="address-type">{t("addresses.type")}</FieldLabel>
            <Select value={addressType} onValueChange={setAddressType}>
              <SelectTrigger id="address-type" className="w-full">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {typeOptions.map((type) => (
                  <SelectItem key={type} value={type}>
                    {typeLabel(type)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </Field>
        </div>
        {text("address-line1", t("addresses.line1"), line1, setLine1, "address-line1")}
        {text("address-line2", t("addresses.line2"), line2, setLine2, "address-line2")}
        <div className="grid gap-4 sm:grid-cols-[1fr_8rem_8rem]">
          {text("address-city", t("addresses.city"), city, setCity, "address-level2")}
          {text("address-state", t("addresses.stateProvince"), stateProvince, setStateProvince, "address-level1")}
          {text("address-postal", t("addresses.postalCode"), postalCode, setPostalCode, "postal-code")}
        </div>
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
