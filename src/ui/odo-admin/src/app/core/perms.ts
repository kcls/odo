/**
 * Permission codes the admin UI gates on. These mirror the codes seeded by
 * sqitch change 090_odo_admin_role (renamed by 097_permission_namespaces). Each admin resource has a read/write
 * pair; the UI gates tool visibility/routes on the read perm and edit
 * affordances on the write perm.
 */
export const PERMS = {
  EMAIL_GROUP_READ: 'odo.notify.email_group.read',
  EMAIL_GROUP_WRITE: 'odo.notify.email_group.write',
  TEMPLATE_READ: 'odo.notify.template.read',
  TEMPLATE_WRITE: 'odo.notify.template.write',
  ROLE_READ: 'odo.auth.role.read',
  ROLE_WRITE: 'odo.auth.role.write',
  USER_ROLE_READ: 'odo.auth.user_role.read',
  USER_ROLE_WRITE: 'odo.auth.user_role.write',
  SAML_READ: 'odo.auth.saml.read',
  SAML_WRITE: 'odo.auth.saml.write',
  USER_DETAIL_READ: 'odo.auth.user.detail.read',
  USER_WRITE: 'odo.auth.user.write',
  ORG_UNIT_READ: 'odo.org.unit.read',
  ORG_UNIT_WRITE: 'odo.org.unit.write',
} as const;

export type PermCode = (typeof PERMS)[keyof typeof PERMS];
