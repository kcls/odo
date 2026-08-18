/**
 * Typed client for the odo-notify email-group admin endpoints. Request/response
 * shapes are the generated types from the committed OpenAPI spec (source of
 * truth: the Rust structs), so a backend field change surfaces here as a
 * compile error.
 */
import { apiPost } from '../../core/api';
import type { components } from '../../core/api-types/odo-notify';

type S = components['schemas'];

export type EmailGroupRow = S['EmailGroupRow'];
export type EmailGroupPage = S['EmailGroupPage'];
export type EmailGroupMemberRow = S['EmailGroupMemberRow'];
export type EmailGroupDetailResponse = S['EmailGroupDetailResponse'];

const BASE = '/api/v1/odo/notify/email-group';

export const emailGroupApi = {
  /** One page of email groups: server-driven search + sort + pagination. */
  listPage(
    params: Partial<S['ListEmailGroupsRequest']>,
  ): Promise<EmailGroupPage> {
    return apiPost<EmailGroupPage>(`${BASE}/list`, params);
  },

  get(id: number): Promise<EmailGroupDetailResponse> {
    return apiPost(`${BASE}/get`, { id } satisfies S['EmailGroupIdRequest']);
  },

  create(params: S['CreateEmailGroupRequest']): Promise<EmailGroupRow> {
    return apiPost(`${BASE}/create`, params);
  },

  update(params: S['UpdateEmailGroupRequest']): Promise<EmailGroupRow> {
    return apiPost(`${BASE}/update`, params);
  },

  addMember(params: S['CreateEmailGroupMemberRequest']): Promise<EmailGroupMemberRow> {
    return apiPost(`${BASE}/member/create`, params);
  },

  updateMember(params: S['UpdateEmailGroupMemberRequest']): Promise<EmailGroupMemberRow> {
    return apiPost(`${BASE}/member/update`, params);
  },

  deleteMember(id: number): Promise<S['SuccessResponse']> {
    return apiPost(`${BASE}/member/delete`, {
      id,
    } satisfies S['EmailGroupMemberIdRequest']);
  },
};
