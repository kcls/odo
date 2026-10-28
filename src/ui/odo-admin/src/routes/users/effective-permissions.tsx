import { useState } from "react"
import { useTranslation } from "react-i18next"

import type { PermScopeRow } from "@/api/authz"
import { DataTable, type Column } from "@/components/data-table/data-table"
import { Section } from "@/components/section"
import { StatusBadge } from "@/components/status-badge"
import { Button } from "@/components/ui/button"

/** What the user can do and where; read-only, derived from their roles. */
export function EffectivePermissions({ scopes }: { scopes: PermScopeRow[] | "failed" | null }) {
  const { t } = useTranslation("users")
  // Perms whose full scope list is expanded, by code.
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(new Set())

  const toggle = (perm: string) =>
    setExpanded((prev) => {
      const next = new Set(prev)
      if (!next.delete(perm)) next.add(perm)
      return next
    })

  const columns: Column<PermScopeRow>[] = [
    {
      id: "permission",
      header: t("effective.columns.permission"),
      cell: (p) => (
        <div className="flex flex-col">
          <span className="font-mono text-xs">{p.perm}</span>
          {p.description && <span className="text-muted-foreground">{p.description}</span>}
        </div>
      ),
    },
    {
      id: "scope",
      header: t("effective.columns.scope"),
      cell: (p) => {
        if (p.global) return <StatusBadge tone="info">{t("effective.global")}</StatusBadge>
        const open = expanded.has(p.perm)
        const shown = open ? p.scope_units : p.scope_units.slice(0, 1)
        const hidden = p.scope_units.length - shown.length
        return (
          <div className="flex flex-col items-start gap-0.5">
            {shown.map((unit) => (
              <span key={unit.id}>{t("effective.unitAndBelow", { unit: unit.label })}</span>
            ))}
            {p.scope_units.length > 1 && (
              <Button variant="link" size="xs" className="h-auto px-0" onClick={() => toggle(p.perm)}>
                {open ? t("effective.less") : t("effective.more", { count: hidden })}
              </Button>
            )}
          </div>
        )
      },
    },
  ]

  return (
    <Section title={t("effective.title")} description={t("effective.description")}>
      {scopes === "failed" ? (
        <p className="text-sm text-destructive">{t("effective.failed")}</p>
      ) : (
        <DataTable
          label={t("effective.title")}
          rows={scopes ?? []}
          columns={columns}
          rowKey={(p) => p.perm}
          empty={t("effective.none")}
        />
      )}
    </Section>
  )
}
