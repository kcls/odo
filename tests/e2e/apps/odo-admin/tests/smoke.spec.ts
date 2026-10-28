import { test, expect } from '@playwright/test';
import { AdminHomePage } from '../pages/home.page';
import { expectNoErrors, loginAs } from '../support';

/**
 * Smoke suite for the ODO Admin SPA. Each test logs in fresh as the full
 * admin (odo-auth rotates refresh tokens, so a shared saved session is
 * fragile), then confirms the home page and every tool's page render.
 */

/** Every registered tool: its nav label, route, and page heading. */
const TOOLS = [
  { label: 'Users', path: 'users', heading: 'Users' },
  { label: 'Roles', path: 'roles', heading: 'Roles' },
  { label: 'Permissions', path: 'permissions', heading: 'Permissions' },
  { label: 'Org units', path: 'org-units', heading: 'Org units' },
  { label: 'Org unit types', path: 'org-unit-types', heading: 'Org unit types' },
  { label: 'Email templates', path: 'templates', heading: 'Email templates' },
  { label: 'Email groups', path: 'email-groups', heading: 'Email groups' },
  { label: 'Notifications', path: 'notifications', heading: 'Notifications' },
  { label: 'SAML', path: 'saml', heading: 'SAML' },
] as const;

test.describe('odo-admin smoke', () => {
  test.use({ storageState: { cookies: [], origins: [] } });

  test.beforeEach(async ({ page }) => {
    await loginAs(page);
  });

  test('home shows every admin tool', async ({ page }) => {
    const home = new AdminHomePage(page);
    await home.goto();
    await home.expectLoaded();

    await expect(home.toolCards()).toHaveCount(TOOLS.length);
    for (const tool of TOOLS) {
      await expect(home.toolCard(tool.label)).toBeVisible();
    }
    // The sidebar lists the same tools, plus Home.
    await expect(home.navLinks()).toHaveCount(TOOLS.length + 1);
  });

  for (const tool of TOOLS) {
    test(`${tool.label} page loads`, async ({ page }) => {
      const home = new AdminHomePage(page);
      await home.goto();
      await home.expectLoaded();

      await home.navLink(tool.label).click();

      await expect(page).toHaveURL(new RegExp(`/odo/admin/${tool.path}(\\?|$)`));
      // The page's heading renders once its lazy chunk and loader finish.
      await expect(page.getByRole('heading', { name: tool.heading, level: 1 })).toBeVisible();
      await expectNoErrors(page);
    });
  }
});
