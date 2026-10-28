import { describe, expect, it } from "vitest"

import { formatSampleData, parseSampleData } from "./sample-data"

describe("parseSampleData", () => {
  it("treats blank as no data", () => {
    expect(parseSampleData("  \n")).toEqual({ ok: true, value: undefined })
  })

  it("parses JSON", () => {
    expect(parseSampleData('{"name": "Ada"}')).toEqual({ ok: true, value: { name: "Ada" } })
  })

  it("rejects invalid JSON", () => {
    expect(parseSampleData("{name: Ada}")).toEqual({ ok: false })
  })
})

describe("formatSampleData", () => {
  it("round-trips through parseSampleData", () => {
    const value = { user: { name: "Ada" }, count: 2 }
    expect(parseSampleData(formatSampleData(value))).toEqual({ ok: true, value })
  })

  it("shows nothing for null", () => {
    expect(formatSampleData(null)).toBe("")
  })
})
