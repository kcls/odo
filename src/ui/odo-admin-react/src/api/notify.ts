/**
 * Typed client for the odo-notify admin endpoints: notification
 * templates and email groups.
 */
import { apiPost } from "./client"
import { fetchAllPages } from "./paging"
import type { components } from "./types/odo-notify"

type S = components["schemas"]

export type TemplateRow = S["TemplateRow"]
export type TemplatePage = S["TemplatePage"]
export type PreviewResponse = S["PreviewResponse"]

const TEMPLATE = "/api/v1/odo/notify/template"

export const templatesApi = {
  /** One page of templates: server-driven search + sort + pagination. */
  listPage(params: Partial<S["ListTemplatesRequest"]>): Promise<TemplatePage> {
    return apiPost(`${TEMPLATE}/list`, params)
  },
  /**
   * One template by id. There's no get endpoint, so this pages through
   * the list; template counts are small. Undefined when there's none.
   */
  async find(id: number): Promise<TemplateRow | undefined> {
    const all = await fetchAllPages((page) => templatesApi.listPage(page))
    return all.find((t) => t.id === id)
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
