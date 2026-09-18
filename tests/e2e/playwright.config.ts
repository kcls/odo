import { defineConfig, devices } from '@playwright/test';

/**
 * Environment configuration
 * Override via environment variables:
 *   BASE_URL=http://localhost:30080 npm test
 */
const BASE_URL = process.env.BASE_URL || 'http://localhost:3001';

/**
 * Playwright configuration for odo E2E tests
 * @see https://playwright.dev/docs/test-configuration
 */
export default defineConfig({
  // Each app owns its tests under apps/<app>/tests; testDir is set per project
  // (the Current app's incident-tracker suite lives in kcls/current).

  // Run tests in parallel
  fullyParallel: true,

  // Fail the build on CI if you accidentally left test.only in the source code
  forbidOnly: !!process.env.CI,

  // Retries: 2 on CI, 1 locally. e2e drives a real browser + backend, so a
  // small number of timing races remain (e.g. the multi-step login flow in the
  // login-flow tests, which by nature can't reuse a shared session). A single
  // retry absorbs those without masking real failures — a genuinely broken test
  // still fails every attempt.
  retries: process.env.CI ? 2 : 1,

  // Workers. Specs share one backend; default to 1 worker so files don't
  // interleave. Override with --workers=N when isolation allows.
  workers: 1,

  // Reporter configuration
  reporter: [
    ['html', { open: 'never', outputFolder: 'reports/html' }],
    ['json', { outputFile: 'reports/results.json' }],
    process.env.CI ? ['github'] : ['list'],
  ],

  // Shared settings for all projects
  use: {
    // Base URL for navigation
    baseURL: BASE_URL,

    // Ignore HTTPS errors (needed for MockSAML SSO flow)
    ignoreHTTPSErrors: true,

    // Collect trace when retrying the failed test
    trace: 'on-first-retry',

    // Screenshot on failure
    screenshot: 'only-on-failure',

    // Video on failure
    video: 'retain-on-failure',

    // Default timeout for actions
    actionTimeout: 10000,

    // Default navigation timeout
    navigationTimeout: 30000,
  },

  // Test timeout
  timeout: 60000,

  // Expect timeout
  expect: {
    timeout: 10000,
  },

  // Projects: the ODO Admin SPA, Chromium.
  projects: [
    // ODO Admin SPA, served under the /odo/admin base path. Self-contained:
    // each spec logs in itself (odo-auth rotates refresh tokens, so a shared
    // saved session is fragile), so there is no setup dependency.
    {
      name: 'odo-admin',
      testDir: './apps/odo-admin/tests',
      use: {
        ...devices['Desktop Chrome'],
        bypassCSP: true,
        launchOptions: {
          args: [
            `--unsafely-treat-insecure-origin-as-secure=${BASE_URL}`,
            '--allow-running-insecure-content',
            '--disable-web-security',
            '--ignore-certificate-errors',
            '--disable-features=BlockInsecurePrivateNetworkRequests',
          ],
        },
      },
    },
  ],

  // Output folder for test artifacts
  outputDir: 'reports/test-results',
});
