import { PencilIcon, Trash2Icon } from "lucide-react"
import { useState } from "react"
import { useTranslation } from "react-i18next"
import { useLoaderData, useNavigate, useRevalidator, type LoaderFunctionArgs } from "react-router"
import { toast } from "sonner"

import { authzApi } from "@/api/authz"
import { PERMS } from "@/auth/perms"
import { checkPerms } from "@/auth/session"
import { ConfirmDialog } from "@/components/confirm-dialog"
import { DetailHeader } from "@/components/detail-header"
import { DisabledReason } from "@/components/disabled-reason"
import { Button } from "@/components/ui/button"
import { useReportError } from "@/hooks/use-report-error"
import { loadNamespaces } from "@/i18n"
import { orNotFound, requireParam } from "@/lib/loader"
import type { RouteHandle } from "@/lib/route-handle"
import { EditRoleDialog } from "./edit-role-dialog"
import { Grants } from "./grants"

export async function loader({ params }: LoaderFunctionArgs) {
  const code = requireParam(params.code)
  const [detail, perms] = await Promise.all([
    orNotFound(authzApi.getRole(code)),
    checkPerms({ canWrite: PERMS.ROLE_WRITE }),
    loadNamespaces("roles"),
  ])
  return { ...detail, ...perms }
}

export const handle: RouteHandle = {
  crumb: (match) => (match.loaderData as Awaited<ReturnType<typeof loader>> | undefined)?.role.label ?? "",
}

/** One role: its details and the permissions it grants. */
export function Component() {
  const { role, grants, canWrite } = useLoaderData<typeof loader>()
  const { t } = useTranslation(["roles", "common"])
  const navigate = useNavigate()
  const revalidator = useRevalidator()
  const reportError = useReportError()
  const [editing, setEditing] = useState(false)
  const [deleting, setDeleting] = useState(false)

  const deleteRole = async () => {
    try {
      await authzApi.deleteRole(role.code)
      toast.success(t("delete.done", { label: role.label }))
      navigate("..", { replace: true })
    } catch (err) {
      reportError(err, "Failed to delete role")
    }
  }

  const deleteButton =
    role.user_count > 0 ? (
      <DisabledReason label={t("detail.delete")} reason={t("detail.deleteBlocked", { count: role.user_count })}>
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
    )

  return (
    <div className="flex flex-col gap-8">
      <DetailHeader
        backTo=".."
        backLabel={t("detail.back")}
        title={role.label}
        summary={[
          <span className="font-mono text-xs">{role.code}</span>,
          t("detail.userCount", { count: role.user_count }),
          role.description || <span className="italic">{t("detail.noDescription")}</span>,
        ]}
        actions={
          canWrite && (
            <>
              <Button variant="outline" onClick={() => setEditing(true)}>
                <PencilIcon data-icon="inline-start" />
                {t("detail.edit")}
              </Button>
              {deleteButton}
            </>
          )
        }
      />

      <Grants role={role} grants={grants} canWrite={canWrite} onChanged={() => void revalidator.revalidate()} />

      <EditRoleDialog
        role={editing ? role : null}
        onOpenChange={setEditing}
        onSaved={() => void revalidator.revalidate()}
      />
      <ConfirmDialog
        open={deleting}
        onOpenChange={setDeleting}
        title={t("delete.title")}
        description={t("delete.body", { label: role.label })}
        confirmLabel={t("common:actions.delete")}
        destructive
        onConfirm={deleteRole}
      />
    </div>
  )
}
