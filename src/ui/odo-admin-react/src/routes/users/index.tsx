import { PlusIcon, UsersIcon } from "lucide-react"
import { useState } from "react"
import { useTranslation } from "react-i18next"
import { Link, useLoaderData, useNavigate, useSearchParams, type LoaderFunctionArgs } from "react-router"

import { usersApi, type UserSummary } from "@/api/users"
import { PERMS } from "@/auth/perms"
import { checkPerms } from "@/auth/session"
import { DataTable, type Column } from "@/components/data-table/data-table"
import { PageHeader } from "@/components/page-header"
import { SearchInput } from "@/components/search-input"
import { StatusBadge } from "@/components/status-badge"
import { Button } from "@/components/ui/button"
import { Empty, EmptyDescription, EmptyHeader, EmptyMedia } from "@/components/ui/empty"
import { useBusy } from "@/hooks/use-busy"
import { loadNamespaces } from "@/i18n"
import { CreateUserDialog } from "./create-user-dialog"

/** odo-auth's search has no total, so ask for one extra to detect more. */
const MAX_RESULTS = 50

export async function loader({ request }: LoaderFunctionArgs) {
  const q = new URL(request.url).searchParams.get("q")?.trim() ?? ""
  const [found, perms] = await Promise.all([
    // The backend requires keywords; an empty search shows the prompt.
    q ? usersApi.search({ keywords: q, limit: MAX_RESULTS + 1 }) : null,
    checkPerms({ canCreate: PERMS.USER_WRITE }),
    loadNamespaces("users"),
  ])
  return {
    users: found?.slice(0, MAX_RESULTS) ?? null,
    hasMore: (found?.length ?? 0) > MAX_RESULTS,
    ...perms,
  }
}

export function Component() {
  const { users, hasMore, canCreate } = useLoaderData<typeof loader>()
  const { t } = useTranslation(["users", "common"])
  const navigate = useNavigate()
  const [params, setParams] = useSearchParams()
  const [creating, setCreating] = useState(false)
  const busy = useBusy()
  const q = params.get("q") ?? ""

  const columns: Column<UserSummary>[] = [
    {
      id: "name",
      header: t("columns.name"),
      cell: (u) => (
        <Link to={String(u.id)} className="font-medium hover:underline focus-visible:underline">
          {u.display_name}
        </Link>
      ),
    },
    {
      id: "username",
      header: t("columns.username"),
      cell: (u) => <span className="font-mono text-xs">{u.username}</span>,
    },
    { id: "email", header: t("columns.email"), cell: (u) => u.email },
    {
      id: "status",
      header: t("columns.status"),
      cell: (u) =>
        u.status === "active" ? (
          <StatusBadge tone="success">{t("status.active")}</StatusBadge>
        ) : (
          u.status && <StatusBadge tone="neutral">{u.status}</StatusBadge>
        ),
    },
  ]

  return (
    <div className="flex flex-col gap-4">
      <PageHeader
        title={t("title")}
        description={t("subtitle")}
        actions={
          canCreate && (
            <Button onClick={() => setCreating(true)}>
              <PlusIcon data-icon="inline-start" />
              {t("new")}
            </Button>
          )
        }
      />
      <SearchInput
        className="max-w-sm"
        value={q}
        label={t("search")}
        placeholder={t("search")}
        onCommit={(value) =>
          setParams(
            (prev) => {
              if (value.trim()) prev.set("q", value)
              else prev.delete("q")
              return prev
            },
            { replace: true }
          )
        }
      />
      {users === null ? (
        <Empty className="border">
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <UsersIcon />
            </EmptyMedia>
            <EmptyDescription>{t("prompt")}</EmptyDescription>
          </EmptyHeader>
        </Empty>
      ) : (
        <>
          <DataTable
            label={t("title")}
            rows={users}
            columns={columns}
            rowKey={(u) => u.id}
            rowHref={(u) => String(u.id)}
            empty={t("noMatches")}
            busy={busy}
          />
          {hasMore && (
            <p className="text-sm text-muted-foreground">{t("moreMatches", { count: MAX_RESULTS })}</p>
          )}
        </>
      )}

      <CreateUserDialog
        open={creating}
        onOpenChange={setCreating}
        // Roles are assigned on the new user's page.
        onCreated={(user) => navigate(String(user.id))}
      />
    </div>
  )
}
