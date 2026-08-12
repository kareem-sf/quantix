import { useCallback, useEffect, useRef, useState } from "react";

import type { CoordinatedBidBaseline } from "./bindings/CoordinatedBidBaseline";
import type { PackageProductionGeneration } from "./bindings/PackageProductionGeneration";
import {
  generateSubmissionSections,
  inspectCoordinatedBidBaselines,
  inspectPackageProduction,
  inspectSubmissionArtifactContent,
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
  const [loading, setLoading] = useState(false);
  const [generating, setGenerating] = useState(false);
  const [loadedContent, setLoadedContent] = useState<
    Record<string, number | "loading" | "failed">
  >({});
  const requestGeneration = useRef(0);
  const actionActive = useRef(false);

  const load = useCallback(async () => {
    const request = ++requestGeneration.current;
    setLoading(true);
    try {
      const [baselines, production] = await Promise.all([
        inspectCoordinatedBidBaselines(tenderId, null, 1),
        inspectPackageProduction(tenderId),
      ]);
      if (request !== requestGeneration.current) return;
      const exactApproved = baselines.items.find(
        (item) =>
          item.current &&
          item.approval?.decision === "approve" &&
          baselines.lifecycle_phase === "package_production",
      );
      setBaseline(exactApproved);
      setGeneration(production);
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
      onTenderStateChange();
    } catch {
      reportCommandFailure();
    } finally {
      actionActive.current = false;
      setGenerating(false);
    }
  }

  const busy = loading || generating;

  async function loadExactBytes(
    artifactId: string,
    version: number,
    manifestSha256: string,
  ) {
    const key = `${artifactId}:${version}`;
    setLoadedContent((current) => ({ ...current, [key]: "loading" }));
    try {
      const content = await inspectSubmissionArtifactContent(
        tenderId,
        artifactId,
        version,
        manifestSha256,
      );
      setLoadedContent((current) => ({
        ...current,
        [key]: content.bytes.length,
      }));
    } catch {
      reportCommandFailure();
      setLoadedContent((current) => ({ ...current, [key]: "failed" }));
    }
  }

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
          disabled={!runtimeReady || !baseline || busy}
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
                    Content <code>{requirement.content_sha256}</code> /{" "}
                    {requirement.size_bytes.toString()} bytes
                  </p>
                  <p>
                    Requirement manifest{" "}
                    <code>{requirement.manifest_sha256}</code>
                  </p>
                  <p>
                    {requirement.generated_artifact
                      ? `Generated Artifact ${requirement.generated_artifact.artifact_id} v${requirement.generated_artifact.version}`
                      : `Unchanged Source Artifact ${requirement.unchanged_source_artifact?.artifact_id} v${requirement.unchanged_source_artifact?.version}`}
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
    </section>
  );
}
