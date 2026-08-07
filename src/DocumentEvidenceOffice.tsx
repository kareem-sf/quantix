import { useState } from "react";

import type { DocumentRegister } from "./bindings/DocumentRegister";
import type { DocumentRegisterEntry } from "./bindings/DocumentRegisterEntry";
import type { EvidenceDocument } from "./bindings/EvidenceDocument";
import { EvidenceReview } from "./EvidenceReview";
import {
  cancelSourceArtifactParse,
  inspectDocumentRegister,
  inspectEvidence,
  parseSourceArtifact,
} from "./quantixHost";

interface DocumentEvidenceOfficeProps {
  tenderId: string;
  register: DocumentRegister;
  updateRegister: (register: DocumentRegister) => void;
  reportCommandFailure: () => void;
}

interface ParseTarget {
  artifactId: string;
  version: number;
}

const readableState = (value: string) => value.replace(/_/g, " ");

export function DocumentEvidenceOffice({
  tenderId,
  register,
  updateRegister,
  reportCommandFailure,
}: DocumentEvidenceOfficeProps) {
  const [parsingTarget, setParsingTarget] = useState<ParseTarget>();
  const [evidence, setEvidence] = useState<EvidenceDocument>();

  const refreshRegister = async () => {
    updateRegister(await inspectDocumentRegister(tenderId));
  };

  const inspectEvidenceLocation = async (
    artifactId: string,
    version: number,
  ) => {
    setEvidence(await inspectEvidence(tenderId, artifactId, version));
  };

  const parseDocument = async (document: DocumentRegisterEntry) => {
    if (parsingTarget) return;
    const target = {
      artifactId: document.artifact_id,
      version: document.version,
    };
    setParsingTarget(target);
    try {
      const result = await parseSourceArtifact(
        tenderId,
        target.artifactId,
        target.version,
      );
      await refreshRegister();
      if (result.state === "parsed") {
        await inspectEvidenceLocation(target.artifactId, target.version);
      }
    } catch {
      reportCommandFailure();
    } finally {
      setParsingTarget(undefined);
    }
  };

  const cancelParse = async () => {
    if (!parsingTarget) return;
    try {
      await cancelSourceArtifactParse(
        tenderId,
        parsingTarget.artifactId,
        parsingTarget.version,
      );
    } catch {
      reportCommandFailure();
    }
  };

  return (
    <>
      <div className="document-register__table-wrap">
        <table>
          <thead>
            <tr>
              <th>Document</th>
              <th>Identity</th>
              <th>Version</th>
              <th>Language</th>
              <th>Type</th>
              <th>Digest</th>
              <th>Registration</th>
              <th>Parse</th>
              <th>Supersession</th>
              <th>Exception</th>
              <th>Evidence</th>
            </tr>
          </thead>
          <tbody>
            {register.documents.map((document) => {
              const isParsing =
                parsingTarget?.artifactId === document.artifact_id &&
                parsingTarget.version === document.version;
              return (
                <tr key={`${document.artifact_id}-${document.version}`}>
                  <td>{document.package_path}</td>
                  <td title={document.artifact_id}>
                    {document.artifact_id.slice(0, 8)}
                  </td>
                  <td>{document.version}</td>
                  <td>{document.language}</td>
                  <td>{readableState(document.document_type)}</td>
                  <td title={document.sha256 ?? "No registered digest"}>
                    {document.sha256?.slice(0, 12) ?? "—"}
                  </td>
                  <td>{readableState(document.registration_state)}</td>
                  <td>
                    {readableState(
                      isParsing ? "running" : document.parse_state,
                    )}
                    {document.parse_exception ? (
                      <small className="parse-exception">
                        {readableState(document.parse_exception)}
                      </small>
                    ) : null}
                  </td>
                  <td>{readableState(document.supersession_state)}</td>
                  <td>
                    {document.exception
                      ? readableState(document.exception)
                      : "—"}
                  </td>
                  <td>
                    {document.parse_state === "parsed" ? (
                      <button
                        type="button"
                        className="table-action"
                        onClick={() =>
                          void inspectEvidenceLocation(
                            document.artifact_id,
                            document.version,
                          ).catch(reportCommandFailure)
                        }
                      >
                        Inspect
                      </button>
                    ) : document.registration_state === "registered" ? (
                      isParsing ? (
                        <button
                          type="button"
                          className="table-action table-action--cancel"
                          onClick={() => void cancelParse()}
                        >
                          Cancel
                        </button>
                      ) : (
                        <button
                          type="button"
                          className="table-action"
                          disabled={Boolean(parsingTarget)}
                          onClick={() => void parseDocument(document)}
                        >
                          {document.parse_state === "not_requested"
                            ? "Parse"
                            : "Retry"}
                        </button>
                      )
                    ) : (
                      <span aria-label="Parsing unsupported">—</span>
                    )}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
      <EvidenceReview
        tenderId={tenderId}
        hasParsedDocuments={register.documents.some(
          (document) => document.parse_state === "parsed",
        )}
        evidence={evidence}
        inspectEvidenceLocation={inspectEvidenceLocation}
        reportCommandFailure={reportCommandFailure}
      />
    </>
  );
}
