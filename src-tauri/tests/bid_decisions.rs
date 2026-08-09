use std::{fs, io, path::Path, sync::Arc};

use quantix_lib::{
    ensure_quantix_setup, AgentRunState, BidDecisionPackageReviewOutcome, BidRecommendationOutcome,
    ComplianceDisposition, ComplianceDispositionUpdate, CreateBidDecisionPackageCommand,
    CreateTenderCommand, DecideTenderRecordCommand, DeviceProtection, ImportTenderPackageCommand,
    ManagerCapabilityDemandInput, ParseSourceArtifactCommand, ProviderFailureCategory, QuantixHost,
    RunBidDecisionPackageReviewCommand, RunTenderRecordExtractionCommand, RuntimeLayout,
    SetupPlatform, SetupState, StoragePermissions, TenderErrorCode, TenderEvidenceReference,
    TenderIntegrityState, TenderRecordEngineerDecisionKind, TenderRecordInspection,
    TenderRecordKind, TenderRecordVersionReference, MINIMUM_SETUP_FREE_SPACE_BYTES,
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

struct Harness {
    _root: tempfile::TempDir,
    application_home: std::path::PathBuf,
    codex: std::path::PathBuf,
    host: QuantixHost,
    tender_id: String,
}

impl Harness {
    fn new(agent_scenario: &str) -> Self {
        let root = tempfile::tempdir().expect("temporary Bid Decision harness");
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
                name: "Controlled Bid Decision Tender".into(),
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

    async fn extract_records(&self) -> Vec<TenderRecordInspection> {
        let source = self._root.path().join("decision-source");
        fs::create_dir(&source).expect("source directory");
        fs::write(
            source.join("conditions.pdf"),
            b"%PDF-1.7\nTENDER_RECORD_GOLDEN\n%%EOF\n",
        )
        .expect("PDF fixture");
        let imported = self
            .host
            .import_tender_package(ImportTenderPackageCommand {
                tender_id: self.tender_id.clone(),
                source_path: source.to_string_lossy().into_owned(),
            })
            .expect("import package");
        let document = imported.documents.first().expect("registered source");
        self.host
            .parse_source_artifact(ParseSourceArtifactCommand {
                tender_id: self.tender_id.clone(),
                artifact_id: document.artifact_id.clone(),
                version: document.version,
            })
            .await
            .expect("parse source");
        let evidence = self
            .host
            .inspect_evidence(ParseSourceArtifactCommand {
                tender_id: self.tender_id.clone(),
                artifact_id: document.artifact_id.clone(),
                version: document.version,
            })
            .expect("inspect Evidence")
            .locations
            .into_iter()
            .map(|location| TenderEvidenceReference {
                artifact_id: document.artifact_id.clone(),
                version: document.version,
                ordinal: location.ordinal,
            })
            .collect();
        let extraction = self
            .host
            .run_tender_record_extraction(RunTenderRecordExtractionCommand {
                tender_id: self.tender_id.clone(),
                evidence,
                authorities: Vec::new(),
            })
            .await
            .expect("extract Tender Records");
        assert_eq!(extraction.run.state, AgentRunState::Completed);
        inspect_all_records(&self.host, &self.tender_id)
    }

    fn verify_records(&self, records: &[TenderRecordInspection], skip_deadline: bool) {
        for record in records.iter().filter(|record| {
            record.version == 1 && (!skip_deadline || record.kind != TenderRecordKind::Deadline)
        }) {
            let decision = if record.kind == TenderRecordKind::Assumption {
                TenderRecordEngineerDecisionKind::ApproveAssumption
            } else {
                TenderRecordEngineerDecisionKind::Verify
            };
            self.host
                .decide_tender_record(DecideTenderRecordCommand {
                    tender_id: self.tender_id.clone(),
                    record_id: record.record_id.clone(),
                    version: record.version,
                    decision,
                    rationale: "Exact pre-bid basis verified for the Bid Decision Package.".into(),
                })
                .expect("verify exact Tender Record");
        }
    }
}

fn inspect_all_records(host: &QuantixHost, tender_id: &str) -> Vec<TenderRecordInspection> {
    let mut cursor = None;
    let mut records = Vec::new();
    loop {
        let page = host
            .inspect_tender_record_page(tender_id, cursor.as_deref(), 4)
            .expect("inspect Tender Record page");
        records.extend(page.records);
        let Some(next) = page.next_cursor else {
            return records;
        };
        cursor = Some(next);
    }
}

fn is_compliance_kind(kind: TenderRecordKind) -> bool {
    matches!(
        kind,
        TenderRecordKind::Requirement
            | TenderRecordKind::EvaluationCriterion
            | TenderRecordKind::Deliverable
            | TenderRecordKind::Deadline
            | TenderRecordKind::Form
            | TenderRecordKind::Clause
    )
}

fn complete_dispositions(records: &[TenderRecordInspection]) -> Vec<ComplianceDispositionUpdate> {
    records
        .iter()
        .filter(|record| record.version == 1 && is_compliance_kind(record.kind))
        .map(|record| ComplianceDispositionUpdate {
            record: TenderRecordVersionReference {
                record_id: record.record_id.clone(),
                version: record.version,
            },
            disposition: ComplianceDisposition::Comply,
            responsibility: "Tender Office Coordinator".into(),
            planned_treatment:
                "Carry this exact verified obligation into the controlled Work Plan.".into(),
            affected_work: vec!["tender_planning".into()],
            uncertainty: record
                .fields
                .iter()
                .find_map(|field| field.uncertainty.clone()),
            related_records: Vec::new(),
        })
        .collect()
}

#[tokio::test]
async fn complete_exact_package_passes_review_and_is_ready_for_the_formal_gate() {
    let harness = Harness::new("record-extraction");
    let records = harness.extract_records().await;
    harness.verify_records(&records, false);
    let package = harness
        .host
        .create_bid_decision_package(CreateBidDecisionPackageCommand {
            tender_id: harness.tender_id.clone(),
            base_version: None,
            disposition_updates: complete_dispositions(&records),
            manager_capability_demands: Vec::new(),
        })
        .expect("create complete Bid Decision Package");

    assert_eq!(package.version, 1);
    assert_eq!(package.compliance_blocker_count, 0);
    assert_eq!(package.project_fingerprint_count, 1);
    assert_eq!(package.risk_count, 1);
    assert_eq!(package.capability_gap_count, 0);
    assert_eq!(
        package.resource_implications.len() as u32,
        package.compliance_row_count
    );
    assert_eq!(
        package.recommendation.outcome,
        BidRecommendationOutcome::Proceed
    );
    assert!(
        !package.decision_gate_ready,
        "Independent Review is mandatory"
    );
    let matrix = harness
        .host
        .inspect_compliance_matrix_page(&harness.tender_id, &package.package_id, 1, None, 4)
        .expect("inspect exact matrix page");
    assert_eq!(matrix.rows.len() as u32, package.compliance_row_count);
    assert!(matrix.rows.iter().all(|row| {
        row.disposition == ComplianceDisposition::Comply
            && !row
                .record
                .fields
                .iter()
                .all(|field| field.evidence.is_empty())
    }));

    harness.set_agent_scenario("bid-package-review");
    let reviewed = harness
        .host
        .run_bid_decision_package_review(RunBidDecisionPackageReviewCommand {
            tender_id: harness.tender_id.clone(),
            package_id: package.package_id.clone(),
            version: package.version,
        })
        .await
        .expect("review exact Bid Decision Package");
    assert_eq!(
        reviewed.run.state,
        AgentRunState::Completed,
        "{:#?}\nfixture error: {}",
        reviewed.run,
        fs::read_to_string(harness.codex.with_extension("fixture-error"))
            .unwrap_or_else(|_| "none".into())
    );
    assert!(!reviewed.run.linked_retry_supported);
    assert_eq!(
        reviewed
            .package
            .review
            .as_ref()
            .map(|review| review.outcome),
        Some(BidDecisionPackageReviewOutcome::Passed)
    );
    assert!(reviewed.package.decision_gate_ready);

    let cold_host =
        QuantixHost::with_setup_platform(&harness.application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&cold_host).state, SetupState::Ready);
    assert_eq!(
        cold_host
            .inspect_tender_integrity(&harness.tender_id)
            .expect("cold-open package integrity")
            .state,
        TenderIntegrityState::Ready
    );
    let reopened = cold_host
        .inspect_current_bid_decision_package(&harness.tender_id)
        .expect("inspect reopened package")
        .expect("current package");
    assert_eq!(reopened.manifest_sha256, package.manifest_sha256);
    assert!(reopened.decision_gate_ready);
}

#[tokio::test]
async fn incomplete_dispositions_and_missing_verification_block_the_gate() {
    let harness = Harness::new("record-extraction");
    harness.extract_records().await;
    let package = harness
        .host
        .create_bid_decision_package(CreateBidDecisionPackageCommand {
            tender_id: harness.tender_id.clone(),
            base_version: None,
            disposition_updates: Vec::new(),
            manager_capability_demands: Vec::new(),
        })
        .expect("create visibly incomplete package");
    assert!(package.compliance_blocker_count > 0);
    assert!(package
        .blockers
        .iter()
        .any(|blocker| blocker.code == "unresolved_disposition"));

    harness.set_agent_scenario("bid-package-review");
    let reviewed = harness
        .host
        .run_bid_decision_package_review(RunBidDecisionPackageReviewCommand {
            tender_id: harness.tender_id.clone(),
            package_id: package.package_id.clone(),
            version: package.version,
        })
        .await
        .expect("persist review without bypassing deterministic blockers");
    assert!(!reviewed.package.decision_gate_ready);
}

#[tokio::test]
async fn package_creation_deadline_fails_without_publishing_a_partial_version() {
    let harness = Harness::new("record-extraction");
    let records = harness.extract_records().await;
    harness.verify_records(&records, false);
    std::env::set_var("QUANTIX_BID_PACKAGE_OPERATION_TIMEOUT", &harness.tender_id);
    let result = harness
        .host
        .create_bid_decision_package(CreateBidDecisionPackageCommand {
            tender_id: harness.tender_id.clone(),
            base_version: None,
            disposition_updates: complete_dispositions(&records),
            manager_capability_demands: Vec::new(),
        });
    std::env::remove_var("QUANTIX_BID_PACKAGE_OPERATION_TIMEOUT");
    assert_eq!(
        result.expect_err("deadline must fail").code,
        TenderErrorCode::OperationTimedOut
    );
    assert!(harness
        .host
        .inspect_current_bid_decision_package(&harness.tender_id)
        .expect("inspect after timeout")
        .is_none());
}

#[tokio::test]
async fn a_new_current_obligation_stales_the_reviewed_snapshot_and_allows_repackaging() {
    let harness = Harness::new("record-extraction");
    let records = harness.extract_records().await;
    harness.verify_records(&records, false);
    let package = harness
        .host
        .create_bid_decision_package(CreateBidDecisionPackageCommand {
            tender_id: harness.tender_id.clone(),
            base_version: None,
            disposition_updates: complete_dispositions(&records),
            manager_capability_demands: Vec::new(),
        })
        .expect("create exact package snapshot");
    harness.set_agent_scenario("bid-package-review");
    let reviewed = harness
        .host
        .run_bid_decision_package_review(RunBidDecisionPackageReviewCommand {
            tender_id: harness.tender_id.clone(),
            package_id: package.package_id.clone(),
            version: package.version,
        })
        .await
        .expect("review exact package snapshot");
    assert!(reviewed.package.decision_gate_ready);

    let mut evidence = records
        .iter()
        .flat_map(|record| {
            record
                .fields
                .iter()
                .flat_map(|field| field.evidence.iter())
                .chain(
                    record
                        .contradictions
                        .iter()
                        .flat_map(|contradiction| contradiction.evidence.iter()),
                )
        })
        .map(|evidence| evidence.reference.clone())
        .collect::<Vec<_>>();
    evidence.sort_by(|left, right| {
        left.artifact_id
            .cmp(&right.artifact_id)
            .then_with(|| left.version.cmp(&right.version))
            .then_with(|| left.ordinal.cmp(&right.ordinal))
    });
    evidence.dedup_by(|left, right| {
        left.artifact_id == right.artifact_id
            && left.version == right.version
            && left.ordinal == right.ordinal
    });
    harness.set_agent_scenario("record-extraction-expanded");
    harness
        .host
        .run_tender_record_extraction(RunTenderRecordExtractionCommand {
            tender_id: harness.tender_id.clone(),
            evidence,
            authorities: Vec::new(),
        })
        .await
        .expect("publish newly discovered obligation");

    let stale = harness
        .host
        .inspect_current_bid_decision_package(&harness.tender_id)
        .expect("inspect stale package")
        .expect("package exists");
    assert!(!stale.current);
    assert!(!stale.decision_gate_ready);
    assert!(stale
        .blockers
        .iter()
        .any(|blocker| blocker.code == "package_dependencies_stale"));
    let successor = harness
        .host
        .create_bid_decision_package(CreateBidDecisionPackageCommand {
            tender_id: harness.tender_id.clone(),
            base_version: Some(package.version),
            disposition_updates: Vec::new(),
            manager_capability_demands: Vec::new(),
        })
        .expect("create replacement package after material change");
    assert_eq!(successor.version, package.version + 1);
    assert_eq!(
        successor.compliance_row_count,
        package.compliance_row_count + 1
    );
}

#[tokio::test]
async fn same_version_verification_stales_a_fingerprint_that_omitted_the_proposal() {
    let harness = Harness::new("record-extraction");
    let records = harness.extract_records().await;
    harness.verify_records(&records, false);
    let mut evidence = records
        .iter()
        .flat_map(|record| record.fields.iter().flat_map(|field| field.evidence.iter()))
        .map(|evidence| evidence.reference.clone())
        .collect::<Vec<_>>();
    evidence.sort_by(|left, right| {
        left.artifact_id
            .cmp(&right.artifact_id)
            .then_with(|| left.version.cmp(&right.version))
            .then_with(|| left.ordinal.cmp(&right.ordinal))
    });
    evidence.dedup_by(|left, right| {
        left.artifact_id == right.artifact_id
            && left.version == right.version
            && left.ordinal == right.ordinal
    });
    harness.set_agent_scenario("record-extraction-extra-characteristic");
    harness
        .host
        .run_tender_record_extraction(RunTenderRecordExtractionCommand {
            tender_id: harness.tender_id.clone(),
            evidence,
            authorities: Vec::new(),
        })
        .await
        .expect("publish proposed Project Characteristic");
    let current_records = inspect_all_records(&harness.host, &harness.tender_id);
    let added = current_records
        .iter()
        .find(|record| record.stable_key == "late_project_characteristic")
        .expect("new Project Characteristic");
    let package = harness
        .host
        .create_bid_decision_package(CreateBidDecisionPackageCommand {
            tender_id: harness.tender_id.clone(),
            base_version: None,
            disposition_updates: complete_dispositions(&current_records),
            manager_capability_demands: Vec::new(),
        })
        .expect("package omitting proposed characteristic from verified fingerprint");
    assert_eq!(package.project_fingerprint_count, 1);
    harness.set_agent_scenario("bid-package-review");
    let reviewed = harness
        .host
        .run_bid_decision_package_review(RunBidDecisionPackageReviewCommand {
            tender_id: harness.tender_id.clone(),
            package_id: package.package_id.clone(),
            version: package.version,
        })
        .await
        .expect("review exact package");
    assert!(reviewed.package.decision_gate_ready);

    harness
        .host
        .decide_tender_record(DecideTenderRecordCommand {
            tender_id: harness.tender_id.clone(),
            record_id: added.record_id.clone(),
            version: added.version,
            decision: TenderRecordEngineerDecisionKind::Verify,
            rationale: "Verify the newly established exact Project Characteristic.".into(),
        })
        .expect("verify same immutable record version");
    let stale = harness
        .host
        .inspect_current_bid_decision_package(&harness.tender_id)
        .expect("inspect package after trust change")
        .expect("package exists");
    assert!(!stale.current);
    assert!(!stale.decision_gate_ready);
    assert!(stale
        .blockers
        .iter()
        .any(|blocker| blocker.code == "package_dependencies_stale"));
}

#[tokio::test]
async fn integrity_rejects_a_head_repointed_to_an_obsolete_package_version() {
    let harness = Harness::new("record-extraction");
    let records = harness.extract_records().await;
    harness.verify_records(&records, false);
    let first = harness
        .host
        .create_bid_decision_package(CreateBidDecisionPackageCommand {
            tender_id: harness.tender_id.clone(),
            base_version: None,
            disposition_updates: complete_dispositions(&records),
            manager_capability_demands: Vec::new(),
        })
        .expect("create first package version");
    harness
        .host
        .create_bid_decision_package(CreateBidDecisionPackageCommand {
            tender_id: harness.tender_id.clone(),
            base_version: Some(first.version),
            disposition_updates: Vec::new(),
            manager_capability_demands: Vec::new(),
        })
        .expect("create successor package version");
    harness
        .host
        .close_tender(&harness.tender_id)
        .expect("close Tender before corruption injection");
    let database = harness
        .application_home
        .join("tenders")
        .join(&harness.tender_id)
        .join("tender.sqlite");
    rusqlite::Connection::open(database)
        .expect("open Tender Store")
        .execute(
            "UPDATE bid_decision_package_heads SET current_version = 1",
            [],
        )
        .expect("repoint package head");
    assert_eq!(
        harness
            .host
            .inspect_tender_integrity(&harness.tender_id)
            .expect("inspect damaged package head")
            .state,
        TenderIntegrityState::RecoveryRequired
    );
}

#[tokio::test]
async fn integrity_rejects_a_review_attributed_to_the_wrong_completed_run() {
    let harness = Harness::new("record-extraction");
    let records = harness.extract_records().await;
    harness.verify_records(&records, false);
    let extraction_run_id = records[0].author_run_id.clone();
    let package = harness
        .host
        .create_bid_decision_package(CreateBidDecisionPackageCommand {
            tender_id: harness.tender_id.clone(),
            base_version: None,
            disposition_updates: complete_dispositions(&records),
            manager_capability_demands: Vec::new(),
        })
        .expect("create exact package");
    harness.set_agent_scenario("bid-package-review");
    harness
        .host
        .run_bid_decision_package_review(RunBidDecisionPackageReviewCommand {
            tender_id: harness.tender_id.clone(),
            package_id: package.package_id,
            version: package.version,
        })
        .await
        .expect("review exact package");
    harness
        .host
        .close_tender(&harness.tender_id)
        .expect("close Tender before corruption injection");
    let database = harness
        .application_home
        .join("tenders")
        .join(&harness.tender_id)
        .join("tender.sqlite");
    rusqlite::Connection::open(database)
        .expect("open Tender Store")
        .execute_batch(&format!(
            "DROP TRIGGER bid_decision_package_reviews_no_update;
             UPDATE bid_decision_package_reviews
             SET reviewer_run_id = '{extraction_run_id}';
             CREATE TRIGGER bid_decision_package_reviews_no_update
             BEFORE UPDATE ON bid_decision_package_reviews
             BEGIN
               SELECT RAISE(ABORT, 'Bid Decision Package Reviews are immutable');
             END;"
        ))
        .expect("substitute mismatched completed reviewer run");
    assert_eq!(
        harness
            .host
            .inspect_tender_integrity(&harness.tender_id)
            .expect("inspect mismatched review attribution")
            .state,
        TenderIntegrityState::RecoveryRequired
    );
}

#[tokio::test]
async fn contradictory_exact_record_and_failed_review_remain_blocking() {
    let harness = Harness::new("record-extraction");
    let records = harness.extract_records().await;
    harness.verify_records(&records, true);
    let package = harness
        .host
        .create_bid_decision_package(CreateBidDecisionPackageCommand {
            tender_id: harness.tender_id.clone(),
            base_version: None,
            disposition_updates: complete_dispositions(&records),
            manager_capability_demands: Vec::new(),
        })
        .expect("create contradictory package");
    assert!(package
        .blockers
        .iter()
        .any(|blocker| { blocker.code == "unresolved_blocking_contradiction" }));

    harness.set_agent_scenario("bid-package-review-failed");
    let reviewed = harness
        .host
        .run_bid_decision_package_review(RunBidDecisionPackageReviewCommand {
            tender_id: harness.tender_id.clone(),
            package_id: package.package_id,
            version: package.version,
        })
        .await
        .expect("record failed exact-package review");
    assert_eq!(
        reviewed
            .package
            .review
            .as_ref()
            .map(|review| review.outcome),
        Some(BidDecisionPackageReviewOutcome::Failed)
    );
    assert!(!reviewed.package.decision_gate_ready);
}

#[tokio::test]
async fn verified_high_risk_package_carries_an_evidence_linked_decline_recommendation() {
    let harness = Harness::new("record-extraction-decline-risk");
    let records = harness.extract_records().await;
    harness.verify_records(&records, false);
    let package = harness
        .host
        .create_bid_decision_package(CreateBidDecisionPackageCommand {
            tender_id: harness.tender_id.clone(),
            base_version: None,
            disposition_updates: complete_dispositions(&records),
            manager_capability_demands: Vec::new(),
        })
        .expect("create high-risk package");
    assert_eq!(
        package.recommendation.outcome,
        BidRecommendationOutcome::Decline
    );
    harness.set_agent_scenario("bid-package-review");
    let reviewed = harness
        .host
        .run_bid_decision_package_review(RunBidDecisionPackageReviewCommand {
            tender_id: harness.tender_id.clone(),
            package_id: package.package_id.clone(),
            version: package.version,
        })
        .await
        .expect("review high-risk package");
    assert!(reviewed.package.decision_gate_ready);
    assert!(!reviewed.package.recommendation.evidence_records.is_empty());
}

#[tokio::test]
async fn unsupported_manager_demand_remains_a_visible_capability_gap() {
    let harness = Harness::new("record-extraction");
    let records = harness.extract_records().await;
    harness.verify_records(&records, false);
    let trigger = records
        .iter()
        .find(|record| record.kind == TenderRecordKind::ProjectCharacteristic)
        .expect("Project Fingerprint trigger");
    let package = harness
        .host
        .create_bid_decision_package(CreateBidDecisionPackageCommand {
            tender_id: harness.tender_id.clone(),
            base_version: None,
            disposition_updates: complete_dispositions(&records),
            manager_capability_demands: vec![ManagerCapabilityDemandInput {
                capability: "specialist_facade_engineering".into(),
                rationale: "Manager-added specialist need for the verified project context.".into(),
                triggering_record: Some(TenderRecordVersionReference {
                    record_id: trigger.record_id.clone(),
                    version: trigger.version,
                }),
            }],
        })
        .expect("create package with Capability Gap");
    assert_eq!(package.capability_gap_count, 1);
    assert_eq!(
        package.recommendation.outcome,
        BidRecommendationOutcome::Hold
    );
    assert!(package
        .blockers
        .iter()
        .any(|blocker| blocker.code == "capability_gap"));
}

#[tokio::test]
async fn review_output_cannot_attach_after_its_exact_package_version_is_superseded() {
    let harness = Harness::new("record-extraction");
    let records = harness.extract_records().await;
    harness.verify_records(&records, false);
    let package = harness
        .host
        .create_bid_decision_package(CreateBidDecisionPackageCommand {
            tender_id: harness.tender_id.clone(),
            base_version: None,
            disposition_updates: complete_dispositions(&records),
            manager_capability_demands: Vec::new(),
        })
        .expect("create review target");
    harness.set_agent_scenario("bid-package-review-delayed");
    let host = harness.host.clone();
    let tender_id = harness.tender_id.clone();
    let package_id = package.package_id.clone();
    let review_task = tokio::spawn(async move {
        host.run_bid_decision_package_review(RunBidDecisionPackageReviewCommand {
            tender_id,
            package_id,
            version: 1,
        })
        .await
    });
    let waiting = harness.codex.with_extension("bid-package-review-waiting");
    for _ in 0..1_000 {
        if waiting.is_file() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    assert!(
        waiting.is_file(),
        "review did not reach delayed boundary; fixture error: {}",
        fs::read_to_string(harness.codex.with_extension("fixture-error"))
            .unwrap_or_else(|_| "none".into())
    );
    let replacement = harness
        .host
        .create_bid_decision_package(CreateBidDecisionPackageCommand {
            tender_id: harness.tender_id.clone(),
            base_version: Some(1),
            disposition_updates: Vec::new(),
            manager_capability_demands: Vec::new(),
        })
        .expect("create exact replacement package version");
    assert_eq!(replacement.version, 2);
    fs::write(
        harness.codex.with_extension("bid-package-review-release"),
        b"release",
    )
    .expect("release delayed review");
    let reviewed = review_task
        .await
        .expect("join review")
        .expect("terminalize superseded review run");
    assert_eq!(reviewed.run.state, AgentRunState::Failed);
    assert_eq!(
        reviewed.run.failure.map(|failure| failure.category),
        Some(ProviderFailureCategory::OutputInvalid)
    );
    assert!(reviewed.package.review.is_none());
    assert!(harness
        .host
        .inspect_current_bid_decision_package(&harness.tender_id)
        .expect("inspect current package")
        .is_some_and(|current| current.version == 2 && current.review.is_none()));
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
