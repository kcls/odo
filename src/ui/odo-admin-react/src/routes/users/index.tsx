import { UsersIcon } from "lucide-react"
import { useTranslation } from "react-i18next"
import { Link, useLoaderData, useSearchParams, type LoaderFunctionArgs } from "react-router"

import { usersApi, type UserSummary } from "@/api/users"
import { DataTable, type Column } from "@/components/data-table/data-table"
import { PageHeader } from "@/components/page-header"
import { SearchInput } from "@/components/search-input"
import { StatusBadge } from "@/components/status-badge"
import { Empty, EmptyDescription, EmptyHeader, EmptyMedia } from "@/components/ui/empty"
import { useBusy } from "@/hooks/use-busy"
import { loadNamespaces } from "@/i18n"

/** odo-auth's search has no total, so ask for one extra to detect more. */
const MAX_RESULTS = 50

export async function loader({ request }: LoaderFunctionArgs) {
  const q = new URL(request.url).searchParams.get("q")?.trim() ?? ""
  const [found] = await Promise.all([
    // The backend requires keywords; an empty search shows the prompt.
    q ? usersApi.search({ keywords: q, limit: MAX_RESULTS + 1 }) : null,
    loadNamespaces("users"),
  ])
  return {
    users: found?.slice(0, MAX_RESULTS) ?? null,
    hasMore: (found?.length ?? 0) > MAX_RESULTS,
  }
}

export function Component() {
  const { users, hasMore } = useLoaderData<typeof loader>()
  const { t } = useTranslation(["users", "common"])
  const [params, setParams] = useSearchParams()
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
      <PageHeader title={t("title")} description={t("subtitle")} />
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
    </div>
  )
}
