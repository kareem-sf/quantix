// Real Quantix UI + isolated synthetic backend. No production accounts or data.
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import path from "node:path";
import os from "node:os";
import { chromium } from "@playwright/test";

const output = path.join(
  os.homedir(),
  ".quantix/runtime/verification/activity-2026-09-13",
);
const fixture = JSON.parse(
  await fs.readFile(path.join(output, "manifest.json"), "utf8"),
);
assert.equal(fixture.synthetic_only, true);
process.env.TEMP = output;
process.env.TMP = output;
const browser = await chromium.launch({
  executablePath: "C:/Program Files/Google/Chrome/Application/chrome.exe",
  headless: true,
});
const errors = [];
const checks = [];
const api = async (suffix) => {
  const response = await fetch(`${fixture.api_base_url}${suffix}`, {
    headers: { Authorization: `Bearer ${fixture.token}` },
  });
  assert.equal(response.status, 200, suffix);
  return response.json();
};
const url = `http://127.0.0.1:1421/#/tenders/${fixture.tender_id}/manager`;
try {
  const context = await browser.newContext({
    viewport: { width: 1440, height: 1000 },
  });
  const page = await context.newPage();
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto(url);
  const composer = page.getByRole("textbox", {
    name: "Message to Tender Manager",
  });
  await composer.fill(
    "Review the synthetic concrete specification and show the source.",
  );
  const admission = page.waitForResponse(
    (response) =>
      response.request().method() === "POST" &&
      response.url().endsWith("/messages"),
  );
  await page
    .getByRole("button", { name: "Send instruction", exact: true })
    .click();
  const response = await admission;
  assert.equal(response.status(), 200);
  const submitted = await response.json();
  const runId =
    submitted.run?.id ?? submitted.run_id ?? submitted.message?.run_id;
  assert.ok(runId, JSON.stringify(submitted));
  const timeline = page
    .getByRole("region", { name: "Work activity", exact: true })
    .last();
  await timeline.waitFor();
  await timeline.getByText("Connected · checking live work").waitFor();
  await page.screenshot({ path: path.join(output, "activity-live.png") });
  checks.push(
    "Actual composer POST creates a run and visible activity before final answer",
  );
  let run;
  for (let index = 0; index < 80; index++) {
    run = await api(`/runs/${runId}`);
    if (!["queued", "running"].includes(run.status)) break;
    await page.waitForTimeout(500);
  }
  assert.equal(run.status, "completed", JSON.stringify(run));
  await timeline.getByText("Saved history", { exact: true }).waitFor();
  await timeline
    .getByRole("button", { name: "Inspect activity", exact: true })
    .click();
  const dialog = page.getByRole("dialog");
  await dialog
    .getByRole("searchbox", { name: "Search activity" })
    .fill("read_source");
  await dialog
    .getByRole("button", { name: /Open details: Finished read_source/ })
    .waitFor();
  await dialog
    .getByRole("button", { name: /Open details: Finished read_source/ })
    .click();
  await dialog.getByRole("button", { name: "Inputs", exact: true }).click();
  await dialog.getByText(fixture.source_id, { exact: false }).first().waitFor();
  await dialog.getByRole("button", { name: "Outcome", exact: true }).click();
  await dialog.locator("pre").filter({ hasText: "C35" }).waitFor();
  checks.push(
    "Tool details expose exact inputs, returned source content, and outcome",
  );
  await page.screenshot({
    path: path.join(output, "activity-tool-detail.png"),
  });
  await dialog.getByRole("button", { name: "Back to activity" }).click();
  await dialog
    .getByRole("searchbox", { name: "Search activity" })
    .fill("no-such-activity-match");
  await dialog
    .getByText("No captured activity matches these filters.")
    .waitFor();
  await page.keyboard.press("Escape");
  await timeline.getByText("Saved history", { exact: true }).waitFor();
  await page.reload();
  await page
    .getByRole("region", { name: "Work activity", exact: true })
    .last()
    .waitFor();
  checks.push("Completed history survives closing inspector and reload");
  await context.close();

  for (const theme of ["light", "dark"]) {
    for (const scale of [1, 1.25, 1.5, 2]) {
      const context = await browser.newContext({
        viewport: {
          width: Math.floor(1440 / scale),
          height: Math.floor(1000 / scale),
        },
        deviceScaleFactor: scale,
      });
      await context.addInitScript(
        (theme) => localStorage.setItem("quantix-theme", theme),
        theme,
      );
      const page = await context.newPage();
      page.on("pageerror", (error) => errors.push(error.message));
      await page.goto(url);
      const timeline = page
        .getByRole("region", { name: "Work activity", exact: true })
        .last();
      await timeline.waitFor();
      await timeline
        .getByRole("button", { name: "Inspect activity", exact: true })
        .click();
      const dialog = page.getByRole("dialog");
      await dialog
        .getByRole("searchbox", { name: "Search activity" })
        .waitFor();
      await dialog
        .getByRole("combobox", { name: "Filter by category" })
        .selectOption("tool");
      await dialog
        .getByRole("button", { name: /Open details:/ })
        .first()
        .waitFor();
      const fits = await dialog.evaluate((node) => ({
        scroll: node.scrollWidth,
        width: node.clientWidth,
        rect: node.getBoundingClientRect().toJSON(),
        viewport: innerWidth,
      }));
      assert.ok(fits.scroll <= fits.width + 2, JSON.stringify(fits));
      assert.ok(
        fits.rect.x >= -2 && fits.rect.right <= fits.viewport + 2,
        JSON.stringify(fits),
      );
      if (fits.viewport < 1024)
        assert.ok(
          fits.rect.x <= 2,
          "Narrow-screen inspector must fill the viewport",
        );
      await page.screenshot({
        path: path.join(output, `activity-${theme}-${scale * 100}.png`),
      });
      checks.push(
        `${theme}, ${scale * 100}% effective display scaling: filter, responsive inspector and overflow checks`,
      );
      await context.close();
    }
  }
  assert.deepEqual(errors, []);
  await fs.writeFile(
    path.join(output, "ui-results.json"),
    JSON.stringify(
      {
        run_id: runId,
        checks,
        errors,
        scaling:
          "Effective viewport and device scale factor emulation in Chrome",
        production_inference: false,
      },
      null,
      2,
    ),
  );
  console.log(JSON.stringify({ runId, checks, errors }, null, 2));
} catch (error) {
  for (const context of browser.contexts())
    for (const page of context.pages()) {
      console.log((await page.locator("body").innerText()).slice(-9000));
      await page.screenshot({
        path: path.join(output, "activity-qa-failure.png"),
      });
    }
  throw error;
} finally {
  await browser.close();
}
