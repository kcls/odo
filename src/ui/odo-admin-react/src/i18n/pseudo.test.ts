import { describe, expect, it } from "vitest"

import { pseudoResource, pseudoString } from "./pseudo"

describe("pseudoString", () => {
  it("accents and pads en-XA but leaves placeholders intact", () => {
    const out = pseudoString("Hello {{name}} <1>link</1>", "en-XA")
    expect(out).toMatch(/^\[Ĥéļļó \{\{name\}\} <1>ļíñķ<\/1>·+\]$/)
  })

  it("wraps ar-XB words in right-to-left overrides", () => {
    expect(pseudoString("Hi {{n}}", "ar-XB")).toBe("‮Hi‬ {{n}}")
  })
})

describe("pseudoResource", () => {
  it("transforms nested string leaves only", () => {
    expect(pseudoResource({ a: { b: "ab" }, n: 1 }, "en-XA")).toEqual({ a: { b: "[áƀ·]" }, n: 1 })
  })
})
