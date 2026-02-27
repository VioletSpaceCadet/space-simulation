import { test, expect } from "@playwright/test";
import { daemonPost, getMeta } from "./helpers";

test.describe("App loads and displays live data", () => {
  test.beforeEach(async ({ page }) => {
    await daemonPost("/api/v1/resume");
    await daemonPost("/api/v1/speed", { ticks_per_sec: 100 });
    await page.goto("/");
  });

  test.afterEach(async () => {
    await daemonPost("/api/v1/pause");
  });

  test("tick counter is visible and incrementing", async ({ page }) => {
    const tickText = page.locator("text=/tick \\d+/");
    await expect(tickText).toBeVisible({ timeout: 10_000 });

    const initialMeta = await getMeta();

    // Wait for ticks to advance, then check via API (avoids flaky DOM reads)
    await page.waitForTimeout(2000);
    const laterMeta = await getMeta();

    expect(laterMeta.tick).toBeGreaterThan(initialMeta.tick);
  });

  test("at least one panel is rendered", async ({ page }) => {
    const navButtons = page.locator("nav button");
    await expect(navButtons.first()).toBeVisible({ timeout: 10_000 });
    const count = await navButtons.count();
    expect(count).toBeGreaterThanOrEqual(1);
  });

  test("status bar shows connection state", async ({ page }) => {
    const statusText = page.locator("text=/connected|Running/i");
    await expect(statusText.first()).toBeVisible({ timeout: 10_000 });
  });
});
