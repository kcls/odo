/**
 * Typed client for the odo-auth SAML config admin endpoints (IdPs, SPs,
 * attributes, and attribute-to-role mappings). Request/response shapes are the
 * generated types from the committed OpenAPI spec (source of truth: the Rust
 * structs), so a backend field change surfaces here as a compile error.
 *
 * Security note: the SP private key is write-only. `SpRow` never carries it
 * (only `has_private_key`); the dialog sends it on create/update but nothing
 * ever renders it.
 */
import { apiPost } from '../../core/api';
import type { components } from '../../core/api-types/odo-auth';

type S = components['schemas'];

export type IdpRow = S['IdpRow'];
export type SpRow = S['SpRow'];
export type AttributeRow = S['AttributeRow'];
export type AttrRoleMapRow = S['AttrRoleMapRow'];
export type AttrRoleMapPage = S['AttrRoleMapPage'];

export type CreateIdpRequest = S['CreateIdpRequest'];
export type UpdateIdpRequest = S['UpdateIdpRequest'];
export type CreateSpRequest = S['CreateSpRequest'];
export type UpdateSpRequest = S['UpdateSpRequest'];
export type CreateAttributeRequest = S['CreateAttributeRequest'];
export type UpdateAttributeRequest = S['UpdateAttributeRequest'];
export type CreateAttrRoleMapRequest = S['CreateAttrRoleMapRequest'];
export type UpdateAttrRoleMapRequest = S['UpdateAttrRoleMapRequest'];
export type ListAttrRoleMapsRequest = S['ListAttrRoleMapsRequest'];

/** Normalizers understood by the server (see auth.normalize_saml_attr_value). */
export const NORMALIZERS = ['split_slash_first', 'split_slash_last'] as const;

const BASE = '/api/v1/odo/auth/saml/admin';

export const samlAdminApi = {
  // --- Identity providers (bounded list) ---

  async listIdps(): Promise<IdpRow[]> {
    const result = await apiPost<S['ListIdpsResponse']>(`${BASE}/idp/list`, {});
    return result.idps;
  },

  createIdp(params: CreateIdpRequest): Promise<IdpRow> {
    return apiPost(`${BASE}/idp/create`, params);
  },
  updateIdp(id: number, params: Omit<UpdateIdpRequest, 'id'>): Promise<IdpRow> {
    return apiPost(`${BASE}/idp/update`, { id, ...params });
  },
  deleteIdp(id: number): Promise<S['SamlAdminSuccessResponse']> {
    return apiPost(`${BASE}/idp/delete`, { id });
  },

  // --- Service providers (bounded list) ---

  async listSps(): Promise<SpRow[]> {
    const result = await apiPost<S['ListSpsResponse']>(`${BASE}/sp/list`, {});
    return result.sps;
  },

  createSp(params: CreateSpRequest): Promise<SpRow> {
    return apiPost(`${BASE}/sp/create`, params);
  },
  updateSp(id: number, params: Omit<UpdateSpRequest, 'id'>): Promise<SpRow> {
    return apiPost(`${BASE}/sp/update`, { id, ...params });
  },
  deleteSp(id: number): Promise<S['SamlAdminSuccessResponse']> {
    return apiPost(`${BASE}/sp/delete`, { id });
  },

  // --- Attributes (bounded list) ---

  async listAttributes(): Promise<AttributeRow[]> {
    const result = await apiPost<S['ListAttributesResponse']>(
      `${BASE}/attribute/list`,
      {},
    );
    return result.attributes;
  },

  createAttribute(params: CreateAttributeRequest): Promise<AttributeRow> {
    return apiPost(`${BASE}/attribute/create`, params);
  },
  updateAttribute(
    id: number,
    params: Omit<UpdateAttributeRequest, 'id'>,
  ): Promise<AttributeRow> {
    return apiPost(`${BASE}/attribute/update`, { id, ...params });
  },
  deleteAttribute(id: number): Promise<S['SamlAttrSuccessResponse']> {
    return apiPost(`${BASE}/attribute/delete`, { id });
  },

  // --- Attribute-to-role mappings (paginated: returns { rows, total }) ---

  listAttrRoleMaps(
    filter: Partial<ListAttrRoleMapsRequest> = {},
  ): Promise<AttrRoleMapPage> {
    return apiPost(`${BASE}/attr-role-map/list`, filter);
  },

  createAttrRoleMap(params: CreateAttrRoleMapRequest): Promise<AttrRoleMapRow> {
    return apiPost(`${BASE}/attr-role-map/create`, params);
  },
  updateAttrRoleMap(
    id: number,
    params: Omit<UpdateAttrRoleMapRequest, 'id'>,
  ): Promise<AttrRoleMapRow> {
    return apiPost(`${BASE}/attr-role-map/update`, { id, ...params });
  },
  deleteAttrRoleMap(id: number): Promise<S['SamlAdminSuccessResponse']> {
    return apiPost(`${BASE}/attr-role-map/delete`, { id });
  },
};
