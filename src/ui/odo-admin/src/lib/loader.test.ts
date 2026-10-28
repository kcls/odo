import { describe, expect, it } from "vitest"

import { appPath, safeRedirectTarget } from "./loader"

describe("safeRedirectTarget", () => {
  it("accepts app paths", () => {
    expect(safeRedirectTarget("/users/5?tab=roles")).toBe("/users/5?tab=roles")
  })

  it.each([null, "", "https://evil.example", "//evil.example", "/\\evil.example", "users"])(
    "falls back to home for %j",
    (value) => {
      expect(safeRedirectTarget(value)).toBe("/")
    }
  )
})

describe("appPath", () => {
  // Tests run with Vite's default base, "/".
  it("returns the pathname under the base", () => {
    expect(appPath(new URL("http://x/users/5?q=1"))).toBe("/users/5")
  })
})
