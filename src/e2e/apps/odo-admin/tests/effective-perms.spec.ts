import { test, expect } from '@playwright/test';
import { AdminLoginPage } from '../pages/login.page';
import { ADMIN_USERS } from '../test-users';

/**
 * The read-only "Effective Permissions" section on a user's role-detail page:
 * the permissions a user holds via their combined role assignments, and where
 * each applies. The odo-admin e2e user holds every admin permission globally
 * (assigned at root), so each row shows the "All org units" scope. The user's
 * id is not fixed: navigate to the detail page via the User Roles search.
 */
test.describe('user-roles effective permissions', () => {
  test.use({ storageState: { cookies: [], origins: [] } });

  test.beforeEach(async ({ page }) => {
    const login = new AdminLoginPage(page);
    await login.goto();
    await login.loginAndWaitForHome(
      ADMIN_USERS.admin.username,
      ADMIN_USERS.admin.password,
    );
    // Find the odo-admin e2e user via the User Roles search (ids are not
    // fixed) and open their detail page.
    await page.goto('/odo/admin/user-roles');
    await page.getByRole('textbox').first().fill(ADMIN_USERS.admin.username);
    await page.getByRole('button', { name: 'Search' }).click();
    const row = page.locator('tr.odo-row', { hasText: ADMIN_USERS.admin.username }).first();
    await row.click();
    await expect(
      page.getByRole('heading', { name: 'Effective Permissions' }),
    ).toBeVisible();
  });

  test('renders effective permissions with scope', async ({ page }) => {
    await expect(
      page.getByRole('heading', { name: 'Effective Permissions' }),
    ).toBeVisible();

    // The effective-permissions table is the second odo-table (after the
    // role-assignments table).
    const permTable = page.locator('odo-table').nth(1);
    await permTable.locator('tr.odo-row').first().waitFor();
    const rowCount = await permTable.locator('tr.odo-row').count();
    expect(rowCount).toBeGreaterThan(0);

    // A known admin permission is listed with a global scope.
    await expect(permTable.getByText('odo.auth.role.read', { exact: true })).toBeVisible();
    await expect(
      permTable.getByText('All org units', { exact: true }).first(),
    ).toBeVisible();
  });
});
