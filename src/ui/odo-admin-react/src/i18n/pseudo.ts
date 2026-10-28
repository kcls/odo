/**
 * Pseudo-locales for testing the UI's own localization, following the
 * Android/Chromium convention: `en-XA` accents and lengthens every
 * string (finds hard-coded text and truncation), `ar-XB` mirrors it
 * right-to-left (finds layout that is not direction-aware). Placeholders
 * and Trans tags pass through untouched.
 */
import { segmentText } from "@/lib/i18next/placeholders"

export type PseudoLocale = "en-XA" | "ar-XB"

const ACCENTS: Record<string, string> = {
  a: "á", b: "ƀ", c: "ç", d: "ð", e: "é", f: "ƒ", g: "ĝ", h: "ĥ", i: "í",
  j: "ĵ", k: "ķ", l: "ļ", m: "ɱ", n: "ñ", o: "ó", p: "þ", q: "ǫ", r: "ŕ",
  s: "š", t: "ţ", u: "ú", v: "ṽ", w: "ŵ", x: "ẋ", y: "ý", z: "ž",
  A: "Á", B: "Ɓ", C: "Ç", D: "Ð", E: "É", F: "Ƒ", G: "Ĝ", H: "Ĥ", I: "Í",
  J: "Ĵ", K: "Ķ", L: "Ļ", M: "Ṁ", N: "Ñ", O: "Ó", P: "Þ", Q: "Ǫ", R: "Ŕ",
  S: "Š", T: "Ţ", U: "Ú", V: "Ṽ", W: "Ŵ", X: "Ẋ", Y: "Ý", Z: "Ž",
}

const RLO = "‮"
const PDF = "‬"

function accent(text: string): string {
  return text.replace(/[a-zA-Z]/g, (ch) => ACCENTS[ch] ?? ch)
}

function mirror(text: string): string {
  return text.replace(/\S+/g, (word) => `${RLO}${word}${PDF}`)
}

export function pseudoString(text: string, locale: PseudoLocale): string {
  const body = segmentText(text)
    .map((s) => {
      if (s.placeholder) return s.text
      return locale === "en-XA" ? accent(s.text) : mirror(s.text)
    })
    .join("")
  if (locale === "ar-XB") return body
  // Roughly 30% longer, as many languages run longer than English.
  const padding = "·".repeat(Math.ceil(text.length * 0.3))
  return `[${body}${padding}]`
}

export function pseudoResource(node: unknown, locale: PseudoLocale): unknown {
  if (typeof node === "string") return pseudoString(node, locale)
  if (Array.isArray(node)) return node.map((n) => pseudoResource(n, locale))
  if (node && typeof node === "object") {
    return Object.fromEntries(
      Object.entries(node).map(([k, v]) => [k, pseudoResource(v, locale)])
    )
  }
  return node
}
