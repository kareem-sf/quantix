import { describe, expect, it, vi } from "vitest";
import { createApi } from "./api";

describe("local API transport", () => {
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
