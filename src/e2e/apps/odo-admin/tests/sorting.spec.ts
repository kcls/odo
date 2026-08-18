import { test, expect } from '@playwright/test';
import { AdminLoginPage } from '../pages/login.page';
import { ADMIN_USERS } from '../test-users';

/**
 * Server-driven sorting on the permissions list (the pilot for API sorting).
 * Confirms a sortable header toggles direction, sends the sort to the API, and
 * that a computed column (Roles) is not sortable.
 */
test.describe('permissions sorting', () => {
  test.use({ storageState: { cookies: [], origins: [] } });

  test.beforeEach(async ({ page }) => {
    const login = new AdminLoginPage(page);
    await login.goto();
    await login.loginAndWaitForHome(
      ADMIN_USERS.admin.username,
      ADMIN_USERS.admin.password,
    );
    await page.goto('/odo/admin/permissions');
    await page.locator('td.cdk-cell').first().waitFor();
  });

  test('clicking a header toggles sort and re-queries the API', async ({
    page,
  }) => {
    const firstCode = () => page.locator('td.cell-code').first().innerText();
    const ascFirst = (await firstCode()).trim();

    const codeHeader = page.locator('odo-sort-header', { hasText: 'Code' });
    const [req] = await Promise.all([
      page.waitForRequest(
        (r) => r.url().includes('/permission/list') && r.method() === 'POST',
      ),
      codeHeader.click(),
    ]);

    // The click sent an explicit descending sort to the server.
    expect(req.postDataJSON()).toMatchObject({
      sort_by: 'code',
      sort_dir: 'desc',
    });

    // The header now shows the descending affordance and the order flipped.
    await expect(codeHeader.locator('mat-icon')).toHaveText(/arrow_downward/);
    await expect
      .poll(async () => (await firstCode()).trim())
      .not.toBe(ascFirst);
  });

  test('computed columns are not sortable', async ({ page }) => {
    // "Roles" is a post-query count, so it has no sort header.
    await expect(
      page.locator('odo-sort-header', { hasText: 'Roles' }),
    ).toHaveCount(0);
  });
});

/**
 * Roles list also has clickable rows (drill into a role). Sorting via a header
 * must re-query without navigating away — a regression guard for header vs
 * row-click interaction.
 */
test.describe('roles sorting (clickable rows)', () => {
  test.use({ storageState: { cookies: [], origins: [] } });

  test.beforeEach(async ({ page }) => {
    const login = new AdminLoginPage(page);
    await login.goto();
    await login.loginAndWaitForHome(
      ADMIN_USERS.admin.username,
      ADMIN_USERS.admin.password,
    );
    await page.goto('/odo/admin/roles');
    await page.locator('td.cdk-cell').first().waitFor();
  });

  test('sorting by Label re-queries and stays on the list', async ({ page }) => {
    const labelHeader = page.locator('odo-sort-header', { hasText: 'Label' });
    const [req] = await Promise.all([
      page.waitForRequest(
        (r) => r.url().includes('/role/list') && r.method() === 'POST',
      ),
      labelHeader.click(),
    ]);
    expect(req.postDataJSON()).toMatchObject({
      sort_by: 'label',
      sort_dir: 'asc',
    });
    // Clicking the header did NOT navigate into a role detail page.
    await expect(page).toHaveURL(/\/odo\/admin\/roles\/?$/);
  });
});
