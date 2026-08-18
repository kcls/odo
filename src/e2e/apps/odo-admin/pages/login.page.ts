import { Page, expect } from '@playwright/test';
import { AdminBasePage } from './base.page';

/**
 * Login page object for the ODO Admin SPA.
 *
 * The local username/password form may be hidden behind a "Sign in with
 * username and password" toggle when SSO providers are configured, so
 * `login()` reveals it first if present.
 */
export class AdminLoginPage extends AdminBasePage {
  constructor(page: Page) {
    super(page);
  }

  async goto(): Promise<void> {
    await this.gotoPath('/login');
  }

  /** Fill and submit the local login form, then wait for the app shell. */
  async login(username: string, password: string): Promise<void> {
    const usernameField = this.page.locator('input[name="username"]');

    // When SSO providers are configured the local form is collapsed behind a
    // "Sign in with username and password" toggle. Wait for the login card to
    // render, then reveal the form if it isn't already showing.
    const toggle = this.page.getByRole('button', {
      name: /username and password/i,
    });
    await this.page
      .getByRole('heading', { name: 'ODO Admin' })
      .waitFor({ state: 'visible' });

    if (!(await usernameField.isVisible().catch(() => false))) {
      await toggle.click();
    }
    await usernameField.waitFor({ state: 'visible' });

    await usernameField.fill(username);
    await this.page.locator('input[name="password"]').fill(password);
    await this.page.getByRole('button', { name: /^sign in$/i }).click();
  }

  /** Log in and wait for the home page (tool grid) to render. */
  async loginAndWaitForHome(username: string, password: string): Promise<void> {
    await this.login(username, password);
    // The shell navigates to '/' (home) on success. Wait for the URL to
    // leave /login and the home content to appear.
    await this.page.waitForURL(
      (url) => !url.pathname.endsWith('/login'),
      { timeout: 30000 },
    );
  }

  /** Assert the login form is showing an authentication error. */
  async expectLoginError(): Promise<void> {
    await expect(this.page.locator('.login-error')).toBeVisible();
  }
}
