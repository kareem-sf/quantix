import { describe, expect, it, vi } from "vitest";
import { createApi } from "./api";

describe("local API transport", () => {
  it("preserves intentional cancellation when the browser rejects an aborted read as TypeError", async () => {
    const controller = new AbortController();
    const api = createApi(
      { base_url: "http://localhost/api", token: "test" },
      async () => {
        controller.abort();
        throw new TypeError("Failed to fetch");
      },
    );
    await expect(
      api.get("/tenders/one/office", controller.signal),
    ).rejects.toMatchObject({ name: "AbortError" });
  });
  it("redacts a credential echoed by a validator without keeping input or context", async () => {
    const secret = "private-key-value";
    const api = createApi(
      { base_url: "http://localhost/api", token: "test" },
      async () =>
        Response.json(
          {
            detail: [
              {
                loc: ["body", "connection", "credentials", "api_key"],
                msg: `Value error, invalid key: ${secret}`,
                input: secret,
                ctx: { error: secret },
              },
            ],
          },
          { status: 422 },
        ),
    );
    try {
      await api.post("/ai/setup/configure", {
        connection: { credentials: { api_key: secret } },
      });
    } catch (failure) {
      expect((failure as Error).message).toContain("[hidden]");
      expect(JSON.stringify(failure)).not.toContain(secret);
      expect(JSON.stringify(failure)).not.toContain("input");
    }
  });
  it("sends the pending instruction identity and revision with DELETE", async () => {
    const body = { pending_id: "pending-one", expected_revision: 3 };
    const fetcher = vi.fn(
      async (_url: RequestInfo | URL, init?: RequestInit) => {
        expect(init?.method).toBe("DELETE");
        expect(new Headers(init?.headers).get("Content-Type")).toBe(
          "application/json",
        );
        expect(JSON.parse(String(init?.body))).toEqual(body);
        return new Response(JSON.stringify({ cancelled: true }));
      },
    );
    const api = createApi(
      { base_url: "http://localhost/api", token: "test" },
      fetcher,
    );
    await expect(api.delete("/tenders/one/pending", body)).resolves.toEqual({
      cancelled: true,
    });
  });
  it("shows validation messages without exposing submitted secret values", async () => {
    const api = createApi(
      { base_url: "http://localhost/api", token: "test" },
      async () =>
        new Response(
          JSON.stringify({
            detail: [
              {
                loc: ["body", "smtp_password"],
                msg: "String should have at least 8 characters",
                input: "private-password-value",
                ctx: { input: "private-password-value" },
              },
            ],
          }),
          { status: 422 },
        ),
    );
    let failure: unknown;
    try {
      await api.patch("/settings", { smtp_password: "private-password-value" });
    } catch (error) {
      failure = error;
    }
    expect(failure).toBeInstanceOf(Error);
    expect((failure as Error).message).toContain(
      "String should have at least 8 characters",
    );
    expect((failure as Error).message).not.toContain("private-password-value");
    expect((failure as { fieldErrors?: unknown[] }).fieldErrors).toEqual([
      {
        path: ["body", "smtp_password"],
        message: "String should have at least 8 characters",
      },
    ]);
  });

  it("keeps safe validation paths for multiple fields and drops input/context", async () => {
    const api = createApi(
      { base_url: "http://localhost/api", token: "test" },
      async () =>
        new Response(
          JSON.stringify({
            detail: [
              {
                loc: ["body", "currency"],
                msg: "Use three letters",
                input: "EGP",
              },
              {
                loc: ["body", "credentials", "api_key"],
                msg: "Invalid key",
                input: "secret",
                ctx: { secret: "secret" },
              },
            ],
          }),
          { status: 422 },
        ),
    );
    let failure: unknown;
    try {
      await api.post("/tenders/one/estimate", {
        currency: "EGP",
        credentials: { api_key: "secret" },
      });
    } catch (error) {
      failure = error;
    }
    expect((failure as { fieldErrors?: unknown[] }).fieldErrors).toEqual([
      { path: ["body", "currency"], message: "Use three letters" },
      { path: ["body", "credentials", "api_key"], message: "Invalid key" },
    ]);
    expect(JSON.stringify(failure)).not.toContain("secret");
  });
  it("explains an unavailable local service without hiding the failure", async () => {
    const api = createApi(
      { base_url: "http://localhost/api", token: "test" },
      async () => {
        throw new TypeError("Failed to fetch");
      },
    );
    await expect(api.get("/tenders")).rejects.toThrow(
      "The local service could not be reached. Reopen Quantix and try again.",
    );
  });
  it("authenticates JSON calls and preserves a server failure message", async () => {
    const fetcher = vi.fn(
      async (_url: RequestInfo | URL, init?: RequestInit) => {
        expect(new Headers(init?.headers).get("Authorization")).toBe(
          "Bearer session-token",
        );
        return new Response(
          JSON.stringify({ detail: "Provider rejected this request." }),
          { status: 400 },
        );
      },
    );
    const api = createApi(
      { base_url: "http://127.0.0.1:8090/api", token: "session-token" },
      fetcher,
    );
    await expect(
      api.post("/tenders/one/messages", { content: "Review scope" }),
    ).rejects.toThrow("Provider rejected this request.");
  });

  it("authenticates source preview requests and returns a blob", async () => {
    const api = createApi(
      { base_url: "http://127.0.0.1:8090/api/", token: "preview-token" },
      async (url, init) => {
        expect(String(url)).toBe(
          "http://127.0.0.1:8090/api/tenders/one/artifacts/doc/preview?page=3",
        );
        expect(new Headers(init?.headers).get("Authorization")).toBe(
          "Bearer preview-token",
        );
        return new Response("png content", {
          headers: { "Content-Type": "image/png" },
        });
      },
    );
    expect(
      (await api.blob("/tenders/one/artifacts/doc/preview?page=3")).type,
    ).toBe("image/png");
  });
});
