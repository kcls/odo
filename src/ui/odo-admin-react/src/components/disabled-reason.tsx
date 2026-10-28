import type { ReactNode } from "react"

import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip"

/**
 * Wraps a disabled control with a tooltip saying why it's disabled. A
 * disabled button gets no pointer or focus events, so the tooltip hangs
 * off a focusable wrapper that carries the control's accessible name;
 * pass the control with aria-hidden and tabIndex={-1}.
 */
export function DisabledReason({
  label,
  reason,
  children,
}: {
  /** The control's accessible name, e.g. "Delete role". */
  label: string
  reason: string
  children: ReactNode
}) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span tabIndex={0} aria-label={label} aria-description={reason} className="inline-flex rounded-md">
          {children}
        </span>
      </TooltipTrigger>
      <TooltipContent>{reason}</TooltipContent>
    </Tooltip>
  )
}
