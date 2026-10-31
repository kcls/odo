import { RotateCcwIcon } from "lucide-react"
import { useState } from "react"
import { useTranslation } from "react-i18next"
import {
  useLoaderData,
  useNavigate,
  useRevalidator,
  useSearchParams,
  type LoaderFunctionArgs,
} from "react-router"
import { toast } from "sonner"

import { templatesApi } from "@/api/notify"
import { PERMS } from "@/auth/perms"
import { checkPerms } from "@/auth/session"
import { ConfirmDialog } from "@/components/confirm-dialog"
import { DetailHeader } from "@/components/detail-header"
import { DisabledReason } from "@/components/disabled-reason"
import { Section } from "@/components/section"
import { StatusBadge } from "@/components/status-badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { useReportError } from "@/hooks/use-report-error"
import { loadNamespaces } from "@/i18n"
import { useFormatters } from "@/lib/format"
import { orNotFound, requireIdParam } from "@/lib/loader"
import type { RouteHandle } from "@/lib/route-handle"
import { useTemplate } from "./template-route"
import {
  CONTENT_FIELDS,
  changedFields,
  diffField,
  fieldText,
  type ContentField,
  type TemplateContent,
} from "./version-diff"
import { useFieldLabel, VersionAction } from "./version-labels"

export async function loader({ params }: LoaderFunctionArgs) {
  const templateId = requireIdParam(params.templateId)
  const number = requireIdParam(params.version)
  const [version, previous, perms] = await Promise.all([
    orNotFound(templatesApi.version(templateId, number)),
    // Version 1 has nothing before it.
    number > 1 ? templatesApi.version(templateId, number - 1) : null,
    checkPerms({ canWrite: PERMS.TEMPLATE_WRITE }),
    loadNamespaces("templates"),
  ])
  return { version, previous, ...perms }
}

export const handle: RouteHandle = {
  crumb: (match, t) =>
    t("history.versionLabel", {
      ns: "templates",
      version: (match.loaderData as Awaited<ReturnType<typeof loader>> | undefined)?.version.version,
    }),
}

/** `?compare=current` compares with the template as it is now. */
type Compare = "previous" | "current"

/**
 * One version of a template: what it changed from the version before, or
 * how it differs from the template now, and a way to restore it.
 */
export function Component() {
  const { version, previous, canWrite } = useLoaderData<typeof loader>()
  const template = useTemplate()
  const { t } = useTranslation(["templates", "common"])
  const fmt = useFormatters()
  const navigate = useNavigate()
  const revalidator = useRevalidator()
  const reportError = useReportError()
  const [params, setParams] = useSearchParams()
  const [confirming, setConfirming] = useState(false)

  const compare: Compare = params.get("compare") === "current" ? "current" : "previous"
  // "previous" shows what this version changed; "current" what restoring
  // it would change, so the template as it is now is the "before".
  const [before, after]: [TemplateContent | null, TemplateContent] =
    compare === "current" ? [template, version] : [previous, version]
  const isCurrent = changedFields(version, template).length === 0

  const restore = async () => {
    try {
      const restored = await templatesApi.restore(template.id, version.version)
      toast.success(t("history.restored", { name: restored.name, version: version.version }))
      await revalidator.revalidate()
      navigate("../..")
    } catch (err) {
      reportError(err, "Failed to restore template version")
    }
  }

  const restoreButton = isCurrent ? (
    <DisabledReason label={t("history.restore")} reason={t("history.isCurrent")}>
      <Button variant="outline" disabled aria-hidden tabIndex={-1}>
        <RotateCcwIcon data-icon="inline-start" />
        {t("history.restore")}
      </Button>
    </DisabledReason>
  ) : (
    <Button variant="outline" onClick={() => setConfirming(true)}>
      <RotateCcwIcon data-icon="inline-start" />
      {t("history.restore")}
    </Button>
  )

  return (
    <div className="flex flex-col gap-6">
      <DetailHeader
        backTo=".."
        backLabel={t("history.crumb")}
        title={t("history.versionLabel", { version: version.version })}
        summary={[
          <VersionAction action={version.action} restoredFrom={version.restored_from} />,
          version.changed_by_name ?? t("history.unknownAuthor"),
          <span title={fmt.dateTime(version.changed_at)}>{fmt.relativeTime(version.changed_at)}</span>,
          isCurrent && <StatusBadge tone="success">{t("history.current")}</StatusBadge>,
        ].filter(Boolean)}
        actions={canWrite && restoreButton}
      />

      <Section
        title={compare === "current" ? t("history.compareCurrentTitle") : t("history.comparePreviousTitle")}
        description={
          compare === "current"
            ? t("history.compareCurrentHint")
            : previous
              ? t("history.comparePreviousHint", { version: previous.version })
              : t("history.firstVersionHint")
        }
        actions={
          <ToggleGroup
            type="single"
            variant="outline"
            size="sm"
            value={compare}
            aria-label={t("history.compareLabel")}
            // Single select that can't be cleared: ignore a deselect.
            onValueChange={(v) =>
              v &&
              setParams(
                (prev) => {
                  if (v === "current") prev.set("compare", v)
                  else prev.delete("compare")
                  return prev
                },
                { replace: true }
              )
            }
          >
            <ToggleGroupItem value="previous" className="px-3">
              {t("history.comparePrevious")}
            </ToggleGroupItem>
            <ToggleGroupItem value="current" className="px-3">
              {t("history.compareCurrent")}
            </ToggleGroupItem>
          </ToggleGroup>
        }
      >
        <FieldDiffs before={before} after={after} />
      </Section>

      <ConfirmDialog
        open={confirming}
        onOpenChange={setConfirming}
        title={t("history.restoreTitle", { version: version.version })}
        description={t("history.restoreBody", { name: template.name, version: version.version })}
        confirmLabel={t("history.restore")}
        onConfirm={restore}
      />
    </div>
  )
}

/** Each changed field's diff, then the names of those that didn't change. */
function FieldDiffs({ before, after }: { before: TemplateContent | null; after: TemplateContent }) {
  const { t } = useTranslation("templates")
  const fieldLabel = useFieldLabel()
  const changed = changedFields(before, after)
  const unchanged = CONTENT_FIELDS.filter((f) => !changed.includes(f))

  if (changed.length === 0) {
    return <p className="text-sm text-muted-foreground">{t("history.identical")}</p>
  }
  return (
    <div className="flex flex-col gap-4">
      {changed.map((field) => (
        <Card key={field} size="sm">
          <CardHeader>
            <CardTitle>
              <h3 className="text-sm font-medium">{fieldLabel(field)}</h3>
            </CardTitle>
          </CardHeader>
          <CardContent>
            {field === "is_active" ? (
              <ActiveChange before={before?.is_active ?? null} after={after.is_active} />
            ) : (
              <TextDiff field={field} before={fieldText(before, field)} after={fieldText(after, field)} />
            )}
          </CardContent>
        </Card>
      ))}
      {unchanged.length > 0 && (
        <p className="text-sm text-muted-foreground">
          {t("history.unchanged", { fields: unchanged.map(fieldLabel).join(", ") })}
        </p>
      )}
    </div>
  )
}

function ActiveChange({ before, after }: { before: boolean | null; after: boolean }) {
  const { t } = useTranslation("templates")
  const label = (active: boolean) => (active ? t("active") : t("inactive"))
  return (
    <p className="text-sm">
      {before === null ? label(after) : t("history.activeChange", { before: label(before), after: label(after) })}
    </p>
  )
}

/**
 * Removed text struck through on a red tint, added text underlined on a
 * green one: the marks carry the meaning, not the color alone. Screen
 * readers mostly ignore <ins>/<del>, so each change is also announced.
 */
function TextDiff({
  field,
  before,
  after,
}: {
  field: Exclude<ContentField, "is_active">
  before: string
  after: string
}) {
  const { t } = useTranslation("templates")
  return (
    <pre className="max-h-[32rem] overflow-auto rounded-md border bg-muted/30 p-3 font-mono text-xs whitespace-pre-wrap">
      {diffField(field, before, after).map((p, i) =>
        p.kind === "added" ? (
          <ins key={i} className="bg-success/15 text-foreground underline decoration-success">
            <span className="sr-only">{t("history.added")} </span>
            {p.value}
          </ins>
        ) : p.kind === "removed" ? (
          <del key={i} className="bg-destructive/15 text-foreground line-through decoration-destructive">
            <span className="sr-only">{t("history.removed")} </span>
            {p.value}
          </del>
        ) : (
          <span key={i}>{p.value}</span>
        )
      )}
    </pre>
  )
}
