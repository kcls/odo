import { ArrowDownIcon, ArrowUpIcon, ChevronsUpDownIcon } from "lucide-react"
import type { MouseEvent, ReactNode } from "react"
import { useTranslation } from "react-i18next"
import { useNavigate } from "react-router"

import { Card } from "@/components/ui/card"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import type { SortState } from "@/lib/list-state"
import { cn } from "@/lib/utils"

export type Column<T> = {
  id: string
  header: ReactNode
  cell: (row: T) => ReactNode
  /**
   * The API's sort key for this column. Only allow-listed, real DB
   * columns are sortable; computed columns (counts) leave this unset.
   */
  sortKey?: string
  /** Applied to the header and every body cell of the column. */
  className?: string
}

type DataTableProps<T> = {
  rows: readonly T[]
  columns: readonly Column<T>[]
  rowKey: (row: T) => string | number
  /** Server-driven sort: the active sort and a callback for header clicks. */
  sort?: SortState
  onSortChange?: (sort: SortState) => void
  /**
   * Makes the whole row open this path on click. Put a real link in the
   * row's primary cell too, for keyboard and screen-reader users.
   */
  rowHref?: (row: T) => string
  /** Shown in place of the rows when there are none. */
  empty: ReactNode
  /** Dims the rows while a reload is in flight. */
  busy?: boolean
  /** "plain" drops the card frame, for tables already inside a card. */
  variant?: "framed" | "plain"
  /** Accessible name for the table. */
  label?: string
}

/** Clicks on these inside a clickable row do their own thing. */
const INTERACTIVE = "a, button, input, select, textarea, label, [role=menuitem], [role=checkbox]"

/**
 * The admin list table. Column definitions are data, so a page declares
 * what each column shows and the table owns layout, framing, sort
 * headers, row navigation, and the empty state.
 */
export function DataTable<T>({
  rows,
  columns,
  rowKey,
  sort,
  onSortChange,
  rowHref,
  empty,
  busy = false,
  variant = "framed",
  label,
}: DataTableProps<T>) {
  const navigate = useNavigate()

  const onRowClick = (row: T) => (e: MouseEvent) => {
    if (!rowHref || (e.target as Element).closest(INTERACTIVE)) return
    navigate(rowHref(row))
  }

  const table = (
    <Table
      aria-label={label}
      aria-busy={busy || undefined}
      className="[&_td:first-child]:ps-4 [&_td:last-child]:pe-4 [&_th:first-child]:ps-4 [&_th:last-child]:pe-4"
    >
      <TableHeader>
        <TableRow className="hover:bg-transparent">
          {columns.map((col) => (
            <TableHead
              key={col.id}
              className={col.className}
              aria-sort={
                col.sortKey && sort?.key === col.sortKey
                  ? sort.dir === "asc"
                    ? "ascending"
                    : "descending"
                  : undefined
              }
            >
              {col.sortKey && onSortChange ? (
                <SortHeader sortKey={col.sortKey} sort={sort} onSortChange={onSortChange}>
                  {col.header}
                </SortHeader>
              ) : (
                col.header
              )}
            </TableHead>
          ))}
        </TableRow>
      </TableHeader>
      <TableBody className={cn("transition-opacity", busy && "opacity-60")}>
        {rows.length === 0 ? (
          <TableRow className="hover:bg-transparent">
            <TableCell colSpan={columns.length} className="py-8 text-center text-muted-foreground">
              {empty}
            </TableCell>
          </TableRow>
        ) : (
          rows.map((row) => (
            <TableRow
              key={rowKey(row)}
              className={cn(rowHref && "cursor-pointer")}
              onClick={rowHref ? onRowClick(row) : undefined}
            >
              {columns.map((col) => (
                <TableCell key={col.id} className={col.className}>
                  {col.cell(row)}
                </TableCell>
              ))}
            </TableRow>
          ))
        )}
      </TableBody>
    </Table>
  )

  return variant === "framed" ? <Card className="py-0">{table}</Card> : table
}

function SortHeader({
  sortKey,
  sort,
  onSortChange,
  children,
}: {
  sortKey: string
  sort?: SortState
  onSortChange: (sort: SortState) => void
  children: ReactNode
}) {
  const { t } = useTranslation()
  const active = sort?.key === sortKey
  // A new column starts ascending; the active one flips.
  const next: SortState = { key: sortKey, dir: active && sort.dir === "asc" ? "desc" : "asc" }
  const Icon = !active ? ChevronsUpDownIcon : sort.dir === "asc" ? ArrowUpIcon : ArrowDownIcon
  return (
    <button
      type="button"
      className="group -mx-1 inline-flex items-center gap-1 rounded-sm px-1 outline-none focus-visible:ring-3 focus-visible:ring-ring/50"
      onClick={() => onSortChange(next)}
    >
      {children}
      <Icon
        aria-hidden="true"
        className={cn(
          "size-3.5",
          active ? "text-foreground" : "text-muted-foreground/60 group-hover:text-muted-foreground"
        )}
      />
      <span className="sr-only">
        {next.dir === "asc" ? t("table.sortAscending") : t("table.sortDescending")}
      </span>
    </button>
  )
}
