import assert from "node:assert/strict";
import test from "node:test";
import { publicIPv4, validateTarget } from "./public-proxy.mjs";

test("private and reserved addresses cannot become proxy targets", () => {
  for (const value of ["127.0.0.1", "10.0.2.2", "172.16.0.1", "192.168.1.1", "169.254.169.254", "100.64.0.1", "203.0.113.1", "::1"])
    assert.equal(publicIPv4(value), false, value);
  assert.equal(publicIPv4("1.1.1.1"), true);
});

test("proxy enforces exact host port and read method", () => {
  const allowed = ["example.test", "1.1.1.1", 443, "https:"];
  assert.equal(validateTarget("https://example.test/page", "GET", ...allowed).hostname, "example.test");
  for (const url of ["https://127.0.0.1/", "https://other.test/", "http://example.test/", "http://example.test:443/", "https://example.test:8443/", "https://user:secret@example.test/", "file:///etc/passwd"])
    assert.throws(() => validateTarget(url, "GET", ...allowed));
  for (const method of ["POST", "PUT", "DELETE", "CONNECT", "OPTIONS"])
    assert.throws(() => validateTarget("https://example.test/", method, ...allowed));
});

test("an HTTP target cannot upgrade scheme on the same port", () => {
  assert.throws(() => validateTarget("https://example.test:80/", "GET", "example.test", "1.1.1.1", 80, "http:"));
});
