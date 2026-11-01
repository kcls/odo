import { test, expect } from '@playwright/test';
import { apiToken, loginAs } from '../support';
import { ADMIN_USERS } from '../test-users';

/**
 * A user's page: its tabs, effective permissions, and assigning and
 * removing roles in place (there is no separate User Roles tool). Ids
 * aren't fixed, so users are found through the search.
 */

const ROLE = { code: 'e2e-ui-role', label: 'E2E UI role' };

test.describe('user page', () => {
  test.use({ storageState: { cookies: [], origins: [] } });

  test.beforeAll(async ({ request }) => {
    // A role of our own to assign, granting nothing, so assigning it can't
    // change what another test's user can see.
    const token = await apiToken(request);
    const resp = await request.post('/api/v1/odo/auth/authz/role/create', {
      headers: { Authorization: `Bearer ${token}` },
      data: { code: ROLE.code, label: ROLE.label },
    });
    expect([200, 409]).toContain(resp.status());
  });

  test.beforeEach(async ({ page }) => {
    await loginAs(page);
  });

  test('shows effective permissions with their scope on their own tab', async ({ page }) => {
    await page.goto(`/odo/admin/users?q=${ADMIN_USERS.admin.username}`);
    await page.getByRole('table', { name: 'Users' }).getByRole('link').first().click();

    const tabs = page.getByRole('tablist', { name: 'User details' });
    await expect(tabs.getByRole('tab', { name: 'Overview' })).toHaveAttribute('aria-selected', 'true');
    await tabs.getByRole('tab', { name: 'Permissions' }).click();
    await expect(page).toHaveURL(/[?&]tab=permissions/);
    await expect(page.getByRole('heading', { name: 'Effective permissions' })).toBeVisible();
    const perms = page.getByRole('table', { name: 'Effective permissions' });
    await expect(perms.getByText('odo.auth.role.read', { exact: true })).toBeVisible();
    // odo-admin is held at the root, so its grants apply everywhere.
    await expect(perms.getByText('All org units').first()).toBeVisible();

    // The tab is in the URL: back returns to the overview, and a reload
    // keeps the tab.
    await page.goBack();
    await expect(tabs.getByRole('tab', { name: 'Overview' })).toHaveAttribute('aria-selected', 'true');
    await expect(page.getByRole('table', { name: 'Roles' })).toBeVisible();
    await page.goForward();
    await page.reload();
    await expect(perms.getByText('odo.auth.role.read', { exact: true })).toBeVisible();
  });

  test('assigns and removes a role', async ({ page }) => {
    await page.goto(`/odo/admin/users?q=${ADMIN_USERS.noPerms.username}`);
    await page.getByRole('table', { name: 'Users' }).getByRole('link').first().click();
    const roles = page.getByRole('table', { name: 'Roles' });
    await expect(roles).toBeVisible();
    const assigned = roles.getByRole('row', { name: new RegExp(ROLE.label) });
    await expect(assigned).toHaveCount(0);

    await page.getByRole('button', { name: 'Assign role' }).click();
    const dialog = page.getByRole('dialog', { name: 'Assign role' });
    await dialog.getByRole('combobox', { name: 'Role' }).click();
    await page.getByPlaceholder('Search roles').fill(ROLE.code);
    await page.getByRole('option', { name: new RegExp(ROLE.label) }).click();
    await dialog.getByRole('combobox', { name: 'Org unit' }).click();
    await page.getByPlaceholder('Search org units').fill('Odo Library System');
    await page.getByRole('option', { name: /Odo Library System/ }).first().click();
    await dialog.getByRole('button', { name: 'Assign', exact: true }).click();

    await expect(assigned).toHaveCount(1);

    await assigned.getByRole('button', { name: /^Remove / }).click();
    await page.getByRole('alertdialog').getByRole('button', { name: 'Remove' }).click();
    await expect(assigned).toHaveCount(0);
  });
});
