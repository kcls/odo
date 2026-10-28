import { EyeIcon, EyeOffIcon } from "lucide-react"
import { useState } from "react"
import { useTranslation } from "react-i18next"

import { Field, FieldDescription, FieldError, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { InputGroup, InputGroupAddon, InputGroupButton, InputGroupInput } from "@/components/ui/input-group"
import type { PasswordProblem } from "./new-user"

/**
 * A new password and its confirmation, with a show/hide toggle. For
 * setting someone else's password: browsers are told it's a new one, so
 * password managers don't offer the admin's own.
 */
export function PasswordFields({
  idPrefix,
  password,
  confirm,
  onPassword,
  onConfirm,
  problems,
  hint,
  autoFocus = false,
}: {
  idPrefix: string
  password: string
  confirm: string
  onPassword: (value: string) => void
  onConfirm: (value: string) => void
  /** The problems to show now (empty until the user tries to submit). */
  problems: ReadonlySet<PasswordProblem>
  hint: string
  autoFocus?: boolean
}) {
  const { t } = useTranslation("users")
  const [show, setShow] = useState(false)
  const type = show ? "text" : "password"
  return (
    <div className="grid gap-4 sm:grid-cols-2">
      <Field data-invalid={problems.has("passwordShort")}>
        <FieldLabel htmlFor={`${idPrefix}-password`}>{t("create.password")}</FieldLabel>
        <InputGroup>
          <InputGroupInput
            id={`${idPrefix}-password`}
            type={type}
            required
            autoFocus={autoFocus}
            autoComplete="new-password"
            value={password}
            aria-invalid={problems.has("passwordShort")}
            onChange={(e) => onPassword(e.target.value)}
          />
          <InputGroupAddon align="inline-end">
            <InputGroupButton
              size="icon-xs"
              aria-label={show ? t("create.hidePassword") : t("create.showPassword")}
              aria-pressed={show}
              onClick={() => setShow((s) => !s)}
            >
              {show ? <EyeOffIcon /> : <EyeIcon />}
            </InputGroupButton>
          </InputGroupAddon>
        </InputGroup>
        {problems.has("passwordShort") && <FieldError>{t("create.passwordShort")}</FieldError>}
      </Field>
      <Field data-invalid={problems.has("passwordMismatch")}>
        <FieldLabel htmlFor={`${idPrefix}-confirm`}>{t("create.confirmPassword")}</FieldLabel>
        <Input
          id={`${idPrefix}-confirm`}
          type={type}
          required
          autoComplete="new-password"
          value={confirm}
          aria-invalid={problems.has("passwordMismatch")}
          onChange={(e) => onConfirm(e.target.value)}
        />
        {problems.has("passwordMismatch") && <FieldError>{t("create.passwordMismatch")}</FieldError>}
      </Field>
      <FieldDescription className="sm:col-span-2">{hint}</FieldDescription>
    </div>
  )
}
