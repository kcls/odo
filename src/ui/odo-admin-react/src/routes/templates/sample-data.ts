export type ParsedSampleData = { ok: true; value: unknown } | { ok: false }

/**
 * The sample-data textarea as JSON. Blank means "no sample data"
 * (value undefined), which is distinct from invalid JSON.
 */
export function parseSampleData(text: string): ParsedSampleData {
  if (!text.trim()) return { ok: true, value: undefined }
  try {
    return { ok: true, value: JSON.parse(text) }
  } catch {
    return { ok: false }
  }
}

/** Stored sample data as editable text. */
export function formatSampleData(value: unknown): string {
  return value === null || value === undefined ? "" : JSON.stringify(value, null, 2)
}
