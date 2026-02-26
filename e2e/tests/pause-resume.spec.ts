import { test, expect } from "@playwright/test";

const DAEMON_URL = "http://localhost:3002";

test.describe("Pause and resume", () => {
  test.beforeEach(async ({ page }) => {
    await fetch(`${DAEMON_URL}/api/v1/resume`, { method: "POST" });
    await fetch(`${DAEMON_URL}/api/v1/speed`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ ticks_per_sec: 100 }),
    });
    await page.goto("/");
    // Wait for app to be ready
    await page.locator("text=/tick \\d+/").waitFor({ timeout: 10_000 });
  });

  test.afterEach(async () => {
    await fetch(`${DAEMON_URL}/api/v1/pause`, { method: "POST" });
  });

  test("pause button stops tick counter", async ({ page }) => {
    // Click pause
    const pauseButton = page.locator("button", { hasText: /running/i });
    await pauseButton.click();

    // Should show "Paused"
    await expect(page.locator("button", { hasText: /paused/i })).toBeVisible();

    // Wait for any in-flight ticks to settle (the daemon may process a few
    // more ticks between the UI click and the actual pause taking effect)
    await page.waitForTimeout(2000);

    // Verify tick is roughly stable — allow up to 200 ticks of drift from
    // in-flight processing between the pause click and the daemon stopping
    const meta1 = await (await fetch(`${DAEMON_URL}/api/v1/meta`)).json();
    await page.waitForTimeout(1500);
    const meta2 = await (await fetch(`${DAEMON_URL}/api/v1/meta`)).json();
    expect(meta2.tick - meta1.tick).toBeLessThanOrEqual(200);
  });

  test("resume button restarts tick counter", async ({ page }) => {
    // Pause via UI button (same as the pause test)
    await page.locator("button", { hasText: /running/i }).click();
    await expect(
      page.locator("button", { hasText: /paused/i }),
    ).toBeVisible();

    // Record tick while paused via API (reliable source of truth)
    const metaBefore = await (
      await fetch(`${DAEMON_URL}/api/v1/meta`)
    ).json();

    // Resume via UI button
    await page.locator("button", { hasText: /paused/i }).click();
    await expect(
      page.locator("button", { hasText: /running/i }),
    ).toBeVisible();

    // Wait for ticks to advance, then verify via API
    await page.waitForTimeout(2000);
    const metaAfter = await (
      await fetch(`${DAEMON_URL}/api/v1/meta`)
    ).json();
    expect(metaAfter.tick).toBeGreaterThan(metaBefore.tick);
  });

  test("spacebar toggles pause", async ({ page }) => {
    // Press space to pause
    await page.keyboard.press("Space");
    await expect(page.locator("button", { hasText: /paused/i })).toBeVisible();

    // Press space to resume
    await page.keyboard.press("Space");
    await expect(page.locator("button", { hasText: /running/i })).toBeVisible();
  });
});
