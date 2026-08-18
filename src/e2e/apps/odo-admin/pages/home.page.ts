import { Page, Locator, expect } from '@playwright/test';
import { AdminBasePage } from './base.page';

/**
 * Home (landing) page object: the tool grid + the shell nav.
 *
 * Both the grid cards and the sidenav links are driven by the same
 * permission-filtered tool list, so tests can assert on either.
 */
export class AdminHomePage extends AdminBasePage {
  constructor(page: Page) {
    super(page);
  }

  async goto(): Promise<void> {
    await this.gotoPath('/');
  }

  heading(): Locator {
    return this.page.getByRole('heading', { name: 'Administration' });
  }

  /** Home-grid tool cards. */
  toolCards(): Locator {
    return this.page.locator('.home-card');
  }

  /**
   * A specific tool's card, matched by its exact title (labels overlap as
   * substrings — "Roles" vs "User Roles" — so match the title element exactly).
   */
  toolCard(label: string): Locator {
    return this.page
      .locator('.home-card')
      .filter({ has: this.page.getByText(label, { exact: true }) });
  }

  /** Sidenav link by exact accessible name (excludes the fixed Home link). */
  navLink(label: string): Locator {
    return this.page.getByRole('link', { name: label, exact: true });
  }

  async expectLoaded(): Promise<void> {
    await expect(this.heading()).toBeVisible();
  }
}
