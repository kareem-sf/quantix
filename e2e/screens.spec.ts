import { expect, test, type Page } from "@playwright/test";
import { startTender } from "./quantix";

const TENDER = ["", "/documents", "/takeoff", "/estimate", "/subcontract", "/queries", "/submission"];
const FIRM = ["/desk", "/tenders", "/library", "/directory", "/rules", "/company", "/settings"];

/** What went wrong in the page: uncaught errors, console errors and failed requests to the service. */
function troubles(page: Page) {
  const found: string[] = [];
  page.on("pageerror", (error) => found.push(`error: ${error.message}`));
  page.on("console", (m) => m.type() === "error" && found.push(`console: ${m.text()}`));
  page.on("response", (r) => r.url().includes("/api/") && r.status() >= 500 && found.push(`${r.status()} ${r.url()}`));
  return found;
}

test("every screen of a tender and of the firm opens without an error", async ({ page }) => {
  const found = troubles(page);
  const tender = await startTender(page, "Synthetic screens tender");

  for (const path of [...TENDER.map((p) => tender + p), ...FIRM]) {
    await page.goto(path);
    await expect(page.getByRole("main")).not.toBeEmpty();
    await expect(page.getByText("Something went wrong on this screen.")).toHaveCount(0);
    await page.waitForLoadState("networkidle");
  }
  expect(found).toEqual([]);
});

test("the engineer moves around with the keyboard: screens, search and the shortcuts list", async ({ page }) => {
  const tender = await startTender(page, "Synthetic keyboard tender");

  await page.keyboard.press("Control+2");
  await expect(page).toHaveURL(`${tender}/documents`);
  await page.keyboard.press("Control+4");
  await expect(page).toHaveURL(`${tender}/estimate`);

  await page.goto("/desk");
  await page.keyboard.press("Control+k");
  const search = page.getByRole("dialog", { name: "Search or jump to" });
  await search.getByRole("textbox", { name: "Search or jump to" }).fill("keyboard tender");
  await page.keyboard.press("Enter");
  await expect(page).toHaveURL(tender);

  await page.keyboard.press("Control+/");
  const shortcuts = page.getByRole("dialog", { name: "Keyboard shortcuts" });
  await expect(shortcuts.getByText("Search or jump to a tender, a screen or a person")).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(shortcuts).toHaveCount(0);
});

test("a tender that doesn't exist says so instead of failing", async ({ page }) => {
  await page.goto("/tenders/no-such-tender/documents");
  await expect(page.getByRole("navigation", { name: "Quantix" })).toBeVisible();
  await expect(page.getByText("Something went wrong on this screen.")).toHaveCount(0);
});
