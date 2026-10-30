import { PlusIcon } from "lucide-react"
import { useState } from "react"
import { useTranslation } from "react-i18next"
import { Link, useLoaderData, useNavigate, useSearchParams } from "react-router"

import { filterTree, orgAdminApi, type Flattened, type OrgUnitNode } from "@/api/org"
import { PERMS } from "@/auth/perms"
import { checkPerms } from "@/auth/session"
import { DataTable, type Column } from "@/components/data-table/data-table"
import { PageHeader } from "@/components/page-header"
import { SearchInput } from "@/components/search-input"
import { Button } from "@/components/ui/button"
import { loadNamespaces } from "@/i18n"
import { skipSearchOnlyChanges } from "@/lib/loader"
import { UnitDialog } from "./unit-dialog"

export async function loader() {
  const [units, unitTypes, perms] = await Promise.all([
    orgAdminApi.fetchTree(),
    orgAdminApi.listAllUnitTypes(),
    checkPerms({ canWrite: PERMS.ORG_UNIT_WRITE }),
    loadNamespaces("orgUnits"),
  ])
  return { units, unitTypes, ...perms }
}

// The filter runs over the loaded tree; typing shouldn't refetch it.
export const shouldRevalidate = skipSearchOnlyChanges

type Row = Flattened<OrgUnitNode>

/** The whole org tree, indented; small enough to load and filter whole. */
export function Component() {
  const { units, unitTypes, canWrite } = useLoaderData<typeof loader>()
  const { t } = useTranslation(["orgUnits", "common"])
  const navigate = useNavigate()
  const [params, setParams] = useSearchParams()
  const [creating, setCreating] = useState(false)
  const q = params.get("q") ?? ""
  const visible = filterTree(units, q)

  const columns: Column<Row>[] = [
    {
      id: "unit",
      header: t("columns.unit"),
      cell: (u) => (
        <span style={{ paddingInlineStart: `${u.depth * 1.25}rem` }}>
          <Link to={String(u.id)} className="font-medium hover:underline focus-visible:underline">
            {u.label}
          </Link>
        </span>
      ),
    },
    { id: "code", header: t("columns.code"), cell: (u) => <span className="font-mono text-xs">{u.code}</span> },
    { id: "type", header: t("columns.type"), cell: (u) => u.unit_type.label },
    {
      id: "timezone",
      header: t("columns.timezone"),
      cell: (u) => u.timezone && <span className="font-mono text-xs">{u.timezone}</span>,
    },
  ]

  return (
    <div className="flex flex-col gap-4">
      <PageHeader
        title={t("title")}
        description={t("subtitle")}
        actions={
          canWrite && (
            <Button onClick={() => setCreating(true)}>
              <PlusIcon data-icon="inline-start" />
              {t("new")}
            </Button>
          )
        }
      />
      <SearchInput
        value={q}
        label={t("search")}
        placeholder={t("search")}
        delay={150}
        // A client-side filter over the loaded tree: no minimum.
        minLength={1}
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
      <DataTable
        label={t("title")}
        rows={visible}
        columns={columns}
        rowKey={(u) => u.id}
        rowHref={(u) => String(u.id)}
        empty={t("noMatches")}
      />

      <UnitDialog
        editing={creating ? { mode: "create" } : null}
        units={units}
        unitTypes={unitTypes}
        onOpenChange={setCreating}
        onSaved={(unit) => navigate(String(unit.id))}
      />
    </div>
  )
}
