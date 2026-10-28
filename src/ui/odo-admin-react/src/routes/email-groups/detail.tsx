import { PauseIcon, PencilIcon, PlayIcon } from "lucide-react"
import { useState } from "react"
import { useTranslation } from "react-i18next"
import { useLoaderData, useRevalidator, type LoaderFunctionArgs } from "react-router"
import { toast } from "sonner"

import { emailGroupsApi } from "@/api/notify"
import { PERMS } from "@/auth/perms"
import { checkPerms } from "@/auth/session"
import { ConfirmDialog } from "@/components/confirm-dialog"
import { DetailHeader } from "@/components/detail-header"
import { StatusBadge } from "@/components/status-badge"
import { Button } from "@/components/ui/button"
import { useReportError } from "@/hooks/use-report-error"
import { loadNamespaces } from "@/i18n"
import { orNotFound, requireIdParam } from "@/lib/loader"
import type { RouteHandle } from "@/lib/route-handle"
import { GroupDialog } from "./group-dialog"
import { Members } from "./members"

export async function loader({ params }: LoaderFunctionArgs) {
  const id = requireIdParam(params.groupId)
  const [detail, perms] = await Promise.all([
    orNotFound(emailGroupsApi.get(id)),
    checkPerms({ canWrite: PERMS.EMAIL_GROUP_WRITE }),
    loadNamespaces("emailGroups"),
  ])
  return { ...detail, ...perms }
}

export const handle: RouteHandle = {
  crumb: (match) => (match.loaderData as Awaited<ReturnType<typeof loader>> | undefined)?.group.label ?? "",
}

/** One email group and its member addresses. */
export function Component() {
  const { group, members, canWrite } = useLoaderData<typeof loader>()
  const { t } = useTranslation(["emailGroups", "common"])
  const revalidator = useRevalidator()
  const reportError = useReportError()
  const [editing, setEditing] = useState(false)
  const [confirmingDeactivate, setConfirmingDeactivate] = useState(false)

  const revalidate = () => void revalidator.revalidate()

  const setActive = async (active: boolean) => {
    try {
      await emailGroupsApi.update({ id: group.id, is_active: active })
      toast.success(t(active ? "detail.reactivated" : "deactivate.done", { label: group.label }))
      revalidate()
    } catch (err) {
      reportError(err, "Failed to update group status")
    }
  }

  return (
    <div className="flex flex-col gap-8">
      <DetailHeader
        backTo=".."
        backLabel={t("detail.back")}
        title={group.label}
        summary={[
          <span className="font-mono text-xs">{group.code}</span>,
          group.is_active ? (
            <StatusBadge tone="success">{t("active")}</StatusBadge>
          ) : (
            <StatusBadge tone="neutral">{t("inactive")}</StatusBadge>
          ),
          t("memberCounts", { active: group.active_member_count, total: group.member_count }),
        ]}
        actions={
          canWrite && (
            <>
              <Button variant="outline" onClick={() => setEditing(true)}>
                <PencilIcon data-icon="inline-start" />
                {t("detail.edit")}
              </Button>
              {group.is_active ? (
                <Button variant="destructive" onClick={() => setConfirmingDeactivate(true)}>
                  <PauseIcon data-icon="inline-start" />
                  {t("detail.deactivate")}
                </Button>
              ) : (
                <Button variant="outline" onClick={() => void setActive(true)}>
                  <PlayIcon data-icon="inline-start" />
                  {t("detail.reactivate")}
                </Button>
              )}
            </>
          )
        }
      />

      <Members group={group} members={members} canWrite={canWrite} onChanged={revalidate} />

      <GroupDialog group={editing ? group : undefined} onOpenChange={setEditing} onSaved={revalidate} />
      <ConfirmDialog
        open={confirmingDeactivate}
        onOpenChange={setConfirmingDeactivate}
        title={t("deactivate.title")}
        description={t("deactivate.body", { label: group.label })}
        confirmLabel={t("detail.deactivate")}
        destructive
        onConfirm={() => setActive(false)}
      />
    </div>
  )
}
