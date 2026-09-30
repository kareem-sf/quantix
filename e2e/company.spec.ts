import { expect, test } from "@playwright/test";

test("a new firm starts with example rules, and the engineer adds and removes their own", async ({ page }) => {
  await page.goto("/rules");
  await expect(page.getByText("Example").first()).toBeVisible();

  await page.getByLabel("Topic").fill("Markups");
  await page.getByLabel("Rule").fill("Overheads 5%, profit 7% on synthetic tenders.");
  await page.getByRole("button", { name: "Add" }).click();
  await page.reload();
  const rule = page.getByText("Overheads 5%, profit 7% on synthetic tenders.");
  await expect(rule).toBeVisible();

  const removes = page.getByRole("button", { name: "Remove" });
  const before = await removes.count();
  await page
    .locator("li, div")
    .filter({ has: rule })
    .last()
    .getByRole("button", { name: "Remove" })
    .click();
  await expect(rule).toHaveCount(0);
  await expect(removes).toHaveCount(before - 1);
});

test("the letterhead every document carries is kept", async ({ page }) => {
  await page.goto("/company");
  await page.getByLabel("Company name").fill("Synthetic Builders Co.");
  await page.getByLabel("VAT registration").fill("300000000000003");
  await page.getByLabel("Address").fill("1 Test Street, Riyadh");
  await page.getByRole("button", { name: "Save" }).click();
  await expect(page.getByText("Saved")).toBeVisible();

  await page.reload();
  await expect(page.getByLabel("Company name")).toHaveValue("Synthetic Builders Co.");
  await expect(page.getByLabel("VAT registration")).toHaveValue("300000000000003");
  await expect(page.getByLabel("Address")).toHaveValue("1 Test Street, Riyadh");
});

test("subcontractors and suppliers are added to the directory, found by trade and removed", async ({ page }) => {
  await page.goto("/directory");
  await page.getByLabel("Company").fill("Synthetic Waterproofing Est.");
  await page.getByLabel("Kind").selectOption("Subcontractor");
  await page.getByLabel("Trades").fill("waterproofing, roofing");
  await page.getByLabel("Email").fill("quotes@waterproofing.test");
  await page.getByRole("button", { name: "Add" }).click();

  await page.getByLabel("Company").fill("Synthetic Ready Mix");
  await page.getByLabel("Kind").selectOption("Supplier");
  await page.getByLabel("Trades").fill("ready-mix concrete");
  await page.getByRole("button", { name: "Add" }).click();
  await expect(page.getByText("Synthetic Ready Mix")).toBeVisible();

  await page.getByLabel("Search the directory").fill("roofing");
  await expect(page.getByText("Synthetic Waterproofing Est.")).toBeVisible();
  await expect(page.getByText("Synthetic Ready Mix")).toHaveCount(0);
  await expect(page.getByText("quotes@waterproofing.test")).toBeVisible();

  await page.getByRole("button", { name: "Remove" }).click();
  await expect(page.getByText("Nothing matches.")).toBeVisible();
});

test("a rate is added to the company library with its source, and removed", async ({ page }) => {
  await page.goto("/library");
  await page.getByLabel("Kind").selectOption("Material");
  await page.getByLabel("Name").fill("Synthetic C30 concrete");
  await page.getByLabel("Unit").fill("m3");
  await page.getByLabel("Rate").fill("265.5");
  await page.getByLabel("Currency").fill("SAR");
  await page.getByLabel("Source").fill("Synthetic supplier quote");
  await page.getByRole("button", { name: "Add" }).click();

  await page.reload();
  const row = page.locator("div").filter({ hasText: /^MaterialSynthetic C30 concrete/ });
  await expect(row).toContainText("265.50 SAR");
  await expect(row).toContainText("Synthetic supplier quote");
  await expect(row).toContainText(new Date().toISOString().slice(0, 10));

  await row.getByRole("button", { name: "Remove" }).click();
  await expect(page.getByText("Synthetic C30 concrete")).toHaveCount(0);
});
