import { test, expect } from "@playwright/test";
import { daemonPost, getMeta } from "./helpers";

test.describe("Speed controls via keyboard presets", () => {
  test.beforeEach(async ({ page }) => {
    await daemonPost("/api/v1/resume");
    await daemonPost("/api/v1/speed", { ticks_per_sec: 10 });
    await page.goto("/");
    await page.locator("text=/tick \\d+/").waitFor({ timeout: 10_000 });
  });

  test.afterEach(async () => {
    await daemonPost("/api/v1/pause");
  });

  test("pressing 1 sets speed to 100 TPS", async ({ page }) => {
    await page.keyboard.press("Digit1");
    const meta = await getMeta();
    expect(meta.ticks_per_sec).toBe(100);
  });

  test("pressing 2 sets speed to 1K TPS", async ({ page }) => {
    await page.keyboard.press("Digit2");
    const meta = await getMeta();
    expect(meta.ticks_per_sec).toBe(1000);
  });

  test("pressing 5 sets speed to max (0)", async ({ page }) => {
    await page.keyboard.press("Digit5");
    const meta = await getMeta();
    expect(meta.ticks_per_sec).toBe(0);
  });
});
