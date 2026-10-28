import { Page, Locator, expect } from '@playwright/test';
import { AdminBasePage } from './base.page';

/**
 * Home (landing) page object: the tool cards and the sidebar.
 *
 * Both are driven by the same permission-filtered tool registry, so tests
 * can assert on either.
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

  /** Home tool cards: links in the page's list of tools. */
  toolCards(): Locator {
    return this.page.getByRole('main').getByRole('listitem');
  }

  /**
   * A tool's card. Its link's name is the label then the description, and
   * labels overlap ("Org units" / "Org unit types"), so anchor at the start
   * and require the label to end there.
   */
  toolCard(label: string): Locator {
    return this.page
      .getByRole('main')
      .getByRole('link', { name: new RegExp(`^${escapeRegExp(label)}(?![a-z])`, 'i') });
  }

  /** The sidebar's links (Home plus one per visible tool). */
  navLinks(): Locator {
    return this.page.getByRole('list', { name: 'Main' }).getByRole('link');
  }

  navLink(label: string): Locator {
    return this.page.getByRole('list', { name: 'Main' }).getByRole('link', { name: label, exact: true });
  }

  async expectLoaded(): Promise<void> {
    await expect(this.heading()).toBeVisible();
  }
}

function escapeRegExp(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}
