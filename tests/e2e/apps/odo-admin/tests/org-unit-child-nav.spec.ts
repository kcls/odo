import { test, expect } from '@playwright/test';
import { loginAs } from '../support';

/**
 * Navigating from an org unit to one of its children stays on the same
 * route with a new :unitId; the page must load the child, not keep showing
 * the parent.
 */
test.describe('org unit child navigation', () => {
  test.use({ storageState: { cookies: [], origins: [] } });

  test.beforeEach(async ({ page }) => {
    await loginAs(page);
  });

  test('clicking a child unit loads its page', async ({ page }) => {
    await page.goto('/odo/admin/org-units');
    await page.getByRole('main').getByRole('link', { name: 'Odo Library System', exact: true }).click();
    await expect(page.getByRole('heading', { level: 1 })).toHaveText('Odo Library System');
    const parentUrl = page.url();

    const children = page.getByRole('table', { name: 'Child units' });
    await children.getByRole('link', { name: 'East Region', exact: true }).click();

    await expect(page.getByRole('heading', { level: 1 })).toHaveText('East Region');
    expect(page.url()).not.toBe(parentUrl);
  });
});
