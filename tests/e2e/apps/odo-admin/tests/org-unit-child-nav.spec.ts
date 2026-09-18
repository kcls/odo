import { test, expect } from '@playwright/test';
import { AdminLoginPage } from '../pages/login.page';
import { ADMIN_USERS } from '../test-users';

/**
 * Regression guard: navigating from an org unit to one of its child units
 * reuses the OrgUnitDetail component (same route, different :id). The page must
 * reload for the new id — an effect on the id signal, not ngOnInit (which fires
 * once). We assert both the URL and the heading change.
 */
test.describe('org-unit child navigation', () => {
  test.use({ storageState: { cookies: [], origins: [] } });

  test.beforeEach(async ({ page }) => {
    const login = new AdminLoginPage(page);
    await login.goto();
    await login.loginAndWaitForHome(
      ADMIN_USERS.admin.username,
      ADMIN_USERS.admin.password,
    );
  });

  test('clicking a child unit routes and reloads the detail page', async ({
    page,
  }) => {
    // Reach the root's detail page via the org-units list (ids are not
    // fixed), then navigate to a child.
    await page.goto('/odo/admin/org-units');
    await page.locator('tr.odo-row', { hasText: 'Odo Library System' }).first().click();
    const heading = page.getByRole('heading', { level: 1 }).first();
    await expect(heading).toHaveText('Odo Library System');

    // Click a known child row (platform seed) in the "Child units" table.
    const childRow = page.locator('tr.odo-row', { hasText: 'East Region' }).first();
    await childRow.click();

    await expect(page).toHaveURL(/\/org-units\/\d+$/);
    // The heading must update to the child — proves the page reloaded, not just
    // the URL changing while the stale parent view remained.
    await expect(heading).toHaveText('East Region');
  });
});
