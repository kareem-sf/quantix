import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { describe, expect, it, vi } from "vitest";
import type { Decision } from "../office/queries";
import { useNeedsYou } from "./needsYou";

const NONE = { manager: 0, boq: 0, facts: 0, takeoff: 0, drawings: 0, pricing: 0, subcontract: 0, enquiries: 0, submission: 0 };

/** What waits on tender t1, as the service counts its gates and lists its questions. */
async function needs(gates: Partial<typeof NONE>, decisions: Partial<Decision>[] = []) {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (request: Request) => {
      const body = new URL(request.url).pathname.endsWith("/gates") ? { ...NONE, ...gates } : decisions;
      return new Response(JSON.stringify(body), { headers: { "Content-Type": "application/json" } });
    }),
  );
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
  const { result } = renderHook(() => useNeedsYou("t1"), { wrapper });
  await waitFor(() => expect(client.getQueryCache().getAll().map((q) => q.state.status)).toEqual(["success", "success"]));
  return result.current;
}

describe("what needs the engineer on a tender", () => {
  it("lists each kind of approval once, one click from the screen it is decided on", async () => {
    const { approvals } = await needs({ facts: 1, takeoff: 1, drawings: 1, pricing: 1, subcontract: 1, enquiries: 1, submission: 1, boq: 1 });

    expect(approvals.map((a) => [a.title, a.to])).toEqual([
      ["1 tender fact to approve", "/tenders/t1/estimate"],
      ["1 takeoff mark to check", "/tenders/t1/takeoff"],
      ["1 query or layer map to decide", "/tenders/t1/queries"],
      ["1 price to approve", "/tenders/t1/estimate?show=waiting"],
      ["1 quote to choose", "/tenders/t1/subcontract"],
      ["1 enquiry to send", "/tenders/t1/subcontract"],
      ["1 draft to review", "/tenders/t1/submission?show=review"],
      ["1 BOQ item to approve", "/tenders/t1/estimate?show=waiting"],
    ]);
  });

  it("counts several in plain words", async () => {
    const { approvals } = await needs({ facts: 2, takeoff: 3, drawings: 2, pricing: 4, subcontract: 2, enquiries: 5, submission: 2, boq: 26 });

    expect(approvals.map((a) => a.title)).toEqual([
      "2 tender facts to approve",
      "3 takeoff marks to check",
      "2 queries and layer maps to decide",
      "4 prices to approve",
      "2 quotes to choose",
      "5 enquiries to send",
      "2 drafts to review",
      "26 BOQ items to approve",
    ]);
  });

  it("leaves out work still with the Tender Manager, and questions already answered", async () => {
    const waiting = { id: "q1", status: "waiting", title: "Site support period" };
    const { approvals, questions } = await needs({ manager: 7 }, [waiting, { id: "q2", status: "answered", title: "Bond" }]);

    expect(approvals).toEqual([]);
    expect(questions.map((q) => q.id)).toEqual(["q1"]);
  });
});
