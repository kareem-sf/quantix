use std::{fs, io, path::Path, sync::Arc};

use quantix_lib::{
    ensure_quantix_setup, AgentRunRecoveryDisposition, AgentRunState, BootstrapAuthority,
    BootstrapRole, ConfirmSourceRelationshipCommand, CreateTenderCommand,
    CreateTenderEngineerEntryCommand, DecideTenderRecordCommand, DeviceProtection,
    ImportTenderPackageCommand, ParseSourceArtifactCommand, ProviderFailureCategory, QuantixHost,
    ResolveIndeterminateAgentRunCommand, ReviseTenderCommand, RunTenderRecordExtractionCommand,
    RunTenderRecordReviewCommand, RuntimeLayout, SetupPlatform, SetupState, SourceRelationshipKind,
    StoragePermissions, TenderEvidenceReference, TenderIntegrityState,
    TenderRecordAuthorityReference, TenderRecordBasisKind, TenderRecordEngineerDecisionKind,
    TenderRecordInspection, TenderRecordKind, TenderRecordReviewOutcome, TenderRecordTrustClass,
    VerificationStatus, MINIMUM_SETUP_FREE_SPACE_BYTES,
};

struct ReadySetupPlatform;

impl SetupPlatform for ReadySetupPlatform {
    fn available_space(&self, _path: &Path) -> io::Result<u64> {
        Ok(MINIMUM_SETUP_FREE_SPACE_BYTES)
    }

    fn is_writable(&self, _path: &Path) -> io::Result<bool> {
        Ok(true)
    }

    fn storage_permissions(&self, _path: &Path) -> io::Result<StoragePermissions> {
        Ok(StoragePermissions::Restrictive)
    }

    fn device_protection(&self, _path: &Path) -> DeviceProtection {
        DeviceProtection::Protected
    }
}

struct RuntimeHarness {
    _root: tempfile::TempDir,
    application_home: std::path::PathBuf,
    codex: std::path::PathBuf,
    host: QuantixHost,
    tender_id: String,
}

struct ParsedEvidence {
    artifact_id: String,
    version: u32,
    references: Vec<TenderEvidenceReference>,
}

impl RuntimeHarness {
    fn new(agent_scenario: &str) -> Self {
        let root = tempfile::tempdir().expect("temporary Tender Records runtime harness");
        let application_home = root.path().join(".quantix");
        let resources = root.path().join("resources");
        let codex = install_codex_fixture(&resources, agent_scenario);
        let host = QuantixHost::with_setup_platform_and_runtime(
            &application_home,
            Arc::new(ReadySetupPlatform),
            RuntimeLayout::bundled(resources),
        );
        host.accept_runtime_fixture();
        assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
        install_docling_fixture(&application_home);
        let tender = host
            .create_tender(CreateTenderCommand {
                name: "Bilingual evidence-backed Tender".into(),
            })
            .expect("create Tender");
        Self {
            _root: root,
            application_home,
            codex,
            host,
            tender_id: tender.tender_id,
        }
    }

    fn set_agent_scenario(&self, scenario: &str) {
        fs::write(self.codex.with_extension("agent-scenario"), scenario)
            .expect("update fake app-server scenario");
    }

    async fn parsed_pdf_evidence(&self, name: &str, marker: &[u8]) -> ParsedEvidence {
        let source = self._root.path().join(format!("{name}-source"));
        fs::create_dir(&source).expect("source directory");
        let mut bytes = b"%PDF-1.7\n".to_vec();
        bytes.extend_from_slice(marker);
        bytes.extend_from_slice(b"\n%%EOF\n");
        fs::write(source.join(format!("{name}.pdf")), bytes).expect("PDF fixture");
        let imported = self
            .host
            .import_tender_package(ImportTenderPackageCommand {
                tender_id: self.tender_id.clone(),
                source_path: source.to_string_lossy().into_owned(),
            })
            .expect("import PDF");
        let document = imported.documents.first().expect("registered PDF");
        self.host
            .parse_source_artifact(ParseSourceArtifactCommand {
                tender_id: self.tender_id.clone(),
                artifact_id: document.artifact_id.clone(),
                version: document.version,
            })
            .await
            .expect("parse PDF");
        let references = self
            .host
            .inspect_evidence(ParseSourceArtifactCommand {
                tender_id: self.tender_id.clone(),
                artifact_id: document.artifact_id.clone(),
                version: document.version,
            })
            .expect("inspect parsed evidence")
            .locations
            .into_iter()
            .map(|location| TenderEvidenceReference {
                artifact_id: document.artifact_id.clone(),
                version: document.version,
                ordinal: location.ordinal,
            })
            .collect();
        ParsedEvidence {
            artifact_id: document.artifact_id.clone(),
            version: document.version,
            references,
        }
    }
}

fn inspect_all_records(host: &QuantixHost, tender_id: &str) -> Vec<TenderRecordInspection> {
    let mut cursor = None;
    let mut records = Vec::new();
    loop {
        let page = host
            .inspect_tender_record_page(tender_id, cursor.as_deref(), 4)
            .expect("inspect bounded Tender Record page");
        records.extend(page.records);
        let Some(next_cursor) = page.next_cursor else {
            return records;
        };
        cursor = Some(next_cursor);
    }
}

fn records_for_run(
    host: &QuantixHost,
    tender_id: &str,
    run_id: &str,
) -> Vec<TenderRecordInspection> {
    inspect_all_records(host, tender_id)
        .into_iter()
        .filter(|record| record.author_run_id == run_id)
        .collect()
}

#[test]
fn a_new_tender_activates_only_the_restricted_bootstrap_team() {
    let root = tempfile::tempdir().expect("temporary Tender Records harness");
    let application_home = root.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Evidence-backed Tender".into(),
        })
        .expect("create Tender");

    let team = host
        .inspect_bootstrap_team(&tender.tender_id)
        .expect("inspect Bootstrap Team");

    assert_eq!(
        team.iter().map(|member| member.role).collect::<Vec<_>>(),
        vec![
            BootstrapRole::TenderOfficeCoordinator,
            BootstrapRole::DocumentController,
            BootstrapRole::TenderAnalyst,
            BootstrapRole::IndependentReviewer,
        ]
    );
    assert!(team.iter().all(|member| member.active));
    assert!(team.iter().all(|member| {
        member.authority == BootstrapAuthority::PreBidAnalysis
            && !member.profile.permissions.network_allowed
    }));
}

#[tokio::test]
async fn agent_record_proposals_preserve_exact_original_evidence_and_explicit_gaps() {
    let harness = RuntimeHarness::new("record-extraction");
    let evidence = harness
        .parsed_pdf_evidence("conditions", b"TENDER_RECORD_GOLDEN")
        .await;

    let extraction = harness
        .host
        .run_tender_record_extraction(RunTenderRecordExtractionCommand {
            tender_id: harness.tender_id.clone(),
            evidence: evidence.references,
            authorities: Vec::new(),
        })
        .await
        .expect("run bounded Tender Record extraction");

    assert_eq!(
        extraction.run.state,
        AgentRunState::Completed,
        "{:#?}\nfixture error: {}",
        extraction.run,
        fs::read_to_string(harness.codex.with_extension("fixture-error"))
            .unwrap_or_else(|_| "none".into())
    );
    assert_eq!(
        extraction.run.profile.identity, "Tender Analyst",
        "record extraction must use the restricted Bootstrap Team role"
    );
    let records = records_for_run(&harness.host, &harness.tender_id, &extraction.run.run_id);
    assert_eq!(extraction.published_record_count as usize, records.len());
    assert!(
        records.iter().any(|record| {
            record.kind == TenderRecordKind::Requirement
                && record.verification_status == VerificationStatus::Proposed
                && record.trust_class == TenderRecordTrustClass::AiProposal
                && record.fields.iter().any(|field| {
                    field.evidence.iter().any(|evidence| {
                        !evidence.location.original_text.is_empty()
                            && evidence
                                .location
                                .translated_text
                                .as_deref()
                                .is_some_and(|translation| !translation.is_empty())
                    })
                })
        }),
        "{:#?}",
        records
    );
    assert!(records.iter().any(|record| {
        record.kind == TenderRecordKind::Assumption
            && record.trust_class == TenderRecordTrustClass::UnresolvedGap
            && record.fields.iter().all(|field| field.evidence.is_empty())
    }));
    assert!(records.iter().any(|record| {
        record.stable_key == "authoritative_notice"
            && record.trust_class == TenderRecordTrustClass::AiProposal
            && record.fields.iter().all(|field| {
                field.value.as_deref().is_some_and(|value| {
                    field
                        .evidence
                        .iter()
                        .any(|evidence| evidence.location.original_text == value)
                })
            })
    }));
    let deadline = records
        .iter()
        .find(|record| record.kind == TenderRecordKind::Deadline)
        .expect("deadline record");
    assert!(deadline.fields.iter().any(|field| {
        field.original_expression.as_deref() == Some("15 May 2026 at 14:00 Cairo time")
            && field.timezone.as_deref() == Some("Africa/Cairo")
            && field.normalized_value.as_deref() == Some("2026-05-15T14:00:00+03:00")
            && field.uncertainty.is_some()
    }));
    assert!(deadline
        .contradictions
        .iter()
        .any(|contradiction| contradiction.evidence.len() >= 2));
}

#[tokio::test]
async fn exact_attributable_engineer_entries_can_support_material_fields() {
    let harness = RuntimeHarness::new("record-extraction");
    let evidence = harness
        .parsed_pdf_evidence("engineer-entry", b"TENDER_RECORD_GOLDEN")
        .await;
    let authority = harness
        .host
        .create_tender_engineer_entry(CreateTenderEngineerEntryCommand {
            tender_id: harness.tender_id.clone(),
            value: "C40/50".into(),
            description: "Engineer-selected concrete strength class for the pre-bid basis.".into(),
        })
        .expect("create immutable Engineer entry");

    let extraction = harness
        .host
        .run_tender_record_extraction(RunTenderRecordExtractionCommand {
            tender_id: harness.tender_id.clone(),
            evidence: evidence.references,
            authorities: vec![TenderRecordAuthorityReference {
                authority_id: authority.authority_id.clone(),
            }],
        })
        .await
        .expect("extract with exact Engineer authority");
    let records = records_for_run(&harness.host, &harness.tender_id, &extraction.run.run_id);
    let field = records
        .iter()
        .flat_map(|record| &record.fields)
        .find(|field| field.basis_kind == TenderRecordBasisKind::EngineerEntry)
        .expect("Engineer-backed material field");

    assert_eq!(field.value.as_deref(), Some("C40/50"));
    assert_eq!(
        field.basis_reference.as_deref(),
        Some(authority.authority_id.as_str())
    );
    assert_eq!(field.basis_authority.as_ref(), Some(&authority));
}

#[tokio::test]
async fn record_history_is_exposed_only_through_stable_bounded_pages() {
    let harness = RuntimeHarness::new("record-extraction");
    let evidence = harness
        .parsed_pdf_evidence("paged-records", b"TENDER_RECORD_GOLDEN")
        .await;
    harness
        .host
        .run_tender_record_extraction(RunTenderRecordExtractionCommand {
            tender_id: harness.tender_id.clone(),
            evidence: evidence.references,
            authorities: Vec::new(),
        })
        .await
        .expect("extract paged records");

    let first = harness
        .host
        .inspect_tender_record_page(&harness.tender_id, None, 2)
        .expect("first bounded page");
    assert_eq!(first.records.len(), 2);
    let cursor = first.next_cursor.expect("next page cursor");
    let second = harness
        .host
        .inspect_tender_record_page(&harness.tender_id, Some(&cursor), 2)
        .expect("second bounded page");
    assert_eq!(second.records.len(), 2);
    assert!(first.records.iter().all(|left| {
        second.records.iter().all(|right| {
            (left.record_id.as_str(), left.version) != (right.record_id.as_str(), right.version)
        })
    }));
    assert!(harness
        .host
        .inspect_tender_record_page(&harness.tender_id, None, 5)
        .is_err());
}

#[test]
fn local_engineer_entries_do_not_require_codex_runtime_readiness() {
    let root = tempfile::tempdir().expect("temporary local Engineer entry harness");
    let application_home = root.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Local authority Tender".into(),
        })
        .expect("create Tender without runtime readiness");

    let entry = host
        .create_tender_engineer_entry(CreateTenderEngineerEntryCommand {
            tender_id: tender.tender_id,
            value: "Engineer exact value".into(),
            description: "Attributable local engineering basis.".into(),
        })
        .expect("create local Engineer entry without Codex");

    assert_eq!(entry.created_by, "engineer_user");
}

#[tokio::test]
async fn unsupported_critical_claims_fail_provenance_validation_without_publication() {
    let harness = RuntimeHarness::new("record-extraction-invalid");
    let evidence = harness
        .parsed_pdf_evidence("unsupported", b"TENDER_RECORD_GOLDEN")
        .await;

    let extraction = harness
        .host
        .run_tender_record_extraction(RunTenderRecordExtractionCommand {
            tender_id: harness.tender_id.clone(),
            evidence: evidence.references,
            authorities: Vec::new(),
        })
        .await
        .expect("record invalid provider output as a failed Agent Run");

    assert_eq!(extraction.run.state, AgentRunState::Failed);
    assert_eq!(
        extraction
            .run
            .failure
            .as_ref()
            .map(|failure| failure.category),
        Some(ProviderFailureCategory::OutputInvalid)
    );
    assert_eq!(extraction.published_record_count, 0);
    assert!(inspect_all_records(&harness.host, &harness.tender_id).is_empty());
}

#[tokio::test]
async fn duplicate_citations_cannot_manufacture_a_contradiction() {
    let harness = RuntimeHarness::new("record-extraction-duplicate-citation");
    let evidence = harness
        .parsed_pdf_evidence("duplicate-citation", b"TENDER_RECORD_GOLDEN")
        .await;

    let extraction = harness
        .host
        .run_tender_record_extraction(RunTenderRecordExtractionCommand {
            tender_id: harness.tender_id.clone(),
            evidence: evidence.references,
            authorities: Vec::new(),
        })
        .await
        .expect("record duplicate-citation output as a failed Agent Run");

    assert_eq!(extraction.run.state, AgentRunState::Failed);
    assert_eq!(
        extraction.run.failure.map(|failure| failure.category),
        Some(ProviderFailureCategory::OutputInvalid)
    );
    assert!(inspect_all_records(&harness.host, &harness.tender_id).is_empty());
}

#[tokio::test]
async fn extraction_output_cannot_publish_after_the_tender_revision_changes() {
    let harness = RuntimeHarness::new("record-extraction-delayed");
    let evidence = harness
        .parsed_pdf_evidence("stale-run", b"TENDER_RECORD_GOLDEN")
        .await;
    let host = harness.host.clone();
    let tender_id = harness.tender_id.clone();
    let extraction_task = tokio::spawn(async move {
        host.run_tender_record_extraction(RunTenderRecordExtractionCommand {
            tender_id,
            evidence: evidence.references,
            authorities: Vec::new(),
        })
        .await
    });
    let waiting = harness.codex.with_extension("record-output-waiting");
    for _ in 0..1_000 {
        if waiting.is_file() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    assert!(
        waiting.is_file(),
        "provider did not reach delayed output boundary"
    );
    harness
        .host
        .revise_tender(ReviseTenderCommand {
            tender_id: harness.tender_id.clone(),
            name: "Tender revised while extraction was running".into(),
        })
        .expect("revise Tender while provider turn is in flight");
    fs::write(
        harness.codex.with_extension("record-output-release"),
        b"release",
    )
    .expect("release delayed provider output");

    let extraction = extraction_task
        .await
        .expect("join extraction task")
        .expect("record stale output as terminal failed run");
    assert_eq!(extraction.run.state, AgentRunState::Failed);
    assert_eq!(
        extraction.run.failure.map(|failure| failure.category),
        Some(ProviderFailureCategory::OutputInvalid)
    );
    assert_eq!(extraction.published_record_count, 0);
    assert!(inspect_all_records(&harness.host, &harness.tender_id).is_empty());
}

#[tokio::test]
async fn record_runs_require_fresh_exact_workflow_reruns_not_bootstrap_linked_retries() {
    let harness = RuntimeHarness::new("record-extraction-malformed-after-turn");
    let evidence = harness
        .parsed_pdf_evidence("record-retry-boundary", b"TENDER_RECORD_GOLDEN")
        .await;
    let extraction = harness
        .host
        .run_tender_record_extraction(RunTenderRecordExtractionCommand {
            tender_id: harness.tender_id.clone(),
            evidence: evidence.references,
            authorities: Vec::new(),
        })
        .await
        .expect("persist indeterminate record extraction");

    assert_eq!(extraction.run.state, AgentRunState::Indeterminate);
    assert!(!extraction.run.linked_retry_supported);
    assert!(harness
        .host
        .resolve_indeterminate_agent_run(ResolveIndeterminateAgentRunCommand {
            tender_id: harness.tender_id.clone(),
            run_id: extraction.run.run_id.clone(),
            disposition: AgentRunRecoveryDisposition::RetryTask,
            rationale: "Attempt a generic retry.".into(),
        })
        .is_err());
    let closed = harness
        .host
        .resolve_indeterminate_agent_run(ResolveIndeterminateAgentRunCommand {
            tender_id: harness.tender_id.clone(),
            run_id: extraction.run.run_id,
            disposition: AgentRunRecoveryDisposition::CloseTask,
            rationale: "Close and rerun from an exact evidence selection.".into(),
        })
        .expect("close unsupported linked retry path");
    assert_eq!(closed.disposition, AgentRunRecoveryDisposition::CloseTask);
}

#[tokio::test]
async fn confirmed_addendum_precedence_remains_attached_to_conflicting_records() {
    let harness = RuntimeHarness::new("record-extraction");
    let prior = harness
        .parsed_pdf_evidence("original", b"TENDER_RECORD_GOLDEN")
        .await;
    let addendum = harness
        .parsed_pdf_evidence("addendum", b"TENDER_RECORD_GOLDEN")
        .await;
    harness
        .host
        .confirm_source_relationship(ConfirmSourceRelationshipCommand {
            tender_id: harness.tender_id.clone(),
            prior_artifact_id: prior.artifact_id.clone(),
            prior_version: prior.version,
            replacement_artifact_id: addendum.artifact_id.clone(),
            replacement_version: addendum.version,
            relationship_kind: SourceRelationshipKind::Addendum,
        })
        .expect("confirm addendum relationship");
    let mut evidence = prior.references;
    evidence.extend(addendum.references);

    let extraction = harness
        .host
        .run_tender_record_extraction(RunTenderRecordExtractionCommand {
            tender_id: harness.tender_id.clone(),
            evidence,
            authorities: Vec::new(),
        })
        .await
        .expect("extract related-source records");
    let records = records_for_run(&harness.host, &harness.tender_id, &extraction.run.run_id);
    let deadline = records
        .iter()
        .find(|record| record.kind == TenderRecordKind::Deadline)
        .expect("deadline record");

    assert!(deadline.source_relationships.iter().any(|relationship| {
        relationship.relationship_kind == SourceRelationshipKind::Addendum
            && relationship.prior_artifact_id == prior.artifact_id
            && relationship.replacement_artifact_id == addendum.artifact_id
    }));
    assert!(deadline
        .contradictions
        .iter()
        .any(|contradiction| contradiction.evidence.len() >= 2));
    assert_eq!(deadline.verification_status, VerificationStatus::Proposed);
}

#[tokio::test]
async fn later_addenda_make_affected_verified_records_visibly_stale() {
    let harness = RuntimeHarness::new("record-extraction");
    let prior = harness
        .parsed_pdf_evidence("stale-original", b"TENDER_RECORD_GOLDEN")
        .await;
    let extraction = harness
        .host
        .run_tender_record_extraction(RunTenderRecordExtractionCommand {
            tender_id: harness.tender_id.clone(),
            evidence: prior.references,
            authorities: Vec::new(),
        })
        .await
        .expect("extract record before addendum");
    let records = records_for_run(&harness.host, &harness.tender_id, &extraction.run.run_id);
    let requirement = records
        .iter()
        .find(|record| record.kind == TenderRecordKind::Requirement)
        .expect("affected requirement");
    harness
        .host
        .decide_tender_record(DecideTenderRecordCommand {
            tender_id: harness.tender_id.clone(),
            record_id: requirement.record_id.clone(),
            version: requirement.version,
            decision: TenderRecordEngineerDecisionKind::Verify,
            rationale: "Verified against the then-current authoritative package.".into(),
        })
        .expect("verify exact pre-addendum record");
    let addendum = harness
        .parsed_pdf_evidence("stale-addendum", b"TENDER_RECORD_GOLDEN")
        .await;
    harness
        .host
        .confirm_source_relationship(ConfirmSourceRelationshipCommand {
            tender_id: harness.tender_id.clone(),
            prior_artifact_id: prior.artifact_id,
            prior_version: prior.version,
            replacement_artifact_id: addendum.artifact_id,
            replacement_version: addendum.version,
            relationship_kind: SourceRelationshipKind::Addendum,
        })
        .expect("confirm later addendum");

    let stale = inspect_all_records(&harness.host, &harness.tender_id)
        .into_iter()
        .find(|record| record.record_id == requirement.record_id)
        .expect("affected record after addendum");
    assert_eq!(stale.verification_status, VerificationStatus::Stale);
    assert_eq!(stale.trust_class, TenderRecordTrustClass::PriorDecision);
}

#[tokio::test]
async fn independent_review_is_attributable_version_bound_and_cannot_edit_the_proposal() {
    let harness = RuntimeHarness::new("record-extraction");
    let evidence = harness
        .parsed_pdf_evidence("review", b"TENDER_RECORD_GOLDEN")
        .await;
    let evidence_references = evidence.references.clone();
    let extraction = harness
        .host
        .run_tender_record_extraction(RunTenderRecordExtractionCommand {
            tender_id: harness.tender_id.clone(),
            evidence: evidence.references,
            authorities: Vec::new(),
        })
        .await
        .expect("extract review target");
    let records = records_for_run(&harness.host, &harness.tender_id, &extraction.run.run_id);
    let proposed = records
        .iter()
        .find(|record| record.kind == TenderRecordKind::Requirement)
        .expect("evidence-backed requirement")
        .clone();
    harness.set_agent_scenario("record-review");

    let reviewed = harness
        .host
        .run_tender_record_review(RunTenderRecordReviewCommand {
            tender_id: harness.tender_id.clone(),
            record_id: proposed.record_id.clone(),
            version: proposed.version,
        })
        .await
        .expect("independent exact-version review");

    assert_eq!(reviewed.run.state, AgentRunState::Completed);
    assert_eq!(reviewed.run.profile.identity, "Independent Reviewer");
    assert_ne!(
        reviewed.record.author_profile_id,
        reviewed.run.profile.profile_id
    );
    assert_eq!(reviewed.record.title, proposed.title);
    assert_eq!(reviewed.record.fields, proposed.fields);
    assert_eq!(
        reviewed.record.verification_status,
        VerificationStatus::Verified
    );
    assert_eq!(
        reviewed.record.trust_class,
        TenderRecordTrustClass::Verified
    );
    assert!(reviewed.record.reviews.iter().any(|review| {
        review.outcome == TenderRecordReviewOutcome::Verified
            && review.reviewer_run_id.as_deref() == Some(reviewed.run.run_id.as_str())
    }));
    assert!(harness
        .host
        .run_tender_record_review(RunTenderRecordReviewCommand {
            tender_id: harness.tender_id.clone(),
            record_id: proposed.record_id.clone(),
            version: proposed.version,
        })
        .await
        .is_err());

    harness.set_agent_scenario("record-extraction");
    harness
        .host
        .run_tender_record_extraction(RunTenderRecordExtractionCommand {
            tender_id: harness.tender_id.clone(),
            evidence: evidence_references,
            authorities: Vec::new(),
        })
        .await
        .expect("propose a new exact version");
    let history = inspect_all_records(&harness.host, &harness.tender_id);
    let current = history
        .iter()
        .filter(|record| record.record_id == proposed.record_id)
        .max_by_key(|record| record.version)
        .expect("current requirement version");
    assert_eq!(current.version, proposed.version + 1);
    assert_eq!(current.verification_status, VerificationStatus::Proposed);
    assert!(current.reviews.is_empty());
    assert!(history.iter().any(|record| {
        record.record_id == proposed.record_id
            && record.version == proposed.version
            && record.verification_status == VerificationStatus::Superseded
            && record.trust_class == TenderRecordTrustClass::PriorDecision
            && !record.reviews.is_empty()
    }));
    assert!(history.iter().any(|record| {
        record.kind == TenderRecordKind::Assumption
            && record.version == 1
            && record.verification_status == VerificationStatus::Superseded
            && record.trust_class == TenderRecordTrustClass::UnresolvedGap
            && record.reviews.is_empty()
    }));
}

#[tokio::test]
async fn engineer_decision_during_review_terminalizes_the_stale_review_run() {
    let harness = RuntimeHarness::new("record-extraction");
    let evidence = harness
        .parsed_pdf_evidence("review-race", b"TENDER_RECORD_GOLDEN")
        .await;
    let extraction = harness
        .host
        .run_tender_record_extraction(RunTenderRecordExtractionCommand {
            tender_id: harness.tender_id.clone(),
            evidence: evidence.references,
            authorities: Vec::new(),
        })
        .await
        .expect("extract review race target");
    let proposed = records_for_run(&harness.host, &harness.tender_id, &extraction.run.run_id)
        .into_iter()
        .find(|record| record.kind == TenderRecordKind::Requirement)
        .expect("review race requirement");
    harness.set_agent_scenario("record-review-delayed");
    let host = harness.host.clone();
    let tender_id = harness.tender_id.clone();
    let record_id = proposed.record_id.clone();
    let version = proposed.version;
    let review_task = tokio::spawn(async move {
        host.run_tender_record_review(RunTenderRecordReviewCommand {
            tender_id,
            record_id,
            version,
        })
        .await
    });
    let waiting = harness.codex.with_extension("record-review-waiting");
    for _ in 0..1_000 {
        if waiting.is_file() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    assert!(
        waiting.is_file(),
        "provider did not reach delayed review boundary"
    );
    harness
        .host
        .decide_tender_record(DecideTenderRecordCommand {
            tender_id: harness.tender_id.clone(),
            record_id: proposed.record_id.clone(),
            version: proposed.version,
            decision: TenderRecordEngineerDecisionKind::Verify,
            rationale: "Engineer decision landed before the independent review output.".into(),
        })
        .expect("record exact Engineer decision during provider review");
    fs::write(
        harness.codex.with_extension("record-review-release"),
        b"release",
    )
    .expect("release delayed review output");

    let reviewed = review_task
        .await
        .expect("join delayed review")
        .expect("terminalize stale review output");
    assert_eq!(reviewed.run.state, AgentRunState::Failed);
    assert_eq!(
        reviewed.run.failure.map(|failure| failure.category),
        Some(ProviderFailureCategory::OutputInvalid)
    );
    assert_eq!(
        reviewed.record.trust_class,
        TenderRecordTrustClass::EngineerVerified
    );
    assert_eq!(reviewed.record.reviews.len(), 1);
    assert_eq!(reviewed.record.reviews[0].reviewer_kind, "engineer_user");
    assert!(harness
        .host
        .inspect_agent_runs(&harness.tender_id)
        .expect("inspect terminal review race run")
        .iter()
        .all(|run| run.state != AgentRunState::Running));
}

#[tokio::test]
async fn missing_provenance_blocks_independent_verification_but_engineer_can_approve_an_assumption()
{
    let harness = RuntimeHarness::new("record-extraction");
    let evidence = harness
        .parsed_pdf_evidence("assumption", b"TENDER_RECORD_GOLDEN")
        .await;
    let extraction = harness
        .host
        .run_tender_record_extraction(RunTenderRecordExtractionCommand {
            tender_id: harness.tender_id.clone(),
            evidence: evidence.references,
            authorities: Vec::new(),
        })
        .await
        .expect("extract explicit gap");
    let records = records_for_run(&harness.host, &harness.tender_id, &extraction.run.run_id);
    let assumption = records
        .iter()
        .find(|record| record.kind == TenderRecordKind::Assumption)
        .expect("explicit assumption")
        .clone();
    harness.set_agent_scenario("record-review");

    let blocked = harness
        .host
        .run_tender_record_review(RunTenderRecordReviewCommand {
            tender_id: harness.tender_id.clone(),
            record_id: assumption.record_id.clone(),
            version: assumption.version,
        })
        .await
        .expect("record invalid verification as failed review run");
    assert_eq!(blocked.run.state, AgentRunState::Failed);
    assert_eq!(
        blocked.record.verification_status,
        VerificationStatus::Proposed
    );
    assert!(blocked.record.reviews.is_empty());
    assert!(harness
        .host
        .decide_tender_record(DecideTenderRecordCommand {
            tender_id: harness.tender_id.clone(),
            record_id: assumption.record_id.clone(),
            version: assumption.version,
            decision: TenderRecordEngineerDecisionKind::Verify,
            rationale: "Attempt to verify a record without exact provenance.".into(),
        })
        .is_err());

    let approved =
        harness
            .host
            .decide_tender_record(DecideTenderRecordCommand {
                tender_id: harness.tender_id.clone(),
                record_id: assumption.record_id,
                version: assumption.version,
                decision: TenderRecordEngineerDecisionKind::ApproveAssumption,
                rationale:
                    "Approved as the controlled pre-bid basis pending the Tender Query response."
                        .into(),
            })
            .expect("Engineer approval of explicit assumption");
    assert_eq!(
        approved.record.verification_status,
        VerificationStatus::Verified
    );
    assert_eq!(
        approved.record.trust_class,
        TenderRecordTrustClass::ApprovedAssumption
    );
    assert_eq!(approved.review.decided_by, "engineer_user");
    assert!(harness
        .host
        .run_tender_record_review(RunTenderRecordReviewCommand {
            tender_id: harness.tender_id.clone(),
            record_id: approved.record.record_id,
            version: approved.record.version,
        })
        .await
        .is_err());
}

#[tokio::test]
async fn exact_record_and_review_ledgers_survive_cold_open_integrity_validation() {
    let harness = RuntimeHarness::new("record-extraction");
    let evidence = harness
        .parsed_pdf_evidence("cold-open", b"TENDER_RECORD_GOLDEN")
        .await;
    let extraction = harness
        .host
        .run_tender_record_extraction(RunTenderRecordExtractionCommand {
            tender_id: harness.tender_id.clone(),
            evidence: evidence.references,
            authorities: Vec::new(),
        })
        .await
        .expect("extract cold-open target");
    let records = records_for_run(&harness.host, &harness.tender_id, &extraction.run.run_id);
    let requirement = records
        .iter()
        .find(|record| record.kind == TenderRecordKind::Requirement)
        .expect("requirement target");
    harness.set_agent_scenario("record-review");
    harness
        .host
        .run_tender_record_review(RunTenderRecordReviewCommand {
            tender_id: harness.tender_id.clone(),
            record_id: requirement.record_id.clone(),
            version: requirement.version,
        })
        .await
        .expect("review cold-open target");

    let cold_host =
        QuantixHost::with_setup_platform(&harness.application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&cold_host).state, SetupState::Ready);
    assert_eq!(
        cold_host
            .inspect_tender_integrity(&harness.tender_id)
            .expect("cold-open integrity")
            .state,
        TenderIntegrityState::Ready
    );
    let reopened = inspect_all_records(&cold_host, &harness.tender_id);
    assert!(reopened.iter().any(|record| {
        record.record_id == requirement.record_id
            && record.verification_status == VerificationStatus::Verified
    }));
}

fn install_codex_fixture(resources: &Path, scenario: &str) -> std::path::PathBuf {
    let runtime_bin = resources.join("runtime").join("bin");
    fs::create_dir_all(&runtime_bin).expect("fake runtime bin");
    let codex = runtime_bin.join(executable_name("codex"));
    fs::copy(
        Path::new(env!("CARGO_BIN_EXE_quantix-runtime-fixture")),
        &codex,
    )
    .expect("copy fake app-server");
    fs::write(codex.with_extension("agent-scenario"), scenario)
        .expect("write fake app-server scenario");
    codex
}

fn install_docling_fixture(application_home: &Path) {
    let executable = application_home
        .join("runtimes")
        .join("docling")
        .join(if cfg!(windows) { "Scripts" } else { "bin" })
        .join(executable_name("docling"));
    fs::create_dir_all(executable.parent().expect("Docling executable parent"))
        .expect("Docling executable directory");
    fs::copy(
        Path::new(env!("CARGO_BIN_EXE_quantix-runtime-fixture")),
        &executable,
    )
    .expect("install Docling fixture");
    let models = application_home.join("models").join("docling");
    for profile in [
        "layout",
        "tableformer",
        "code_formula",
        "picture_classifier",
        "rapidocr",
    ] {
        let model = models.join(profile).join("model.bin");
        fs::create_dir_all(model.parent().expect("model parent")).expect("model directory");
        fs::write(model, format!("{profile} fixture model")).expect("model fixture");
    }
}

fn executable_name(name: &str) -> String {
    if std::env::consts::EXE_EXTENSION.is_empty() {
        name.to_owned()
    } else {
        format!("{name}.{}", std::env::consts::EXE_EXTENSION)
    }
}
