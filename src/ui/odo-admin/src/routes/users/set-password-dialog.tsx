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
import { FieldError, FieldGroup } from "@/components/ui/field"
import { passwordProblems } from "./new-user"
import { PasswordFields } from "./password-fields"

/** Set a local user's password; they are signed out everywhere. */
export function SetPasswordDialog({
  user,
  onOpenChange,
  onDone,
}: {
  user: UserAccount | null
  onOpenChange: (open: boolean) => void
  onDone: () => void
}) {
  return (
    <Dialog open={user !== null} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        {/* Remounting per open clears the password. */}
        {user && (
          <SetPasswordForm
            user={user}
            onDone={() => {
              onOpenChange(false)
              onDone()
            }}
          />
        )}
      </DialogContent>
    </Dialog>
  )
}

function SetPasswordForm({ user, onDone }: { user: UserAccount; onDone: () => void }) {
  const { t } = useTranslation(["users", "common"])
  const [password, setPassword] = useState("")
  const [confirm, setConfirm] = useState("")
  const [attempted, setAttempted] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [pending, setPending] = useState(false)
  const problems = passwordProblems(password, confirm)
  const name = user.display_name || user.username || user.email

  const submit = async (e: FormEvent) => {
    e.preventDefault()
    setAttempted(true)
    if (problems.size > 0) return
    setPending(true)
    setError(null)
    try {
      const result = await usersApi.setPassword(user.id, password)
      toast.success(
        result.sessions_revoked > 0
          ? t("setPassword.doneSessions", { name, count: result.sessions_revoked })
          : t("setPassword.done", { name })
      )
      onDone()
    } catch (err) {
      setError(describeError(err))
      setPending(false)
    }
  }

  return (
    <form onSubmit={submit} className="contents">
      <DialogHeader>
        <DialogTitle>{t("setPassword.title")}</DialogTitle>
        <DialogDescription>{t("setPassword.description", { name })}</DialogDescription>
      </DialogHeader>
      <FieldGroup>
        <PasswordFields
          idPrefix="set-password"
          password={password}
          confirm={confirm}
          onPassword={setPassword}
          onConfirm={setConfirm}
          problems={attempted ? problems : new Set()}
          hint={t("setPassword.hint")}
          autoFocus
        />
        {error && <FieldError>{error}</FieldError>}
      </FieldGroup>
      <DialogFooter>
        <DialogClose asChild>
          <Button type="button" variant="outline">
            {t("common:actions.cancel")}
          </Button>
        </DialogClose>
        <Button type="submit" disabled={pending}>
          {t("setPassword.submit")}
        </Button>
      </DialogFooter>
    </form>
  )
}
