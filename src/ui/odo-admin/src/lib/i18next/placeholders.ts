/**
 * Placeholder extraction for react-i18next strings: interpolation
 * (`{{name}}`, `{{value, number}}`), nesting (`$t(other.key)`), and
 * `<Trans>` tags, both indexed (`<0>`, `</0>`, `<1/>`) and named
 * (`<strong>`, `</strong>`).
 */
const PLACEHOLDER_RE =
  /\{\{[^{}]+\}\}|\$t\([^()]*\)|<\/?\s*(?:\d+|[a-zA-Z][\w-]*)\s*\/?>/g

export type TextSegment = { text: string; placeholder: boolean }

/** Split text into literal runs and placeholders, for highlighting. */
export function segmentText(text: string): TextSegment[] {
  const segments: TextSegment[] = []
  let last = 0
  for (const match of text.matchAll(PLACEHOLDER_RE)) {
    const start = match.index
    if (start > last) {
      segments.push({ text: text.slice(last, start), placeholder: false })
    }
    segments.push({ text: match[0], placeholder: true })
    last = start + match[0].length
  }
  if (last < text.length) {
    segments.push({ text: text.slice(last), placeholder: false })
  }
  return segments
}

/**
 * Whitespace inside a placeholder is not significant to i18next, so
 * `{{ name }}` and `{{name}}`, or `<1 />` and `<1/>`, compare equal.
 */
function normalize(placeholder: string): string {
  return placeholder.replace(/\s+/g, "")
}

export function extractPlaceholders(text: string): string[] {
  return Array.from(text.matchAll(PLACEHOLDER_RE), (m) => normalize(m[0]))
}

export type PlaceholderCheck = {
  ok: boolean
  /** In the source but not (or not as often) in the translation. */
  missing: string[]
  /** In the translation but not (or not as often) in the source. */
  extra: string[]
  /** Distinct placeholders of the source, for the "match" message. */
  expected: string[]
}

/** Compare placeholder multisets between source and translation. */
export function checkPlaceholders(
  source: string,
  translation: string
): PlaceholderCheck {
  const expectedList = extractPlaceholders(source)
  const counts = new Map<string, number>()
  for (const p of expectedList) counts.set(p, (counts.get(p) ?? 0) + 1)

  const extra: string[] = []
  for (const p of extractPlaceholders(translation)) {
    const n = counts.get(p) ?? 0
    if (n > 0) counts.set(p, n - 1)
    else extra.push(p)
  }
  const missing: string[] = []
  for (const [p, n] of counts) {
    for (let i = 0; i < n; i++) missing.push(p)
  }
  return {
    ok: missing.length === 0 && extra.length === 0,
    missing,
    extra,
    expected: [...new Set(expectedList)],
  }
}
