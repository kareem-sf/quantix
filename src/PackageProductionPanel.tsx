import { useCallback, useEffect, useRef, useState } from "react";

import type { CoordinatedBidBaseline } from "./bindings/CoordinatedBidBaseline";
import type { PackageProductionGeneration } from "./bindings/PackageProductionGeneration";
import type { SubmissionDecisionReference } from "./bindings/SubmissionDecisionReference";
import type { SubmissionPackageVersion } from "./bindings/SubmissionPackageVersion";
import type { TenderLifecyclePhase } from "./bindings/TenderLifecyclePhase";
import {
  assembleSubmissionPackage,
  generateSubmissionSections,
  inspectCoordinatedBidBaselines,
  inspectCurrentSubmissionPackage,
  inspectPackageProduction,
  inspectSubmissionArtifactContent,
  inspectSubmissionPackageItemContent,
} from "./quantixHost";

interface PackageProductionPanelProps {
  tenderId: string;
  runtimeReady: boolean;
  refreshToken: number;
  reportCommandFailure: () => void;
  onTenderStateChange: () => void;
}

function humanize(value: string) {
  return value.replace(/_/g, " ");
}

function evidenceDirection(value: string): "ltr" | "rtl" | "auto" {
  if (value === "right_to_left") return "rtl";
  if (value === "left_to_right") return "ltr";
  return "auto";
}

export function PackageProductionPanel({
  tenderId,
  runtimeReady,
  refreshToken,
  reportCommandFailure,
  onTenderStateChange,
}: PackageProductionPanelProps) {
  const [baseline, setBaseline] = useState<CoordinatedBidBaseline>();
  const [generation, setGeneration] =
    useState<PackageProductionGeneration | null>(null);
  const [submissionPackage, setSubmissionPackage] =
    useState<SubmissionPackageVersion | null>(null);
  const [lifecyclePhase, setLifecyclePhase] = useState<TenderLifecyclePhase>();
  const [selectedRequirementId, setSelectedRequirementId] = useState<string>();
  const [loading, setLoading] = useState(false);
  const [generating, setGenerating] = useState(false);
  const [assembling, setAssembling] = useState(false);
  const [loadedContent, setLoadedContent] = useState<
    Record<string, number | "loading" | "failed">
  >({});
  const requestGeneration = useRef(0);
  const actionActive = useRef(false);

  const load = useCallback(async () => {
    const request = ++requestGeneration.current;
    setLoading(true);
    try {
      const [baselines, production, assembled] = await Promise.all([
        inspectCoordinatedBidBaselines(tenderId, null, 1),
        inspectPackageProduction(tenderId),
        inspectCurrentSubmissionPackage(tenderId),
      ]);
      if (request !== requestGeneration.current) return;
      const exactApproved = baselines.items.find(
        (item) =>
          item.current &&
          item.approval?.decision === "approve" &&
          ["package_production", "final_review"].includes(
            baselines.lifecycle_phase,
          ),
      );
      setBaseline(exactApproved);
      setLifecyclePhase(baselines.lifecycle_phase);
      setGeneration(production);
      setSubmissionPackage(assembled);
      setSelectedRequirementId(
        assembled?.coverage[0]?.requirement.requirement_id,
      );
    } catch {
      if (request === requestGeneration.current) reportCommandFailure();
    } finally {
      if (request === requestGeneration.current) setLoading(false);
    }
  }, [reportCommandFailure, tenderId]);

  useEffect(() => {
    void load();
    return () => {
      requestGeneration.current += 1;
    };
  }, [load, refreshToken]);

  async function generate() {
    if (!baseline || actionActive.current) return;
    actionActive.current = true;
    requestGeneration.current += 1;
    setGenerating(true);
    try {
      const next = await generateSubmissionSections(
        tenderId,
        baseline.baseline_id,
        baseline.version,
        baseline.manifest_sha256,
      );
      setGeneration(next);
      setSubmissionPackage(null);
      setSelectedRequirementId(undefined);
      onTenderStateChange();
    } catch {
      reportCommandFailure();
    } finally {
      actionActive.current = false;
      setGenerating(false);
    }
  }

  const busy = loading || generating || assembling;

  async function assemble() {
    if (!generation || actionActive.current) return;
    actionActive.current = true;
    setAssembling(true);
    try {
      const next = await assembleSubmissionPackage(
        tenderId,
        generation.generation_id,
        generation.manifest_sha256,
      );
      setSubmissionPackage(next);
      setLifecyclePhase(
        next.assessment === "complete" ? "final_review" : "package_production",
      );
      setSelectedRequirementId(next.coverage[0]?.requirement.requirement_id);
      requestAnimationFrame(() =>
        document.getElementById("submission-package-detail")?.focus(),
      );
      onTenderStateChange();
    } catch {
      reportCommandFailure();
    } finally {
      actionActive.current = false;
      setAssembling(false);
    }
  }

  async function loadContent(key: string, loader: () => Promise<number>) {
    setLoadedContent((current) => ({ ...current, [key]: "loading" }));
    try {
      const byteLength = await loader();
      setLoadedContent((current) => ({
        ...current,
        [key]: byteLength,
      }));
    } catch {
      reportCommandFailure();
      setLoadedContent((current) => ({ ...current, [key]: "failed" }));
    }
  }

  async function loadExactBytes(
    artifactId: string,
    version: number,
    manifestSha256: string,
  ) {
    await loadContent(`${artifactId}:${version}`, async () => {
      const content = await inspectSubmissionArtifactContent(
        tenderId,
        artifactId,
        version,
        manifestSha256,
      );
      return content.bytes.length;
    });
  }

  async function loadPackageItemBytes(itemId: string) {
    if (!submissionPackage) return;
    await loadContent(`package:${itemId}`, async () => {
      const content = await inspectSubmissionPackageItemContent(
        tenderId,
        submissionPackage.package_id,
        submissionPackage.version,
        submissionPackage.manifest_sha256,
        itemId,
      );
      return content.bytes.length;
    });
  }

  function selectCoverage(requirementId: string) {
    setSelectedRequirementId(requirementId);
    requestAnimationFrame(() =>
      document.getElementById("submission-package-detail")?.focus(),
    );
  }

  function navigateDependency(referenceId: string) {
    const target = submissionPackage?.coverage.find((coverage) => {
      const requirement = coverage.requirement;
      return [
        ...requirement.calculation_references,
        ...requirement.review_references,
        ...requirement.decision_references,
      ].some(
        (reference) =>
          reference === referenceId || reference.startsWith(`${referenceId}:`),
      );
    });
    if (target) selectCoverage(target.requirement.requirement_id);
  }

  function exactDecisionSubject(
    reference: SubmissionDecisionReference,
  ): string | null {
    const exactSubject = submissionPackage?.coverage.find(
      ({ requirement }) =>
        reference.subject_kind === "tender_record_version" &&
        requirement.record.record_id === reference.subject_reference_id &&
        requirement.record.version === reference.subject_version &&
        requirement.record.manifest_sha256 ===
          reference.subject_manifest_sha256,
    );
    return exactSubject?.requirement.requirement_id ?? null;
  }

  const selectedCoverage = submissionPackage?.coverage.find(
    (coverage) => coverage.requirement.requirement_id === selectedRequirementId,
  );
  const selectedItem = submissionPackage?.items.find(
    (item) => item.item_id === selectedCoverage?.item_id,
  );

  return (
    <section
      className="workspace-card"
      aria-labelledby="package-production-heading"
    >
      <div className="workspace-card__heading">
        <div>
          <p className="section-label">Controlled authoring</p>
          <h2 id="package-production-heading">Submission Package Production</h2>
        </div>
        <button
          type="button"
          disabled={
            !runtimeReady ||
            !baseline ||
            lifecyclePhase !== "package_production" ||
            busy
          }
          onClick={() => void generate()}
        >
          {generating ? "Generating…" : "Generate exact sections"}
        </button>
      </div>

      {baseline ? (
        <div className="notice">
          <strong>Exact approved Coordinated Bid Baseline</strong>
          <p>
            {baseline.baseline_id} v{baseline.version} / manifest{" "}
            <code>{baseline.manifest_sha256}</code>
          </p>
          <p>
            Generation uses only current Verified Tender instructions. It cannot
            infer requirements from renderer content.
          </p>
        </div>
      ) : (
        <div className="notice notice-warning" role="status">
          No current approved Coordinated Bid Baseline is available for Package
          Production.
        </div>
      )}

      {generation ? (
        <article className="record-card">
          <p className="eyebrow">
            Immutable generation {generation.sequence} / {generation.created_at}
          </p>
          <h3>
            {generation.artifact_versions.length} generated Artifact Versions
          </h3>
          <p>
            Generation manifest <code>{generation.manifest_sha256}</code>
          </p>
          <button
            type="button"
            disabled={
              !runtimeReady || lifecyclePhase !== "package_production" || busy
            }
            onClick={() => void assemble()}
          >
            {assembling ? "Assembling…" : "Assemble exact package"}
          </button>
          <div className="record-list">
            {generation.artifact_versions.map((artifact) => (
              <article
                className="record-card record-card-compact"
                key={`${artifact.artifact_id}-${artifact.version}`}
              >
                <strong>{artifact.package_path}</strong>
                <p>
                  {artifact.section_key} / envelope {artifact.envelope_key} /{" "}
                  {artifact.language} / {humanize(artifact.authoring_mode)}
                </p>
                <p>
                  Artifact {artifact.artifact_id} v{artifact.version} /{" "}
                  {artifact.size_bytes.toString()} bytes
                </p>
                <p>
                  Content <code>{artifact.content_sha256}</code>
                </p>
                <p>
                  Artifact manifest <code>{artifact.manifest_sha256}</code>
                </p>
                <details>
                  <summary>Exact inputs and provenance</summary>
                  <dl>
                    <div>
                      <dt>Exact inputs</dt>
                      <dd>{artifact.exact_inputs.join(" · ")}</dd>
                    </div>
                    <div>
                      <dt>Provenance</dt>
                      <dd>{artifact.provenance.join(" · ")}</dd>
                    </div>
                  </dl>
                </details>
                <button
                  type="button"
                  disabled={
                    loadedContent[
                      `${artifact.artifact_id}:${artifact.version}`
                    ] === "loading"
                  }
                  onClick={() =>
                    void loadExactBytes(
                      artifact.artifact_id,
                      artifact.version,
                      artifact.manifest_sha256,
                    )
                  }
                >
                  Load exact bytes
                </button>
                {typeof loadedContent[
                  `${artifact.artifact_id}:${artifact.version}`
                ] === "number" ? (
                  <p role="status">
                    {
                      loadedContent[
                        `${artifact.artifact_id}:${artifact.version}`
                      ]
                    }{" "}
                    exact bytes loaded and digest-verified.
                  </p>
                ) : loadedContent[
                    `${artifact.artifact_id}:${artifact.version}`
                  ] === "failed" ? (
                  <p role="alert">Exact immutable bytes are unavailable.</p>
                ) : null}
              </article>
            ))}
          </div>

          <details>
            <summary>
              {generation.requirements.length} immutable Generation Requirements
            </summary>
            <div className="record-list">
              {generation.requirements.map((requirement) => (
                <article
                  className="record-card record-card-compact"
                  key={requirement.requirement_id}
                >
                  <strong>{humanize(requirement.kind)}</strong>
                  <p>{requirement.record.title}</p>
                  <p>
                    {requirement.mandatory ? "Mandatory" : "Optional"} ·{" "}
                    {requirement.section_key} ·{" "}
                    {humanize(requirement.authoring_mode)}
                  </p>
                  <p>
                    {requirement.package_path} · envelope{" "}
                    {requirement.envelope_key} · {requirement.language}
                  </p>
                  <p>
                    Verified record {requirement.record.record_id} v
                    {requirement.record.version} / {requirement.evidence.length}{" "}
                    Evidence references
                  </p>
                  <p>
                    Record manifest{" "}
                    <code>{requirement.record.manifest_sha256}</code>
                  </p>
                  <p>
                    {requirement.availability === "available" ? (
                      <>
                        Content <code>{requirement.content_sha256}</code> /{" "}
                        {requirement.size_bytes?.toString()} bytes
                      </>
                    ) : (
                      `${humanize(requirement.availability)} — no package bytes were invented`
                    )}
                  </p>
                  <p>
                    Requirement manifest{" "}
                    <code>{requirement.manifest_sha256}</code>
                  </p>
                  <p>
                    {requirement.generated_artifact
                      ? `Generated Artifact ${requirement.generated_artifact.artifact_id} v${requirement.generated_artifact.version}`
                      : requirement.unchanged_source_artifact
                        ? `Unchanged Source Artifact ${requirement.unchanged_source_artifact.artifact_id} v${requirement.unchanged_source_artifact.version}`
                        : `${humanize(requirement.availability)} — no immutable source item`}
                  </p>
                  <dl>
                    {requirement.authored_fields.map((field) => (
                      <div key={field.name}>
                        <dt>{humanize(field.name)}</dt>
                        <dd>
                          {field.value ?? field.normalized_value}
                          {field.original_expression
                            ? ` · original ${field.original_expression}`
                            : ""}
                          {field.uncertainty
                            ? ` · uncertainty ${field.uncertainty}`
                            : ""}
                          {` · ${humanize(field.basis_kind)}`}
                          {field.basis_reference
                            ? ` · ${field.basis_reference}`
                            : ""}
                        </dd>
                      </div>
                    ))}
                    <div>
                      <dt>Calculation references</dt>
                      <dd>
                        {requirement.calculation_references.join(" · ") ||
                          "None"}
                      </dd>
                    </div>
                    <div>
                      <dt>Review references</dt>
                      <dd>
                        {requirement.review_references.join(" · ") || "None"}
                      </dd>
                    </div>
                    <div>
                      <dt>Decision references</dt>
                      <dd>
                        {requirement.decision_references.join(" · ") || "None"}
                      </dd>
                    </div>
                  </dl>
                  <details>
                    <summary>
                      {requirement.evidence.length} exact Evidence references
                    </summary>
                    {requirement.evidence.map((evidence) => (
                      <article
                        className="notice"
                        key={`${evidence.reference.artifact_id}-${evidence.reference.version}-${evidence.reference.ordinal}`}
                      >
                        <p>
                          {evidence.package_path} ·{" "}
                          {humanize(evidence.location.kind)}{" "}
                          {evidence.location.ordinal} ·{" "}
                          {evidence.location.structural_path}
                        </p>
                        <blockquote
                          dir={evidenceDirection(evidence.location.direction)}
                        >
                          {evidence.location.original_text}
                        </blockquote>
                        {evidence.location.translated_text ? (
                          <div>
                            <p>Derived translation — non-authoritative</p>
                            <blockquote dir="auto">
                              {evidence.location.translated_text}
                            </blockquote>
                          </div>
                        ) : null}
                      </article>
                    ))}
                  </details>
                </article>
              ))}
            </div>
          </details>
        </article>
      ) : (
        <p role="status">
          {loading
            ? "Inspecting controlled Package Production…"
            : "No Submission Sections have been published."}
        </p>
      )}

      {submissionPackage ? (
        <article
          className="record-card"
          aria-labelledby="submission-package-title"
        >
          <p className="eyebrow">
            {humanize(submissionPackage.status)} /{" "}
            {humanize(submissionPackage.assessment)} Submission Package v
            {submissionPackage.version}
          </p>
          <h3 id="submission-package-title">Canonical coverage manifest</h3>
          <p>
            Package <code>{submissionPackage.package_id}</code> / root{" "}
            <code>{submissionPackage.manifest_sha256}</code>
          </p>
          <p>
            Exact Generation {submissionPackage.generation_id} / approved
            baseline {submissionPackage.baseline_id} v
            {submissionPackage.baseline_version} / Work Plan{" "}
            {submissionPackage.work_plan.plan_id} v
            {submissionPackage.work_plan.plan_version}
          </p>
          <p role="status">
            {submissionPackage.current
              ? "This exact package is current."
              : "This immutable package is stale and remains available for review."}
          </p>
          {!submissionPackage.current ? (
            <ul aria-label="Submission package currentness">
              {submissionPackage.currentness_facts
                .filter((fact) => !fact.current)
                .map((fact) => (
                  <li key={`${fact.code}-${fact.reference_id}`}>
                    {humanize(fact.code)}: {fact.reference_id} / expected{" "}
                    <code>{fact.expected_value}</code>
                  </li>
                ))}
            </ul>
          ) : null}
          <nav aria-label="Exact package dependencies">
            {submissionPackage.calculation_manifest_references.map(
              (reference) => (
                <button
                  key={`${reference.kind}-${reference.reference_id}-${reference.version}`}
                  type="button"
                  onClick={() => navigateDependency(reference.reference_id)}
                >
                  {humanize(reference.kind)} {reference.reference_id} v
                  {reference.version} / <code>{reference.manifest_sha256}</code>
                </button>
              ),
            )}
            {submissionPackage.current_decision_references.map((reference) => {
              const targetRequirementId = exactDecisionSubject(reference);
              const key = `${reference.kind}-${reference.decision_id}-${reference.subject_kind}-${reference.subject_reference_id}-${reference.subject_version}`;
              const label = (
                <>
                  {humanize(reference.kind)} {reference.decision_id} / subject{" "}
                  {humanize(reference.subject_kind)}{" "}
                  {reference.subject_reference_id} v{reference.subject_version}{" "}
                  / <code>{reference.subject_manifest_sha256}</code>
                </>
              );
              return targetRequirementId ? (
                <button
                  key={key}
                  type="button"
                  onClick={() => selectCoverage(targetRequirementId)}
                >
                  {label}
                </button>
              ) : (
                <span key={key}>{label}</span>
              );
            })}
          </nav>
          <div className="decision-cockpit-layout">
            <nav aria-label="Submission requirement coverage">
              <ul className="record-list">
                {submissionPackage.coverage.map((coverage) => (
                  <li key={coverage.requirement.requirement_id}>
                    <button
                      type="button"
                      aria-current={
                        selectedRequirementId ===
                        coverage.requirement.requirement_id
                          ? "true"
                          : undefined
                      }
                      onClick={() =>
                        selectCoverage(coverage.requirement.requirement_id)
                      }
                    >
                      {humanize(coverage.requirement.kind)} —{" "}
                      {humanize(coverage.disposition)}
                    </button>
                  </li>
                ))}
              </ul>
            </nav>
            {selectedCoverage ? (
              <section
                id="submission-package-detail"
                tabIndex={-1}
                aria-labelledby="submission-package-detail-title"
              >
                <h4 id="submission-package-detail-title">
                  {selectedCoverage.requirement.record.title}
                </h4>
                <p>
                  Verified requirement{" "}
                  <code>{selectedCoverage.requirement.requirement_id}</code> /
                  record {selectedCoverage.requirement.record.record_id} v
                  {selectedCoverage.requirement.record.version}
                </p>
                <p>
                  Disposition: {humanize(selectedCoverage.disposition)} / path{" "}
                  {selectedCoverage.requirement.package_path}
                </p>
                {selectedCoverage.blockers.map((blocker) => (
                  <p
                    className="notice notice-warning"
                    key={`${blocker.code}-${blocker.detail}`}
                  >
                    {humanize(blocker.code)}: {blocker.detail}
                  </p>
                ))}
                {selectedItem ? (
                  <article className="notice">
                    <strong>Exact package item</strong>
                    <p>
                      <code>{selectedItem.item_id}</code> /{" "}
                      {selectedItem.media_type} /{" "}
                      {selectedItem.size_bytes.toString()} bytes
                    </p>
                    <p>
                      Content <code>{selectedItem.content_sha256}</code> /
                      validation{" "}
                      <code>{selectedItem.validation_context_sha256}</code>
                    </p>
                    <details>
                      <summary>Exact source and provenance</summary>
                      <p>
                        {humanize(selectedItem.source.kind)} artifact{" "}
                        {selectedItem.source.artifact_id} v
                        {selectedItem.source.version} /{" "}
                        {selectedItem.source.size_bytes.toString()} bytes /
                        content{" "}
                        <code>{selectedItem.source.content_sha256}</code>
                      </p>
                      {selectedItem.source.kind === "generated" ? (
                        <p>
                          Artifact manifest{" "}
                          <code>{selectedItem.source.manifest_sha256}</code>
                        </p>
                      ) : null}
                      <ul>
                        {selectedItem.provenance.map((reference) => (
                          <li key={reference}>{reference}</li>
                        ))}
                      </ul>
                    </details>
                    <details>
                      <summary>Evidence</summary>
                      {selectedItem.evidence.map((evidence) => (
                        <article
                          key={`${evidence.reference.artifact_id}-${evidence.reference.version}-${evidence.reference.ordinal}`}
                        >
                          <p>
                            {evidence.package_path} /{" "}
                            {evidence.reference.artifact_id} v
                            {evidence.reference.version} / location{" "}
                            {evidence.reference.ordinal}
                          </p>
                          <blockquote
                            dir={evidenceDirection(evidence.location.direction)}
                          >
                            {evidence.location.original_text}
                          </blockquote>
                          {evidence.location.translated_text ? (
                            <div>
                              <p>Derived translation — non-authoritative</p>
                              <blockquote dir="auto">
                                {evidence.location.translated_text}
                              </blockquote>
                            </div>
                          ) : null}
                        </article>
                      ))}
                    </details>
                    <details>
                      <summary>Calculations, reviews, and decisions</summary>
                      <p>
                        Calculations:{" "}
                        {selectedItem.calculation_references.join(" · ") ||
                          "None"}
                      </p>
                      <p>
                        Reviews:{" "}
                        {selectedItem.review_references.join(" · ") || "None"}
                      </p>
                      <p>
                        Decisions:{" "}
                        {selectedItem.decision_references.join(" · ") || "None"}
                      </p>
                    </details>
                    <button
                      type="button"
                      disabled={
                        loadedContent[`package:${selectedItem.item_id}`] ===
                        "loading"
                      }
                      onClick={() =>
                        void loadPackageItemBytes(selectedItem.item_id)
                      }
                    >
                      Load and verify exact item bytes
                    </button>
                    {typeof loadedContent[`package:${selectedItem.item_id}`] ===
                    "number" ? (
                      <p role="status">
                        {loadedContent[`package:${selectedItem.item_id}`]} exact
                        bytes loaded.
                      </p>
                    ) : null}
                    {loadedContent[`package:${selectedItem.item_id}`] ===
                    "failed" ? (
                      <p role="alert">
                        Exact immutable item bytes are unavailable. Retry the
                        exact item load.
                      </p>
                    ) : null}
                  </article>
                ) : null}
              </section>
            ) : null}
          </div>
        </article>
      ) : generation ? (
        <p role="status">
          No immutable Submission Package Version has been assembled.
        </p>
      ) : null}
    </section>
  );
}
