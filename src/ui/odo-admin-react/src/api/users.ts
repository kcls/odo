/**
 * Typed client for the odo-auth user endpoints: search, and the admin
 * detail + update surface.
 */
import { apiPost } from "./client"
import type { components } from "./types/odo-auth"

type S = components["schemas"]

export type UserSummary = S["UserResponse"]
export type UserAccount = S["UserAccountRow"]
export type UserDetail = S["UserDetailResponse"]
export type UserSession = S["SessionRow"]

const BASE = "/api/v1/odo/auth/user"

export const usersApi = {
  /** Active users matching every keyword, ordered by display name. */
  search(params: S["UserSearchRequest"]): Promise<UserSummary[]> {
    return apiPost(`${BASE}/search`, params)
  },

  getDetail(id: number): Promise<UserDetail> {
    return apiPost(`${BASE}/detail`, { id })
  },

  /**
   * A new active account. A local account's password must be set here:
   * there is no endpoint to set one later.
   */
  create(params: S["CreateUserRequest"]): Promise<UserAccount> {
    return apiPost(`${BASE}/create`, params)
  },

  /** Local accounts only; SAML accounts are refused server-side. */
  update(id: number, params: Omit<S["UpdateUserRequest"], "id">): Promise<UserAccount> {
    return apiPost(`${BASE}/update`, { id, ...params })
  },
}
