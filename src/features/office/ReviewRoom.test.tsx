import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { ApiContext, type Api, type Schema } from "../../api";
import { ReviewRoom } from "./ReviewRoom";

type ReviewSession = Schema<"ReviewSession">;

function session(partial: Partial<ReviewSession> = {}): ReviewSession {
  return {
    id: "review-1",
    subject_id: "estimate-1",
    author_conclusion_hidden: true,
    findings: [
      {
        id: "finding-1",
        topic: "calculation",
        agreed: true,
        detail:
          "The recorded product calculation reproduces 100 from its saved typed inputs.",
      },
    ],
    calculation_id: "calculation-1",
    calculation_method_id: "product",
    calculation_method_version: "synthetic-area-v1",
    checked_basis_fingerprint: "fp-1",
    status: "checked",
    resolution: null,
    resolved_at: null,
    limitations: [],
    created_at: "2026-09-10T08:05:00Z",
    ...partial,
  };
}

function renderRoom(api: Api, onDecide = vi.fn()) {
  return render(
    <ApiContext.Provider value={api}>
      <ReviewRoom tenderId="tender-a" onDecide={onDecide} />
    </ApiContext.Provider>,
  );
}

describe("ReviewRoom", () => {
  it("lists reviews and opens the independent findings without approval language", async () => {
    const api = {
      get: vi.fn().mockResolvedValue([session()]),
      post: vi.fn(),
    } as unknown as Api;
    renderRoom(api);

    expect(await screen.findByText("estimate-1 · Checked")).toBeInTheDocument();
    await userEvent.click(
      screen.getByRole("button", { name: "estimate-1 · Checked" }),
    );
    expect(screen.getByText("Author conclusion hidden")).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /approve/i }),
    ).not.toBeInTheDocument();
    expect(api.get).toHaveBeenCalledWith(
      "/tenders/tender-a/reviews",
      expect.any(AbortSignal),
    );
  });

  it("filters by status and retries a failed read", async () => {
    const api = {
      get: vi
        .fn()
        .mockRejectedValueOnce(new Error("Reviews unavailable."))
        .mockResolvedValueOnce([session({ status: "needs_review" })]),
      post: vi.fn(),
    } as unknown as Api;
    renderRoom(api);

    expect(await screen.findByText("Reviews unavailable.")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Needs review" }));
    expect(
      await screen.findByText("estimate-1 · Needs review"),
    ).toBeInTheDocument();
    expect(api.get).toHaveBeenLastCalledWith(
      "/tenders/tender-a/reviews?status=needs_review",
      expect.any(AbortSignal),
    );
  });

  it("records a discussion finding and resolves without granting commercial effect", async () => {
    const current = session();
    const api = {
      get: vi.fn().mockResolvedValue([current]),
      post: vi.fn().mockImplementation((url: string) => {
        if (url.endsWith("/contributions")) {
          current.findings = [
            ...current.findings,
            {
              id: `finding-${current.findings.length + 1}`,
              topic: "deduction rule",
              agreed: false,
              detail: "The deduction rule is not approved.",
            },
          ];
        }
        if (url.endsWith("/resolve")) {
          current.resolution =
            "Deduction rule needs an engineer decision before pricing.";
        }
        return Promise.resolve(current);
      }),
    } as unknown as Api;
    const onDecide = vi.fn();
    renderRoom(api, onDecide);

    await userEvent.click(
      await screen.findByRole("button", { name: "estimate-1 · Checked" }),
    );
    await userEvent.type(
      screen.getByPlaceholderText("Measured area"),
      "Deduction rule",
    );
    await userEvent.type(
      screen.getByLabelText("Finding detail"),
      "The deduction rule is not approved.",
    );
    await userEvent.click(screen.getByRole("button", { name: "Add finding" }));
    expect(api.post).toHaveBeenCalledWith(
      "/tenders/tender-a/reviews/review-1/contributions",
      expect.objectContaining({ topic: "Deduction rule", agreed: true }),
    );

    await userEvent.click(screen.getByLabelText("Agreed"));
    await userEvent.click(screen.getByRole("button", { name: "Add finding" }));
    expect(api.post).toHaveBeenLastCalledWith(
      "/tenders/tender-a/reviews/review-1/contributions",
      expect.objectContaining({ agreed: false }),
    );

    await userEvent.type(
      screen.getByPlaceholderText(
        "What was decided and what still needs an engineer decision",
      ),
      "Deduction rule needs an engineer decision before pricing.",
    );
    await userEvent.click(
      screen.getByRole("button", { name: "Resolve review" }),
    );
    expect(api.post).toHaveBeenCalledWith(
      "/tenders/tender-a/reviews/review-1/resolve",
      expect.objectContaining({
        resolution: "Deduction rule needs an engineer decision before pricing.",
      }),
    );
    await userEvent.click(
      screen.getByRole("button", { name: "Record engineer decision" }),
    );
    expect(onDecide).toHaveBeenCalledOnce();
  });
});
