import { useState, type FormEvent } from "react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"

import { authzApi, type PermissionRow } from "@/api/authz"
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

type Editing = { mode: "create" } | { mode: "edit"; permission: PermissionRow }

/** Create a permission, or edit one's description (codes are immutable). */
export function PermissionDialog({
  editing,
  onOpenChange,
  onSaved,
}: {
  editing: Editing | null
  onOpenChange: (open: boolean) => void
  onSaved: (permission: PermissionRow) => void
}) {
  return (
    <Dialog open={editing !== null} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        {/* Remounting per open resets the form. */}
        {editing && (
          <PermissionForm
            editing={editing}
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

function PermissionForm({
  editing,
  onDone,
}: {
  editing: Editing
  onDone: (permission: PermissionRow) => void
}) {
  const { t } = useTranslation(["permissions", "common"])
  const existing = editing.mode === "edit" ? editing.permission : null
  const [code, setCode] = useState(existing?.code ?? "")
  const [description, setDescription] = useState(existing?.description ?? "")
  const [codeError, setCodeError] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [pending, setPending] = useState(false)

  const submit = async (e: FormEvent) => {
    e.preventDefault()
    setPending(true)
    setCodeError(null)
    setError(null)
    try {
      const params = { code: code.trim(), description: description.trim() }
      const saved = existing
        ? await authzApi.updatePermission(params)
        : await authzApi.createPermission(params)
      toast.success(t(existing ? "dialog.updated" : "dialog.created", { code: saved.code }))
      onDone(saved)
    } catch (err) {
      if (err instanceof ApiRequestError && err.code === "PERMISSION_CODE_TAKEN") {
        setCodeError(t("common:errors.codes.PERMISSION_CODE_TAKEN"))
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
        <Field data-invalid={codeError !== null} data-disabled={existing !== null}>
          <FieldLabel htmlFor="permission-code">{t("dialog.code")}</FieldLabel>
          <Input
            id="permission-code"
            className="font-mono"
            required
            autoFocus={!existing}
            disabled={existing !== null}
            value={code}
            placeholder={t("dialog.codePlaceholder")}
            aria-invalid={codeError !== null}
            onChange={(e) => setCode(e.target.value)}
          />
          {existing && <FieldDescription>{t("dialog.codeImmutable")}</FieldDescription>}
          {codeError && <FieldError>{codeError}</FieldError>}
        </Field>
        <Field>
          <FieldLabel htmlFor="permission-description">{t("dialog.description")}</FieldLabel>
          <Input
            id="permission-description"
            autoFocus={existing !== null}
            value={description}
            onChange={(e) => setDescription(e.target.value)}
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
        <Button type="submit" disabled={pending || code.trim() === ""}>
          {t("common:actions.save")}
        </Button>
      </DialogFooter>
    </form>
  )
}
