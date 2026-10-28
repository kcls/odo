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

export function weekdayName(day: number, displayLng: string): string {
  // 2023-01-01 was a Sunday.
  return new Intl.DateTimeFormat(displayLng, { weekday: "long" }).format(new Date(2023, 0, 1 + day))
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
    /** A date-only value ("2026-12-25"), read as a calendar date, not UTC. */
    calendarDate: (ymd: string) => {
      const [y, m, d] = ymd.split("-").map(Number)
      return new Intl.DateTimeFormat(lng, { dateStyle: "medium" }).format(new Date(y, m - 1, d))
    },
    /** A date-only value, short, e.g. "Oct 28", for chart axes. */
    calendarDateShort: (ymd: string) => {
      const [y, m, d] = ymd.split("-").map(Number)
      return new Intl.DateTimeFormat(lng, { month: "short", day: "numeric" }).format(new Date(y, m - 1, d))
    },
    /** A time of day ("17:30:00"), e.g. "5:30 PM". */
    timeOfDay: (hms: string) => {
      const [h, min] = hms.split(":").map(Number)
      return new Intl.DateTimeFormat(lng, { timeStyle: "short" }).format(new Date(2000, 0, 1, h, min))
    },
    /** A headline count, compacted past 10,000: 1,284 / 12.9K. */
    count: (n: number) =>
      new Intl.NumberFormat(lng, n >= 10_000 ? { notation: "compact", maximumFractionDigits: 1 } : {}).format(n),
    /** 0 = Sunday, as odo-org stores it. */
    weekday: (day: number) => weekdayName(day, lng),
  }
}
