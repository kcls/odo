import { Page } from '@playwright/test';

/**
 * Base page object for the ODO Admin SPA.
 *
 * The admin app is served under the `/odo/admin` base href, so every
 * navigation is relative to that prefix. Page objects extend this and use
 * `gotoPath('/org-units')` etc.
 */
export class AdminBasePage {
  /** The SPA base path (matches the Angular baseHref + nginx alias). */
  static readonly BASE = '/odo/admin';

  constructor(protected readonly page: Page) {}

  /** Navigate to an in-app route (path is relative to the admin base). */
  protected async gotoPath(path: string): Promise<void> {
    const clean = path.replace(/^\//, '');
    await this.page.goto(`${AdminBasePage.BASE}/${clean}`);
  }
}
