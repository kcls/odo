import { describe, expect, it } from "vitest"

import { newUserProblems, newUserRequest, type NewUserForm } from "./new-user"

const form = (patch: Partial<NewUserForm> = {}): NewUserForm => ({
  authMethod: "local",
  username: "ada",
  email: "ada@example.org",
  firstName: "Ada",
  middleName: "",
  familyName: "Lovelace",
  password: "correct horse",
  confirmPassword: "correct horse",
  ...patch,
})

describe("newUserProblems", () => {
  it("accepts a complete local account", () => {
    expect(newUserProblems(form()).size).toBe(0)
  })

  it("rejects spaces inside a username but not around it", () => {
    expect(newUserProblems(form({ username: "ada l" }))).toContain("usernameSpaces")
    expect(newUserProblems(form({ username: " ada " })).size).toBe(0)
  })

  it("requires a long enough, confirmed password for local accounts", () => {
    expect(newUserProblems(form({ password: "short", confirmPassword: "short" }))).toContain("passwordShort")
    expect(newUserProblems(form({ confirmPassword: "different" }))).toContain("passwordMismatch")
  })

  it("ignores passwords for SAML accounts", () => {
    expect(newUserProblems(form({ authMethod: "saml", password: "", confirmPassword: "x" })).size).toBe(0)
  })
})

describe("newUserRequest", () => {
  it("trims, nulls blank names, and sends the password for local accounts", () => {
    expect(newUserRequest(form({ username: " ada ", middleName: "  " }))).toEqual({
      auth_method: "local",
      username: "ada",
      email: "ada@example.org",
      first_given_name: "Ada",
      second_given_name: null,
      family_name: "Lovelace",
      password: "correct horse",
    })
  })

  it("never sends a password for SAML accounts", () => {
    expect(newUserRequest(form({ authMethod: "saml" }))).not.toHaveProperty("password")
  })
})
