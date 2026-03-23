import { test, expect } from "@playwright/test";
import { daemonPost, getMeta } from "./helpers";

test.describe("Speed controls via keyboard arrows", () => {
  test.beforeEach(async ({ page }) => {
    await daemonPost("/api/v1/resume");
    await daemonPost("/api/v1/speed", { ticks_per_sec: 10 });
    await page.goto("/");
    await page.locator("text=/tick \\d+/").waitFor({ timeout: 10_000 });
  });

  test.afterEach(async () => {
    await daemonPost("/api/v1/pause");
  });

  test("pressing ArrowRight increases speed to first preset", async ({ page }) => {
    // Start at 10 TPS (not a preset), ArrowRight should go to 100
    await page.keyboard.press("ArrowRight");
    await page.waitForTimeout(200);
    const meta = await getMeta();
    expect(meta.ticks_per_sec).toBe(100);
  });

  test("pressing ArrowRight twice reaches 1K TPS", async ({ page }) => {
    await page.keyboard.press("ArrowRight");
    await page.waitForTimeout(100);
    await page.keyboard.press("ArrowRight");
    await page.waitForTimeout(200);
    const meta = await getMeta();
    expect(meta.ticks_per_sec).toBe(1000);
  });

  test("pressing ArrowLeft decreases speed", async ({ page }) => {
    // Set to 1K first, then decrease
    await daemonPost("/api/v1/speed", { ticks_per_sec: 1000 });
    await page.waitForTimeout(200);
    await page.keyboard.press("ArrowLeft");
    await page.waitForTimeout(200);
    const meta = await getMeta();
    expect(meta.ticks_per_sec).toBe(100);
  });
});
