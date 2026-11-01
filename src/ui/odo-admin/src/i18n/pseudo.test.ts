import { describe, expect, it } from "vitest"

import { pseudoResource, pseudoString } from "./pseudo"

describe("pseudoString", () => {
  it("wraps ar-XB words in right-to-left overrides", () => {
    expect(pseudoString("Hi {{n}}")).toBe("‮Hi‬ {{n}}")
  })
})

describe("pseudoResource", () => {
  it("transforms nested string leaves only", () => {
    expect(pseudoResource({ a: { b: "ab" }, n: 1 })).toEqual({ a: { b: "\u202eab\u202c" }, n: 1 })
  })
})
