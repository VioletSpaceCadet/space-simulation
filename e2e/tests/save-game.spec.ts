import { test, expect } from "@playwright/test";

test.describe("Save game", () => {
  test.beforeEach(async ({ page }) => {
    await fetch("http://localhost:3002/api/v1/resume", { method: "POST" });
    await fetch("http://localhost:3002/api/v1/speed", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ ticks_per_sec: 100 }),
    });
    await page.goto("/");
    await page.locator("text=/tick \\d+/").waitFor({ timeout: 10_000 });
  });

  test.afterEach(async () => {
    await fetch("http://localhost:3002/api/v1/pause", { method: "POST" });
  });

  test("save button shows success feedback", async ({ page }) => {
    const saveButton = page.locator("button", { hasText: /^save$/i });
    await saveButton.click();
    // Should show "Saved" or success indicator
    await expect(page.locator("text=/saved/i")).toBeVisible({ timeout: 5_000 });
  });

  test("Cmd+S triggers save", async ({ page }) => {
    await page.keyboard.press("Meta+s");
    await expect(page.locator("text=/saved/i")).toBeVisible({ timeout: 5_000 });
  });
});
