import { PlusIcon, Trash2Icon } from "lucide-react"
import { useEffect, useState, type FormEvent } from "react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"

import { authzApi, type GrantRow, type PermissionRow, type RoleRow } from "@/api/authz"
import { ApiRequestError } from "@/api/client"
import { describeError } from "@/api/errors"
import { Combobox } from "@/components/combobox"
import { ConfirmDialog } from "@/components/confirm-dialog"
import { DataTable, type Column } from "@/components/data-table/data-table"
import { Section } from "@/components/section"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Field, FieldDescription, FieldError, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { useReportError } from "@/hooks/use-report-error"

/** A role's permission grants, managed in place. */
export function Grants({
  role,
  grants,
  canWrite,
  onChanged,
}: {
  role: RoleRow
  grants: GrantRow[]
  canWrite: boolean
  onChanged: () => void
}) {
  const { t } = useTranslation(["roles", "common"])
  const reportError = useReportError()
  const [granting, setGranting] = useState(false)
  const [revoking, setRevoking] = useState<GrantRow | null>(null)

  const revoke = async (grant: GrantRow) => {
    try {
      await authzApi.deleteGrant(grant.id)
      toast.success(t("grants.revoked", { perm: grant.perm }))
      onChanged()
    } catch (err) {
      reportError(err, "Failed to revoke grant")
    }
  }

  const columns: Column<GrantRow>[] = [
    {
      id: "permission",
      header: t("grants.columns.permission"),
      cell: (g) => (
        <div className="flex flex-col">
          <span className="font-mono text-xs">{g.perm}</span>
          {g.perm_description && <span className="text-muted-foreground">{g.perm_description}</span>}
        </div>
      ),
    },
    {
      id: "minDepth",
      header: t("grants.columns.minDepth"),
      className: "w-32",
      cell: (g) =>
        canWrite ? (
          // Keyed by the saved value so a revalidation resets the draft.
          <MinDepthInput key={`${g.id}:${g.min_depth}`} grant={g} onChanged={onChanged} />
        ) : (
          <span className="tabular-nums">{g.min_depth}</span>
        ),
    },
  ]
  if (canWrite) {
    columns.push({
      id: "actions",
      header: null,
      className: "w-1 text-end",
      cell: (g) => (
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label={t("grants.revokeLabel", { perm: g.perm })}
          onClick={() => setRevoking(g)}
        >
          <Trash2Icon />
        </Button>
      ),
    })
  }

  return (
    <>
      <Section
        title={t("grants.titleCount", { count: grants.length })}
        description={t("grants.description")}
        actions={
          canWrite && (
            <Button variant="outline" onClick={() => setGranting(true)}>
              <PlusIcon data-icon="inline-start" />
              {t("grants.grant")}
            </Button>
          )
        }
      >
        <DataTable
          label={t("grants.title")}
          rows={grants}
          columns={columns}
          rowKey={(g) => g.id}
          empty={t("grants.none")}
        />
      </Section>

      <Dialog open={granting} onOpenChange={setGranting}>
        <DialogContent className="sm:max-w-lg">
          {granting && (
            <GrantForm
              role={role}
              granted={grants}
              onDone={() => {
                setGranting(false)
                onChanged()
              }}
            />
          )}
        </DialogContent>
      </Dialog>
      <ConfirmDialog
        open={revoking !== null}
        onOpenChange={(open) => !open && setRevoking(null)}
        title={t("grants.revokeTitle")}
        description={t("grants.revokeBody", { perm: revoking?.perm ?? "", label: role.label })}
        confirmLabel={t("grants.revoke")}
        destructive
        onConfirm={async () => {
          if (revoking) await revoke(revoking)
        }}
      />
    </>
  )
}

/** Commits on blur or Enter when the value changed; Escape reverts. */
function MinDepthInput({ grant, onChanged }: { grant: GrantRow; onChanged: () => void }) {
  const { t } = useTranslation("roles")
  const reportError = useReportError()
  const [draft, setDraft] = useState(String(grant.min_depth))
  const [pending, setPending] = useState(false)

  const commit = async () => {
    const depth = Number(draft)
    if (draft.trim() === "" || !Number.isInteger(depth) || depth < 0) {
      setDraft(String(grant.min_depth))
      return
    }
    if (depth === grant.min_depth) return
    setPending(true)
    try {
      await authzApi.updateGrant({ id: grant.id, min_depth: depth })
      toast.success(t("grants.minDepthSaved", { perm: grant.perm, depth }))
      onChanged()
    } catch (err) {
      setDraft(String(grant.min_depth))
      reportError(err, "Failed to update grant")
    } finally {
      setPending(false)
    }
  }

  return (
    <Input
      type="number"
      inputMode="numeric"
      min={0}
      step={1}
      className="h-8 w-20 tabular-nums"
      aria-label={t("grants.minDepthLabel", { perm: grant.perm })}
      value={draft}
      disabled={pending}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={() => void commit()}
      onKeyDown={(e) => {
        if (e.key === "Enter") e.currentTarget.blur()
        if (e.key === "Escape") setDraft(String(grant.min_depth))
      }}
    />
  )
}

function GrantForm({
  role,
  granted,
  onDone,
}: {
  role: RoleRow
  granted: GrantRow[]
  onDone: () => void
}) {
  const { t } = useTranslation(["roles", "common"])
  const [permissions, setPermissions] = useState<PermissionRow[] | "failed" | null>(null)
  const [perm, setPerm] = useState("")
  const [minDepth, setMinDepth] = useState("0")
  const [error, setError] = useState<string | null>(null)
  const [pending, setPending] = useState(false)

  useEffect(() => {
    let cancelled = false
    authzApi.listAllPermissions().then(
      (rows) => !cancelled && setPermissions(rows),
      (err) => {
        console.error("Failed to load permissions", err)
        if (!cancelled) setPermissions("failed")
      }
    )
    return () => {
      cancelled = true
    }
  }, [])

  const grantedCodes = new Set(granted.map((g) => g.perm))
  const available = Array.isArray(permissions) ? permissions.filter((p) => !grantedCodes.has(p.code)) : []
  const depth = Number(minDepth)
  const depthValid = minDepth.trim() !== "" && Number.isInteger(depth) && depth >= 0

  const submit = async (e: FormEvent) => {
    e.preventDefault()
    if (!perm || !depthValid) return
    setPending(true)
    setError(null)
    try {
      await authzApi.createGrant({ role: role.code, perm, min_depth: depth })
      toast.success(t("grants.granted", { perm }))
      onDone()
    } catch (err) {
      setError(
        err instanceof ApiRequestError && err.code === "PERMISSION_ALREADY_GRANTED"
          ? t("common:errors.codes.PERMISSION_ALREADY_GRANTED")
          : describeError(err)
      )
      setPending(false)
    }
  }

  return (
    <form onSubmit={submit} className="contents">
      <DialogHeader>
        <DialogTitle>{t("grants.grant")}</DialogTitle>
      </DialogHeader>
      <FieldGroup>
        <Field>
          <FieldLabel htmlFor="grant-permission">{t("grants.permission")}</FieldLabel>
          <Combobox
            id="grant-permission"
            // Codes are what admins know permissions by; descriptions help find them.
            options={available.map((p) => ({ value: p.code, label: p.code, detail: p.description ?? undefined }))}
            value={perm}
            onValueChange={setPerm}
            disabled={!Array.isArray(permissions) || available.length === 0}
            placeholder={t("grants.permissionPlaceholder")}
            searchPlaceholder={t("grants.permissionSearch")}
          />
          {permissions === "failed" && <FieldError>{t("grants.pickerFailed")}</FieldError>}
          {Array.isArray(permissions) && available.length === 0 && (
            <FieldDescription>{t("grants.allGranted")}</FieldDescription>
          )}
        </Field>
        <Field data-invalid={!depthValid}>
          <FieldLabel htmlFor="grant-min-depth">{t("grants.minDepth")}</FieldLabel>
          <Input
            id="grant-min-depth"
            type="number"
            inputMode="numeric"
            min={0}
            step={1}
            className="w-24"
            value={minDepth}
            aria-invalid={!depthValid}
            onChange={(e) => setMinDepth(e.target.value)}
          />
          <FieldDescription>{t("grants.minDepthHint")}</FieldDescription>
        </Field>
        {error && <FieldError>{error}</FieldError>}
      </FieldGroup>
      <DialogFooter>
        <DialogClose asChild>
          <Button type="button" variant="outline">
            {t("common:actions.cancel")}
          </Button>
        </DialogClose>
        <Button type="submit" disabled={pending || !perm || !depthValid}>
          {t("grants.submit")}
        </Button>
      </DialogFooter>
    </form>
  )
}
