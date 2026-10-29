import { useTranslation } from "react-i18next"

import { StatusBadge } from "@/components/status-badge"

/** An account's status; anything other than active or inactive shows as stored. */
export function AccountStatusBadge({ status }: { status: string | null | undefined }) {
  const { t } = useTranslation("users")
  if (status === "active") return <StatusBadge tone="success">{t("status.active")}</StatusBadge>
  if (status === "inactive") return <StatusBadge tone="warning">{t("status.inactive")}</StatusBadge>
  return status ? <StatusBadge tone="neutral">{status}</StatusBadge> : null
}
