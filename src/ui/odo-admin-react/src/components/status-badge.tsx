import type { ReactNode } from "react"

import { Badge } from "@/components/ui/badge"
import { cn } from "@/lib/utils"

export type Tone = "success" | "info" | "neutral" | "warning" | "danger"

const TONE: Record<Tone, string> = {
  success: "text-success",
  info: "text-primary",
  neutral: "text-muted-foreground",
  warning: "text-warning",
  danger: "text-destructive",
}

/** A short status label; tone carries emphasis, the text carries meaning. */
export function StatusBadge({ tone, children }: { tone: Tone; children: ReactNode }) {
  return (
    <Badge variant="outline" className={cn(TONE[tone])}>
      {children}
    </Badge>
  )
}
