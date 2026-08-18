/**
 * Typed client for the odo-auth authz admin endpoints (permissions, roles, and
 * their permission grants). Request/response shapes are the generated types
 * from the committed OpenAPI spec (source of truth: the Rust structs), so a
 * backend field change surfaces here as a compile error.
 *
 * The permissions and roles admin tools both live on this one service and share
 * the `odo.auth.role.*` read/write perm pair, so they share this single client:
 * the roles tool owns it (it is the richer subject) and the permissions tool
 * imports what it needs.
 */
import { apiPost } from '../../core/api';
import type { components } from '../../core/api-types/odo-auth';

type S = components['schemas'];

export type PermissionRow = S['PermissionRow'];
export type PermissionPage = S['PermissionPage'];
export type RoleRow = S['RoleRow'];
export type RolePage = S['RolePage'];
export type RoleDetailResponse = S['RoleDetailResponse'];
export type GrantRow = S['GrantRow'];
export type RolePermissionRow = S['RolePermissionRow'];

const BASE = '/api/v1/odo/auth/authz';

export const authzAdminApi = {
  // --- Permissions ---

  /** All permissions matching `search` (unpaginated; for pickers). */
  async listPermissions(search?: string): Promise<PermissionRow[]> {
    const result = await apiPost<PermissionPage>(`${BASE}/permission/list`, {
      search: search || undefined,
    });
    return result.rows;
  },

  /** One page of permissions: server-driven search + sort + pagination. */
  listPermissionsPage(
    params: Partial<S['ListPermissionsRequest']>,
  ): Promise<PermissionPage> {
    return apiPost<PermissionPage>(`${BASE}/permission/list`, params);
  },

  createPermission(params: S['CreatePermissionRequest']): Promise<PermissionRow> {
    return apiPost(`${BASE}/permission/create`, params);
  },
  updatePermission(params: S['UpdatePermissionRequest']): Promise<PermissionRow> {
    return apiPost(`${BASE}/permission/update`, params);
  },
  deletePermission(code: string): Promise<S['AuthzAdminSuccessResponse']> {
    return apiPost(`${BASE}/permission/delete`, { code });
  },

  // --- Roles ---

  /** All roles matching `search` (unpaginated; for pickers). */
  async listRoles(search?: string): Promise<RoleRow[]> {
    const result = await apiPost<RolePage>(`${BASE}/role/list`, {
      search: search || undefined,
    });
    return result.rows;
  },

  /** One page of roles: server-driven search + sort + pagination. */
  listRolesPage(params: Partial<S['ListRolesRequest']>): Promise<RolePage> {
    return apiPost<RolePage>(`${BASE}/role/list`, params);
  },

  getRole(code: string): Promise<RoleDetailResponse> {
    return apiPost(`${BASE}/role/get`, { code });
  },
  createRole(params: S['CreateRoleRequest']): Promise<RoleRow> {
    return apiPost(`${BASE}/role/create`, params);
  },
  updateRole(params: S['UpdateRoleRequest']): Promise<RoleRow> {
    return apiPost(`${BASE}/role/update`, params);
  },
  deleteRole(code: string): Promise<S['AuthzAdminSuccessResponse']> {
    return apiPost(`${BASE}/role/delete`, { code });
  },

  // --- Permission grants (a role's granted permissions) ---

  createGrant(params: S['CreateRolePermissionRequest']): Promise<RolePermissionRow> {
    return apiPost(`${BASE}/role-permission/create`, params);
  },
  updateGrant(params: S['UpdateRolePermissionRequest']): Promise<RolePermissionRow> {
    return apiPost(`${BASE}/role-permission/update`, params);
  },
  deleteGrant(id: number): Promise<S['AuthzAdminSuccessResponse']> {
    return apiPost(`${BASE}/role-permission/delete`, { id });
  },
};
