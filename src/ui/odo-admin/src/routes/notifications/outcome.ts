import { CircleCheckIcon, CircleMinusIcon, CircleXIcon, ClockIcon, type LucideIcon } from "lucide-react"

import type { Tone } from "@/components/status-badge"

/**
 * What happened to a delivery, as people ask about it. "Queued" folds the
 * processor's pending and processing together: both mean "not done yet".
 */
export const OUTCOMES = ["delivered", "failed", "skipped", "queued"] as const
export type Outcome = (typeof OUTCOMES)[number]

export function isOutcome(value: string | null): value is Outcome {
  return (OUTCOMES as readonly string[]).includes(value ?? "")
}

/** The API statuses an outcome covers. */
export function statusesFor(outcome: Outcome): string[] {
  return outcome === "queued" ? ["pending", "processing"] : [outcome]
}

export function outcomeOf(status: string): Outcome | null {
  if (status === "pending" || status === "processing") return "queued"
  return isOutcome(status) ? status : null
}

/** Status colors are reserved for state and always travel with an icon. */
export const OUTCOME_STYLE: Record<Outcome, { icon: LucideIcon; tone: Tone; iconClass: string }> = {
  delivered: { icon: CircleCheckIcon, tone: "success", iconClass: "text-success" },
  failed: { icon: CircleXIcon, tone: "danger", iconClass: "text-destructive" },
  skipped: { icon: CircleMinusIcon, tone: "neutral", iconClass: "text-muted-foreground" },
  queued: { icon: ClockIcon, tone: "warning", iconClass: "text-warning" },
}

/** Count per outcome from the summary's per-status rows. */
export function outcomeTotals(counts: readonly { status: string; count: number }[]): Record<Outcome, number> {
  const totals: Record<Outcome, number> = { delivered: 0, failed: 0, skipped: 0, queued: 0 }
  for (const { status, count } of counts) {
    const outcome = outcomeOf(status)
    if (outcome) totals[outcome] += count
  }
  return totals
}
