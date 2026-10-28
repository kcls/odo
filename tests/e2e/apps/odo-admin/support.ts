import { APIRequestContext, Page, expect } from '@playwright/test';
import { AdminLoginPage } from './pages/login.page';
import { ADMIN_USERS, type AdminTestUser } from './test-users';

/** Log in through the UI as `user` (each test logs in fresh). */
export async function loginAs(page: Page, user: AdminTestUser = ADMIN_USERS.admin): Promise<void> {
  const login = new AdminLoginPage(page);
  await login.goto();
  await login.loginAndWaitForHome(user.username, user.password);
}

/** An API access token for `user`, for arranging data outside the UI. */
export async function apiToken(
  request: APIRequestContext,
  user: AdminTestUser = ADMIN_USERS.admin,
): Promise<string> {
  const resp = await request.post('/api/v1/odo/auth/login', {
    data: { username: user.username, password: user.password },
  });
  expect(resp.ok(), `API login as ${user.username}`).toBeTruthy();
  return (await resp.json()).access_token;
}

/** Nothing on the page reports a failure: no error page, no error toast. */
export async function expectNoErrors(page: Page): Promise<void> {
  await expect(page.getByRole('heading', { name: 'Something went wrong' })).toHaveCount(0);
  // Sonner marks each toast's type; any error toast means a failed call.
  await expect(page.locator('[data-sonner-toast][data-type="error"]')).toHaveCount(0);
}
