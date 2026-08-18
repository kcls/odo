/**
 * Typed client for the odo-auth user-role assignment endpoints (a user's role
 * grants at org units). Request/response shapes are the generated types from
 * the committed OpenAPI spec (source of truth: the Rust structs), so a backend
 * field change surfaces here as a compile error.
 *
 * The role picker on the detail page reuses the roles tool's listRoles (both
 * live on the same odo-auth authz service) — see user-role-detail.
 */
import { apiPost } from '../../core/api';
import type { components } from '../../core/api-types/odo-auth';

type S = components['schemas'];

export type AssignmentRow = S['AssignmentRow'];
export type PermScopeRow = S['PermScopeRow'];
export type ScopeUnit = S['ScopeUnit'];

const BASE = '/api/v1/odo/auth/authz/user-role';

export const userRoleApi = {
  /**
   * A user's own role assignments — a bounded list (returned under the
   * `assignments` key), not a paginated page.
   */
  async listAssignments(usr: number): Promise<AssignmentRow[]> {
    const result = await apiPost<S['ListAssignmentsResponse']>(`${BASE}/list`, {
      usr,
    });
    return result.assignments;
  },

  /**
   * The user's effective permissions and where each applies, computed from
   * their combined role assignments (read-only). Each row is either global
   * (applies everywhere) or carries a minimal set of org-unit subtree roots.
   */
  async permScopes(usr: number): Promise<PermScopeRow[]> {
    const result = await apiPost<S['UserPermScopesResponse']>(
      '/api/v1/odo/auth/authz/user-perm-scopes',
      { usr },
    );
    return result.perms;
  },

  createAssignment(params: S['CreateAssignmentRequest']): Promise<AssignmentRow> {
    return apiPost(`${BASE}/create`, params);
  },

  deleteAssignment(id: number): Promise<S['AuthzAdminSuccessResponse']> {
    return apiPost(`${BASE}/delete`, { id });
  },
};
