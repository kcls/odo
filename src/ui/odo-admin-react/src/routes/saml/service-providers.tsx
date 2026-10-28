import { useState, type FormEvent } from "react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"

import { ApiRequestError } from "@/api/client"
import { describeError } from "@/api/errors"
import { samlApi, type IdpRow, type SpRow } from "@/api/saml"
import { FacetSection, type FacetEditing } from "@/components/facet-section"
import { StatusBadge } from "@/components/status-badge"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { DialogClose, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Field, FieldDescription, FieldError, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Textarea } from "@/components/ui/textarea"

/** Select can't hold an empty value, so "no IdP" gets a sentinel. */
const NO_IDP = "none"

const spName = (sp: SpRow) => sp.label || sp.entity_id

/**
 * Service providers: an IdP's own, or those linked to none. New ones
 * start linked to `defaultIdp`.
 */
export function ServiceProviders({
  title,
  description,
  empty,
  sps,
  idps,
  defaultIdp,
  canWrite,
  onChanged,
}: {
  title: string
  description: string
  empty: string
  sps: SpRow[]
  idps: IdpRow[]
  defaultIdp: number | null
  canWrite: boolean
  onChanged: () => void
}) {
  const { t } = useTranslation("saml")
  return (
    <FacetSection
      title={title}
      description={description}
      newLabel={t("sps.new")}
      empty={empty}
      rows={sps}
      canWrite={canWrite}
      rowName="service provider"
      columns={[
        {
          id: "label",
          header: t("sps.columns.label"),
          cell: (sp) => (
            <div className="flex flex-col">
              <span className="font-medium">{sp.label}</span>
              <span className="font-mono text-xs break-all text-muted-foreground">{sp.entity_id}</span>
            </div>
          ),
        },
        {
          id: "acsUrl",
          header: t("sps.columns.acsUrl"),
          cell: (sp) => <span className="font-mono text-xs break-all">{sp.acs_url}</span>,
        },
        {
          id: "status",
          header: t("sps.columns.status"),
          cell: (sp) =>
            sp.is_active ? (
              <StatusBadge tone="success">{t("active")}</StatusBadge>
            ) : (
              <StatusBadge tone="neutral">{t("inactive")}</StatusBadge>
            ),
        },
      ]}
      editLabel={(sp) => t("sps.editLabel", { name: spName(sp) })}
      deleteLabel={(sp) => t("sps.deleteLabel", { name: spName(sp) })}
      deleteTitle={t("sps.deleteTitle")}
      deleteBody={(sp) => t("sps.deleteBody", { name: spName(sp) })}
      deletedMessage={(sp) => t("sps.deleted", { name: spName(sp) })}
      onDelete={(sp) => samlApi.deleteSp(sp.id)}
      onChanged={onChanged}
      renderForm={(editing, done) => (
        <SpForm editing={editing} idps={idps} defaultIdp={defaultIdp} onDone={done} />
      )}
    />
  )
}

function SpForm({
  editing,
  idps,
  defaultIdp,
  onDone,
}: {
  editing: FacetEditing<SpRow>
  idps: IdpRow[]
  defaultIdp: number | null
  onDone: () => void
}) {
  const { t } = useTranslation(["saml", "common"])
  const existing = editing.mode === "edit" ? editing.row : null
  const initialIdp = existing ? existing.idp : defaultIdp
  const [label, setLabel] = useState(existing?.label ?? "")
  const [entityId, setEntityId] = useState(existing?.entity_id ?? "")
  const [acsUrl, setAcsUrl] = useState(existing?.acs_url ?? "")
  const [sloUrl, setSloUrl] = useState(existing?.slo_url ?? "")
  const [callbackUrl, setCallbackUrl] = useState(existing?.callback_url ?? "")
  const [idp, setIdp] = useState(initialIdp != null ? String(initialIdp) : NO_IDP)
  const [isActive, setIsActive] = useState(existing?.is_active ?? true)
  const [cert, setCert] = useState(existing?.idp_x509_cert ?? "")
  const [entityIdError, setEntityIdError] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [pending, setPending] = useState(false)

  // The API treats a null idp as "unchanged", so a linked SP can't be
  // unlinked; don't offer "None" once it has one.
  const idpPermanent = existing?.idp != null

  const submit = async (e: FormEvent) => {
    e.preventDefault()
    setPending(true)
    setEntityIdError(null)
    setError(null)
    const fields = {
      label: label.trim(),
      entity_id: entityId.trim(),
      acs_url: acsUrl.trim(),
      slo_url: sloUrl.trim(),
      callback_url: callbackUrl.trim(),
      is_active: isActive,
      idp_x509_cert: cert.trim(),
      idp: idp === NO_IDP ? null : Number(idp),
    }
    try {
      const saved = existing ? await samlApi.updateSp(existing.id, fields) : await samlApi.createSp(fields)
      toast.success(t("sps.saved", { name: spName(saved) }))
      onDone()
    } catch (err) {
      if (err instanceof ApiRequestError && err.code === "ENTITY_ID_TAKEN") {
        setEntityIdError(t("common:errors.codes.ENTITY_ID_TAKEN"))
      } else {
        setError(describeError(err))
      }
      setPending(false)
    }
  }

  const url = (id: string, labelText: string, value: string, set: (v: string) => void, required = false) => (
    <Field>
      <FieldLabel htmlFor={id}>{labelText}</FieldLabel>
      <Input
        id={id}
        type="url"
        className="font-mono text-xs"
        required={required}
        value={value}
        onChange={(e) => set(e.target.value)}
      />
    </Field>
  )

  return (
    <form onSubmit={submit} className="contents">
      <DialogHeader>
        <DialogTitle>{existing ? t("sps.editTitle") : t("sps.createTitle")}</DialogTitle>
      </DialogHeader>
      <FieldGroup className="max-h-[65vh] overflow-y-auto pe-1">
        <Field>
          <FieldLabel htmlFor="sp-label">{t("sps.label")}</FieldLabel>
          <Input id="sp-label" autoFocus value={label} onChange={(e) => setLabel(e.target.value)} />
        </Field>
        <Field data-invalid={entityIdError !== null}>
          <FieldLabel htmlFor="sp-entity-id">{t("sps.entityId")}</FieldLabel>
          <Input
            id="sp-entity-id"
            className="font-mono text-xs"
            required
            value={entityId}
            aria-invalid={entityIdError !== null}
            onChange={(e) => setEntityId(e.target.value)}
          />
          {entityIdError && <FieldError>{entityIdError}</FieldError>}
        </Field>
        {url("sp-acs-url", t("sps.acsUrl"), acsUrl, setAcsUrl, true)}
        {url("sp-slo-url", t("sps.sloUrl"), sloUrl, setSloUrl)}
        {url("sp-callback-url", t("sps.callbackUrl"), callbackUrl, setCallbackUrl)}
        <Field>
          <FieldLabel htmlFor="sp-idp">{t("sps.idp")}</FieldLabel>
          <Select value={idp} onValueChange={setIdp}>
            <SelectTrigger id="sp-idp" className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {!idpPermanent && <SelectItem value={NO_IDP}>{t("sps.idpNone")}</SelectItem>}
              {idps.map((i) => (
                <SelectItem key={i.id} value={String(i.id)}>
                  {i.name}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          {idpPermanent && <FieldDescription>{t("sps.idpPermanent")}</FieldDescription>}
        </Field>
        <Field>
          <FieldLabel htmlFor="sp-cert">{t("sps.cert")}</FieldLabel>
          <Textarea
            id="sp-cert"
            className="min-h-28 font-mono text-xs"
            spellCheck={false}
            value={cert}
            onChange={(e) => setCert(e.target.value)}
          />
          <FieldDescription>{t("sps.certHint")}</FieldDescription>
        </Field>
        <Field orientation="horizontal">
          <Checkbox id="sp-active" checked={isActive} onCheckedChange={(v) => setIsActive(v === true)} />
          <FieldLabel htmlFor="sp-active" className="font-normal">
            {t("sps.isActive")}
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
        <Button type="submit" disabled={pending || !entityId.trim() || !acsUrl.trim()}>
          {t("common:actions.save")}
        </Button>
      </DialogFooter>
    </form>
  )
}
