import { test, expect } from '@playwright/test';
import { AdminLoginPage } from '../pages/login.page';
import { AdminHomePage } from '../pages/home.page';
import { ADMIN_USERS } from '../test-users';

/**
 * Negative permission coverage: a user who authenticates but holds no
 * odo-admin permissions must see no tools and be unable to reach any tool
 * route directly. This guards the perm-gating end to end (guard + registry
 * filtering), complementing the backend's own 403s.
 */
test.describe('odo-admin permission gating', () => {
  // Fresh, unauthenticated context — this suite logs in as its own user.
  test.use({ storageState: { cookies: [], origins: [] } });

  test.beforeEach(async ({ page }) => {
    const login = new AdminLoginPage(page);
    await login.goto();
    await login.loginAndWaitForHome(
      ADMIN_USERS.noPerms.username,
      ADMIN_USERS.noPerms.password,
    );
  });

  test('a user without admin perms sees no tools', async ({ page }) => {
    const home = new AdminHomePage(page);
    await home.goto();
    await home.expectLoaded();

    // No tool cards and no tool nav links (only the fixed Home link remains).
    await expect(home.toolCards()).toHaveCount(0);
    await expect(page.locator('.shell-nav-link')).toHaveCount(1);
  });

  test('direct navigation to a tool route is bounced to home', async ({
    page,
  }) => {
    // permGuard redirects an authenticated-but-unauthorized user to '/'.
    await page.goto('/odo/admin/org-units');

    await expect(page).toHaveURL(/\/odo\/admin\/?$/);
    await expect(
      page.getByRole('heading', { name: 'Administration' }),
    ).toBeVisible();
    // The org-units heading must NOT be present — the tool did not render.
    await expect(
      page.getByRole('heading', { name: 'Org Units' }),
    ).toHaveCount(0);
  });
});
