import type { ReactNode } from "react"

export function PageHeader({
  title,
  description,
  actions,
  as: Heading = "h1",
}: {
  title: ReactNode
  description?: ReactNode
  actions?: ReactNode
  as?: "h1" | "h2" | "h3"
}) {
  const size = Heading === "h1" ? "text-2xl tracking-tight" : "text-lg"
  return (
    <div className="flex flex-wrap items-end justify-between gap-4">
      <div className="min-w-0">
        <Heading className={`${size} font-semibold`}>{title}</Heading>
        {description && <p className="text-sm text-muted-foreground">{description}</p>}
      </div>
      {actions && <div className="flex flex-wrap items-center gap-2">{actions}</div>}
    </div>
  )
}
