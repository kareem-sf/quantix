import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { ApiContext, type Api, type Schema } from "../../api";
import { StaffHandoffs } from "./StaffHandoffs";

type HandoffView = Schema<"HandoffView">;

function view(partial: Partial<HandoffView> & { id: string }): HandoffView {
  return {
    direction: "received",
    counterpart_staff_id: "staff-sender",
    counterpart_display_name: "Samir Fares",
    ...partial,
    handoff: {
      id: partial.id,
      tender_id: "tender-a",
      sender_staff_id: "staff-sender",
      recipient_staff_id: "staff-1",
      result_id: "result-1",
      purpose: "Hand the measured table to the recipient.",
      basis_fingerprint: "fp-1",
      applicability: "current",
      created_at: "2026-09-10T08:05:00Z",
      ...(partial.handoff ?? {}),
    },
  };
}

function renderHandoffs(api: Api) {
  return render(
    <ApiContext.Provider value={api}>
      <StaffHandoffs tenderId="tender-a" staffId="staff-1" />
    </ApiContext.Provider>,
  );
}

describe("StaffHandoffs", () => {
  it("lists handoffs and pages exact rows without rewriting values", async () => {
    const item = view({ id: "handoff-1" });
    const api = {
      get: vi.fn().mockImplementation((url: string) => {
        if (url.includes("/handoffs?")) {
          return Promise.resolve({
            items: [item],
            next_cursor: null,
            total: 1,
          });
        }
        const params = new URLSearchParams(url.split("?")[1] ?? "");
        const offset = Number(params.get("offset") ?? "0");
        return Promise.resolve({
          handoff: item.handoff,
          items: [{ row: offset + 701, quantity: "701.50", unit: "m3" }],
          next_offset: null,
          total_rows: 701 + offset,
          exact_versions: { result_id: "result-1", basis_fingerprint: "fp-1" },
          applicability: "current",
        });
      }),
    } as unknown as Api;
    renderHandoffs(api);

    expect(await screen.findByText("From Samir Fares")).toBeInTheDocument();
    await userEvent.click(
      screen.getByRole("button", { name: "Show exact rows" }),
    );
    expect(await screen.findByText("701.50")).toBeInTheDocument();
    expect(screen.getByText("Current basis")).toBeInTheDocument();
  });

  it("switches to sent handoffs and marks a changed basis", async () => {
    const calls: string[] = [];
    const api = {
      get: vi.fn().mockImplementation((url: string) => {
        calls.push(url);
        if (url.includes("direction=sent")) {
          return Promise.resolve({
            items: [
              view({
                id: "handoff-2",
                direction: "sent",
                counterpart_staff_id: "staff-1",
                counterpart_display_name: "Amal Kassem",
                handoff: { applicability: "needs_review" } as never,
              }),
            ],
            next_cursor: null,
            total: 1,
          });
        }
        return Promise.resolve({ items: [], next_cursor: null, total: 0 });
      }),
    } as unknown as Api;
    renderHandoffs(api);

    expect(
      await screen.findByText("No handoffs received by this colleague yet."),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Sent" }));
    expect(await screen.findByText("To Amal Kassem")).toBeInTheDocument();
    expect(
      screen.getByText("Needs review — source basis changed"),
    ).toBeInTheDocument();
    expect(calls.some((url) => url.includes("direction=sent"))).toBe(true);
  });

  it("retries a failed handoff read without losing the desk", async () => {
    const api = {
      get: vi
        .fn()
        .mockRejectedValueOnce(new Error("Handoffs unavailable."))
        .mockResolvedValueOnce({ items: [], next_cursor: null, total: 0 }),
    } as unknown as Api;
    renderHandoffs(api);

    expect(
      await screen.findByText("Handoffs unavailable."),
    ).toBeInTheDocument();
    await userEvent.click(
      screen.getByRole("button", { name: "Retry handoffs" }),
    );
    expect(
      await screen.findByText("No handoffs received by this colleague yet."),
    ).toBeInTheDocument();
  });
});
