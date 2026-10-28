import { PencilIcon, PlusIcon, Trash2Icon } from "lucide-react"
import { useEffect, useState, type FormEvent } from "react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"

import { authzApi, type RoleRow } from "@/api/authz"
import { ApiRequestError } from "@/api/client"
import { describeError } from "@/api/errors"
import { samlApi, type AttributeRow, type AttrRoleMapPage, type AttrRoleMapRow } from "@/api/saml"
import { Combobox } from "@/components/combobox"
import { ConfirmDialog } from "@/components/confirm-dialog"
import { DataTable, type Column } from "@/components/data-table/data-table"
import { Paginator } from "@/components/data-table/paginator"
import { DisabledReason } from "@/components/disabled-reason"
import { SearchInput } from "@/components/search-input"
import { Section } from "@/components/section"
import { StatusBadge } from "@/components/status-badge"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
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
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { useBusy } from "@/hooks/use-busy"
import { useReportError } from "@/hooks/use-report-error"
import { useListState, type ListDefaults } from "@/lib/list-state"

/** Matches the backend's sort allow-list for attr-role-map/list. */
export const MAPPING_LIST: ListDefaults = {
  sort: { key: "role", dir: "asc" },
  sortKeys: ["role", "value", "active"],
}

type Editing = { mode: "create" } | { mode: "edit"; row: AttrRoleMapRow }

/**
 * An IdP's attribute-to-role mappings. They can grow large, so they're
 * server-filtered and paginated, with the list state in the page URL.
 */
export function Mappings({
  page,
  attributes,
  canWrite,
  onChanged,
}: {
  page: AttrRoleMapPage
  attributes: AttributeRow[]
  canWrite: boolean
  onChanged: () => void
}) {
  const { t } = useTranslation(["saml", "common"])
  const [list, setList] = useListState(MAPPING_LIST)
  const busy = useBusy()
  const reportError = useReportError()
  const [editing, setEditing] = useState<Editing | null>(null)
  const [deleting, setDeleting] = useState<AttrRoleMapRow | null>(null)

  const remove = async (m: AttrRoleMapRow) => {
    try {
      await samlApi.deleteMapping(m.id)
      toast.success(t("mappings.deleted", { value: m.attr_value }))
      onChanged()
    } catch (err) {
      reportError(err, "Failed to delete role mapping")
    }
  }

  const columns: Column<AttrRoleMapRow>[] = [
    { id: "attribute", header: t("mappings.columns.attribute"), cell: (m) => <span className="font-mono text-xs">{m.attr_key}</span> },
    { id: "value", header: t("mappings.columns.value"), sortKey: "value", cell: (m) => m.attr_value },
    {
      id: "role",
      header: t("mappings.columns.role"),
      sortKey: "role",
      cell: (m) => (
        <div className="flex flex-col">
          <span>{m.role_label}</span>
          <span className="font-mono text-xs text-muted-foreground">{m.role}</span>
        </div>
      ),
    },
    {
      id: "status",
      header: t("mappings.columns.status"),
      sortKey: "active",
      cell: (m) =>
        m.is_active ? (
          <StatusBadge tone="success">{t("active")}</StatusBadge>
        ) : (
          <StatusBadge tone="neutral">{t("inactive")}</StatusBadge>
        ),
    },
  ]
  if (canWrite) {
    columns.push({
      id: "actions",
      header: null,
      className: "w-1 whitespace-nowrap text-end",
      cell: (m) => (
        <div className="flex justify-end gap-1">
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label={t("mappings.editLabel", { value: m.attr_value })}
            onClick={() => setEditing({ mode: "edit", row: m })}
          >
            <PencilIcon />
          </Button>
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label={t("mappings.deleteLabel", { value: m.attr_value })}
            onClick={() => setDeleting(m)}
          >
            <Trash2Icon />
          </Button>
        </div>
      ),
    })
  }


  return (
    <Section
      title={t("mappings.title")}
      description={t("mappings.description")}
      actions={
        canWrite &&
        (attributes.length === 0 ? (
          <DisabledReason label={t("mappings.new")} reason={t("mappings.needsAttribute")}>
            <Button variant="outline" disabled aria-hidden="true" tabIndex={-1}>
              <PlusIcon data-icon="inline-start" />
              {t("mappings.new")}
            </Button>
          </DisabledReason>
        ) : (
          <Button variant="outline" onClick={() => setEditing({ mode: "create" })}>
            <PlusIcon data-icon="inline-start" />
            {t("mappings.new")}
          </Button>
        ))
      }
    >
      <SearchInput value={list.q} onCommit={(q) => setList({ q })} label={t("mappings.search")} placeholder={t("mappings.search")} />
      <DataTable
        label={t("mappings.title")}
        rows={page.rows}
        columns={columns}
        rowKey={(m) => m.id}
        sort={list.sort}
        onSortChange={(sort) => setList({ sort })}
        empty={list.q ? t("mappings.noMatches") : t("mappings.none")}
        busy={busy}
      />
      <Paginator total={page.total} pageIndex={list.pageIndex} pageSize={list.pageSize} onChange={setList} />

      <Dialog open={editing !== null} onOpenChange={(open) => !open && setEditing(null)}>
        <DialogContent className="sm:max-w-lg">
          {editing && (
            <MappingForm
              editing={editing}
              attributes={attributes}
              onDone={() => {
                setEditing(null)
                onChanged()
              }}
            />
          )}
        </DialogContent>
      </Dialog>
      <ConfirmDialog
        open={deleting !== null}
        onOpenChange={(open) => !open && setDeleting(null)}
        title={t("mappings.deleteTitle")}
        description={t("mappings.deleteBody", { value: deleting?.attr_value ?? "", role: deleting?.role_label ?? "" })}
        confirmLabel={t("common:actions.delete")}
        destructive
        onConfirm={async () => {
          if (deleting) await remove(deleting)
        }}
      />
    </Section>
  )
}

function MappingForm({
  editing,
  attributes,
  onDone,
}: {
  editing: Editing
  attributes: AttributeRow[]
  onDone: () => void
}) {
  const { t } = useTranslation(["saml", "common"])
  const existing = editing.mode === "edit" ? editing.row : null
  const [attr, setAttr] = useState(existing ? String(existing.attr) : attributes.length === 1 ? String(attributes[0].id) : "")
  const [value, setValue] = useState(existing?.attr_value ?? "")
  const [role, setRole] = useState(existing?.role ?? "")
  const [isActive, setIsActive] = useState(existing?.is_active ?? true)
  const [roles, setRoles] = useState<RoleRow[] | "failed" | null>(null)
  const [valueError, setValueError] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [pending, setPending] = useState(false)

  // Every role, not the first 50: the Angular picker stopped at the
  // server's default page size.
  useEffect(() => {
    let cancelled = false
    authzApi.listAllRoles().then(
      (rows) => !cancelled && setRoles(rows),
      (err) => {
        console.error("Failed to load roles", err)
        if (!cancelled) setRoles("failed")
      }
    )
    return () => {
      cancelled = true
    }
  }, [])

  const submit = async (e: FormEvent) => {
    e.preventDefault()
    setPending(true)
    setValueError(null)
    setError(null)
    const fields = { attr: Number(attr), role, attr_value: value.trim(), is_active: isActive }
    try {
      const saved = existing ? await samlApi.updateMapping(existing.id, fields) : await samlApi.createMapping(fields)
      toast.success(t("mappings.saved", { value: saved.attr_value }))
      onDone()
    } catch (err) {
      if (err instanceof ApiRequestError && err.code === "MAPPING_EXISTS") {
        setValueError(t("common:errors.codes.MAPPING_EXISTS"))
      } else {
        setError(describeError(err))
      }
      setPending(false)
    }
  }

  return (
    <form onSubmit={submit} className="contents">
      <DialogHeader>
        <DialogTitle>{existing ? t("mappings.editTitle") : t("mappings.createTitle")}</DialogTitle>
      </DialogHeader>
      <FieldGroup>
        <Field>
          <FieldLabel htmlFor="mapping-attr">{t("mappings.attribute")}</FieldLabel>
          <Select value={attr} onValueChange={setAttr}>
            <SelectTrigger id="mapping-attr" className="w-full">
              <SelectValue placeholder={t("mappings.attributePlaceholder")} />
            </SelectTrigger>
            <SelectContent>
              {attributes.map((a) => (
                <SelectItem key={a.id} value={String(a.id)}>
                  {a.label}
                  <span className="font-mono text-xs text-muted-foreground">{a.key}</span>
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </Field>
        <Field data-invalid={valueError !== null}>
          <FieldLabel htmlFor="mapping-value">{t("mappings.value")}</FieldLabel>
          <Input
            id="mapping-value"
            required
            value={value}
            placeholder={t("mappings.valuePlaceholder")}
            aria-invalid={valueError !== null}
            onChange={(e) => setValue(e.target.value)}
          />
          {valueError ? <FieldError>{valueError}</FieldError> : <FieldDescription>{t("mappings.valueHint")}</FieldDescription>}
        </Field>
        <Field>
          <FieldLabel htmlFor="mapping-role">{t("mappings.role")}</FieldLabel>
          <Combobox
            id="mapping-role"
            options={Array.isArray(roles) ? roles.map((r) => ({ value: r.code, label: r.label, detail: r.code })) : []}
            value={role}
            onValueChange={setRole}
            disabled={!Array.isArray(roles)}
            placeholder={t("mappings.rolePlaceholder")}
            searchPlaceholder={t("mappings.roleSearch")}
          />
          {roles === "failed" && <FieldError>{t("mappings.rolesFailed")}</FieldError>}
        </Field>
        <Field orientation="horizontal">
          <Checkbox id="mapping-active" checked={isActive} onCheckedChange={(v) => setIsActive(v === true)} />
          <FieldLabel htmlFor="mapping-active" className="font-normal">
            {t("mappings.isActive")}
          </FieldLabel>
        </Field>
        {error && <FieldError>{error}</FieldError>}
      </FieldGroup>
      <DialogFooter>
        <DialogClose asChild>
          <Button type="button" variant="outline">
            {t("common:actions.cancel")}
          </Button>
        </DialogClose>
        <Button type="submit" disabled={pending || !attr || !value.trim() || !role}>
          {t("common:actions.save")}
        </Button>
      </DialogFooter>
    </form>
  )
}
