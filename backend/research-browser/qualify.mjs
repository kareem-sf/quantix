// Runs inside the actual networkless browser container, against the real proxy.
import assert from "node:assert/strict";
import fs from "node:fs";
import net from "node:net";
import os from "node:os";
import { createRequire } from "node:module";
import { chromium } from "playwright";
import { proxyCall, waitForProxy } from "./public-proxy.mjs";

const socketPath = process.argv[2];
assert.equal(process.getuid(), 1000);
assert.deepEqual(Object.keys(os.networkInterfaces()), ["lo"]);
assert.equal(fs.existsSync("/mnt/c"), false);
assert.equal(fs.existsSync("/run/podman/podman.sock"), false);
const status = fs.readFileSync("/proc/self/status", "utf8");
assert.match(status, /CapEff:\s+0000000000000000/);
assert.match(status, /NoNewPrivs:\s+1/);
assert.equal(fs.readFileSync("/sys/fs/cgroup/memory.max", "utf8").trim(), "805306368");
assert.equal(fs.readFileSync("/sys/fs/cgroup/pids.max", "utf8").trim(), "128");
const [quota, period] = fs.readFileSync("/sys/fs/cgroup/cpu.max", "utf8").trim().split(/\s+/).map(Number);
assert.equal(quota / period, 1);
assert.throws(() => fs.writeFileSync("/proxy/renderer-write-attempt", "denied"));
assert.ok(fs.readFileSync("/proc/self/mountinfo", "utf8").split("\n").some(line => {
  const fields = line.split(" "); return fields[4] === "/" && fields[5]?.split(",").includes("ro");
}));
const directTargets = ["10.0.2.2", "192.168.1.1", "169.254.169.254", "1.1.1.1"];
for (const host of directTargets) {
  const connected = await new Promise(resolve => {
    const socket = net.connect({ host, port: 443 });
    socket.once("connect", () => { socket.destroy(); resolve(true); });
    socket.once("error", () => resolve(false));
    socket.setTimeout(300, () => { socket.destroy(); resolve(false); });
  });
  assert.equal(connected, false);
}
await waitForProxy(socketPath);
const rejectedTargets = ["http://127.0.0.1/", "http://169.254.169.254/", "https://other.example/", "http://example.com:443/"];
for (const url of rejectedTargets) assert.equal((await proxyCall(socketPath, url)).denied, true);
assert.equal((await proxyCall(socketPath, "https://example.com/", "POST")).denied, true);
const allowed = await proxyCall(socketPath, "https://example.com/");
assert.equal(allowed.denied, undefined);
assert.equal(allowed.status, 200);
const allowedBytes = Buffer.from(allowed.body, "base64").length;
assert.ok(allowedBytes > 0 && allowedBytes <= 2 * 1024 * 1024);
const browser = await chromium.launch({ headless: true });
try {
  const page = await browser.newPage();
  await page.setContent("<main id='proof'></main><script>document.querySelector('#proof').textContent='Quantix isolated reader';</script>");
  assert.equal(await page.locator("main").innerText(), "Quantix isolated reader");
} finally { await browser.close(); }
const require = createRequire(import.meta.url);
process.stdout.write(JSON.stringify({ qualified: true, playwright: require("playwright/package.json").version,
  networkless_renderer: true, direct_egress_denied: directTargets, gateway_rejections: rejectedTargets,
  remote_post_denied: true, local_chromium_rendered: true, uid: process.getuid(),
  gateway_get_url: "https://example.com/", gateway_get_status: allowed.status,
  gateway_get_body_bytes: allowedBytes, gateway_tls_verified: true }));
