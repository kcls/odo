import { describe, expect, it, vi } from "vitest"

import common from "@/locales/en/common.json"
import { ApiRequestError } from "./client"
import { describeError } from "./errors"

// The app's i18n module detects the browser language and touches the
// document; a plain instance with the English strings is enough here.
vi.mock("@/i18n", async () => {
  const { createInstance } = await import("i18next")
  const i18n = createInstance()
  await i18n.init({ lng: "en", defaultNS: "common", resources: { en: { common } } })
  return { default: i18n }
})

describe("describeError", () => {
  it("prefers a known code", () => {
    const err = new ApiRequestError(409, { code: "ROLE_ASSIGNED", message: "raw server text" })
    expect(describeError(err)).toBe(common.errors.codes.ROLE_ASSIGNED)
  })

  it("then the server's message", () => {
    expect(describeError(new ApiRequestError(400, { message: "Bad email" }))).toBe("Bad email")
  })

  it("then a status fallback", () => {
    expect(describeError(new ApiRequestError(403))).toBe(common.errors.status.forbidden)
    expect(describeError(new ApiRequestError(502))).toBe(common.errors.status.server)
  })

  it("never shows a non-API error's internals", () => {
    expect(describeError(new TypeError("x is undefined"))).toBe(common.errors.unexpected)
  })
})
