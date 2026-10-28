import { describe, expect, it } from "vitest"

import { checkPlaceholders, extractPlaceholders, segmentText } from "./placeholders"

describe("extractPlaceholders", () => {
  it("finds interpolation, formatted interpolation, nesting, and Trans tags", () => {
    expect(
      extractPlaceholders(
        "Hi {{name}}, {{value, number}} $t(common:ok) <0>a</0> <1/> <strong>b</strong>"
      )
    ).toEqual([
      "{{name}}",
      "{{value,number}}",
      "$t(common:ok)",
      "<0>",
      "</0>",
      "<1/>",
      "<strong>",
      "</strong>",
    ])
  })

  it("ignores whitespace inside placeholders", () => {
    expect(extractPlaceholders("{{ name }} <1 />")).toEqual(["{{name}}", "<1/>"])
  })

  it("does not treat a lone brace or angle bracket as a placeholder", () => {
    expect(extractPlaceholders("a < b and {single}")).toEqual([])
  })
})

describe("checkPlaceholders", () => {
  it("passes when the multisets match in any order", () => {
    const result = checkPlaceholders("{{a}} and {{b}}", "{{b}} et {{a}}")
    expect(result.ok).toBe(true)
    expect(result.expected).toEqual(["{{a}}", "{{b}}"])
  })

  it("reports missing and extra placeholders", () => {
    const result = checkPlaceholders("Pay {{amount}}", "Payer {{montant}}")
    expect(result.ok).toBe(false)
    expect(result.missing).toEqual(["{{amount}}"])
    expect(result.extra).toEqual(["{{montant}}"])
  })

  it("counts repeats, not just presence", () => {
    const result = checkPlaceholders("{{n}} of {{n}}", "{{n}}")
    expect(result.missing).toEqual(["{{n}}"])
    expect(result.extra).toEqual([])
  })

  it("catches a dropped Trans closing tag", () => {
    const result = checkPlaceholders("I agree to the <1>terms</1>", "J'accepte les <1>conditions")
    expect(result.missing).toEqual(["</1>"])
  })
})

describe("segmentText", () => {
  it("splits literals from placeholders and round-trips", () => {
    const text = "Hello {{name}}!"
    const segments = segmentText(text)
    expect(segments).toEqual([
      { text: "Hello ", placeholder: false },
      { text: "{{name}}", placeholder: true },
      { text: "!", placeholder: false },
    ])
    expect(segments.map((s) => s.text).join("")).toBe(text)
  })
})
