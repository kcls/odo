import { Routes } from '@angular/router';

import { AuthService } from './auth.service';
import { PERMS, type PermCode } from './perms';

/**
 * Registry of admin tools. Adding a tool here wires up its lazy route, its
 * sidenav entry, and its home-page card in one place — the only other thing
 * a new tool needs is its feature folder. See docs/adding-a-tool.md.
 */
export interface AdminTool {
  /** Top-level route path segment (e.g. 'org-units'). */
  path: string;
  /** Material icon name for the nav entry + home card. */
  icon: string;
  label: string;
  description: string;
  /** Read perm gating the route, nav entry, and home card. */
  readPerm: PermCode;
  loadChildren: () => Promise<Routes>;
}

/**
 * The tools, in no particular order (the nav and home page sort by label).
 * Entries are appended as each tool's feature folder is added.
 */
export const ADMIN_TOOLS: AdminTool[] = [
  {
    path: 'org-units',
    icon: 'account_tree',
    label: $localize`:Admin tool name:Org Units`,
    description: $localize`:Admin tool description:Manage the organizational tree and each unit's addresses, closures, and hours.`,
    readPerm: PERMS.ORG_UNIT_READ,
    loadChildren: () =>
      import('../features/org-units/org-units.routes').then(
        (m) => m.ORG_UNIT_ROUTES,
      ),
  },
  {
    path: 'org-unit-types',
    icon: 'category',
    label: $localize`:Admin tool name:Org Unit Types`,
    description: $localize`:Admin tool description:Manage the categories of org units (system, region, branch, …) and what each may contain.`,
    readPerm: PERMS.ORG_UNIT_READ,
    loadChildren: () =>
      import('../features/org-unit-types/org-unit-types.routes').then(
        (m) => m.ORG_UNIT_TYPE_ROUTES,
      ),
  },
  {
    path: 'templates',
    icon: 'drafts',
    label: $localize`:Admin tool name:Email Templates`,
    description: $localize`:Admin tool description:Edit and preview notification templates for email and in-app messages.`,
    readPerm: PERMS.TEMPLATE_READ,
    loadChildren: () =>
      import('../features/templates/templates.routes').then(
        (m) => m.TEMPLATE_ROUTES,
      ),
  },
  {
    path: 'email-groups',
    icon: 'group',
    label: $localize`:Admin tool name:Email Groups`,
    description: $localize`:Admin tool description:Manage notification email groups and their member addresses.`,
    readPerm: PERMS.EMAIL_GROUP_READ,
    loadChildren: () =>
      import('../features/email-groups/email-groups.routes').then(
        (m) => m.EMAIL_GROUP_ROUTES,
      ),
  },
  {
    path: 'roles',
    icon: 'verified_user',
    label: $localize`:Admin tool name:Roles`,
    description: $localize`:Admin tool description:Manage roles and the permissions granted to them.`,
    readPerm: PERMS.ROLE_READ,
    loadChildren: () =>
      import('../features/roles/roles.routes').then((m) => m.ROLE_ROUTES),
  },
  {
    path: 'permissions',
    icon: 'key',
    label: $localize`:Admin tool name:Permissions`,
    description: $localize`:Admin tool description:Manage the permission codes available for role grants.`,
    readPerm: PERMS.ROLE_READ,
    loadChildren: () =>
      import('../features/permissions/permissions.routes').then(
        (m) => m.PERMISSION_ROUTES,
      ),
  },
  {
    path: 'user-roles',
    icon: 'manage_accounts',
    label: $localize`:Admin tool name:User Roles`,
    description: $localize`:Admin tool description:Assign roles to users at org units.`,
    readPerm: PERMS.USER_ROLE_READ,
    loadChildren: () =>
      import('../features/user-roles/user-roles.routes').then(
        (m) => m.USER_ROLE_ROUTES,
      ),
  },
  {
    path: 'users',
    icon: 'person_search',
    label: $localize`:Admin tool name:Users`,
    description: $localize`:Admin tool description:View user accounts, roles, SAML identities, and recent sessions.`,
    readPerm: PERMS.USER_DETAIL_READ,
    loadChildren: () =>
      import('../features/users/users.routes').then((m) => m.USER_ROUTES),
  },
  {
    path: 'saml',
    icon: 'security',
    label: $localize`:Admin tool name:SAML`,
    description: $localize`:Admin tool description:Configure SAML identity and service providers for single sign-on.`,
    readPerm: PERMS.SAML_READ,
    loadChildren: () =>
      import('../features/saml/saml.routes').then((m) => m.SAML_ROUTES),
  },
];

/** The subset of ADMIN_TOOLS the current user may see, sorted by label. */
export async function visibleAdminTools(auth: AuthService): Promise<AdminTool[]> {
  const checks = await Promise.all(
    ADMIN_TOOLS.map((tool) => auth.hasPerm(tool.readPerm)),
  );
  return ADMIN_TOOLS.filter((_, i) => checks[i]).sort((a, b) =>
    a.label.localeCompare(b.label),
  );
}
