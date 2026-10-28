import { useState, type FormEvent } from "react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"

import { authzApi, type RoleRow } from "@/api/authz"
import { ApiRequestError } from "@/api/client"
import { describeError } from "@/api/errors"
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

export function CreateRoleDialog({
  open,
  onOpenChange,
  onCreated,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  onCreated: (role: RoleRow) => void
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        {/* Remounting per open resets the form. */}
        {open && (
          <CreateRoleForm
            onDone={(role) => {
              onOpenChange(false)
              onCreated(role)
            }}
          />
        )}
      </DialogContent>
    </Dialog>
  )
}

function CreateRoleForm({ onDone }: { onDone: (role: RoleRow) => void }) {
  const { t } = useTranslation(["roles", "common"])
  const [code, setCode] = useState("")
  const [label, setLabel] = useState("")
  const [description, setDescription] = useState("")
  const [codeError, setCodeError] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [pending, setPending] = useState(false)

  const submit = async (e: FormEvent) => {
    e.preventDefault()
    setPending(true)
    setCodeError(null)
    setError(null)
    try {
      const role = await authzApi.createRole({
        code: code.trim(),
        label: label.trim(),
        description: description.trim(),
      })
      toast.success(t("create.created", { label: role.label }))
      onDone(role)
    } catch (err) {
      if (err instanceof ApiRequestError && err.code === "ROLE_CODE_TAKEN") {
        setCodeError(t("common:errors.codes.ROLE_CODE_TAKEN"))
      } else {
        setError(describeError(err))
      }
      setPending(false)
    }
  }

  return (
    <form onSubmit={submit} className="contents">
      <DialogHeader>
        <DialogTitle>{t("create.title")}</DialogTitle>
      </DialogHeader>
      <FieldGroup>
        <Field data-invalid={codeError !== null}>
          <FieldLabel htmlFor="role-code">{t("create.code")}</FieldLabel>
          <Input
            id="role-code"
            className="font-mono"
            required
            autoFocus
            value={code}
            placeholder={t("create.codePlaceholder")}
            aria-invalid={codeError !== null}
            onChange={(e) => setCode(e.target.value)}
          />
          <FieldDescription>{t("create.codeHint")}</FieldDescription>
          {codeError && <FieldError>{codeError}</FieldError>}
        </Field>
        <Field>
          <FieldLabel htmlFor="role-label">{t("create.label")}</FieldLabel>
          <Input
            id="role-label"
            required
            value={label}
            placeholder={t("create.labelPlaceholder")}
            onChange={(e) => setLabel(e.target.value)}
          />
        </Field>
        <Field>
          <FieldLabel htmlFor="role-description">{t("create.description")}</FieldLabel>
          <Input id="role-description" value={description} onChange={(e) => setDescription(e.target.value)} />
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
          {t("create.submit")}
        </Button>
      </DialogFooter>
    </form>
  )
}
