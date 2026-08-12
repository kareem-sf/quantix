import { useEffect, useRef, useState } from "react";

import type { DecisionAction } from "./bindings/DecisionAction";
import type { DecisionCockpit } from "./bindings/DecisionCockpit";
import type { DecisionEvidence } from "./bindings/DecisionEvidence";
import type { DecisionFact } from "./bindings/DecisionFact";
import type { DecisionFactKind } from "./bindings/DecisionFactKind";
import type { DecisionKind } from "./bindings/DecisionKind";
import type { DecisionTargetKind } from "./bindings/DecisionTargetKind";
import type { EvidenceDocument } from "./bindings/EvidenceDocument";
import type { PendingDecision } from "./bindings/PendingDecision";
import type { TenderQueryTreatment } from "./bindings/TenderQueryTreatment";
import {
  EvidenceLocationDetails,
  evidenceLocationLabel,
} from "./EvidenceReview";
import {
  approveBasisOfEstimate,
  approveCalculationRule,
  approveCommercialStrategy,
  approveControlledBoqCalculationRun,
  approveExternalRfiForIssue,
  approvePricedCostBaseline,
  approvePricingAdjustment,
  approveProductionFindingException,
  approveTenderPrice,
  decideBidDecisionPackage,
  decideChangeAssessment,
  decideCoordinatedBidBaseline,
  decideTenderRecord,
  decideTenderQueryTreatment,
  decideWorkPlanProposal,
  inspectDecisionCockpit,
  inspectEvidence,
  inspectPricingWorkspace,
  interpretExternalRfiResponse,
  resolveIndeterminateAgentRun,
  resolveTenderRecovery,
  selectPricingScenario,
} from "./quantixHost";

interface DecisionCockpitPanelProps {
  tenderId: string;
  refreshToken: number;
  reportCommandFailure: () => void;
  onTenderStateChange: () => void;
}

type CockpitState =
  | { kind: "loading" }
  | { kind: "ready"; cockpit: DecisionCockpit }
  | { kind: "error" };

const decisionDestinations: Record<DecisionKind, string> = {
  bid_decision: "bid-decision-title",
  work_plan_approval: "tender-office-title",
  tender_record: "tender-records-title",
  query_treatment: "query-register-title",
  external_rfi_issue: "external-rfi-title",
  external_rfi_response_interpretation: "external-rfi-title",
  calculation_rule_approval: "controlled-boq-title",
  calculation_run_approval: "controlled-boq-title",
  basis_of_estimate_approval: "basis-of-estimate-title",
  priced_cost_baseline_approval: "pricing-decision-title",
  pricing_adjustment_approval: "pricing-decision-title",
  commercial_strategy_approval: "pricing-decision-title",
  pricing_scenario_selection: "pricing-decision-title",
  tender_price_approval: "pricing-decision-title",
  production_finding_exception: "tender-office-title",
  coordinated_bid_baseline_approval: "coordinated-baseline-title",
  change_assessment: "change-assessment-title",
  agent_access_request: "agent-office-title",
  agent_run_recovery: "agent-office-title",
  tender_recovery: "cockpit-title",
};

const targetDestinations: Record<DecisionTargetKind, string> = {
  bid_decision_package: "bid-decision-title",
  work_plan: "tender-office-title",
  work_plan_workstream: "tender-office-title",
  work_plan_task: "tender-office-title",
  agent_profile: "tender-office-title",
  tender_record: "tender-records-title",
  tender_query: "query-register-title",
  external_rfi: "external-rfi-title",
  external_rfi_response: "external-rfi-title",
  calculation_rule: "controlled-boq-title",
  calculation_run: "controlled-boq-title",
  basis_of_estimate: "basis-of-estimate-title",
  priced_cost_baseline: "pricing-decision-title",
  pricing_adjustment: "pricing-decision-title",
  commercial_strategy: "pricing-decision-title",
  pricing_scenario: "pricing-decision-title",
  approved_tender_price: "pricing-decision-title",
  calculation_manifest: "controlled-boq-title",
  production_review_finding: "tender-office-title",
  production_task: "tender-office-title",
  production_artifact: "tender-office-title",
  production_review: "tender-office-title",
  coordinated_bid_baseline: "coordinated-baseline-title",
  change_assessment: "change-assessment-title",
  agent_access_request: "agent-office-title",
  agent_run: "agent-office-title",
  tender_package: "tender-office-title",
  tender_backup: "cockpit-title",
  tender_recovery: "cockpit-title",
  approval: "cockpit-title",
};

const humanize = (value: string) =>
  value
    .replace(/_/g, " ")
    .replace(/\b\w/g, (character) => character.toUpperCase());

const splitDecisionItems = (value: string) =>
  [...new Set(value.split(/\r?\n/).map((item) => item.trim()))].filter(Boolean);

const factLabels: Record<DecisionFactKind, string> = {
  agent_recommendation: "Agent recommendation",
  verified_fact: "Verified fact",
  approved_assumption: "Approved assumption",
  deterministic_result: "Deterministic result",
  unresolved_gap: "Unresolved gap",
  prior_engineer_decision: "Prior Engineer decision",
};

const sameTarget = (left: PendingDecision, right: PendingDecision) =>
  left.kind === right.kind &&
  left.target.object_id === right.target.object_id &&
  left.target.version === right.target.version &&
  left.target.manifest_sha256 === right.target.manifest_sha256;

const cockpitActions = new Set<DecisionKind>([
  "bid_decision",
  "work_plan_approval",
  "tender_record",
  "query_treatment",
  "external_rfi_issue",
  "external_rfi_response_interpretation",
  "calculation_rule_approval",
  "calculation_run_approval",
  "basis_of_estimate_approval",
  "priced_cost_baseline_approval",
  "pricing_adjustment_approval",
  "commercial_strategy_approval",
  "pricing_scenario_selection",
  "tender_price_approval",
  "production_finding_exception",
  "coordinated_bid_baseline_approval",
  "change_assessment",
  "agent_run_recovery",
  "tender_recovery",
]);

function TrustFact({ fact }: { fact: DecisionFact }) {
  return (
    <div className={`cockpit-fact cockpit-fact--${fact.kind}`}>
      <dt>
        <span className="cockpit-trust-label">{factLabels[fact.kind]}</span>
        {fact.label}
      </dt>
      <dd>{fact.value}</dd>
    </div>
  );
}

function FactCollection({
  id,
  title,
  facts,
}: {
  id: string;
  title: string;
  facts: DecisionFact[];
}) {
  return (
    <section aria-labelledby={id}>
      <h4 id={id}>{title}</h4>
      {facts.length > 0 ? (
        <dl className="decision-cockpit__facts">
          {facts.map((fact, index) => (
            <TrustFact
              key={`${fact.kind}-${fact.label}-${index}`}
              fact={fact}
            />
          ))}
        </dl>
      ) : (
        <p>No current {title.toLowerCase()}.</p>
      )}
    </section>
  );
}

export function DecisionCockpitPanel({
  tenderId,
  refreshToken,
  reportCommandFailure,
  onTenderStateChange,
}: DecisionCockpitPanelProps) {
  const [state, setState] = useState<CockpitState>({ kind: "loading" });
  const [retryToken, setRetryToken] = useState(0);
  const [selectedId, setSelectedId] = useState<string>();
  const [evidenceDocument, setEvidenceDocument] = useState<EvidenceDocument>();
  const [evidenceOrdinal, setEvidenceOrdinal] = useState<number>();
  const [evidenceArtifactScope, setEvidenceArtifactScope] = useState(false);
  const [restoreEvidenceFocus, setRestoreEvidenceFocus] = useState(false);
  const [routeMessage, setRouteMessage] = useState<string>();
  const [pendingAction, setPendingAction] = useState<DecisionAction>();
  const [rationale, setRationale] = useState("");
  const [treatmentDetails, setTreatmentDetails] = useState("");
  const [responseInterpretation, setResponseInterpretation] = useState("");
  const [exceptionConsequence, setExceptionConsequence] = useState("");
  const [treatment, setTreatment] =
    useState<TenderQueryTreatment>("qualification");
  const [materialResponse, setMaterialResponse] = useState(true);
  const [closesQuery, setClosesQuery] = useState(false);
  const [conditions, setConditions] = useState("");
  const [exceptions, setExceptions] = useState("");
  const [requiredRework, setRequiredRework] = useState("");
  const [actionBusy, setActionBusy] = useState(false);
  const [decisionHistory, setDecisionHistory] = useState<
    { decisionId: string; focusId: string }[]
  >([]);
  const [domainReturn, setDomainReturn] = useState<{
    decisionId: string;
    focusId: string;
    target: PendingDecision["target"];
  }>();
  const [returnFocusId, setReturnFocusId] = useState<string>();
  const [focusSelectedDetail, setFocusSelectedDetail] = useState(false);
  const [actionFocusTarget, setActionFocusTarget] = useState<
    { kind: "decision"; decisionId: string } | { kind: "status" }
  >();
  const evidenceOrigin = useRef<HTMLButtonElement | null>(null);

  useEffect(() => {
    let active = true;
    setState({ kind: "loading" });
    setEvidenceDocument(undefined);
    setEvidenceArtifactScope(false);
    setDomainReturn(undefined);
    setRouteMessage(undefined);
    void inspectDecisionCockpit(tenderId)
      .then((cockpit) => {
        if (!active) return;
        setState({ kind: "ready", cockpit });
        setSelectedId((current) =>
          cockpit.pending_decisions.some(
            (decision) => decision.decision_id === current,
          )
            ? current
            : cockpit.pending_decisions[0]?.decision_id,
        );
      })
      .catch(() => {
        if (!active) return;
        setState({ kind: "error" });
        reportCommandFailure();
      });
    return () => {
      active = false;
    };
  }, [refreshToken, reportCommandFailure, retryToken, tenderId]);

  useEffect(() => {
    if (!returnFocusId) return;
    document.getElementById(returnFocusId)?.focus();
    setReturnFocusId(undefined);
  }, [returnFocusId, selectedId]);

  useEffect(() => {
    if (!focusSelectedDetail) return;
    const heading = document.getElementById("cockpit-detail-title");
    if (heading) {
      heading.tabIndex = -1;
      heading.focus();
    }
    setFocusSelectedDetail(false);
  }, [focusSelectedDetail, selectedId]);

  useEffect(() => {
    if (!actionFocusTarget) return;
    const element =
      actionFocusTarget.kind === "decision"
        ? document.getElementById(
            `cockpit-decision-${actionFocusTarget.decisionId}`,
          )
        : document.getElementById("cockpit-action-status");
    if (!element) return;
    if (actionFocusTarget.kind === "status") element.tabIndex = -1;
    element.focus();
    setActionFocusTarget(undefined);
  }, [actionFocusTarget, routeMessage, selectedId, state]);

  useEffect(() => {
    if (!evidenceDocument) return;
    const heading = document.getElementById("cockpit-source-title");
    if (heading) {
      heading.tabIndex = -1;
      heading.focus();
    }
  }, [evidenceDocument, evidenceOrdinal]);

  useEffect(() => {
    if (evidenceDocument || !restoreEvidenceFocus) return;
    evidenceOrigin.current?.focus();
    setRestoreEvidenceFocus(false);
  }, [evidenceDocument, restoreEvidenceFocus]);

  if (state.kind === "loading") {
    return (
      <section className="decision-cockpit" aria-labelledby="cockpit-title">
        <h2 id="cockpit-title">Decision Cockpit</h2>
        <p aria-live="polite">Loading exact formal decisions…</p>
      </section>
    );
  }

  if (state.kind === "error") {
    return (
      <section className="decision-cockpit" aria-labelledby="cockpit-title">
        <h2 id="cockpit-title">Decision Cockpit</h2>
        <p
          role="alert"
          aria-label="Formal decision inventory could not be inspected"
        >
          The formal decision inventory could not be inspected.
        </p>
        <button
          type="button"
          onClick={() => setRetryToken((token) => token + 1)}
        >
          Retry cockpit
        </button>
      </section>
    );
  }

  const { cockpit } = state;
  const selected = cockpit.pending_decisions.find(
    (decision) => decision.decision_id === selectedId,
  );

  const openEvidence = async (
    reference: DecisionEvidence,
    origin: HTMLButtonElement,
  ) => {
    evidenceOrigin.current = origin;
    try {
      const document = await inspectEvidence(
        tenderId,
        reference.artifact_id,
        reference.version,
      );
      const location =
        reference.location_ordinal === null
          ? undefined
          : document.locations.find(
              (candidate) => candidate.ordinal === reference.location_ordinal,
            );
      if (reference.location_ordinal !== null && !location) {
        throw new Error("exact evidence location is unavailable");
      }
      setEvidenceDocument(document);
      setEvidenceOrdinal(location?.ordinal);
      setEvidenceArtifactScope(reference.location_ordinal === null);
      setRouteMessage(undefined);
    } catch {
      reportCommandFailure();
      setRouteMessage("Exact source evidence is unavailable. Try again.");
    }
  };

  const closeEvidence = () => {
    setEvidenceDocument(undefined);
    setEvidenceOrdinal(undefined);
    setEvidenceArtifactScope(false);
    setRestoreEvidenceFocus(true);
  };

  const routeToExactGate = async (decision: PendingDecision) => {
    setDomainReturn(undefined);
    setRouteMessage("Checking the exact current target…");
    try {
      const latest = await inspectDecisionCockpit(tenderId);
      setState({ kind: "ready", cockpit: latest });
      const current = latest.pending_decisions.find(
        (candidate) => candidate.decision_id === decision.decision_id,
      );
      if (!current || !sameTarget(decision, current)) {
        setSelectedId(latest.pending_decisions[0]?.decision_id);
        setRouteMessage(
          "Decision changed. The cockpit refreshed instead of applying an action to stale evidence.",
        );
        return;
      }
      const destination = document.getElementById(
        decision.kind === "tender_recovery"
          ? `backup-${tenderId}`
          : decisionDestinations[decision.kind],
      );
      if (!destination) {
        setRouteMessage(
          "The exact domain decision gate is not available in this workspace.",
        );
        return;
      }
      destination.tabIndex = -1;
      destination.focus();
      destination.scrollIntoView({ block: "start" });
      setRouteMessage(
        `Opened the exact ${humanize(decision.kind)} gate. Recheck its current version before deciding.`,
      );
    } catch {
      reportCommandFailure();
      setRouteMessage(
        "The decision could not be revalidated. No action was taken.",
      );
    }
  };

  const navigateExactTarget = (
    decision: PendingDecision,
    target: PendingDecision["target"],
    label: string,
    focusId: string,
  ) => {
    const exact = cockpit.pending_decisions.find(
      (candidate) =>
        candidate.decision_id !== decision.decision_id &&
        candidate.target.kind === target.kind &&
        candidate.target.object_id === target.object_id &&
        candidate.target.version === target.version &&
        candidate.target.manifest_sha256 === target.manifest_sha256,
    );
    if (!exact) {
      const destination = document.getElementById(
        target.kind === "tender_backup" || target.kind === "tender_recovery"
          ? `backup-${tenderId}`
          : targetDestinations[target.kind],
      );
      if (destination) {
        setDomainReturn({
          decisionId: decision.decision_id,
          focusId,
          target,
        });
        destination.tabIndex = -1;
        destination.focus();
        destination.scrollIntoView({ block: "start" });
        setRouteMessage(
          `Opened the exact ${humanize(target.kind)} domain view for ${label}.`,
        );
      } else {
        setRouteMessage(
          "This dependency's exact domain view is not available in this workspace.",
        );
      }
      return;
    }
    setDecisionHistory((history) => [
      ...history,
      { decisionId: decision.decision_id, focusId },
    ]);
    setDomainReturn(undefined);
    setSelectedId(exact.decision_id);
    setFocusSelectedDetail(true);
    setPendingAction(undefined);
    setRouteMessage(undefined);
  };

  const navigateDependency = (
    decision: PendingDecision,
    dependencyIndex: number,
  ) => {
    const dependency = [
      ...decision.dependencies,
      ...decision.unresolved_queries,
    ][dependencyIndex];
    if (!dependency) return;
    navigateExactTarget(
      decision,
      dependency.target,
      "dependency provenance",
      `cockpit-dependency-${decision.decision_id}-${dependencyIndex}`,
    );
  };

  const returnToPriorDecision = () => {
    const prior = decisionHistory[decisionHistory.length - 1];
    if (!prior) return;
    setDecisionHistory((history) => history.slice(0, -1));
    setSelectedId(prior.decisionId);
    setFocusSelectedDetail(false);
    setReturnFocusId(prior.focusId);
  };

  const returnFromDomainDependency = () => {
    if (!domainReturn) return;
    setSelectedId(domainReturn.decisionId);
    setReturnFocusId(domainReturn.focusId);
    setDomainReturn(undefined);
    document
      .getElementById("cockpit-title")
      ?.scrollIntoView({ block: "start" });
  };

  const submitExactAction = async (
    decision: PendingDecision,
    action: DecisionAction,
  ) => {
    const exactRationale = rationale.trim();
    const exactTreatmentDetails = treatmentDetails.trim();
    const exactResponseInterpretation = responseInterpretation.trim();
    const exactExceptionConsequence = exceptionConsequence.trim();
    const manifest = decision.target.manifest_sha256;
    if (
      !exactRationale ||
      ((decision.kind === "query_treatment" ||
        decision.kind === "external_rfi_response_interpretation") &&
        !exactTreatmentDetails) ||
      (decision.kind === "external_rfi_response_interpretation" &&
        !exactResponseInterpretation) ||
      (decision.kind === "production_finding_exception" &&
        !exactExceptionConsequence) ||
      !cockpitActions.has(decision.kind)
    )
      return;
    setActionBusy(true);
    setRouteMessage("Revalidating the exact target before the decision...");
    try {
      const latest = await inspectDecisionCockpit(tenderId);
      const current = latest.pending_decisions.find(
        (candidate) => candidate.decision_id === decision.decision_id,
      );
      if (
        !current ||
        !sameTarget(decision, current) ||
        !current.allowed_actions.includes(action)
      ) {
        setState({ kind: "ready", cockpit: latest });
        setPendingAction(undefined);
        setRationale("");
        setTreatmentDetails("");
        setResponseInterpretation("");
        setExceptionConsequence("");
        setClosesQuery(false);
        setConditions("");
        setExceptions("");
        setRequiredRework("");
        setRouteMessage(
          "Decision changed. The cockpit refreshed instead of applying an action to stale evidence.",
        );
        return;
      }
      switch (decision.kind) {
        case "bid_decision":
          if (
            !manifest ||
            !(["accept", "return", "reject"] as string[]).includes(action)
          )
            throw new Error("unsupported exact action");
          await decideBidDecisionPackage(
            tenderId,
            decision.target.object_id,
            decision.target.version,
            manifest,
            action as "accept" | "return" | "reject",
            exactRationale,
            splitDecisionItems(conditions),
            splitDecisionItems(exceptions),
            action === "return" ? splitDecisionItems(requiredRework) : [],
          );
          break;
        case "work_plan_approval":
          if (!(["accept", "return", "reject"] as string[]).includes(action))
            throw new Error("unsupported exact action");
          await decideWorkPlanProposal(
            tenderId,
            decision.target.object_id,
            decision.target.version,
            action === "accept" ? "approve" : (action as "return" | "reject"),
            exactRationale,
          );
          break;
        case "tender_record":
          if (
            !(["verify", "approve_assumption", "reject"] as string[]).includes(
              action,
            )
          )
            throw new Error("unsupported exact action");
          await decideTenderRecord(
            tenderId,
            decision.target.object_id,
            decision.target.version,
            action as "verify" | "approve_assumption" | "reject",
            exactRationale,
          );
          break;
        case "query_treatment":
          if (action !== "apply_treatment")
            throw new Error("unsupported exact action");
          await decideTenderQueryTreatment({
            tender_id: tenderId,
            query_id: decision.target.object_id,
            query_version: decision.target.version,
            treatment,
            rationale: exactRationale,
            treatment_details: exactTreatmentDetails,
            closes_query: closesQuery,
          });
          break;
        case "external_rfi_issue":
          if (action !== "approve" || !manifest)
            throw new Error("unsupported exact action");
          await approveExternalRfiForIssue({
            tender_id: tenderId,
            rfi_id: decision.target.object_id,
            version: decision.target.version,
            manifest_sha256: manifest,
            rationale: exactRationale,
          });
          break;
        case "external_rfi_response_interpretation": {
          if (action !== "apply_treatment")
            throw new Error("unsupported exact action");
          const [responseLinkId, queryId] =
            decision.target.object_id.split(":");
          if (!responseLinkId || !queryId)
            throw new Error("invalid response decision target");
          const issued = current.dependencies.find(
            (dependency) =>
              dependency.target.kind === "tender_query" &&
              dependency.target.object_id === queryId &&
              dependency.status !== "unresolved",
          );
          const currentQuery = current.dependencies.find(
            (dependency) =>
              dependency.target.kind === "tender_query" &&
              dependency.target.object_id === queryId &&
              dependency.status === "unresolved",
          );
          if (!issued || !currentQuery?.target.manifest_sha256)
            throw new Error("stale response decision target");
          await interpretExternalRfiResponse({
            tender_id: tenderId,
            response_link_id: responseLinkId,
            query_id: queryId,
            issued_query_version: issued.target.version,
            base_query_version: currentQuery.target.version,
            base_query_manifest_sha256: currentQuery.target.manifest_sha256,
            material: materialResponse,
            interpretation: exactResponseInterpretation,
            treatment,
            rationale: exactRationale,
            treatment_details: exactTreatmentDetails,
            closes_query: closesQuery,
          });
          break;
        }
        case "calculation_rule_approval":
          if (action !== "approve" || !manifest)
            throw new Error("unsupported exact action");
          await approveCalculationRule({
            tender_id: tenderId,
            rule_id: decision.target.object_id,
            version: decision.target.version,
            manifest_sha256: manifest,
            rationale: exactRationale,
          });
          break;
        case "calculation_run_approval":
          if (action !== "approve" || !manifest)
            throw new Error("unsupported exact action");
          await approveControlledBoqCalculationRun({
            tender_id: tenderId,
            calculation_run_id: decision.target.object_id,
            manifest_sha256: manifest,
            rationale: exactRationale,
          });
          break;
        case "basis_of_estimate_approval":
          if (action !== "approve" || !manifest)
            throw new Error("unsupported exact action");
          await approveBasisOfEstimate({
            tender_id: tenderId,
            basis_id: decision.target.object_id,
            version: decision.target.version,
            manifest_sha256: manifest,
            rationale: exactRationale,
          });
          break;
        case "priced_cost_baseline_approval":
          if (action !== "approve" || !manifest)
            throw new Error("unsupported exact action");
          await approvePricedCostBaseline({
            tender_id: tenderId,
            baseline_id: decision.target.object_id,
            version: decision.target.version,
            manifest_sha256: manifest,
            rationale: exactRationale,
          });
          break;
        case "pricing_adjustment_approval":
          if (action !== "approve" || !manifest)
            throw new Error("unsupported exact action");
          await approvePricingAdjustment({
            tender_id: tenderId,
            adjustment_id: decision.target.object_id,
            version: decision.target.version,
            manifest_sha256: manifest,
            rationale: exactRationale,
          });
          break;
        case "commercial_strategy_approval":
          if (action !== "approve" || !manifest)
            throw new Error("unsupported exact action");
          await approveCommercialStrategy({
            tender_id: tenderId,
            strategy_id: decision.target.object_id,
            manifest_sha256: manifest,
            rationale: exactRationale,
          });
          break;
        case "pricing_scenario_selection":
          if (action !== "select" || !manifest)
            throw new Error("unsupported exact action");
          await selectPricingScenario({
            tender_id: tenderId,
            pricing_scenario_id: decision.target.object_id,
            version: decision.target.version,
            manifest_sha256: manifest,
            rationale: exactRationale,
          });
          break;
        case "tender_price_approval": {
          if (action !== "approve" || !manifest)
            throw new Error("unsupported exact action");
          const workspace = await inspectPricingWorkspace(tenderId);
          const scenario = workspace.scenarios.find(
            (candidate) =>
              candidate.pricing_scenario_id === decision.target.object_id &&
              candidate.version === decision.target.version &&
              candidate.manifest_sha256 === manifest,
          );
          if (!scenario) throw new Error("stale pricing scenario");
          await approveTenderPrice({
            tender_id: tenderId,
            pricing_scenario_id: decision.target.object_id,
            version: decision.target.version,
            manifest_sha256: manifest,
            calculation_manifest_sha256: scenario.calculation.manifest_sha256,
            rationale: exactRationale,
          });
          break;
        }
        case "production_finding_exception": {
          if (action !== "approve_exception" || !manifest)
            throw new Error("unsupported exact action");
          const task = decision.dependencies.find(
            (dependency) => dependency.target.kind === "production_task",
          );
          const review = decision.dependencies.find(
            (dependency) => dependency.target.kind === "production_review",
          );
          const artifact = decision.dependencies.find(
            (dependency) => dependency.target.kind === "production_artifact",
          );
          if (!task || !review || !artifact)
            throw new Error("incomplete exact exception target");
          await approveProductionFindingException(
            tenderId,
            task.target.object_id,
            decision.target.object_id,
            review.target.object_id,
            artifact.target.object_id,
            artifact.target.version,
            manifest,
            exactRationale,
            exactExceptionConsequence,
          );
          break;
        }
        case "coordinated_bid_baseline_approval":
          if (
            !manifest ||
            !(["accept", "return", "reject"] as string[]).includes(action)
          )
            throw new Error("unsupported exact action");
          await decideCoordinatedBidBaseline(
            tenderId,
            decision.target.object_id,
            decision.target.version,
            manifest,
            action === "accept" ? "approve" : (action as "return" | "reject"),
            exactRationale,
            splitDecisionItems(conditions),
            splitDecisionItems(exceptions),
          );
          break;
        case "change_assessment":
          if (
            !manifest ||
            !(
              ["classify_irrelevant", "classify_material"] as string[]
            ).includes(action)
          )
            throw new Error("unsupported exact action");
          await decideChangeAssessment(
            tenderId,
            decision.target.object_id,
            manifest,
            action === "classify_irrelevant" ? "irrelevant" : "material",
            exactRationale,
          );
          break;
        case "agent_run_recovery":
          if (!(["retry_task", "close_task"] as string[]).includes(action))
            throw new Error("unsupported exact action");
          await resolveIndeterminateAgentRun(
            tenderId,
            decision.target.object_id,
            action as "retry_task" | "close_task",
            exactRationale,
          );
          break;
        case "tender_recovery":
          if (
            !manifest ||
            !(["approve_replacement", "reject"] as string[]).includes(action)
          )
            throw new Error("unsupported exact action");
          await resolveTenderRecovery(
            tenderId,
            decision.target.object_id,
            action as "approve_replacement" | "reject",
            exactRationale,
          );
          break;
        default:
          throw new Error("unsupported exact action");
      }
      const remaining = latest.pending_decisions.filter(
        (candidate) => candidate.decision_id !== decision.decision_id,
      );
      setState({
        kind: "ready",
        cockpit: { ...latest, pending_decisions: remaining },
      });
      setSelectedId(remaining[0]?.decision_id);
      setActionFocusTarget(
        remaining[0]
          ? { kind: "decision", decisionId: remaining[0].decision_id }
          : { kind: "status" },
      );
      setPendingAction(undefined);
      setRationale("");
      setTreatmentDetails("");
      setResponseInterpretation("");
      setExceptionConsequence("");
      setClosesQuery(false);
      setConditions("");
      setExceptions("");
      setRequiredRework("");
      setRouteMessage(`${humanize(action)} recorded for the exact target.`);
      onTenderStateChange();
      try {
        const refreshed = await inspectDecisionCockpit(tenderId);
        setState({ kind: "ready", cockpit: refreshed });
        setSelectedId(refreshed.pending_decisions[0]?.decision_id);
        setActionFocusTarget(
          refreshed.pending_decisions[0]
            ? {
                kind: "decision",
                decisionId: refreshed.pending_decisions[0].decision_id,
              }
            : { kind: "status" },
        );
      } catch {
        reportCommandFailure();
        setRouteMessage(
          `${humanize(action)} recorded for the exact target, but the refreshed inventory is unavailable.`,
        );
      }
    } catch {
      reportCommandFailure();
      setRouteMessage(
        "The exact decision was rejected or became stale. No substitute action was applied.",
      );
    } finally {
      setActionBusy(false);
    }
  };

  const evidenceLocation =
    evidenceOrdinal === undefined
      ? undefined
      : evidenceDocument?.locations.find(
          (location) => location.ordinal === evidenceOrdinal,
        );
  const evidenceLocationIndex =
    evidenceDocument && evidenceLocation
      ? evidenceDocument.locations.findIndex(
          (location) => location.ordinal === evidenceLocation.ordinal,
        )
      : -1;
  const detailFacts = selected?.facts ?? [];

  return (
    <section className="decision-cockpit" aria-labelledby="cockpit-title">
      <div className="section-heading">
        <div>
          <p className="eyebrow">Tendering Manager control surface</p>
          <h2 id="cockpit-title">Decision Cockpit</h2>
        </div>
        <span>
          {cockpit.pending_decisions.length} pending · revision{" "}
          {cockpit.tender_revision}
        </span>
      </div>
      <p>
        Review canonical evidence and apply only existing exact domain commands.
        The cockpit does not create a parallel approval authority.
      </p>
      {domainReturn ? (
        <div className="decision-cockpit__domain-return">
          <p role="status" aria-label="Exact dependency target">
            {humanize(domainReturn.target.kind)} ·{" "}
            {domainReturn.target.object_id}
            {" · "}v{domainReturn.target.version}
            {domainReturn.target.manifest_sha256
              ? ` · manifest ${domainReturn.target.manifest_sha256}`
              : ""}
          </p>
          <button type="button" onClick={returnFromDomainDependency}>
            Return to cockpit provenance
          </button>
        </div>
      ) : null}

      {cockpit.pending_decisions.length === 0 ? (
        <p className="notice" aria-live="polite">
          No pending formal decisions in {humanize(cockpit.lifecycle_phase)}.
        </p>
      ) : (
        <div className="decision-cockpit__layout">
          <nav aria-label="Pending formal decisions">
            <ol className="decision-cockpit__list">
              {cockpit.pending_decisions.map((decision) => (
                <li key={decision.decision_id}>
                  <button
                    type="button"
                    id={`cockpit-decision-${decision.decision_id}`}
                    aria-current={
                      decision.decision_id === selectedId ? "true" : undefined
                    }
                    onClick={() => {
                      setSelectedId(decision.decision_id);
                      setDomainReturn(undefined);
                      setEvidenceDocument(undefined);
                      setEvidenceArtifactScope(false);
                      setRouteMessage(undefined);
                      setPendingAction(undefined);
                      setRationale("");
                      setTreatmentDetails("");
                      setResponseInterpretation("");
                      setExceptionConsequence("");
                      setClosesQuery(false);
                      setConditions("");
                      setExceptions("");
                      setRequiredRework("");
                    }}
                  >
                    <span
                      className={`status-pill status-pill--${decision.status}`}
                    >
                      {humanize(decision.status)} · {humanize(decision.urgency)}
                    </span>
                    <strong>{decision.title}</strong>
                    <span>
                      {decision.responsible.label} ·{" "}
                      {humanize(decision.lifecycle_gate)}
                    </span>
                    <span>{decision.urgency_reason}</span>
                  </button>
                </li>
              ))}
            </ol>
          </nav>

          {selected ? (
            <article
              className="decision-cockpit__detail"
              aria-labelledby="cockpit-detail-title"
            >
              <p className="eyebrow">Exact formal decision</p>
              <h3 id="cockpit-detail-title">{selected.title}</h3>
              {decisionHistory.length > 0 ? (
                <button type="button" onClick={returnToPriorDecision}>
                  Back to prior decision
                </button>
              ) : null}
              <p>{selected.summary}</p>
              <dl className="decision-cockpit__metadata">
                <div>
                  <dt>Target</dt>
                  <dd>
                    {humanize(selected.target.kind)} ·{" "}
                    {selected.target.object_id} · v{selected.target.version}
                  </dd>
                </div>
                <div>
                  <dt>Manifest</dt>
                  <dd title={selected.target.manifest_sha256 ?? undefined}>
                    {selected.target.manifest_sha256?.slice(0, 16) ??
                      "Not applicable"}
                  </dd>
                </div>
                <div>
                  <dt>Deadline</dt>
                  <dd>{selected.deadline ?? "No canonical deadline"}</dd>
                </div>
                <div>
                  <dt>Legal exact actions</dt>
                  <dd>
                    {selected.allowed_actions.length > 0
                      ? selected.allowed_actions.map(humanize).join(" · ")
                      : "None until blockers clear"}
                  </dd>
                </div>
              </dl>

              {selected.blocking_consequences.length > 0 ? (
                <section aria-labelledby="cockpit-blockers-title">
                  <h4 id="cockpit-blockers-title">Blocking consequences</h4>
                  <ul>
                    {selected.blocking_consequences.map((blocker) => (
                      <li key={blocker}>{blocker}</li>
                    ))}
                  </ul>
                </section>
              ) : null}

              <FactCollection
                id="cockpit-facts-title"
                title="Decision facts by trust class"
                facts={detailFacts}
              />

              <section aria-labelledby="cockpit-evidence-title">
                <h4 id="cockpit-evidence-title">
                  Exact evidence and provenance
                </h4>
                {selected.evidence.length > 0 ? (
                  <ul className="decision-cockpit__evidence">
                    {selected.evidence.map((reference, index) => (
                      <li
                        key={`${reference.artifact_id}-${reference.version}-${reference.location_ordinal}-${index}`}
                      >
                        <button
                          type="button"
                          aria-label={`Open exact evidence ${reference.label}`}
                          onClick={(event) =>
                            void openEvidence(reference, event.currentTarget)
                          }
                        >
                          <strong>{reference.label}</strong>
                          <span>
                            {reference.artifact_id} · v{reference.version}
                          </span>
                        </button>
                      </li>
                    ))}
                  </ul>
                ) : (
                  <p>No exact source location is bound to this decision.</p>
                )}
              </section>

              <section aria-labelledby="cockpit-changes-title">
                <h4 id="cockpit-changes-title">Changes since prior review</h4>
                {selected.changes_since_prior_review.length > 0 ? (
                  <ul>
                    {selected.changes_since_prior_review.map((change) => (
                      <li key={change}>{change}</li>
                    ))}
                  </ul>
                ) : (
                  <p>No changes are recorded since the prior review.</p>
                )}
              </section>

              <section aria-labelledby="cockpit-dependencies-title">
                <h4 id="cockpit-dependencies-title">
                  Dependencies and Queries
                </h4>
                {selected.dependencies.length > 0 ||
                selected.unresolved_queries.length > 0 ? (
                  <ul>
                    {[
                      ...selected.dependencies,
                      ...selected.unresolved_queries,
                    ].map((dependency, index) => (
                      <li key={`${dependency.target.object_id}-${index}`}>
                        <button
                          type="button"
                          id={`cockpit-dependency-${selected.decision_id}-${index}`}
                          aria-label={`Open dependency ${dependency.label}`}
                          onClick={() => navigateDependency(selected, index)}
                        >
                          {dependency.label} · v{dependency.target.version} ·{" "}
                          {humanize(dependency.status)}
                        </button>
                      </li>
                    ))}
                  </ul>
                ) : (
                  <p>No unresolved dependency or Query is bound.</p>
                )}
              </section>

              <FactCollection
                id="cockpit-assumptions-title"
                title="Assumptions"
                facts={selected.assumptions}
              />
              <FactCollection
                id="cockpit-calculations-title"
                title="Calculations"
                facts={selected.calculations}
              />
              <FactCollection
                id="cockpit-findings-title"
                title="Findings"
                facts={selected.findings}
              />
              <FactCollection
                id="cockpit-exceptions-title"
                title="Exceptions"
                facts={selected.exceptions}
              />
              <FactCollection
                id="cockpit-review-title"
                title="Independent review"
                facts={
                  selected.independent_review
                    ? [selected.independent_review]
                    : []
                }
              />

              {selected.group_members.length > 0 ? (
                <section aria-labelledby="cockpit-group-title">
                  <h4 id="cockpit-group-title">Every grouped target</h4>
                  <ol>
                    {selected.group_members.map((member, index) => (
                      <li
                        key={`${member.target.kind}-${member.target.object_id}-${member.target.version}`}
                      >
                        <button
                          type="button"
                          id={`cockpit-group-${selected.decision_id}-${index}`}
                          aria-label={`Open grouped target ${member.target.object_id}`}
                          onClick={() =>
                            navigateExactTarget(
                              selected,
                              member.target,
                              "grouped decision provenance",
                              `cockpit-group-${selected.decision_id}-${index}`,
                            )
                          }
                        >
                          {member.target.object_id} · v{member.target.version} ·{" "}
                          {member.condition} · {humanize(member.status)}
                        </button>
                      </li>
                    ))}
                  </ol>
                </section>
              ) : null}

              <button
                type="button"
                className="button-primary"
                onClick={() => void routeToExactGate(selected)}
              >
                Open exact decision gate
              </button>
              {selected.allowed_actions.length > 0 &&
              cockpitActions.has(selected.kind) ? (
                <div
                  className="button-row"
                  aria-label="Available exact actions"
                >
                  {selected.allowed_actions.map((action) => (
                    <button
                      type="button"
                      className="button-secondary"
                      key={action}
                      aria-label={`${humanize(action)} exact target`}
                      onClick={() => {
                        setPendingAction(action);
                        setRationale("");
                        setTreatmentDetails("");
                        setResponseInterpretation("");
                        setExceptionConsequence("");
                        setTreatment("qualification");
                        setMaterialResponse(true);
                        setClosesQuery(false);
                        setConditions("");
                        setExceptions("");
                        setRequiredRework("");
                      }}
                    >
                      {humanize(action)} exact target
                    </button>
                  ))}
                </div>
              ) : null}
              {pendingAction &&
              selected.allowed_actions.includes(pendingAction) ? (
                <form
                  className="stacked-form"
                  onSubmit={(event) => {
                    event.preventDefault();
                    void submitExactAction(selected, pendingAction);
                  }}
                >
                  <label htmlFor="cockpit-rationale">Decision rationale</label>
                  <textarea
                    id="cockpit-rationale"
                    value={rationale}
                    onChange={(event) => setRationale(event.target.value)}
                    maxLength={4000}
                  />
                  {selected.kind === "query_treatment" ||
                  selected.kind === "external_rfi_response_interpretation" ? (
                    <>
                      <label>
                        Exact Query treatment
                        <select
                          value={treatment}
                          onChange={(event) =>
                            setTreatment(
                              event.target.value as TenderQueryTreatment,
                            )
                          }
                        >
                          <option value="internal_resolution">
                            Internal resolution
                          </option>
                          <option value="external_rfi">External RFI</option>
                          <option value="approved_assumption">
                            Approved Assumption
                          </option>
                          <option value="qualification">Qualification</option>
                          <option value="exclusion">Exclusion</option>
                          <option value="allowance">Allowance</option>
                          <option value="blocked">Blocked</option>
                        </select>
                      </label>
                      <label htmlFor="cockpit-treatment-details">
                        Exact treatment details
                      </label>
                      <textarea
                        id="cockpit-treatment-details"
                        value={treatmentDetails}
                        onChange={(event) =>
                          setTreatmentDetails(event.target.value)
                        }
                        maxLength={4000}
                      />
                    </>
                  ) : null}
                  {selected.kind === "external_rfi_response_interpretation" ? (
                    <>
                      <label htmlFor="cockpit-response-interpretation">
                        Exact response interpretation
                      </label>
                      <textarea
                        id="cockpit-response-interpretation"
                        value={responseInterpretation}
                        onChange={(event) =>
                          setResponseInterpretation(event.target.value)
                        }
                        maxLength={4000}
                      />
                      <label>
                        <input
                          type="checkbox"
                          checked={materialResponse}
                          onChange={(event) =>
                            setMaterialResponse(event.target.checked)
                          }
                        />
                        Material response interpretation
                      </label>
                    </>
                  ) : null}
                  {selected.kind === "query_treatment" ||
                  selected.kind === "external_rfi_response_interpretation" ? (
                    <label>
                      <input
                        type="checkbox"
                        checked={closesQuery}
                        onChange={(event) =>
                          setClosesQuery(event.target.checked)
                        }
                      />
                      Close exact Query after applying this treatment
                    </label>
                  ) : null}
                  {selected.kind === "production_finding_exception" ? (
                    <>
                      <label htmlFor="cockpit-exception-consequence">
                        Exact exception consequence
                      </label>
                      <textarea
                        id="cockpit-exception-consequence"
                        value={exceptionConsequence}
                        onChange={(event) =>
                          setExceptionConsequence(event.target.value)
                        }
                        maxLength={4000}
                      />
                    </>
                  ) : null}
                  {selected.kind === "bid_decision" ||
                  selected.kind === "coordinated_bid_baseline_approval" ? (
                    <>
                      <label htmlFor="cockpit-conditions">
                        Decision conditions (one per line)
                      </label>
                      <textarea
                        id="cockpit-conditions"
                        value={conditions}
                        onChange={(event) => setConditions(event.target.value)}
                        maxLength={4000}
                      />
                      <label htmlFor="cockpit-exceptions">
                        Decision exceptions (one per line)
                      </label>
                      <textarea
                        id="cockpit-exceptions"
                        value={exceptions}
                        onChange={(event) => setExceptions(event.target.value)}
                        maxLength={4000}
                      />
                    </>
                  ) : null}
                  {selected.kind === "bid_decision" &&
                  pendingAction === "return" ? (
                    <>
                      <label htmlFor="cockpit-required-rework">
                        Required rework (one per line)
                      </label>
                      <textarea
                        id="cockpit-required-rework"
                        value={requiredRework}
                        onChange={(event) =>
                          setRequiredRework(event.target.value)
                        }
                        maxLength={4000}
                      />
                    </>
                  ) : null}
                  <button
                    type="submit"
                    disabled={
                      actionBusy ||
                      !rationale.trim() ||
                      ((selected.kind === "query_treatment" ||
                        selected.kind ===
                          "external_rfi_response_interpretation") &&
                        !treatmentDetails.trim()) ||
                      (selected.kind ===
                        "external_rfi_response_interpretation" &&
                        !responseInterpretation.trim()) ||
                      (selected.kind === "production_finding_exception" &&
                        !exceptionConsequence.trim()) ||
                      (selected.kind === "bid_decision" &&
                        pendingAction === "return" &&
                        splitDecisionItems(requiredRework).length === 0)
                    }
                  >
                    Confirm {humanize(pendingAction)}
                  </button>
                </form>
              ) : null}
            </article>
          ) : null}
        </div>
      )}

      {evidenceDocument && evidenceLocation ? (
        <section
          className="cockpit-evidence-detail"
          aria-labelledby="cockpit-source-title"
        >
          <div className="section-heading">
            <div>
              <p className="eyebrow">Exact source provenance</p>
              <h3 id="cockpit-source-title">
                {evidenceLocationLabel(evidenceLocation)}
              </h3>
            </div>
            <button type="button" onClick={closeEvidence}>
              Back to decision
            </button>
          </div>
          <p>
            {evidenceDocument.artifact_id} · v{evidenceDocument.version} ·
            location {evidenceLocation.ordinal}
          </p>
          <nav aria-label="Exact evidence locations" className="button-row">
            <button
              type="button"
              disabled={evidenceLocationIndex <= 0}
              onClick={() =>
                setEvidenceOrdinal(
                  evidenceDocument.locations[evidenceLocationIndex - 1]
                    ?.ordinal,
                )
              }
            >
              Previous evidence location
            </button>
            <button
              type="button"
              disabled={
                evidenceLocationIndex < 0 ||
                evidenceLocationIndex >= evidenceDocument.locations.length - 1
              }
              onClick={() =>
                setEvidenceOrdinal(
                  evidenceDocument.locations[evidenceLocationIndex + 1]
                    ?.ordinal,
                )
              }
            >
              Next evidence location
            </button>
          </nav>
          <EvidenceLocationDetails
            location={evidenceLocation}
            includeHeading={false}
          />
        </section>
      ) : null}

      {evidenceDocument && evidenceArtifactScope ? (
        <section
          className="cockpit-evidence-detail"
          aria-labelledby="cockpit-source-title"
        >
          <div className="section-heading">
            <div>
              <p className="eyebrow">Exact source provenance</p>
              <h3 id="cockpit-source-title">Artifact-level source reference</h3>
            </div>
            <button type="button" onClick={closeEvidence}>
              Back to decision
            </button>
          </div>
          <p>
            {evidenceDocument.artifact_id} · v{evidenceDocument.version}
          </p>
          <p>
            This decision binds the immutable artifact version as a whole. No
            exact source location was claimed.
          </p>
        </section>
      ) : null}

      {routeMessage ? (
        <p
          id="cockpit-action-status"
          role={
            routeMessage.startsWith("Decision changed") ||
            /unavailable|not available|could not|rejected|became stale/.test(
              routeMessage,
            )
              ? "alert"
              : "status"
          }
          aria-label={
            routeMessage.startsWith("Decision changed")
              ? "Decision changed"
              : undefined
          }
          aria-live="polite"
        >
          {routeMessage}
        </p>
      ) : null}
    </section>
  );
}
