import { test, expect } from '@playwright/test';
import { AdminLoginPage } from '../pages/login.page';
import { AdminHomePage } from '../pages/home.page';
import { ADMIN_USERS } from '../test-users';

/**
 * Smoke suite for the ODO Admin SPA. Each test logs in fresh as the full
 * admin (odo-auth rotates refresh tokens, so a shared saved session is
 * fragile), then confirms the home tool grid and every tool's list page
 * render. This is the per-tool smoke coverage required from the first tool on.
 */

/** Every registered tool: its nav label + a heading its list page renders. */
const TOOLS = [
  { label: 'Org Units', path: 'org-units', heading: 'Org Units' },
  { label: 'Org Unit Types', path: 'org-unit-types', heading: 'Org Unit Types' },
  { label: 'Email Templates', path: 'templates', heading: /templates/i },
  { label: 'Email Groups', path: 'email-groups', heading: /email groups/i },
  { label: 'Roles', path: 'roles', heading: /roles/i },
  { label: 'Permissions', path: 'permissions', heading: /permissions/i },
  { label: 'User Roles', path: 'user-roles', heading: /user roles/i },
  { label: 'Users', path: 'users', heading: /users/i },
  { label: 'SAML', path: 'saml', heading: /saml|identity provider/i },
] as const;

test.describe('odo-admin smoke', () => {
  // Fresh, unauthenticated context; log in per test.
  test.use({ storageState: { cookies: [], origins: [] } });

  test.beforeEach(async ({ page }) => {
    const login = new AdminLoginPage(page);
    await login.goto();
    await login.loginAndWaitForHome(
      ADMIN_USERS.admin.username,
      ADMIN_USERS.admin.password,
    );
  });

  test('home shows every admin tool', async ({ page }) => {
    const home = new AdminHomePage(page);
    await home.goto();
    await home.expectLoaded();

    // Full admin sees all tools as cards.
    await expect(home.toolCards()).toHaveCount(TOOLS.length);
    for (const tool of TOOLS) {
      await expect(home.toolCard(tool.label)).toBeVisible();
    }
  });

  for (const tool of TOOLS) {
    test(`${tool.label} list loads`, async ({ page }) => {
      const home = new AdminHomePage(page);
      await home.goto();
      await home.expectLoaded();

      await home.navLink(tool.label).click();

      // Landed on the tool's route.
      await expect(page).toHaveURL(new RegExp(`/odo/admin/${tool.path}`));

      // The list page rendered its heading (proves the lazy chunk loaded and
      // the tool's first data call resolved without a hard error).
      await expect(
        page.getByRole('heading', { name: tool.heading }).first(),
      ).toBeVisible();

      // No unhandled error surfaced to the global error snackbar.
      await expect(page.locator('.app-error-snackbar')).toHaveCount(0);
    });
  }
});
