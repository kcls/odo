import { diffLines, diffWordsWithSpace } from "diff"

import { formatJsonText } from "@/lib/json-text"

/** The versioned content of a template, as both a version and the live template carry it. */
export type TemplateContent = {
  code: string
  name: string
  description?: string | null
  is_active: boolean
  subject_template: string
  body_template: string
  body_template_html?: string | null
  sample_data?: unknown
}

/** Fields in the order the editor shows them. */
export const CONTENT_FIELDS = [
  "code",
  "name",
  "description",
  "is_active",
  "subject_template",
  "body_template",
  "body_template_html",
  "sample_data",
] as const
export type ContentField = (typeof CONTENT_FIELDS)[number]

/** Fields compared word by word; the rest line by line. */
const SINGLE_LINE: ReadonlySet<ContentField> = new Set(["code", "name", "description", "subject_template"])

/** A field's value as comparable text; is_active is shown as a state, not diffed. */
export function fieldText(content: TemplateContent | null, field: Exclude<ContentField, "is_active">): string {
  if (!content) return ""
  if (field === "sample_data") return formatJsonText(content.sample_data)
  return content[field] ?? ""
}

/** The fields whose values differ between two versions of a template. */
export function changedFields(before: TemplateContent | null, after: TemplateContent): ContentField[] {
  return CONTENT_FIELDS.filter((field) =>
    field === "is_active"
      ? before?.is_active !== after.is_active
      : fieldText(before, field) !== fieldText(after, field)
  )
}

export type DiffPart = { value: string; kind: "added" | "removed" | "same" }

/** How a field's text changed, in display order. */
export function diffField(field: Exclude<ContentField, "is_active">, before: string, after: string): DiffPart[] {
  const parts = SINGLE_LINE.has(field) ? diffWordsWithSpace(before, after) : diffLines(before, after)
  return parts.map((p) => ({ value: p.value, kind: p.added ? "added" : p.removed ? "removed" : "same" }))
}
