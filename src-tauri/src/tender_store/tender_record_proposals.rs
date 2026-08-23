#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::*;
    use crate::tender_store::{
        TenderEvidenceReference, TenderRecordAuthority, TenderRecordAuthorityKind,
    };

    fn evidence(artifact_id: &str, version: u32, ordinal: u32) -> TenderEvidenceReference {
        TenderEvidenceReference {
            artifact_id: artifact_id.into(),
            version,
            ordinal,
        }
    }

    fn authority(authority_id: &str, kind: TenderRecordAuthorityKind) -> TenderRecordAuthority {
        TenderRecordAuthority {
            authority_id: authority_id.into(),
            kind,
            value: "value".into(),
            description: "description".into(),
            manifest_sha256: None,
            tender_revision: 1,
            created_by: "engineer".into(),
            created_at: "2026-08-23T00:00:00Z".into(),
        }
    }

    #[test]
    fn proposal_context_uses_only_task_scoped_evidence_handles() {
        let evidence = vec![
            evidence("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", 1, 4),
            evidence("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", 2, 7),
        ];
        let context = TenderRecordProposalContext::new(&evidence, &[]).unwrap();

        assert_eq!(context.evidence_reference("e0001"), Some(&evidence[0]));
        assert_eq!(context.evidence_reference("e0002"), Some(&evidence[1]));
        assert_eq!(context.evidence_reference("e0003"), None);
        assert_eq!(
            context
                .provider_evidence()
                .map(|(handle, reference)| (handle, reference.clone()))
                .collect::<Vec<_>>(),
            vec![
                ("e0001", evidence[0].clone()),
                ("e0002", evidence[1].clone())
            ]
        );

        let schema: Value = serde_json::from_str(&context.output_contract_json().unwrap()).unwrap();
        assert_eq!(
            schema.pointer("/$defs/evidence_handle/enum").unwrap(),
            &json!(["e0001", "e0002"]),
        );
    }

    #[test]
    fn proposal_context_sorts_authority_handles_by_authority_id() {
        let authorities = vec![
            authority(
                "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                TenderRecordAuthorityKind::EngineerEntry,
            ),
            authority(
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                TenderRecordAuthorityKind::CalculationRun,
            ),
        ];
        let context = TenderRecordProposalContext::new(
            &[evidence("cccccccccccccccccccccccccccccccc", 1, 1)],
            &authorities,
        )
        .unwrap();

        assert_eq!(
            context.authority_reference("a0001").unwrap().authority_id,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );
        let schema: Value = serde_json::from_str(&context.output_contract_json().unwrap()).unwrap();
        assert_eq!(
            schema.pointer("/$defs/authority_handle/enum").unwrap(),
            &json!(["a0001", "a0002"]),
        );
    }

    #[test]
    fn evidence_basis_carries_only_evidence_handles() {
        let basis = TenderRecordFieldBasisProposal::Evidence {
            evidence: vec!["e0001".into()],
        };
        let value = serde_json::to_value(basis).unwrap();
        assert_eq!(value, json!({"kind": "evidence", "evidence": ["e0001"]}));
        assert!(value.get("basis_reference").is_none());
        assert!(value.get("basis_description").is_none());
    }

    #[test]
    fn assumption_basis_carries_its_stable_key_and_description() {
        let basis = TenderRecordFieldBasisProposal::Assumption {
            stable_key: "missing_schedule".into(),
            description: "The source does not state a schedule.".into(),
        };
        assert_eq!(
            serde_json::to_value(basis).unwrap(),
            json!({
                "kind": "assumption",
                "stable_key": "missing_schedule",
                "description": "The source does not state a schedule."
            })
        );
    }

    #[test]
    fn tender_query_basis_carries_its_stable_key_and_description() {
        let basis = TenderRecordFieldBasisProposal::TenderQuery {
            stable_key: "confirm_schedule".into(),
            description: "Ask the issuer for the schedule.".into(),
        };
        assert_eq!(
            serde_json::to_value(basis).unwrap(),
            json!({
                "kind": "tender_query",
                "stable_key": "confirm_schedule",
                "description": "Ask the issuer for the schedule."
            })
        );
    }

    #[test]
    fn calculation_run_basis_carries_only_an_authority_handle() {
        let basis = TenderRecordFieldBasisProposal::CalculationRun {
            authority: "a0001".into(),
        };
        assert_eq!(
            serde_json::to_value(basis).unwrap(),
            json!({"kind": "calculation_run", "authority": "a0001"})
        );
    }

    #[test]
    fn engineer_entry_basis_carries_only_an_authority_handle() {
        let basis = TenderRecordFieldBasisProposal::EngineerEntry {
            authority: "a0001".into(),
        };
        assert_eq!(
            serde_json::to_value(basis).unwrap(),
            json!({"kind": "engineer_entry", "authority": "a0001"})
        );
    }

    #[test]
    fn resolver_rejects_an_evidence_handle_from_another_task() {
        let context = TenderRecordProposalContext::new(
            &[evidence("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", 1, 4)],
            &[],
        )
        .unwrap();
        let payload = json!({
            "records": [{
                "stable_key": "submission_deadline",
                "kind": "deadline",
                "title": "Submission deadline",
                "generation_instruction": null,
                "fields": [{
                    "name": "deadline",
                    "value": "2026-08-23",
                    "basis": {"kind": "evidence", "evidence": ["e0002"]},
                    "original_expression": null,
                    "normalized_value": null,
                    "timezone": null,
                    "uncertainty": null
                }],
                "contradictions": []
            }]
        });

        let report = context.resolve(&payload.to_string()).unwrap_err();
        assert_eq!(
            report.issues,
            vec![TenderRecordValidationIssue {
                code: "unknown_evidence_handle".into(),
                pointer: "/records/0/fields/0/basis/evidence/0".into(),
            }]
        );
    }

    #[test]
    fn resolver_converts_task_handles_to_canonical_references() {
        let context = TenderRecordProposalContext::new(
            &[evidence("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", 1, 4)],
            &[],
        )
        .unwrap();
        let payload = json!({
            "records": [{
                "stable_key": "submission_deadline",
                "kind": "deadline",
                "title": "Submission deadline",
                "generation_instruction": null,
                "fields": [{
                    "name": "deadline",
                    "value": "2026-08-23",
                    "basis": {"kind": "evidence", "evidence": ["e0001"]},
                    "original_expression": "23 August 2026",
                    "normalized_value": null,
                    "timezone": null,
                    "uncertainty": null
                }],
                "contradictions": []
            }]
        });

        let resolved = context.resolve(&payload.to_string()).unwrap();
        assert_eq!(
            resolved.candidate.records[0].fields[0].evidence,
            vec![evidence("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", 1, 4)]
        );
        assert_eq!(
            serde_json::from_str::<Value>(&resolved.provider_payload_json).unwrap(),
            payload
        );
        assert_eq!(
            serde_json::from_str::<Value>(&resolved.canonical_payload_json)
                .unwrap()
                .pointer("/records/0/fields/0/evidence/0/artifact_id"),
            Some(&json!("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"))
        );
    }
}
use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::tender_records::{
    GenerationAuthoringMode, GenerationRequirementKind, TenderEvidenceReference,
    TenderRecordAuthority, TenderRecordAuthorityKind, TenderRecordCandidate,
    TenderRecordCandidateBatch, TenderRecordContradictionCandidate, TenderRecordFieldCandidate,
    TenderRecordGenerationInstructionCandidate, TenderRecordKind,
};
use crate::agent_runtime::TenderTaskView;

const MAX_HANDLES: usize = 9_999;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TenderRecordProposalBatch {
    pub records: Vec<TenderRecordProposal>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TenderRecordProposal {
    pub stable_key: String,
    pub kind: TenderRecordKind,
    pub title: String,
    pub generation_instruction: Option<TenderRecordGenerationInstructionProposal>,
    pub fields: Vec<TenderRecordFieldProposal>,
    pub contradictions: Vec<TenderRecordContradictionProposal>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TenderRecordGenerationInstructionProposal {
    pub kind: GenerationRequirementKind,
    pub mandatory: bool,
    pub section_key: String,
    pub package_path: String,
    pub envelope_key: String,
    pub language: String,
    pub authoring_mode: GenerationAuthoringMode,
    pub requested_authoring_format: Option<String>,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TenderRecordFieldProposal {
    pub name: String,
    pub value: Option<String>,
    pub basis: TenderRecordFieldBasisProposal,
    pub original_expression: Option<String>,
    pub normalized_value: Option<String>,
    pub timezone: Option<String>,
    pub uncertainty: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum TenderRecordFieldBasisProposal {
    Evidence {
        evidence: Vec<String>,
    },
    Assumption {
        stable_key: String,
        description: String,
    },
    TenderQuery {
        stable_key: String,
        description: String,
    },
    CalculationRun {
        authority: String,
    },
    EngineerEntry {
        authority: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TenderRecordContradictionProposal {
    pub field_name: String,
    pub summary: String,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TenderRecordValidationIssue {
    pub code: String,
    pub pointer: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TenderRecordValidationReport {
    pub issues: Vec<TenderRecordValidationIssue>,
}

impl TenderRecordValidationReport {
    fn one(code: &str, pointer: impl Into<String>) -> Self {
        Self {
            issues: vec![TenderRecordValidationIssue {
                code: code.into(),
                pointer: pointer.into(),
            }],
        }
    }

    fn push(&mut self, code: &str, pointer: impl Into<String>) {
        self.issues.push(TenderRecordValidationIssue {
            code: code.into(),
            pointer: pointer.into(),
        });
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.issues.is_empty()
    }
}

pub(crate) struct TenderRecordProposalContext {
    evidence_by_handle: BTreeMap<String, TenderEvidenceReference>,
    authority_by_handle: BTreeMap<String, TenderRecordAuthority>,
}

#[derive(Debug)]
pub(crate) struct ResolvedTenderRecordProposal {
    pub provider_payload_json: String,
    pub canonical_payload_json: String,
    pub candidate: TenderRecordCandidateBatch,
}

impl TenderRecordProposalContext {
    pub(crate) fn new(
        evidence: &[TenderEvidenceReference],
        authorities: &[TenderRecordAuthority],
    ) -> Result<Self, TenderRecordValidationReport> {
        if evidence.is_empty() || evidence.len() > MAX_HANDLES {
            return Err(TenderRecordValidationReport::one(
                "invalid_evidence_context",
                "/evidence",
            ));
        }
        let mut seen_evidence = HashSet::new();
        if evidence
            .iter()
            .any(|reference| !seen_evidence.insert(reference))
        {
            return Err(TenderRecordValidationReport::one(
                "duplicate_evidence_reference",
                "/evidence",
            ));
        }
        if authorities.len() > MAX_HANDLES {
            return Err(TenderRecordValidationReport::one(
                "invalid_authority_context",
                "/authorities",
            ));
        }
        let mut sorted_authorities = authorities.to_vec();
        sorted_authorities.sort_by(|left, right| left.authority_id.cmp(&right.authority_id));
        if sorted_authorities
            .windows(2)
            .any(|pair| pair[0].authority_id == pair[1].authority_id)
        {
            return Err(TenderRecordValidationReport::one(
                "duplicate_authority",
                "/authorities",
            ));
        }
        Ok(Self {
            evidence_by_handle: evidence
                .iter()
                .cloned()
                .enumerate()
                .map(|(index, reference)| (format!("e{:04}", index + 1), reference))
                .collect(),
            authority_by_handle: sorted_authorities
                .into_iter()
                .enumerate()
                .map(|(index, authority)| (format!("a{:04}", index + 1), authority))
                .collect(),
        })
    }

    pub(crate) fn from_task(
        task: &TenderTaskView,
        authorities: &[TenderRecordAuthority],
    ) -> Result<Self, TenderRecordValidationReport> {
        let mut evidence = Vec::new();
        for input in &task.exact_inputs {
            if input.kind == "source_evidence" {
                let Some((artifact_id, ordinal)) = input.reference.split_once('#') else {
                    return Err(TenderRecordValidationReport::one(
                        "invalid_task_context",
                        "/exact_inputs",
                    ));
                };
                let Ok(ordinal) = ordinal.parse() else {
                    return Err(TenderRecordValidationReport::one(
                        "invalid_task_context",
                        "/exact_inputs",
                    ));
                };
                evidence.push(TenderEvidenceReference {
                    artifact_id: artifact_id.into(),
                    version: input.version,
                    ordinal,
                });
            }
        }
        Self::new(&evidence, authorities)
    }

    pub(crate) fn evidence_reference(&self, handle: &str) -> Option<&TenderEvidenceReference> {
        self.evidence_by_handle.get(handle)
    }

    pub(crate) fn provider_evidence(
        &self,
    ) -> impl Iterator<Item = (&str, &TenderEvidenceReference)> {
        self.evidence_by_handle
            .iter()
            .map(|(handle, reference)| (handle.as_str(), reference))
    }

    pub(crate) fn authority_reference(&self, handle: &str) -> Option<&TenderRecordAuthority> {
        self.authority_by_handle.get(handle)
    }

    pub(crate) fn authority_handle(&self, authority_id: &str) -> Option<&str> {
        self.authority_by_handle
            .iter()
            .find_map(|(handle, authority)| {
                (authority.authority_id == authority_id).then_some(handle.as_str())
            })
    }

    pub(crate) fn output_contract_json(&self) -> Result<String, TenderRecordValidationReport> {
        let authority_handles = self.authority_by_handle.keys().cloned().collect();
        let calculation_authority_handles = self
            .authority_by_handle
            .iter()
            .filter(|(_, authority)| authority.kind == TenderRecordAuthorityKind::CalculationRun)
            .map(|(handle, _)| handle.clone())
            .collect();
        let engineer_entry_authority_handles = self
            .authority_by_handle
            .iter()
            .filter(|(_, authority)| authority.kind == TenderRecordAuthorityKind::EngineerEntry)
            .map(|(handle, _)| handle.clone())
            .collect();
        serde_json_canonicalizer::to_string(&proposal_schema(
            self.evidence_by_handle.keys().cloned().collect(),
            authority_handles,
            calculation_authority_handles,
            engineer_entry_authority_handles,
        ))
        .map_err(|_| TenderRecordValidationReport::one("contract_serialization_failed", ""))
    }

    pub(crate) fn resolve(
        &self,
        payload_json: &str,
    ) -> Result<ResolvedTenderRecordProposal, TenderRecordValidationReport> {
        let proposal: TenderRecordProposalBatch = serde_json::from_str(payload_json)
            .map_err(|_| TenderRecordValidationReport::one("invalid_provider_payload", ""))?;
        let provider_payload_json = serde_json_canonicalizer::to_string(&proposal)
            .map_err(|_| TenderRecordValidationReport::one("invalid_provider_payload", ""))?;
        let mut report = TenderRecordValidationReport { issues: Vec::new() };
        let candidate = TenderRecordCandidateBatch {
            records: proposal
                .records
                .into_iter()
                .enumerate()
                .map(|(record_index, record)| {
                    self.resolve_record(record, record_index, &mut report)
                })
                .collect(),
        };
        if !report.is_empty() {
            return Err(report);
        }
        let canonical_payload_json = serde_json_canonicalizer::to_string(&candidate)
            .map_err(|_| TenderRecordValidationReport::one("canonicalization_failed", ""))?;
        Ok(ResolvedTenderRecordProposal {
            provider_payload_json,
            canonical_payload_json,
            candidate,
        })
    }

    fn resolve_record(
        &self,
        record: TenderRecordProposal,
        record_index: usize,
        report: &mut TenderRecordValidationReport,
    ) -> TenderRecordCandidate {
        let base = format!("/records/{record_index}");
        TenderRecordCandidate {
            stable_key: record.stable_key,
            kind: record.kind,
            title: record.title,
            generation_instruction: record.generation_instruction.map(|instruction| {
                TenderRecordGenerationInstructionCandidate {
                    kind: instruction.kind,
                    mandatory: instruction.mandatory,
                    section_key: instruction.section_key,
                    package_path: instruction.package_path,
                    envelope_key: instruction.envelope_key,
                    language: instruction.language,
                    authoring_mode: instruction.authoring_mode,
                    requested_authoring_format: instruction.requested_authoring_format,
                    evidence: self.resolve_evidence(
                        instruction.evidence,
                        &format!("{base}/generation_instruction/evidence"),
                        report,
                    ),
                }
            }),
            fields: record
                .fields
                .into_iter()
                .enumerate()
                .map(|(field_index, field)| {
                    self.resolve_field(field, &format!("{base}/fields/{field_index}"), report)
                })
                .collect(),
            contradictions: record
                .contradictions
                .into_iter()
                .enumerate()
                .map(
                    |(index, contradiction)| TenderRecordContradictionCandidate {
                        field_name: contradiction.field_name,
                        summary: contradiction.summary,
                        evidence: self.resolve_evidence(
                            contradiction.evidence,
                            &format!("{base}/contradictions/{index}/evidence"),
                            report,
                        ),
                    },
                )
                .collect(),
        }
    }

    fn resolve_field(
        &self,
        field: TenderRecordFieldProposal,
        pointer: &str,
        report: &mut TenderRecordValidationReport,
    ) -> TenderRecordFieldCandidate {
        let (basis_kind, basis_reference, basis_description, evidence) = match field.basis {
            TenderRecordFieldBasisProposal::Evidence { evidence } => (
                super::TenderRecordBasisKind::Evidence,
                None,
                None,
                self.resolve_evidence(evidence, &format!("{pointer}/basis/evidence"), report),
            ),
            TenderRecordFieldBasisProposal::Assumption {
                stable_key,
                description,
            } => (
                super::TenderRecordBasisKind::Assumption,
                Some(stable_key),
                Some(description),
                Vec::new(),
            ),
            TenderRecordFieldBasisProposal::TenderQuery {
                stable_key,
                description,
            } => (
                super::TenderRecordBasisKind::TenderQuery,
                Some(stable_key),
                Some(description),
                Vec::new(),
            ),
            TenderRecordFieldBasisProposal::CalculationRun { authority } => {
                let authority = self.resolve_authority(
                    &authority,
                    TenderRecordAuthorityKind::CalculationRun,
                    &format!("{pointer}/basis/authority"),
                    report,
                );
                (
                    super::TenderRecordBasisKind::CalculationRun,
                    authority.map(|authority| authority.authority_id.clone()),
                    authority.map(|authority| authority.description.clone()),
                    Vec::new(),
                )
            }
            TenderRecordFieldBasisProposal::EngineerEntry { authority } => {
                let authority = self.resolve_authority(
                    &authority,
                    TenderRecordAuthorityKind::EngineerEntry,
                    &format!("{pointer}/basis/authority"),
                    report,
                );
                (
                    super::TenderRecordBasisKind::EngineerEntry,
                    authority.map(|authority| authority.authority_id.clone()),
                    authority.map(|authority| authority.description.clone()),
                    Vec::new(),
                )
            }
        };
        TenderRecordFieldCandidate {
            name: field.name,
            value: field.value,
            basis_kind,
            basis_reference,
            basis_description,
            original_expression: field.original_expression,
            normalized_value: field.normalized_value,
            timezone: field.timezone,
            uncertainty: field.uncertainty,
            evidence,
        }
    }

    fn resolve_evidence(
        &self,
        handles: Vec<String>,
        pointer: &str,
        report: &mut TenderRecordValidationReport,
    ) -> Vec<TenderEvidenceReference> {
        handles
            .into_iter()
            .enumerate()
            .filter_map(|(index, handle)| match self.evidence_reference(&handle) {
                Some(reference) => Some(reference.clone()),
                None => {
                    report.push("unknown_evidence_handle", format!("{pointer}/{index}"));
                    None
                }
            })
            .collect()
    }

    fn resolve_authority(
        &self,
        handle: &str,
        kind: TenderRecordAuthorityKind,
        pointer: &str,
        report: &mut TenderRecordValidationReport,
    ) -> Option<&TenderRecordAuthority> {
        match self.authority_reference(handle) {
            Some(authority) if authority.kind == kind => Some(authority),
            _ => {
                report.push("unknown_authority_handle", pointer);
                None
            }
        }
    }
}

fn proposal_schema(
    evidence_handles: Vec<String>,
    authority_handles: Vec<String>,
    calculation_authority_handles: Vec<String>,
    engineer_entry_authority_handles: Vec<String>,
) -> Value {
    let required_object = |properties: Value| {
        let required = properties
            .as_object()
            .expect("schema properties are an object")
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        json!({
            "type": "object",
            "additionalProperties": false,
            "properties": properties,
            "required": required,
        })
    };
    let evidence_handle = json!({"type": "string", "enum": evidence_handles});
    let evidence_list = json!({"type": "array", "minItems": 1, "maxItems": 32, "items": {"$ref": "#/$defs/evidence_handle"}});
    let mut basis_variants = vec![
        required_object(
            json!({"kind": {"const": "evidence"}, "evidence": {"$ref": "#/$defs/evidence_list"}}),
        ),
        required_object(
            json!({"kind": {"const": "assumption"}, "stable_key": {"$ref": "#/$defs/name"}, "description": {"$ref": "#/$defs/description"}}),
        ),
        required_object(
            json!({"kind": {"const": "tender_query"}, "stable_key": {"$ref": "#/$defs/name"}, "description": {"$ref": "#/$defs/description"}}),
        ),
    ];
    if !calculation_authority_handles.is_empty() {
        basis_variants.push(required_object(json!({"kind": {"const": "calculation_run"}, "authority": {"$ref": "#/$defs/calculation_authority_handle"}})));
    }
    if !engineer_entry_authority_handles.is_empty() {
        basis_variants.push(required_object(json!({"kind": {"const": "engineer_entry"}, "authority": {"$ref": "#/$defs/engineer_entry_authority_handle"}})));
    }
    let basis = json!({"anyOf": basis_variants});
    let field = required_object(json!({
        "name": {"$ref": "#/$defs/name"},
        "value": {"type": ["string", "null"], "maxLength": 4000},
        "basis": basis,
        "original_expression": {"type": ["string", "null"], "maxLength": 2000},
        "normalized_value": {"type": ["string", "null"], "maxLength": 2000},
        "timezone": {"type": ["string", "null"], "maxLength": 100},
        "uncertainty": {"type": ["string", "null"], "maxLength": 2000}
    }));
    let contradiction = required_object(json!({
        "field_name": {"$ref": "#/$defs/name"},
        "summary": {"$ref": "#/$defs/description"},
        "evidence": {"$ref": "#/$defs/evidence_list"}
    }));
    let instruction = required_object(json!({
        "kind": {"enum": ["mandatory_requirement", "deliverable", "addendum_instruction", "signature", "form_field", "execution_requirement", "required_file"]},
        "mandatory": {"type": "boolean"},
        "section_key": {"type": "string", "minLength": 1, "maxLength": 200},
        "package_path": {"type": "string", "minLength": 1, "maxLength": 1000},
        "envelope_key": {"type": "string", "minLength": 1, "maxLength": 200},
        "language": {"type": "string", "minLength": 1, "maxLength": 100},
        "authoring_mode": {"enum": ["docx", "xlsx", "unchanged_source", "unsupported"]},
        "requested_authoring_format": {"type": ["string", "null"], "maxLength": 200},
        "evidence": {"$ref": "#/$defs/evidence_list"}
    }));
    let record = required_object(json!({
        "stable_key": {"$ref": "#/$defs/name"},
        "kind": {"enum": ["requirement", "evaluation_criterion", "deliverable", "deadline", "form", "clause", "risk", "assumption", "tender_query", "project_characteristic"]},
        "title": {"type": "string", "minLength": 1, "maxLength": 500},
        "generation_instruction": {"anyOf": [instruction, {"type": "null"}]},
        "fields": {"type": "array", "minItems": 1, "maxItems": 64, "items": field},
        "contradictions": {"type": "array", "maxItems": 32, "items": contradiction}
    }));
    let mut root = required_object(json!({
        "records": {"type": "array", "minItems": 1, "maxItems": 256, "items": record}
    }));
    let mut definitions = json!({
        "evidence_handle": evidence_handle,
        "evidence_list": evidence_list,
        "name": {"type": "string", "minLength": 1, "maxLength": 100, "pattern": "^[a-z0-9][a-z0-9_-]*$"},
        "description": {"type": "string", "minLength": 1, "maxLength": 2000}
    });
    let definitions = definitions
        .as_object_mut()
        .expect("schema definitions are an object");
    if !authority_handles.is_empty() {
        definitions.insert(
            "authority_handle".into(),
            json!({"type": "string", "enum": authority_handles}),
        );
    }
    if !calculation_authority_handles.is_empty() {
        definitions.insert(
            "calculation_authority_handle".into(),
            json!({"type": "string", "enum": calculation_authority_handles}),
        );
    }
    if !engineer_entry_authority_handles.is_empty() {
        definitions.insert(
            "engineer_entry_authority_handle".into(),
            json!({"type": "string", "enum": engineer_entry_authority_handles}),
        );
    }
    root.as_object_mut()
        .expect("root schema object")
        .insert("$defs".into(), definitions.clone().into());
    root
}
