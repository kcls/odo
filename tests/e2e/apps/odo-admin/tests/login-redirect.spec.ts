import { test, expect } from '@playwright/test';
import { AdminLoginPage } from '../pages/login.page';
import { ADMIN_USERS } from '../test-users';

/**
 * A deep link opened while signed out goes to login and, after signing
 * in, back to the original page with its query intact.
 */
test.describe('login redirect', () => {
  test.use({ storageState: { cookies: [], origins: [] } });

  test('returns to the requested page after signing in', async ({ page }) => {
    await page.goto('/odo/admin/roles?q=odo');
    await expect(page).toHaveURL(/\/odo\/admin\/login\?redirect_to=/);

    await new AdminLoginPage(page).login(ADMIN_USERS.admin.username, ADMIN_USERS.admin.password);

    await expect(page).toHaveURL(/\/odo\/admin\/roles\?q=odo$/);
    await expect(page.getByRole('searchbox', { name: 'Filter roles' })).toHaveValue('odo');
  });

  test('a wrong password shows an error and stays on login', async ({ page }) => {
    const login = new AdminLoginPage(page);
    await login.goto();
    await login.login(ADMIN_USERS.admin.username, 'not-the-password');
    await login.expectLoginError();
    await expect(page).toHaveURL(/\/login/);
  });
});
