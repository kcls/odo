import { useState, type FormEvent } from "react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"

import { ApiRequestError } from "@/api/client"
import { describeError } from "@/api/errors"
import { NORMALIZERS, samlApi, type AttributeRow, type Normalizer } from "@/api/saml"
import { FacetSection, type FacetEditing } from "@/components/facet-section"
import { StatusBadge } from "@/components/status-badge"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { DialogClose, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Field, FieldError, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"

/** Select can't hold an empty value; the API's "no normalizer" is "". */
const NO_NORMALIZER = "none"

function useNormalizerLabel() {
  const { t } = useTranslation("saml")
  return (n: string | null | undefined) =>
    !n
      ? t("attributes.normalizerNone")
      : (NORMALIZERS as readonly string[]).includes(n)
        ? t(`attributes.normalizers.${n as Normalizer}`)
        : n
}

/** The assertion attributes recorded for one IdP. */
export function Attributes({
  idpId,
  attributes,
  canWrite,
  onChanged,
}: {
  idpId: number
  attributes: AttributeRow[]
  canWrite: boolean
  onChanged: () => void
}) {
  const { t } = useTranslation("saml")
  const normalizerLabel = useNormalizerLabel()
  return (
    <FacetSection
      title={t("attributes.title")}
      description={t("attributes.description")}
      newLabel={t("attributes.new")}
      empty={t("attributes.none")}
      rows={attributes}
      canWrite={canWrite}
      rowName="attribute"
      columns={[
        { id: "key", header: t("attributes.columns.key"), cell: (a) => <span className="font-mono text-xs">{a.key}</span> },
        { id: "label", header: t("attributes.columns.label"), cell: (a) => a.label },
        {
          id: "normalizer",
          header: t("attributes.columns.normalizer"),
          cell: (a) => <span className={a.normalizer ? undefined : "text-muted-foreground"}>{normalizerLabel(a.normalizer)}</span>,
        },
        {
          id: "usedFor",
          header: t("attributes.columns.usedFor"),
          cell: (a) => (
            <div className="flex flex-wrap gap-1">
              {a.is_location && <StatusBadge tone="info">{t("attributes.location")}</StatusBadge>}
              {a.mapping_count > 0 && (
                <StatusBadge tone="info">{t("attributes.mappingCount", { count: a.mapping_count })}</StatusBadge>
              )}
            </div>
          ),
        },
      ]}
      editLabel={(a) => t("attributes.editLabel", { key: a.key })}
      deleteLabel={(a) => t("attributes.deleteLabel", { key: a.key })}
      deleteBlockedReason={(a) =>
        a.mapping_count > 0 ? t("attributes.deleteBlocked", { count: a.mapping_count }) : null
      }
      deleteTitle={t("attributes.deleteTitle")}
      deleteBody={(a) => t("attributes.deleteBody", { key: a.key })}
      deletedMessage={(a) => t("attributes.deleted", { key: a.key })}
      onDelete={(a) => samlApi.deleteAttribute(a.id)}
      onChanged={onChanged}
      renderForm={(editing, done) => <AttributeForm idpId={idpId} editing={editing} onDone={done} />}
    />
  )
}

function AttributeForm({
  idpId,
  editing,
  onDone,
}: {
  idpId: number
  editing: FacetEditing<AttributeRow>
  onDone: () => void
}) {
  const { t } = useTranslation(["saml", "common"])
  const normalizerLabel = useNormalizerLabel()
  const existing = editing.mode === "edit" ? editing.row : null
  const [key, setKey] = useState(existing?.key ?? "")
  const [label, setLabel] = useState(existing?.label ?? "")
  const [normalizer, setNormalizer] = useState(existing?.normalizer || NO_NORMALIZER)
  const [isLocation, setIsLocation] = useState(existing?.is_location ?? false)
  const [keyError, setKeyError] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [pending, setPending] = useState(false)

  // Keep an unknown stored normalizer selectable rather than dropping it.
  const options: string[] = [...NORMALIZERS]
  if (normalizer !== NO_NORMALIZER && !options.includes(normalizer)) options.push(normalizer)

  const submit = async (e: FormEvent) => {
    e.preventDefault()
    setPending(true)
    setKeyError(null)
    setError(null)
    const fields = {
      key: key.trim(),
      label: label.trim(),
      is_location: isLocation,
      // An empty string clears it on update.
      normalizer: normalizer === NO_NORMALIZER ? "" : normalizer,
    }
    try {
      const saved = existing
        ? await samlApi.updateAttribute(existing.id, fields)
        : await samlApi.createAttribute({ idp: idpId, ...fields })
      toast.success(t("attributes.saved", { key: saved.key }))
      onDone()
    } catch (err) {
      if (err instanceof ApiRequestError && err.code === "ATTRIBUTE_EXISTS") {
        setKeyError(t("common:errors.codes.ATTRIBUTE_EXISTS"))
      } else {
        setError(describeError(err))
      }
      setPending(false)
    }
  }

  return (
    <form onSubmit={submit} className="contents">
      <DialogHeader>
        <DialogTitle>{existing ? t("attributes.editTitle") : t("attributes.createTitle")}</DialogTitle>
      </DialogHeader>
      <FieldGroup>
        <Field data-invalid={keyError !== null}>
          <FieldLabel htmlFor="attr-key">{t("attributes.key")}</FieldLabel>
          <Input
            id="attr-key"
            className="font-mono"
            required
            autoFocus
            value={key}
            placeholder={t("attributes.keyPlaceholder")}
            aria-invalid={keyError !== null}
            onChange={(e) => setKey(e.target.value)}
          />
          {keyError && <FieldError>{keyError}</FieldError>}
        </Field>
        <Field>
          <FieldLabel htmlFor="attr-label">{t("attributes.label")}</FieldLabel>
          <Input id="attr-label" required value={label} onChange={(e) => setLabel(e.target.value)} />
        </Field>
        <Field>
          <FieldLabel htmlFor="attr-normalizer">{t("attributes.normalizer")}</FieldLabel>
          <Select value={normalizer} onValueChange={setNormalizer}>
            <SelectTrigger id="attr-normalizer" className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value={NO_NORMALIZER}>{t("attributes.normalizerNone")}</SelectItem>
              {options.map((n) => (
                <SelectItem key={n} value={n}>
                  {normalizerLabel(n)}
                  <span className="font-mono text-xs text-muted-foreground">{n}</span>
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </Field>
        <Field orientation="horizontal">
          <Checkbox id="attr-location" checked={isLocation} onCheckedChange={(v) => setIsLocation(v === true)} />
          <FieldLabel htmlFor="attr-location" className="font-normal">
            {t("attributes.isLocation")}
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
        <Button type="submit" disabled={pending || !key.trim() || !label.trim()}>
          {t("common:actions.save")}
        </Button>
      </DialogFooter>
    </form>
  )
}
