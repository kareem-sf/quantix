import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

async function loadFocusedAction() {
  vi.resetModules();
  vi.doMock("./BidDecisionPanel", () => ({
    BidDecisionPanel: ({
      onTenderStateChange,
    }: {
      onTenderStateChange: () => void;
    }) => (
      <section aria-label="Bid Decision panel">
        <button type="button" onClick={onTenderStateChange}>
          Accept Bid Decision
        </button>
      </section>
    ),
  }));
  vi.doMock("./TenderOfficePanel", () => ({
    TenderOfficePanel: ({
      onTenderStateChange,
    }: {
      onTenderStateChange: () => void;
    }) => (
      <section aria-label="Work Plan panel">
        <button type="button" onClick={onTenderStateChange}>
          Compose Work Plan
        </button>
        <button type="button" onClick={onTenderStateChange}>
          Approve Exact Work Plan
        </button>
        <button type="button" onClick={onTenderStateChange}>
          Activate Exact Work Plan
        </button>
      </section>
    ),
  }));
  return (await import("./TenderFocusedAction")).TenderFocusedAction;
}

afterEach(() => {
  cleanup();
  vi.doUnmock("./BidDecisionPanel");
  vi.doUnmock("./TenderOfficePanel");
  vi.resetModules();
});

describe("TenderFocusedAction", () => {
  it("routes bid acceptance into Work Plan review and refreshes after every mutation", async () => {
    const TenderFocusedAction = await loadFocusedAction();
    const onManagerRefresh = vi.fn().mockResolvedValue(undefined);
    const onClose = vi.fn();
    const view = render(
      <TenderFocusedAction
        tenderId="tender-1"
        actionKind="review_bid_decision"
        runtimeReady
        reportCommandFailure={vi.fn()}
        onManagerRefresh={onManagerRefresh}
        onClose={onClose}
      />,
    );

    fireEvent.click(
      screen.getByRole("button", { name: "Accept Bid Decision" }),
    );
    expect(onManagerRefresh).toHaveBeenCalledTimes(1);

    view.rerender(
      <TenderFocusedAction
        tenderId="tender-1"
        actionKind="prepare_work_plan"
        runtimeReady
        reportCommandFailure={vi.fn()}
        onManagerRefresh={onManagerRefresh}
        onClose={onClose}
      />,
    );
    expect(screen.getByRole("heading", { name: "Work Plan" })).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Compose Work Plan" }));
    fireEvent.click(
      screen.getByRole("button", { name: "Approve Exact Work Plan" }),
    );
    fireEvent.click(
      screen.getByRole("button", { name: "Activate Exact Work Plan" }),
    );
    expect(onManagerRefresh).toHaveBeenCalledTimes(4);
  });

  it("keeps the focused Work Plan route available for an activation retry", async () => {
    const TenderFocusedAction = await loadFocusedAction();
    const onManagerRefresh = vi.fn().mockResolvedValue(undefined);
    render(
      <TenderFocusedAction
        tenderId="tender-1"
        actionKind="review_work_plan"
        runtimeReady
        reportCommandFailure={vi.fn()}
        onManagerRefresh={onManagerRefresh}
        onClose={vi.fn()}
      />,
    );

    const activate = screen.getByRole("button", {
      name: "Activate Exact Work Plan",
    });
    fireEvent.click(activate);
    fireEvent.click(activate);
    expect(onManagerRefresh).toHaveBeenCalledTimes(2);
    expect(screen.getByRole("heading", { name: "Work Plan" })).toBeTruthy();
  });
});
