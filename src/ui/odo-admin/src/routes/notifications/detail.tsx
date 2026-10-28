import { RotateCcwIcon } from "lucide-react"
import { useState } from "react"
import { useTranslation } from "react-i18next"
import { Link, useLoaderData, useRevalidator, type LoaderFunctionArgs } from "react-router"
import { toast } from "sonner"

import { ApiRequestError } from "@/api/client"
import { deliveriesApi, type DeliveryDetail } from "@/api/notify"
import { PERMS } from "@/auth/perms"
import { checkPerms } from "@/auth/session"
import { ConfirmDialog } from "@/components/confirm-dialog"
import { DetailHeader } from "@/components/detail-header"
import { Section } from "@/components/section"
import { StatusBadge } from "@/components/status-badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent } from "@/components/ui/card"
import { useReportError } from "@/hooks/use-report-error"
import { loadNamespaces } from "@/i18n"
import { useFormatters } from "@/lib/format"
import { orNotFound, requireIdParam } from "@/lib/loader"
import type { RouteHandle } from "@/lib/route-handle"
import { DeliveryStatus } from "./delivery-status"

export async function loader({ params }: LoaderFunctionArgs) {
  const id = requireIdParam(params.deliveryId)
  const [detail, perms] = await Promise.all([
    orNotFound(deliveriesApi.get(id)),
    checkPerms({
      canSeeGroups: PERMS.EMAIL_GROUP_READ,
      canSeeUsers: PERMS.USER_DETAIL_READ,
      canRetry: PERMS.DELIVERY_WRITE,
    }),
    loadNamespaces("notifications"),
  ])
  return { detail, ...perms }
}

export const handle: RouteHandle = {
  crumb: (match) =>
    (match.loaderData as Awaited<ReturnType<typeof loader>> | undefined)?.detail.delivery.subject ?? "",
}

type Fact = [string, React.ReactNode]

/** One delivery: what happened, to whom, what was sent, and why. */
export function Component() {
  const { detail, canSeeGroups, canSeeUsers, canRetry } = useLoaderData<typeof loader>()
  const { t } = useTranslation(["notifications", "common"])
  const fmt = useFormatters()
  const revalidator = useRevalidator()
  const reportError = useReportError()
  const [confirmingRetry, setConfirmingRetry] = useState(false)
  const d = detail.delivery
  // Only the email queue retries; in-app deliveries are delivered on enqueue.
  const retryable = canRetry && d.channel === "email" && (d.status === "failed" || d.status === "skipped")

  const retry = async () => {
    try {
      await deliveriesApi.retry(d.id)
      toast.success(t("retry.done"))
      await revalidator.revalidate()
    } catch (err) {
      if (err instanceof ApiRequestError && err.code === "DELIVERY_NOT_RETRYABLE") {
        toast.error(t("retry.notRetryable"))
        await revalidator.revalidate()
      } else {
        reportError(err, "Failed to retry delivery")
      }
    }
  }
  const none = <span className="text-muted-foreground">{t("common:none")}</span>
  const when = (iso: string | null | undefined) => (iso ? fmt.dateTime(iso) : none)

  const recipient = d.recipient
  const recipientHref =
    recipient &&
    (recipient.kind === "email_group"
      ? canSeeGroups && `/email-groups/${recipient.id}`
      : canSeeUsers && `/users/${recipient.id}`)

  const outcome: Fact[] = [
    [t("detail.facts.status"), <DeliveryStatus status={d.status} />],
    [
      t("detail.facts.recipient"),
      recipient ? (
        <span className="inline-flex flex-wrap items-center gap-1.5">
          {recipientHref ? (
            <Link to={recipientHref} className="font-medium hover:underline">
              {recipient.label}
            </Link>
          ) : (
            <span className="font-medium">{recipient.label}</span>
          )}
          <span className="font-mono text-xs text-muted-foreground">{recipient.detail}</span>
          {!recipient.active && <StatusBadge tone="warning">{t("inactive")}</StatusBadge>}
        </span>
      ) : (
        <span className="text-muted-foreground">{t("unknownRecipient")}</span>
      ),
    ],
    [t("detail.facts.channel"), channelLabel(d.channel, t)],
    [t("detail.facts.template"), d.template_code ? <span className="font-mono text-xs">{d.template_code}</span> : none],
    [t("detail.facts.queued"), when(d.created_at)],
    ...(d.scheduled_for ? ([[t("detail.facts.scheduledFor"), when(d.scheduled_for)]] as Fact[]) : []),
    [t("detail.facts.processed"), when(d.processed_at)],
    [t("detail.facts.attempts"), t("detail.attempts", { count: d.retry_count, max: d.max_retries })],
    ...(d.next_retry_at && d.status === "pending" ? ([[t("detail.facts.nextRetry"), when(d.next_retry_at)]] as Fact[]) : []),
    ...(d.error_code || d.error_message
      ? ([
          [
            t("detail.facts.error"),
            <span className="flex flex-col gap-0.5">
              {d.error_code && <span className="font-mono text-xs">{d.error_code}</span>}
              {d.error_message && <span className="break-words text-destructive">{d.error_message}</span>}
            </span>,
          ],
        ] as Fact[])
      : []),
    ...(d.channel === "in_app"
      ? ([
          [t("detail.facts.read"), when(detail.read_at)],
          [t("detail.facts.dismissed"), when(detail.dismissed_at)],
        ] as Fact[])
      : []),
  ]

  return (
    <div className="flex flex-col gap-8">
      <DetailHeader
        backTo=".."
        backLabel={t("detail.back")}
        title={d.subject || t("detail.untitled")}
        summary={[
          <DeliveryStatus status={d.status} />,
          channelLabel(d.channel, t),
          <span title={fmt.dateTime(d.created_at)}>{fmt.relativeTime(d.created_at)}</span>,
        ]}
        actions={
          retryable && (
            <Button variant="outline" onClick={() => setConfirmingRetry(true)}>
              <RotateCcwIcon data-icon="inline-start" />
              {t("detail.retry")}
            </Button>
          )
        }
      />

      <Section title={t("detail.outcome")}>
        <FactsCard facts={outcome} />
      </Section>

      {d.channel === "email" && <Addresses detail={detail} />}

      <Section title={t("detail.message")}>
        <Card size="sm">
          <CardContent className="flex flex-col gap-4">
            <Part label={t("detail.subject")}>
              <p className="font-medium">{d.subject || t("detail.untitled")}</p>
            </Part>
            <Part label={t("detail.body")}>
              {!detail.body ? (
                <p className="text-muted-foreground">{t("detail.noBody")}</p>
              ) : d.channel === "email" ? (
                // Email bodies are HTML as sent: render them in an empty
                // sandbox (no scripts, forms, or same-origin access), on the
                // light background mail clients use.
                <iframe
                  title={t("detail.htmlFrame")}
                  sandbox=""
                  srcDoc={detail.body}
                  className="h-96 w-full rounded-md border bg-white"
                />
              ) : (
                <pre className="font-sans text-sm whitespace-pre-wrap">{detail.body}</pre>
              )}
            </Part>
            {detail.action_url && (
              <Part label={t("detail.actionUrl")}>
                <span className="font-mono text-xs break-all">{detail.action_url}</span>
              </Part>
            )}
          </CardContent>
        </Card>
      </Section>

      <ConfirmDialog
        open={confirmingRetry}
        onOpenChange={setConfirmingRetry}
        title={t("retry.title")}
        description={t("retry.body", { recipient: recipient?.label ?? t("unknownRecipient") })}
        confirmLabel={t("retry.confirm")}
        onConfirm={retry}
      />

      <Section title={t("detail.event")}>
        {detail.event ? (
          <FactsCard
            facts={[
              [t("detail.eventFacts.source"), detail.event.source_service ?? none],
              [
                t("detail.eventFacts.entity"),
                detail.event.source_entity_type
                  ? t("detail.entity", {
                      type: detail.event.source_entity_type,
                      id: detail.event.source_entity_id ?? "?",
                    })
                  : none,
              ],
              [t("detail.eventFacts.createdBy"), detail.event.created_by_name ?? none],
              [t("detail.eventFacts.createdAt"), when(detail.event.created_at)],
              [t("detail.eventFacts.id"), <span className="font-mono text-xs">{detail.event.id}</span>],
            ]}
          />
        ) : (
          <p className="text-sm text-muted-foreground">{t("detail.noEvent")}</p>
        )}
      </Section>
    </div>
  )
}

function channelLabel(channel: string, t: ReturnType<typeof useTranslation<"notifications">>["t"]) {
  return channel === "email" || channel === "in_app" ? t(`channel.${channel}`) : channel
}

/** Which of a group's addresses took the email and which refused it. */
function Addresses({ detail }: { detail: DeliveryDetail }) {
  const { t } = useTranslation("notifications")
  const { delivered_addresses: delivered, failed_addresses: failed } = detail
  return (
    <Section title={t("detail.addresses")}>
      {delivered.length === 0 && failed.length === 0 ? (
        <p className="text-sm text-muted-foreground">{t("detail.noAddresses")}</p>
      ) : (
        <div className="grid gap-4 sm:grid-cols-2">
          <AddressList label={t("detail.deliveredTo")} addresses={delivered} />
          <AddressList label={t("detail.failedFor")} addresses={failed} failed />
        </div>
      )}
    </Section>
  )
}

function AddressList({ label, addresses, failed = false }: { label: string; addresses: string[]; failed?: boolean }) {
  const { t } = useTranslation("common")
  return (
    <Card size="sm">
      <CardContent className="flex flex-col gap-2">
        <h3 className="text-sm font-medium">
          {label} <span className="text-muted-foreground tabular-nums">({addresses.length})</span>
        </h3>
        {addresses.length === 0 ? (
          <p className="text-sm text-muted-foreground">{t("none")}</p>
        ) : (
          <ul className={failed ? "font-mono text-xs text-destructive" : "font-mono text-xs"}>
            {addresses.map((a) => (
              <li key={a} className="break-all">
                {a}
              </li>
            ))}
          </ul>
        )}
      </CardContent>
    </Card>
  )
}

function FactsCard({ facts }: { facts: Fact[] }) {
  return (
    <Card size="sm">
      <CardContent>
        <dl className="grid grid-cols-[minmax(8rem,max-content)_1fr] gap-x-6 gap-y-2">
          {facts.map(([label, value]) => (
            <div key={label} className="contents">
              <dt className="text-muted-foreground">{label}</dt>
              <dd className="min-w-0">{value}</dd>
            </div>
          ))}
        </dl>
      </CardContent>
    </Card>
  )
}

function Part({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-1.5">
      <h3 className="text-xs font-medium tracking-wide text-muted-foreground uppercase">{label}</h3>
      {children}
    </div>
  )
}
