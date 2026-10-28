import { useTranslation } from "react-i18next"

import { PSEUDO_LOCALES } from "@/i18n"

const RELATIVE_STEPS: [Intl.RelativeTimeFormatUnit, number][] = [
  ["year", 365 * 24 * 3600],
  ["month", 30 * 24 * 3600],
  ["week", 7 * 24 * 3600],
  ["day", 24 * 3600],
  ["hour", 3600],
  ["minute", 60],
]

export function relativeTime(iso: string, displayLng: string): string {
  const seconds = (new Date(iso).getTime() - Date.now()) / 1000
  const rtf = new Intl.RelativeTimeFormat(displayLng, { numeric: "auto" })
  for (const [unit, size] of RELATIVE_STEPS) {
    if (Math.abs(seconds) >= size) return rtf.format(Math.round(seconds / size), unit)
  }
  return rtf.format(0, "minute")
}

/**
 * Locale-aware formatters bound to the current UI language. Pseudo
 * locales are not real Intl locales, so they format as English. Null or
 * missing timestamps render as an em dash.
 */
export function useFormatters() {
  const { i18n } = useTranslation()
  const resolved = i18n.resolvedLanguage ?? "en"
  const lng = PSEUDO_LOCALES.includes(resolved) ? "en" : resolved
  const dateTime = (iso: string | null | undefined, style: Intl.DateTimeFormatOptions) =>
    iso ? new Intl.DateTimeFormat(lng, style).format(new Date(iso)) : "—"
  return {
    lng,
    /** e.g. "Oct 28, 2026, 1:54:03 PM" */
    dateTime: (iso: string | null | undefined) =>
      dateTime(iso, { dateStyle: "medium", timeStyle: "medium" }),
    /** e.g. "10/28/26, 1:54 PM" */
    dateTimeShort: (iso: string | null | undefined) =>
      dateTime(iso, { dateStyle: "short", timeStyle: "short" }),
    /** e.g. "Oct 28, 2026" */
    date: (iso: string | null | undefined) => dateTime(iso, { dateStyle: "medium" }),
    relativeTime: (iso: string) => relativeTime(iso, lng),
  }
}
