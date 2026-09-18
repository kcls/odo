/**
 * E2E test users for the ODO Admin SPA.
 *
 * Created by sqitch test data:
 * - e2e.odo.admin (id 205): holds the odo-admin role at root — every admin
 *   read/write perm. The positive smoke subject.
 * - e2e.odo.staff (id 200): a login-only user with NO odo-admin perms.
 *   Authenticates fine but sees an empty tool grid — the negative-perm subject.
 *
 * Local users have password 'test123!'. Deploy with
 * `./scripts/manage-database.sh deploy-test`.
 */
export interface AdminTestUser {
  username: string;
  password: string;
  displayName: string;
}

export const ADMIN_USERS = {
  /** Full admin — every tool visible. */
  admin: {
    username: 'e2e.odo.admin',
    password: 'test123!',
    displayName: 'E2E Odo Admin',
  } as AdminTestUser,

  /** Authenticated but holds no admin perms — sees no tools. */
  noPerms: {
    username: 'e2e.odo.staff',
    password: 'test123!',
    displayName: 'E2E Staff',
  } as AdminTestUser,
} as const;
