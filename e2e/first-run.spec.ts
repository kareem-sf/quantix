import { expect, test } from "@playwright/test";

// Runs first, on the empty data folder: the other specs add tenders.
test("a new engineer is asked to start their first tender, and told the office needs an AI", async ({ page }) => {
  await page.goto("/");
  await expect(page).toHaveURL("/new");
  await expect(page.getByRole("heading", { name: "Start a tender" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Start tender" })).toBeDisabled();

  await page.getByLabel("Tender name").fill("First synthetic tender");
  await page.getByRole("button", { name: "Start tender" }).click();

  await expect(page).toHaveURL(/\/tenders\/\w+$/);
  const main = page.getByRole("main");
  await expect(main.getByText("First synthetic tender", { exact: true })).toBeVisible();
  await expect(main.getByRole("heading", { name: "Add the tender package" })).toBeVisible();
  await expect(main.getByText(/Choose the office’s AI in\s*Settings\s*so the team can start work/)).toBeVisible();
  await main.getByRole("link", { name: "Settings" }).click();
  await expect(page).toHaveURL("/settings?section=ai");
  await expect(page.getByText("Add an AI connection so the office can start work.")).toBeVisible();

  await page.goto("/");
  await expect(page).toHaveURL("/desk");
});
