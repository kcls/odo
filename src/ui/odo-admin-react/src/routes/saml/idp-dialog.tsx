import { useState, type FormEvent } from "react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"

import { ApiRequestError } from "@/api/client"
import { describeError } from "@/api/errors"
import { samlApi, type IdpRow } from "@/api/saml"
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
import { Field, FieldDescription, FieldError, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Textarea } from "@/components/ui/textarea"
import { parseJsonText } from "@/lib/json-text"

/** Create an IdP (`idp` null) or edit one. Closed when `idp` is undefined. */
export function IdpDialog({
  idp,
  onOpenChange,
  onSaved,
}: {
  idp: IdpRow | null | undefined
  onOpenChange: (open: boolean) => void
  onSaved: (idp: IdpRow) => void
}) {
  return (
    <Dialog open={idp !== undefined} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-xl">
        {idp !== undefined && (
          <IdpForm
            idp={idp}
            onDone={(saved) => {
              onOpenChange(false)
              onSaved(saved)
            }}
          />
        )}
      </DialogContent>
    </Dialog>
  )
}

function IdpForm({ idp, onDone }: { idp: IdpRow | null; onDone: (idp: IdpRow) => void }) {
  const { t } = useTranslation(["saml", "common"])
  const [name, setName] = useState(idp?.name ?? "")
  const [entityId, setEntityId] = useState(idp?.entity_id ?? "")
  const [ssoUrl, setSsoUrl] = useState(idp?.sso_url ?? "")
  const [sloUrl, setSloUrl] = useState(idp?.slo_url ?? "")
  const [metadataUrl, setMetadataUrl] = useState(idp?.metadata_url ?? "")
  const [hours, setHours] = useState(idp?.session_lifetime_hours != null ? String(idp.session_lifetime_hours) : "")
  const [allowIdpInitiated, setAllowIdpInitiated] = useState(idp?.allow_idp_initiated ?? false)
  const [isActive, setIsActive] = useState(idp?.is_active ?? true)
  const [mapping, setMapping] = useState(
    idp?.attribute_mapping != null ? JSON.stringify(idp.attribute_mapping, null, 2) : ""
  )
  const [entityIdError, setEntityIdError] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [pending, setPending] = useState(false)

  // Null means "unchanged" to the API, so neither can be cleared once
  // set; the form requires a value instead of offering a blank that
  // would silently do nothing.
  const hoursPermanent = idp?.session_lifetime_hours != null
  const mappingPermanent = idp?.attribute_mapping != null

  const hoursNumber = Number(hours)
  const hoursValid = hours.trim() === "" ? !hoursPermanent : Number.isInteger(hoursNumber) && hoursNumber >= 1
  const parsedMapping = parseJsonText(mapping)
  const mappingValid = parsedMapping.ok && (!mappingPermanent || parsedMapping.value !== undefined)

  const submit = async (e: FormEvent) => {
    e.preventDefault()
    if (!hoursValid || !parsedMapping.ok || !mappingValid) return
    setPending(true)
    setEntityIdError(null)
    setError(null)
    const fields = {
      name: name.trim(),
      entity_id: entityId.trim(),
      sso_url: ssoUrl.trim(),
      slo_url: sloUrl.trim(),
      metadata_url: metadataUrl.trim(),
      is_active: isActive,
      allow_idp_initiated: allowIdpInitiated,
      ...(hours.trim() !== "" ? { session_lifetime_hours: hoursNumber } : {}),
      ...(parsedMapping.value !== undefined ? { attribute_mapping: parsedMapping.value } : {}),
    }
    try {
      const saved = idp ? await samlApi.updateIdp(idp.id, fields) : await samlApi.createIdp(fields)
      toast.success(t(idp ? "idpForm.saved" : "idpForm.created", { name: saved.name }))
      onDone(saved)
    } catch (err) {
      if (err instanceof ApiRequestError && err.code === "ENTITY_ID_TAKEN") {
        setEntityIdError(t("common:errors.codes.ENTITY_ID_TAKEN"))
      } else {
        setError(describeError(err))
      }
      setPending(false)
    }
  }

  const url = (id: string, labelText: string, value: string, set: (v: string) => void) => (
    <Field>
      <FieldLabel htmlFor={id}>{labelText}</FieldLabel>
      <Input id={id} type="url" className="font-mono text-xs" value={value} onChange={(e) => set(e.target.value)} />
    </Field>
  )

  return (
    <form onSubmit={submit} className="contents">
      <DialogHeader>
        <DialogTitle>{idp ? t("idpForm.editTitle") : t("idpForm.createTitle")}</DialogTitle>
      </DialogHeader>
      <FieldGroup className="max-h-[65vh] overflow-y-auto pe-1">
        <Field>
          <FieldLabel htmlFor="idp-name">{t("idpForm.name")}</FieldLabel>
          <Input id="idp-name" required autoFocus value={name} onChange={(e) => setName(e.target.value)} />
        </Field>
        <Field data-invalid={entityIdError !== null}>
          <FieldLabel htmlFor="idp-entity-id">{t("idpForm.entityId")}</FieldLabel>
          <Input
            id="idp-entity-id"
            className="font-mono text-xs"
            required
            value={entityId}
            aria-invalid={entityIdError !== null}
            onChange={(e) => setEntityId(e.target.value)}
          />
          {entityIdError && <FieldError>{entityIdError}</FieldError>}
        </Field>
        {url("idp-sso-url", t("idpForm.ssoUrl"), ssoUrl, setSsoUrl)}
        {url("idp-slo-url", t("idpForm.sloUrl"), sloUrl, setSloUrl)}
        {url("idp-metadata-url", t("idpForm.metadataUrl"), metadataUrl, setMetadataUrl)}
        <Field data-invalid={!hoursValid}>
          <FieldLabel htmlFor="idp-hours">{t("idpForm.sessionLifetime")}</FieldLabel>
          <Input
            id="idp-hours"
            type="number"
            inputMode="numeric"
            min={1}
            step={1}
            className="w-28"
            required={hoursPermanent}
            value={hours}
            aria-invalid={!hoursValid}
            onChange={(e) => setHours(e.target.value)}
          />
          {!hoursValid && hours.trim() !== "" ? (
            <FieldError>{t("idpForm.invalidHours")}</FieldError>
          ) : (
            <FieldDescription>
              {hoursPermanent ? t("idpForm.sessionLifetimePermanent") : t("idpForm.sessionLifetimeHint")}
            </FieldDescription>
          )}
        </Field>
        <Field orientation="horizontal">
          <Checkbox
            id="idp-initiated"
            checked={allowIdpInitiated}
            onCheckedChange={(v) => setAllowIdpInitiated(v === true)}
          />
          <FieldLabel htmlFor="idp-initiated" className="font-normal">
            {t("idpForm.allowIdpInitiated")}
          </FieldLabel>
        </Field>
        <Field orientation="horizontal">
          <Checkbox id="idp-active" checked={isActive} onCheckedChange={(v) => setIsActive(v === true)} />
          <FieldLabel htmlFor="idp-active" className="font-normal">
            {t("idpForm.isActive")}
          </FieldLabel>
        </Field>
        <Field data-invalid={!mappingValid}>
          <FieldLabel htmlFor="idp-mapping">{t("idpForm.attributeMapping")}</FieldLabel>
          <Textarea
            id="idp-mapping"
            className="min-h-28 font-mono text-xs"
            spellCheck={false}
            required={mappingPermanent}
            value={mapping}
            aria-invalid={!mappingValid}
            onChange={(e) => setMapping(e.target.value)}
          />
          {!parsedMapping.ok ? (
            <FieldError>{t("idpForm.invalidJson")}</FieldError>
          ) : (
            mappingPermanent && <FieldDescription>{t("idpForm.attributeMappingPermanent")}</FieldDescription>
          )}
        </Field>
        {error && <FieldError>{error}</FieldError>}
      </FieldGroup>
      <DialogFooter>
        <DialogClose asChild>
          <Button type="button" variant="outline">
            {t("common:actions.cancel")}
          </Button>
        </DialogClose>
        <Button type="submit" disabled={pending || !name.trim() || !entityId.trim() || !hoursValid || !mappingValid}>
          {t("common:actions.save")}
        </Button>
      </DialogFooter>
    </form>
  )
}
