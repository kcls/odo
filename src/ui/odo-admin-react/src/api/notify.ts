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
