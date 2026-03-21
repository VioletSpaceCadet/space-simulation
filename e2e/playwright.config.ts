import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./tests",
  fullyParallel: false, // Tests share a single daemon instance with accumulating state
  workers: 1, // Must be 1 — all tests share one daemon, parallel beforeEach calls race on pause/resume/speed
  timeout: 30_000,
  retries: process.env.CI ? 1 : 0,
  use: {
    baseURL: "http://localhost:5174",
    screenshot: "only-on-failure",
    trace: "retain-on-failure",
  },
  projects: [
    {
      name: "chromium",
      use: { browserName: "chromium" },
    },
  ],
  outputDir: "./test-results",
  globalSetup: "./global-setup.ts",
  globalTeardown: "./global-teardown.ts",
});
