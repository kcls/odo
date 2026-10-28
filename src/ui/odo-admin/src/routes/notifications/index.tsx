import { XIcon } from "lucide-react"
import { useTranslation } from "react-i18next"
import {
  Link,
  useLoaderData,
  useSearchParams,
  type LoaderFunctionArgs,
  type ShouldRevalidateFunctionArgs,
} from "react-router"

import {
  deliveriesApi,
  type DeliveryRecipient,
  type DeliveryRow,
  type RecipientActivityRow,
} from "@/api/notify"
import { PERMS } from "@/auth/perms"
import { checkPerms } from "@/auth/session"
import { DataTable, type Column } from "@/components/data-table/data-table"
import { Paginator } from "@/components/data-table/paginator"
import { PageHeader } from "@/components/page-header"
import { SearchInput } from "@/components/search-input"
import { StatTile } from "@/components/stat-tile"
import { StatusBadge } from "@/components/status-badge"
import { Button } from "@/components/ui/button"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { useBusy } from "@/hooks/use-busy"
import { loadNamespaces } from "@/i18n"
import { useFormatters } from "@/lib/format"
import {
  listRequest,
  readListState,
  redirectPastLastPage,
  useListState,
  type ListDefaults,
} from "@/lib/list-state"
import { DailyChart } from "./daily-chart"
import { DeliveryStatus } from "./delivery-status"
import { isOutcome, OUTCOME_STYLE, OUTCOMES, outcomeTotals, statusesFor } from "./outcome"

/*
 * Everything this page shows is in its URL:
 *   window   1d | 7d (default) | 30d | 90d
 *   channel  email (default) | in_app | all
 *   view     recipients (default) | deliveries
 *   kind     email_group | user          (recipients view)
 *   status   an outcome                    (deliveries view)
 *   group / user   one recipient's deliveries (deliveries view)
 *   daily    table, to show the per-day numbers as a table
 * plus each view's search/sort/page list state.
 */

const WINDOWS = { "1d": 1, "7d": 7, "30d": 30, "90d": 90 } as const
type WindowKey = keyof typeof WINDOWS
const CHANNELS = ["email", "in_app", "all"] as const
type ChannelKey = (typeof CHANNELS)[number]
type View = "recipients" | "deliveries"
type Kind = "email_group" | "user"

const RECIPIENT_LIST: ListDefaults = {
  sort: { key: "last", dir: "desc" },
  sortKeys: ["total", "failed", "last"],
}
const DELIVERY_LIST: ListDefaults = {
  sort: { key: "created", dir: "desc" },
  sortKeys: ["created", "processed", "status", "template"],
}

/** Params owned by one view; switching views drops them. */
const VIEW_PARAMS = ["kind", "status", "group", "user", "q", "sort", "dir", "page", "size"]

function readFilters(params: URLSearchParams) {
  const w = params.get("window")
  const c = params.get("channel")
  const k = params.get("kind")
  const status = params.get("status")
  const group = Number(params.get("group"))
  const user = Number(params.get("user"))
  return {
    window: (w && w in WINDOWS ? w : "7d") as WindowKey,
    channel: (CHANNELS as readonly string[]).includes(c ?? "") ? (c as ChannelKey) : "email",
    view: (params.get("view") === "deliveries" ? "deliveries" : "recipients") as View,
    kind: k === "email_group" || k === "user" ? (k as Kind) : null,
    status: isOutcome(status) ? status : null,
    group: Number.isInteger(group) && group > 0 ? group : null,
    user: Number.isInteger(user) && user > 0 ? user : null,
  }
}

export async function loader({ request }: LoaderFunctionArgs) {
  const url = new URL(request.url)
  const f = readFilters(url.searchParams)
  const since = new Date(Date.now() - WINDOWS[f.window] * 24 * 3600 * 1000).toISOString()
  const channel = f.channel === "all" ? undefined : f.channel

  const listDefaults = f.view === "recipients" ? RECIPIENT_LIST : DELIVERY_LIST
  const list = readListState(url.searchParams, listDefaults)

  // Days are counted in the viewer's own zone.
  const timezone = Intl.DateTimeFormat().resolvedOptions().timeZone
  const [summary, daily, recipients, deliveries, perms] = await Promise.all([
    deliveriesApi.summary({ since, channel }),
    deliveriesApi.daily({ since, channel, timezone }),
    f.view === "recipients"
      ? deliveriesApi.recipientsPage({ ...listRequest(list), since, channel, kind: f.kind ?? undefined })
      : null,
    f.view === "deliveries"
      ? deliveriesApi.listPage({
          ...listRequest(list),
          since,
          channel,
          status: f.status ? statusesFor(f.status) : undefined,
          recipient_email_group: f.group ?? undefined,
          recipient_user: f.user ?? undefined,
        })
      : null,
    checkPerms({ canSeeGroups: PERMS.EMAIL_GROUP_READ, canSeeUsers: PERMS.USER_DETAIL_READ }),
    loadNamespaces("notifications"),
  ])
  redirectPastLastPage(url, list, (recipients ?? deliveries)?.total ?? 0, listDefaults)
  return { summary, daily, recipients, deliveries, ...perms }
}

/** Switching the per-day numbers between chart and table needs no refetch. */
export function shouldRevalidate({ currentUrl, nextUrl, defaultShouldRevalidate }: ShouldRevalidateFunctionArgs) {
  const strip = (url: URL) => {
    const params = new URLSearchParams(url.search)
    params.delete("daily")
    return `${url.pathname}?${params}`
  }
  if (currentUrl.search !== nextUrl.search && strip(currentUrl) === strip(nextUrl)) return false
  return defaultShouldRevalidate
}

export function Component() {
  const { summary, daily, recipients, deliveries, canSeeGroups, canSeeUsers } = useLoaderData<typeof loader>()
  const { t } = useTranslation("notifications")
  const fmt = useFormatters()
  const [params, setParams] = useSearchParams()
  const f = readFilters(params)
  const totals = outcomeTotals(summary.counts)

  /** Set page-level params; a view change also drops the old view's. */
  const update = (patch: Record<string, string | null>, defaults: Record<string, string> = {}) =>
    setParams((prev) => {
      const next = new URLSearchParams(prev)
      if ("view" in patch) for (const p of VIEW_PARAMS) next.delete(p)
      // Narrowing what's shown changes what each page holds.
      next.delete("page")
      for (const [key, value] of Object.entries(patch)) {
        if (value === null || value === defaults[key]) next.delete(key)
        else next.set(key, value)
      }
      return next
    })

  /** The deliveries view, narrowed; keeps the time range and channel. */
  const deliveriesHref = (extra: Record<string, string>) => {
    const next = new URLSearchParams()
    for (const key of ["window", "channel"]) {
      const v = params.get(key)
      if (v) next.set(key, v)
    }
    next.set("view", "deliveries")
    for (const [k, v] of Object.entries(extra)) next.set(k, v)
    return `?${next}`
  }

  const recipientLink = (r: DeliveryRecipient | null | undefined) => (
    <RecipientName recipient={r} canSeeGroups={canSeeGroups} canSeeUsers={canSeeUsers} />
  )

  return (
    <div className="flex flex-col gap-6">
      <PageHeader title={t("title")} description={t("subtitle")} />

      <div className="flex flex-wrap items-center gap-2">
        <Select value={f.window} onValueChange={(v) => update({ window: v }, { window: "7d" })}>
          <SelectTrigger className="w-44" aria-label={t("window.label")}>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {(Object.keys(WINDOWS) as WindowKey[]).map((w) => (
              <SelectItem key={w} value={w}>
                {t(`window.${w}`)}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <Select value={f.channel} onValueChange={(v) => update({ channel: v }, { channel: "email" })}>
          <SelectTrigger className="w-40" aria-label={t("channel.label")}>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {CHANNELS.map((c) => (
              <SelectItem key={c} value={c}>
                {t(`channel.${c}`)}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>

      <section aria-label={t("tiles")} className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        {OUTCOMES.map((outcome) => (
          <StatTile
            key={outcome}
            label={t(`status.${outcome}`)}
            value={fmt.count(totals[outcome])}
            hint={t(`statusHint.${outcome}`)}
            icon={OUTCOME_STYLE[outcome].icon}
            iconClass={OUTCOME_STYLE[outcome].iconClass}
            to={deliveriesHref({ status: outcome })}
            linkLabel={t("showFiltered", { status: t(`status.${outcome}`).toLowerCase() })}
          />
        ))}
      </section>

      <DailyChart
        days={daily.days}
        timezone={daily.timezone}
        asTable={params.get("daily") === "table"}
        onAsTable={(asTable) =>
          setParams(
            (prev) => {
              if (asTable) prev.set("daily", "table")
              else prev.delete("daily")
              return prev
            },
            { replace: true }
          )
        }
      />

      <Tabs value={f.view} onValueChange={(v) => update({ view: v }, { view: "recipients" })}>
        <TabsList aria-label={t("views.label")}>
          <TabsTrigger value="recipients">{t("views.recipients")}</TabsTrigger>
          <TabsTrigger value="deliveries">{t("views.deliveries")}</TabsTrigger>
        </TabsList>
      </Tabs>

      {recipients && (
        <RecipientsView
          page={recipients}
          kind={f.kind}
          onKind={(kind) => update({ kind })}
          recipientLink={recipientLink}
          deliveriesHref={deliveriesHref}
        />
      )}
      {deliveries && (
        <DeliveriesView
          page={deliveries}
          filters={f}
          onStatus={(status) => update({ status })}
          onClearRecipient={() => update({ group: null, user: null })}
          recipientLink={recipientLink}
        />
      )}
    </div>
  )
}

function RecipientName({
  recipient,
  canSeeGroups,
  canSeeUsers,
}: {
  recipient: DeliveryRecipient | null | undefined
  canSeeGroups: boolean
  canSeeUsers: boolean
}) {
  const { t } = useTranslation("notifications")
  if (!recipient) return <span className="text-muted-foreground">{t("unknownRecipient")}</span>
  const href =
    recipient.kind === "email_group"
      ? canSeeGroups && `/email-groups/${recipient.id}`
      : canSeeUsers && `/users/${recipient.id}`
  return (
    <div className="flex min-w-0 flex-col">
      <span className="flex flex-wrap items-center gap-1.5">
        {href ? (
          <Link to={href} className="font-medium hover:underline focus-visible:underline">
            {recipient.label}
          </Link>
        ) : (
          <span className="font-medium">{recipient.label}</span>
        )}
        <StatusBadge tone="neutral">{t(`recipientKind.${recipient.kind}`)}</StatusBadge>
        {!recipient.active && <StatusBadge tone="warning">{t("inactive")}</StatusBadge>}
      </span>
      <span className="truncate font-mono text-xs text-muted-foreground">{recipient.detail}</span>
    </div>
  )
}

function RecipientsView({
  page,
  kind,
  onKind,
  recipientLink,
  deliveriesHref,
}: {
  page: { rows: RecipientActivityRow[]; total: number }
  kind: Kind | null
  onKind: (kind: string | null) => void
  recipientLink: (r: DeliveryRecipient | null | undefined) => React.ReactNode
  deliveriesHref: (extra: Record<string, string>) => string
}) {
  const { t } = useTranslation("notifications")
  const fmt = useFormatters()
  const [list, setList] = useListState(RECIPIENT_LIST)
  const busy = useBusy()

  const count = (n: number, emphasize = false) => (
    <span className={emphasize && n > 0 ? "font-medium text-destructive" : n === 0 ? "text-muted-foreground" : undefined}>
      {fmt.count(n)}
    </span>
  )
  const when = (iso: string | null | undefined) =>
    iso ? <span title={fmt.dateTime(iso)}>{fmt.relativeTime(iso)}</span> : <span className="text-muted-foreground">—</span>

  const columns: Column<RecipientActivityRow>[] = [
    { id: "recipient", header: t("recipients.columns.recipient"), cell: (r) => recipientLink(r.recipient) },
    {
      id: "total",
      header: t("recipients.columns.total"),
      sortKey: "total",
      className: "text-end tabular-nums",
      cell: (r) =>
        r.recipient ? (
          <Link
            to={deliveriesHref({ [r.recipient.kind === "email_group" ? "group" : "user"]: String(r.recipient.id) })}
            aria-label={t("recipients.showDeliveries", { name: r.recipient.label })}
            className="font-medium hover:underline focus-visible:underline"
          >
            {fmt.count(r.total)}
          </Link>
        ) : (
          fmt.count(r.total)
        ),
    },
    { id: "delivered", header: t("recipients.columns.delivered"), className: "text-end tabular-nums", cell: (r) => count(r.delivered) },
    {
      id: "failed",
      header: t("recipients.columns.failed"),
      sortKey: "failed",
      className: "text-end tabular-nums",
      cell: (r) => count(r.failed, true),
    },
    { id: "skipped", header: t("recipients.columns.skipped"), className: "text-end tabular-nums", cell: (r) => count(r.skipped) },
    { id: "queued", header: t("recipients.columns.queued"), className: "text-end tabular-nums", cell: (r) => count(r.queued) },
    {
      id: "lastSent",
      header: t("recipients.columns.lastSent"),
      sortKey: "last",
      className: "whitespace-nowrap",
      cell: (r) => when(r.last_delivered_at ?? r.last_created_at),
    },
    {
      id: "lastFailed",
      header: t("recipients.columns.lastFailed"),
      className: "whitespace-nowrap",
      cell: (r) => when(r.last_failed_at),
    },
  ]

  return (
    <div className="flex flex-col gap-4">
      <Select value={kind ?? "all"} onValueChange={(v) => onKind(v === "all" ? null : v)}>
        <SelectTrigger className="w-48" aria-label={t("kind.label")}>
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value="all">{t("kind.all")}</SelectItem>
          <SelectItem value="email_group">{t("kind.email_group")}</SelectItem>
          <SelectItem value="user">{t("kind.user")}</SelectItem>
        </SelectContent>
      </Select>
      <DataTable
        label={t("views.recipients")}
        rows={page.rows}
        columns={columns}
        rowKey={(r) => (r.recipient ? `${r.recipient.kind}:${r.recipient.id}` : r.last_created_at)}
        sort={list.sort}
        onSortChange={(sort) => setList({ sort })}
        empty={t("recipients.empty")}
        busy={busy}
      />
      <Paginator total={page.total} pageIndex={list.pageIndex} pageSize={list.pageSize} onChange={setList} />
    </div>
  )
}

function DeliveriesView({
  page,
  filters,
  onStatus,
  onClearRecipient,
  recipientLink,
}: {
  page: { rows: DeliveryRow[]; total: number }
  filters: ReturnType<typeof readFilters>
  onStatus: (status: string | null) => void
  onClearRecipient: () => void
  recipientLink: (r: DeliveryRecipient | null | undefined) => React.ReactNode
}) {
  const { t } = useTranslation("notifications")
  const fmt = useFormatters()
  const [list, setList] = useListState(DELIVERY_LIST)
  const busy = useBusy()
  // The rows name the recipient being filtered to, if any came back.
  const filteredTo =
    (filters.group || filters.user) &&
    page.rows.find((r) => r.recipient && r.recipient.id === (filters.group ?? filters.user))?.recipient

  const columns: Column<DeliveryRow>[] = [
    {
      id: "created",
      header: t("deliveries.columns.created"),
      sortKey: "created",
      className: "whitespace-nowrap",
      cell: (d) => <span title={fmt.dateTime(d.created_at)}>{fmt.relativeTime(d.created_at)}</span>,
    },
    {
      id: "subject",
      header: t("deliveries.columns.subject"),
      cell: (d) => (
        <Link to={String(d.id)} className="line-clamp-2 font-medium hover:underline focus-visible:underline">
          {d.subject}
        </Link>
      ),
    },
    { id: "recipient", header: t("deliveries.columns.recipient"), cell: (d) => recipientLink(d.recipient) },
    {
      id: "status",
      header: t("deliveries.columns.status"),
      sortKey: "status",
      cell: (d) => (
        <div className="flex flex-col items-start gap-1">
          <DeliveryStatus status={d.status} />
          {d.failed_address_count > 0 && (
            <span className="text-xs text-destructive">{t("deliveries.partial", { count: d.failed_address_count })}</span>
          )}
          {d.status === "pending" && d.retry_count > 0 && (
            <span className="text-xs text-muted-foreground">
              {t("deliveries.retries", { count: d.retry_count, max: d.max_retries })}
            </span>
          )}
        </div>
      ),
    },
    {
      id: "template",
      header: t("deliveries.columns.template"),
      sortKey: "template",
      cell: (d) => d.template_code && <span className="font-mono text-xs">{d.template_code}</span>,
    },
  ]

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-center gap-2">
        <SearchInput
          value={list.q}
          onCommit={(q) => setList({ q })}
          label={t("deliveries.search")}
          placeholder={t("deliveries.search")}
        />
        <Select value={filters.status ?? "all"} onValueChange={(v) => onStatus(v === "all" ? null : v)}>
          <SelectTrigger className="w-44" aria-label={t("deliveries.statusFilter")}>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="all">{t("deliveries.allStatuses")}</SelectItem>
            {OUTCOMES.map((o) => (
              <SelectItem key={o} value={o}>
                {t(`status.${o}`)}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        {(filters.group || filters.user) && (
          <Button variant="outline" size="sm" onClick={onClearRecipient}>
            <XIcon data-icon="inline-start" />
            {filteredTo
              ? t("deliveries.filteredTo", { name: filteredTo.label })
              : t("deliveries.clearRecipient")}
          </Button>
        )}
      </div>
      <DataTable
        label={t("views.deliveries")}
        rows={page.rows}
        columns={columns}
        rowKey={(d) => d.id}
        rowHref={(d) => String(d.id)}
        sort={list.sort}
        onSortChange={(sort) => setList({ sort })}
        empty={list.q || filters.status ? t("deliveries.noMatches") : t("deliveries.empty")}
        busy={busy}
      />
      <Paginator total={page.total} pageIndex={list.pageIndex} pageSize={list.pageSize} onChange={setList} />
    </div>
  )
}
