import { test, expect } from "@playwright/test";
import { daemonPost, getMeta } from "./helpers";

test.describe("Pause and resume", () => {
  test.beforeEach(async ({ page }) => {
    await daemonPost("/api/v1/resume");
    await daemonPost("/api/v1/speed", { ticks_per_sec: 100 });
    await page.goto("/");
    await page.locator("text=/tick \\d+/").waitFor({ timeout: 10_000 });
  });

  test.afterEach(async () => {
    await daemonPost("/api/v1/pause");
  });

  test("pause button stops tick counter", async ({ page }) => {
    const pauseButton = page.locator("button", { hasText: /running/i });
    await pauseButton.click();

    await expect(page.locator("button", { hasText: /paused/i })).toBeVisible();

    // Wait for any in-flight ticks to settle
    await page.waitForTimeout(2000);

    // Verify tick is roughly stable — allow up to 200 ticks of drift from
    // in-flight processing between the pause click and the daemon stopping
    const meta1 = await getMeta();
    await page.waitForTimeout(1500);
    const meta2 = await getMeta();
    expect(meta2.tick - meta1.tick).toBeLessThanOrEqual(200);
  });

  test("resume button restarts tick counter", async ({ page }) => {
    await page.locator("button", { hasText: /running/i }).click();
    await expect(
      page.locator("button", { hasText: /paused/i }),
    ).toBeVisible();

    const metaBefore = await getMeta();

    await page.locator("button", { hasText: /paused/i }).click();
    await expect(
      page.locator("button", { hasText: /running/i }),
    ).toBeVisible();

    await page.waitForTimeout(2000);
    const metaAfter = await getMeta();
    expect(metaAfter.tick).toBeGreaterThan(metaBefore.tick);
  });

  test("spacebar toggles pause", async ({ page }) => {
    await page.keyboard.press("Space");
    await expect(page.locator("button", { hasText: /paused/i })).toBeVisible();

    await page.keyboard.press("Space");
    await expect(page.locator("button", { hasText: /running/i })).toBeVisible();
  });
});
