import { defineConfig, devices } from '@playwright/test'

/**
 * Playwright config for the browser-level frontend E2E smoke (S6).
 *
 * Runs against the browser-only dev server (`npm run dev:mock-tauri`), which
 * serves the React/Vite app with `dev-tauri-mock.ts` standing in for the Tauri
 * backend (the S1–S5 local-model / pet / onboarding commands are stubbed there).
 *
 * Run: `cd ui && npm install && npx playwright install chromium && npm run e2e`
 */
const BASE_URL = 'http://127.0.0.1:9527'

export default defineConfig({
  testDir: './e2e',
  // The smoke flows reload + drive timers (streamed events); be patient.
  timeout: 30_000,
  expect: { timeout: 10_000 },
  fullyParallel: false,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? 'github' : 'list',
  use: {
    baseURL: BASE_URL,
    trace: 'on-first-retry',
  },
  projects: [
    {
      name: 'chromium',
      // CI installs Playwright's chromium; locally `PW_CHANNEL=chrome` runs the
      // same suite on the system Chrome when the bundled browser isn't installed.
      use: {
        ...devices['Desktop Chrome'],
        ...(process.env.PW_CHANNEL ? { channel: process.env.PW_CHANNEL } : {}),
      },
    },
  ],
  webServer: {
    command: 'npm run dev:mock-tauri',
    url: BASE_URL,
    reuseExistingServer: !process.env.CI,
    timeout: 120_000,
  },
})
