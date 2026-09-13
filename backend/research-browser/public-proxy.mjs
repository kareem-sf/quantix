// Trusted read-only HTTP gateway. No browser/page JavaScript runs in this process.
import fs from "node:fs";
import http from "node:http";
import https from "node:https";
import net from "node:net";
import { pathToFileURL } from "node:url";

export function publicIPv4(address) {
  if (net.isIP(address) !== 4) return false;
  const [a, b, c] = address.split(".").map(Number);
  return !(a === 0 || a === 10 || a === 127 || a >= 224 ||
    (a === 100 && b >= 64 && b <= 127) || (a === 169 && b === 254) ||
    (a === 172 && b >= 16 && b <= 31) || (a === 192 && (b === 168 || b === 0)) ||
    (a === 198 && (b === 18 || b === 19 || (b === 51 && c === 100))) ||
    (a === 203 && b === 0 && c === 113));
}

export function validateTarget(raw, method, expectedHost, checkedAddress, checkedPort, checkedScheme) {
  if (!publicIPv4(checkedAddress) || !["GET", "HEAD"].includes(method) || typeof raw !== "string" || raw.length > 8192)
    throw new Error("Blocked public request");
  const url = new URL(raw);
  const port = Number(url.port || (url.protocol === "https:" ? 443 : 80));
  if (!["http:", "https:"].includes(checkedScheme) || url.protocol !== checkedScheme || url.hostname !== expectedHost ||
      port !== checkedPort || url.username || url.password)
    throw new Error("Blocked public request");
  return url;
}

export function createPublicProxy(expectedHost, checkedAddress, checkedPort, checkedScheme) {
  if (!/^[a-z0-9][a-z0-9.-]*$/i.test(expectedHost) || !publicIPv4(checkedAddress) ||
      !Number.isInteger(checkedPort) || checkedPort < 1 || checkedPort > 65535 || !["http:", "https:"].includes(checkedScheme))
    throw new Error("Invalid checked destination");
  let requestCount = 0, totalBytes = 0, active = 0;
  const maxBody = 2 * 1024 * 1024, maxTotal = 8 * 1024 * 1024;
  const server = http.createServer(async (request, response) => {
    if (request.method === "GET" && request.url === "/ready") { response.writeHead(204).end(); return; }
    if (request.method !== "POST" || request.url !== "/fetch") { response.writeHead(405).end(); return; }
    if (++requestCount > 40 || active >= 16) { response.writeHead(429).end(); return; }
    active++;
    try {
      let raw = Buffer.alloc(0);
      for await (const chunk of request) {
        if (raw.length + chunk.length > 32768) throw new Error("Request limit");
        raw = Buffer.concat([raw, chunk]);
      }
      const data = JSON.parse(raw.toString("utf8"));
      const target = validateTarget(data.url, data.method, expectedHost, checkedAddress, checkedPort, checkedScheme);
      const upstream = await new Promise((resolve, reject) => {
        const transport = target.protocol === "https:" ? https : http;
        const connection = transport.request({
          protocol: target.protocol, hostname: expectedHost, port: checkedPort,
          path: target.pathname + target.search, method: data.method,
          servername: expectedHost, rejectUnauthorized: true,
          lookup: (_host, options, callback) => options?.all
            ? callback(null, [{ address: checkedAddress, family: 4 }])
            : callback(null, checkedAddress, 4),
          headers: { "user-agent": "QuantixPublicReader/2", accept: "*/*" },
        }, incoming => {
          const chunks = []; let size = 0;
          incoming.on("data", chunk => {
            size += chunk.length; totalBytes += chunk.length;
            if (size > maxBody || totalBytes > maxTotal) { connection.destroy(new Error("Response limit")); return; }
            chunks.push(chunk);
          });
          incoming.on("error", reject);
          incoming.on("end", () => {
            try {
            const headers = {};
            for (const [key, value] of Object.entries(incoming.headers)) {
              if (!["content-length", "transfer-encoding", "connection", "set-cookie", "upgrade"].includes(key) && typeof value === "string") headers[key] = value;
            }
            if (headers.location) validateTarget(new URL(headers.location, target).href, "GET", expectedHost, checkedAddress, checkedPort, checkedScheme);
            resolve({ status: incoming.statusCode, headers, body: Buffer.concat(chunks).toString("base64") });
            } catch (error) { reject(error); }
          });
        });
        connection.setTimeout(20000, () => connection.destroy(new Error("Public request timeout")));
        connection.on("error", reject);
        connection.end();
      });
      response.writeHead(200, { "content-type": "application/json" });
      response.end(JSON.stringify(upstream));
    } catch {
      response.writeHead(403, { "content-type": "application/json" });
      response.end('{"error":"The request was denied or the bounded public response failed."}');
    } finally { active--; }
  });
  server.on("connect", (_request, socket) => socket.end("HTTP/1.1 405 Method Not Allowed\r\n\r\n"));
  server.on("upgrade", (_request, socket) => socket.destroy());
  server.maxConnections = 20;
  server.headersTimeout = 5000;
  server.requestTimeout = 25000;
  server.keepAliveTimeout = 1000;
  return server;
}

export async function proxyCall(socketPath, url, method = "GET") {
  const body = JSON.stringify({ url, method });
  return await new Promise((resolve, reject) => {
    const request = http.request({ socketPath, path: "/fetch", method: "POST",
      headers: { "content-type": "application/json", "content-length": Buffer.byteLength(body) } }, response => {
      const chunks = []; let size = 0;
      response.on("data", chunk => { size += chunk.length; if (size > 3_000_000) request.destroy(new Error("Proxy response limit")); else chunks.push(chunk); });
      response.on("error", reject);
      response.on("end", () => {
        if (response.statusCode !== 200) { resolve({ denied: true, status: response.statusCode }); return; }
        try { resolve(JSON.parse(Buffer.concat(chunks).toString("utf8"))); } catch (error) { reject(error); }
      });
    });
    request.setTimeout(25000, () => request.destroy(new Error("Proxy timeout")));
    request.on("error", reject); request.end(body);
  });
}

export async function waitForProxy(socketPath) {
  for (let attempt = 0; attempt < 50; attempt++) {
    try {
      await new Promise((resolve, reject) => {
        const request = http.get({ socketPath, path: "/ready" }, response => { response.resume(); response.statusCode === 204 ? resolve() : reject(new Error("Not ready")); });
        request.setTimeout(500, () => request.destroy(new Error("Not ready"))); request.on("error", reject);
      }); return;
    } catch { await new Promise(resolve => setTimeout(resolve, 100)); }
  }
  throw new Error("The constrained public gateway did not become ready.");
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const [socketPath, host, address, port, scheme] = process.argv.slice(2);
  if (socketPath !== "/proxy/reader.sock") throw new Error("Invalid gateway socket path");
  const server = createPublicProxy(host, address, Number(port), scheme);
  server.listen(socketPath, () => fs.chmodSync(socketPath, 0o666));
}
