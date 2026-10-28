import { test, expect } from '@playwright/test';
import { expectNoErrors, loginAs } from '../support';

/**
 * The Notifications page's per-day view: a chart by default, the same
 * numbers as a table on request, and the choice kept in the URL.
 */
test.describe('notifications daily view', () => {
  test.use({ storageState: { cookies: [], origins: [] } });

  test.beforeEach(async ({ page }) => {
    await loginAs(page);
  });

  test('switches between chart and table, in the URL', async ({ page }) => {
    await page.goto('/odo/admin/notifications?window=30d');
    await expect(page.getByRole('heading', { name: 'Deliveries per day' })).toBeVisible();

    const view = page.getByRole('radiogroup', { name: 'Show as' });
    await view.getByRole('radio', { name: 'Table' }).click();
    await expect(page).toHaveURL(/daily=table/);
    // Every day of the window is a row, newest first, quiet days included
    // (unless the whole window was quiet, which shows a message instead).
    const table = page.getByRole('table', { name: 'Deliveries per day' });
    const quiet = page.getByText('Nothing was sent in this period.');
    await expect(table.or(quiet)).toBeVisible();
    if (await table.isVisible()) {
      await expect(table.getByRole('row')).toHaveCount(31 + 1);
    }

    await view.getByRole('radio', { name: 'Chart' }).click();
    await expect(page).not.toHaveURL(/daily=table/);
    await expectNoErrors(page);
  });
});
