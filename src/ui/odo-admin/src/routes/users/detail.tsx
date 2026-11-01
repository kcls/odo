import { BanIcon, KeyRoundIcon, PencilIcon, RotateCcwIcon, UserCheckIcon, UserXIcon } from "lucide-react"
import { useState } from "react"
import { useTranslation } from "react-i18next"
import {
  useLoaderData,
  useRevalidator,
  useSearchParams,
  type LoaderFunctionArgs,
  type ShouldRevalidateFunctionArgs,
} from "react-router"
import { toast } from "sonner"

import { authzApi } from "@/api/authz"
import { usersApi, type AccountStatus, type UserDetail, type UserSession } from "@/api/users"
import { PERMS } from "@/auth/perms"
import { checkPerms, useCurrentUser } from "@/auth/session"
import { ConfirmDialog } from "@/components/confirm-dialog"
import { DataTable, type Column } from "@/components/data-table/data-table"
import { DetailHeader } from "@/components/detail-header"
import { DisabledReason } from "@/components/disabled-reason"
import { Section } from "@/components/section"
import { StatusBadge } from "@/components/status-badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent } from "@/components/ui/card"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { useReportError } from "@/hooks/use-report-error"
import { loadNamespaces } from "@/i18n"
import { useFormatters } from "@/lib/format"
import { orNotFound, requireIdParam } from "@/lib/loader"
import type { RouteHandle } from "@/lib/route-handle"
import { AccountStatusBadge } from "./account-status"
import { readTab, tabParams, type UserTab } from "./detail-tabs"
import { EditUserDialog } from "./edit-user-dialog"
import { EffectivePermissions } from "./effective-permissions"
import { RoleAssignments } from "./role-assignments"
import { SetPasswordDialog } from "./set-password-dialog"

export async function loader({ params, request }: LoaderFunctionArgs) {
  const id = requireIdParam(params.userId)
  const [detail, perms] = await Promise.all([
    orNotFound(usersApi.getDetail(id)),
    checkPerms({
      canEdit: PERMS.USER_WRITE,
      canReadRoles: PERMS.USER_ROLE_READ,
      canAssignRoles: PERMS.USER_ROLE_WRITE,
    }),
    loadNamespaces("users"),
  ])
  // Effective scope is the page's costliest read, so it's fetched only
  // for its own tab. It's supplementary: a failure shows in the tab
  // instead of failing the page.
  const tab = readTab(new URL(request.url).searchParams, shownTabs(detail, perms.canReadRoles))
  const permScopes =
    tab === "permissions"
      ? await authzApi.permScopes(id).catch((err) => {
        console.error("Failed to load effective permissions", err)
          return "failed" as const
        })
      : null
  return { detail, permScopes, ...perms }
}

/**
 * Switching tabs reuses the loaded user. Only the permissions tab has
 * data of its own, so it reloads on opening (fresh after role changes).
 */
export function shouldRevalidate({ currentUrl, nextUrl, defaultShouldRevalidate }: ShouldRevalidateFunctionArgs) {
  if (currentUrl.pathname === nextUrl.pathname && currentUrl.search !== nextUrl.search) {
    return nextUrl.searchParams.get("tab") === "permissions"
  }
  return defaultShouldRevalidate
}

/** Effective permissions need the role read perm; SAML only shows when there's SAML to show. */
function shownTabs(detail: UserDetail, canReadRoles: boolean): UserTab[] {
  const hasSaml =
    detail.user.auth_method === "saml" || detail.saml_identities.length > 0 || detail.saml_attributes.length > 0
  return [
    "overview",
    ...(canReadRoles ? (["permissions"] as const) : []),
    "sessions",
    ...(hasSaml ? (["saml"] as const) : []),
  ]
}

export const handle: RouteHandle = {
  crumb: (match) => (match.loaderData as Awaited<ReturnType<typeof loader>> | undefined)?.detail.user.display_name ?? "",
}

/**
 * Everything about one user, in tabs below a header that keeps their
 * status and actions in view: account facts and role assignments
 * (managed here; there is no separate user-roles tool), effective
 * permissions, recent sessions, and SAML identity. The tab is `?tab=`.
 */
export function Component() {
  const { detail, permScopes, canEdit, canReadRoles, canAssignRoles } = useLoaderData<typeof loader>()
  const { t } = useTranslation(["users", "common"])
  const revalidator = useRevalidator()
  const reportError = useReportError()
  const [editing, setEditing] = useState(false)
  const [settingPassword, setSettingPassword] = useState(false)
  const [confirmingDelete, setConfirmingDelete] = useState(false)
  const [confirmingDeactivate, setConfirmingDeactivate] = useState(false)
  const currentUser = useCurrentUser()
  const [searchParams, setSearchParams] = useSearchParams()
  const tabs = shownTabs(detail, canReadRoles)
  const tab = readTab(searchParams, tabs)
  const showTab = (next: UserTab) => setSearchParams((prev) => tabParams(prev, next))

  const user = detail.user
  // SAML accounts belong to the identity provider, apart from their status.
  const editable = canEdit && user.auth_method === "local"
  // A deleted account is already blocked from signing in.
  const canSetStatus = canEdit && !user.deleted_at

  const setDeleted = async (deleted: boolean) => {
    try {
      await usersApi.update(user.id, { deleted })
      toast.success(t(deleted ? "delete.done" : "delete.restored", { name: user.display_name }))
      await revalidator.revalidate()
    } catch (err) {
      reportError(err, "Failed to update deletion state")
    }
  }

  const setStatus = async (status: AccountStatus) => {
    try {
      await usersApi.update(user.id, { status })
      const key = status === "inactive" ? "deactivate.done" : "deactivate.reactivated"
      toast.success(t(key, { name: user.display_name }))
      await revalidator.revalidate()
    } catch (err) {
      reportError(err, "Failed to update account status")
    }
  }

  let statusAction = null
  if (canSetStatus && user.status === "inactive") {
    statusAction = (
      <Button variant="outline" onClick={() => void setStatus("active")}>
        <UserCheckIcon data-icon="inline-start" />
        {t("detail.reactivate")}
      </Button>
    )
  } else if (canSetStatus && currentUser?.id === user.id) {
    statusAction = (
      <DisabledReason label={t("detail.deactivate")} reason={t("detail.selfDeactivate")}>
        <Button variant="outline" disabled aria-hidden tabIndex={-1}>
          <BanIcon data-icon="inline-start" />
          {t("detail.deactivate")}
        </Button>
      </DisabledReason>
    )
  } else if (canSetStatus) {
    statusAction = (
      <Button variant="outline" onClick={() => setConfirmingDeactivate(true)}>
        <BanIcon data-icon="inline-start" />
        {t("detail.deactivate")}
      </Button>
    )
  }

  const summary = [
    user.username && <span className="font-mono text-xs">{user.username}</span>,
    user.email,
    user.status && <AccountStatusBadge status={user.status} />,
    <StatusBadge tone="info">{user.auth_method}</StatusBadge>,
    user.deleted_at && <StatusBadge tone="danger">{t("status.deleted")}</StatusBadge>,
  ].filter(Boolean)

  return (
    <div className="flex flex-col gap-8">
      <DetailHeader
        backTo=".."
        backLabel={t("detail.back")}
        title={user.display_name}
        summary={summary}
        actions={
          (editable || statusAction) && (
            <>
              {editable && (
                <Button variant="outline" onClick={() => setEditing(true)}>
                  <PencilIcon data-icon="inline-start" />
                  {t("detail.edit")}
                </Button>
              )}
              {/* A deleted account must be restored before it gets a password. */}
              {editable && !user.deleted_at && (
                <Button variant="outline" onClick={() => setSettingPassword(true)}>
                  <KeyRoundIcon data-icon="inline-start" />
                  {t("detail.setPassword")}
                </Button>
              )}
              {statusAction}
              {editable &&
                (user.deleted_at ? (
                  <Button variant="outline" onClick={() => void setDeleted(false)}>
                    <RotateCcwIcon data-icon="inline-start" />
                    {t("detail.restore")}
                  </Button>
                ) : (
                  <Button variant="destructive" onClick={() => setConfirmingDelete(true)}>
                    <UserXIcon data-icon="inline-start" />
                    {t("detail.markDeleted")}
                  </Button>
                ))}
            </>
          )
        }
      />

      <Tabs value={tab} onValueChange={(v) => showTab(v as UserTab)} className="gap-6">
        <TabsList aria-label={t("detail.tabs.label")}>
          {tabs.map((id) => (
            <TabsTrigger key={id} value={id}>
              {id === "sessions" ? t("detail.tabs.sessions", { count: detail.sessions.length }) : t(`detail.tabs.${id}`)}
            </TabsTrigger>
          ))}
        </TabsList>

        <TabsContent value="overview" className="flex flex-col gap-8">
          <AccountSection detail={detail} />
          <RoleAssignments
            user={user}
            assignments={detail.roles}
            canAssign={canAssignRoles}
            onChanged={() => void revalidator.revalidate()}
            onViewPermissions={canReadRoles ? () => showTab("permissions") : undefined}
          />
        </TabsContent>
        {canReadRoles && (
          <TabsContent value="permissions">
            <EffectivePermissions scopes={permScopes} />
          </TabsContent>
        )}
        <TabsContent value="sessions">
          <SessionsSection sessions={detail.sessions} />
        </TabsContent>
        {tabs.includes("saml") && (
          <TabsContent value="saml">
            <SamlSection detail={detail} />
          </TabsContent>
        )}
      </Tabs>

      <EditUserDialog
        user={editing ? user : null}
        onOpenChange={setEditing}
        onSaved={() => void revalidator.revalidate()}
      />
      <SetPasswordDialog
        user={settingPassword ? user : null}
        onOpenChange={setSettingPassword}
        // The account section shows lockout and failed attempts.
        onDone={() => void revalidator.revalidate()}
      />
      <ConfirmDialog
        open={confirmingDelete}
        onOpenChange={setConfirmingDelete}
        title={t("delete.title")}
        description={t("delete.body", { name: user.display_name })}
        confirmLabel={t("detail.markDeleted")}
        destructive
        onConfirm={() => setDeleted(true)}
      />
      <ConfirmDialog
        open={confirmingDeactivate}
        onOpenChange={setConfirmingDeactivate}
        title={t("deactivate.title")}
        description={t("deactivate.body", { name: user.display_name })}
        confirmLabel={t("detail.deactivate")}
        destructive
        onConfirm={() => setStatus("inactive")}
      />
    </div>
  )
}

function AccountSection({ detail }: { detail: UserDetail }) {
  const { t } = useTranslation(["users", "common"])
  const fmt = useFormatters()
  const u = detail.user
  const local = detail.local_account
  const name = [u.first_given_name, u.second_given_name, u.family_name].filter(Boolean).join(" ")
  return (
    <Section title={t("detail.account")}>
      <Card size="sm">
        <CardContent>
          <Facts
            items={[
              [t("detail.facts.name"), name || t("common:none")],
              [t("detail.facts.created"), fmt.dateTime(u.created_at)],
              [t("detail.facts.lastLogin"), fmt.dateTime(u.last_login_at)],
              ...(u.deleted_at
                ? ([[t("detail.facts.deleted"), fmt.dateTime(u.deleted_at)]] as const)
                : []),
              [
                t("detail.facts.passwordLogin"),
                local ? (
                  <span className="inline-flex flex-wrap items-center gap-2">
                    {t("detail.passwordYes")}
                    {local.failed_login_attempts > 0 && (
                      <StatusBadge tone="warning">
                        {t("detail.failedAttempts", { count: local.failed_login_attempts })}
                      </StatusBadge>
                    )}
                    {local.locked_until && <StatusBadge tone="danger">{t("detail.locked")}</StatusBadge>}
                  </span>
                ) : (
                  t("detail.passwordNo")
                ),
              ],
            ]}
          />
        </CardContent>
      </Card>
    </Section>
  )
}

function SamlSection({ detail }: { detail: UserDetail }) {
  const { t } = useTranslation("users")
  const { saml_identities: identities, saml_attributes: attributes } = detail
  return (
    <Section title={t("detail.saml")}>
      {identities.length === 0 && attributes.length === 0 ? (
        <p className="text-sm text-muted-foreground">{t("detail.samlNone")}</p>
      ) : (
        <Card size="sm">
          <CardContent className="flex flex-col gap-4">
            {identities.length > 0 && (
              <div className="flex flex-col gap-2">
                <h3 className="text-sm font-medium text-muted-foreground">{t("detail.samlIdentities")}</h3>
                <ul className="flex flex-col gap-1">
                  {identities.map((i) => (
                    <li key={i.idp}>
                      <span className="font-medium">{i.idp_name}</span>{" "}
                      <span className="font-mono text-xs text-muted-foreground">{i.name_id}</span>
                    </li>
                  ))}
                </ul>
              </div>
            )}
            {attributes.length > 0 && (
              <div className="flex flex-col gap-2">
                <h3 className="text-sm font-medium text-muted-foreground">{t("detail.samlAttributes")}</h3>
                <Facts items={attributes.map((a) => [<span title={a.key}>{a.label || a.key}</span>, a.value])} />
              </div>
            )}
          </CardContent>
        </Card>
      )}
    </Section>
  )
}

function SessionsSection({ sessions }: { sessions: UserSession[] }) {
  const { t } = useTranslation(["users", "common"])
  const fmt = useFormatters()
  const columns: Column<UserSession>[] = [
    { id: "started", header: t("detail.sessionColumns.started"), cell: (s) => fmt.dateTimeShort(s.created_at) },
    { id: "lastActivity", header: t("detail.sessionColumns.lastActivity"), cell: (s) => fmt.dateTimeShort(s.last_activity_at) },
    { id: "expires", header: t("detail.sessionColumns.expires"), cell: (s) => fmt.dateTimeShort(s.expires_at) },
    { id: "method", header: t("detail.sessionColumns.method"), cell: (s) => <span className="font-mono text-xs">{s.auth_method}</span> },
    { id: "orgUnit", header: t("detail.sessionColumns.orgUnit"), cell: (s) => s.org_unit_label },
    { id: "ip", header: t("detail.sessionColumns.ip"), cell: (s) => <span className="font-mono text-xs">{s.ip_address}</span> },
    {
      id: "client",
      header: t("detail.sessionColumns.client"),
      className: "max-w-64 truncate",
      cell: (s) => <span title={s.user_agent ?? undefined}>{s.user_agent}</span>,
    },
    {
      id: "status",
      header: <span className="sr-only">{t("detail.sessionColumns.status")}</span>,
      cell: (s) => s.is_active && <StatusBadge tone="success">{t("status.sessionActive")}</StatusBadge>,
    },
  ]
  return (
    <Section title={t("detail.sessionsCount", { count: sessions.length })}>
      <DataTable
        label={t("detail.sessions")}
        rows={sessions}
        columns={columns}
        rowKey={(s) => `${s.created_at}-${s.ip_address}-${s.expires_at}`}
        empty={t("detail.sessionsNone")}
      />
    </Section>
  )
}

/** A two-column label/value list. */
function Facts({ items }: { items: readonly (readonly [React.ReactNode, React.ReactNode])[] }) {
  return (
    <dl className="grid grid-cols-[minmax(8rem,max-content)_1fr] gap-x-6 gap-y-2">
      {items.map(([label, value], i) => (
        <div key={i} className="contents">
          <dt className="text-muted-foreground">{label}</dt>
          <dd className="min-w-0 break-words">{value}</dd>
        </div>
      ))}
    </dl>
  )
}
