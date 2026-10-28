import { Page, expect } from '@playwright/test';
import { AdminBasePage } from './base.page';

/**
 * Login page object for the ODO Admin SPA.
 *
 * When SSO providers are configured the local username/password form is
 * collapsed behind a "Sign in with username and password" button, so
 * `login()` reveals it first if present.
 */
export class AdminLoginPage extends AdminBasePage {
  constructor(page: Page) {
    super(page);
  }

  async goto(): Promise<void> {
    await this.gotoPath('/login');
  }

  /** Fill and submit the local login form. */
  async login(username: string, password: string): Promise<void> {
    await this.page.getByRole('heading', { name: 'Odo Admin', level: 1 }).waitFor();

    const usernameField = this.page.getByLabel('Username');
    if (!(await usernameField.isVisible().catch(() => false))) {
      await this.page.getByRole('button', { name: /username and password/i }).click();
    }
    await usernameField.fill(username);
    await this.page.getByLabel('Password').fill(password);
    // Exact: SSO buttons are named "Sign in with <provider>".
    await this.page.getByRole('button', { name: 'Sign in', exact: true }).click();
  }

  /** Log in and wait to leave the login page. */
  async loginAndWaitForHome(username: string, password: string): Promise<void> {
    await this.login(username, password);
    await this.page.waitForURL((url) => !url.pathname.endsWith('/login'), { timeout: 30000 });
  }

  /** Assert the login form is showing an authentication error. */
  async expectLoginError(): Promise<void> {
    await expect(this.page.getByRole('alert')).toBeVisible();
  }
}
