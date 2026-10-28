import {
  ChevronLeftIcon,
  ChevronRightIcon,
  ChevronsLeftIcon,
  ChevronsRightIcon,
} from "lucide-react"
import { useId } from "react"
import { useTranslation } from "react-i18next"

import { Button } from "@/components/ui/button"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { PAGE_SIZES } from "@/lib/list-state"

/**
 * Page controls for a server-driven list. Place directly beneath its
 * DataTable and drive both from the same list state.
 */
export function Paginator({
  total,
  pageIndex,
  pageSize,
  onChange,
}: {
  /** Rows matching the filter, across all pages. */
  total: number
  pageIndex: number
  pageSize: number
  onChange: (page: { pageIndex: number; pageSize: number }) => void
}) {
  const { t } = useTranslation()
  const sizeLabelId = useId()
  const pageCount = Math.max(1, Math.ceil(total / pageSize))
  const canPrev = pageIndex > 0
  const canNext = pageIndex < pageCount - 1
  const go = (index: number) => onChange({ pageIndex: index, pageSize })

  return (
    <nav
      aria-label={t("table.pagination")}
      className="flex flex-wrap items-center justify-end gap-x-6 gap-y-2 text-sm text-muted-foreground"
    >
      <div className="flex items-center gap-2">
        <span id={sizeLabelId}>{t("table.rowsPerPage")}</span>
        <Select
          value={String(pageSize)}
          // A new size starts at the first page; the old offset would skip rows.
          onValueChange={(v) => onChange({ pageIndex: 0, pageSize: Number(v) })}
        >
          <SelectTrigger size="sm" className="w-20" aria-labelledby={sizeLabelId}>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {PAGE_SIZES.map((size) => (
              <SelectItem key={size} value={String(size)}>
                {size}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
      <span className="tabular-nums" aria-live="polite">
        {t("table.range", {
          start: total === 0 ? 0 : pageIndex * pageSize + 1,
          end: Math.min(total, (pageIndex + 1) * pageSize),
          total,
        })}
      </span>
      <div className="flex items-center gap-1">
        <Button variant="ghost" size="icon-sm" disabled={!canPrev} onClick={() => go(0)} aria-label={t("table.firstPage")}>
          <ChevronsLeftIcon className="rtl:rotate-180" />
        </Button>
        <Button variant="ghost" size="icon-sm" disabled={!canPrev} onClick={() => go(pageIndex - 1)} aria-label={t("table.previousPage")}>
          <ChevronLeftIcon className="rtl:rotate-180" />
        </Button>
        <Button variant="ghost" size="icon-sm" disabled={!canNext} onClick={() => go(pageIndex + 1)} aria-label={t("table.nextPage")}>
          <ChevronRightIcon className="rtl:rotate-180" />
        </Button>
        <Button variant="ghost" size="icon-sm" disabled={!canNext} onClick={() => go(pageCount - 1)} aria-label={t("table.lastPage")}>
          <ChevronsRightIcon className="rtl:rotate-180" />
        </Button>
      </div>
    </nav>
  )
}
