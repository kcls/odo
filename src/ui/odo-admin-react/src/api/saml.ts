/**
 * Typed client for the odo-auth SAML admin endpoints: identity
 * providers, service providers, tracked assertion attributes, and
 * attribute-to-role mappings.
 *
 * The SP has no signing material of its own (odo never signs; see
 * odo:006_drop_sp_signing_material). An SP's `idp_x509_cert` is the
 * *IdP's* certificate, used to verify incoming assertions.
 */
import { apiPost } from "./client"
import type { components } from "./types/odo-auth"

type S = components["schemas"]

export type IdpRow = S["IdpRow"]
export type SpRow = S["SpRow"]
export type AttributeRow = S["AttributeRow"]
export type AttrRoleMapRow = S["AttrRoleMapRow"]
export type AttrRoleMapPage = S["AttrRoleMapPage"]

/** Value normalizers the server understands (auth.normalize_saml_attr_value). */
export const NORMALIZERS = ["split_slash_first", "split_slash_last"] as const
export type Normalizer = (typeof NORMALIZERS)[number]

const BASE = "/api/v1/odo/auth/saml/admin"

export const samlApi = {
  // IdPs, SPs, and attributes are small, bounded lists.
  async listIdps(): Promise<IdpRow[]> {
    return (await apiPost<S["ListIdpsResponse"]>(`${BASE}/idp/list`, {})).idps
  },
  createIdp(params: S["CreateIdpRequest"]): Promise<IdpRow> {
    return apiPost(`${BASE}/idp/create`, params)
  },
  /** Null fields are unchanged; the `clear_*` flags remove session lifetime or attribute mapping. */
  updateIdp(id: number, params: Omit<S["UpdateIdpRequest"], "id">): Promise<IdpRow> {
    return apiPost(`${BASE}/idp/update`, { id, ...params })
  },
  deleteIdp(id: number): Promise<S["SamlAdminSuccessResponse"]> {
    return apiPost(`${BASE}/idp/delete`, { id })
  },

  async listSps(): Promise<SpRow[]> {
    return (await apiPost<S["ListSpsResponse"]>(`${BASE}/sp/list`, {})).sps
  },
  createSp(params: S["CreateSpRequest"]): Promise<SpRow> {
    return apiPost(`${BASE}/sp/create`, params)
  },
  /** A null `idp` is unchanged; `clear_idp` unlinks the SP. */
  updateSp(id: number, params: Omit<S["UpdateSpRequest"], "id">): Promise<SpRow> {
    return apiPost(`${BASE}/sp/update`, { id, ...params })
  },
  deleteSp(id: number): Promise<S["SamlAdminSuccessResponse"]> {
    return apiPost(`${BASE}/sp/delete`, { id })
  },

  async listAttributes(): Promise<AttributeRow[]> {
    return (await apiPost<S["ListAttributesResponse"]>(`${BASE}/attribute/list`, {})).attributes
  },
  createAttribute(params: S["CreateAttributeRequest"]): Promise<AttributeRow> {
    return apiPost(`${BASE}/attribute/create`, params)
  },
  /** An empty normalizer clears it. */
  updateAttribute(id: number, params: Omit<S["UpdateAttributeRequest"], "id">): Promise<AttributeRow> {
    return apiPost(`${BASE}/attribute/update`, { id, ...params })
  },
  deleteAttribute(id: number): Promise<S["SamlAttrSuccessResponse"]> {
    return apiPost(`${BASE}/attribute/delete`, { id })
  },

  /** Mappings can grow large: server-filtered and paginated. */
  listMappingsPage(params: Partial<S["ListAttrRoleMapsRequest"]>): Promise<AttrRoleMapPage> {
    return apiPost(`${BASE}/attr-role-map/list`, params)
  },
  createMapping(params: S["CreateAttrRoleMapRequest"]): Promise<AttrRoleMapRow> {
    return apiPost(`${BASE}/attr-role-map/create`, params)
  },
  updateMapping(id: number, params: Omit<S["UpdateAttrRoleMapRequest"], "id">): Promise<AttrRoleMapRow> {
    return apiPost(`${BASE}/attr-role-map/update`, { id, ...params })
  },
  deleteMapping(id: number): Promise<S["SamlAdminSuccessResponse"]> {
    return apiPost(`${BASE}/attr-role-map/delete`, { id })
  },
}
