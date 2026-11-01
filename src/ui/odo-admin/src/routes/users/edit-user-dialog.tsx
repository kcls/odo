import { useState, type FormEvent } from "react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"

import { describeError } from "@/api/errors"
import { usersApi, type UserAccount } from "@/api/users"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Field, FieldError, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"

/** Edit a local user's name. A blank field clears that name. */
export function EditUserDialog({
  user,
  onOpenChange,
  onSaved,
}: {
  user: UserAccount | null
  onOpenChange: (open: boolean) => void
  onSaved: (user: UserAccount) => void
}) {
  return (
    <Dialog open={user !== null} onOpenChange={onOpenChange}>
      <DialogContent>
        {user && (
          <EditUserForm
            user={user}
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

function EditUserForm({ user, onDone }: { user: UserAccount; onDone: (user: UserAccount) => void }) {
  const { t } = useTranslation(["users", "common"])
  const [firstName, setFirstName] = useState(user.first_given_name ?? "")
  const [middleName, setMiddleName] = useState(user.second_given_name ?? "")
  const [familyName, setFamilyName] = useState(user.family_name ?? "")
  const [error, setError] = useState<string | null>(null)
  const [pending, setPending] = useState(false)

  const submit = async (e: FormEvent) => {
    e.preventDefault()
    setPending(true)
    setError(null)
    try {
      const saved = await usersApi.update(user.id, {
        first_given_name: firstName.trim(),
        second_given_name: middleName.trim(),
        family_name: familyName.trim(),
      })
      toast.success(t("edit.saved", { name: saved.display_name }))
      onDone(saved)
    } catch (err) {
      setError(describeError(err))
      setPending(false)
    }
  }

  return (
    <form onSubmit={submit} className="contents">
      <DialogHeader>
        <DialogTitle>{t("edit.title")}</DialogTitle>
        <DialogDescription>{t("edit.description")}</DialogDescription>
      </DialogHeader>
      <FieldGroup>
        <Field>
          <FieldLabel htmlFor="user-first-name">{t("edit.firstName")}</FieldLabel>
          <Input id="user-first-name" autoFocus value={firstName} onChange={(e) => setFirstName(e.target.value)} />
        </Field>
        <Field>
          <FieldLabel htmlFor="user-middle-name">{t("edit.middleName")}</FieldLabel>
          <Input id="user-middle-name" value={middleName} onChange={(e) => setMiddleName(e.target.value)} />
        </Field>
        <Field>
          <FieldLabel htmlFor="user-family-name">{t("edit.familyName")}</FieldLabel>
          <Input id="user-family-name" value={familyName} onChange={(e) => setFamilyName(e.target.value)} />
        </Field>
        {error && <FieldError>{error}</FieldError>}
      </FieldGroup>
      <DialogFooter>
        <DialogClose asChild>
          <Button type="button" variant="outline">
            {t("common:actions.cancel")}
          </Button>
        </DialogClose>
        <Button type="submit" disabled={pending}>
          {t("common:actions.save")}
        </Button>
      </DialogFooter>
    </form>
  )
}
