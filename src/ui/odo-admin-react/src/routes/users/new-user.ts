export type AuthMethod = "local" | "saml"

export type NewUserForm = {
  authMethod: AuthMethod
  username: string
  email: string
  firstName: string
  middleName: string
  familyName: string
  password: string
  confirmPassword: string
}

export type PasswordProblem = "passwordShort" | "passwordMismatch"
export type NewUserProblem = "usernameSpaces" | PasswordProblem

/** The server's minimum (odo-auth create_user). */
export const MIN_PASSWORD_LENGTH = 8

/** A new password and its confirmation, as the server and people need them. */
export function passwordProblems(password: string, confirm: string): Set<PasswordProblem> {
  const problems = new Set<PasswordProblem>()
  if (password.length < MIN_PASSWORD_LENGTH) problems.add("passwordShort")
  if (password !== confirm) problems.add("passwordMismatch")
  return problems
}

/**
 * Problems the server would reject, checked before sending. The
 * password check matters beyond convenience: odo-auth creates the
 * account before validating the password, so a short one leaves a
 * passwordless account behind (see TODO.md).
 */
export function newUserProblems(form: NewUserForm): Set<NewUserProblem> {
  const problems = new Set<NewUserProblem>()
  if (/\s/.test(form.username.trim())) problems.add("usernameSpaces")
  if (form.authMethod === "local") {
    for (const p of passwordProblems(form.password, form.confirmPassword)) problems.add(p)
  }
  return problems
}

/** The create request for a form that has no problems. */
export function newUserRequest(form: NewUserForm) {
  return {
    auth_method: form.authMethod,
    username: form.username.trim(),
    email: form.email.trim(),
    first_given_name: form.firstName.trim() || null,
    second_given_name: form.middleName.trim() || null,
    family_name: form.familyName.trim() || null,
    // SAML accounts are refused if they carry a password at all.
    ...(form.authMethod === "local" ? { password: form.password } : {}),
  }
}
