/**
 * Typed client for the odo-notify template admin endpoints. Request/response
 * shapes are the generated types from the committed OpenAPI spec (source of
 * truth: the Rust structs), so a backend field change surfaces here as a
 * compile error.
 */
import { apiPost } from '../../core/api';
import type { components } from '../../core/api-types/odo-notify';

type S = components['schemas'];

export type TemplateRow = S['TemplateRow'];
export type TemplatePage = S['TemplatePage'];
export type CreateTemplateRequest = S['CreateTemplateRequest'];
export type UpdateTemplateRequest = S['UpdateTemplateRequest'];
export type PreviewRequest = S['PreviewRequest'];
export type PreviewResponse = S['PreviewResponse'];

const BASE = '/api/v1/odo/notify/template';

export const templatesApi = {
  /**
   * All templates matching `search` (unpaginated). Used by the editor to look
   * up a template by id (there is no single-get endpoint); counts are small.
   */
  async list(search?: string): Promise<TemplateRow[]> {
    const result = await apiPost<TemplatePage>(`${BASE}/list`, {
      search: search || undefined,
      limit: 1000,
    } satisfies S['ListTemplatesRequest']);
    return result.rows;
  },

  /** One page of templates: server-driven search + sort + pagination. */
  listPage(params: Partial<S['ListTemplatesRequest']>): Promise<TemplatePage> {
    return apiPost<TemplatePage>(`${BASE}/list`, params);
  },

  create(params: CreateTemplateRequest): Promise<TemplateRow> {
    return apiPost(`${BASE}/create`, params);
  },

  update(
    id: number,
    params: Omit<UpdateTemplateRequest, 'id'>,
  ): Promise<TemplateRow> {
    return apiPost(`${BASE}/update`, { id, ...params });
  },

  delete(id: number): Promise<S['SuccessResponse']> {
    return apiPost(`${BASE}/delete`, { id } satisfies S['TemplateIdRequest']);
  },

  preview(params: PreviewRequest): Promise<PreviewResponse> {
    return apiPost(`${BASE}/preview`, params);
  },
};
