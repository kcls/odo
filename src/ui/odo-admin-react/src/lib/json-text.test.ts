import { describe, expect, it } from "vitest"

import { formatJsonText, parseJsonText } from "./json-text"

describe("parseJsonText", () => {
  it("treats blank as no data", () => {
    expect(parseJsonText("  \n")).toEqual({ ok: true, value: undefined })
  })

  it("parses JSON", () => {
    expect(parseJsonText('{"name": "Ada"}')).toEqual({ ok: true, value: { name: "Ada" } })
  })

  it("rejects invalid JSON", () => {
    expect(parseJsonText("{name: Ada}")).toEqual({ ok: false })
  })
})

describe("formatJsonText", () => {
  it("round-trips through parseJsonText", () => {
    const value = { user: { name: "Ada" }, count: 2 }
    expect(parseJsonText(formatJsonText(value))).toEqual({ ok: true, value })
  })

  it("shows nothing for null", () => {
    expect(formatJsonText(null)).toBe("")
  })
})
