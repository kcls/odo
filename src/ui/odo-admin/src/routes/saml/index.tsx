import { PlusIcon } from "lucide-react"
import { useState } from "react"
import { useTranslation } from "react-i18next"
import { Link, useLoaderData, useNavigate, useRevalidator } from "react-router"

import { samlApi, type IdpRow } from "@/api/saml"
import { PERMS } from "@/auth/perms"
import { checkPerms } from "@/auth/session"
import { DataTable, type Column } from "@/components/data-table/data-table"
import { PageHeader } from "@/components/page-header"
import { StatusBadge } from "@/components/status-badge"
import { Button } from "@/components/ui/button"
import { loadNamespaces } from "@/i18n"
import { IdpDialog } from "./idp-dialog"
import { ServiceProviders } from "./service-providers"

export async function loader() {
  const [idps, sps, perms] = await Promise.all([
    samlApi.listIdps(),
    samlApi.listSps(),
    checkPerms({ canWrite: PERMS.SAML_WRITE }),
    loadNamespaces("saml"),
  ])
  // SPs not linked to any IdP appear nowhere else, so list them here.
  return { idps, unlinkedSps: sps.filter((sp) => sp.idp == null), ...perms }
}

export function Component() {
  const { idps, unlinkedSps, canWrite } = useLoaderData<typeof loader>()
  const { t } = useTranslation("saml")
  const navigate = useNavigate()
  const revalidator = useRevalidator()
  const [creating, setCreating] = useState(false)

  const columns: Column<IdpRow>[] = [
    {
      id: "name",
      header: t("columns.name"),
      cell: (i) => (
        <Link to={String(i.id)} className="font-medium hover:underline focus-visible:underline">
          {i.name}
        </Link>
      ),
    },
    {
      id: "entityId",
      header: t("columns.entityId"),
      cell: (i) => <span className="font-mono text-xs break-all">{i.entity_id}</span>,
    },
    {
      id: "status",
      header: t("columns.status"),
      cell: (i) =>
        i.is_active ? (
          <StatusBadge tone="success">{t("active")}</StatusBadge>
        ) : (
          <StatusBadge tone="neutral">{t("inactive")}</StatusBadge>
        ),
    },
    {
      id: "usage",
      header: t("columns.usage"),
      className: "whitespace-nowrap",
      cell: (i) =>
        t("usage", {
          sps: t("spCount", { count: i.sp_count }),
          users: t("userCount", { count: i.user_count }),
        }),
    },
  ]

  return (
    <div className="flex flex-col gap-8">
      <div className="flex flex-col gap-4">
        <PageHeader
          title={t("title")}
          description={t("subtitle")}
          actions={
            canWrite && (
              <Button onClick={() => setCreating(true)}>
                <PlusIcon data-icon="inline-start" />
                {t("newIdp")}
              </Button>
            )
          }
        />
        <DataTable
          label={t("idps")}
          rows={idps}
          columns={columns}
          rowKey={(i) => i.id}
          rowHref={(i) => String(i.id)}
          empty={t("noIdps")}
        />
      </div>

      {unlinkedSps.length > 0 && (
        <ServiceProviders
          title={t("unlinked.title")}
          description={t("unlinked.description")}
          empty=""
          sps={unlinkedSps}
          idps={idps}
          defaultIdp={null}
          canWrite={canWrite}
          onChanged={() => void revalidator.revalidate()}
        />
      )}

      <IdpDialog
        idp={creating ? null : undefined}
        onOpenChange={setCreating}
        onSaved={(idp) => navigate(String(idp.id))}
      />
    </div>
  )
}
