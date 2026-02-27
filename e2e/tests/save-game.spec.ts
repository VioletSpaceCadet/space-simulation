import { test, expect } from "@playwright/test";
import { daemonPost } from "./helpers";

test.describe("Save game", () => {
  test.beforeEach(async ({ page }) => {
    await daemonPost("/api/v1/resume");
    await daemonPost("/api/v1/speed", { ticks_per_sec: 100 });
    await page.goto("/");
    await page.locator("text=/tick \\d+/").waitFor({ timeout: 10_000 });
  });

  test.afterEach(async () => {
    await daemonPost("/api/v1/pause");
  });

  test("save button shows success feedback", async ({ page }) => {
    const saveButton = page.locator("button", { hasText: /^save$/i });
    await saveButton.click();
    await expect(page.locator("text=/saved/i")).toBeVisible({ timeout: 5_000 });
  });

  test("Ctrl+S triggers save", async ({ page }) => {
    await page.keyboard.press("Control+s");
    await expect(page.locator("text=/saved/i")).toBeVisible({ timeout: 5_000 });
  });
});
