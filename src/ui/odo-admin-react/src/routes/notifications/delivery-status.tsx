import { useTranslation } from "react-i18next"

import { Badge } from "@/components/ui/badge"
import { cn } from "@/lib/utils"
import { OUTCOME_STYLE, outcomeOf } from "./outcome"

/** A delivery's status as an icon + label; never color alone. */
export function DeliveryStatus({ status }: { status: string }) {
  const { t } = useTranslation("notifications")
  const outcome = outcomeOf(status)
  if (!outcome) return <Badge variant="outline">{status}</Badge>
  const { icon: Icon, iconClass } = OUTCOME_STYLE[outcome]
  // "Sending" is worth telling apart from waiting, so it keeps its own label.
  const label = status === "processing" ? t("status.processing") : t(`status.${outcome}`)
  return (
    <Badge variant="outline" className="gap-1 text-foreground">
      <Icon aria-hidden="true" className={cn("size-3.5", iconClass)} />
      {label}
    </Badge>
  )
}
