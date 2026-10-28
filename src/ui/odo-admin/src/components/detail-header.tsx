import { ArrowLeftIcon } from "lucide-react"
import type { ReactNode } from "react"
import { Link } from "react-router"

import { Button } from "@/components/ui/button"

/**
 * Header for a subject's drill-down page: a back link to its list, the
 * subject's title, a row of summary facts, and header actions. The body
 * below composes stacked <Section>s about the same subject.
 */
export function DetailHeader({
  backTo,
  backLabel,
  title,
  summary,
  actions,
}: {
  backTo: string
  backLabel: string
  title: ReactNode
  /** Each fact is its own node; the row spaces and wraps them. */
  summary?: ReactNode[]
  actions?: ReactNode
}) {
  return (
    <div className="flex flex-col gap-1">
      <Button asChild variant="ghost" size="sm" className="-ms-2.5 self-start text-muted-foreground">
        <Link to={backTo}>
          <ArrowLeftIcon data-icon="inline-start" className="rtl:rotate-180" />
          {backLabel}
        </Link>
      </Button>
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div className="min-w-0">
          <h1 className="text-2xl font-semibold tracking-tight">{title}</h1>
          {summary && summary.length > 0 && (
            <div className="mt-1 flex flex-wrap items-center gap-x-3 gap-y-1 text-sm text-muted-foreground">
              {summary.map((fact, i) => (
                <span key={i} className="inline-flex items-center gap-1">
                  {fact}
                </span>
              ))}
            </div>
          )}
        </div>
        {actions && <div className="flex flex-wrap items-center gap-2">{actions}</div>}
      </div>
    </div>
  )
}
