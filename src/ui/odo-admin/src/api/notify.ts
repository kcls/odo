/**
 * Typed client for the odo-notify admin endpoints: notification
 * templates and email groups.
 */
import { apiPost } from "./client"
import type { components } from "./types/odo-notify"

type S = components["schemas"]

export type TemplateRow = S["TemplateRow"]
export type TemplatePage = S["TemplatePage"]
export type PreviewResponse = S["PreviewResponse"]
export type TemplateVersionSummary = S["TemplateVersionSummary"]
export type TemplateVersionPage = S["TemplateVersionPage"]
export type TemplateVersionRow = S["TemplateVersionRow"]

const TEMPLATE = "/api/v1/odo/notify/template"

export const templatesApi = {
  /** One page of templates: server-driven search + sort + pagination. */
  listPage(params: Partial<S["ListTemplatesRequest"]>): Promise<TemplatePage> {
    return apiPost(`${TEMPLATE}/list`, params)
  },
  /** One template by id; 404 if it's deleted or never existed. */
  get(id: number): Promise<TemplateRow> {
    return apiPost(`${TEMPLATE}/get`, { id } satisfies S["TemplateIdRequest"])
  },
  create(params: S["CreateTemplateRequest"]): Promise<TemplateRow> {
    return apiPost(`${TEMPLATE}/create`, params)
  },
  /** An empty body_template_html clears the HTML variant. */
  update(id: number, params: Omit<S["UpdateTemplateRequest"], "id">): Promise<TemplateRow> {
    return apiPost(`${TEMPLATE}/update`, { id, ...params })
  },
  delete(id: number): Promise<S["SuccessResponse"]> {
    return apiPost(`${TEMPLATE}/delete`, { id } satisfies S["TemplateIdRequest"])
  },
  /** Render (possibly unsaved) template text against sample variables. */
  preview(params: S["PreviewRequest"]): Promise<PreviewResponse> {
    return apiPost(`${TEMPLATE}/preview`, params)
  },
  /** One page of a live template's versions, newest first. */
  versions(template: number, page: { limit: number; offset: number }): Promise<TemplateVersionPage> {
    return apiPost(`${TEMPLATE}/versions`, { template, ...page } satisfies S["ListTemplateVersionsRequest"])
  },
  /** The template as it stood at one version. */
  version(template: number, version: number): Promise<TemplateVersionRow> {
    return apiPost(`${TEMPLATE}/version`, { template, version } satisfies S["TemplateVersionRequest"])
  },
  /** Put the template back as it was at a version; recorded as a new version. */
  restore(template: number, version: number): Promise<TemplateRow> {
    return apiPost(`${TEMPLATE}/restore`, { template, version } satisfies S["TemplateVersionRequest"])
  },
}

export type EmailGroupRow = S["EmailGroupRow"]
export type EmailGroupPage = S["EmailGroupPage"]
export type EmailGroupMemberRow = S["EmailGroupMemberRow"]
export type EmailGroupDetail = S["EmailGroupDetailResponse"]

const EMAIL_GROUP = "/api/v1/odo/notify/email-group"

export const emailGroupsApi = {
  /** One page of groups: server-driven search + sort + pagination. */
  listPage(params: Partial<S["ListEmailGroupsRequest"]>): Promise<EmailGroupPage> {
    return apiPost(`${EMAIL_GROUP}/list`, params)
  },
  get(id: number): Promise<EmailGroupDetail> {
    return apiPost(`${EMAIL_GROUP}/get`, { id } satisfies S["EmailGroupIdRequest"])
  },
  create(params: S["CreateEmailGroupRequest"]): Promise<EmailGroupRow> {
    return apiPost(`${EMAIL_GROUP}/create`, params)
  },
  update(params: S["UpdateEmailGroupRequest"]): Promise<EmailGroupRow> {
    return apiPost(`${EMAIL_GROUP}/update`, params)
  },
  addMember(params: S["CreateEmailGroupMemberRequest"]): Promise<EmailGroupMemberRow> {
    return apiPost(`${EMAIL_GROUP}/member/create`, params)
  },
  updateMember(params: S["UpdateEmailGroupMemberRequest"]): Promise<EmailGroupMemberRow> {
    return apiPost(`${EMAIL_GROUP}/member/update`, params)
  },
  deleteMember(id: number): Promise<S["SuccessResponse"]> {
    return apiPost(`${EMAIL_GROUP}/member/delete`, { id } satisfies S["EmailGroupMemberIdRequest"])
  },
}

export type DeliveryRow = S["DeliveryRow"]
export type DeliveryPage = S["DeliveryPage"]
export type DeliveryDetail = S["DeliveryDetailResponse"]
export type DeliveryRecipient = S["DeliveryRecipient"]
export type DeliverySummary = S["DeliverySummaryResponse"]
export type RecipientActivityRow = S["RecipientActivityRow"]
export type RecipientActivityPage = S["RecipientActivityPage"]
export type DailyCounts = S["DailyCounts"]

const DELIVERY = "/api/v1/odo/notify/delivery"

/** Delivery history (odo.notify.delivery.read) and retrying (….write). */
export const deliveriesApi = {
  /** One page of deliveries, newest first unless sorted otherwise. */
  listPage(params: Partial<S["ListDeliveriesRequest"]>): Promise<DeliveryPage> {
    return apiPost(`${DELIVERY}/list`, params)
  },
  get(id: number): Promise<DeliveryDetail> {
    return apiPost(`${DELIVERY}/get`, { id } satisfies S["DeliveryIdRequest"])
  },
  /** Counts by channel and status; the window defaults to the last 7 days. */
  summary(params: S["DeliveryWindowRequest"]): Promise<DeliverySummary> {
    return apiPost(`${DELIVERY}/summary`, params)
  },
  /** Deliveries per day by outcome, every day in the window, in `timezone`. */
  daily(params: S["DailyDeliveriesRequest"]): Promise<S["DailyDeliveriesResponse"]> {
    return apiPost(`${DELIVERY}/daily`, params)
  },
  /** Queue a failed or skipped email delivery to send again (delivery.write). */
  retry(id: number): Promise<DeliveryRow> {
    return apiPost(`${DELIVERY}/retry`, { id } satisfies S["DeliveryIdRequest"])
  },
  /** Per-recipient activity; sortable by total, failed, or last. */
  recipientsPage(params: Partial<S["ListRecipientActivityRequest"]>): Promise<RecipientActivityPage> {
    return apiPost(`${DELIVERY}/recipients`, params)
  },
}
