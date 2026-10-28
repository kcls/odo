import { useState, type FormEvent } from "react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"

import { authzApi, type RoleRow } from "@/api/authz"
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
import { Field, FieldError, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"

/** Edit a role's label and description; its code is immutable. */
export function EditRoleDialog({
  role,
  onOpenChange,
  onSaved,
}: {
  role: RoleRow | null
  onOpenChange: (open: boolean) => void
  onSaved: () => void
}) {
  return (
    <Dialog open={role !== null} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        {role && (
          <EditRoleForm
            role={role}
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

function EditRoleForm({ role, onDone }: { role: RoleRow; onDone: () => void }) {
  const { t } = useTranslation(["roles", "common"])
  const [label, setLabel] = useState(role.label)
  const [description, setDescription] = useState(role.description ?? "")
  const [error, setError] = useState<string | null>(null)
  const [pending, setPending] = useState(false)

  const submit = async (e: FormEvent) => {
    e.preventDefault()
    setPending(true)
    setError(null)
    try {
      const saved = await authzApi.updateRole({
        code: role.code,
        label: label.trim(),
        description: description.trim(),
      })
      toast.success(t("edit.saved", { label: saved.label }))
      onDone()
    } catch (err) {
      setError(describeError(err))
      setPending(false)
    }
  }

  return (
    <form onSubmit={submit} className="contents">
      <DialogHeader>
        <DialogTitle>{t("edit.title")}</DialogTitle>
      </DialogHeader>
      <FieldGroup>
        <Field>
          <FieldLabel htmlFor="role-label">{t("edit.label")}</FieldLabel>
          <Input id="role-label" required autoFocus value={label} onChange={(e) => setLabel(e.target.value)} />
        </Field>
        <Field>
          <FieldLabel htmlFor="role-description">{t("edit.description")}</FieldLabel>
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
        <Button type="submit" disabled={pending || !label.trim()}>
          {t("common:actions.save")}
        </Button>
      </DialogFooter>
    </form>
  )
}
