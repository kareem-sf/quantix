import { afterEach, beforeEach, vi } from "vitest";
import { connect } from "./api";

const native = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({
  isTauri: () => true,
  invoke: native.invoke,
}));
beforeEach(() => {
  native.invoke.mockReset().mockResolvedValue({
    base_url: "http://127.0.0.1:18001/api",
    token: "synthetic-session",
  });
});
afterEach(() => {
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

it("turns a native string rejection into an actionable Error without echoing its payload", async () => {
  native.invoke.mockRejectedValue("untrusted-native-payload");
  await expect(connect()).rejects.toThrow(
    "The local service connection is not ready. Quantix will try again automatically.",
  );
});

it("waits for authenticated health before returning the API and performs no mutations", async () => {
  let ready!: (response: Response) => void;
  const fetcher = vi.fn(
    () =>
      new Promise<Response>((resolve) => {
        ready = resolve;
      }),
  );
  vi.stubGlobal("fetch", fetcher);
  let connected = false;
  const pending = connect().then(() => {
    connected = true;
  });
  await Promise.resolve();
  expect(connected).toBe(false);
  expect(fetcher).toHaveBeenCalledWith(
    "http://127.0.0.1:18001/api/health",
    expect.objectContaining({
      method: "GET",
      headers: expect.objectContaining({
        Authorization: "Bearer synthetic-session",
      }),
    }),
  );
  ready(Response.json({ reset_pending: true }));
  await pending;
  expect(connected).toBe(true);
  expect(fetcher).toHaveBeenCalledTimes(1);
});

it("rereads the current native session on retry after an unavailable service", async () => {
  const fetcher = vi
    .fn()
    .mockResolvedValueOnce(Response.json({}, { status: 503 }))
    .mockResolvedValueOnce(Response.json({}));
  vi.stubGlobal("fetch", fetcher);
  await expect(connect()).rejects.toThrow(
    "The local service is not responding yet.",
  );
  native.invoke.mockResolvedValue({
    base_url: "http://127.0.0.1:18002/api",
    token: "new-synthetic-session",
  });
  await connect();
  expect(native.invoke).toHaveBeenCalledTimes(2);
  expect(fetcher.mock.calls[1][0]).toBe("http://127.0.0.1:18002/api/health");
});

it("bounds a stalled health request so startup can retry", async () => {
  vi.useFakeTimers();
  vi.stubGlobal(
    "fetch",
    vi.fn(
      (_url, options: RequestInit) =>
        new Promise<Response>((_resolve, reject) => {
          options.signal?.addEventListener(
            "abort",
            () => reject(new DOMException("Aborted", "AbortError")),
            { once: true },
          );
        }),
    ),
  );
  const pending = connect();
  const failure = expect(pending).rejects.toThrow(
    "The local service is not responding yet.",
  );
  await vi.advanceTimersByTimeAsync(3000);
  await failure;
});
