export type ParsedJsonText = { ok: true; value: unknown } | { ok: false }

/**
 * An optional JSON textarea's value. Blank means "no value" (value
 * undefined), which is distinct from invalid JSON.
 */
export function parseJsonText(text: string): ParsedJsonText {
  if (!text.trim()) return { ok: true, value: undefined }
  try {
    return { ok: true, value: JSON.parse(text) }
  } catch {
    return { ok: false }
  }
}

/** A stored JSON value as editable text; nothing for null. */
export function formatJsonText(value: unknown): string {
  return value === null || value === undefined ? "" : JSON.stringify(value, null, 2)
}
