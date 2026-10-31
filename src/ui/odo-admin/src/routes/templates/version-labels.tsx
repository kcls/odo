import { useTranslation } from "react-i18next"

import { StatusBadge, type Tone } from "@/components/status-badge"
import type { ContentField } from "./version-diff"

const TONE: Record<string, Tone> = {
  baseline: "neutral",
  create: "info",
  update: "neutral",
  restore: "warning",
  delete: "danger",
}

/** What a version did: created, edited, restored from another, and so on. */
export function VersionAction({ action, restoredFrom }: { action: string; restoredFrom?: number | null }) {
  const { t } = useTranslation("templates")
  const label =
    action === "restore" && restoredFrom != null
      ? t("history.actions.restore", { version: restoredFrom })
      : t(`history.actions.${action as "baseline" | "create" | "update" | "delete"}`, { defaultValue: action })
  return <StatusBadge tone={TONE[action] ?? "neutral"}>{label}</StatusBadge>
}

const FIELD_LABEL = {
  code: "editor.code",
  name: "editor.name",
  description: "editor.description",
  is_active: "editor.isActive",
  subject_template: "editor.subject",
  body_template: "editor.body",
  body_template_html: "editor.bodyHtml",
  sample_data: "editor.sampleData",
  deleted: "history.deleted",
} as const

/** A versioned field's name as the editor labels it. */
export function useFieldLabel() {
  const { t } = useTranslation("templates")
  return (field: ContentField | "deleted" | string) =>
    field in FIELD_LABEL ? t(FIELD_LABEL[field as keyof typeof FIELD_LABEL]) : field
}
