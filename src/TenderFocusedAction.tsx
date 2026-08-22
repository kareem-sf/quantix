import { useCallback, useState } from "react";

import type { WorkspaceActionKind } from "./bindings/WorkspaceActionKind";
import { BidDecisionPanel } from "./BidDecisionPanel";
import { TenderOfficePanel } from "./TenderOfficePanel";
import "./TenderFocusedAction.css";

type FocusedActionKind = Extract<
  WorkspaceActionKind,
  "review_bid_decision" | "prepare_work_plan" | "review_work_plan"
>;

interface TenderFocusedActionProps {
  tenderId: string;
  actionKind: FocusedActionKind;
  runtimeReady: boolean;
  reportCommandFailure: () => void;
  onManagerRefresh: () => Promise<void>;
  onClose: () => void;
}

export function TenderFocusedAction({
  tenderId,
  actionKind,
  runtimeReady,
  reportCommandFailure,
  onManagerRefresh,
  onClose,
}: TenderFocusedActionProps) {
  const [refreshToken, setRefreshToken] = useState(0);
  const onProductionSchedulingChange = useCallback(() => undefined, []);
  const onTenderStateChange = useCallback(() => {
    setRefreshToken((current) => current + 1);
    void onManagerRefresh();
  }, [onManagerRefresh]);

  return (
    <section
      className="tender-focused-action"
      data-testid="tender-focused-action"
      aria-labelledby="tender-focused-action-title"
    >
      <header className="tender-focused-action__header">
        <div>
          <p className="section-label">Tender decision workspace</p>
          <h2 id="tender-focused-action-title">
            {actionKind === "review_bid_decision"
              ? "Bid Decision"
              : "Work Plan"}
          </h2>
          <p>
            Complete the exact governed action here. The Manager projection
            refreshes after each recorded mutation.
          </p>
        </div>
        <button
          type="button"
          className="manager-workspace__secondary"
          onClick={onClose}
        >
          Back to Manager
        </button>
      </header>

      {actionKind === "review_bid_decision" ? (
        <BidDecisionPanel
          tenderId={tenderId}
          runtimeReady={runtimeReady}
          reportCommandFailure={reportCommandFailure}
          onTenderStateChange={onTenderStateChange}
        />
      ) : (
        <TenderOfficePanel
          tenderId={tenderId}
          runtimeReady={runtimeReady}
          reportCommandFailure={reportCommandFailure}
          refreshToken={refreshToken}
          onTenderStateChange={onTenderStateChange}
          onProductionSchedulingChange={onProductionSchedulingChange}
        />
      )}
    </section>
  );
}
