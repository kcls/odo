import type { LucideIcon } from "lucide-react"
import type { ReactNode } from "react"
import { Link } from "react-router"

import { Card } from "@/components/ui/card"
import { cn } from "@/lib/utils"

/**
 * One headline number: label, value, and an optional hint. The value is
 * text-colored; a status color, when there is one, goes on the icon only.
 * With `to`, the whole tile links to the detail behind the number.
 */
export function StatTile({
  label,
  value,
  hint,
  icon: Icon,
  iconClass,
  to,
  linkLabel,
}: {
  label: string
  value: ReactNode
  hint?: string
  icon?: LucideIcon
  iconClass?: string
  to?: string
  /** Accessible name for the link, e.g. "Show failed deliveries". */
  linkLabel?: string
}) {
  const body = (
    <Card size="sm" className={cn("h-full gap-1 px-4", to && "transition-colors hover:bg-muted/50")}>
      <div className="flex items-center gap-1.5 text-sm text-muted-foreground">
        {Icon && <Icon aria-hidden="true" className={cn("size-4", iconClass)} />}
        {label}
      </div>
      <div className="text-3xl font-semibold tracking-tight">{value}</div>
      {hint && <div className="text-xs text-muted-foreground">{hint}</div>}
    </Card>
  )
  return to ? (
    <Link to={to} aria-label={linkLabel} className="block rounded-xl outline-none focus-visible:ring-3 focus-visible:ring-ring/50">
      {body}
    </Link>
  ) : (
    body
  )
}
