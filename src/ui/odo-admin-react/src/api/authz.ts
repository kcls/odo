/**
 * Typed client for the odo-auth authz admin endpoints: permissions, roles,
 * their permission grants, and user role assignments. Shapes are the
 * generated types from the committed OpenAPI spec (source of truth: the
 * Rust structs), so a backend field change surfaces here as a compile
 * error.
 */
import { apiPost } from "./client"
import { fetchAllPages } from "./paging"
import type { components } from "./types/odo-auth"

type S = components["schemas"]

export type PermissionRow = S["PermissionRow"]
export type PermissionPage = S["PermissionPage"]
export type RoleRow = S["RoleRow"]
export type RolePage = S["RolePage"]
export type RoleDetailResponse = S["RoleDetailResponse"]
export type GrantRow = S["GrantRow"]
export type RolePermissionRow = S["RolePermissionRow"]
export type AssignmentRow = S["AssignmentRow"]
export type PermScopeRow = S["PermScopeRow"]
export type ScopeUnit = S["ScopeUnit"]

const BASE = "/api/v1/odo/auth/authz"

export const authzApi = {
  // --- Permissions ---

  /** One page of permissions: server-driven search + sort + pagination. */
  listPermissionsPage(params: Partial<S["ListPermissionsRequest"]>): Promise<PermissionPage> {
    return apiPost(`${BASE}/permission/list`, params)
  },
  createPermission(params: S["CreatePermissionRequest"]): Promise<PermissionRow> {
    return apiPost(`${BASE}/permission/create`, params)
  },
  updatePermission(params: S["UpdatePermissionRequest"]): Promise<PermissionRow> {
    return apiPost(`${BASE}/permission/update`, params)
  },
  deletePermission(code: string): Promise<S["AuthzAdminSuccessResponse"]> {
    return apiPost(`${BASE}/permission/delete`, { code })
  },
  /** Every permission, for pickers. */
  listAllPermissions(): Promise<PermissionRow[]> {
    return fetchAllPages((page) => authzApi.listPermissionsPage(page))
  },

  // --- Roles ---

  /** One page of roles: server-driven search + sort + pagination. */
  listRolesPage(params: Partial<S["ListRolesRequest"]>): Promise<RolePage> {
    return apiPost(`${BASE}/role/list`, params)
  },
  /** Every role, for pickers. */
  listAllRoles(): Promise<RoleRow[]> {
    return fetchAllPages((page) => authzApi.listRolesPage(page))
  },
  getRole(code: string): Promise<RoleDetailResponse> {
    return apiPost(`${BASE}/role/get`, { code })
  },
  createRole(params: S["CreateRoleRequest"]): Promise<RoleRow> {
    return apiPost(`${BASE}/role/create`, params)
  },
  updateRole(params: S["UpdateRoleRequest"]): Promise<RoleRow> {
    return apiPost(`${BASE}/role/update`, params)
  },
  deleteRole(code: string): Promise<S["AuthzAdminSuccessResponse"]> {
    return apiPost(`${BASE}/role/delete`, { code })
  },

  // --- Permission grants (a role's granted permissions) ---

  createGrant(params: S["CreateRolePermissionRequest"]): Promise<RolePermissionRow> {
    return apiPost(`${BASE}/role-permission/create`, params)
  },
  updateGrant(params: S["UpdateRolePermissionRequest"]): Promise<RolePermissionRow> {
    return apiPost(`${BASE}/role-permission/update`, params)
  },
  deleteGrant(id: number): Promise<S["AuthzAdminSuccessResponse"]> {
    return apiPost(`${BASE}/role-permission/delete`, { id })
  },

  // --- User role assignments ---

  createAssignment(params: S["CreateAssignmentRequest"]): Promise<AssignmentRow> {
    return apiPost(`${BASE}/user-role/create`, params)
  },
  deleteAssignment(id: number): Promise<S["AuthzAdminSuccessResponse"]> {
    return apiPost(`${BASE}/user-role/delete`, { id })
  },

  /**
   * The user's effective permissions and where each applies, computed
   * from their combined role assignments (read-only). Each row is either
   * global or carries a minimal set of org-unit subtree roots.
   */
  async permScopes(usr: number): Promise<PermScopeRow[]> {
    const result = await apiPost<S["UserPermScopesResponse"]>(`${BASE}/user-perm-scopes`, { usr })
    return result.perms
  },
}
