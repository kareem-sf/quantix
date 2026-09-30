import { expect, test } from "@playwright/test";
import { pdf, startTender } from "./quantix";

const conditions = pdf("Conditions of Contract.pdf", [
  ["Conditions of Contract", "Clause 1. The Works are a two storey school block."],
  ["Clause 14. Tender security", "The tenderer shall provide a tender security of one percent of the tender price."],
]);
const specification = pdf("Specification.pdf", [["Section 03 30 00", "Cast-in-place concrete grade C30/37."]]);

test("the package is added, read and searched, and a page opens where the words are", async ({ page }) => {
  await startTender(page, "Synthetic school package");

  await page.getByLabel("Tender files").setInputFiles([conditions, specification]);
  const read = page.getByRole("main").getByRole("link", { name: /Documents\s*2 of 2 read/ });
  await expect(read).toBeVisible({ timeout: 30_000 });

  await read.click();
  const register = page.getByRole("region", { name: "Documents" });
  await expect(register.getByText("2 files", { exact: true })).toBeVisible();

  await register.getByLabel("Search the documents").fill("tender security");
  await register.getByLabel("Search the documents").press("Enter");
  await register.getByRole("button", { name: /Conditions of Contract/ }).first().click();
  await expect(page).toHaveURL(/doc=\w+&page=2/);
  await expect(page.getByRole("region", { name: "Viewer" }).getByText(/one percent of the tender price/)).toBeVisible();

  await register.getByLabel("Clear the search").click();
  await register.getByRole("button", { name: /Specification/ }).click();
  await expect(page.getByRole("region", { name: "Viewer" }).getByText(/Cast-in-place concrete/)).toBeVisible();
});

test("the due date is set on the tender and closes it on the Desk", async ({ page }) => {
  await startTender(page, "Synthetic warehouse");

  await page.getByRole("button", { name: "Set the due date" }).click();
  await page.getByLabel("Due date", { exact: true }).fill("2030-03-14");
  await page.getByLabel("Due date", { exact: true }).press("Enter");
  await expect(page.getByRole("main").getByText(/^Due 14 March, \d+ days left\./)).toBeVisible();
  await expect(page.getByLabel("Where the due date comes from: Set by you, not from the tender documents")).toBeVisible();

  await page.goto("/desk");
  await expect(page.getByRole("region", { name: "Closing dates" }).getByText("Synthetic warehouse")).toBeVisible();
});

test("a tender is marked submitted, archived, restored and deleted from its menu", async ({ page }) => {
  const url = await startTender(page, "Synthetic clinic");

  const menu = page.getByRole("button", { name: "Tender actions" });
  const closed = () => expect(page.getByRole("menu", { name: "Tender actions" })).toHaveCount(0); // once it's kept
  await menu.click();
  await page.getByRole("menuitemradio", { name: "Submitted" }).click();
  await closed();
  await page.goto("/tenders");
  await page.getByRole("tab", { name: /Submitted/ }).click();
  await expect(page.getByRole("main").getByRole("link", { name: "Synthetic clinic" })).toBeVisible();

  await page.goto(url);
  await menu.click();
  await page.getByRole("menuitem", { name: "Archive tender" }).click();
  await closed();
  await page.goto("/tenders");
  await page.getByRole("tab", { name: /Archived/ }).click();
  await expect(page.getByRole("main").getByRole("link", { name: "Synthetic clinic" })).toBeVisible();

  await page.goto(url);
  await menu.click();
  await page.getByRole("menuitem", { name: "Restore from the archive" }).click();
  await closed();
  await menu.click();
  await page.getByRole("menuitem", { name: "Delete this tender" }).click();
  await page.getByRole("button", { name: "Delete tender" }).click();
  await expect(page).not.toHaveURL(url);
  await page.goto("/tenders");
  await expect(page.getByRole("main").getByText("Synthetic clinic")).toHaveCount(0);
});
