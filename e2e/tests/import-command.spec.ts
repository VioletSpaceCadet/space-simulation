import { test, expect } from "@playwright/test";

const DAEMON_URL = "http://localhost:3002";
const TRADE_UNLOCK_TICK = 525_600;

/**
 * Poll the daemon until the sim tick reaches the target.
 * Uses max speed (tps=0) to advance as fast as possible,
 * then pauses the simulation once the target is reached.
 */
async function advanceToTick(
  target: number,
  timeoutMs = 60_000,
): Promise<void> {
  await fetch(`${DAEMON_URL}/api/v1/speed`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ ticks_per_sec: 0 }),
  });

  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const meta = await (await fetch(`${DAEMON_URL}/api/v1/meta`)).json();
    if (meta.tick >= target) {
      return;
    }
    await new Promise((resolve) => setTimeout(resolve, 500));
  }
  throw new Error(`Timed out waiting for tick ${target}`);
}

async function getBalance(): Promise<number> {
  const snapshot = await (
    await fetch(`${DAEMON_URL}/api/v1/snapshot`)
  ).json();
  return snapshot.balance;
}

test.describe("Import command via Economy panel", () => {
  test.beforeEach(async ({ page }) => {
    await fetch(`${DAEMON_URL}/api/v1/resume`, { method: "POST" });
    await fetch(`${DAEMON_URL}/api/v1/speed`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ ticks_per_sec: 100 }),
    });
    await page.goto("/");
    await page.locator("text=/tick \\d+/").waitFor({ timeout: 10_000 });
  });

  test.afterEach(async () => {
    await fetch(`${DAEMON_URL}/api/v1/pause`, { method: "POST" });
  });

  test("importing an item decreases the balance", async ({ page }) => {
    // Advance past the trade unlock tick so imports are accepted.
    // This uses max speed; may take several seconds to reach tick 525,700.
    await advanceToTick(TRADE_UNLOCK_TICK + 100);

    // Pause the sim so the balance stays stable while we interact with the UI
    await fetch(`${DAEMON_URL}/api/v1/pause`, { method: "POST" });

    // Record balance before the import
    const balanceBefore = await getBalance();
    expect(balanceBefore).toBeGreaterThan(0);

    // Reload page so the UI reflects the current paused state and balance
    await page.goto("/");
    await page.locator("text=/tick \\d+/").waitFor({ timeout: 10_000 });

    // Ensure the Economy panel is visible by checking for its tab
    const economyTab = page.locator('[data-testid="panel-tab-economy"]');
    if (!(await economyTab.isVisible())) {
      await page.locator("nav button", { hasText: /economy/i }).click();
    }
    await economyTab.waitFor({ state: "visible", timeout: 5_000 });

    // Scope to the Import section (identified by its heading text).
    // The Economy panel has Import and Export sections, each with their own
    // category select, item select, quantity input, and action button.
    const importSection = page
      .locator("div")
      .filter({ hasText: /^Import$/ })
      .first();
    await importSection.waitFor({ state: "visible", timeout: 10_000 });

    // Select "Material" category (should be the default)
    const categorySelect = importSection.locator("select").first();
    await categorySelect.selectOption("Material");

    // Select the first available item in the item dropdown
    const itemSelect = importSection.locator("select").nth(1);
    await itemSelect.waitFor({ state: "visible", timeout: 5_000 });
    const firstOptionValue = await itemSelect
      .locator("option")
      .first()
      .getAttribute("value");
    expect(firstOptionValue).toBeTruthy();
    await itemSelect.selectOption(firstOptionValue!);

    // Set a quantity (visible for Materials)
    const quantityInput = importSection.locator('input[type="number"]');
    await quantityInput.waitFor({ state: "visible", timeout: 5_000 });
    await quantityInput.fill("10");

    // Click Import button
    const importButton = importSection.locator("button", {
      hasText: /^Import$/,
    });
    await importButton.waitFor({ state: "visible", timeout: 5_000 });
    await importButton.click();

    // Wait for the button to show "Sent" confirming the command was accepted
    await expect(
      importSection.locator("button", { hasText: /Sent/ }),
    ).toBeVisible({ timeout: 5_000 });

    // Resume briefly so the queued import command gets processed
    await fetch(`${DAEMON_URL}/api/v1/resume`, { method: "POST" });
    await fetch(`${DAEMON_URL}/api/v1/speed`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ ticks_per_sec: 100 }),
    });
    // Give the sim a few ticks to process the import command
    await page.waitForTimeout(1_000);
    await fetch(`${DAEMON_URL}/api/v1/pause`, { method: "POST" });

    // Verify the balance decreased
    const balanceAfter = await getBalance();
    expect(balanceAfter).toBeLessThan(balanceBefore);
  });
});
