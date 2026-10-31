import { describe, expect, it } from "vitest"

import { changedFields, diffField, type TemplateContent } from "./version-diff"

const base: TemplateContent = {
  code: "incident.assigned",
  name: "Assigned",
  description: null,
  is_active: true,
  subject_template: "Assigned: {{title}}",
  body_template: "Line one\nLine two\n",
  body_template_html: null,
  sample_data: { title: "Leak" },
}

describe("changedFields", () => {
  it("finds nothing between equal content", () => {
    expect(changedFields(base, { ...base, sample_data: { title: "Leak" } })).toEqual([])
  })

  it("treats a missing and an empty optional field alike", () => {
    expect(changedFields(base, { ...base, description: "", body_template_html: undefined })).toEqual([])
  })

  it("lists changed fields in editor order", () => {
    const after = { ...base, sample_data: { title: "Fire" }, is_active: false, name: "Renamed" }
    expect(changedFields(base, after)).toEqual(["name", "is_active", "sample_data"])
  })

  it("counts every field as changed from nothing", () => {
    expect(changedFields(null, base)).toEqual(["code", "name", "is_active", "subject_template", "body_template", "sample_data"])
  })
})

describe("diffField", () => {
  it("diffs bodies by line", () => {
    expect(diffField("body_template", base.body_template, "Line one\nLine 2\n")).toEqual([
      { value: "Line one\n", kind: "same" },
      { value: "Line two\n", kind: "removed" },
      { value: "Line 2\n", kind: "added" },
    ])
  })

  it("diffs one-line fields by word", () => {
    const parts = diffField("subject_template", "Assigned: {{title}}", "Reassigned: {{title}}")
    expect(parts.filter((p) => p.kind !== "same").map((p) => [p.kind, p.value])).toEqual([
      ["removed", "Assigned"],
      ["added", "Reassigned"],
    ])
  })
})
