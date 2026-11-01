/**
 * Pseudo-locale for testing the UI's own localization, following the
 * Android/Chromium convention: `ar-XB` mirrors every string
 * right-to-left (finds layout that is not direction-aware). Placeholders
 * and Trans tags pass through untouched.
 */
import { segmentText } from "@/lib/i18next/placeholders"

const RLO = "‮"
const PDF = "‬"

function mirror(text: string): string {
  return text.replace(/\S+/g, (word) => `${RLO}${word}${PDF}`)
}

export function pseudoString(text: string): string {
  return segmentText(text)
    .map((s) => (s.placeholder ? s.text : mirror(s.text)))
    .join("")
}

export function pseudoResource(node: unknown): unknown {
  if (typeof node === "string") return pseudoString(node)
  if (Array.isArray(node)) return node.map(pseudoResource)
  if (node && typeof node === "object") {
    return Object.fromEntries(
      Object.entries(node).map(([k, v]) => [k, pseudoResource(v)])
    )
  }
  return node
}
