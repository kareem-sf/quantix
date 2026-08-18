import { FormEvent, useEffect, useState } from "react";

import type { EvidenceDocument } from "./bindings/EvidenceDocument";
import type { EvidenceLocation } from "./bindings/EvidenceLocation";
import type { EvidenceSearchResult } from "./bindings/EvidenceSearchResult";
import { searchEvidence } from "./quantixHost";

interface EvidenceReviewProps {
  tenderId: string;
  hasParsedDocuments: boolean;
  evidence?: EvidenceDocument;
  inspectEvidenceLocation: (
    artifactId: string,
    version: number,
    ordinal?: number,
  ) => Promise<void>;
  reportCommandFailure: () => void;
}

const readableState = (value: string) => value.replace(/_/g, " ");

export const evidenceTextDirection = (location: EvidenceLocation) => {
  if (location.direction === "right_to_left") return "rtl";
  if (location.direction === "left_to_right") return "ltr";
  return "auto";
};

export const evidenceLocationLabel = (location: EvidenceLocation) => {
  const pages = Array.from(
    new Set(location.provenance.map((region) => region.page_number)),
  );
  return [
    pages.length > 0
      ? `${pages.length === 1 ? "page" : "pages"} ${pages.join(", ")}`
      : undefined,
    location.section ? `section ${location.section}` : undefined,
    location.paragraph_number
      ? `paragraph ${location.paragraph_number}`
      : undefined,
    location.table_number ? `table ${location.table_number}` : undefined,
    location.sheet_name ? `sheet ${location.sheet_name}` : undefined,
    location.cell_range ? `cell ${location.cell_range}` : undefined,
  ]
    .filter(Boolean)
    .join(" · ");
};

export function EvidenceLocationDetails({
  location,
  includeHeading = true,
}: {
  location: EvidenceLocation;
  includeHeading?: boolean;
}) {
  return (
    <>
      {includeHeading ? (
        <div className="evidence-location__heading">
          <strong>
            {readableState(location.kind)} {location.ordinal}
          </strong>
          <span>
            {evidenceLocationLabel(location) || location.structural_path}
          </span>
        </div>
      ) : null}
      <p className="evidence-authority">
        Authoritative source text · {location.language} ·{" "}
        {readableState(location.direction)}
      </p>
      <blockquote dir={evidenceTextDirection(location)}>
        {location.original_text}
      </blockquote>
      {location.translated_text ? (
        <div className="evidence-translation">
          <p>Derived translation — non-authoritative</p>
          <blockquote dir="auto">{location.translated_text}</blockquote>
        </div>
      ) : null}
      <details>
        <summary>Exact provenance</summary>
        <dl>
          <div>
            <dt>Structural path</dt>
            <dd>{location.structural_path}</dd>
          </div>
          {location.provenance.map((region, index) => (
            <div key={`${region.page_number}-${index}`}>
              <dt>Region {index + 1}</dt>
              <dd>
                Page {region.page_number}
                {region.char_start !== null
                  ? ` · characters ${region.char_start}–${region.char_end}`
                  : ""}
                {region.bounding_box
                  ? ` · box ${region.bounding_box.left},${region.bounding_box.top} → ${region.bounding_box.right},${region.bounding_box.bottom} · ${region.bounding_box.coordinate_origin}`
                  : ""}
              </dd>
            </div>
          ))}
        </dl>
      </details>
    </>
  );
}

export function EvidenceReview({
  tenderId,
  hasParsedDocuments,
  evidence,
  inspectEvidenceLocation,
  reportCommandFailure,
}: EvidenceReviewProps) {
  const [query, setQuery] = useState("");
  const [searchResult, setSearchResult] = useState<EvidenceSearchResult>();
  const [selectedOrdinal, setSelectedOrdinal] = useState<number>();

  useEffect(() => {
    setQuery("");
    setSearchResult(undefined);
    setSelectedOrdinal(undefined);
  }, [tenderId]);

  useEffect(() => {
    if (selectedOrdinal === undefined) return;
    document
      .getElementById(`evidence-location-${selectedOrdinal}`)
      ?.scrollIntoView({ block: "nearest" });
  }, [evidence, selectedOrdinal]);

  if (!hasParsedDocuments) return null;

  const handleSearch = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const exactQuery = query.trim();
    if (!exactQuery) return;
    try {
      setSearchResult(await searchEvidence(tenderId, exactQuery));
    } catch {
      reportCommandFailure();
    }
  };

  const navigateToMatch = async (
    artifactId: string,
    version: number,
    ordinal: number,
  ) => {
    try {
      await inspectEvidenceLocation(artifactId, version, ordinal);
      setSelectedOrdinal(ordinal);
    } catch {
      reportCommandFailure();
    }
  };

  return (
    <section
      className="evidence-review"
      aria-labelledby="evidence-review-title"
    >
      <div className="evidence-review__heading">
        <div>
          <p className="section-label">Engineer evidence review</p>
          <h5 id="evidence-review-title">Exact source evidence</h5>
        </div>
        {evidence ? (
          <span>
            {evidence.locations.length} locations · {evidence.language}
          </span>
        ) : null}
      </div>
      <form className="evidence-search" onSubmit={handleSearch}>
        <label htmlFor="evidence-query">Search authoritative source text</label>
        <div>
          <input
            id="evidence-query"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            maxLength={512}
            autoComplete="off"
          />
          <button type="submit" disabled={!query.trim()}>
            Search
          </button>
        </div>
      </form>
      {searchResult ? (
        <div className="evidence-search__results" aria-live="polite">
          <p>
            {searchResult.matches.length} exact matches for “
            {searchResult.query}”
          </p>
          {searchResult.matches.length > 0 ? (
            <ul>
              {searchResult.matches.map((match) => (
                <li
                  key={`${match.artifact_id}-${match.version}-${match.location.ordinal}`}
                >
                  <button
                    type="button"
                    onClick={() =>
                      void navigateToMatch(
                        match.artifact_id,
                        match.version,
                        match.location.ordinal,
                      )
                    }
                  >
                    <strong>{match.package_path}</strong>
                    <span>
                      {evidenceLocationLabel(match.location) ||
                        match.location.structural_path}
                    </span>
                    <q dir={evidenceTextDirection(match.location)}>
                      {match.location.original_text}
                    </q>
                  </button>
                </li>
              ))}
            </ul>
          ) : null}
        </div>
      ) : null}
      {evidence ? (
        <div className="evidence-document">
          <dl className="evidence-document__metadata">
            <div>
              <dt>Artifact and version</dt>
              <dd title={evidence.artifact_id}>
                {evidence.artifact_id.slice(0, 12)} · v{evidence.version}
              </dd>
            </div>
            <div>
              <dt>Markdown pipeline</dt>
              <dd>{evidence.pipeline_version}</dd>
            </div>
            <div>
              <dt>Markdown digest</dt>
              <dd title={evidence.markdown_sha256 ?? undefined}>
                {evidence.markdown_sha256?.slice(0, 16)}…
              </dd>
            </div>
          </dl>
          <ol className="evidence-locations">
            {evidence.locations.map((location) => (
              <li
                key={location.ordinal}
                id={`evidence-location-${location.ordinal}`}
                className={
                  selectedOrdinal === location.ordinal
                    ? "evidence-location evidence-location--selected"
                    : "evidence-location"
                }
              >
                <EvidenceLocationDetails location={location} />
              </li>
            ))}
          </ol>
        </div>
      ) : (
        <p className="catalogue-message">
          Inspect a parsed document or choose an exact search hit.
        </p>
      )}
    </section>
  );
}
