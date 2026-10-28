import { test, expect, Page } from '@playwright/test';
import { loginAs } from '../support';

/**
 * Server-driven list state on the permissions list, the reference list:
 * sorting goes to the API, and search/sort/page live in the URL, so a
 * list view deep-links and survives back/forward.
 */

const codeHeader = (page: Page) => page.getByRole('columnheader', { name: /^Code/ });
const firstCode = async (page: Page) =>
  (await page.getByRole('table', { name: 'Permissions' }).getByRole('row').nth(1).getByRole('cell').first().innerText()).trim();

test.describe('permissions list state', () => {
  test.use({ storageState: { cookies: [], origins: [] } });

  test.beforeEach(async ({ page }) => {
    await loginAs(page);
  });

  test('a sortable header re-queries the API and flips direction', async ({ page }) => {
    await page.goto('/odo/admin/permissions');
    await expect(codeHeader(page)).toHaveAttribute('aria-sort', 'ascending');
    const ascFirst = await firstCode(page);

    const sorted = page.waitForRequest(
      (req) => req.url().endsWith('/permission/list') && req.postDataJSON()?.sort_dir === 'desc',
    );
    await codeHeader(page).getByRole('button').click();
    await sorted;

    await expect(page).toHaveURL(/sort=code&dir=desc/);
    await expect(codeHeader(page)).toHaveAttribute('aria-sort', 'descending');
    await expect.poll(() => firstCode(page)).not.toBe(ascFirst);
  });

  test('computed columns are not sortable', async ({ page }) => {
    await page.goto('/odo/admin/permissions');
    const roles = page.getByRole('columnheader', { name: 'Roles' });
    await expect(roles).toBeVisible();
    await expect(roles.getByRole('button')).toHaveCount(0);
  });

  test('a deep link restores search and sort, and back/forward follow', async ({ page }) => {
    await page.goto('/odo/admin/permissions?q=odo.auth&sort=code&dir=desc');

    await expect(page.getByRole('searchbox', { name: 'Filter permissions' })).toHaveValue('odo.auth');
    await expect(codeHeader(page)).toHaveAttribute('aria-sort', 'descending');
    const codes = page.getByRole('table', { name: 'Permissions' }).getByRole('row');
    await expect(codes.nth(1)).toContainText('odo.auth');

    // A new sort is a new history entry; back returns to the deep link.
    await codeHeader(page).getByRole('button').click();
    await expect(codeHeader(page)).toHaveAttribute('aria-sort', 'ascending');
    await page.goBack();
    await expect(page).toHaveURL(/dir=desc/);
    await expect(codeHeader(page)).toHaveAttribute('aria-sort', 'descending');
    await page.goForward();
    await expect(codeHeader(page)).toHaveAttribute('aria-sort', 'ascending');
  });
});
