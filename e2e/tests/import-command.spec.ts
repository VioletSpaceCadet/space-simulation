import { test, expect } from "@playwright/test";

const DAEMON_URL = "http://localhost:3002";

async function getBalance(): Promise<number> {
  const response = await fetch(`${DAEMON_URL}/api/v1/snapshot`);
  if (!response.ok) throw new Error(`snapshot returned ${response.status}`);
  const snapshot = await response.json();
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
    // The daemon starts from a pre-built state past TRADE_UNLOCK_TICK,
    // so imports are already accepted — no need to simulate 525K ticks.

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

    // The Economy panel has Import and Export sections. Each section is a
    // div.mb-3 containing a heading div ("Import"/"Export"), selects, input,
    // and a button. We find the Import section by locating the heading and
    // then scoping to its parent container.
    const importHeading = page.locator("div", { hasText: /^Import$/ }).first();
    await importHeading.waitFor({ state: "visible", timeout: 10_000 });
    // The parent of the heading contains all the form controls
    const importSection = importHeading.locator("..");

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
