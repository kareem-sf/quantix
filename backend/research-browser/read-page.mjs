import { chromium } from "playwright";
import { proxyCall, waitForProxy } from "./public-proxy.mjs";

const [requestedUrl, expectedHost, _checkedAddress, maxTextRaw, socketPath] = process.argv.slice(2);
const maxText = Number(maxTextRaw);
if (!requestedUrl || !expectedHost || !Number.isSafeInteger(maxText) || maxText < 1 || maxText > 400000 || socketPath !== "/proxy/reader.sock") process.exit(2);
await waitForProxy(socketPath);
const browser = await chromium.launch({ headless: true });
try {
  const context = await browser.newContext({ acceptDownloads: false, serviceWorkers: "block" });
  await context.routeWebSocket("**/*", route => route.close());
  await context.route("**/*", async route => {
    try {
      const request = route.request();
      const target = new URL(request.url());
      if (!["http:", "https:"].includes(target.protocol) || target.hostname !== expectedHost || !["GET", "HEAD"].includes(request.method())) {
        await route.abort("blockedbyclient"); return;
      }
      const result = await proxyCall(socketPath, target.href, request.method());
      if (result.denied) { await route.abort("blockedbyclient"); return; }
      await route.fulfill({ status: result.status, headers: result.headers, body: Buffer.from(result.body, "base64") });
    } catch { await route.abort("blockedbyclient"); }
  });
  const page = await context.newPage();
  const response = await page.goto(requestedUrl, { waitUntil: "domcontentloaded", timeout: 30000 });
  await page.waitForLoadState("networkidle", { timeout: 5000 }).catch(() => undefined);
  const result = await page.evaluate(limit => ({ final_url: location.href, title: document.title.slice(0, 300),
    text: (document.body?.innerText || "").slice(0, limit), html: document.documentElement.outerHTML.slice(0, limit) }), maxText);
  result.status_code = response?.status() ?? 0;
  if (result.status_code < 200 || result.status_code >= 400) throw new Error("Public navigation failed");
  process.stdout.write(JSON.stringify(result));
} finally { await browser.close(); }
