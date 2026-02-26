import { test, expect } from "@playwright/test";

test.describe("Pause and resume", () => {
  test.beforeEach(async ({ page }) => {
    await fetch("http://localhost:3002/api/v1/resume", { method: "POST" });
    await fetch("http://localhost:3002/api/v1/speed", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ ticks_per_sec: 100 }),
    });
    await page.goto("/");
    // Wait for app to be ready
    await page.locator("text=/tick \\d+/").waitFor({ timeout: 10_000 });
  });

  test.afterEach(async () => {
    await fetch("http://localhost:3002/api/v1/pause", { method: "POST" });
  });

  test("pause button stops tick counter", async ({ page }) => {
    // Click pause
    const pauseButton = page.locator("button", { hasText: /running/i });
    await pauseButton.click();

    // Should show "Paused"
    await expect(page.locator("button", { hasText: /paused/i })).toBeVisible();

    // Record tick, wait, verify it hasn't changed
    const tickText = page.locator("text=/tick \\d+/");
    const pausedText = await tickText.textContent();
    await page.waitForTimeout(1500);
    const stillPausedText = await tickText.textContent();
    expect(stillPausedText).toBe(pausedText);
  });

  test("resume button restarts tick counter", async ({ page }) => {
    // Pause first
    await page.locator("button", { hasText: /running/i }).click();
    await expect(page.locator("button", { hasText: /paused/i })).toBeVisible();

    // Resume
    await page.locator("button", { hasText: /paused/i }).click();
    await expect(page.locator("button", { hasText: /running/i })).toBeVisible();

    // Verify ticks advancing
    const tickText = page.locator("text=/tick \\d+/");
    const afterResumeText = await tickText.textContent();
    const afterResumeTick = parseInt(afterResumeText!.match(/tick (\d+)/)![1], 10);
    await page.waitForTimeout(1500);
    const laterText = await tickText.textContent();
    const laterTick = parseInt(laterText!.match(/tick (\d+)/)![1], 10);
    expect(laterTick).toBeGreaterThan(afterResumeTick);
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
