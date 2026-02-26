import { test, expect } from "@playwright/test";

test.describe("Speed controls via keyboard presets", () => {
  test.beforeEach(async ({ page }) => {
    await fetch("http://localhost:3002/api/v1/resume", { method: "POST" });
    await fetch("http://localhost:3002/api/v1/speed", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ ticks_per_sec: 10 }),
    });
    await page.goto("/");
    await page.locator("text=/tick \\d+/").waitFor({ timeout: 10_000 });
  });

  test.afterEach(async () => {
    await fetch("http://localhost:3002/api/v1/pause", { method: "POST" });
  });

  test("pressing 1 sets speed to 100 TPS", async ({ page }) => {
    await page.keyboard.press("Digit1");
    // Verify via API that speed was set
    const meta = await (await fetch("http://localhost:3002/api/v1/meta")).json();
    expect(meta.ticks_per_sec).toBe(100);
  });

  test("pressing 2 sets speed to 1K TPS", async ({ page }) => {
    await page.keyboard.press("Digit2");
    const meta = await (await fetch("http://localhost:3002/api/v1/meta")).json();
    expect(meta.ticks_per_sec).toBe(1000);
  });

  test("pressing 5 sets speed to max (0)", async ({ page }) => {
    await page.keyboard.press("Digit5");
    const meta = await (await fetch("http://localhost:3002/api/v1/meta")).json();
    expect(meta.ticks_per_sec).toBe(0);
  });
});
