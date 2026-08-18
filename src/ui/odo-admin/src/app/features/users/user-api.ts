/**
 * Typed client for the odo-auth read-only user detail + update endpoints.
 * Request/response shapes are the generated types from the committed OpenAPI
 * spec (source of truth: the Rust structs), so a backend field change surfaces
 * here as a compile error.
 *
 * User *search* and *get* go through @odo/core's authApi (which carries its
 * own shapes and token handling) — see users-search / user-role-detail. This
 * client covers only the admin detail + update surface, which is distinct.
 */
import { apiPost } from '../../core/api';
import type { components } from '../../core/api-types/odo-auth';

type S = components['schemas'];

export type UserAccount = S['UserAccountRow'];
export type UserDetail = S['UserDetailResponse'];
export type LocalAccount = S['LocalAccountRow'];
export type SamlIdentity = S['SamlIdentityRow'];
export type SamlUserAttr = S['SamlUserAttrRow'];
export type UserSession = S['SessionRow'];
export type AssignmentRow = S['AssignmentRow'];

const BASE = '/api/v1/odo/auth/user';

export const userAdminApi = {
  getDetail(id: number): Promise<UserDetail> {
    return apiPost(`${BASE}/detail`, { id });
  },

  /** Local accounts only; SAML accounts are refused server-side. */
  updateUser(
    id: number,
    params: Omit<S['UpdateUserRequest'], 'id'>,
  ): Promise<UserAccount> {
    return apiPost(`${BASE}/update`, { id, ...params });
  },
};
