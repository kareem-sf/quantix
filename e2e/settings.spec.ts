import { createServer, type IncomingMessage, type Server } from "node:http";
import type { AddressInfo } from "node:net";
import { expect, test } from "@playwright/test";

const KEY = "sk-synthetic-e2e-7c1d";

/** A stand-in for an OpenAI-compatible service on this computer: it lists one model, calls the tool a check asks
 * for, and can't read images. Quantix never reaches a real AI service from these tests. */
function fakeService() {
  const keys: string[] = [];
  const server = createServer(async (request, response) => {
    keys.push(request.headers.authorization ?? "");
    const reply = (body: unknown) => {
      response.writeHead(200, { "Content-Type": "application/json" });
      response.end(JSON.stringify(body));
    };
    if (request.url === "/v1/models") {
      return reply({ object: "list", data: [{ id: "synthetic-model", object: "model", created: 0, owned_by: "e2e" }] });
    }
    const body = JSON.parse(await text(request));
    const code = /The code is (\w+)\./.exec(JSON.stringify(body.messages))?.[1];
    const call = { id: "call_1", type: "function", function: { name: "confirm", arguments: JSON.stringify({ code }) } };
    const message = code && body.tools ? { role: "assistant", content: null, tool_calls: [call] } : { role: "assistant", content: "I can't see images." };
    reply({
      id: "chatcmpl-e2e",
      object: "chat.completion",
      created: 0,
      model: body.model,
      choices: [{ index: 0, message, finish_reason: code ? "tool_calls" : "stop" }],
      usage: { prompt_tokens: 1, completion_tokens: 1, total_tokens: 2 },
    });
  });
  return { server, keys };
}

function text(request: IncomingMessage) {
  return new Promise<string>((resolve) => {
    let body = "";
    request.on("data", (chunk) => (body += chunk));
    request.on("end", () => resolve(body));
  });
}

let fake: { server: Server; keys: string[] };
test.beforeAll(async () => {
  fake = fakeService();
  await new Promise<void>((resolve) => fake.server.listen(0, "127.0.0.1", resolve));
});
test.afterAll(() => fake.server.close());

test("an AI connection is added, checked and given to the office, and its key is never shown", async ({ page }) => {
  const address = `http://127.0.0.1:${(fake.server.address() as AddressInfo).port}/v1`;
  await page.goto("/settings?section=ai");

  await page.getByRole("radio", { name: "OpenAI-compatible service" }).click();
  await page.getByLabel("Service address").fill(address);
  await page.getByLabel("API key").fill(KEY);
  await page.getByRole("button", { name: "Add connection" }).click();

  await expect(page.getByText("Key …7c1d")).toBeVisible();
  await expect(page.getByLabel("API key")).toHaveValue("");
  expect(fake.keys).toContain(`Bearer ${KEY}`);
  const listed = await page.request.get("/api/ai/connections");
  expect(await listed.text()).not.toContain(KEY);
  await expect(page.locator("body")).not.toContainText(KEY);

  await page.getByRole("combobox", { name: "Model" }).selectOption("synthetic-model");
  await page.getByRole("button", { name: "Check" }).click();
  await expect(page.getByText(/Works, including the tools the office needs\. It can't read images/)).toBeVisible();

  await page.getByLabel("Office AI").selectOption({ label: "synthetic-model · 127.0.0.1" });
  await page.reload();
  await expect(page.getByLabel("Office AI")).toHaveValue(/\|synthetic-model$/);

  // leaves the office without an AI, as the other tests expect
  await page.getByRole("button", { name: "Remove" }).click();
  await expect(page.getByText("Add an AI connection so the office can start work.")).toBeVisible();
  expect((await (await page.request.get("/api/settings")).json()).office_ai).toBeNull();
});

test("how the office works and what it notifies are kept", async ({ page }) => {
  // each choice shows once the service has kept it
  const choose = async (name: RegExp) => {
    await page.getByRole("radio", { name }).click();
    await expect(page.getByRole("radio", { name })).toBeChecked();
  };
  await page.goto("/settings");
  await choose(/Fully autonomous/);
  await choose(/Decisions only/);

  await page.reload();
  await expect(page.getByRole("radio", { name: /Fully autonomous/ })).toBeChecked();
  await expect(page.getByRole("radio", { name: /Decisions only/ })).toBeChecked();

  await choose(/Engineer in the loop/);
  await page.reload();
  await expect(page.getByRole("radio", { name: /Engineer in the loop/ })).toBeChecked();
});

test("the AI allowance per tender is kept, and cleared to no limit", async ({ page }) => {
  await page.goto("/settings?section=usage");
  const allowance = page.getByLabel("Allowance in million tokens");
  await allowance.fill("2.5");
  await page.getByRole("button", { name: "Save" }).click();
  await page.reload();
  await expect(allowance).toHaveValue("2.5");

  await allowance.fill("");
  await page.getByRole("button", { name: "Save" }).click();
  await page.reload();
  await expect(allowance).toHaveValue("");
});
