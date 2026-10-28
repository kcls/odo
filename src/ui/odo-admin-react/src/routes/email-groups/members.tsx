import { PlusIcon, SearchIcon, Trash2Icon } from "lucide-react"
import { useState, type FormEvent } from "react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"

import { ApiRequestError } from "@/api/client"
import { describeError } from "@/api/errors"
import { emailGroupsApi, type EmailGroupMemberRow, type EmailGroupRow } from "@/api/notify"
import { ConfirmDialog } from "@/components/confirm-dialog"
import { DataTable, type Column } from "@/components/data-table/data-table"
import { Section } from "@/components/section"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Field, FieldError } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { InputGroup, InputGroupAddon, InputGroupInput } from "@/components/ui/input-group"
import { useReportError } from "@/hooks/use-report-error"
import { useFormatters } from "@/lib/format"

/** Above this many members, offer a filter box. */
const FILTER_THRESHOLD = 10

/**
 * A group's member addresses: add inline (one after another, so the
 * field keeps focus), switch each on or off, or remove it.
 */
export function Members({
  group,
  members,
  canWrite,
  onChanged,
}: {
  group: EmailGroupRow
  members: EmailGroupMemberRow[]
  canWrite: boolean
  onChanged: () => void
}) {
  const { t } = useTranslation(["emailGroups", "common"])
  const fmt = useFormatters()
  const reportError = useReportError()
  const [filter, setFilter] = useState("")
  const [removing, setRemoving] = useState<EmailGroupMemberRow | null>(null)
  // Member ids with an active-flag change in flight.
  const [toggling, setToggling] = useState<ReadonlySet<number>>(new Set())

  const q = filter.trim().toLowerCase()
  const visible = [...members]
    .sort((a, b) => a.email.localeCompare(b.email))
    .filter((m) => !q || m.email.toLowerCase().includes(q))

  const setMemberActive = async (member: EmailGroupMemberRow, active: boolean) => {
    setToggling((s) => new Set(s).add(member.id))
    try {
      await emailGroupsApi.updateMember({ id: member.id, is_active: active })
      toast.success(t(active ? "members.activated" : "members.deactivated", { email: member.email }))
      onChanged()
    } catch (err) {
      reportError(err, "Failed to update member")
    } finally {
      setToggling((s) => {
        const next = new Set(s)
        next.delete(member.id)
        return next
      })
    }
  }

  const remove = async (member: EmailGroupMemberRow) => {
    try {
      await emailGroupsApi.deleteMember(member.id)
      toast.success(t("members.removed", { email: member.email }))
      onChanged()
    } catch (err) {
      reportError(err, "Failed to remove member")
    }
  }

  const columns: Column<EmailGroupMemberRow>[] = [
    {
      id: "email",
      header: t("members.columns.email"),
      cell: (m) => <span className={m.is_active ? undefined : "text-muted-foreground line-through"}>{m.email}</span>,
    },
    {
      id: "active",
      header: t("members.columns.active"),
      className: "w-24",
      cell: (m) => (
        <Checkbox
          checked={m.is_active}
          disabled={!canWrite || toggling.has(m.id)}
          aria-label={t("members.activeLabel", { email: m.email })}
          onCheckedChange={(v) => void setMemberActive(m, v === true)}
        />
      ),
    },
    {
      id: "added",
      header: t("members.columns.added"),
      className: "whitespace-nowrap",
      cell: (m) => fmt.date(m.created_at),
    },
  ]
  if (canWrite) {
    columns.push({
      id: "actions",
      header: null,
      className: "w-1 text-end",
      cell: (m) => (
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label={t("members.removeLabel", { email: m.email })}
          onClick={() => setRemoving(m)}
        >
          <Trash2Icon />
        </Button>
      ),
    })
  }

  return (
    <Section title={t("members.title")} description={t("members.description")}>
      <div className="flex flex-wrap items-start justify-between gap-3">
        {canWrite && <AddMember groupId={group.id} onAdded={onChanged} />}
        {members.length > FILTER_THRESHOLD && (
          <InputGroup className="max-w-xs">
            <InputGroupAddon>
              <SearchIcon />
            </InputGroupAddon>
            <InputGroupInput
              type="search"
              value={filter}
              placeholder={t("members.filter")}
              aria-label={t("members.filter")}
              onChange={(e) => setFilter(e.target.value)}
            />
          </InputGroup>
        )}
      </div>
      <DataTable
        label={t("members.title")}
        rows={visible}
        columns={columns}
        rowKey={(m) => m.id}
        empty={members.length === 0 ? t("members.none") : t("members.noMatches")}
      />

      <ConfirmDialog
        open={removing !== null}
        onOpenChange={(open) => !open && setRemoving(null)}
        title={t("members.removeTitle")}
        description={t("members.removeBody", { email: removing?.email ?? "", label: group.label })}
        confirmLabel={t("common:actions.remove")}
        destructive
        onConfirm={async () => {
          if (removing) await remove(removing)
        }}
      />
    </Section>
  )
}

function AddMember({ groupId, onAdded }: { groupId: number; onAdded: () => void }) {
  const { t } = useTranslation(["emailGroups", "common"])
  const [email, setEmail] = useState("")
  const [error, setError] = useState<string | null>(null)
  const [pending, setPending] = useState(false)

  const submit = async (e: FormEvent) => {
    e.preventDefault()
    const address = email.trim()
    if (!address) return
    setPending(true)
    setError(null)
    try {
      await emailGroupsApi.addMember({ email_group: groupId, email: address })
      toast.success(t("members.added", { email: address }))
      setEmail("")
      onAdded()
    } catch (err) {
      if (err instanceof ApiRequestError && err.code === "EMAIL_ALREADY_IN_GROUP") {
        setError(t("common:errors.codes.EMAIL_ALREADY_IN_GROUP"))
      } else if (err instanceof ApiRequestError && err.status === 400) {
        setError(t("members.invalidEmail"))
      } else {
        setError(describeError(err))
      }
    } finally {
      setPending(false)
    }
  }

  return (
    <form onSubmit={submit} className="flex min-w-0 flex-1 flex-col gap-1.5 sm:max-w-md">
      <Field data-invalid={error !== null} orientation="horizontal">
        <Input
          type="email"
          autoComplete="off"
          value={email}
          placeholder={t("members.addPlaceholder")}
          aria-label={t("members.addLabel")}
          aria-invalid={error !== null}
          onChange={(e) => setEmail(e.target.value)}
        />
        <Button type="submit" variant="outline" disabled={pending || !email.trim()}>
          <PlusIcon data-icon="inline-start" />
          {t("members.add")}
        </Button>
      </Field>
      {error && <FieldError>{error}</FieldError>}
    </form>
  )
}
