import type { RoleAssignment } from '../types';

export function hasAnyRole(
  userRoles: RoleAssignment[] | undefined,
  roles: readonly string[]
): boolean {
  if (!userRoles) return false;
  return userRoles.some(r => roles.includes(r.role));
}

export function hasRole(
  userRoles: RoleAssignment[] | undefined,
  role: string
): boolean {
  if (!userRoles) return false;
  return userRoles.some(r => r.role === role);
}
