import { test, expect } from '@playwright/test';
import { AdminHomePage } from '../pages/home.page';
import { loginAs } from '../support';
import { ADMIN_USERS } from '../test-users';

/**
 * A user who authenticates but holds no odo-admin permissions sees no
 * tools and can't reach a tool route directly. Guards the perm gating end
 * to end (route middleware + registry filtering), alongside the backend's
 * own 403s.
 */
test.describe('odo-admin permission gating', () => {
  test.use({ storageState: { cookies: [], origins: [] } });

  test.beforeEach(async ({ page }) => {
    await loginAs(page, ADMIN_USERS.noPerms);
  });

  test('a user without admin perms sees no tools', async ({ page }) => {
    const home = new AdminHomePage(page);
    await home.goto();
    await home.expectLoaded();

    await expect(page.getByText('Your account has no admin tools.')).toBeVisible();
    await expect(home.toolCards()).toHaveCount(0);
    // Only the fixed Home link remains in the sidebar.
    await expect(home.navLinks()).toHaveCount(1);
  });

  test('direct navigation to a tool route lands on home', async ({ page }) => {
    await page.goto('/odo/admin/org-units');

    await expect(page).toHaveURL(/\/odo\/admin\/?$/);
    await expect(page.getByRole('heading', { name: 'Administration' })).toBeVisible();
    await expect(page.getByRole('heading', { name: 'Org units' })).toHaveCount(0);
  });
});
