import { useState, type FormEvent } from "react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"

import { ApiRequestError } from "@/api/client"
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
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
  FieldLegend,
  FieldSet,
} from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group"
import { newUserProblems, newUserRequest, passwordProblems, type AuthMethod, type NewUserForm } from "./new-user"
import { PasswordFields } from "./password-fields"

export function CreateUserDialog({
  open,
  onOpenChange,
  onCreated,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  onCreated: (user: UserAccount) => void
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        {/* Remounting per open resets the form, password included. */}
        {open && (
          <CreateUserForm
            onDone={(user) => {
              onOpenChange(false)
              onCreated(user)
            }}
          />
        )}
      </DialogContent>
    </Dialog>
  )
}

const EMPTY: NewUserForm = {
  authMethod: "local",
  username: "",
  email: "",
  firstName: "",
  middleName: "",
  familyName: "",
  password: "",
  confirmPassword: "",
}

type FieldErrors = { username?: string; email?: string }

function CreateUserForm({ onDone }: { onDone: (user: UserAccount) => void }) {
  const { t } = useTranslation(["users", "common"])
  const [form, setForm] = useState(EMPTY)
  // Problems show once the user has tried to submit, not while typing.
  const [attempted, setAttempted] = useState(false)
  const [fieldErrors, setFieldErrors] = useState<FieldErrors>({})
  const [error, setError] = useState<string | null>(null)
  const [pending, setPending] = useState(false)

  const set = <K extends keyof NewUserForm>(key: K) => (value: NewUserForm[K]) => {
    setForm((f) => ({ ...f, [key]: value }))
    if (key === "username" || key === "email") setFieldErrors((e) => ({ ...e, [key]: undefined }))
  }
  const problems = newUserProblems(form)
  const shown = attempted ? problems : new Set()
  const isLocal = form.authMethod === "local"

  const submit = async (e: FormEvent) => {
    e.preventDefault()
    setAttempted(true)
    if (problems.size > 0) return
    setPending(true)
    setFieldErrors({})
    setError(null)
    try {
      const user = await usersApi.create(newUserRequest(form))
      toast.success(t("create.created", { name: user.display_name || user.username }))
      onDone(user)
    } catch (err) {
      if (err instanceof ApiRequestError && err.code === "USERNAME_TAKEN") {
        setFieldErrors({ username: t("create.usernameTaken") })
      } else if (err instanceof ApiRequestError && err.code === "EMAIL_TAKEN") {
        setFieldErrors({ email: t("create.emailTaken") })
      } else {
        setError(describeError(err))
      }
      setPending(false)
    }
  }

  const usernameError = fieldErrors.username ?? (shown.has("usernameSpaces") ? t("create.usernameSpaces") : null)

  return (
    <form onSubmit={submit} className="contents">
      <DialogHeader>
        <DialogTitle>{t("create.title")}</DialogTitle>
        <DialogDescription>{t("create.description")}</DialogDescription>
      </DialogHeader>
      <FieldGroup className="max-h-[65vh] overflow-y-auto pe-1">
        <FieldSet>
          <FieldLegend variant="label">{t("create.accountType")}</FieldLegend>
          <RadioGroup value={form.authMethod} onValueChange={(v) => set("authMethod")(v as AuthMethod)}>
            {(["local", "saml"] as const).map((method) => (
              <Field key={method} orientation="horizontal">
                <RadioGroupItem value={method} id={`new-user-${method}`} />
                <FieldContent>
                  <FieldLabel htmlFor={`new-user-${method}`} className="font-normal">
                    {t(`create.${method}`)}
                  </FieldLabel>
                  <FieldDescription>{t(`create.${method}Hint`)}</FieldDescription>
                </FieldContent>
              </Field>
            ))}
          </RadioGroup>
        </FieldSet>

        <div className="grid gap-4 sm:grid-cols-2">
          <Field data-invalid={usernameError !== null}>
            <FieldLabel htmlFor="new-user-username">{t("create.username")}</FieldLabel>
            <Input
              id="new-user-username"
              required
              autoFocus
              autoComplete="off"
              spellCheck={false}
              value={form.username}
              aria-invalid={usernameError !== null}
              onChange={(e) => set("username")(e.target.value)}
            />
            {usernameError ? <FieldError>{usernameError}</FieldError> : <FieldDescription>{t("create.usernameHint")}</FieldDescription>}
          </Field>
          <Field data-invalid={fieldErrors.email !== undefined}>
            <FieldLabel htmlFor="new-user-email">{t("create.email")}</FieldLabel>
            <Input
              id="new-user-email"
              type="email"
              required
              autoComplete="off"
              value={form.email}
              aria-invalid={fieldErrors.email !== undefined}
              onChange={(e) => set("email")(e.target.value)}
            />
            {fieldErrors.email && <FieldError>{fieldErrors.email}</FieldError>}
          </Field>
        </div>

        <div className="grid gap-4 sm:grid-cols-3">
          <Field>
            <FieldLabel htmlFor="new-user-first">{t("create.firstName")}</FieldLabel>
            <Input id="new-user-first" autoComplete="off" value={form.firstName} onChange={(e) => set("firstName")(e.target.value)} />
          </Field>
          <Field>
            <FieldLabel htmlFor="new-user-middle">{t("create.middleName")}</FieldLabel>
            <Input id="new-user-middle" autoComplete="off" value={form.middleName} onChange={(e) => set("middleName")(e.target.value)} />
          </Field>
          <Field>
            <FieldLabel htmlFor="new-user-family">{t("create.familyName")}</FieldLabel>
            <Input id="new-user-family" autoComplete="off" value={form.familyName} onChange={(e) => set("familyName")(e.target.value)} />
          </Field>
        </div>

        {isLocal && (
          <PasswordFields
            idPrefix="new-user"
            password={form.password}
            confirm={form.confirmPassword}
            onPassword={set("password")}
            onConfirm={set("confirmPassword")}
            problems={attempted ? passwordProblems(form.password, form.confirmPassword) : new Set()}
            hint={t("create.passwordHint")}
          />
        )}

        {error && <FieldError>{error}</FieldError>}
      </FieldGroup>
      <DialogFooter>
        <DialogClose asChild>
          <Button type="button" variant="outline">
            {t("common:actions.cancel")}
          </Button>
        </DialogClose>
        <Button type="submit" disabled={pending || !form.username.trim() || !form.email.trim()}>
          {t("create.submit")}
        </Button>
      </DialogFooter>
    </form>
  )
}
