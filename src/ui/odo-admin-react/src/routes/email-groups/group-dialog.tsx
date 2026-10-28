import { useState, type FormEvent } from "react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"

import { ApiRequestError } from "@/api/client"
import { describeError } from "@/api/errors"
import { emailGroupsApi, type EmailGroupRow } from "@/api/notify"
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

/**
 * Create a group (`group` null) or edit one's code and label. Closed
 * when `group` is undefined.
 */
export function GroupDialog({
  group,
  onOpenChange,
  onSaved,
}: {
  group: EmailGroupRow | null | undefined
  onOpenChange: (open: boolean) => void
  onSaved: (group: EmailGroupRow) => void
}) {
  return (
    <Dialog open={group !== undefined} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        {group !== undefined && (
          <GroupForm
            group={group}
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

function GroupForm({ group, onDone }: { group: EmailGroupRow | null; onDone: (group: EmailGroupRow) => void }) {
  const { t } = useTranslation(["emailGroups", "common"])
  const [code, setCode] = useState(group?.code ?? "")
  const [label, setLabel] = useState(group?.label ?? "")
  const [codeError, setCodeError] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [pending, setPending] = useState(false)

  const submit = async (e: FormEvent) => {
    e.preventDefault()
    setPending(true)
    setCodeError(null)
    setError(null)
    const fields = { code: code.trim(), label: label.trim() }
    try {
      const saved = group
        ? await emailGroupsApi.update({ id: group.id, ...fields })
        : await emailGroupsApi.create(fields)
      toast.success(t(group ? "form.saved" : "form.created", { label: saved.label }))
      onDone(saved)
    } catch (err) {
      if (err instanceof ApiRequestError && err.code === "EMAIL_GROUP_CODE_TAKEN") {
        setCodeError(t("common:errors.codes.EMAIL_GROUP_CODE_TAKEN"))
      } else {
        setError(describeError(err))
      }
      setPending(false)
    }
  }

  return (
    <form onSubmit={submit} className="contents">
      <DialogHeader>
        <DialogTitle>{group ? t("form.editTitle") : t("form.createTitle")}</DialogTitle>
      </DialogHeader>
      <FieldGroup>
        <Field data-invalid={codeError !== null}>
          <FieldLabel htmlFor="group-code">{t("form.code")}</FieldLabel>
          <Input
            id="group-code"
            className="font-mono"
            required
            autoFocus
            value={code}
            placeholder={t("form.codePlaceholder")}
            aria-invalid={codeError !== null}
            onChange={(e) => setCode(e.target.value)}
          />
          <FieldDescription>{t("form.codeHint")}</FieldDescription>
          {codeError && <FieldError>{codeError}</FieldError>}
        </Field>
        <Field>
          <FieldLabel htmlFor="group-label">{t("form.label")}</FieldLabel>
          <Input
            id="group-label"
            required
            value={label}
            placeholder={t("form.labelPlaceholder")}
            onChange={(e) => setLabel(e.target.value)}
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
        <Button type="submit" disabled={pending || !code.trim() || !label.trim()}>
          {group ? t("common:actions.save") : t("form.submitCreate")}
        </Button>
      </DialogFooter>
    </form>
  )
}
