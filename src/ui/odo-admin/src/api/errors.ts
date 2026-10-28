import i18n from "@/i18n"

import { ApiRequestError } from "./client"

/**
 * Backend error codes shared across tools that warrant a friendlier
 * message than the raw server text. Tool-specific codes are handled
 * inline where the context makes a better message possible; this covers
 * the general cases and anything not caught inline. Each code has a
 * matching `errors.codes.*` string in locales/en/common.json.
 */
const KNOWN_CODES = [
  "NOT_LOCAL_ACCOUNT",
  "SAML_MANAGED",
  "PERMISSION_IN_USE",
  "ROLE_ASSIGNED",
  "ROLE_SAML_MAPPED",
  "ATTRIBUTE_IN_USE",
  "ATTRIBUTE_HAS_VALUES",
  "IDP_IN_USE",
  "IDP_HAS_IDENTITIES",
  "SP_IN_USE",
  "TYPE_IN_USE",
  "TYPE_HAS_CHILDREN",
  "UNIT_HAS_CHILDREN",
  "UNIT_IS_ROOT",
  "PERMISSION_CODE_TAKEN",
  "ROLE_CODE_TAKEN",
  "PERMISSION_ALREADY_GRANTED",
  "ALREADY_ASSIGNED",
  "ENTITY_ID_TAKEN",
  "ATTRIBUTE_EXISTS",
  "MAPPING_EXISTS",
  "TEMPLATE_CODE_TAKEN",
  "EMAIL_GROUP_CODE_TAKEN",
  "EMAIL_ALREADY_IN_GROUP",
  "UNIT_CODE_TAKEN",
  "UNIT_LABEL_TAKEN",
  "TYPE_LABEL_TAKEN",
] as const
type KnownCode = (typeof KNOWN_CODES)[number]

function isKnownCode(code: string | undefined): code is KnownCode {
  return KNOWN_CODES.includes(code as KnownCode)
}

function statusMessage(status: number): string {
  const { t } = i18n
  switch (status) {
    case 400:
      return t("errors.status.badRequest")
    case 401:
      return t("errors.status.unauthorized")
    case 403:
      return t("errors.status.forbidden")
    case 404:
      return t("errors.status.notFound")
    case 409:
      return t("errors.status.conflict")
    default:
      return status >= 500
        ? t("errors.status.server")
        : t("errors.status.other")
  }
}

/**
 * Turn any thrown value into a user-facing message. Prefers a known-code
 * message, then the server-supplied message, then a status-based fallback.
 */
export function describeError(err: unknown): string {
  const { t } = i18n
  if (err instanceof ApiRequestError) {
    if (isKnownCode(err.code)) return t(`errors.codes.${err.code}`)
    if (err.serverMessage) return err.serverMessage
    return statusMessage(err.status)
  }
  return t("errors.unexpected")
}
