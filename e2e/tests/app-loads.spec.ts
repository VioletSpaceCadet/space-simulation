import { test, expect } from "@playwright/test";

test.describe("App loads and displays live data", () => {
  test.beforeEach(async ({ page }) => {
    // Resume the sim so ticks advance
    await fetch("http://localhost:3002/api/v1/resume", { method: "POST" });
    await fetch("http://localhost:3002/api/v1/speed", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ ticks_per_sec: 100 }),
    });
    await page.goto("/");
  });

  test.afterEach(async () => {
    // Re-pause for other tests
    await fetch("http://localhost:3002/api/v1/pause", { method: "POST" });
  });

  test("tick counter is visible and incrementing", async ({ page }) => {
    // Wait for tick to appear (contains "tick" text with a number)
    const tickText = page.locator("text=/tick \\d+/");
    await expect(tickText).toBeVisible({ timeout: 10_000 });

    // Get initial tick value
    const initialText = await tickText.textContent();
    const initialTick = parseInt(initialText!.match(/tick (\d+)/)![1], 10);

    // Wait a moment and check tick has advanced
    await page.waitForTimeout(2000);
    const laterText = await tickText.textContent();
    const laterTick = parseInt(laterText!.match(/tick (\d+)/)![1], 10);

    expect(laterTick).toBeGreaterThan(initialTick);
  });

  test("at least one panel is rendered", async ({ page }) => {
    // Wait for any nav button to be visible (panel toggle buttons)
    const navButtons = page.locator("nav button");
    await expect(navButtons.first()).toBeVisible({ timeout: 10_000 });
    const count = await navButtons.count();
    expect(count).toBeGreaterThanOrEqual(1);
  });

  test("status bar shows connection state", async ({ page }) => {
    // Should show "connected" or "Running" indicating SSE is working
    const statusText = page.locator("text=/connected|Running/i");
    await expect(statusText.first()).toBeVisible({ timeout: 10_000 });
  });
});
