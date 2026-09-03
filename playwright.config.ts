import { defineConfig, devices } from '@playwright/test';

export default defineConfig({
  testDir: './tests-e2e',
  timeout: 30000,
  fullyParallel: false,
  retries: 0,
  reporter: [['list']],
  use: {
    baseURL: 'http://localhost:43801',
    trace: 'retain-on-failure',
  },
  webServer: {
    command: 'pnpm run dev',
    url: 'http://localhost:43801',
    reuseExistingServer: true,
    timeout: 60000,
  },
  projects: [{ name: 'chromium', use: devices['Desktop Chrome'] }],
});
