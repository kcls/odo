import { test, expect } from '@playwright/test';
import { loginAs } from '../support';

/**
 * The template editor previews the form as typed, and asks before
 * leaving with unsaved edits. Nothing here is saved.
 */
test.describe('template editor', () => {
  test.use({ storageState: { cookies: [], origins: [] } });

  test.beforeEach(async ({ page }) => {
    await loginAs(page);
    await page.goto('/odo/admin/templates/new');
    await expect(page.getByRole('heading', { name: 'New template', level: 1 })).toBeVisible();
  });

  test('previews the subject with the sample data, as text', async ({ page }) => {
    await page.getByLabel('Subject template').fill('Hello {{name}}');
    await page.getByLabel('Sample data').fill('{"name": "O\'Brien & Sons"}');

    const preview = page.getByRole('heading', { name: 'Preview' }).locator('xpath=ancestor::*[@data-slot="card"]');
    // Rendered as text: no HTML entities in the subject.
    await expect(preview.getByText("Hello O'Brien & Sons", { exact: true })).toBeVisible();
  });

  test('flags invalid sample data', async ({ page }) => {
    await page.getByLabel('Sample data').fill('{not json');
    await expect(page.getByText('Sample data must be valid JSON.').first()).toBeVisible();
  });

  test('asks before leaving with unsaved edits', async ({ page }) => {
    await page.getByLabel('Name').fill('Unsaved draft');
    const nav = page.getByRole('list', { name: 'Main' });

    await nav.getByRole('link', { name: 'Roles', exact: true }).click();
    const confirm = page.getByRole('alertdialog', { name: 'Discard unsaved changes?' });
    await expect(confirm).toBeVisible();
    await confirm.getByRole('button', { name: 'Keep editing' }).click();
    await expect(page).toHaveURL(/\/templates\/new$/);
    await expect(page.getByLabel('Name')).toHaveValue('Unsaved draft');

    await nav.getByRole('link', { name: 'Roles', exact: true }).click();
    await page.getByRole('alertdialog').getByRole('button', { name: 'Discard changes' }).click();
    await expect(page).toHaveURL(/\/roles$/);
  });
});
