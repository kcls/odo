import { useTranslation } from "react-i18next"
import { Bar, BarChart, CartesianGrid, XAxis, YAxis } from "recharts"

import type { DailyCounts } from "@/api/notify"
import { Card, CardAction, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import {
  ChartContainer,
  ChartLegend,
  ChartLegendContent,
  ChartTooltip,
  ChartTooltipContent,
  type ChartConfig,
} from "@/components/ui/chart"
import { useDirection } from "@/components/ui/direction"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { useFormatters } from "@/lib/format"
import type { Outcome } from "./outcome"

/*
 * Bottom to top. Outcomes wear the reserved status colors, and this is
 * the one order in which every adjacent pair clears the dataviz skill's
 * normal-vision floor in both themes (queued next to skipped did not).
 * Adjacent color-blind separation sits in the 6-8 band, which is legal
 * only with secondary encoding: the 2px gaps, the legend, the tooltip,
 * and the table view.
 */
const STACK: readonly Outcome[] = ["skipped", "failed", "delivered", "queued"]
const COLOR: Record<Outcome, string> = {
  skipped: "var(--muted-foreground)",
  failed: "var(--destructive)",
  delivered: "var(--success)",
  queued: "var(--warning)",
}

/** Deliveries per day, stacked by outcome, with a table of the same numbers. */
export function DailyChart({
  days,
  timezone,
  asTable,
  onAsTable,
}: {
  days: DailyCounts[]
  timezone: string
  asTable: boolean
  onAsTable: (asTable: boolean) => void
}) {
  const { t } = useTranslation("notifications")
  const fmt = useFormatters()
  const rtl = useDirection() === "rtl"
  const config = Object.fromEntries(
    STACK.map((o) => [o, { label: t(`status.${o}`), color: COLOR[o] }])
  ) satisfies ChartConfig
  const total = (d: DailyCounts) => d.delivered + d.failed + d.skipped + d.queued
  const empty = days.every((d) => total(d) === 0)
  const shortDay = (day: string) => fmt.calendarDateShort(day)

  return (
    <Card>
      <CardHeader>
        <CardTitle>
          <h2>{t("daily.title")}</h2>
        </CardTitle>
        <CardDescription>{t("daily.description", { timezone })}</CardDescription>
        <CardAction>
          <ToggleGroup
            type="single"
            variant="outline"
            size="sm"
            value={asTable ? "table" : "chart"}
            aria-label={t("daily.view")}
            // Single select that can't be cleared: ignore a deselect.
            onValueChange={(v) => v && onAsTable(v === "table")}
          >
            <ToggleGroupItem value="chart" className="px-3">
              {t("daily.chart")}
            </ToggleGroupItem>
            <ToggleGroupItem value="table" className="px-3">
              {t("daily.table")}
            </ToggleGroupItem>
          </ToggleGroup>
        </CardAction>
      </CardHeader>
      <CardContent>
        {empty ? (
          <p className="py-6 text-center text-sm text-muted-foreground">{t("daily.empty")}</p>
        ) : asTable ? (
          <div className="max-h-80 overflow-y-auto">
            <Table aria-label={t("daily.title")}>
              <TableHeader>
                <TableRow>
                  <TableHead>{t("daily.day")}</TableHead>
                  {[...STACK].reverse().map((o) => (
                    <TableHead key={o} className="text-end">
                      {t(`status.${o}`)}
                    </TableHead>
                  ))}
                  <TableHead className="text-end">{t("daily.total")}</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {[...days].reverse().map((d) => (
                  <TableRow key={d.day}>
                    <TableCell>{fmt.calendarDate(d.day)}</TableCell>
                    {[...STACK].reverse().map((o) => (
                      <TableCell key={o} className="text-end tabular-nums">
                        {fmt.count(d[o])}
                      </TableCell>
                    ))}
                    <TableCell className="text-end font-medium tabular-nums">{fmt.count(total(d))}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>
        ) : (
          <ChartContainer config={config} className="aspect-auto h-64 w-full">
            <BarChart data={days} accessibilityLayer>
              <CartesianGrid vertical={false} />
              <XAxis
                dataKey="day"
                tickLine={false}
                axisLine={false}
                tickMargin={8}
                minTickGap={24}
                tickFormatter={shortDay}
                reversed={rtl}
              />
              <YAxis
                width={36}
                tickLine={false}
                axisLine={false}
                allowDecimals={false}
                orientation={rtl ? "right" : "left"}
              />
              <ChartTooltip
                cursor={{ fill: "var(--muted)", opacity: 0.5 }}
                content={<ChartTooltipContent labelFormatter={(day) => fmt.calendarDate(String(day))} />}
              />
              <ChartLegend itemSorter={null} content={<ChartLegendContent />} />
              {STACK.map((o) => (
                <Bar
                  key={o}
                  dataKey={o}
                  stackId="outcome"
                  fill={`var(--color-${o})`}
                  // A 2px surface gap between stacked segments.
                  stroke="var(--card)"
                  strokeWidth={2}
                />
              ))}
            </BarChart>
          </ChartContainer>
        )}
      </CardContent>
    </Card>
  )
}
