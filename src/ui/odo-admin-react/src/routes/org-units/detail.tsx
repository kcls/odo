import { PencilIcon, PlusIcon, Trash2Icon } from "lucide-react"
import { useState } from "react"
import { useTranslation } from "react-i18next"
import { Link, data, useLoaderData, useNavigate, useRevalidator, type LoaderFunctionArgs } from "react-router"
import { toast } from "sonner"

import { orgAdminApi, type Flattened, type OrgUnitNode } from "@/api/org"
import { PERMS } from "@/auth/perms"
import { checkPerms } from "@/auth/session"
import { ConfirmDialog } from "@/components/confirm-dialog"
import { DataTable, type Column } from "@/components/data-table/data-table"
import { DetailHeader } from "@/components/detail-header"
import { DisabledReason } from "@/components/disabled-reason"
import { Section } from "@/components/section"
import { Button } from "@/components/ui/button"
import { useReportError } from "@/hooks/use-report-error"
import { loadNamespaces } from "@/i18n"
import { orNotFound, requireIdParam } from "@/lib/loader"
import type { RouteHandle } from "@/lib/route-handle"
import { Addresses } from "./addresses"
import { Closures } from "./closures"
import { OperatingHours } from "./operating-hours"
import { UnitDialog, type UnitEditing } from "./unit-dialog"

export async function loader({ params }: LoaderFunctionArgs) {
  const id = requireIdParam(params.unitId)
  const [units, unitTypes, facets, perms] = await Promise.all([
    orgAdminApi.fetchTree(),
    orgAdminApi.listAllUnitTypes(),
    orNotFound(orgAdminApi.unitChildren(id)),
    checkPerms({ canWrite: PERMS.ORG_UNIT_WRITE }),
    loadNamespaces("orgUnits"),
  ])
  // The tree holds active units only, so a deleted unit is a 404 too.
  const unit = units.find((u) => u.id === id)
  if (!unit) throw data("Org unit not found", { status: 404 })
  return { unit, units, unitTypes, ...facets, ...perms }
}

export const handle: RouteHandle = {
  crumb: (match) => (match.loaderData as Awaited<ReturnType<typeof loader>> | undefined)?.unit.label ?? "",
}

type Row = Flattened<OrgUnitNode>

/**
 * Everything about one org unit: its child units (which drill deeper
 * into this same page), addresses, closures, and operating hours.
 */
export function Component() {
  const { unit, units, unitTypes, addresses, closures, operating_hours, canWrite } = useLoaderData<typeof loader>()
  const { t } = useTranslation(["orgUnits", "common"])
  const navigate = useNavigate()
  const revalidator = useRevalidator()
  const reportError = useReportError()
  const [editing, setEditing] = useState<UnitEditing | null>(null)
  const [deleting, setDeleting] = useState(false)

  const revalidate = () => void revalidator.revalidate()
  const parent = units.find((u) => u.id === unit.parent)
  const children = units.filter((u) => u.parent === unit.id)

  const deleteUnit = async () => {
    try {
      await orgAdminApi.deleteUnit(unit.id)
      toast.success(t("delete.done", { label: unit.label }))
      navigate(parent ? `../${parent.id}` : "..", { replace: true })
    } catch (err) {
      reportError(err, "Failed to delete org unit")
    }
  }

  const deleteBlockedBy =
    unit.parent == null
      ? t("detail.deleteRoot")
      : children.length > 0
        ? t("detail.deleteHasChildren", { count: children.length })
        : null

  const childColumns: Column<Row>[] = [
    {
      id: "unit",
      header: t("columns.unit"),
      cell: (u) => (
        <Link to={`../${u.id}`} className="font-medium hover:underline focus-visible:underline">
          {u.label}
        </Link>
      ),
    },
    { id: "code", header: t("columns.code"), cell: (u) => <span className="font-mono text-xs">{u.code}</span> },
    { id: "type", header: t("columns.type"), cell: (u) => u.unit_type.label },
  ]

  return (
    <div className="flex flex-col gap-8">
      <DetailHeader
        backTo=".."
        backLabel={t("detail.back")}
        title={unit.label}
        summary={[
          <span className="font-mono text-xs">{unit.code}</span>,
          unit.unit_type.label,
          unit.timezone && <span className="font-mono text-xs">{unit.timezone}</span>,
          parent && (
            <Link to={`../${parent.id}`} className="hover:underline">
              {t("detail.parent", { label: parent.label })}
            </Link>
          ),
        ].filter(Boolean)}
        actions={
          canWrite && (
            <>
              <Button variant="outline" onClick={() => setEditing({ mode: "edit", unit })}>
                <PencilIcon data-icon="inline-start" />
                {t("detail.edit")}
              </Button>
              {deleteBlockedBy ? (
                <DisabledReason label={t("detail.delete")} reason={deleteBlockedBy}>
                  <Button variant="destructive" disabled aria-hidden="true" tabIndex={-1}>
                    <Trash2Icon data-icon="inline-start" />
                    {t("detail.delete")}
                  </Button>
                </DisabledReason>
              ) : (
                <Button variant="destructive" onClick={() => setDeleting(true)}>
                  <Trash2Icon data-icon="inline-start" />
                  {t("detail.delete")}
                </Button>
              )}
            </>
          )
        }
      />

      {(children.length > 0 || canWrite) && (
        <Section
          title={t("children.titleCount", { count: children.length })}
          actions={
            canWrite && (
              <Button variant="outline" onClick={() => setEditing({ mode: "create", parent: unit.id })}>
                <PlusIcon data-icon="inline-start" />
                {t("detail.addChild")}
              </Button>
            )
          }
        >
          {children.length > 0 && (
            <DataTable
              label={t("children.title")}
              rows={children}
              columns={childColumns}
              rowKey={(u) => u.id}
              rowHref={(u) => `../${u.id}`}
              empty={null}
            />
          )}
        </Section>
      )}

      <Addresses orgUnit={unit.id} addresses={addresses} canWrite={canWrite} onChanged={revalidate} />
      <Closures orgUnit={unit.id} closures={closures} canWrite={canWrite} onChanged={revalidate} />
      <OperatingHours orgUnit={unit.id} hours={operating_hours} canWrite={canWrite} onChanged={revalidate} />

      <UnitDialog
        editing={editing}
        units={units}
        unitTypes={unitTypes}
        onOpenChange={(open) => !open && setEditing(null)}
        onSaved={(saved) => {
          if (editing?.mode === "create") navigate(`../${saved.id}`)
          else revalidate()
        }}
      />
      <ConfirmDialog
        open={deleting}
        onOpenChange={setDeleting}
        title={t("delete.title")}
        description={t("delete.body", { label: unit.label })}
        confirmLabel={t("common:actions.delete")}
        destructive
        onConfirm={deleteUnit}
      />
    </div>
  )
}
