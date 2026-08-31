import { CircleAlert, FileText, LoaderCircle, Search } from "lucide-react";
import { type FormEvent, useEffect, useMemo, useState } from "react";

import type { EvidenceDocument } from "./bindings/EvidenceDocument";
import type { EvidenceLocation } from "./bindings/EvidenceLocation";
import type { EvidenceSearchResult } from "./bindings/EvidenceSearchResult";
import { evidenceTextAttributes } from "./evidenceTypography";
import { inspectEvidence, searchEvidence } from "./quantixHost";

export type EvidenceReviewTarget = {
  artifactId: string;
  version: number;
  ordinal: number | null;
  label: string;
};

export type EvidenceReviewConflict = {
  artifactId: string;
  version: number;
  ordinal: number | null;
  label: string;
};

interface TenderEvidenceReviewProps {
  tenderId: string;
  target: EvidenceReviewTarget;
  conflicts: EvidenceReviewConflict[];
  originLabel: string;
  onOpenTarget: (target: EvidenceReviewTarget) => void;
  onClose: () => void;
}

const MAX_CONFLICT_SOURCES = 4;

function locationId(artifactId: string, version: number, ordinal: number) {
  return `tender-evidence-location-${artifactId}-${version}-${ordinal}`;
}

export function evidenceLocationLabel(location: EvidenceLocation): string {
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
}

function useEvidenceDocument(
  tenderId: string,
  artifactId: string,
  version: number,
): { document: EvidenceDocument | null; failed: boolean } {
  const [document, setDocument] = useState<EvidenceDocument | null>(null);
  const [failed, setFailed] = useState(false);
  useEffect(() => {
    let active = true;
    setDocument(null);
    setFailed(false);
    inspectEvidence(tenderId, artifactId, version)
      .then((next) => {
        if (active) setDocument(next);
      })
      .catch(() => {
        if (active) setFailed(true);
      });
    return () => {
      active = false;
    };
  }, [artifactId, tenderId, version]);
  return { document, failed };
}

function LocationProvenance({ location }: { location: EvidenceLocation }) {
  return (
    <details className="tender-evidence__provenance">
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
  );
}

function LocationText({ location }: { location: EvidenceLocation }) {
  return (
    <>
      <blockquote {...evidenceTextAttributes(location)}>
        {location.original_text}
      </blockquote>
      {location.translated_text ? (
        <div className="tender-evidence__translation">
          <p>Derived translation — non-authoritative</p>
          <blockquote dir="auto">{location.translated_text}</blockquote>
        </div>
      ) : null}
    </>
  );
}

function ConflictSource({
  tenderId,
  conflict,
}: {
  tenderId: string;
  conflict: EvidenceReviewConflict;
}) {
  const { document, failed } = useEvidenceDocument(
    tenderId,
    conflict.artifactId,
    conflict.version,
  );
  const location = useMemo(() => {
    if (!document) return null;
    return (
      document.locations.find(
        (candidate) =>
          conflict.ordinal === null || candidate.ordinal === conflict.ordinal,
      ) ?? document.locations[0]
    );
  }, [conflict.ordinal, document]);

  return (
    <article className="tender-evidence__conflict-source">
      <header>
        <FileText size={16} aria-hidden="true" />
        <div>
          <strong>{conflict.label}</strong>
          <span>
            v{conflict.version}
            {location
              ? ` · ${evidenceLocationLabel(location) || location.structural_path}`
              : ""}
          </span>
        </div>
      </header>
      {failed ? (
        <p className="tender-evidence__error" role="alert">
          Quantix could not open this source.
        </p>
      ) : location ? (
        <>
          <LocationText location={location} />
          <LocationProvenance location={location} />
        </>
      ) : (
        <p className="tender-evidence__loading" role="status">
          <LoaderCircle size={15} aria-hidden="true" /> Opening the cited
          passage…
        </p>
      )}
    </article>
  );
}

export function TenderEvidenceReview({
  tenderId,
  target,
  conflicts,
  originLabel,
  onOpenTarget,
  onClose,
}: TenderEvidenceReviewProps) {
  const [query, setQuery] = useState("");
  const [searchResult, setSearchResult] = useState<EvidenceSearchResult | null>(
    null,
  );
  const [searching, setSearching] = useState(false);
  const [searchFailed, setSearchFailed] = useState(false);
  const [jumpOrdinal, setJumpOrdinal] = useState<number | null>(null);
  const { document, failed } = useEvidenceDocument(
    tenderId,
    target.artifactId,
    target.version,
  );
  const activeOrdinal = jumpOrdinal ?? target.ordinal;

  useEffect(() => {
    setQuery("");
    setSearchResult(null);
    setSearchFailed(false);
    setJumpOrdinal(null);
  }, [target.artifactId, target.version]);

  useEffect(() => {
    if (!document || activeOrdinal === null) return;
    window.document
      .getElementById(
        locationId(document.artifact_id, document.version, activeOrdinal),
      )
      ?.scrollIntoView?.({ block: "center" });
  }, [activeOrdinal, document]);

  const handleSearch = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const exactQuery = query.trim();
    if (!exactQuery || searching) return;
    setSearching(true);
    setSearchFailed(false);
    try {
      setSearchResult(await searchEvidence(tenderId, exactQuery));
    } catch {
      setSearchFailed(true);
    } finally {
      setSearching(false);
    }
  };

  const openMatch = (artifactId: string, version: number, ordinal: number) => {
    if (artifactId === target.artifactId && version === target.version) {
      setJumpOrdinal(ordinal);
      return;
    }
    onOpenTarget({
      artifactId,
      version,
      ordinal,
      label: `${artifactId.slice(0, 12)} · v${version}`,
    });
  };

  const conflictSources = useMemo(
    () =>
      conflicts
        .filter(
          (conflict) =>
            conflict.artifactId !== target.artifactId ||
            conflict.version !== target.version ||
            target.ordinal === null ||
            conflict.ordinal === null ||
            conflict.ordinal !== target.ordinal,
        )
        .slice(0, MAX_CONFLICT_SOURCES),
    [conflicts, target.artifactId, target.ordinal, target.version],
  );

  return (
    <section
      className="tender-evidence"
      data-testid="tender-evidence-review"
      aria-labelledby="tender-evidence-title"
    >
      <header className="tender-evidence__header">
        <div>
          <p className="section-label">Source evidence</p>
          <h2 id="tender-evidence-title">{target.label}</h2>
          <p>
            Exact source v{target.version}
            {target.ordinal !== null ? ` · passage ${target.ordinal}` : ""}.
            Original text is authoritative.
          </p>
        </div>
        <button
          type="button"
          className="manager-workspace__secondary"
          onClick={onClose}
        >
          Back to {originLabel}
        </button>
      </header>

      {failed ? (
        <p className="tender-evidence__error" role="alert">
          <CircleAlert size={16} aria-hidden="true" /> Quantix could not open
          this source. Close and try again.
        </p>
      ) : !document ? (
        <p className="tender-evidence__loading" role="status">
          <LoaderCircle size={16} aria-hidden="true" /> Opening the original
          source…
        </p>
      ) : (
        <>
          <form className="tender-evidence__search" onSubmit={handleSearch}>
            <label htmlFor="tender-evidence-query">
              Find exact words in this Tender&apos;s sources
            </label>
            <div>
              <input
                id="tender-evidence-query"
                type="search"
                value={query}
                maxLength={512}
                autoComplete="off"
                onChange={(event) => setQuery(event.target.value)}
              />
              <button
                type="submit"
                className="manager-workspace__secondary"
                disabled={!query.trim() || searching}
              >
                <Search size={15} aria-hidden="true" />
                {searching ? "Searching…" : "Search"}
              </button>
            </div>
          </form>
          {searchFailed ? (
            <p className="tender-evidence__error" role="alert">
              Quantix could not run that search.
            </p>
          ) : null}
          {searchResult ? (
            <div className="tender-evidence__search-results" aria-live="polite">
              <p>
                {searchResult.matches.length} exact match
                {searchResult.matches.length === 1 ? "" : "es"} for “
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
                          openMatch(
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
                        <q {...evidenceTextAttributes(match.location)}>
                          {match.location.original_text}
                        </q>
                      </button>
                    </li>
                  ))}
                </ul>
              ) : null}
            </div>
          ) : null}

          <ol className="tender-evidence__locations">
            {document.locations.map((location) => (
              <li
                key={location.ordinal}
                id={locationId(
                  document.artifact_id,
                  document.version,
                  location.ordinal,
                )}
                className={
                  activeOrdinal === location.ordinal
                    ? "tender-evidence__location is-highlighted"
                    : "tender-evidence__location"
                }
              >
                <div className="tender-evidence__location-heading">
                  <strong>
                    {location.kind.replace(/_/g, " ")} {location.ordinal}
                  </strong>
                  <span>
                    {evidenceLocationLabel(location) ||
                      location.structural_path}
                  </span>
                </div>
                <LocationText location={location} />
                <LocationProvenance location={location} />
              </li>
            ))}
          </ol>
        </>
      )}

      {conflictSources.length > 0 ? (
        <section
          className="tender-evidence__conflicts"
          aria-label="Sources that disagree"
        >
          <h3>Sources that disagree</h3>
          <p>
            Other cited sources contradict this passage. Compare them before
            deciding.
          </p>
          <div className="tender-evidence__conflict-list">
            {conflictSources.map((conflict) => (
              <ConflictSource
                key={`${conflict.artifactId}-${conflict.version}-${conflict.ordinal ?? 0}`}
                tenderId={tenderId}
                conflict={conflict}
              />
            ))}
          </div>
        </section>
      ) : null}
    </section>
  );
}
