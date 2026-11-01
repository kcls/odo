import { describe, expect, it } from "vitest"

import { readTab, tabParams } from "./detail-tabs"

describe("readTab", () => {
  const all = ["overview", "permissions", "sessions", "saml"] as const

  it("reads a shown tab", () => {
    expect(readTab(new URLSearchParams("tab=sessions"), all)).toBe("sessions")
  })

  it("falls back to the overview when absent, unknown, or hidden", () => {
    expect(readTab(new URLSearchParams(), all)).toBe("overview")
    expect(readTab(new URLSearchParams("tab=bogus"), all)).toBe("overview")
    expect(readTab(new URLSearchParams("tab=saml"), ["overview", "sessions"])).toBe("overview")
  })
})

describe("tabParams", () => {
  it("leaves the overview out of the URL and keeps other params", () => {
    expect(tabParams(new URLSearchParams("tab=saml&x=1"), "overview").toString()).toBe("x=1")
    expect(tabParams(new URLSearchParams("x=1"), "permissions").toString()).toBe("x=1&tab=permissions")
  })
})
