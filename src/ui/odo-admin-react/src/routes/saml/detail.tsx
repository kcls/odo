import { PencilIcon, Trash2Icon } from "lucide-react"
import { useState } from "react"
import { useTranslation } from "react-i18next"
import { data, useLoaderData, useNavigate, useRevalidator, type LoaderFunctionArgs } from "react-router"
import { toast } from "sonner"

import { samlApi, type IdpRow } from "@/api/saml"
import { PERMS } from "@/auth/perms"
import { checkPerms } from "@/auth/session"
import { ConfirmDialog } from "@/components/confirm-dialog"
import { DetailHeader } from "@/components/detail-header"
import { DisabledReason } from "@/components/disabled-reason"
import { Section } from "@/components/section"
import { StatusBadge } from "@/components/status-badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent } from "@/components/ui/card"
import { useReportError } from "@/hooks/use-report-error"
import { loadNamespaces } from "@/i18n"
import { listRequest, readListState, redirectPastLastPage } from "@/lib/list-state"
import { requireIdParam } from "@/lib/loader"
import type { RouteHandle } from "@/lib/route-handle"
import { Attributes } from "./attributes"
import { IdpDialog } from "./idp-dialog"
import { MAPPING_LIST, Mappings } from "./mappings"
import { ServiceProviders } from "./service-providers"

export async function loader({ request, params }: LoaderFunctionArgs) {
  const id = requireIdParam(params.idpId)
  const url = new URL(request.url)
  const mappingState = readListState(url.searchParams, MAPPING_LIST)
  // There's no get-one endpoint; IdPs, SPs, and attributes are small
  // lists, so read them whole and pick this IdP's.
  const [idps, sps, attributes, mappings, perms] = await Promise.all([
    samlApi.listIdps(),
    samlApi.listSps(),
    samlApi.listAttributes(),
    samlApi.listMappingsPage({ ...listRequest(mappingState), idp: id }),
    checkPerms({ canWrite: PERMS.SAML_WRITE }),
    loadNamespaces("saml"),
  ])
  const idp = idps.find((i) => i.id === id)
  if (!idp) throw data("Identity provider not found", { status: 404 })
  redirectPastLastPage(url, mappingState, mappings.total, MAPPING_LIST)
  return {
    idp,
    idps,
    sps: sps.filter((sp) => sp.idp === id),
    attributes: attributes.filter((a) => a.idp === id),
    mappings,
    ...perms,
  }
}

export const handle: RouteHandle = {
  crumb: (match) => (match.loaderData as Awaited<ReturnType<typeof loader>> | undefined)?.idp.name ?? "",
}

/** One identity provider: its settings, SPs, attributes, and role mappings. */
export function Component() {
  const { idp, idps, sps, attributes, mappings, canWrite } = useLoaderData<typeof loader>()
  const { t } = useTranslation(["saml", "common"])
  const navigate = useNavigate()
  const revalidator = useRevalidator()
  const reportError = useReportError()
  const [editing, setEditing] = useState(false)
  const [deleting, setDeleting] = useState(false)

  const revalidate = () => void revalidator.revalidate()

  const deleteIdp = async () => {
    try {
      await samlApi.deleteIdp(idp.id)
      toast.success(t("deleteIdp.done", { name: idp.name }))
      navigate("..", { replace: true })
    } catch (err) {
      reportError(err, "Failed to delete identity provider")
    }
  }

  const inUse = idp.sp_count > 0 || idp.user_count > 0

  return (
    <div className="flex flex-col gap-8">
      <DetailHeader
        backTo=".."
        backLabel={t("detail.back")}
        title={idp.name}
        summary={[
          <span className="font-mono text-xs break-all">{idp.entity_id}</span>,
          idp.is_active ? (
            <StatusBadge tone="success">{t("active")}</StatusBadge>
          ) : (
            <StatusBadge tone="neutral">{t("inactive")}</StatusBadge>
          ),
        ]}
        actions={
          canWrite && (
            <>
              <Button variant="outline" onClick={() => setEditing(true)}>
                <PencilIcon data-icon="inline-start" />
                {t("detail.edit")}
              </Button>
              {inUse ? (
                <DisabledReason
                  label={t("detail.delete")}
                  reason={t("detail.deleteBlocked", {
                    sps: t("spCount", { count: idp.sp_count }),
                    users: t("userCount", { count: idp.user_count }),
                  })}
                >
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

      <Configuration idp={idp} />

      <ServiceProviders
        title={t("sps.title")}
        description={t("sps.description")}
        empty={t("sps.none")}
        sps={sps}
        idps={idps}
        defaultIdp={idp.id}
        canWrite={canWrite}
        onChanged={revalidate}
      />
      <Attributes idpId={idp.id} attributes={attributes} canWrite={canWrite} onChanged={revalidate} />
      <Mappings page={mappings} attributes={attributes} canWrite={canWrite} onChanged={revalidate} />

      <IdpDialog idp={editing ? idp : undefined} onOpenChange={setEditing} onSaved={revalidate} />
      <ConfirmDialog
        open={deleting}
        onOpenChange={setDeleting}
        title={t("deleteIdp.title")}
        description={t("deleteIdp.body", { name: idp.name })}
        confirmLabel={t("common:actions.delete")}
        destructive
        onConfirm={deleteIdp}
      />
    </div>
  )
}

function Configuration({ idp }: { idp: IdpRow }) {
  const { t } = useTranslation(["saml", "common"])
  const none = <span className="text-muted-foreground">{t("common:none")}</span>
  const url = (value: string | null | undefined) =>
    value ? <span className="font-mono text-xs break-all">{value}</span> : none
  const facts: [string, React.ReactNode][] = [
    [t("detail.facts.ssoUrl"), url(idp.sso_url)],
    [t("detail.facts.sloUrl"), url(idp.slo_url)],
    [t("detail.facts.metadataUrl"), url(idp.metadata_url)],
    [
      t("detail.facts.sessionLifetime"),
      idp.session_lifetime_hours != null ? t("detail.hours", { count: idp.session_lifetime_hours }) : none,
    ],
    [t("detail.facts.idpInitiated"), idp.allow_idp_initiated ? t("detail.allowed") : t("detail.notAllowed")],
    [t("detail.facts.users"), idp.user_count],
  ]
  return (
    <Section title={t("detail.configuration")}>
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
    </Section>
  )
}
