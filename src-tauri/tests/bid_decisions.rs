use std::{fs, io, path::Path, sync::Arc};

use serde_json::Value;
use sha2::{Digest, Sha256};

use quantix_lib::{
    ensure_quantix_setup, ActivateTenderProductionCommand, AgentProfileStatus,
    AgentRunRecoveryDisposition, AgentRunState, AgentTaskInputReference,
    ApproveBasisOfEstimateCommand, ApproveCalculationRuleCommand, ApproveCommercialStrategyCommand,
    ApproveControlledBoqCalculationRunCommand, ApproveExternalRfiForIssueCommand,
    ApprovePricedCostBaselineCommand, ApprovePricingAdjustmentCommand,
    ApproveProductionFindingExceptionCommand, ApproveTenderPriceCommand,
    AssembleCoordinatedBidBaselineCommand, BasisOfEstimateReviewOutcome, BasisOfEstimateVersion,
    BidDecisionApprovalDecision, BidDecisionPackageInspection, BidDecisionPackageReviewOutcome,
    BidRecommendationOutcome, BoqRowDisposition, CalculationDecimalInput, CalculationInputState,
    CalculationRoundingMode, CalculationRuleReviewOutcome, ComplianceDisposition,
    ComplianceDispositionUpdate, ComposeTenderOfficeCommand, ConfirmSourceRelationshipCommand,
    ControlledBoqCalculationStatus, CoordinatedBidBaselineBlockerCode,
    CoordinatedBidBaselineDecision, CreateBidDecisionPackageCommand,
    CreateCalculationScenarioCommand, CreateCommercialStrategyCommand,
    CreateExternalRfiDraftCommand, CreatePricedCostBaselineCommand, CreatePricingAdjustmentCommand,
    CreatePricingScenarioCommand, CreateTenderCommand, CreateTenderEngineerEntryCommand,
    CreateTenderQueryCommand, DecideBidDecisionPackageCommand, DecideCoordinatedBidBaselineCommand,
    DecideTenderQueryTreatmentCommand, DecideTenderRecordCommand, DecideWorkPlanProposalCommand,
    DesignateBoqTableCommand, DeviceProtection, ExchangeRateType, ExportApprovedExternalRfiCommand,
    ExternalRfiQueryReference, ExternalRfiRecipient, ImportTenderPackageCommand,
    InspectBidDecisionApprovalHistoryCommand, InspectCalculationWorkspaceCommand,
    InspectCoordinatedBidBaselinesCommand, InspectEstimateWorkspaceCommand,
    InspectExternalRfiResponseCandidatesCommand, InspectExternalRfisCommand,
    InspectProductionTaskReviewCommand, InspectTenderQueriesCommand,
    InterpretExternalRfiResponseCommand, InvalidateBidDecisionApprovalCommand, MajorFindingPolicy,
    ManagerCapabilityDemandInput, ParseSourceArtifactCommand, PricedCostBaselineReviewOutcome,
    PricingAdjustmentDirection, PricingAdjustmentKind, PricingAdjustmentReference,
    ProductionFindingDispositionKind, ProductionFindingSeverity, ProductionTaskState,
    ProposeBoqCalculationRuleCommand, ProviderFailureCategory, QuantixHost,
    RegisterExternalRfiResponseCommand, ResolveBidDecisionReturnReworkCommand,
    ResolveIndeterminateAgentRunCommand, ReviseExternalRfiDraftCommand, ReviseTenderCommand,
    ReviseTenderQueryCommand, ReviseWorkPlanProposalCommand, RunBasisOfEstimateReviewCommand,
    RunBidDecisionPackageReviewCommand, RunBootstrapAgentCommand, RunCalculationRuleReviewCommand,
    RunCostEstimatorBasisCommand, RunCostEstimatorCalculationCommand, RunExternalRfiReviewCommand,
    RunPricedCostBaselineReviewCommand, RunPricingAdjustmentReviewCommand,
    RunProductionTaskCommand, RunTenderRecordExtractionCommand, RuntimeLayout,
    SelectPricingScenarioCommand, SetupPlatform, SetupState, SourceRelationshipKind,
    StoragePermissions, TenderErrorCode, TenderEvidenceReference, TenderIntegrityState,
    TenderLifecyclePhase, TenderQuery, TenderQueryTreatment, TenderQueryTreatmentProposalInput,
    TenderQueryType, TenderRecordEngineerDecisionKind, TenderRecordInspection, TenderRecordKind,
    TenderRecordVersionReference, WorkPlanDecision, WorkPlanRevisionAction,
    MINIMUM_SETUP_FREE_SPACE_BYTES,
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

    async fn import_evidence(&self) -> Vec<TenderEvidenceReference> {
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
        self.host
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
            .collect()
    }

    async fn extract_records(&self) -> Vec<TenderRecordInspection> {
        let evidence = self.import_evidence().await;
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

#[tokio::test]
async fn coordinated_baseline_exposes_incomplete_integration_and_denies_approval() {
    let harness = Harness::new("record-extraction-coordinated");
    active_production(&harness).await;

    let proposed = harness
        .host
        .assemble_coordinated_bid_baseline(AssembleCoordinatedBidBaselineCommand {
            tender_id: harness.tender_id.clone(),
            base_version: None,
        })
        .expect("assemble a bounded blocked integration proposal");
    assert!(proposed.blockers.iter().any(|blocker| {
        blocker.code == CoordinatedBidBaselineBlockerCode::ProductionTaskNotReady
    }));
    assert!(proposed.blockers.iter().any(|blocker| {
        blocker.code == CoordinatedBidBaselineBlockerCode::ApprovedTenderPriceMissing
    }));
    assert_eq!(
        harness
            .host
            .open_tender(&harness.tender_id)
            .expect("inspect retained production lifecycle")
            .lifecycle_phase,
        TenderLifecyclePhase::ActiveProduction
    );
    assert_eq!(
        harness
            .host
            .decide_coordinated_bid_baseline(DecideCoordinatedBidBaselineCommand {
                tender_id: harness.tender_id.clone(),
                baseline_id: proposed.baseline_id,
                version: proposed.version,
                manifest_sha256: proposed.manifest_sha256,
                decision: CoordinatedBidBaselineDecision::Approve,
                rationale: "A blocked integration proposal cannot be approved.".into(),
                conditions: Vec::new(),
                exceptions: Vec::new(),
            })
            .expect_err("blockers deny Baseline Approval")
            .code,
        TenderErrorCode::InvalidCommand
    );
}

#[tokio::test]
async fn production_output_without_the_required_typed_coordination_fact_fails_closed() {
    let harness = Harness::new("record-extraction-coordinated");
    let (_, production) = active_production(&harness).await;
    let ready = production
        .tasks
        .iter()
        .find(|task| task.state == ProductionTaskState::Ready)
        .expect("ready production task");
    harness.set_agent_scenario("production-task-coordination-missing");
    let denied = harness
        .host
        .run_production_task(RunProductionTaskCommand {
            tender_id: harness.tender_id.clone(),
            production_task_id: ready.production_task_id.clone(),
        })
        .await
        .expect("terminalize invalid typed coordination output");
    assert_eq!(denied.run.state, AgentRunState::Failed);
    assert_eq!(
        denied.run.failure.as_ref().map(|failure| failure.category),
        Some(ProviderFailureCategory::OutputInvalid)
    );
    assert_eq!(denied.task.artifact_version_count, 0);

    let harness = Harness::new("record-extraction-coordinated");
    let (_, production) = active_production(&harness).await;
    let ready = production
        .tasks
        .iter()
        .find(|task| task.state == ProductionTaskState::Ready)
        .expect("second ready production task");
    harness.set_agent_scenario("production-task-coordination-wrong-key");
    let denied = harness
        .host
        .run_production_task(RunProductionTaskCommand {
            tender_id: harness.tender_id.clone(),
            production_task_id: ready.production_task_id.clone(),
        })
        .await
        .expect("terminalize the agent-invented coordination key");
    assert_eq!(denied.run.state, AgentRunState::Failed);
    assert_eq!(
        denied.run.failure.as_ref().map(|failure| failure.category),
        Some(ProviderFailureCategory::OutputInvalid)
    );
    assert_eq!(denied.task.artifact_version_count, 0);
}

#[tokio::test]
async fn production_coordination_contract_pages_more_than_thirty_two_host_owned_keys() {
    let harness = Harness::new("record-extraction-coordination-many");
    let (_, production) = active_production(&harness).await;
    let ready = production
        .tasks
        .iter()
        .find(|task| task.task.task_key == "tender_coordination_production")
        .expect("technical production task");
    harness.set_agent_scenario("production-task");
    let completed = harness
        .host
        .run_production_task(RunProductionTaskCommand {
            tender_id: harness.tender_id.clone(),
            production_task_id: ready.production_task_id.clone(),
        })
        .await
        .expect("publish the bounded multi-observation coordination account");
    assert_eq!(completed.run.state, AgentRunState::Completed);
    let payload: Value = serde_json::from_str(
        &completed
            .run
            .proposed_result
            .as_ref()
            .expect("production result")
            .payload_json,
    )
    .expect("typed production payload");
    let observations = payload["coordination_observations"]
        .as_array()
        .expect("coordination observations");
    assert!(observations.len() > 1);
    assert!(
        observations
            .iter()
            .filter_map(|observation| observation.pointer("/value/values")?.as_array())
            .map(Vec::len)
            .sum::<usize>()
            > 32
    );
    assert_eq!(completed.task.artifact_version_count, 1);
}

#[tokio::test]
async fn production_activation_audits_an_unrepresentable_coordination_contract() {
    let harness = Harness::new("record-extraction-coordination-overflow");
    let package = ready_package(&harness).await;
    harness
        .host
        .decide_bid_decision_package(approval_command(
            &harness,
            &package,
            BidDecisionApprovalDecision::Accept,
        ))
        .expect("accept the exact large package");
    let plan = harness
        .host
        .compose_tender_office(ComposeTenderOfficeCommand {
            tender_id: harness.tender_id.clone(),
        })
        .expect("compose the bounded Work Plan");
    let approved = harness
        .host
        .decide_work_plan_proposal(DecideWorkPlanProposalCommand {
            tender_id: harness.tender_id.clone(),
            plan_id: plan.plan_id,
            version: plan.version,
            decision: WorkPlanDecision::Approve,
            rationale: "Approve the exact plan before coordination preflight.".into(),
        })
        .expect("approve the exact large Work Plan");
    assert_eq!(
        harness
            .host
            .activate_tender_production(ActivateTenderProductionCommand {
                tender_id: harness.tender_id.clone(),
                plan_id: approved.plan_id,
                plan_version: approved.version,
                plan_manifest_sha256: approved.manifest_sha256,
            })
            .expect_err("oversized coordination contract must fail before provider dispatch")
            .code,
        TenderErrorCode::InvalidCommand
    );
    let database = harness
        .application_home
        .join("tenders")
        .join(&harness.tender_id)
        .join("tender.sqlite");
    let connection = rusqlite::Connection::open(database).expect("open large Tender Store");
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM audit_events
                 WHERE event_type = 'production_activation_denied'
                   AND json_extract(payload_json, '$.change.reason') = 'coordination_contract_boundary'",
                [],
                |row| row.get::<_, u32>(0),
            )
            .expect("count audited coordination boundary"),
        1
    );
    drop(connection);
    harness
        .host
        .close_tender(&harness.tender_id)
        .expect("close the non-activated large Tender");
    assert_eq!(
        harness
            .host
            .inspect_tender_integrity(&harness.tender_id)
            .expect("cold-verify no partial production activation")
            .state,
        TenderIntegrityState::Ready
    );
}

#[tokio::test]
async fn coordinated_baseline_binds_every_current_control_and_approves_only_the_exact_head() {
    let harness = Harness::new("record-extraction-coordinated");
    active_production(&harness).await;
    approve_exact_final_price_for_integration(&harness).await;
    complete_all_production_for_integration(&harness).await;

    let first = harness
        .host
        .assemble_coordinated_bid_baseline(AssembleCoordinatedBidBaselineCommand {
            tender_id: harness.tender_id.clone(),
            base_version: None,
        })
        .expect("assemble the exact coordinated baseline");
    assert!(first.blockers.is_empty(), "{:#?}", first.blockers);
    assert!(
        first.contradictions.is_empty(),
        "{:#?}",
        first.contradictions
    );
    assert_eq!(
        first
            .bindings
            .iter()
            .filter(|binding| {
                matches!(
                    binding.source.as_str(),
                    "submission_deadline" | "clarification_cutoff"
                )
            })
            .count(),
        2,
        "two distinct exact milestones must coexist without a false date contradiction"
    );
    let categories = first
        .bindings
        .iter()
        .map(|binding| binding.category)
        .collect::<std::collections::HashSet<_>>();
    for required in [
        quantix_lib::CoordinatedBidBaselineCategory::Technical,
        quantix_lib::CoordinatedBidBaselineCategory::Programme,
        quantix_lib::CoordinatedBidBaselineCategory::Procurement,
        quantix_lib::CoordinatedBidBaselineCategory::Contractual,
        quantix_lib::CoordinatedBidBaselineCategory::Risk,
        quantix_lib::CoordinatedBidBaselineCategory::Query,
        quantix_lib::CoordinatedBidBaselineCategory::Qualification,
        quantix_lib::CoordinatedBidBaselineCategory::Exclusion,
        quantix_lib::CoordinatedBidBaselineCategory::Submission,
        quantix_lib::CoordinatedBidBaselineCategory::Commercial,
    ] {
        assert!(categories.contains(&required), "missing {required:?}");
    }
    assert_eq!(
        harness
            .host
            .open_tender(&harness.tender_id)
            .expect("inspect Integrated Review transition")
            .lifecycle_phase,
        TenderLifecyclePhase::IntegratedReview
    );

    let successor = harness
        .host
        .assemble_coordinated_bid_baseline(AssembleCoordinatedBidBaselineCommand {
            tender_id: harness.tender_id.clone(),
            base_version: Some(first.version),
        })
        .expect("publish immutable exact successor before approval");
    assert_eq!(successor.version, first.version + 1);
    assert_eq!(
        harness
            .host
            .decide_coordinated_bid_baseline(DecideCoordinatedBidBaselineCommand {
                tender_id: harness.tender_id.clone(),
                baseline_id: first.baseline_id,
                version: first.version,
                manifest_sha256: first.manifest_sha256,
                decision: CoordinatedBidBaselineDecision::Approve,
                rationale: "A superseded version must lose the approval race.".into(),
                conditions: Vec::new(),
                exceptions: Vec::new(),
            })
            .expect_err("stale Baseline version cannot be approved")
            .code,
        TenderErrorCode::InvalidCommand
    );
    for invalid in [
        DecideCoordinatedBidBaselineCommand {
            tender_id: harness.tender_id.clone(),
            baseline_id: successor.baseline_id.clone(),
            version: successor.version,
            manifest_sha256: successor.manifest_sha256.clone(),
            decision: CoordinatedBidBaselineDecision::Approve,
            rationale: "   ".into(),
            conditions: Vec::new(),
            exceptions: Vec::new(),
        },
        DecideCoordinatedBidBaselineCommand {
            tender_id: harness.tender_id.clone(),
            baseline_id: successor.baseline_id.clone(),
            version: successor.version,
            manifest_sha256: successor.manifest_sha256.clone(),
            decision: CoordinatedBidBaselineDecision::Approve,
            rationale: "Exact decision shape must remain canonical.".into(),
            conditions: vec!["Same condition".into(), " Same condition ".into()],
            exceptions: Vec::new(),
        },
    ] {
        assert_eq!(
            harness
                .host
                .decide_coordinated_bid_baseline(invalid)
                .expect_err("semantic approval shape must fail closed and be audited")
                .code,
            TenderErrorCode::InvalidCommand
        );
    }
    let command = DecideCoordinatedBidBaselineCommand {
        tender_id: harness.tender_id.clone(),
        baseline_id: successor.baseline_id.clone(),
        version: successor.version,
        manifest_sha256: successor.manifest_sha256.clone(),
        decision: CoordinatedBidBaselineDecision::Approve,
        rationale: "Approve one exact reconciled Coordinated Bid Baseline.".into(),
        conditions: vec!["Package Production preserves every bound commitment.".into()],
        exceptions: vec!["Disclosed Minor findings remain visible in package review.".into()],
    };
    let decisions = std::thread::scope(|scope| {
        let barrier = Arc::new(std::sync::Barrier::new(3));
        let left_barrier = Arc::clone(&barrier);
        let right_barrier = Arc::clone(&barrier);
        let left_command = command.clone();
        let right_command = command.clone();
        let left_host = &harness.host;
        let right_host = &harness.host;
        let left = scope.spawn(move || {
            left_barrier.wait();
            left_host.decide_coordinated_bid_baseline(left_command)
        });
        let right = scope.spawn(move || {
            right_barrier.wait();
            right_host.decide_coordinated_bid_baseline(right_command)
        });
        barrier.wait();
        vec![
            left.join().expect("left approval caller"),
            right.join().expect("right approval caller"),
        ]
    });
    assert_eq!(
        decisions.iter().filter(|decision| decision.is_ok()).count(),
        1
    );
    assert_eq!(
        decisions
            .iter()
            .filter_map(|decision| decision.as_ref().err())
            .filter(|error| error.code == TenderErrorCode::InvalidCommand)
            .count(),
        1
    );
    let approved = decisions
        .into_iter()
        .find_map(Result::ok)
        .expect("exactly one caller approves the current Coordinated Bid Baseline");
    let approval = approved
        .approval
        .as_ref()
        .expect("immutable Baseline Approval");
    assert_eq!(approval.decision, CoordinatedBidBaselineDecision::Approve);
    assert_eq!(approval.decided_by, "engineer_user");
    assert_eq!(approval.acting_role, "tendering_manager");
    assert_eq!(
        harness
            .host
            .open_tender(&harness.tender_id)
            .expect("inspect Package Production transition")
            .lifecycle_phase,
        TenderLifecyclePhase::PackageProduction
    );
    assert_eq!(
        harness
            .host
            .revise_tender(ReviseTenderCommand {
                tender_id: harness.tender_id.clone(),
                name: "Late unassessed material change".into(),
            })
            .expect_err("Package Production must close ordinary material-change mutation")
            .code,
        TenderErrorCode::InvalidCommand
    );

    harness
        .host
        .close_tender(&harness.tender_id)
        .expect("close approved coordinated baseline Tender");
    let integrity = harness
        .host
        .inspect_tender_integrity(&harness.tender_id)
        .expect("cold-verify exact baseline history");
    assert_eq!(
        integrity.state,
        TenderIntegrityState::Ready,
        "integrity issues: {:#?}",
        integrity.issues
    );

    let database = harness
        .application_home
        .join("tenders")
        .join(&harness.tender_id)
        .join("tender.sqlite");
    let connection = rusqlite::Connection::open(database).expect("open coordinated baseline store");
    let audited_shape_denials: u32 = connection
        .query_row(
            "SELECT COUNT(*) FROM audit_events
             WHERE event_type = 'coordinated_bid_baseline_denied'
               AND json_extract(payload_json, '$.change.reason') = 'command_shape'",
            [],
            |row| row.get(0),
        )
        .expect("count audited semantic approval denials");
    assert_eq!(audited_shape_denials, 2);
    let exact_decision_events: u32 = connection
        .query_row(
            "SELECT COUNT(*) FROM audit_events
             WHERE event_type = 'coordinated_bid_baseline_decided'
               AND json_extract(payload_json, '$.change.baseline_id') = ?1
               AND json_extract(payload_json, '$.change.baseline_version') = ?2",
            rusqlite::params![successor.baseline_id, successor.version.to_string()],
            |row| row.get(0),
        )
        .expect("count the single immutable baseline decision");
    assert_eq!(exact_decision_events, 1);
    connection
        .execute(
            "UPDATE tender SET lifecycle_phase = 'active_production' WHERE singleton = 1",
            [],
        )
        .expect("corrupt the derived lifecycle only");
    let lifecycle_corrupted_host =
        QuantixHost::with_setup_platform(&harness.application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(
        ensure_quantix_setup(&lifecycle_corrupted_host).state,
        SetupState::Ready
    );
    assert_eq!(
        lifecycle_corrupted_host
            .inspect_tender_integrity(&harness.tender_id)
            .expect("cold-detect lifecycle divergence from exact Baseline Approval")
            .state,
        TenderIntegrityState::RecoveryRequired
    );
    connection
        .execute(
            "UPDATE tender SET lifecycle_phase = 'package_production' WHERE singleton = 1",
            [],
        )
        .expect("restore the exact approved lifecycle before the next corruption seam");
    connection
        .execute_batch("DROP TRIGGER coordinated_bid_baseline_approvals_no_update")
        .expect("open exact approval corruption seam");
    let manifest_json: String = connection
        .query_row(
            "SELECT manifest_json FROM coordinated_bid_baseline_approvals WHERE approval_id = ?1",
            [&approval.approval_id],
            |row| row.get(0),
        )
        .expect("load exact Baseline Approval manifest");
    let mut manifest: Value =
        serde_json::from_str(&manifest_json).expect("parse approval manifest");
    manifest["rationale"] = Value::String("Forged rationale outside the Audit chain.".into());
    let forged_manifest_json = serde_json::to_string(&manifest).expect("canonical forged approval");
    let forged_manifest_sha256 = Sha256::digest(forged_manifest_json.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    connection
        .execute(
            "UPDATE coordinated_bid_baseline_approvals
             SET rationale = ?1, manifest_json = ?2, approval_sha256 = ?3
             WHERE approval_id = ?4",
            rusqlite::params![
                "Forged rationale outside the Audit chain.",
                forged_manifest_json,
                forged_manifest_sha256,
                approval.approval_id,
            ],
        )
        .expect("tamper with a self-consistent approval record");
    drop(connection);
    let corrupted_host =
        QuantixHost::with_setup_platform(&harness.application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(
        ensure_quantix_setup(&corrupted_host).state,
        SetupState::Ready
    );
    assert_eq!(
        corrupted_host
            .inspect_tender_integrity(&harness.tender_id)
            .expect("cold-detect approval facts diverging from Audit")
            .state,
        TenderIntegrityState::RecoveryRequired
    );
}

#[tokio::test]
async fn coordinated_baseline_surfaces_recorded_cross_workstream_contradictions() {
    let harness = Harness::new("record-extraction-coordinated");
    active_production(&harness).await;
    approve_exact_final_price_for_integration(&harness).await;
    harness.set_agent_scenario("production-task");
    let cost_task_id = loop {
        let production = harness
            .host
            .inspect_tender_production(&harness.tender_id)
            .expect("inspect the coordination frontier")
            .expect("active production");
        if let Some(task) = production.tasks.iter().find(|task| {
            task.task.task_key == "cost_estimation_production"
                && task.state == ProductionTaskState::Ready
        }) {
            break task.production_task_id.clone();
        }
        let task = production
            .tasks
            .iter()
            .find(|task| {
                matches!(
                    task.state,
                    ProductionTaskState::Ready
                        | ProductionTaskState::ReviewReady
                        | ProductionTaskState::RemediationReady
                )
            })
            .unwrap_or_else(|| panic!("cost workstream never became ready: {production:#?}"));
        harness
            .host
            .run_production_task(RunProductionTaskCommand {
                tender_id: harness.tender_id.clone(),
                production_task_id: task.production_task_id.clone(),
            })
            .await
            .expect("advance exact prerequisites for cost coordination");
    };
    harness.set_agent_scenario("production-task-coordination-cost-conflict");
    harness
        .host
        .run_production_task(RunProductionTaskCommand {
            tender_id: harness.tender_id.clone(),
            production_task_id: cost_task_id,
        })
        .await
        .expect("publish a typed production cost observation that conflicts with pricing");
    complete_all_production_for_integration(&harness).await;
    let proposed = harness
        .host
        .assemble_coordinated_bid_baseline(AssembleCoordinatedBidBaselineCommand {
            tender_id: harness.tender_id.clone(),
            base_version: None,
        })
        .expect("assemble contradiction-bearing proposal");
    let cost_conflict = proposed
        .contradictions
        .iter()
        .find(|contradiction| contradiction.key == "expected_delivery_cost")
        .expect("production and approved pricing disagree on one exact cost subject");
    assert!(cost_conflict.references.len() >= 2);
    assert!(proposed
        .blockers
        .iter()
        .any(|blocker| { blocker.code == CoordinatedBidBaselineBlockerCode::ContradictionOpen }));
}

#[tokio::test]
async fn coordinated_baseline_compares_non_numeric_commitments_across_workstreams() {
    let harness = Harness::new("record-extraction-coordinated");
    let (_, production) = active_production(&harness).await;
    approve_exact_final_price_for_integration(&harness).await;
    let coordination = production
        .tasks
        .iter()
        .find(|task| task.task.task_key == "tender_coordination_production")
        .expect("ready coordination workstream");
    harness.set_agent_scenario("production-task-coordination-commitment-conflict");
    harness
        .host
        .run_production_task(RunProductionTaskCommand {
            tender_id: harness.tender_id.clone(),
            production_task_id: coordination.production_task_id.clone(),
        })
        .await
        .expect("publish a production commitment against one Host-owned source key");
    complete_all_production_for_integration(&harness).await;

    let proposed = harness
        .host
        .assemble_coordinated_bid_baseline(AssembleCoordinatedBidBaselineCommand {
            tender_id: harness.tender_id.clone(),
            base_version: None,
        })
        .expect("assemble the commitment contradiction");
    let contradiction = proposed
        .contradictions
        .iter()
        .find(|contradiction| {
            contradiction.key == "commitment:v:project_delivery_context.v:required_capability"
        })
        .expect("production must be reconciled against the exact Tender Record proposition");
    assert_eq!(
        contradiction.category,
        quantix_lib::CoordinatedBidBaselineContradictionCategory::Commitment
    );
    assert!(contradiction.references.len() >= 2);
}

#[tokio::test]
async fn coordinated_baseline_reconciles_work_plan_deadlines_and_responsibility() {
    let harness = Harness::new("record-extraction-coordinated");
    let (_, production) = active_production(&harness).await;
    approve_exact_final_price_for_integration(&harness).await;
    let coordination = production
        .tasks
        .iter()
        .find(|task| task.task.task_key == "tender_coordination_production")
        .expect("ready coordination workstream");
    let document_control = production
        .tasks
        .iter()
        .find(|task| task.task.task_key == "document_control_production")
        .expect("ready document-control workstream");
    harness.set_agent_scenario("production-task-coordination-date-source-responsibility-conflict");
    let deadline_and_source_result = harness
        .host
        .run_production_task(RunProductionTaskCommand {
            tender_id: harness.tender_id.clone(),
            production_task_id: coordination.production_task_id.clone(),
        })
        .await
        .expect("publish a conflicting typed deadline");
    assert_eq!(
        deadline_and_source_result.run.state,
        AgentRunState::Completed,
        "deadline/source contradiction fixture must publish a complete attributable artifact: {:#?}",
        deadline_and_source_result.run
    );
    harness.set_agent_scenario("production-task-coordination-responsibility-conflict");
    let responsibility_result = harness
        .host
        .run_production_task(RunProductionTaskCommand {
            tender_id: harness.tender_id.clone(),
            production_task_id: document_control.production_task_id.clone(),
        })
        .await
        .expect("publish a conflicting typed accountable party");
    assert_eq!(
        responsibility_result.run.state,
        AgentRunState::Completed,
        "responsibility contradiction fixture must publish a complete attributable artifact: {:#?}",
        responsibility_result.run.failure
    );
    complete_all_production_for_integration(&harness).await;

    let proposed = harness
        .host
        .assemble_coordinated_bid_baseline(AssembleCoordinatedBidBaselineCommand {
            tender_id: harness.tender_id.clone(),
            base_version: None,
        })
        .expect("assemble Work Plan coordination conflicts");
    assert!(proposed.contradictions.iter().any(|contradiction| {
        contradiction.category == quantix_lib::CoordinatedBidBaselineContradictionCategory::Date
            && contradiction.key.contains("tender_coordination_production")
    }));
    assert!(proposed.contradictions.iter().any(|contradiction| {
        contradiction.category
            == quantix_lib::CoordinatedBidBaselineContradictionCategory::Responsibility
            && contradiction.key.contains("document_control_production")
    }));
    assert!(proposed.contradictions.iter().any(|contradiction| {
        contradiction.category
            == quantix_lib::CoordinatedBidBaselineContradictionCategory::Responsibility
            && contradiction.key.contains("project_delivery_context")
            && contradiction.key.contains("responsible_party")
    }));
    assert!(proposed
        .blockers
        .iter()
        .any(|blocker| blocker.code == CoordinatedBidBaselineBlockerCode::ContradictionOpen));
}

#[tokio::test]
async fn coordinated_baseline_marks_exact_dependencies_stale_and_denies_approval() {
    let harness = Harness::new("record-extraction-coordinated");
    active_production(&harness).await;
    approve_exact_final_price_for_integration(&harness).await;

    let first = harness
        .host
        .assemble_coordinated_bid_baseline(AssembleCoordinatedBidBaselineCommand {
            tender_id: harness.tender_id.clone(),
            base_version: None,
        })
        .expect("capture the incomplete exact dependency snapshot");
    let query_page = harness
        .host
        .inspect_tender_queries(InspectTenderQueriesCommand {
            tender_id: harness.tender_id.clone(),
            cursor: None,
            limit: 8,
        })
        .expect("inspect the current Query Register");
    let owner = query_page
        .owner_profiles
        .first()
        .expect("approved Query owner");
    let evidence = query_page
        .items
        .first()
        .and_then(|query| query.evidence.first())
        .cloned()
        .expect("an exact registered source reference");
    let late_query = harness
        .host
        .create_tender_query(CreateTenderQueryCommand {
            tender_id: harness.tender_id.clone(),
            query_type: TenderQueryType::Ambiguity,
            question: "Which late qualification applies to the priced cost basis?".into(),
            ambiguity_or_gap:
                "A new exact estimating dependency was registered after the coordinated snapshot."
                    .into(),
            owner_profile_id: owner.profile_id.clone(),
            owner_profile_version: owner.version,
            evidence: vec![evidence],
            affected_records: Vec::new(),
            affected_task_keys: vec!["*".into()],
            due_at: "2099-01-01T00:00:00.000Z".into(),
            material: false,
            release_blocking: false,
            proposed_treatments: vec![TenderQueryTreatmentProposalInput {
                treatment: TenderQueryTreatment::Qualification,
                rationale: "Carry the exact clarification only after a new reviewed Basis.".into(),
            }],
        })
        .expect("register an allowed late exact pricing dependency");
    harness
        .host
        .decide_tender_query_treatment(DecideTenderQueryTreatmentCommand {
            tender_id: harness.tender_id.clone(),
            query_id: late_query.query_id.clone(),
            query_version: late_query.version,
            treatment: TenderQueryTreatment::Qualification,
            rationale:
                "Approve the separate late qualification without changing the earlier assumption."
                    .into(),
            treatment_details:
                "Carry this exact late qualification as a distinct Query proposition.".into(),
            closes_query: true,
        })
        .expect("approve a second distinct Query treatment");
    let page = harness
        .host
        .inspect_coordinated_bid_baselines(InspectCoordinatedBidBaselinesCommand {
            tender_id: harness.tender_id.clone(),
            before_version: None,
            limit: 4,
        })
        .expect("inspect dynamic baseline currentness");
    assert!(!page.items[0].current);
    assert_eq!(
        harness
            .host
            .decide_coordinated_bid_baseline(DecideCoordinatedBidBaselineCommand {
                tender_id: harness.tender_id.clone(),
                baseline_id: first.baseline_id.clone(),
                version: first.version,
                manifest_sha256: first.manifest_sha256,
                decision: CoordinatedBidBaselineDecision::Approve,
                rationale: "A stale exact dependency cannot pass Integrated Review.".into(),
                conditions: Vec::new(),
                exceptions: Vec::new(),
            })
            .expect_err("stale dependency must deny exact approval")
            .code,
        TenderErrorCode::InvalidCommand
    );
    let successor = harness
        .host
        .assemble_coordinated_bid_baseline(AssembleCoordinatedBidBaselineCommand {
            tender_id: harness.tender_id.clone(),
            base_version: Some(first.version),
        })
        .expect("assemble a successor that visibly carries stale dependencies");
    assert!(successor
        .blockers
        .iter()
        .any(|blocker| blocker.code == CoordinatedBidBaselineBlockerCode::StaleInput));
    assert!(successor
        .contradictions
        .iter()
        .all(|contradiction| !contradiction.key.starts_with("query_treatment")));

    harness
        .host
        .close_tender(&harness.tender_id)
        .expect("close stale baseline Tender");
    assert_eq!(
        harness
            .host
            .inspect_tender_integrity(&harness.tender_id)
            .expect("cold-verify attributable stale history")
            .state,
        TenderIntegrityState::Ready
    );
}

#[tokio::test]
async fn record_inventory_overflow_fails_terminally_without_publication_and_is_audited() {
    let harness = Harness::new("record-extraction-inventory-fill");
    let evidence = harness.import_evidence().await;
    let filled = harness
        .host
        .run_tender_record_extraction(RunTenderRecordExtractionCommand {
            tender_id: harness.tender_id.clone(),
            evidence: evidence.clone(),
            authorities: Vec::new(),
        })
        .await
        .expect("fill the bounded Bid Decision record inventory");
    assert_eq!(filled.run.state, AgentRunState::Completed);
    assert_eq!(filled.published_record_count, 255);

    harness.set_agent_scenario("record-extraction-inventory-overflow");
    let denied = harness
        .host
        .run_tender_record_extraction(RunTenderRecordExtractionCommand {
            tender_id: harness.tender_id.clone(),
            evidence,
            authorities: Vec::new(),
        })
        .await
        .expect("terminalize the denied publication");
    assert_eq!(denied.run.state, AgentRunState::Failed);
    assert_eq!(
        denied.run.failure.as_ref().map(|failure| failure.category),
        Some(ProviderFailureCategory::OutputInvalid)
    );
    assert_eq!(denied.published_record_count, 0);

    let database = harness
        .application_home
        .join("tenders")
        .join(&harness.tender_id)
        .join("tender.sqlite");
    let connection = rusqlite::Connection::open(database).expect("open Tender Store");
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM tender_record_heads", [], |row| {
                row.get::<_, u32>(0)
            })
            .expect("count retained Tender Records"),
        255
    );
    let denial_payload: String = connection
        .query_row(
            "SELECT payload_json FROM audit_events
             WHERE event_type = 'tender_record_publication_denied'
             ORDER BY sequence DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .expect("inspect publication denial audit");
    let denial_payload: Value =
        serde_json::from_str(&denial_payload).expect("parse publication denial audit");
    assert_eq!(
        denial_payload["change"]["reason"],
        Value::String("bid_decision_record_inventory_limit".into())
    );
    assert_eq!(
        denial_payload["change"]["run_id"],
        Value::String(denied.run.run_id)
    );
    assert_eq!(
        denial_payload["change"]["candidate_record_count"],
        Value::String("2".into())
    );
}

#[tokio::test]
async fn exact_proceed_composes_the_mandatory_tender_office_as_a_proposal() {
    let harness = Harness::new("record-extraction");
    let package = ready_package(&harness).await;
    harness
        .host
        .decide_bid_decision_package(approval_command(
            &harness,
            &package,
            BidDecisionApprovalDecision::Accept,
        ))
        .expect("accept exact Bid Decision Package");

    let proposal = harness
        .host
        .compose_tender_office(ComposeTenderOfficeCommand {
            tender_id: harness.tender_id.clone(),
        })
        .expect("compose the deterministic Tender Office proposal");

    assert_eq!(proposal.version, 1);
    assert!(proposal.current);
    assert!(proposal.approval.is_none());
    assert!(proposal.capability_gaps.is_empty());
    assert!(proposal.profiles.iter().all(|binding| {
        binding.status == AgentProfileStatus::Proposed
            && !binding.profile.seniority.is_empty()
            && !binding.profile.objective.is_empty()
            && !binding.profile.behavior.is_empty()
            && !binding.profile.skepticism.is_empty()
            && !binding.profile.risk_tolerance.is_empty()
            && !binding.profile.permissions.data_scopes.is_empty()
            && !binding.profile.prohibited_actions.is_empty()
    }));
    let cost_estimator = proposal
        .profiles
        .iter()
        .find(|binding| binding.profile.identity == "Cost Estimator")
        .expect("mandatory Cost Estimator");
    assert!(cost_estimator
        .profile
        .capabilities
        .iter()
        .any(|capability| capability == "cost_estimation"));
    let cost_reviewer = proposal
        .profiles
        .iter()
        .find(|binding| {
            binding
                .profile
                .capabilities
                .iter()
                .any(|capability| capability == "review_cost_estimation")
        })
        .expect("separate qualified Cost Reviewer");
    assert_ne!(
        cost_estimator.profile.profile_id,
        cost_reviewer.profile.profile_id
    );
    assert!(proposal
        .workstreams
        .iter()
        .any(|workstream| workstream.workstream_key == "cost_estimation"));
    assert!(proposal
        .workstreams
        .iter()
        .any(|workstream| workstream.workstream_key == "query_rfi_control"));
    assert!(proposal.workstreams.iter().all(|workstream| {
        workstream
            .deadlines
            .contains(&"2026-05-15T11:00:00Z".into())
    }));
    assert!(proposal
        .tasks
        .iter()
        .all(|task| task.deadline == "2026-05-15T11:00:00Z"));
    assert_eq!(
        proposal.query_bindings.len() as u32,
        package.unresolved_query_count
    );
    assert!(proposal.tasks.iter().any(|task| {
        task.review_profile_id.is_some()
            && task.profile_id != task.review_profile_id.clone().expect("reviewer")
    }));
}

#[tokio::test]
async fn supported_conditional_demand_composes_a_separate_qualified_specialist() {
    let harness = Harness::new("record-extraction");
    let records = harness.extract_records().await;
    harness.verify_records(&records, false);
    let package = harness
        .host
        .create_bid_decision_package(CreateBidDecisionPackageCommand {
            tender_id: harness.tender_id.clone(),
            base_version: None,
            disposition_updates: complete_dispositions(&records),
            manager_capability_demands: vec![ManagerCapabilityDemandInput {
                capability: "programme_planning".into(),
                rationale: "The accelerated programme requires a dedicated planning specialist."
                    .into(),
                triggering_record: None,
            }],
        })
        .expect("create package with a supported conditional demand");
    harness.set_agent_scenario("bid-package-review");
    let package = harness
        .host
        .run_bid_decision_package_review(RunBidDecisionPackageReviewCommand {
            tender_id: harness.tender_id.clone(),
            package_id: package.package_id,
            version: package.version,
        })
        .await
        .expect("review conditional package")
        .package;
    harness
        .host
        .decide_bid_decision_package(approval_command(
            &harness,
            &package,
            BidDecisionApprovalDecision::Accept,
        ))
        .expect("accept conditional package");

    let proposal = harness
        .host
        .compose_tender_office(ComposeTenderOfficeCommand {
            tender_id: harness.tender_id.clone(),
        })
        .expect("compose conditional specialist");

    let specialist = proposal
        .profiles
        .iter()
        .find(|binding| {
            binding
                .profile
                .capabilities
                .iter()
                .any(|capability| capability == "programme_planning")
        })
        .expect("Planning Engineer profile");
    assert_eq!(specialist.profile.profession, "Planning Engineer");
    assert_ne!(
        specialist.profile.profile_id,
        proposal
            .profiles
            .iter()
            .find(|binding| {
                binding
                    .profile
                    .capabilities
                    .iter()
                    .any(|capability| capability == "independent_review")
            })
            .expect("independent reviewer")
            .profile
            .profile_id
    );
    let reviewer = proposal
        .profiles
        .iter()
        .find(|binding| {
            binding
                .profile
                .capabilities
                .iter()
                .any(|capability| capability == "review_programme_planning")
        })
        .expect("separate qualified Planning Reviewer");
    let programme_task = proposal
        .tasks
        .iter()
        .find(|task| task.workstream_key == "programme_planning")
        .expect("programme task");
    assert_eq!(
        programme_task.review_profile_id.as_deref(),
        Some(reviewer.profile.profile_id.as_str())
    );
}

#[tokio::test]
async fn manager_team_actions_create_validated_proposal_versions_and_visible_gaps() {
    let harness = Harness::new("record-extraction");
    let package = ready_package(&harness).await;
    harness
        .host
        .decide_bid_decision_package(approval_command(
            &harness,
            &package,
            BidDecisionApprovalDecision::Accept,
        ))
        .expect("accept exact package");
    let first = harness
        .host
        .compose_tender_office(ComposeTenderOfficeCommand {
            tender_id: harness.tender_id.clone(),
        })
        .expect("compose first proposal");
    let coordinator = first
        .profiles
        .iter()
        .find(|binding| binding.archetype == "tender_office_coordinator")
        .expect("Coordinator")
        .profile
        .profile_id
        .clone();
    let split = harness
        .host
        .revise_work_plan_proposal(ReviseWorkPlanProposalCommand {
            tender_id: harness.tender_id.clone(),
            plan_id: first.plan_id.clone(),
            base_version: first.version,
            actions: vec![WorkPlanRevisionAction::SplitProfile {
                profile_id: coordinator,
                identities: vec!["Tender Coordinator".into(), "Query Coordinator".into()],
            }],
        })
        .expect("split compatible Coordinator responsibilities");
    assert_eq!(split.version, 2);
    let split_profiles = split
        .profiles
        .iter()
        .filter(|binding| binding.archetype == "tender_office_coordinator")
        .map(|binding| binding.profile.profile_id.clone())
        .collect::<Vec<_>>();
    assert_eq!(split_profiles.len(), 2);
    assert!(split
        .profiles
        .iter()
        .filter(|binding| binding.archetype == "tender_office_coordinator")
        .all(|binding| {
            let capability = binding.profile.capabilities.first().map(String::as_str);
            let owned_scope = match capability {
                Some("tender_coordination") => Some("tender_coordination"),
                Some("query_rfi_control") => Some("tender_queries"),
                _ => None,
            };
            binding.profile.capabilities.len() == 1
                && owned_scope.is_some_and(|scope| {
                    binding
                        .profile
                        .permissions
                        .data_scopes
                        .iter()
                        .any(|candidate| candidate == scope)
                })
                && binding.profile.permissions.allowed_actions.len() == 1
        }));
    assert_eq!(
        split.workstreams[0].deadlines,
        first.workstreams[0].deadlines
    );
    let combined = harness
        .host
        .revise_work_plan_proposal(ReviseWorkPlanProposalCommand {
            tender_id: harness.tender_id.clone(),
            plan_id: split.plan_id.clone(),
            base_version: split.version,
            actions: vec![WorkPlanRevisionAction::CombineProfiles {
                profile_ids: split_profiles,
                identity: "Tender Coordination and Query Lead".into(),
            }],
        })
        .expect("combine compatible Coordinator responsibilities");
    assert_eq!(combined.version, 3);
    let recombined = combined
        .profiles
        .iter()
        .find(|binding| binding.profile.identity == "Tender Coordination and Query Lead")
        .expect("recombined Coordinator");
    assert_eq!(recombined.profile.capabilities.len(), 2);
    assert!(recombined
        .profile
        .permissions
        .data_scopes
        .iter()
        .any(|scope| scope == "tender_coordination"));
    assert!(recombined
        .profile
        .permissions
        .data_scopes
        .iter()
        .any(|scope| scope == "tender_queries"));
    assert_eq!(
        combined.workstreams[0].deadlines,
        first.workstreams[0].deadlines
    );
    let analyst = combined
        .profiles
        .iter()
        .find(|binding| binding.archetype == "tender_analyst")
        .expect("Tender Analyst")
        .profile
        .profile_id
        .clone();
    let renamed = harness
        .host
        .revise_work_plan_proposal(ReviseWorkPlanProposalCommand {
            tender_id: harness.tender_id.clone(),
            plan_id: combined.plan_id.clone(),
            base_version: combined.version,
            actions: vec![WorkPlanRevisionAction::RenameProfile {
                profile_id: analyst,
                identity: "Tender Requirements Analyst".into(),
            }],
        })
        .expect("rename exact profile");
    assert!(renamed
        .profiles
        .iter()
        .any(|binding| binding.profile.identity == "Tender Requirements Analyst"));
    let estimator = renamed
        .profiles
        .iter()
        .find(|binding| binding.archetype == "cost_estimator")
        .expect("Cost Estimator")
        .profile
        .profile_id
        .clone();
    let adjusted = harness
        .host
        .revise_work_plan_proposal(ReviseWorkPlanProposalCommand {
            tender_id: harness.tender_id.clone(),
            plan_id: renamed.plan_id.clone(),
            base_version: renamed.version,
            actions: vec![WorkPlanRevisionAction::AdjustProfile {
                profile_id: estimator.clone(),
                objective:
                    "Develop the controlled estimate and explicitly reconcile quotation gaps."
                        .into(),
                behavior: "Use deterministic calculation inputs and preserve every exception."
                    .into(),
                skepticism:
                    "Challenge missing quantities, optimistic rates, and unsupported allowances."
                        .into(),
                risk_tolerance: "Very low tolerance for unpriced or unreviewed exposure.".into(),
                resource_budget: quantix_lib::AgentResourceBudget {
                    provider_turns: 1,
                    duration_seconds: 90,
                    output_bytes: 128 * 1024,
                },
            }],
        })
        .expect("adjust profile inside Safety Limits");
    let removed = harness
        .host
        .revise_work_plan_proposal(ReviseWorkPlanProposalCommand {
            tender_id: harness.tender_id.clone(),
            plan_id: adjusted.plan_id.clone(),
            base_version: adjusted.version,
            actions: vec![WorkPlanRevisionAction::RemoveProfile {
                profile_id: estimator,
            }],
        })
        .expect("remove Cost Estimator into a blocking gap");
    assert!(removed
        .capability_gaps
        .iter()
        .any(|gap| gap.capability == "cost_estimation"));
    assert!(removed
        .blocker_codes
        .iter()
        .any(|code| code == "capability_gap"));
    let restored = harness
        .host
        .revise_work_plan_proposal(ReviseWorkPlanProposalCommand {
            tender_id: harness.tender_id.clone(),
            plan_id: removed.plan_id.clone(),
            base_version: removed.version,
            actions: vec![WorkPlanRevisionAction::AddProfile {
                archetype: "cost_estimator".into(),
                identity: "Cost Estimator".into(),
            }],
        })
        .expect("restore mandatory Cost Estimator");
    assert_eq!(restored.version, 7);
    assert!(restored.capability_gaps.is_empty());
    let document_controller = restored
        .profiles
        .iter()
        .find(|binding| binding.archetype == "document_controller")
        .expect("Document Controller")
        .profile
        .profile_id
        .clone();
    let missing_core = harness
        .host
        .revise_work_plan_proposal(ReviseWorkPlanProposalCommand {
            tender_id: harness.tender_id.clone(),
            plan_id: restored.plan_id.clone(),
            base_version: restored.version,
            actions: vec![WorkPlanRevisionAction::RemoveProfile {
                profile_id: document_controller,
            }],
        })
        .expect("remove a core profile into a visible blocking gap");
    assert!(missing_core
        .capability_gaps
        .iter()
        .any(|gap| gap.capability == "document_control"));
    let repaired_core = harness
        .host
        .revise_work_plan_proposal(ReviseWorkPlanProposalCommand {
            tender_id: harness.tender_id.clone(),
            plan_id: missing_core.plan_id,
            base_version: missing_core.version,
            actions: vec![WorkPlanRevisionAction::AddProfile {
                archetype: "document_controller".into(),
                identity: "Replacement Document Controller".into(),
            }],
        })
        .expect("restore a removed core profile through a validated revision");
    assert!(repaired_core.capability_gaps.is_empty());
    harness
        .host
        .close_tender(&harness.tender_id)
        .expect("close before revision-lineage integrity inspection");
    assert_eq!(
        harness
            .host
            .inspect_tender_integrity(&harness.tender_id)
            .expect("inspect exact manager revision lineage")
            .state,
        TenderIntegrityState::Ready
    );
}

#[tokio::test]
async fn exact_work_plan_approval_enforces_conflicts_staleness_gaps_and_one_decision() {
    let harness = Harness::new("record-extraction");
    let package = ready_package(&harness).await;
    harness
        .host
        .decide_bid_decision_package(approval_command(
            &harness,
            &package,
            BidDecisionApprovalDecision::Accept,
        ))
        .expect("accept exact package");
    let first = harness
        .host
        .compose_tender_office(ComposeTenderOfficeCommand {
            tender_id: harness.tender_id.clone(),
        })
        .expect("compose proposal");
    let reviewer = first
        .profiles
        .iter()
        .find(|binding| binding.archetype == "independent_reviewer")
        .expect("reviewer")
        .profile
        .profile_id
        .clone();
    let estimator = first
        .profiles
        .iter()
        .find(|binding| binding.archetype == "cost_estimator")
        .expect("estimator")
        .profile
        .profile_id
        .clone();
    let audit_before_conflict = harness
        .host
        .open_tender(&harness.tender_id)
        .expect("inspect audit before conflict denial")
        .audit_event_count;
    assert_eq!(
        harness
            .host
            .revise_work_plan_proposal(ReviseWorkPlanProposalCommand {
                tender_id: harness.tender_id.clone(),
                plan_id: first.plan_id.clone(),
                base_version: first.version,
                actions: vec![WorkPlanRevisionAction::CombineProfiles {
                    profile_ids: vec![reviewer, estimator.clone()],
                    identity: "Conflicted Author Reviewer".into(),
                }],
            })
            .expect_err("author and reviewer must stay separate")
            .code,
        TenderErrorCode::InvalidCommand
    );
    assert_eq!(
        harness
            .host
            .open_tender(&harness.tender_id)
            .expect("inspect attributable conflict denial")
            .audit_event_count,
        audit_before_conflict + 1
    );
    let analyst = first
        .profiles
        .iter()
        .find(|binding| binding.archetype == "tender_analyst")
        .expect("analyst")
        .profile
        .profile_id
        .clone();
    let second = harness
        .host
        .revise_work_plan_proposal(ReviseWorkPlanProposalCommand {
            tender_id: harness.tender_id.clone(),
            plan_id: first.plan_id.clone(),
            base_version: first.version,
            actions: vec![WorkPlanRevisionAction::RenameProfile {
                profile_id: analyst,
                identity: "Tender Requirements Analyst".into(),
            }],
        })
        .expect("revise proposal");
    let stale = harness
        .host
        .decide_work_plan_proposal(DecideWorkPlanProposalCommand {
            tender_id: harness.tender_id.clone(),
            plan_id: first.plan_id.clone(),
            version: first.version,
            decision: WorkPlanDecision::Approve,
            rationale: "Approve stale plan.".into(),
        })
        .expect_err("superseded proposal cannot be approved");
    assert_eq!(stale.code, TenderErrorCode::InvalidCommand);
    let approved = harness
        .host
        .decide_work_plan_proposal(DecideWorkPlanProposalCommand {
            tender_id: harness.tender_id.clone(),
            plan_id: second.plan_id.clone(),
            version: second.version,
            decision: WorkPlanDecision::Approve,
            rationale: "Approve the exact validated Work Plan for controlled production.".into(),
        })
        .expect("approve exact Work Plan");
    assert_eq!(
        approved.approval.as_ref().map(|approval| approval.decision),
        Some(WorkPlanDecision::Approve)
    );
    assert!(approved
        .profiles
        .iter()
        .all(|binding| binding.status == AgentProfileStatus::Proposed));
    assert_eq!(
        harness
            .host
            .decide_work_plan_proposal(DecideWorkPlanProposalCommand {
                tender_id: harness.tender_id.clone(),
                plan_id: second.plan_id,
                version: second.version,
                decision: WorkPlanDecision::Approve,
                rationale: "Duplicate approval.".into(),
            })
            .expect_err("duplicate approval must fail")
            .code,
        TenderErrorCode::InvalidCommand
    );

    let blocked_harness = Harness::new("record-extraction");
    let package = ready_package(&blocked_harness).await;
    blocked_harness
        .host
        .decide_bid_decision_package(approval_command(
            &blocked_harness,
            &package,
            BidDecisionApprovalDecision::Accept,
        ))
        .expect("accept exact package for blocked plan");
    let plan = blocked_harness
        .host
        .compose_tender_office(ComposeTenderOfficeCommand {
            tender_id: blocked_harness.tender_id.clone(),
        })
        .expect("compose blocked-plan basis");
    let estimator = plan
        .profiles
        .iter()
        .find(|binding| binding.archetype == "cost_estimator")
        .expect("estimator")
        .profile
        .profile_id
        .clone();
    let blocked = blocked_harness
        .host
        .revise_work_plan_proposal(ReviseWorkPlanProposalCommand {
            tender_id: blocked_harness.tender_id.clone(),
            plan_id: plan.plan_id,
            base_version: plan.version,
            actions: vec![WorkPlanRevisionAction::RemoveProfile {
                profile_id: estimator,
            }],
        })
        .expect("create blocked proposal");
    assert_eq!(
        blocked_harness
            .host
            .decide_work_plan_proposal(DecideWorkPlanProposalCommand {
                tender_id: blocked_harness.tender_id.clone(),
                plan_id: blocked.plan_id.clone(),
                version: blocked.version,
                decision: WorkPlanDecision::Approve,
                rationale: "Attempt to approve a Capability Gap.".into(),
            })
            .expect_err("Capability Gap blocks approval")
            .code,
        TenderErrorCode::InvalidCommand
    );
    let returned = blocked_harness
        .host
        .decide_work_plan_proposal(DecideWorkPlanProposalCommand {
            tender_id: blocked_harness.tender_id.clone(),
            plan_id: blocked.plan_id.clone(),
            version: blocked.version,
            decision: WorkPlanDecision::Return,
            rationale: "Return the incomplete staffing proposal for exact revision.".into(),
        })
        .expect("return blocked proposal");
    assert_eq!(
        returned.approval.as_ref().map(|approval| approval.decision),
        Some(WorkPlanDecision::Return)
    );
    assert!(returned
        .profiles
        .iter()
        .all(|binding| binding.status == AgentProfileStatus::Proposed));
    let repaired = blocked_harness
        .host
        .revise_work_plan_proposal(ReviseWorkPlanProposalCommand {
            tender_id: blocked_harness.tender_id.clone(),
            plan_id: blocked.plan_id,
            base_version: blocked.version,
            actions: vec![WorkPlanRevisionAction::AddProfile {
                archetype: "cost_estimator".into(),
                identity: "Revised Cost Estimator".into(),
            }],
        })
        .expect("a returned plan can publish an exact successor");
    assert!(repaired.capability_gaps.is_empty());
    let rejected = blocked_harness
        .host
        .decide_work_plan_proposal(DecideWorkPlanProposalCommand {
            tender_id: blocked_harness.tender_id.clone(),
            plan_id: repaired.plan_id,
            version: repaired.version,
            decision: WorkPlanDecision::Reject,
            rationale: "Reject the revised staffing proposal without activating production.".into(),
        })
        .expect("reject revised proposal");
    assert_eq!(
        rejected.approval.as_ref().map(|approval| approval.decision),
        Some(WorkPlanDecision::Reject)
    );
    assert!(rejected
        .profiles
        .iter()
        .all(|binding| binding.status == AgentProfileStatus::Proposed));
}

#[tokio::test]
async fn stale_accepted_package_dependencies_block_work_plan_activation() {
    let harness = Harness::new("record-extraction");
    let package = ready_package(&harness).await;
    harness
        .host
        .decide_bid_decision_package(approval_command(
            &harness,
            &package,
            BidDecisionApprovalDecision::Accept,
        ))
        .expect("accept exact package");
    let plan = harness
        .host
        .compose_tender_office(ComposeTenderOfficeCommand {
            tender_id: harness.tender_id.clone(),
        })
        .expect("compose exact Work Plan before a material observation");
    let current_records = inspect_all_records(&harness.host, &harness.tender_id);
    let mut evidence = current_records
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
    harness.set_agent_scenario("record-extraction-expanded");
    harness
        .host
        .run_tender_record_extraction(RunTenderRecordExtractionCommand {
            tender_id: harness.tender_id.clone(),
            evidence,
            authorities: Vec::new(),
        })
        .await
        .expect("publish a material dependency after Proceed");

    assert_eq!(
        harness
            .host
            .decide_work_plan_proposal(DecideWorkPlanProposalCommand {
                tender_id: harness.tender_id.clone(),
                plan_id: plan.plan_id,
                version: plan.version,
                decision: WorkPlanDecision::Approve,
                rationale: "Attempt to activate a plan whose package basis is stale.".into(),
            })
            .expect_err("stale accepted package must block Work Plan approval")
            .code,
        TenderErrorCode::InvalidCommand
    );
    assert!(harness
        .host
        .inspect_current_work_plan(&harness.tender_id)
        .expect("inspect denied Work Plan")
        .expect("Work Plan remains")
        .profiles
        .iter()
        .all(|binding| binding.status == AgentProfileStatus::Proposed));
}

#[tokio::test]
async fn unapproved_work_plan_can_rebase_after_material_change_reproceed() {
    let harness = Harness::new("record-extraction");
    let package = ready_package(&harness).await;
    let accepted = harness
        .host
        .decide_bid_decision_package(approval_command(
            &harness,
            &package,
            BidDecisionApprovalDecision::Accept,
        ))
        .expect("accept exact package");
    let plan = harness
        .host
        .compose_tender_office(ComposeTenderOfficeCommand {
            tender_id: harness.tender_id.clone(),
        })
        .expect("leave exact Work Plan unapproved");
    let current_records = inspect_all_records(&harness.host, &harness.tender_id);
    let mut evidence = current_records
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
    harness.set_agent_scenario("record-extraction-expanded");
    harness
        .host
        .run_tender_record_extraction(RunTenderRecordExtractionCommand {
            tender_id: harness.tender_id.clone(),
            evidence,
            authorities: Vec::new(),
        })
        .await
        .expect("publish exact material dependency");
    let invalidated = harness
        .host
        .invalidate_bid_decision_approval(InvalidateBidDecisionApprovalCommand {
            tender_id: harness.tender_id.clone(),
            approval_id: accepted.approval.approval_id,
            approval_sha256: accepted.approval.approval_sha256,
            material_change_summary: "A verified material obligation changes the planning basis."
                .into(),
            affected_areas: vec!["delivery_plan".into()],
        })
        .expect("invalidate Proceed while Work Plan remains Proposed");
    assert!(harness
        .host
        .inspect_current_work_plan(&harness.tender_id)
        .expect("inspect stale Proposed Work Plan")
        .expect("Work Plan remains inspectable")
        .profiles
        .iter()
        .all(|binding| binding.status == AgentProfileStatus::Proposed));
    let captured_successor = harness
        .host
        .create_bid_decision_package(CreateBidDecisionPackageCommand {
            tender_id: harness.tender_id.clone(),
            base_version: Some(package.version),
            disposition_updates: Vec::new(),
            manager_capability_demands: Vec::new(),
        })
        .expect("publish captured material-change successor");
    let changed = invalidated
        .invalidation
        .changed_records
        .first()
        .expect("captured changed record");
    harness
        .host
        .decide_tender_record(DecideTenderRecordCommand {
            tender_id: harness.tender_id.clone(),
            record_id: changed.record_id.clone(),
            version: changed.version,
            decision: TenderRecordEngineerDecisionKind::Verify,
            rationale: "Verify the exact changed obligation before re-Proceed.".into(),
        })
        .expect("verify captured material-change record");
    let current_records = inspect_all_records(&harness.host, &harness.tender_id);
    let successor = harness
        .host
        .create_bid_decision_package(CreateBidDecisionPackageCommand {
            tender_id: harness.tender_id.clone(),
            base_version: Some(captured_successor.version),
            disposition_updates: complete_dispositions(&current_records),
            manager_capability_demands: Vec::new(),
        })
        .expect("publish ready exact successor");
    harness.set_agent_scenario("bid-package-review");
    let successor = harness
        .host
        .run_bid_decision_package_review(RunBidDecisionPackageReviewCommand {
            tender_id: harness.tender_id.clone(),
            package_id: successor.package_id,
            version: successor.version,
        })
        .await
        .expect("review exact successor")
        .package;
    harness
        .host
        .decide_bid_decision_package(approval_command(
            &harness,
            &successor,
            BidDecisionApprovalDecision::Accept,
        ))
        .expect("re-Proceed on exact successor");

    let rebased = harness
        .host
        .revise_work_plan_proposal(ReviseWorkPlanProposalCommand {
            tender_id: harness.tender_id.clone(),
            plan_id: plan.plan_id,
            base_version: plan.version,
            actions: vec![WorkPlanRevisionAction::RebasePackageBasis],
        })
        .expect("rebase previously unapproved Work Plan");
    assert_eq!(rebased.bid_package_id, successor.package_id);
    assert_eq!(rebased.bid_package_version, successor.version);
    assert!(rebased.approval.is_none());
    assert!(rebased
        .profiles
        .iter()
        .all(|binding| binding.status == AgentProfileStatus::Proposed));
    harness
        .host
        .close_tender(&harness.tender_id)
        .expect("close before unapproved rebase integrity inspection");
    assert_eq!(
        harness
            .host
            .inspect_tender_integrity(&harness.tender_id)
            .expect("inspect unapproved rebase lineage")
            .state,
        TenderIntegrityState::Ready
    );
}

#[tokio::test]
async fn cold_open_rejects_a_tampered_work_plan_approval_and_profile_binding() {
    let harness = Harness::new("record-extraction");
    let package = ready_package(&harness).await;
    harness
        .host
        .decide_bid_decision_package(approval_command(
            &harness,
            &package,
            BidDecisionApprovalDecision::Accept,
        ))
        .expect("accept exact package");
    let plan = harness
        .host
        .compose_tender_office(ComposeTenderOfficeCommand {
            tender_id: harness.tender_id.clone(),
        })
        .expect("compose exact Work Plan");
    let approved = harness
        .host
        .decide_work_plan_proposal(DecideWorkPlanProposalCommand {
            tender_id: harness.tender_id.clone(),
            plan_id: plan.plan_id,
            version: plan.version,
            decision: WorkPlanDecision::Approve,
            rationale: "Approve the exact production team and Work Plan.".into(),
        })
        .expect("approve exact Work Plan");
    let corrupted_profile_id = approved.profiles[0].profile.profile_id.clone();
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
            "UPDATE agent_profile_heads SET status = 'retired' WHERE profile_id = ?1",
            [&corrupted_profile_id],
        )
        .expect("corrupt an approved Work Plan profile binding");

    let integrity = harness
        .host
        .inspect_tender_integrity(&harness.tender_id)
        .expect("inspect tampered Work Plan");
    assert_eq!(integrity.state, TenderIntegrityState::RecoveryRequired);
    assert!(
        integrity
            .issues
            .contains(&quantix_lib::TenderIntegrityIssue::ManifestInvalid),
        "{integrity:#?}"
    );
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

async fn ready_package(harness: &Harness) -> BidDecisionPackageInspection {
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
        .expect("create complete decision package");
    harness.set_agent_scenario("bid-package-review");
    harness
        .host
        .run_bid_decision_package_review(RunBidDecisionPackageReviewCommand {
            tender_id: harness.tender_id.clone(),
            package_id: package.package_id,
            version: package.version,
        })
        .await
        .expect("review complete decision package")
        .package
}

#[tokio::test]
async fn active_production_materializes_only_the_exact_approved_plan_and_ready_frontier() {
    let harness = Harness::new("record-extraction");
    let (approved, production) = active_production(&harness).await;

    assert_eq!(production.plan_id, approved.plan_id);
    assert_eq!(production.plan_version, approved.version);
    assert_eq!(
        harness
            .host
            .open_tender(&harness.tender_id)
            .expect("inspect Active Production lifecycle")
            .lifecycle_phase,
        TenderLifecyclePhase::ActiveProduction
    );
    assert_eq!(production.tasks.len(), approved.tasks.len());
    assert!(production
        .tasks
        .iter()
        .any(|task| task.state == ProductionTaskState::Ready));
    assert!(production
        .tasks
        .iter()
        .any(|task| task.state == ProductionTaskState::Blocked));
    assert!(production.tasks.iter().all(|task| {
        task.plan_manifest_sha256 == approved.manifest_sha256
            && task.run_ids.is_empty()
            && task.artifact_version_count == 0
    }));
    assert!(harness
        .host
        .inspect_current_work_plan(&harness.tender_id)
        .expect("inspect activated Work Plan")
        .expect("Work Plan")
        .profiles
        .iter()
        .all(|binding| binding.status == AgentProfileStatus::Active));
    let database = harness
        .application_home
        .join("tenders")
        .join(&harness.tender_id)
        .join("tender.sqlite");
    let connection = rusqlite::Connection::open(database).expect("open active Tender Store");
    let mut statement = connection
        .prepare(
            "SELECT profile_id FROM agent_profile_heads
             WHERE status = 'active' ORDER BY profile_id",
        )
        .expect("prepare active profile inventory");
    let active_profile_ids = statement
        .query_map([], |row| row.get::<_, String>(0))
        .expect("query active profile inventory")
        .collect::<Result<Vec<_>, _>>()
        .expect("read active profile inventory");
    let mut approved_profile_ids = approved
        .profiles
        .iter()
        .map(|binding| binding.profile.profile_id.clone())
        .collect::<Vec<_>>();
    approved_profile_ids.sort();
    assert_eq!(active_profile_ids, approved_profile_ids);
    drop(statement);
    drop(connection);
    harness
        .host
        .close_tender(&harness.tender_id)
        .expect("close activated Tender");
    let integrity = harness
        .host
        .inspect_tender_integrity(&harness.tender_id)
        .expect("inspect Active Production integrity");
    assert_eq!(
        integrity.state,
        TenderIntegrityState::Ready,
        "{integrity:#?}"
    );
}

#[tokio::test]
async fn intake_material_query_initializes_its_exact_global_scope_as_query_blocked() {
    let harness = Harness::new("record-extraction");
    let records = harness.extract_records().await;
    harness.verify_records(&records, false);
    assert_eq!(
        harness
            .host
            .open_tender(&harness.tender_id)
            .expect("inspect Intake Query lifecycle")
            .lifecycle_phase,
        TenderLifecyclePhase::Intake
    );
    let owner = harness
        .host
        .inspect_tender_queries(InspectTenderQueriesCommand {
            tender_id: harness.tender_id.clone(),
            cursor: None,
            limit: 8,
        })
        .expect("inspect Query Register during Intake")
        .owner_profiles
        .into_iter()
        .next()
        .expect("bootstrap Query owner");
    let affected_record = records.first().expect("exact affected Tender Record");
    let query = harness
        .host
        .create_tender_query(CreateTenderQueryCommand {
            tender_id: harness.tender_id.clone(),
            query_type: TenderQueryType::Ambiguity,
            question: "Which exact responsibility basis governs future production work?".into(),
            ambiguity_or_gap:
                "The Intake evidence preserves two plausible responsibility readings.".into(),
            owner_profile_id: owner.profile_id.clone(),
            owner_profile_version: owner.version,
            evidence: vec![AgentTaskInputReference {
                kind: "tender_record_version".into(),
                reference: affected_record.record_id.clone(),
                version: affected_record.version,
            }],
            affected_records: vec![TenderRecordVersionReference {
                record_id: affected_record.record_id.clone(),
                version: affected_record.version,
            }],
            affected_task_keys: vec!["*".into()],
            due_at: "2099-01-01T00:00:00.000Z".into(),
            material: true,
            release_blocking: true,
            proposed_treatments: vec![TenderQueryTreatmentProposalInput {
                treatment: TenderQueryTreatment::Qualification,
                rationale: "Qualify all dependent work until evidence resolves it.".into(),
            }],
        })
        .expect("register globally scoped material Query during Intake");
    assert_eq!(query.owner_profile_id, owner.profile_id);
    assert_eq!(query.affected_task_keys, vec!["*"]);

    let package = harness
        .host
        .create_bid_decision_package(CreateBidDecisionPackageCommand {
            tender_id: harness.tender_id.clone(),
            base_version: None,
            disposition_updates: complete_dispositions(&records),
            manager_capability_demands: Vec::new(),
        })
        .expect("create decision package after Intake Query");
    harness.set_agent_scenario("bid-package-review");
    let package = harness
        .host
        .run_bid_decision_package_review(RunBidDecisionPackageReviewCommand {
            tender_id: harness.tender_id.clone(),
            package_id: package.package_id,
            version: package.version,
        })
        .await
        .expect("review package after Intake Query")
        .package;
    harness
        .host
        .decide_bid_decision_package(approval_command(
            &harness,
            &package,
            BidDecisionApprovalDecision::Accept,
        ))
        .expect("accept exact package after Intake Query registration");
    let plan = harness
        .host
        .compose_tender_office(ComposeTenderOfficeCommand {
            tender_id: harness.tender_id.clone(),
        })
        .expect("compose exact preproduction Work Plan");
    let approved = harness
        .host
        .decide_work_plan_proposal(DecideWorkPlanProposalCommand {
            tender_id: harness.tender_id.clone(),
            plan_id: plan.plan_id,
            version: plan.version,
            decision: WorkPlanDecision::Approve,
            rationale: "Approve the exact plan while preserving preproduction Query control."
                .into(),
        })
        .expect("approve exact preproduction Work Plan");
    let production = harness
        .host
        .activate_tender_production(ActivateTenderProductionCommand {
            tender_id: harness.tender_id.clone(),
            plan_id: approved.plan_id,
            plan_version: approved.version,
            plan_manifest_sha256: approved.manifest_sha256,
        })
        .expect("activate exact plan under preproduction Query control");
    assert!(production
        .tasks
        .iter()
        .all(|task| task.state == ProductionTaskState::QueryBlocked));
    harness
        .host
        .close_tender(&harness.tender_id)
        .expect("close preproduction Query-controlled Tender");
    assert_eq!(
        harness
            .host
            .inspect_tender_integrity(&harness.tender_id)
            .expect("cold-verify preproduction Query activation")
            .state,
        TenderIntegrityState::Ready
    );
}

#[tokio::test]
async fn query_context_capacity_rejects_the_first_unmaterializable_global_successor() {
    let harness = Harness::new("record-extraction");
    let records = harness.extract_records().await;
    let owner = harness
        .host
        .inspect_tender_queries(InspectTenderQueriesCommand {
            tender_id: harness.tender_id.clone(),
            cursor: None,
            limit: 8,
        })
        .expect("inspect Query owner profiles")
        .owner_profiles
        .into_iter()
        .next()
        .expect("bounded Query owner");
    let evidence = AgentTaskInputReference {
        kind: "tender_record_version".into(),
        reference: records
            .first()
            .expect("exact Query evidence")
            .record_id
            .clone(),
        version: records.first().expect("exact Query evidence").version,
    };
    let mut accepted = 0usize;
    for sequence in 0..256 {
        let prefix = format!("Context {sequence:03}: ");
        let question = format!("{prefix}{}", "q".repeat(4_000 - prefix.len()));
        let gap = format!("{prefix}{}", "g".repeat(4_000 - prefix.len()));
        match harness.host.create_tender_query(CreateTenderQueryCommand {
            tender_id: harness.tender_id.clone(),
            query_type: TenderQueryType::Ambiguity,
            question,
            ambiguity_or_gap: gap,
            owner_profile_id: owner.profile_id.clone(),
            owner_profile_version: owner.version,
            evidence: vec![evidence.clone()],
            affected_records: Vec::new(),
            affected_task_keys: vec!["*".into()],
            due_at: "2099-01-01T00:00:00.000Z".into(),
            material: false,
            release_blocking: false,
            proposed_treatments: Vec::new(),
        }) {
            Ok(_) => accepted += 1,
            Err(error) => {
                assert_eq!(error.code, TenderErrorCode::InvalidCommand);
                break;
            }
        }
    }
    assert!(accepted > 1 && accepted < 256, "accepted={accepted}");
    assert_eq!(
        harness
            .host
            .inspect_tender_queries(InspectTenderQueriesCommand {
                tender_id: harness.tender_id.clone(),
                cursor: None,
                limit: 8,
            })
            .expect("inspect bounded Query context inventory")
            .total_current_count,
        u32::try_from(accepted).expect("bounded accepted Query count")
    );
    harness
        .host
        .close_tender(&harness.tender_id)
        .expect("close context-capacity Tender");
    assert_eq!(
        harness
            .host
            .inspect_tender_integrity(&harness.tender_id)
            .expect("cold-verify context-capacity boundary")
            .state,
        TenderIntegrityState::Ready
    );
}

#[tokio::test]
async fn named_material_query_blocks_its_dependency_closure_without_touching_an_unrelated_branch() {
    let harness = Harness::new("record-extraction");
    let (_, production) = active_production(&harness).await;

    let (target, affected_task_keys) = production
        .tasks
        .iter()
        .find_map(|candidate| {
            let mut closure = std::collections::HashSet::from([candidate.task.task_key.clone()]);
            loop {
                let before = closure.len();
                for task in &production.tasks {
                    if task
                        .task
                        .dependencies
                        .iter()
                        .any(|dependency| closure.contains(dependency))
                    {
                        closure.insert(task.task.task_key.clone());
                    }
                }
                if closure.len() == before {
                    break;
                }
            }
            (closure.len() > 1 && closure.len() < production.tasks.len())
                .then_some((candidate.clone(), closure))
        })
        .expect("named prerequisite with downstream work and an unrelated branch");
    let original_states = production
        .tasks
        .iter()
        .map(|task| (task.task.task_key.clone(), task.state))
        .collect::<std::collections::HashMap<_, _>>();

    let query = harness
        .host
        .create_tender_query(CreateTenderQueryCommand {
            tender_id: harness.tender_id.clone(),
            query_type: TenderQueryType::ResponsibilitySensitive,
            question: "Who owns the named prerequisite and its dependent production work?".into(),
            ambiguity_or_gap:
                "The exact responsibility basis is unresolved for this dependency branch.".into(),
            owner_profile_id: target.task.profile_id.clone(),
            owner_profile_version: target.task.profile_version,
            evidence: vec![target
                .task
                .exact_inputs
                .first()
                .expect("named task exact input")
                .clone()],
            affected_records: Vec::new(),
            affected_task_keys: vec![target.task.task_key.clone()],
            due_at: "2099-01-01T00:00:00.000Z".into(),
            material: true,
            release_blocking: true,
            proposed_treatments: vec![TenderQueryTreatmentProposalInput {
                treatment: TenderQueryTreatment::Qualification,
                rationale: "Qualify the exact dependency branch before production continues."
                    .into(),
            }],
        })
        .expect("register named material Query");

    let inspected = harness
        .host
        .inspect_tender_production(&harness.tender_id)
        .expect("inspect targeted dependency invalidation")
        .expect("active production");
    for task in &inspected.tasks {
        if affected_task_keys.contains(&task.task.task_key) {
            assert_eq!(
                task.state,
                ProductionTaskState::QueryBlocked,
                "{} must be blocked by the named dependency Query",
                task.task.task_key
            );
        } else {
            assert_eq!(
                Some(&task.state),
                original_states.get(&task.task.task_key),
                "{} is outside the dependency closure",
                task.task.task_key
            );
        }
    }
    let invalidated_task_ids = query
        .invalidations
        .iter()
        .filter(|invalidation| invalidation.target_kind == "production_task")
        .map(|invalidation| invalidation.target_id.as_str())
        .collect::<std::collections::HashSet<_>>();
    let expected_task_ids = inspected
        .tasks
        .iter()
        .filter(|task| affected_task_keys.contains(&task.task.task_key))
        .map(|task| task.production_task_id.as_str())
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(invalidated_task_ids, expected_task_ids);
    assert!(inspected.tasks.iter().any(|task| {
        !affected_task_keys.contains(&task.task.task_key)
            && task.state != ProductionTaskState::QueryBlocked
    }));

    harness
        .host
        .close_tender(&harness.tender_id)
        .expect("close dependency-controlled Tender");
    assert_eq!(
        harness
            .host
            .inspect_tender_integrity(&harness.tender_id)
            .expect("cold-verify targeted dependency invalidation")
            .state,
        TenderIntegrityState::Ready
    );
}

#[tokio::test]
async fn nonblocking_query_remediation_and_recursive_review_survive_cold_open() {
    let harness = Harness::new("record-extraction");
    let (_, production) = active_production(&harness).await;
    let target = production
        .tasks
        .iter()
        .find(|candidate| {
            candidate.state == ProductionTaskState::Ready
                && production
                    .tasks
                    .iter()
                    .any(|task| task.task.dependencies.contains(&candidate.task.task_key))
        })
        .expect("ready prerequisite with downstream work")
        .clone();

    harness.set_agent_scenario("production-task");
    let authored = harness
        .host
        .run_production_task(RunProductionTaskCommand {
            tender_id: harness.tender_id.clone(),
            production_task_id: target.production_task_id.clone(),
        })
        .await
        .expect("author prerequisite Artifact before nonblocking Query");
    assert_eq!(authored.task.state, ProductionTaskState::ReviewReady);

    let query = harness
        .host
        .create_tender_query(CreateTenderQueryCommand {
            tender_id: harness.tender_id.clone(),
            query_type: TenderQueryType::Ambiguity,
            question: "Which additional evidence qualifies this prerequisite output?".into(),
            ambiguity_or_gap:
                "The new observation is nonblocking but makes the current Artifact stale.".into(),
            owner_profile_id: target.task.profile_id.clone(),
            owner_profile_version: target.task.profile_version,
            evidence: vec![target
                .task
                .exact_inputs
                .first()
                .expect("prerequisite exact input")
                .clone()],
            affected_records: Vec::new(),
            affected_task_keys: vec![target.task.task_key.clone()],
            due_at: "2099-01-01T00:00:00.000Z".into(),
            material: false,
            release_blocking: false,
            proposed_treatments: vec![TenderQueryTreatmentProposalInput {
                treatment: TenderQueryTreatment::Qualification,
                rationale: "Carry the explicit nonblocking qualification into the successor."
                    .into(),
            }],
        })
        .expect("register nonblocking Query against authored work");
    assert!(query.approved_treatment.is_none());
    let stale = harness
        .host
        .inspect_tender_production(&harness.tender_id)
        .expect("inspect nonblocking Query staleness")
        .expect("active production")
        .tasks
        .into_iter()
        .find(|task| task.production_task_id == target.production_task_id)
        .expect("stale prerequisite task");
    assert_eq!(stale.state, ProductionTaskState::RemediationReady);

    let remediated_result = harness
        .host
        .run_production_task(RunProductionTaskCommand {
            tender_id: harness.tender_id.clone(),
            production_task_id: target.production_task_id.clone(),
        })
        .await;
    let mut remediated = remediated_result.unwrap_or_else(|error| {
        panic!(
            "remediate nonblocking Query without a Manager treatment: {error:?}; production={:#?}; fixture={}",
            harness
                .host
                .inspect_tender_production(&harness.tender_id),
            fs::read_to_string(harness.codex.with_extension("fixture-error"))
                .unwrap_or_else(|_| "none".into())
        )
    });
    assert!(remediated.run.task.exact_inputs.iter().any(|input| {
        input.kind == "tender_query_version"
            && input.reference == query.query_id
            && input.version == query.version
    }));
    assert_eq!(
        remediated.run.state,
        AgentRunState::Completed,
        "{:#?}",
        remediated.run
    );
    assert_eq!(
        remediated.task.state,
        ProductionTaskState::ReviewReady,
        "{:#?}",
        remediated.run
    );
    if remediated.task.state == ProductionTaskState::ReviewReady {
        remediated = harness
            .host
            .run_production_task(RunProductionTaskCommand {
                tender_id: harness.tender_id.clone(),
                production_task_id: target.production_task_id.clone(),
            })
            .await
            .expect("review nonblocking Query remediation");
    }
    assert_eq!(
        remediated.task.state,
        ProductionTaskState::ReadyForIntegration,
        "{:#?}",
        remediated.run
    );

    let production = harness
        .host
        .inspect_tender_production(&harness.tender_id)
        .expect("inspect released downstream frontier")
        .expect("active production");
    let downstream = production
        .tasks
        .iter()
        .find(|task| {
            task.state == ProductionTaskState::Ready
                && task.task.dependencies.contains(&target.task.task_key)
        })
        .expect("ready downstream task reached by Query invalidation")
        .clone();
    let mut completed = harness
        .host
        .run_production_task(RunProductionTaskCommand {
            tender_id: harness.tender_id.clone(),
            production_task_id: downstream.production_task_id.clone(),
        })
        .await
        .expect("author downstream Artifact with recursive Query context");
    assert!(completed.run.task.exact_inputs.iter().any(|input| {
        input.kind == "tender_query_version"
            && input.reference == query.query_id
            && input.version == query.version
    }));
    if completed.task.state == ProductionTaskState::ReviewReady {
        completed = harness
            .host
            .run_production_task(RunProductionTaskCommand {
                tender_id: harness.tender_id.clone(),
                production_task_id: downstream.production_task_id.clone(),
            })
            .await
            .expect("independently review recursive Query-bearing Artifact");
    }
    assert_eq!(
        completed.task.state,
        ProductionTaskState::ReadyForIntegration
    );

    harness
        .host
        .close_tender(&harness.tender_id)
        .expect("close recursive Query review Tender");
    let integrity = harness
        .host
        .inspect_tender_integrity(&harness.tender_id)
        .expect("cold-verify nonblocking Query remediation and downstream review");
    assert_eq!(
        integrity.state,
        TenderIntegrityState::Ready,
        "{integrity:#?}"
    );
}

#[tokio::test]
async fn stale_query_control_completion_terminalizes_without_stranding_the_task() {
    let harness = Harness::new("record-extraction");
    let (_, production) = active_production(&harness).await;
    let target = production
        .tasks
        .iter()
        .find(|task| task.state == ProductionTaskState::Ready)
        .expect("ready Query owner task")
        .clone();
    let moved_task_key = production
        .tasks
        .iter()
        .find(|candidate| {
            candidate.task.task_key != target.task.task_key
                && !production.tasks.iter().any(|dependent| {
                    dependent
                        .task
                        .dependencies
                        .contains(&candidate.task.task_key)
                })
        })
        .expect("separate Query successor scope")
        .task
        .task_key
        .clone();
    let query = harness
        .host
        .create_tender_query(CreateTenderQueryCommand {
            tender_id: harness.tender_id.clone(),
            query_type: TenderQueryType::MissingInformation,
            question: "Which exact missing fact controls this work?".into(),
            ambiguity_or_gap:
                "The task cannot proceed without an attributable specialist response.".into(),
            owner_profile_id: target.task.profile_id.clone(),
            owner_profile_version: target.task.profile_version,
            evidence: vec![target
                .task
                .exact_inputs
                .first()
                .expect("owner task exact input")
                .clone()],
            affected_records: Vec::new(),
            affected_task_keys: vec![target.task.task_key.clone()],
            due_at: "2099-01-01T00:00:00.000Z".into(),
            material: true,
            release_blocking: true,
            proposed_treatments: vec![TenderQueryTreatmentProposalInput {
                treatment: TenderQueryTreatment::Qualification,
                rationale: "Request a bounded specialist qualification.".into(),
            }],
        })
        .expect("register exact Query-control basis");

    harness.set_agent_scenario("production-task-delayed-a");
    let host = harness.host.clone();
    let tender_id = harness.tender_id.clone();
    let production_task_id = target.production_task_id.clone();
    let control = tokio::spawn(async move {
        host.run_production_task(RunProductionTaskCommand {
            tender_id,
            production_task_id,
        })
        .await
    });
    wait_for_fixture_path(&harness.codex.with_extension("production-a-waiting")).await;
    let successor = harness
        .host
        .revise_tender_query(ReviseTenderQueryCommand {
            tender_id: harness.tender_id.clone(),
            query_id: query.query_id.clone(),
            base_version: query.version,
            query_type: query.query_type,
            question: query.question.clone(),
            ambiguity_or_gap:
                "The Engineer registered a successor while the specialist turn was in flight."
                    .into(),
            owner_profile_id: query.owner_profile_id.clone(),
            owner_profile_version: query.owner_profile_version,
            evidence: query.evidence.clone(),
            affected_records: query.affected_records.clone(),
            affected_task_keys: vec![moved_task_key],
            due_at: query.due_at.clone(),
            material: query.material,
            release_blocking: query.release_blocking,
            proposed_treatments: query
                .proposed_treatments
                .iter()
                .map(|proposal| TenderQueryTreatmentProposalInput {
                    treatment: proposal.treatment,
                    rationale: proposal.rationale.clone(),
                })
                .collect(),
            response: None,
            response_evidence: Vec::new(),
        })
        .expect("publish concurrent exact Query successor");
    assert_eq!(successor.version, query.version + 1);
    fs::write(
        harness.codex.with_extension("production-a-release"),
        b"release",
    )
    .expect("release stale Query-control output");
    let completed = control
        .await
        .expect("join stale Query-control turn")
        .expect("terminalize stale Query-control turn");
    assert_eq!(completed.run.state, AgentRunState::Failed);
    assert_eq!(
        completed.run.failure.map(|failure| failure.category),
        Some(ProviderFailureCategory::OutputInvalid)
    );
    assert_eq!(completed.task.state, ProductionTaskState::Ready);
    assert!(harness
        .host
        .inspect_tender_production(&harness.tender_id)
        .expect("inspect stale Query-control terminality")
        .expect("active production")
        .tasks
        .iter()
        .all(|task| task.state != ProductionTaskState::Running));

    harness
        .host
        .close_tender(&harness.tender_id)
        .expect("close stale Query-control Tender");
    assert_eq!(
        harness
            .host
            .inspect_tender_integrity(&harness.tender_id)
            .expect("cold-verify stale Query-control terminality")
            .state,
        TenderIntegrityState::Ready
    );
}

#[tokio::test]
async fn artifact_backed_stale_query_control_resumes_exact_remediation_after_scope_removal() {
    let harness = Harness::new("record-extraction");
    let (_, production) = active_production(&harness).await;
    let target = production
        .tasks
        .iter()
        .find(|task| task.state == ProductionTaskState::Ready)
        .expect("ready artifact-backed Query owner task")
        .clone();
    let moved_task_key = production
        .tasks
        .iter()
        .find(|candidate| {
            candidate.task.task_key != target.task.task_key
                && !production.tasks.iter().any(|dependent| {
                    dependent
                        .task
                        .dependencies
                        .contains(&candidate.task.task_key)
                })
        })
        .expect("separate artifact-backed Query successor scope")
        .task
        .task_key
        .clone();
    harness.set_agent_scenario("production-task");
    let authored = harness
        .host
        .run_production_task(RunProductionTaskCommand {
            tender_id: harness.tender_id.clone(),
            production_task_id: target.production_task_id.clone(),
        })
        .await
        .expect("author Artifact before Query-control race");
    assert_eq!(authored.task.state, ProductionTaskState::ReviewReady);

    let query = harness
        .host
        .create_tender_query(CreateTenderQueryCommand {
            tender_id: harness.tender_id.clone(),
            query_type: TenderQueryType::Ambiguity,
            question: "Which exact new observation applies to the authored Artifact?".into(),
            ambiguity_or_gap: "The current Artifact predates an attributable Query observation."
                .into(),
            owner_profile_id: target.task.profile_id.clone(),
            owner_profile_version: target.task.profile_version,
            evidence: vec![target
                .task
                .exact_inputs
                .first()
                .expect("artifact-backed Query evidence")
                .clone()],
            affected_records: Vec::new(),
            affected_task_keys: vec![target.task.task_key.clone()],
            due_at: "2099-01-01T00:00:00.000Z".into(),
            material: true,
            release_blocking: true,
            proposed_treatments: vec![TenderQueryTreatmentProposalInput {
                treatment: TenderQueryTreatment::Qualification,
                rationale: "Request exact specialist input before remediating the Artifact.".into(),
            }],
        })
        .expect("register artifact-backed Query-control basis");
    harness.set_agent_scenario("production-task-delayed-a");
    let host = harness.host.clone();
    let tender_id = harness.tender_id.clone();
    let production_task_id = target.production_task_id.clone();
    let control = tokio::spawn(async move {
        host.run_production_task(RunProductionTaskCommand {
            tender_id,
            production_task_id,
        })
        .await
    });
    wait_for_fixture_path(&harness.codex.with_extension("production-a-waiting")).await;
    harness
        .host
        .revise_tender_query(ReviseTenderQueryCommand {
            tender_id: harness.tender_id.clone(),
            query_id: query.query_id.clone(),
            base_version: query.version,
            query_type: query.query_type,
            question: query.question.clone(),
            ambiguity_or_gap: "The exact successor now targets a separate leaf workstream.".into(),
            owner_profile_id: query.owner_profile_id.clone(),
            owner_profile_version: query.owner_profile_version,
            evidence: query.evidence.clone(),
            affected_records: Vec::new(),
            affected_task_keys: vec![moved_task_key],
            due_at: query.due_at.clone(),
            material: query.material,
            release_blocking: query.release_blocking,
            proposed_treatments: query
                .proposed_treatments
                .iter()
                .map(|proposal| TenderQueryTreatmentProposalInput {
                    treatment: proposal.treatment,
                    rationale: proposal.rationale.clone(),
                })
                .collect(),
            response: None,
            response_evidence: Vec::new(),
        })
        .expect("move artifact-backed Query scope during specialist turn");
    fs::write(
        harness.codex.with_extension("production-a-release"),
        b"release",
    )
    .expect("release stale artifact-backed Query-control output");
    let stale = control
        .await
        .expect("join artifact-backed Query-control turn")
        .expect("terminalize artifact-backed stale Query-control turn");
    assert_eq!(stale.run.state, AgentRunState::Failed);
    assert_eq!(stale.task.state, ProductionTaskState::RemediationReady);

    harness.set_agent_scenario("production-task");
    let mut remediated = harness
        .host
        .run_production_task(RunProductionTaskCommand {
            tender_id: harness.tender_id.clone(),
            production_task_id: target.production_task_id.clone(),
        })
        .await
        .expect("resume exact Query-driven Artifact remediation");
    assert!(remediated.run.task.exact_inputs.iter().any(|input| {
        input.kind == "tender_query_version"
            && input.reference == query.query_id
            && input.version == query.version
    }));
    if remediated.task.state == ProductionTaskState::ReviewReady {
        remediated = harness
            .host
            .run_production_task(RunProductionTaskCommand {
                tender_id: harness.tender_id.clone(),
                production_task_id: target.production_task_id.clone(),
            })
            .await
            .expect("review resumed exact Query remediation");
    }
    assert_eq!(
        remediated.task.state,
        ProductionTaskState::ReadyForIntegration
    );
    harness
        .host
        .close_tender(&harness.tender_id)
        .expect("close artifact-backed Query race Tender");
    let integrity = harness
        .host
        .inspect_tender_integrity(&harness.tender_id)
        .expect("cold-verify artifact-backed Query-control recovery");
    assert_eq!(
        integrity.state,
        TenderIntegrityState::Ready,
        "{integrity:#?}"
    );
}

#[tokio::test]
async fn removed_query_scope_preserves_the_exact_historical_treatment_for_remediation() {
    let harness = Harness::new("record-extraction");
    let (_, production) = active_production(&harness).await;
    let target = production
        .tasks
        .iter()
        .find(|task| task.state == ProductionTaskState::Ready)
        .expect("ready task for historical treatment")
        .clone();
    let unrelated_task_key = production
        .tasks
        .iter()
        .find(|candidate| {
            candidate.task.task_key != target.task.task_key
                && !production.tasks.iter().any(|dependent| {
                    dependent
                        .task
                        .dependencies
                        .contains(&candidate.task.task_key)
                })
        })
        .expect("unrelated leaf task")
        .task
        .task_key
        .clone();

    harness.set_agent_scenario("production-task");
    let mut initial = harness
        .host
        .run_production_task(RunProductionTaskCommand {
            tender_id: harness.tender_id.clone(),
            production_task_id: target.production_task_id.clone(),
        })
        .await
        .expect("author pre-Query Artifact");
    if initial.task.state == ProductionTaskState::ReviewReady {
        initial = harness
            .host
            .run_production_task(RunProductionTaskCommand {
                tender_id: harness.tender_id.clone(),
                production_task_id: target.production_task_id.clone(),
            })
            .await
            .expect("review pre-Query Artifact");
    }
    assert_eq!(initial.task.state, ProductionTaskState::ReadyForIntegration);

    let query = harness
        .host
        .create_tender_query(CreateTenderQueryCommand {
            tender_id: harness.tender_id.clone(),
            query_type: TenderQueryType::ResponsibilitySensitive,
            question: "Which exact responsibility assumption qualifies the current Artifact?"
                .into(),
            ambiguity_or_gap: "The current Artifact predates the required responsibility basis."
                .into(),
            owner_profile_id: target.task.profile_id.clone(),
            owner_profile_version: target.task.profile_version,
            evidence: vec![target
                .task
                .exact_inputs
                .first()
                .expect("historical treatment exact input")
                .clone()],
            affected_records: Vec::new(),
            affected_task_keys: vec![target.task.task_key.clone()],
            due_at: "2099-01-01T00:00:00.000Z".into(),
            material: true,
            release_blocking: true,
            proposed_treatments: vec![TenderQueryTreatmentProposalInput {
                treatment: TenderQueryTreatment::ApprovedAssumption,
                rationale: "Apply the bounded responsibility assumption to the successor.".into(),
            }],
        })
        .expect("invalidate existing Artifact with exact material Query");
    let decided = harness
        .host
        .decide_tender_query_treatment(DecideTenderQueryTreatmentCommand {
            tender_id: harness.tender_id.clone(),
            query_id: query.query_id.clone(),
            query_version: query.version,
            treatment: TenderQueryTreatment::ApprovedAssumption,
            rationale: "The Manager approves the exact bounded historical assumption.".into(),
            treatment_details: "Carry the assumption into the successor Artifact and review."
                .into(),
            closes_query: false,
        })
        .expect("approve exact historical treatment");
    let decision = decided
        .approved_treatment
        .as_ref()
        .expect("historical approved treatment")
        .clone();
    let successor = harness
        .host
        .revise_tender_query(ReviseTenderQueryCommand {
            tender_id: harness.tender_id.clone(),
            query_id: decided.query_id.clone(),
            base_version: decided.version,
            query_type: decided.query_type,
            question: decided.question.clone(),
            ambiguity_or_gap:
                "The current Query scope moved to a separate unrelated leaf workstream.".into(),
            owner_profile_id: decided.owner_profile_id.clone(),
            owner_profile_version: decided.owner_profile_version,
            evidence: decided.evidence.clone(),
            affected_records: Vec::new(),
            affected_task_keys: vec![unrelated_task_key],
            due_at: decided.due_at.clone(),
            material: true,
            release_blocking: true,
            proposed_treatments: decided
                .proposed_treatments
                .iter()
                .map(|proposal| TenderQueryTreatmentProposalInput {
                    treatment: proposal.treatment,
                    rationale: proposal.rationale.clone(),
                })
                .collect(),
            response: None,
            response_evidence: Vec::new(),
        })
        .expect("remove original task from current Query scope");
    assert_eq!(successor.version, decided.version + 1);
    let stale = harness
        .host
        .inspect_tender_production(&harness.tender_id)
        .expect("inspect historically invalidated task")
        .expect("active production")
        .tasks
        .into_iter()
        .find(|task| task.production_task_id == target.production_task_id)
        .expect("historically invalidated task");
    assert_eq!(stale.state, ProductionTaskState::RemediationReady);

    let mut remediated = harness
        .host
        .run_production_task(RunProductionTaskCommand {
            tender_id: harness.tender_id.clone(),
            production_task_id: target.production_task_id.clone(),
        })
        .await
        .expect("apply historical treatment after scope removal");
    assert!(remediated.run.task.exact_inputs.iter().any(|input| {
        input.kind == "approved_query_treatment"
            && input.reference == decision.decision_id
            && input.version == decision.query_version
    }));
    if remediated.task.state == ProductionTaskState::ReviewReady {
        remediated = harness
            .host
            .run_production_task(RunProductionTaskCommand {
                tender_id: harness.tender_id.clone(),
                production_task_id: target.production_task_id.clone(),
            })
            .await
            .expect("review historical treatment-bearing successor");
    }
    assert_eq!(
        remediated.task.state,
        ProductionTaskState::ReadyForIntegration
    );
    harness
        .host
        .close_tender(&harness.tender_id)
        .expect("close historical treatment Tender");
    let integrity = harness
        .host
        .inspect_tender_integrity(&harness.tender_id)
        .expect("cold-verify historical treatment-bearing successor");
    assert_eq!(
        integrity.state,
        TenderIntegrityState::Ready,
        "{integrity:#?}"
    );
}

#[tokio::test]
async fn coordinator_runs_only_ready_tasks_and_handoffs_registered_outputs() {
    let harness = Harness::new("record-extraction");
    let (_, mut production) = active_production(&harness).await;
    let target_key = production
        .tasks
        .iter()
        .find(|task| !task.task.dependencies.is_empty())
        .expect("dependent production task")
        .task
        .task_key
        .clone();
    assert_eq!(
        harness
            .host
            .run_production_task(RunProductionTaskCommand {
                tender_id: harness.tender_id.clone(),
                production_task_id: production
                    .tasks
                    .iter()
                    .find(|task| task.task.task_key == target_key)
                    .expect("target")
                    .production_task_id
                    .clone(),
            })
            .await
            .expect_err("blocked dependency cannot be scheduled")
            .code,
        TenderErrorCode::InvalidCommand
    );

    harness.set_agent_scenario("production-task");
    while production
        .tasks
        .iter()
        .find(|task| task.task.task_key == target_key)
        .expect("target")
        .state
        != ProductionTaskState::Ready
    {
        let ready = production
            .tasks
            .iter()
            .find(|task| task.state == ProductionTaskState::Ready)
            .expect("ready dependency frontier");
        let mut completed = harness
            .host
            .run_production_task(RunProductionTaskCommand {
                tender_id: harness.tender_id.clone(),
                production_task_id: ready.production_task_id.clone(),
            })
            .await
            .expect("run ready dependency");
        if completed.task.state == ProductionTaskState::ReviewReady {
            completed = harness
                .host
                .run_production_task(RunProductionTaskCommand {
                    tender_id: harness.tender_id.clone(),
                    production_task_id: ready.production_task_id.clone(),
                })
                .await
                .expect("independently review ready dependency output");
        }
        assert_eq!(
            completed.run.state,
            AgentRunState::Completed,
            "{:#?}; fixture={}",
            completed.run,
            fs::read_to_string(harness.codex.with_extension("fixture-error"))
                .unwrap_or_else(|_| "none".into())
        );
        assert_eq!(
            completed.task.state,
            ProductionTaskState::ReadyForIntegration
        );
        assert_eq!(completed.task.artifact_version_count, 1);
        production = harness
            .host
            .inspect_tender_production(&harness.tender_id)
            .expect("inspect production frontier")
            .expect("active production");
    }

    let target = production
        .tasks
        .iter()
        .find(|task| task.task.task_key == target_key)
        .expect("ready target");
    let mut completed = harness
        .host
        .run_production_task(RunProductionTaskCommand {
            tender_id: harness.tender_id.clone(),
            production_task_id: target.production_task_id.clone(),
        })
        .await
        .expect("run exact ready target");
    let author_exact_inputs = completed.run.task.exact_inputs.clone();
    if completed.task.state == ProductionTaskState::ReviewReady {
        completed = harness
            .host
            .run_production_task(RunProductionTaskCommand {
                tender_id: harness.tender_id.clone(),
                production_task_id: target.production_task_id.clone(),
            })
            .await
            .expect("independently review exact ready target output");
    }
    assert_eq!(
        completed.task.state,
        ProductionTaskState::ReadyForIntegration
    );
    assert!(author_exact_inputs.iter().any(|input| {
        input.kind == "production_artifact_version" && !target.task.dependencies.is_empty()
    }));
}

#[tokio::test]
async fn coordinator_automatically_schedules_author_and_independent_review_turns() {
    let harness = Harness::new("record-extraction");
    let (_, production) = active_production(&harness).await;
    assert!(production
        .tasks
        .iter()
        .any(|task| task.state == ProductionTaskState::Ready));

    harness.set_agent_scenario("production-task-multiplex-auto");
    harness
        .host
        .schedule_tender_production(&harness.tender_id)
        .await
        .expect("Coordinator schedules the exact ready frontier");

    let completed = harness
        .host
        .inspect_tender_production(&harness.tender_id)
        .expect("inspect automatically scheduled production")
        .expect("active production");
    assert!(
        harness
            .codex
            .with_extension("production-multiplex-observed")
            .is_file(),
        "the Coordinator used the bounded two-role frontier when capacity permitted"
    );
    assert!(
        completed
            .tasks
            .iter()
            .all(|task| task.state == ProductionTaskState::ReadyForIntegration),
        "{completed:#?}"
    );
    assert!(completed.tasks.iter().all(|task| {
        task.artifact_version_count == 1
            && task
                .task
                .review_profile_id
                .as_ref()
                .map_or_else(|| task.run_ids.len() == 1, |_| task.run_ids.len() == 2)
    }));
    harness
        .host
        .close_tender(&harness.tender_id)
        .expect("close fully completed production");
    let integrity = harness
        .host
        .inspect_tender_integrity(&harness.tender_id)
        .expect("cold-verify fully completed production");
    assert_eq!(
        integrity.state,
        TenderIntegrityState::Ready,
        "{integrity:#?}"
    );
}

#[tokio::test]
async fn critical_finding_requires_immutable_author_remediation_and_a_new_exact_review() {
    let harness = Harness::new("record-extraction");
    let (_, production) = active_production(&harness).await;
    let task = production
        .tasks
        .iter()
        .find(|task| {
            task.state == ProductionTaskState::Ready
                && task.task.review_profile_id.is_some()
                && task.task.major_finding_policy == MajorFindingPolicy::EngineerExceptionAllowed
        })
        .expect("exception-permitted review-bearing task");
    harness.set_agent_scenario("production-task-review-critical");

    let authored = harness
        .host
        .run_production_task(RunProductionTaskCommand {
            tender_id: harness.tender_id.clone(),
            production_task_id: task.production_task_id.clone(),
        })
        .await
        .expect("author exact production candidate");
    assert_eq!(authored.task.state, ProductionTaskState::ReviewReady);
    let reviewed = harness
        .host
        .run_production_task(RunProductionTaskCommand {
            tender_id: harness.tender_id.clone(),
            production_task_id: task.production_task_id.clone(),
        })
        .await
        .expect("publish the attributable critical review finding");

    assert_eq!(
        reviewed.run.state,
        AgentRunState::Completed,
        "{reviewed:#?}; fixture={}",
        fs::read_to_string(harness.codex.with_extension("fixture-error"))
            .unwrap_or_else(|_| "none".into())
    );
    assert_eq!(reviewed.task.state, ProductionTaskState::RemediationReady);
    assert_eq!(reviewed.task.run_ids.len(), 2);
    assert_eq!(reviewed.task.artifact_version_count, 1);
    assert_eq!(reviewed.task.review_count, 1);
    assert_eq!(reviewed.task.open_blocking_finding_count, 1);
    let first = harness
        .host
        .inspect_production_task_review(InspectProductionTaskReviewCommand {
            tender_id: harness.tender_id.clone(),
            production_task_id: task.production_task_id.clone(),
        })
        .expect("inspect exact critical review");
    assert_eq!(first.artifact_versions.len(), 1);
    assert_eq!(first.reviews.len(), 1);
    assert_eq!(first.reviews[0].target_version, 1);
    assert_eq!(first.reviews[0].findings.len(), 1);
    assert_eq!(
        first.reviews[0].findings[0].severity,
        ProductionFindingSeverity::Critical
    );
    assert!(first.reviews[0].findings[0].disposition.is_none());
    assert!(first.readiness.is_none());
    assert_ne!(
        authored.run.profile.profile_id,
        reviewed.run.profile.profile_id
    );
    assert_ne!(
        authored.run.provider_thread_ref,
        reviewed.run.provider_thread_ref
    );
    assert_ne!(
        authored.run.permission_grant.workspace.workspace_id,
        reviewed.run.permission_grant.workspace.workspace_id
    );
    let audit_before_denial = harness
        .host
        .open_tender(&harness.tender_id)
        .expect("inspect audit before nonwaivable denial")
        .audit_event_count;
    assert_eq!(
        harness
            .host
            .approve_production_finding_exception(ApproveProductionFindingExceptionCommand {
                tender_id: harness.tender_id.clone(),
                production_task_id: task.production_task_id.clone(),
                finding_id: first.reviews[0].findings[0].finding_id.clone(),
                review_id: first.reviews[0].review_id.clone(),
                artifact_id: first.artifact_versions[0].summary.artifact_id.clone(),
                artifact_version: 1,
                payload_sha256: first.artifact_versions[0].summary.payload_sha256.clone(),
                rationale: "A Critical finding must never be waived.".into(),
                consequence: "The unsafe candidate would reach integration.".into(),
            })
            .expect_err("Critical finding is nonwaivable")
            .code,
        TenderErrorCode::InvalidCommand
    );
    assert_eq!(
        harness
            .host
            .open_tender(&harness.tender_id)
            .expect("inspect audited nonwaivable denial")
            .audit_event_count,
        audit_before_denial + 1
    );

    harness.set_agent_scenario("production-task");
    let remediated = harness
        .host
        .run_production_task(RunProductionTaskCommand {
            tender_id: harness.tender_id.clone(),
            production_task_id: task.production_task_id.clone(),
        })
        .await
        .expect("author a new immutable remediation Artifact Version");
    assert_eq!(remediated.run.state, AgentRunState::Completed);
    assert_eq!(remediated.task.state, ProductionTaskState::ReviewReady);
    assert_eq!(remediated.task.artifact_version_count, 2);
    assert!(remediated.run.retry_of_run_id.is_none());
    let integrated = harness
        .host
        .run_production_task(RunProductionTaskCommand {
            tender_id: harness.tender_id.clone(),
            production_task_id: task.production_task_id.clone(),
        })
        .await
        .expect("independently review the new exact Artifact Version");
    assert_eq!(
        integrated.task.state,
        ProductionTaskState::ReadyForIntegration
    );
    let detail = harness
        .host
        .inspect_production_task_review(InspectProductionTaskReviewCommand {
            tender_id: harness.tender_id.clone(),
            production_task_id: task.production_task_id.clone(),
        })
        .expect("inspect immutable remediation and review lineage");
    assert_eq!(detail.artifact_versions.len(), 2);
    assert_eq!(detail.artifact_versions[1].summary.prior_version, Some(1));
    assert_eq!(
        detail.artifact_versions[1]
            .summary
            .remediation_review_id
            .as_deref(),
        Some(first.reviews[0].review_id.as_str())
    );
    assert_eq!(detail.reviews.len(), 2);
    assert_eq!(detail.reviews[1].target_version, 2);
    assert_eq!(
        detail.reviews[0].findings[0]
            .disposition
            .as_ref()
            .expect("verified remediation disposition")
            .kind,
        ProductionFindingDispositionKind::RemediationVerified
    );
    assert_eq!(
        detail
            .readiness
            .as_ref()
            .expect("exact integration readiness")
            .artifact_version,
        2
    );
    harness
        .host
        .close_tender(&harness.tender_id)
        .expect("close remediated production");
    let integrity = harness
        .host
        .inspect_tender_integrity(&harness.tender_id)
        .expect("cold-verify immutable review and remediation lineage");
    assert_eq!(
        integrity.state,
        TenderIntegrityState::Ready,
        "{integrity:#?}"
    );
}

#[tokio::test]
async fn major_finding_requires_an_exact_attributable_exception_before_integration() {
    let harness = Harness::new("record-extraction");
    let (_, production) = active_production(&harness).await;
    let task = production
        .tasks
        .iter()
        .find(|task| {
            task.state == ProductionTaskState::Ready
                && task.task.review_profile_id.is_some()
                && task.task.major_finding_policy == MajorFindingPolicy::EngineerExceptionAllowed
        })
        .expect("exception-permitted review-bearing task");
    harness.set_agent_scenario("production-task-review-major");
    harness
        .host
        .run_production_task(RunProductionTaskCommand {
            tender_id: harness.tender_id.clone(),
            production_task_id: task.production_task_id.clone(),
        })
        .await
        .expect("author exact candidate");
    let reviewed = harness
        .host
        .run_production_task(RunProductionTaskCommand {
            tender_id: harness.tender_id.clone(),
            production_task_id: task.production_task_id.clone(),
        })
        .await
        .expect("publish exact Major finding");
    assert_eq!(reviewed.task.state, ProductionTaskState::RemediationReady);
    let detail = harness
        .host
        .inspect_production_task_review(InspectProductionTaskReviewCommand {
            tender_id: harness.tender_id.clone(),
            production_task_id: task.production_task_id.clone(),
        })
        .expect("inspect Major finding");
    let finding = &detail.reviews[0].findings[0];
    assert_eq!(finding.severity, ProductionFindingSeverity::Major);
    let accepted = harness
        .host
        .approve_production_finding_exception(ApproveProductionFindingExceptionCommand {
            tender_id: harness.tender_id.clone(),
            production_task_id: task.production_task_id.clone(),
            finding_id: finding.finding_id.clone(),
            review_id: detail.reviews[0].review_id.clone(),
            artifact_id: detail.artifact_versions[0].summary.artifact_id.clone(),
            artifact_version: 1,
            payload_sha256: detail.artifact_versions[0].summary.payload_sha256.clone(),
            rationale: "The exact Major consequence is accepted under production review policy."
                .into(),
            consequence: "Integration proceeds with the disclosed Major limitation.".into(),
        })
        .expect("approve exact policy-permitted Major exception");
    assert!(accepted.readiness.is_some());
    assert_eq!(
        accepted.reviews[0].findings[0]
            .disposition
            .as_ref()
            .expect("Major exception disposition")
            .kind,
        ProductionFindingDispositionKind::ExceptionApproved
    );
    assert_eq!(
        harness
            .host
            .inspect_tender_production(&harness.tender_id)
            .expect("inspect exception-ready production")
            .expect("production")
            .tasks
            .iter()
            .find(|candidate| candidate.production_task_id == task.production_task_id)
            .expect("task")
            .state,
        ProductionTaskState::ReadyForIntegration
    );
    harness
        .host
        .close_tender(&harness.tender_id)
        .expect("close exception-ready Tender before tamper fixture");
    assert_eq!(
        harness
            .host
            .inspect_tender_integrity(&harness.tender_id)
            .expect("cold-verify exact exception disposition")
            .state,
        TenderIntegrityState::Ready
    );
    harness
        .host
        .close_tender(&harness.tender_id)
        .expect("close verified Tender before tamper fixture");
    let database = harness
        .application_home
        .join("tenders")
        .join(&harness.tender_id)
        .join("tender.sqlite");
    let connection = rusqlite::Connection::open(database).expect("open Tender Store");
    connection
        .execute_batch("DROP TRIGGER production_finding_dispositions_no_update")
        .expect("enable disposition tamper fixture");
    connection
        .execute(
            "UPDATE production_finding_dispositions
             SET rationale = 'Selectively altered exception rationale.'
             WHERE finding_id = ?1",
            [&finding.finding_id],
        )
        .expect("tamper exact exception rationale");
    drop(connection);
    let cold_host =
        QuantixHost::with_setup_platform(&harness.application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&cold_host).state, SetupState::Ready);
    assert_eq!(
        cold_host
            .inspect_tender_integrity(&harness.tender_id)
            .expect("inspect tampered exact disposition")
            .state,
        TenderIntegrityState::RecoveryRequired
    );
}

#[tokio::test]
async fn major_exceptions_in_any_order_bind_readiness_to_the_latest_reviewed_artifact() {
    let harness = Harness::new("record-extraction");
    let (_, production) = active_production(&harness).await;
    let task = production
        .tasks
        .iter()
        .find(|task| {
            task.state == ProductionTaskState::Ready
                && task.task.review_profile_id.is_some()
                && task.task.major_finding_policy == MajorFindingPolicy::EngineerExceptionAllowed
        })
        .expect("exception-permitted review-bearing task");
    harness.set_agent_scenario("production-task-review-major");
    for expected in [
        ProductionTaskState::ReviewReady,
        ProductionTaskState::RemediationReady,
        ProductionTaskState::ReviewReady,
        ProductionTaskState::RemediationReady,
    ] {
        let result = harness
            .host
            .run_production_task(RunProductionTaskCommand {
                tender_id: harness.tender_id.clone(),
                production_task_id: task.production_task_id.clone(),
            })
            .await
            .expect("advance two exact author/review rounds");
        assert_eq!(result.task.state, expected);
    }
    let detail = harness
        .host
        .inspect_production_task_review(InspectProductionTaskReviewCommand {
            tender_id: harness.tender_id.clone(),
            production_task_id: task.production_task_id.clone(),
        })
        .expect("inspect two exact Major reviews");
    assert_eq!(detail.artifact_versions.len(), 2);
    assert_eq!(detail.reviews.len(), 2);
    assert_eq!(detail.reviews[0].findings.len(), 1);
    assert_eq!(detail.reviews[1].findings.len(), 1);

    let approve = |review: &quantix_lib::ProductionReview,
                   artifact: &quantix_lib::ProductionArtifactVersion| {
        harness.host.approve_production_finding_exception(
            ApproveProductionFindingExceptionCommand {
                tender_id: harness.tender_id.clone(),
                production_task_id: task.production_task_id.clone(),
                finding_id: review.findings[0].finding_id.clone(),
                review_id: review.review_id.clone(),
                artifact_id: artifact.summary.artifact_id.clone(),
                artifact_version: artifact.summary.version,
                payload_sha256: artifact.summary.payload_sha256.clone(),
                rationale: "Accept the exact policy-permitted Major limitation.".into(),
                consequence: "Integration retains the attributable limitation.".into(),
            },
        )
    };
    let first_approval = approve(&detail.reviews[1], &detail.artifact_versions[1])
        .expect("approve the newest Major finding first");
    assert!(first_approval.readiness.is_none());
    let ready = approve(&detail.reviews[0], &detail.artifact_versions[0])
        .expect("approve the remaining older Major finding");
    let readiness = ready.readiness.expect("latest exact readiness");
    assert_eq!(readiness.artifact_version, 2);
    assert_eq!(
        readiness.review_id.as_deref(),
        Some(detail.reviews[1].review_id.as_str())
    );
}

#[tokio::test]
async fn remediation_only_review_policy_denies_a_major_exception() {
    let harness = Harness::new("record-extraction");
    let (_, production) = active_production(&harness).await;
    let task = production
        .tasks
        .iter()
        .find(|task| {
            task.state == ProductionTaskState::Ready
                && task.task.review_profile_id.is_some()
                && task.task.major_finding_policy == MajorFindingPolicy::RemediationRequired
        })
        .expect("remediation-only review-bearing task");
    harness.set_agent_scenario("production-task-review-major");
    harness
        .host
        .run_production_task(RunProductionTaskCommand {
            tender_id: harness.tender_id.clone(),
            production_task_id: task.production_task_id.clone(),
        })
        .await
        .expect("author exact candidate");
    harness
        .host
        .run_production_task(RunProductionTaskCommand {
            tender_id: harness.tender_id.clone(),
            production_task_id: task.production_task_id.clone(),
        })
        .await
        .expect("publish exact Major finding");
    let detail = harness
        .host
        .inspect_production_task_review(InspectProductionTaskReviewCommand {
            tender_id: harness.tender_id.clone(),
            production_task_id: task.production_task_id.clone(),
        })
        .expect("inspect remediation-only Major finding");
    let review = &detail.reviews[0];
    assert!(!review
        .criteria
        .iter()
        .any(|criterion| criterion == "major_exception_requires_engineer_approval"));
    let finding = &review.findings[0];
    let artifact = &detail.artifact_versions[0];
    assert_eq!(
        harness
            .host
            .approve_production_finding_exception(ApproveProductionFindingExceptionCommand {
                tender_id: harness.tender_id.clone(),
                production_task_id: task.production_task_id.clone(),
                finding_id: finding.finding_id.clone(),
                review_id: review.review_id.clone(),
                artifact_id: artifact.summary.artifact_id.clone(),
                artifact_version: artifact.summary.version,
                payload_sha256: artifact.summary.payload_sha256.clone(),
                rationale: "Attempt a forbidden policy exception.".into(),
                consequence: "The remediation-only policy would be bypassed.".into(),
            })
            .expect_err("remediation-only policy denies Major exception")
            .code,
        TenderErrorCode::InvalidCommand
    );
    assert!(harness
        .host
        .inspect_production_task_review(InspectProductionTaskReviewCommand {
            tender_id: harness.tender_id.clone(),
            production_task_id: task.production_task_id.clone(),
        })
        .expect("inspect retained blocker")
        .readiness
        .is_none());

    let database = harness
        .application_home
        .join("tenders")
        .join(&harness.tender_id)
        .join("tender.sqlite");
    let connection = rusqlite::Connection::open(&database).expect("open Tender Store fixture");
    let review_trigger: String = connection
        .query_row(
            "SELECT sql FROM sqlite_schema
             WHERE type = 'trigger' AND name = 'production_reviews_no_update'",
            [],
            |row| row.get(0),
        )
        .expect("load exact Review immutability trigger");
    let task_trigger: String = connection
        .query_row(
            "SELECT sql FROM sqlite_schema
             WHERE type = 'trigger' AND name = 'production_tasks_definition_immutable'",
            [],
            |row| row.get(0),
        )
        .expect("load exact Production Task immutability trigger");
    let (original_task_json, original_task_sha256): (String, String) = connection
        .query_row(
            "SELECT task_definition_json, task_definition_sha256
             FROM production_tasks WHERE production_task_id = ?1",
            [&task.production_task_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("load exact remediation-only task definition");
    connection
        .execute_batch(
            "DROP TRIGGER production_reviews_no_update;
             DROP TRIGGER production_tasks_definition_immutable;",
        )
        .expect("enable forbidden exception fixture");
    let original_criteria_json =
        serde_json::to_string(&review.criteria).expect("serialize original Review criteria");
    let mut forged_criteria = review.criteria.clone();
    forged_criteria.push("major_exception_requires_engineer_approval".into());
    connection
        .execute(
            "UPDATE production_reviews SET criteria_json = ?1 WHERE review_id = ?2",
            rusqlite::params![
                serde_json::to_string(&forged_criteria).expect("serialize forged Review criteria"),
                review.review_id,
            ],
        )
        .expect("forge transient exception criterion");
    let mut forged_task: Value =
        serde_json::from_str(&original_task_json).expect("parse exact Production Task");
    forged_task["major_finding_policy"] = Value::String("engineer_exception_allowed".into());
    let forged_task_json =
        serde_json::to_string(&forged_task).expect("serialize forged Production Task");
    let forged_task_sha256 = Sha256::digest(forged_task_json.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    connection
        .execute(
            "UPDATE production_tasks
             SET task_definition_json = ?1, task_definition_sha256 = ?2
             WHERE production_task_id = ?3",
            rusqlite::params![
                forged_task_json,
                forged_task_sha256,
                task.production_task_id,
            ],
        )
        .expect("forge transient exception-permitted task policy");
    connection
        .execute_batch(&review_trigger)
        .expect("restore exact Review immutability trigger");
    connection
        .execute_batch(&task_trigger)
        .expect("restore exact Production Task immutability trigger");
    drop(connection);

    harness
        .host
        .approve_production_finding_exception(ApproveProductionFindingExceptionCommand {
            tender_id: harness.tender_id.clone(),
            production_task_id: task.production_task_id.clone(),
            finding_id: finding.finding_id.clone(),
            review_id: review.review_id.clone(),
            artifact_id: artifact.summary.artifact_id.clone(),
            artifact_version: artifact.summary.version,
            payload_sha256: artifact.summary.payload_sha256.clone(),
            rationale: "Corruption fixture temporarily forges the exception criterion.".into(),
            consequence: "Cold integrity must reject this policy-forbidden exception.".into(),
        })
        .expect("publish exact forbidden-exception corruption fixture");

    let connection = rusqlite::Connection::open(&database).expect("reopen Tender Store fixture");
    connection
        .execute_batch(
            "DROP TRIGGER production_reviews_no_update;
             DROP TRIGGER production_tasks_definition_immutable;",
        )
        .expect("restore canonical remediation-only criteria");
    connection
        .execute(
            "UPDATE production_reviews SET criteria_json = ?1 WHERE review_id = ?2",
            rusqlite::params![original_criteria_json, review.review_id],
        )
        .expect("restore exact approved Review criteria");
    connection
        .execute(
            "UPDATE production_tasks
             SET task_definition_json = ?1, task_definition_sha256 = ?2
             WHERE production_task_id = ?3",
            rusqlite::params![
                original_task_json,
                original_task_sha256,
                task.production_task_id,
            ],
        )
        .expect("restore exact approved remediation-only task policy");
    connection
        .execute_batch(&review_trigger)
        .expect("restore exact Review immutability trigger again");
    connection
        .execute_batch(&task_trigger)
        .expect("restore exact Production Task immutability trigger again");
    drop(connection);
    harness
        .host
        .close_tender(&harness.tender_id)
        .expect("close policy-forbidden exception fixture");
    assert_eq!(
        harness
            .host
            .inspect_tender_integrity(&harness.tender_id)
            .expect("cold-reject policy-forbidden Major exception")
            .state,
        TenderIntegrityState::RecoveryRequired
    );
}

#[tokio::test]
async fn attempt_exhaustion_terminalizes_an_unresolved_critical_remediation_chain() {
    let harness = Harness::new("record-extraction");
    let (_, production) = active_production(&harness).await;
    let task = production
        .tasks
        .iter()
        .find(|task| {
            task.state == ProductionTaskState::Ready && task.task.review_profile_id.is_some()
        })
        .expect("review-bearing ready task");
    harness.set_agent_scenario("production-task-review-critical-repeat");
    for expected in [
        ProductionTaskState::ReviewReady,
        ProductionTaskState::RemediationReady,
        ProductionTaskState::ReviewReady,
        ProductionTaskState::RemediationReady,
        ProductionTaskState::ReviewReady,
        ProductionTaskState::RemediationReady,
        ProductionTaskState::ReviewReady,
        ProductionTaskState::RemediationReady,
    ] {
        let result = harness
            .host
            .run_production_task(RunProductionTaskCommand {
                tender_id: harness.tender_id.clone(),
                production_task_id: task.production_task_id.clone(),
            })
            .await
            .expect("advance bounded critical remediation chain");
        assert_eq!(result.task.state, expected);
    }
    assert_eq!(
        harness
            .host
            .run_production_task(RunProductionTaskCommand {
                tender_id: harness.tender_id.clone(),
                production_task_id: task.production_task_id.clone(),
            })
            .await
            .expect_err("attempt nine is denied and terminalized")
            .code,
        TenderErrorCode::InvalidCommand
    );
    let exhausted = harness
        .host
        .inspect_tender_production(&harness.tender_id)
        .expect("inspect exhausted task")
        .expect("production")
        .tasks
        .into_iter()
        .find(|candidate| candidate.production_task_id == task.production_task_id)
        .expect("exhausted task");
    assert_eq!(exhausted.state, ProductionTaskState::AttemptLimitReached);
    assert_eq!(exhausted.run_ids.len(), 8);
    assert!(exhausted.open_blocking_finding_count > 0);
    harness
        .host
        .close_tender(&harness.tender_id)
        .expect("close attempt-limited production");
    assert_eq!(
        harness
            .host
            .inspect_tender_integrity(&harness.tender_id)
            .expect("cold-verify attributable attempt exhaustion")
            .state,
        TenderIntegrityState::Ready
    );
}

#[tokio::test]
async fn linked_retry_returns_its_committed_attempt_when_follow_up_hits_the_limit() {
    let harness = Harness::new("record-extraction");
    let (_, production) = active_production(&harness).await;
    let task = production
        .tasks
        .iter()
        .find(|task| {
            task.state == ProductionTaskState::Ready && task.task.review_profile_id.is_some()
        })
        .expect("review-bearing ready task");
    harness.set_agent_scenario("production-task-review-critical-repeat");
    for expected in [
        ProductionTaskState::ReviewReady,
        ProductionTaskState::RemediationReady,
        ProductionTaskState::ReviewReady,
        ProductionTaskState::RemediationReady,
        ProductionTaskState::ReviewReady,
        ProductionTaskState::RemediationReady,
    ] {
        let result = harness
            .host
            .run_production_task(RunProductionTaskCommand {
                tender_id: harness.tender_id.clone(),
                production_task_id: task.production_task_id.clone(),
            })
            .await
            .expect("advance six bounded production attempts");
        assert_eq!(result.task.state, expected);
    }

    harness.set_agent_scenario("production-task-evidence-invalid");
    let failed = harness
        .host
        .run_production_task(RunProductionTaskCommand {
            tender_id: harness.tender_id.clone(),
            production_task_id: task.production_task_id.clone(),
        })
        .await
        .expect("terminalize retry-safe attempt seven");
    assert_eq!(failed.task.state, ProductionTaskState::Failed);
    assert!(failed
        .run
        .failure
        .as_ref()
        .is_some_and(|failure| failure.retry_safe));

    harness.set_agent_scenario("production-task-review-critical-repeat");
    let committed_retry = harness
        .host
        .run_bootstrap_agent(RunBootstrapAgentCommand {
            tender_id: harness.tender_id.clone(),
            retry_of_run_id: Some(failed.run.run_id.clone()),
        })
        .await
        .expect("return committed linked retry despite terminal follow-up limit");
    assert_eq!(committed_retry.state, AgentRunState::Completed);
    assert_eq!(
        committed_retry.retry_of_run_id.as_deref(),
        Some(failed.run.run_id.as_str())
    );
    let exhausted = harness
        .host
        .inspect_tender_production(&harness.tender_id)
        .expect("inspect linked-retry exhaustion")
        .expect("production")
        .tasks
        .into_iter()
        .find(|candidate| candidate.production_task_id == task.production_task_id)
        .expect("attempt-limited task");
    assert_eq!(exhausted.state, ProductionTaskState::AttemptLimitReached);
    assert_eq!(exhausted.run_ids.len(), 8);
}

#[tokio::test]
async fn minor_findings_remain_disclosed_without_blocking_integration() {
    let harness = Harness::new("record-extraction");
    let (_, production) = active_production(&harness).await;
    let task = production
        .tasks
        .iter()
        .find(|task| {
            task.state == ProductionTaskState::Ready && task.task.review_profile_id.is_some()
        })
        .expect("review-bearing ready task");
    harness.set_agent_scenario("production-task-review-minor");
    harness
        .host
        .run_production_task(RunProductionTaskCommand {
            tender_id: harness.tender_id.clone(),
            production_task_id: task.production_task_id.clone(),
        })
        .await
        .expect("author exact candidate");
    let reviewed = harness
        .host
        .run_production_task(RunProductionTaskCommand {
            tender_id: harness.tender_id.clone(),
            production_task_id: task.production_task_id.clone(),
        })
        .await
        .expect("publish satisfied review with disclosed Minor finding");
    assert_eq!(
        reviewed.task.state,
        ProductionTaskState::ReadyForIntegration
    );
    let detail = harness
        .host
        .inspect_production_task_review(InspectProductionTaskReviewCommand {
            tender_id: harness.tender_id.clone(),
            production_task_id: task.production_task_id.clone(),
        })
        .expect("inspect disclosed Minor finding");
    assert_eq!(
        detail.reviews[0].findings[0].severity,
        ProductionFindingSeverity::Minor
    );
    assert!(detail.reviews[0].findings[0].disposition.is_none());
    assert!(detail.readiness.is_some());
}

#[tokio::test]
async fn invalid_production_evidence_fails_terminally_without_publishing_an_artifact() {
    let harness = Harness::new("record-extraction");
    let (_, production) = active_production(&harness).await;
    let task = production
        .tasks
        .iter()
        .find(|task| {
            task.state == ProductionTaskState::Ready && task.task.review_profile_id.is_some()
        })
        .expect("review-bearing ready task");
    harness.set_agent_scenario("production-task-evidence-invalid");

    let rejected = harness
        .host
        .run_production_task(RunProductionTaskCommand {
            tender_id: harness.tender_id.clone(),
            production_task_id: task.production_task_id.clone(),
        })
        .await
        .expect("terminalize semantic Evidence rejection");
    assert_eq!(rejected.run.state, AgentRunState::Failed);
    assert_eq!(rejected.task.state, ProductionTaskState::Failed);
    assert_eq!(rejected.task.artifact_version_count, 0);
    let failure = rejected.run.failure.as_ref().expect("attributable failure");
    assert_eq!(failure.category, ProviderFailureCategory::OutputInvalid);
    assert!(failure.retry_safe);

    harness.set_agent_scenario("production-task");
    let corrected = harness
        .host
        .run_production_task(RunProductionTaskCommand {
            tender_id: harness.tender_id.clone(),
            production_task_id: task.production_task_id.clone(),
        })
        .await
        .expect("retry the exact failed author attempt");
    assert_eq!(corrected.run.state, AgentRunState::Completed);
    assert_eq!(corrected.task.state, ProductionTaskState::ReviewReady);
    assert_eq!(corrected.task.artifact_version_count, 1);
    assert_eq!(
        corrected.run.retry_of_run_id.as_deref(),
        Some(rejected.run.run_id.as_str())
    );
}

#[tokio::test]
async fn a_review_cannot_publish_after_its_exact_target_version_changes() {
    let harness = Harness::new("record-extraction");
    let (_, production) = active_production(&harness).await;
    let task = production
        .tasks
        .iter()
        .find(|task| {
            task.state == ProductionTaskState::Ready && task.task.review_profile_id.is_some()
        })
        .expect("review-bearing ready task")
        .clone();
    harness.set_agent_scenario("production-task");
    harness
        .host
        .run_production_task(RunProductionTaskCommand {
            tender_id: harness.tender_id.clone(),
            production_task_id: task.production_task_id.clone(),
        })
        .await
        .expect("author exact Artifact Version 1");

    harness.set_agent_scenario("production-task-delayed-review");
    let review_host = harness.host.clone();
    let review_tender_id = harness.tender_id.clone();
    let review_task_id = task.production_task_id.clone();
    let reviewing = tokio::spawn(async move {
        review_host
            .run_production_task(RunProductionTaskCommand {
                tender_id: review_tender_id,
                production_task_id: review_task_id,
            })
            .await
    });
    wait_for_fixture_path(&harness.codex.with_extension("production-review-waiting")).await;
    let database = harness
        .application_home
        .join("tenders")
        .join(&harness.tender_id)
        .join("tender.sqlite");
    let connection = rusqlite::Connection::open(database).expect("open active Tender Store");
    connection
        .execute_batch("DROP TRIGGER production_artifact_versions_no_update")
        .expect("enable exact target corruption fixture");
    connection
        .execute(
            "UPDATE production_artifact_versions SET version = 2
             WHERE production_task_id = ?1 AND version = 1",
            [&task.production_task_id],
        )
        .expect("change exact target version while review is running");
    drop(connection);
    fs::write(
        harness.codex.with_extension("production-review-release"),
        b"release",
    )
    .expect("release exact review response");

    let rejected = reviewing
        .await
        .expect("join review")
        .expect("terminalize changed-target review");
    assert_eq!(rejected.run.state, AgentRunState::Failed);
    assert_eq!(rejected.task.state, ProductionTaskState::Failed);
    assert_eq!(rejected.task.review_count, 0);
    assert!(!rejected.task.ready_for_integration);
    assert_eq!(
        rejected
            .run
            .failure
            .as_ref()
            .map(|failure| failure.category),
        Some(ProviderFailureCategory::OutputInvalid)
    );
}

#[tokio::test]
async fn cold_integrity_rejects_self_review_and_unqualified_review_attribution() {
    for self_review in [true, false] {
        let harness = Harness::new("record-extraction");
        let (_, production) = active_production(&harness).await;
        let task = production
            .tasks
            .iter()
            .find(|task| {
                task.state == ProductionTaskState::Ready && task.task.review_profile_id.is_some()
            })
            .expect("review-bearing ready task")
            .clone();
        harness.set_agent_scenario("production-task");
        harness
            .host
            .run_production_task(RunProductionTaskCommand {
                tender_id: harness.tender_id.clone(),
                production_task_id: task.production_task_id.clone(),
            })
            .await
            .expect("author exact candidate");
        harness
            .host
            .run_production_task(RunProductionTaskCommand {
                tender_id: harness.tender_id.clone(),
                production_task_id: task.production_task_id.clone(),
            })
            .await
            .expect("complete qualified independent review");
        let review = harness
            .host
            .inspect_production_task_review(InspectProductionTaskReviewCommand {
                tender_id: harness.tender_id.clone(),
                production_task_id: task.production_task_id.clone(),
            })
            .expect("inspect exact review")
            .reviews
            .into_iter()
            .next()
            .expect("review");
        let substitute = if self_review {
            (task.task.profile_id.clone(), task.task.profile_version)
        } else {
            production
                .tasks
                .iter()
                .map(|candidate| {
                    (
                        candidate.task.profile_id.clone(),
                        candidate.task.profile_version,
                    )
                })
                .find(|candidate| {
                    candidate.0 != task.task.profile_id && candidate.0 != review.reviewer_profile_id
                })
                .expect("separate unqualified production profile")
        };
        harness
            .host
            .close_tender(&harness.tender_id)
            .expect("close reviewed Tender before attribution corruption");
        let database = harness
            .application_home
            .join("tenders")
            .join(&harness.tender_id)
            .join("tender.sqlite");
        let connection = rusqlite::Connection::open(database).expect("open Tender Store");
        let substitute_capabilities: Vec<String> = serde_json::from_str(
            &connection
                .query_row(
                    "SELECT capabilities_json FROM agent_profile_versions
                     WHERE profile_id = ?1 AND version = ?2",
                    rusqlite::params![substitute.0, substitute.1],
                    |row| row.get::<_, String>(0),
                )
                .expect("load substitute profile capabilities"),
        )
        .expect("parse substitute profile capabilities");
        if !self_review {
            assert!(!substitute_capabilities.contains(&review.capability));
        }
        connection
            .execute_batch("DROP TRIGGER production_reviews_no_update")
            .expect("enable review attribution corruption fixture");
        connection
            .execute(
                "UPDATE production_reviews
                 SET reviewer_profile_id = ?1, reviewer_profile_version = ?2
                 WHERE review_id = ?3",
                rusqlite::params![substitute.0, substitute.1, review.review_id],
            )
            .expect("substitute forbidden reviewer attribution");
        drop(connection);

        let integrity = harness
            .host
            .inspect_tender_integrity(&harness.tender_id)
            .expect("cold inspect forbidden reviewer attribution");
        assert_eq!(
            integrity.state,
            TenderIntegrityState::RecoveryRequired,
            "self_review={self_review}: {integrity:#?}"
        );
    }
}

#[tokio::test]
async fn independent_ready_profiles_overlap_within_the_host_concurrency_ceiling() {
    let harness = Harness::new("record-extraction");
    let (_, production) = active_production(&harness).await;
    let ready = production
        .tasks
        .iter()
        .filter(|task| task.state == ProductionTaskState::Ready)
        .take(2)
        .collect::<Vec<_>>();
    assert_eq!(
        ready.len(),
        2,
        "the plan exposes a bounded parallel frontier"
    );
    assert_ne!(ready[0].task.profile_id, ready[1].task.profile_id);

    let process_start_count = fs::read_to_string(harness.codex.with_extension("agent-start-count"))
        .expect("read provider process count before overlap");
    assert_eq!(process_start_count, "1");
    harness.set_agent_scenario("production-task-multiplex");
    let first_host = harness.host.clone();
    let first_tender = harness.tender_id.clone();
    let first_task = ready[0].production_task_id.clone();
    let first = tokio::spawn(async move {
        first_host
            .run_production_task(RunProductionTaskCommand {
                tender_id: first_tender,
                production_task_id: first_task,
            })
            .await
    });
    wait_for_fixture_path(&harness.codex.with_extension("production-a-waiting")).await;

    let second_host = harness.host.clone();
    let second_tender = harness.tender_id.clone();
    let second_task = ready[1].production_task_id.clone();
    let second = tokio::spawn(async move {
        second_host
            .run_production_task(RunProductionTaskCommand {
                tender_id: second_tender,
                production_task_id: second_task,
            })
            .await
    });
    wait_for_fixture_path(&harness.codex.with_extension("production-b-waiting")).await;
    assert_eq!(
        fs::read_to_string(harness.codex.with_extension("agent-start-count"))
            .expect("read provider process count during overlap"),
        process_start_count,
        "overlapping role turns share one supervised app-server process"
    );
    let overlapping = harness
        .host
        .inspect_tender_production(&harness.tender_id)
        .expect("inspect overlapping production")
        .expect("active production");
    assert_eq!(
        overlapping
            .tasks
            .iter()
            .filter(|task| task.state == ProductionTaskState::Running)
            .count(),
        2
    );

    fs::write(
        harness.codex.with_extension("production-a-release"),
        b"release",
    )
    .expect("release first production run");
    fs::write(
        harness.codex.with_extension("production-b-release"),
        b"release",
    )
    .expect("release second production run");
    assert_eq!(
        first
            .await
            .expect("join first run")
            .expect("first run")
            .run
            .state,
        AgentRunState::Completed
    );
    assert_eq!(
        second
            .await
            .expect("join second run")
            .expect("second run")
            .run
            .state,
        AgentRunState::Completed
    );
}

#[tokio::test]
async fn production_cancellation_and_output_budget_failure_are_terminal_without_outputs() {
    let harness = Harness::new("record-extraction");
    let (_, production) = active_production(&harness).await;
    let first = production
        .tasks
        .iter()
        .find(|task| task.state == ProductionTaskState::Ready)
        .expect("first ready production task");
    harness.set_agent_scenario("production-task-delayed-a");
    let run_host = harness.host.clone();
    let run_tender = harness.tender_id.clone();
    let production_task_id = first.production_task_id.clone();
    let running = tokio::spawn(async move {
        run_host
            .run_production_task(RunProductionTaskCommand {
                tender_id: run_tender,
                production_task_id,
            })
            .await
    });
    wait_for_fixture_path(&harness.codex.with_extension("production-a-waiting")).await;
    let running_view = harness
        .host
        .inspect_tender_production(&harness.tender_id)
        .expect("inspect cancellable production")
        .expect("active production");
    let running_task = running_view
        .tasks
        .iter()
        .find(|task| task.production_task_id == first.production_task_id)
        .expect("running task");
    let run_id = running_task
        .run_ids
        .last()
        .expect("running Agent Run")
        .clone();
    assert!(harness
        .host
        .interrupt_agent_run(quantix_lib::InterruptAgentRunCommand {
            tender_id: harness.tender_id.clone(),
            run_id,
        })
        .expect("request interruption"));
    let cancelled = running
        .await
        .expect("join cancelled run")
        .expect("terminal cancelled run");
    assert_eq!(cancelled.run.state, AgentRunState::Interrupted);
    assert_eq!(cancelled.task.state, ProductionTaskState::Cancelled);
    assert_eq!(cancelled.task.artifact_version_count, 0);
    harness.set_agent_scenario("production-task");
    let cancelled_retry_author = harness
        .host
        .run_production_task(RunProductionTaskCommand {
            tender_id: harness.tender_id.clone(),
            production_task_id: first.production_task_id.clone(),
        })
        .await
        .expect("create a linked retry for the cancelled production task");
    assert_eq!(
        cancelled_retry_author.task.state,
        ProductionTaskState::ReviewReady
    );
    let cancelled_retry = harness
        .host
        .run_production_task(RunProductionTaskCommand {
            tender_id: harness.tender_id.clone(),
            production_task_id: first.production_task_id.clone(),
        })
        .await
        .expect("independently review the linked retry output");
    assert_eq!(cancelled_retry.run.state, AgentRunState::Completed);
    assert_eq!(
        cancelled_retry.task.state,
        ProductionTaskState::ReadyForIntegration
    );
    assert_eq!(cancelled_retry.task.run_ids.len(), 3);
    assert_eq!(
        cancelled_retry_author.run.retry_of_run_id.as_deref(),
        Some(cancelled.run.run_id.as_str())
    );

    let next = harness
        .host
        .inspect_tender_production(&harness.tender_id)
        .expect("inspect remaining frontier")
        .expect("active production")
        .tasks
        .into_iter()
        .find(|task| task.state == ProductionTaskState::Ready)
        .expect("another ready task");
    harness.set_agent_scenario("production-task-output-over-budget");
    let failed = harness
        .host
        .run_production_task(RunProductionTaskCommand {
            tender_id: harness.tender_id.clone(),
            production_task_id: next.production_task_id,
        })
        .await
        .expect("terminalize output budget failure");
    assert_eq!(failed.run.state, AgentRunState::Failed);
    assert_eq!(failed.task.state, ProductionTaskState::Failed);
    assert_eq!(failed.task.artifact_version_count, 0);
    assert_eq!(
        failed.run.failure.as_ref().map(|failure| failure.category),
        Some(ProviderFailureCategory::OutputInvalid)
    );
    assert!(failed
        .run
        .failure
        .as_ref()
        .is_some_and(|failure| failure.retry_safe));
    harness.set_agent_scenario("production-task");
    let failed_retry = harness
        .host
        .run_bootstrap_agent(RunBootstrapAgentCommand {
            tender_id: harness.tender_id.clone(),
            retry_of_run_id: Some(failed.run.run_id.clone()),
        })
        .await
        .expect("route the common Agent Run retry through the production scheduler");
    assert_eq!(failed_retry.state, AgentRunState::Completed);
    assert_eq!(
        failed_retry.retry_of_run_id.as_deref(),
        Some(failed.run.run_id.as_str())
    );
    let retried = harness
        .host
        .inspect_tender_production(&harness.tender_id)
        .expect("inspect retried production")
        .expect("active production")
        .tasks
        .into_iter()
        .find(|task| task.production_task_id == failed.task.production_task_id)
        .expect("retried production task");
    assert_eq!(retried.state, ProductionTaskState::ReadyForIntegration);
    assert_eq!(retried.run_ids.len(), 3);
    assert_eq!(retried.artifact_version_count, 1);
    harness
        .host
        .close_tender(&harness.tender_id)
        .expect("close retried production before cold integrity");
    let cold_host =
        QuantixHost::with_setup_platform(&harness.application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&cold_host).state, SetupState::Ready);
    let integrity = cold_host
        .inspect_tender_integrity(&harness.tender_id)
        .expect("inspect linked production retries after cold reopen");
    assert_eq!(
        integrity.state,
        TenderIntegrityState::Ready,
        "{integrity:#?}"
    );
}

#[tokio::test]
async fn restart_reconciles_an_accepted_production_turn_without_replaying_it() {
    let harness = Harness::new("record-extraction");
    let (_, production) = active_production(&harness).await;
    let ready = production
        .tasks
        .iter()
        .find(|task| task.state == ProductionTaskState::Ready)
        .expect("ready production task");
    harness.set_agent_scenario("production-task-delayed-a");
    let host = harness.host.clone();
    let tender_id = harness.tender_id.clone();
    let production_task_id = ready.production_task_id.clone();
    let running = tokio::spawn(async move {
        host.run_production_task(RunProductionTaskCommand {
            tender_id,
            production_task_id,
        })
        .await
    });
    wait_for_fixture_path(&harness.codex.with_extension("production-a-waiting")).await;
    running.abort();
    assert!(running
        .await
        .expect_err("simulate abrupt Host stop")
        .is_cancelled());
    for attempt in 0..200 {
        match harness.host.close_tender(&harness.tender_id) {
            Ok(()) => break,
            Err(error) if error.code == TenderErrorCode::StoreUnavailable && attempt < 199 => {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
            Err(error) => panic!("close interrupted cached Tender: {error:?}"),
        }
    }

    let cold_host =
        QuantixHost::with_setup_platform(&harness.application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&cold_host).state, SetupState::Ready);
    let reopened = cold_host
        .inspect_tender_production(&harness.tender_id)
        .expect("reconcile interrupted production")
        .expect("preserved production activation");
    let reconciled = reopened
        .tasks
        .iter()
        .find(|task| task.production_task_id == ready.production_task_id)
        .expect("reconciled task");
    assert_eq!(reconciled.state, ProductionTaskState::Indeterminate);
    assert_eq!(reconciled.artifact_version_count, 0);
    assert_eq!(
        reconciled.run_ids.len(),
        1,
        "restart never replays the turn"
    );
}

#[tokio::test]
async fn indeterminate_production_requires_an_engineer_disposition_before_linked_retry() {
    let harness = Harness::new("record-extraction");
    let (approved, production) = active_production(&harness).await;
    let accepted = harness
        .host
        .inspect_current_bid_decision_package(&harness.tender_id)
        .expect("inspect accepted package")
        .expect("accepted package remains current")
        .approval
        .expect("accepted package has an Approval Record");
    let ready = production
        .tasks
        .iter()
        .find(|task| task.state == ProductionTaskState::Ready)
        .expect("ready production task");
    harness.set_agent_scenario("production-task-malformed-after-turn");
    let indeterminate = harness
        .host
        .run_production_task(RunProductionTaskCommand {
            tender_id: harness.tender_id.clone(),
            production_task_id: ready.production_task_id.clone(),
        })
        .await
        .expect("terminalize the unknown production outcome");
    assert_eq!(indeterminate.run.state, AgentRunState::Indeterminate);
    assert_eq!(indeterminate.task.state, ProductionTaskState::Indeterminate);

    let changed_profile = approved
        .profiles
        .iter()
        .find(|binding| binding.profile.profile_id == ready.task.profile_id)
        .expect("profile bound to indeterminate task");
    let blocked_amendment = harness
        .host
        .revise_work_plan_proposal(ReviseWorkPlanProposalCommand {
            tender_id: harness.tender_id.clone(),
            plan_id: approved.plan_id.clone(),
            base_version: approved.version,
            actions: vec![WorkPlanRevisionAction::RenameProfile {
                profile_id: changed_profile.profile.profile_id.clone(),
                identity: "Unknown Outcome Must Be Resolved".into(),
            }],
        })
        .expect_err("unknown production outcome blocks a superseding amendment");
    assert_eq!(blocked_amendment.code, TenderErrorCode::InvalidCommand);
    let blocked_retry = harness
        .host
        .run_production_task(RunProductionTaskCommand {
            tender_id: harness.tender_id.clone(),
            production_task_id: ready.production_task_id.clone(),
        })
        .await
        .expect_err("unknown outcome cannot be retried without Engineer disposition");
    assert_eq!(blocked_retry.code, TenderErrorCode::InvalidCommand);

    harness
        .host
        .resolve_indeterminate_agent_run(ResolveIndeterminateAgentRunCommand {
            tender_id: harness.tender_id.clone(),
            run_id: indeterminate.run.run_id.clone(),
            disposition: AgentRunRecoveryDisposition::RetryTask,
            rationale: "The Engineer confirmed that the exact production task still requires a linked retry."
                .into(),
        })
        .expect("authorize exactly one linked retry");
    assert_eq!(
        harness
            .host
            .invalidate_bid_decision_approval(InvalidateBidDecisionApprovalCommand {
                tender_id: harness.tender_id.clone(),
                approval_id: accepted.approval_id,
                approval_sha256: accepted.approval_sha256,
                material_change_summary:
                    "A pending recovery cannot be stranded by lifecycle invalidation.".into(),
                affected_areas: vec!["production_recovery".into()],
            })
            .expect_err("an exact pending retry blocks approval invalidation")
            .code,
        TenderErrorCode::InvalidCommand
    );
    let database = harness
        .application_home
        .join("tenders")
        .join(&harness.tender_id)
        .join("tender.sqlite");
    let denial_payload: String = rusqlite::Connection::open(database)
        .expect("open Tender Store for invalidation audit")
        .query_row(
            "SELECT payload_json FROM audit_events
             WHERE event_type = 'bid_decision_approval_invalidation_denied'
             ORDER BY sequence DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .expect("inspect pending-retry invalidation denial");
    let denial_payload: Value =
        serde_json::from_str(&denial_payload).expect("parse invalidation denial audit");
    assert_eq!(
        denial_payload["change"]["reason"],
        Value::String("production_recovery_retry_pending".into())
    );
    let audit_before_change_denial = harness
        .host
        .open_tender(&harness.tender_id)
        .expect("inspect audit before pending-retry change denial")
        .audit_event_count;
    assert_eq!(
        harness
            .host
            .revise_tender(ReviseTenderCommand {
                tender_id: harness.tender_id.clone(),
                name: "Material change must wait for production recovery".into(),
            })
            .expect_err("material change intake cannot strand an authorized production retry")
            .code,
        TenderErrorCode::InvalidCommand
    );
    assert_eq!(
        harness
            .host
            .open_tender(&harness.tender_id)
            .expect("inspect audited pending-retry change denial")
            .audit_event_count,
        audit_before_change_denial + 1
    );
    harness.set_agent_scenario("production-task");
    let retry = harness
        .host
        .run_bootstrap_agent(RunBootstrapAgentCommand {
            tender_id: harness.tender_id.clone(),
            retry_of_run_id: Some(indeterminate.run.run_id.clone()),
        })
        .await
        .expect("run the Engineer-authorized linked production retry");
    assert_eq!(retry.state, AgentRunState::Completed);
    assert_eq!(
        retry.retry_of_run_id.as_deref(),
        Some(indeterminate.run.run_id.as_str())
    );
    let recovered = harness
        .host
        .inspect_tender_production(&harness.tender_id)
        .expect("inspect recovered production task")
        .expect("active production")
        .tasks
        .into_iter()
        .find(|task| task.production_task_id == ready.production_task_id)
        .expect("recovered task");
    assert_eq!(recovered.state, ProductionTaskState::ReadyForIntegration);
    assert_eq!(recovered.run_ids.len(), 3);
    assert_eq!(recovered.artifact_version_count, 1);
}

#[tokio::test]
async fn stale_production_recovery_can_close_but_cannot_authorize_a_retry() {
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
        .expect("publish a proposed non-gating Project Characteristic");
    let current_records = inspect_all_records(&harness.host, &harness.tender_id);
    let newly_decidable = current_records
        .iter()
        .find(|record| record.stable_key == "late_project_characteristic")
        .expect("proposed Project Characteristic")
        .clone();
    let package = harness
        .host
        .create_bid_decision_package(CreateBidDecisionPackageCommand {
            tender_id: harness.tender_id.clone(),
            base_version: None,
            disposition_updates: complete_dispositions(&current_records),
            manager_capability_demands: Vec::new(),
        })
        .expect("create package with a proposed non-gating characteristic");
    harness.set_agent_scenario("bid-package-review");
    let package = harness
        .host
        .run_bid_decision_package_review(RunBidDecisionPackageReviewCommand {
            tender_id: harness.tender_id.clone(),
            package_id: package.package_id.clone(),
            version: package.version,
        })
        .await
        .expect("review exact package")
        .package;
    let accepted = harness
        .host
        .decide_bid_decision_package(approval_command(
            &harness,
            &package,
            BidDecisionApprovalDecision::Accept,
        ))
        .expect("accept exact package");
    let plan = harness
        .host
        .compose_tender_office(ComposeTenderOfficeCommand {
            tender_id: harness.tender_id.clone(),
        })
        .expect("compose exact Work Plan");
    let approved = harness
        .host
        .decide_work_plan_proposal(DecideWorkPlanProposalCommand {
            tender_id: harness.tender_id.clone(),
            plan_id: plan.plan_id,
            version: plan.version,
            decision: WorkPlanDecision::Approve,
            rationale: "Approve the exact production plan for recovery ordering coverage.".into(),
        })
        .expect("approve exact Work Plan");
    let production = harness
        .host
        .activate_tender_production(ActivateTenderProductionCommand {
            tender_id: harness.tender_id.clone(),
            plan_id: approved.plan_id,
            plan_version: approved.version,
            plan_manifest_sha256: approved.manifest_sha256,
        })
        .expect("activate exact production plan");
    let ready = production
        .tasks
        .iter()
        .find(|task| task.state == ProductionTaskState::Ready)
        .expect("ready production task");
    harness.set_agent_scenario("production-task-malformed-after-turn");
    let indeterminate = harness
        .host
        .run_production_task(RunProductionTaskCommand {
            tender_id: harness.tender_id.clone(),
            production_task_id: ready.production_task_id.clone(),
        })
        .await
        .expect("terminalize the unknown production outcome");
    assert_eq!(indeterminate.run.state, AgentRunState::Indeterminate);

    harness
        .host
        .decide_tender_record(DecideTenderRecordCommand {
            tender_id: harness.tender_id.clone(),
            record_id: newly_decidable.record_id,
            version: newly_decidable.version,
            decision: TenderRecordEngineerDecisionKind::Verify,
            rationale: "Verify a same-version record while production recovery is unresolved."
                .into(),
        })
        .expect("register a dependency change before the recovery disposition");
    harness
        .host
        .invalidate_bid_decision_approval(InvalidateBidDecisionApprovalCommand {
            tender_id: harness.tender_id.clone(),
            approval_id: accepted.approval.approval_id,
            approval_sha256: accepted.approval.approval_sha256,
            material_change_summary:
                "A same-version verified characteristic changes the exact production basis.".into(),
            affected_areas: vec!["project_characteristics".into()],
        })
        .expect("invalidate the stale basis before resolving the unknown production outcome");
    assert_eq!(
        harness
            .host
            .resolve_indeterminate_agent_run(ResolveIndeterminateAgentRunCommand {
                tender_id: harness.tender_id.clone(),
                run_id: indeterminate.run.run_id.clone(),
                disposition: AgentRunRecoveryDisposition::RetryTask,
                rationale: "Attempt to retry work whose exact package basis is now stale.".into(),
            })
            .expect_err("a stale production basis cannot receive an immutable retry disposition")
            .code,
        TenderErrorCode::InvalidCommand
    );
    let closed = harness
        .host
        .resolve_indeterminate_agent_run(ResolveIndeterminateAgentRunCommand {
            tender_id: harness.tender_id.clone(),
            run_id: indeterminate.run.run_id,
            disposition: AgentRunRecoveryDisposition::CloseTask,
            rationale: "Close the uncertain stale task so change assessment can proceed.".into(),
        })
        .expect("close the stale uncertain task");
    assert_eq!(closed.disposition, AgentRunRecoveryDisposition::CloseTask);
}

#[tokio::test]
async fn each_multiplexed_indeterminate_run_can_receive_its_own_disposition() {
    let harness = Harness::new("record-extraction");
    let (_, production) = active_production(&harness).await;
    let ready = production
        .tasks
        .iter()
        .filter(|task| task.state == ProductionTaskState::Ready)
        .take(2)
        .collect::<Vec<_>>();
    assert_eq!(
        ready.len(),
        2,
        "two independent tasks form the ready frontier"
    );
    harness.set_agent_scenario("production-task-malformed-after-turn");
    let first = harness
        .host
        .run_production_task(RunProductionTaskCommand {
            tender_id: harness.tender_id.clone(),
            production_task_id: ready[0].production_task_id.clone(),
        })
        .await
        .expect("record the first unknown outcome");
    let second = harness
        .host
        .run_production_task(RunProductionTaskCommand {
            tender_id: harness.tender_id.clone(),
            production_task_id: ready[1].production_task_id.clone(),
        })
        .await
        .expect("record the second unknown outcome");
    assert_eq!(first.run.state, AgentRunState::Indeterminate);
    assert_eq!(second.run.state, AgentRunState::Indeterminate);

    harness
        .host
        .resolve_indeterminate_agent_run(ResolveIndeterminateAgentRunCommand {
            tender_id: harness.tender_id.clone(),
            run_id: first.run.run_id,
            disposition: AgentRunRecoveryDisposition::RetryTask,
            rationale: "Authorize one attributable retry for the first exact task.".into(),
        })
        .expect("resolve the first unknown outcome");
    let closed = harness
        .host
        .resolve_indeterminate_agent_run(ResolveIndeterminateAgentRunCommand {
            tender_id: harness.tender_id.clone(),
            run_id: second.run.run_id,
            disposition: AgentRunRecoveryDisposition::CloseTask,
            rationale: "Close the separate second uncertain task without replay.".into(),
        })
        .expect("another pending retry cannot block this independent disposition");
    assert_eq!(closed.disposition, AgentRunRecoveryDisposition::CloseTask);
}

#[tokio::test]
async fn material_authority_change_requires_an_exact_approved_work_plan_amendment() {
    let harness = Harness::new("record-extraction");
    let (approved, production) = active_production(&harness).await;
    let initial_task = production
        .tasks
        .iter()
        .find(|task| task.state == ProductionTaskState::Ready)
        .expect("ready task whose profile will be versioned");
    let changed_profile = approved
        .profiles
        .iter()
        .find(|binding| binding.profile.profile_id == initial_task.task.profile_id)
        .expect("exact profile bound to the ready task");
    harness.set_agent_scenario("production-task");
    let initial_run = harness
        .host
        .run_production_task(RunProductionTaskCommand {
            tender_id: harness.tender_id.clone(),
            production_task_id: initial_task.production_task_id.clone(),
        })
        .await
        .expect("establish the prior profile version thread");
    assert!(initial_run.run.provider_thread_ref.is_some());
    let amendment = harness
        .host
        .revise_work_plan_proposal(ReviseWorkPlanProposalCommand {
            tender_id: harness.tender_id.clone(),
            plan_id: approved.plan_id.clone(),
            base_version: approved.version,
            actions: vec![WorkPlanRevisionAction::RenameProfile {
                profile_id: changed_profile.profile.profile_id.clone(),
                identity: "Lead Tender Office Coordinator".into(),
            }],
        })
        .expect("publish immutable Work Plan Amendment");
    assert_eq!(amendment.version, approved.version + 1);
    assert!(amendment.approval.is_none());
    let amended_profile = amendment
        .profiles
        .iter()
        .find(|binding| binding.profile.profile_id == changed_profile.profile.profile_id)
        .expect("amended immutable profile version");
    assert_eq!(
        amended_profile.profile.version,
        changed_profile.profile.version + 1
    );
    assert!(amendment
        .profiles
        .iter()
        .all(|binding| binding.status == AgentProfileStatus::Proposed));
    assert!(harness
        .host
        .inspect_tender_production(&harness.tender_id)
        .expect("inspect suspended prior activation")
        .expect("prior activation")
        .tasks
        .iter()
        .filter(|task| task.state != ProductionTaskState::ReadyForIntegration)
        .all(|task| task.state == ProductionTaskState::Suspended));
    assert_eq!(
        harness
            .host
            .activate_tender_production(ActivateTenderProductionCommand {
                tender_id: harness.tender_id.clone(),
                plan_id: amendment.plan_id.clone(),
                plan_version: amendment.version,
                plan_manifest_sha256: amendment.manifest_sha256.clone(),
            })
            .expect_err("an unapproved amendment cannot activate")
            .code,
        TenderErrorCode::InvalidCommand
    );
    let approved_amendment = harness
        .host
        .decide_work_plan_proposal(DecideWorkPlanProposalCommand {
            tender_id: harness.tender_id.clone(),
            plan_id: amendment.plan_id.clone(),
            version: amendment.version,
            decision: WorkPlanDecision::Approve,
            rationale: "Approve the exact bounded authority amendment.".into(),
        })
        .expect("approve exact Work Plan Amendment");
    let reactivated = harness
        .host
        .activate_tender_production(ActivateTenderProductionCommand {
            tender_id: harness.tender_id.clone(),
            plan_id: approved_amendment.plan_id.clone(),
            plan_version: approved_amendment.version,
            plan_manifest_sha256: approved_amendment.manifest_sha256.clone(),
        })
        .expect("activate approved Work Plan Amendment");
    assert!(reactivated.active);
    assert_ne!(reactivated.activation_id, production.activation_id);
    assert!(reactivated.tasks.iter().all(|task| {
        task.task.profile_version
            == approved_amendment
                .profiles
                .iter()
                .find(|binding| binding.profile.profile_id == task.task.profile_id)
                .expect("amended exact profile")
                .profile
                .version
    }));
    let database = harness
        .application_home
        .join("tenders")
        .join(&harness.tender_id)
        .join("tender.sqlite");
    let connection = rusqlite::Connection::open(&database).expect("open amended Tender Store");
    let new_version_thread_count: u32 = connection
        .query_row(
            "SELECT COUNT(*) FROM provider_threads
             WHERE profile_id = ?1 AND profile_version = ?2",
            rusqlite::params![
                amended_profile.profile.profile_id,
                amended_profile.profile.version
            ],
            |row| row.get(0),
        )
        .expect("inspect thread exposure after profile versioning");
    assert_eq!(new_version_thread_count, 0);
    drop(connection);
    harness
        .host
        .close_tender(&harness.tender_id)
        .expect("close amended production before cold integrity");
    let cold_host =
        QuantixHost::with_setup_platform(&harness.application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&cold_host).state, SetupState::Ready);
    let integrity = cold_host
        .inspect_tender_integrity(&harness.tender_id)
        .expect("inspect amended production after cold reopen");
    assert_eq!(
        integrity.state,
        TenderIntegrityState::Ready,
        "{integrity:#?}"
    );
}

async fn wait_for_fixture_path(path: &Path) {
    for _ in 0..1_000 {
        if path.is_file() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    panic!("fixture did not reach {}", path.display());
}

async fn active_production(
    harness: &Harness,
) -> (
    quantix_lib::WorkPlanProposalInspection,
    quantix_lib::TenderProductionInspection,
) {
    let package = ready_package(harness).await;
    harness
        .host
        .decide_bid_decision_package(approval_command(
            harness,
            &package,
            BidDecisionApprovalDecision::Accept,
        ))
        .expect("accept exact package");
    let plan = harness
        .host
        .compose_tender_office(ComposeTenderOfficeCommand {
            tender_id: harness.tender_id.clone(),
        })
        .expect("compose exact Work Plan");
    let approved = harness
        .host
        .decide_work_plan_proposal(DecideWorkPlanProposalCommand {
            tender_id: harness.tender_id.clone(),
            plan_id: plan.plan_id,
            version: plan.version,
            decision: WorkPlanDecision::Approve,
            rationale: "Approve the exact bounded production plan.".into(),
        })
        .expect("approve exact Work Plan");
    assert!(approved
        .profiles
        .iter()
        .all(|binding| binding.status == AgentProfileStatus::Proposed));

    let production = harness
        .host
        .activate_tender_production(ActivateTenderProductionCommand {
            tender_id: harness.tender_id.clone(),
            plan_id: approved.plan_id.clone(),
            plan_version: approved.version,
            plan_manifest_sha256: approved.manifest_sha256.clone(),
        })
        .expect("activate exact approved Work Plan");
    (approved, production)
}

async fn complete_all_production_for_integration(harness: &Harness) {
    harness.set_agent_scenario("production-task");
    for _ in 0..64 {
        let production = harness
            .host
            .inspect_tender_production(&harness.tender_id)
            .expect("inspect integration production frontier")
            .expect("active production");
        if production
            .tasks
            .iter()
            .all(|task| task.state == ProductionTaskState::ReadyForIntegration)
        {
            return;
        }
        let task = production
            .tasks
            .iter()
            .find(|task| {
                matches!(
                    task.state,
                    ProductionTaskState::Ready
                        | ProductionTaskState::ReviewReady
                        | ProductionTaskState::RemediationReady
                )
            })
            .unwrap_or_else(|| panic!("production frontier is stranded: {production:#?}"));
        harness
            .host
            .run_production_task(RunProductionTaskCommand {
                tender_id: harness.tender_id.clone(),
                production_task_id: task.production_task_id.clone(),
            })
            .await
            .expect("advance exact integration production task");
    }
    panic!("production did not complete within its bounded attempt frontier");
}

async fn run_cost_estimator_fixture(
    harness: &Harness,
    fixture_scenario: &str,
    scenario_id: String,
    scenario_version: u32,
    description: &str,
    evidence: &AgentTaskInputReference,
) -> quantix_lib::ControlledBoqCalculationRun {
    run_cost_estimator_fixture_with_evidence(
        harness,
        fixture_scenario,
        scenario_id,
        scenario_version,
        description,
        std::slice::from_ref(evidence),
    )
    .await
}

async fn run_cost_estimator_fixture_with_evidence(
    harness: &Harness,
    fixture_scenario: &str,
    scenario_id: String,
    scenario_version: u32,
    description: &str,
    evidence: &[AgentTaskInputReference],
) -> quantix_lib::ControlledBoqCalculationRun {
    harness.set_agent_scenario(fixture_scenario);
    harness
        .host
        .run_cost_estimator_calculation(RunCostEstimatorCalculationCommand {
            tender_id: harness.tender_id.clone(),
            scenario_id,
            scenario_version,
            description: description.into(),
            quantity_evidence: evidence.to_vec(),
            unit_rate_evidence: evidence.to_vec(),
        })
        .await
        .expect("Cost Estimator input proposal and Host calculation")
        .calculation
        .expect("Host publishes a Calculation Run for a valid candidate")
}

struct PreparedEstimateBasis {
    boq_evidence: Vec<TenderEvidenceReference>,
    quotation_evidence: TenderEvidenceReference,
    calculation_evidence: AgentTaskInputReference,
    scenario_id: String,
    scenario_version: u32,
    calculation_run_id: String,
    quotation_calculation_run_id: String,
    total_calculation_run_id: String,
}

async fn import_exact_boq_table_named(
    harness: &Harness,
    directory: &str,
    filename: &str,
) -> Vec<TenderEvidenceReference> {
    import_exact_boq_table_named_with_headers(harness, directory, filename, 1).await
}

async fn import_exact_boq_table_named_with_headers(
    harness: &Harness,
    directory: &str,
    filename: &str,
    header_row_count: u32,
) -> Vec<TenderEvidenceReference> {
    let source = harness._root.path().join(directory);
    fs::create_dir(&source).expect("estimate BOQ source directory");
    fs::write(source.join(filename), b"%PDF-1.7\nBOQ_TABLE\n%%EOF\n").expect("BOQ PDF fixture");
    let imported = harness
        .host
        .import_tender_package(ImportTenderPackageCommand {
            tender_id: harness.tender_id.clone(),
            source_path: source.to_string_lossy().into_owned(),
        })
        .expect("import exact BOQ table source");
    let document = imported.documents.first().expect("registered BOQ source");
    harness
        .host
        .parse_source_artifact(ParseSourceArtifactCommand {
            tender_id: harness.tender_id.clone(),
            artifact_id: document.artifact_id.clone(),
            version: document.version,
        })
        .await
        .expect("parse exact BOQ table source");
    harness
        .host
        .designate_boq_table(DesignateBoqTableCommand {
            tender_id: harness.tender_id.clone(),
            artifact_id: document.artifact_id.clone(),
            artifact_version: document.version,
            table_number: 1,
            header_row_count,
        })
        .expect("designate exact parsed BOQ table");
    let evidence = harness
        .host
        .inspect_evidence(ParseSourceArtifactCommand {
            tender_id: harness.tender_id.clone(),
            artifact_id: document.artifact_id.clone(),
            version: document.version,
        })
        .expect("inspect designated BOQ Evidence")
        .locations
        .into_iter()
        .filter(|location| {
            location.table_number == Some(1)
                && location
                    .cell_range
                    .as_deref()
                    .and_then(|range| {
                        range
                            .chars()
                            .skip_while(|character| !character.is_ascii_digit())
                            .take_while(char::is_ascii_digit)
                            .collect::<String>()
                            .parse::<u32>()
                            .ok()
                    })
                    .is_some_and(|row| row > header_row_count)
        })
        .map(|location| TenderEvidenceReference {
            artifact_id: document.artifact_id.clone(),
            version: document.version,
            ordinal: location.ordinal,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        evidence.len(),
        usize::try_from(2 * (2_u32.saturating_sub(header_row_count)))
            .expect("bounded BOQ Evidence count"),
        "every exact BOQ data row has two cells",
    );
    evidence
}

async fn import_exact_boq_table(harness: &Harness) -> Vec<TenderEvidenceReference> {
    import_exact_boq_table_named(harness, "estimate-boq-source", "boq.pdf").await
}

async fn prepare_estimate_basis(harness: &Harness) -> PreparedEstimateBasis {
    let rule = harness
        .host
        .propose_boq_calculation_rule(ProposeBoqCalculationRuleCommand {
            tender_id: harness.tender_id.clone(),
            supported_rounding: vec![CalculationRoundingMode::MidpointAwayFromZero],
            change_rationale: "Establish the exact estimate arithmetic policy.".into(),
        })
        .expect("propose estimate Calculation Rule");
    harness.set_agent_scenario("calculation-rule-review");
    harness
        .host
        .run_calculation_rule_review(RunCalculationRuleReviewCommand {
            tender_id: harness.tender_id.clone(),
            rule_id: rule.rule_id.clone(),
            version: rule.version,
        })
        .await
        .expect("independently review estimate Calculation Rule");
    harness
        .host
        .approve_calculation_rule(ApproveCalculationRuleCommand {
            tender_id: harness.tender_id.clone(),
            rule_id: rule.rule_id,
            version: rule.version,
            manifest_sha256: rule.manifest_sha256,
            rationale: "Activate only the exact independently reviewed rule.".into(),
        })
        .expect("activate estimate Calculation Rule");

    let source = harness
        .host
        .inspect_document_register(&harness.tender_id)
        .expect("inspect estimate source")
        .documents
        .into_iter()
        .next()
        .expect("registered estimate source");
    let locations = harness
        .host
        .inspect_evidence(ParseSourceArtifactCommand {
            tender_id: harness.tender_id.clone(),
            artifact_id: source.artifact_id.clone(),
            version: source.version,
        })
        .expect("inspect estimate Evidence")
        .locations;
    let boq_location = locations.first().expect("BOQ row Evidence");
    let quote_location = locations.get(1).unwrap_or(boq_location);
    let quotation_evidence = TenderEvidenceReference {
        artifact_id: source.artifact_id.clone(),
        version: source.version,
        ordinal: quote_location.ordinal,
    };
    let calculation_evidence = AgentTaskInputReference {
        kind: "source_evidence".into(),
        reference: format!("{}#{}", source.artifact_id, boq_location.ordinal),
        version: source.version,
    };
    let boq_evidence = import_exact_boq_table(harness).await;
    let boq_calculation_evidence = boq_evidence
        .iter()
        .map(|reference| AgentTaskInputReference {
            kind: "source_evidence".into(),
            reference: format!("{}#{}", reference.artifact_id, reference.ordinal),
            version: reference.version,
        })
        .collect::<Vec<_>>();
    let quotation_calculation_evidence = std::iter::once(AgentTaskInputReference {
        kind: "source_evidence".into(),
        reference: format!(
            "{}#{}",
            quotation_evidence.artifact_id, quotation_evidence.ordinal
        ),
        version: quotation_evidence.version,
    })
    .chain(boq_calculation_evidence.iter().cloned())
    .collect::<Vec<_>>();
    let scenario = harness
        .host
        .create_calculation_scenario(CreateCalculationScenarioCommand {
            tender_id: harness.tender_id.clone(),
            name: "BOQ account base".into(),
            quantity_unit: "mm".into(),
            rate_basis_unit: "m".into(),
            rate_currency: "USD".into(),
            exchange_rate: CalculationDecimalInput {
                state: CalculationInputState::Provided,
                value: Some("50".into()),
                evidence: vec![calculation_evidence.clone()],
            },
            exchange_rate_effective_date: Some("2026-08-01".into()),
            pricing_date: "2026-08-10".into(),
            exchange_rate_type: Some(ExchangeRateType::Spot),
            output_currency: "EGP".into(),
            precision: 2,
            rounding_mode: CalculationRoundingMode::MidpointAwayFromZero,
            rationale: "Bind the exact pricing date, currency, FX and rounding basis.".into(),
        })
        .expect("approve estimate Calculation Scenario");
    let calculated = run_cost_estimator_fixture_with_evidence(
        harness,
        "cost-estimator-calculation",
        scenario.scenario_id.clone(),
        scenario.version,
        "BOQ-001 cable containment",
        &boq_calculation_evidence,
    )
    .await;
    let calculated = harness
        .host
        .approve_controlled_boq_calculation_run(ApproveControlledBoqCalculationRunCommand {
            tender_id: harness.tender_id.clone(),
            calculation_run_id: calculated.calculation_run_id,
            manifest_sha256: calculated.manifest_sha256,
            rationale: "Approve the exact BOQ row build-up and canonical total.".into(),
        })
        .expect("approve exact estimate Calculation Run");
    let quotation_calculation = run_cost_estimator_fixture_with_evidence(
        harness,
        "cost-estimator-calculation",
        scenario.scenario_id.clone(),
        scenario.version,
        "Supplier quotation normalization for BOQ-001",
        &quotation_calculation_evidence,
    )
    .await;
    let quotation_calculation = harness
        .host
        .approve_controlled_boq_calculation_run(ApproveControlledBoqCalculationRunCommand {
            tender_id: harness.tender_id.clone(),
            calculation_run_id: quotation_calculation.calculation_run_id,
            manifest_sha256: quotation_calculation.manifest_sha256,
            rationale: "Approve the distinct quotation-normalization Calculation Run.".into(),
        })
        .expect("approve quotation-normalization Calculation Run");
    let total_calculation = run_cost_estimator_fixture(
        harness,
        "cost-estimator-calculation",
        scenario.scenario_id.clone(),
        scenario.version,
        "Independent comparison total for the exact BOQ account",
        &calculation_evidence,
    )
    .await;
    let total_calculation = harness
        .host
        .approve_controlled_boq_calculation_run(ApproveControlledBoqCalculationRunCommand {
            tender_id: harness.tender_id.clone(),
            calculation_run_id: total_calculation.calculation_run_id,
            manifest_sha256: total_calculation.manifest_sha256,
            rationale: "Approve the distinct reconciliation comparison total.".into(),
        })
        .expect("approve comparison-total Calculation Run");

    let query_page = harness
        .host
        .inspect_tender_queries(InspectTenderQueriesCommand {
            tender_id: harness.tender_id.clone(),
            cursor: None,
            limit: 8,
        })
        .expect("inspect estimate Query Register");
    let owner = query_page
        .owner_profiles
        .first()
        .expect("approved Query owner");
    let assumption = harness
        .host
        .create_tender_query(CreateTenderQueryCommand {
            tender_id: harness.tender_id.clone(),
            query_type: TenderQueryType::MissingInformation,
            question: "Which productivity basis governs BOQ-001?".into(),
            ambiguity_or_gap: "The source does not state the installation productivity basis."
                .into(),
            owner_profile_id: owner.profile_id.clone(),
            owner_profile_version: owner.version,
            evidence: vec![calculation_evidence.clone()],
            affected_records: Vec::new(),
            affected_task_keys: vec!["*".into()],
            due_at: "2099-01-01T00:00:00.000Z".into(),
            material: false,
            release_blocking: false,
            proposed_treatments: vec![TenderQueryTreatmentProposalInput {
                treatment: TenderQueryTreatment::ApprovedAssumption,
                rationale:
                    "Use the stated conservative productivity basis for this estimate version."
                        .into(),
            }],
        })
        .expect("register material estimate assumption");
    harness
        .host
        .decide_tender_query_treatment(DecideTenderQueryTreatmentCommand {
            tender_id: harness.tender_id.clone(),
            query_id: assumption.query_id.clone(),
            query_version: assumption.version,
            treatment: TenderQueryTreatment::ApprovedAssumption,
            rationale: "EITL approves this exact material estimating assumption.".into(),
            treatment_details: "Use one conservative crew productivity basis only for BOQ-001."
                .into(),
            closes_query: true,
        })
        .expect("approve exact material assumption");

    PreparedEstimateBasis {
        quotation_evidence,
        boq_evidence,
        calculation_evidence,
        scenario_id: scenario.scenario_id,
        scenario_version: scenario.version,
        calculation_run_id: calculated.calculation_run_id,
        quotation_calculation_run_id: quotation_calculation.calculation_run_id,
        total_calculation_run_id: total_calculation.calculation_run_id,
    }
}

async fn prepare_and_approve_exact_basis_for_pricing(
    harness: &Harness,
) -> (BasisOfEstimateVersion, PreparedEstimateBasis) {
    let prepared = prepare_estimate_basis(harness).await;
    harness.set_agent_scenario("cost-estimator-basis");
    let proposed = harness
        .host
        .run_cost_estimator_basis(RunCostEstimatorBasisCommand {
            tender_id: harness.tender_id.clone(),
            quotation_evidence: vec![prepared.quotation_evidence.clone()],
            calculation_run_ids: vec![
                prepared.calculation_run_id.clone(),
                prepared.quotation_calculation_run_id.clone(),
                prepared.total_calculation_run_id.clone(),
            ],
        })
        .await
        .expect("Cost Estimator publishes the pricing Basis")
        .basis
        .expect("canonical pricing Basis");
    harness.set_agent_scenario("basis-of-estimate-review");
    let reviewed = harness
        .host
        .run_basis_of_estimate_review(RunBasisOfEstimateReviewCommand {
            tender_id: harness.tender_id.clone(),
            basis_id: proposed.basis_id.clone(),
            version: proposed.version,
        })
        .await
        .expect("independently review the pricing Basis")
        .basis;
    let approved = harness
        .host
        .approve_basis_of_estimate(ApproveBasisOfEstimateCommand {
            tender_id: harness.tender_id.clone(),
            basis_id: reviewed.basis_id.clone(),
            version: reviewed.version,
            manifest_sha256: reviewed.manifest_sha256.clone(),
            rationale: "Approve the exact reviewed Basis before pricing decisions.".into(),
        })
        .expect("approve exact pricing Basis");
    (approved, prepared)
}

async fn approve_exact_basis_for_pricing(harness: &Harness) -> BasisOfEstimateVersion {
    prepare_and_approve_exact_basis_for_pricing(harness).await.0
}

async fn approve_exact_final_price_for_integration(
    harness: &Harness,
) -> quantix_lib::PricingScenarioVersion {
    let (basis, prepared) = prepare_and_approve_exact_basis_for_pricing(harness).await;
    let baseline = harness
        .host
        .create_priced_cost_baseline(CreatePricedCostBaselineCommand {
            tender_id: harness.tender_id.clone(),
            basis_id: basis.basis_id,
            basis_version: basis.version,
            basis_manifest_sha256: basis.manifest_sha256,
            rationale: "Establish the exact delivery-cost basis for Integrated Review.".into(),
        })
        .expect("create integrated-review Priced Cost Baseline");
    harness.set_agent_scenario("priced-cost-baseline-review");
    let baseline = harness
        .host
        .run_priced_cost_baseline_review(RunPricedCostBaselineReviewCommand {
            tender_id: harness.tender_id.clone(),
            baseline_id: baseline.baseline_id,
            version: baseline.version,
        })
        .await
        .expect("review integrated-review Priced Cost Baseline")
        .baseline;
    let baseline = harness
        .host
        .approve_priced_cost_baseline(ApprovePricedCostBaselineCommand {
            tender_id: harness.tender_id.clone(),
            baseline_id: baseline.baseline_id,
            version: baseline.version,
            manifest_sha256: baseline.manifest_sha256,
            rationale: "Approve the exact reviewed cost baseline.".into(),
        })
        .expect("approve integrated-review Priced Cost Baseline");

    let adjustment_run = run_cost_estimator_fixture(
        harness,
        "cost-estimator-calculation",
        prepared.scenario_id,
        prepared.scenario_version,
        "Integrated Review commercial strategy input",
        &prepared.calculation_evidence,
    )
    .await;
    let adjustment_run = harness
        .host
        .approve_controlled_boq_calculation_run(ApproveControlledBoqCalculationRunCommand {
            tender_id: harness.tender_id.clone(),
            calculation_run_id: adjustment_run.calculation_run_id,
            manifest_sha256: adjustment_run.manifest_sha256,
            rationale: "Approve the distinct commercial strategy input.".into(),
        })
        .expect("approve integrated-review adjustment calculation");
    let adjustment = harness
        .host
        .create_pricing_adjustment(CreatePricingAdjustmentCommand {
            tender_id: harness.tender_id.clone(),
            baseline_id: baseline.baseline_id.clone(),
            baseline_version: baseline.version,
            baseline_manifest_sha256: baseline.manifest_sha256.clone(),
            calculation_run_id: adjustment_run.calculation_run_id,
            calculation_manifest_sha256: adjustment_run.manifest_sha256,
            kind: PricingAdjustmentKind::CommercialStrategy,
            direction: PricingAdjustmentDirection::Add,
            scope: "Integrated Review commercial commitments.".into(),
            rationale: "Keep the commercial strategy separate and reviewed.".into(),
            commercial_appetite: Some("Protect delivery certainty and margin discipline.".into()),
            exclusions: Vec::new(),
            qualifications: Vec::new(),
            remediates: Vec::new(),
        })
        .expect("create integrated-review strategy input");
    harness.set_agent_scenario("pricing-adjustment-review");
    let adjustment = harness
        .host
        .run_pricing_adjustment_review(RunPricingAdjustmentReviewCommand {
            tender_id: harness.tender_id.clone(),
            adjustment_id: adjustment.adjustment_id,
            version: adjustment.version,
        })
        .await
        .expect("review integrated-review strategy input")
        .adjustment;
    let adjustment = harness
        .host
        .approve_pricing_adjustment(ApprovePricingAdjustmentCommand {
            tender_id: harness.tender_id.clone(),
            adjustment_id: adjustment.adjustment_id,
            version: adjustment.version,
            manifest_sha256: adjustment.manifest_sha256,
            rationale: "Approve the exact reviewed commercial strategy input.".into(),
        })
        .expect("approve integrated-review strategy input");
    let strategy = harness
        .host
        .create_commercial_strategy(CreateCommercialStrategyCommand {
            tender_id: harness.tender_id.clone(),
            baseline_id: baseline.baseline_id.clone(),
            baseline_version: baseline.version,
            baseline_manifest_sha256: baseline.manifest_sha256.clone(),
            reviewed_inputs: vec![PricingAdjustmentReference {
                adjustment_id: adjustment.adjustment_id,
                version: adjustment.version,
                manifest_sha256: adjustment.manifest_sha256,
            }],
        })
        .expect("create exact reviewed commercial strategy");
    let strategy = harness
        .host
        .approve_commercial_strategy(ApproveCommercialStrategyCommand {
            tender_id: harness.tender_id.clone(),
            strategy_id: strategy.strategy_id,
            manifest_sha256: strategy.manifest_sha256,
            rationale: "Approve the exact reviewed commercial commitments.".into(),
        })
        .expect("approve exact commercial strategy");
    let scenario = harness
        .host
        .create_pricing_scenario(CreatePricingScenarioCommand {
            tender_id: harness.tender_id.clone(),
            name: "Integrated Review Tender Price".into(),
            baseline_id: baseline.baseline_id,
            baseline_version: baseline.version,
            baseline_manifest_sha256: baseline.manifest_sha256,
            strategy_id: strategy.strategy_id,
            strategy_manifest_sha256: strategy.manifest_sha256,
            adjustments: Vec::new(),
        })
        .expect("create exact Integrated Review price scenario");
    let scenario = harness
        .host
        .select_pricing_scenario(SelectPricingScenarioCommand {
            tender_id: harness.tender_id.clone(),
            pricing_scenario_id: scenario.pricing_scenario_id,
            version: scenario.version,
            manifest_sha256: scenario.manifest_sha256,
            rationale: "Select the exact Integrated Review scenario.".into(),
        })
        .expect("select exact Integrated Review scenario");
    harness
        .host
        .approve_tender_price(ApproveTenderPriceCommand {
            tender_id: harness.tender_id.clone(),
            pricing_scenario_id: scenario.pricing_scenario_id,
            version: scenario.version,
            manifest_sha256: scenario.manifest_sha256,
            calculation_manifest_sha256: scenario.calculation.manifest_sha256,
            rationale: "Approve the exact Final Price for the coordinated baseline.".into(),
        })
        .expect("approve exact Integrated Review Final Price")
}

#[tokio::test]
async fn priced_cost_baseline_is_distinct_independently_reviewed_and_exactly_approved() {
    let harness = Harness::new("record-extraction");
    active_production(&harness).await;
    let basis = approve_exact_basis_for_pricing(&harness).await;

    let baseline = harness
        .host
        .create_priced_cost_baseline(CreatePricedCostBaselineCommand {
            tender_id: harness.tender_id.clone(),
            basis_id: basis.basis_id.clone(),
            basis_version: basis.version,
            basis_manifest_sha256: basis.manifest_sha256.clone(),
            rationale: "Establish expected delivery cost separately from sell price.".into(),
        })
        .expect("create exact Priced Cost Baseline");
    assert_eq!(baseline.amount, basis.total_amount);
    assert_eq!(baseline.currency, basis.total_currency);
    assert!(baseline.current);
    assert!(baseline.review.is_none());
    assert!(baseline.approval.is_none());
    assert_eq!(
        harness
            .host
            .approve_priced_cost_baseline(ApprovePricedCostBaselineCommand {
                tender_id: harness.tender_id.clone(),
                baseline_id: baseline.baseline_id.clone(),
                version: baseline.version,
                manifest_sha256: baseline.manifest_sha256.clone(),
                rationale: "A separate qualified review is mandatory.".into(),
            })
            .expect_err("unreviewed cost baseline cannot be approved")
            .code,
        TenderErrorCode::InvalidCommand
    );

    harness.set_agent_scenario("priced-cost-baseline-review");
    let reviewed = harness
        .host
        .run_priced_cost_baseline_review(RunPricedCostBaselineReviewCommand {
            tender_id: harness.tender_id.clone(),
            baseline_id: baseline.baseline_id.clone(),
            version: baseline.version,
        })
        .await
        .expect("independently review exact Priced Cost Baseline");
    assert_eq!(
        reviewed.run.state,
        AgentRunState::Completed,
        "{:#?}\nfixture: {:?}",
        reviewed.run,
        fs::read_to_string(harness.codex.with_extension("fixture-error"))
    );
    assert_ne!(reviewed.run.profile.profile_id, basis.author_profile_id);
    assert!(reviewed.baseline.review.is_some());
    assert!(reviewed.baseline.approval.is_none());

    let approved = harness
        .host
        .approve_priced_cost_baseline(ApprovePricedCostBaselineCommand {
            tender_id: harness.tender_id.clone(),
            baseline_id: baseline.baseline_id,
            version: baseline.version,
            manifest_sha256: baseline.manifest_sha256,
            rationale: "EITL approves the exact independently reviewed expected cost.".into(),
        })
        .expect("approve exact Priced Cost Baseline");
    assert!(approved.current);
    assert!(approved.approval.is_some());
    assert_eq!(approved.amount, basis.total_amount);
    assert_ne!(approved.manifest_sha256, basis.manifest_sha256);
}

#[tokio::test]
async fn failed_priced_cost_baseline_review_is_remediated_by_an_exact_basis_successor() {
    let harness = Harness::new("record-extraction");
    active_production(&harness).await;
    let (basis, prepared) = prepare_and_approve_exact_basis_for_pricing(&harness).await;
    let first = harness
        .host
        .create_priced_cost_baseline(CreatePricedCostBaselineCommand {
            tender_id: harness.tender_id.clone(),
            basis_id: basis.basis_id,
            basis_version: basis.version,
            basis_manifest_sha256: basis.manifest_sha256,
            rationale: "Establish the first immutable expected-cost basis.".into(),
        })
        .expect("create first Priced Cost Baseline");
    harness.set_agent_scenario("priced-cost-baseline-review-failed");
    let first = harness
        .host
        .run_priced_cost_baseline_review(RunPricedCostBaselineReviewCommand {
            tender_id: harness.tender_id.clone(),
            baseline_id: first.baseline_id,
            version: first.version,
        })
        .await
        .expect("record findings against first Priced Cost Baseline")
        .baseline;
    let failed_review_manifest_sha256 = first
        .review
        .as_ref()
        .expect("failed baseline review")
        .manifest_sha256
        .clone();

    let query_page = harness
        .host
        .inspect_tender_queries(InspectTenderQueriesCommand {
            tender_id: harness.tender_id.clone(),
            cursor: None,
            limit: 8,
        })
        .expect("inspect Query Register before successor Basis");
    let owner = query_page
        .owner_profiles
        .first()
        .expect("approved Query owner");
    let evidence = prepared.boq_evidence.first().expect("BOQ Evidence");
    let evidence = AgentTaskInputReference {
        kind: "source_evidence".into(),
        reference: format!("{}#{}", evidence.artifact_id, evidence.ordinal),
        version: evidence.version,
    };
    let query = harness
        .host
        .create_tender_query(CreateTenderQueryCommand {
            tender_id: harness.tender_id.clone(),
            query_type: TenderQueryType::Ambiguity,
            question: "Which updated productivity qualification governs the priced cost?".into(),
            ambiguity_or_gap: "A new exact commercial clarification changes the estimate basis."
                .into(),
            owner_profile_id: owner.profile_id.clone(),
            owner_profile_version: owner.version,
            evidence: vec![evidence],
            affected_records: Vec::new(),
            affected_task_keys: vec!["*".into()],
            due_at: "2099-01-01T00:00:00.000Z".into(),
            material: false,
            release_blocking: false,
            proposed_treatments: vec![TenderQueryTreatmentProposalInput {
                treatment: TenderQueryTreatment::ApprovedAssumption,
                rationale: "Carry the clarified productivity basis into a successor estimate."
                    .into(),
            }],
        })
        .expect("create exact changed estimate dependency");
    harness
        .host
        .decide_tender_query_treatment(DecideTenderQueryTreatmentCommand {
            tender_id: harness.tender_id.clone(),
            query_id: query.query_id,
            query_version: query.version,
            treatment: TenderQueryTreatment::ApprovedAssumption,
            rationale: "EITL approves the exact changed productivity assumption.".into(),
            treatment_details: "Apply only to the successor Basis and dependent pricing facts."
                .into(),
            closes_query: true,
        })
        .expect("approve exact successor estimate assumption");

    harness.set_agent_scenario("cost-estimator-basis");
    let successor_basis = harness
        .host
        .run_cost_estimator_basis(RunCostEstimatorBasisCommand {
            tender_id: harness.tender_id.clone(),
            quotation_evidence: vec![prepared.quotation_evidence],
            calculation_run_ids: vec![
                prepared.calculation_run_id,
                prepared.quotation_calculation_run_id,
                prepared.total_calculation_run_id,
            ],
        })
        .await
        .expect("publish successor Basis from changed exact inputs")
        .basis
        .expect("successor Basis");
    assert_eq!(successor_basis.version, 2);
    harness.set_agent_scenario("basis-of-estimate-review");
    let successor_basis = harness
        .host
        .run_basis_of_estimate_review(RunBasisOfEstimateReviewCommand {
            tender_id: harness.tender_id.clone(),
            basis_id: successor_basis.basis_id,
            version: successor_basis.version,
        })
        .await
        .expect("review successor Basis")
        .basis;
    let successor_basis = harness
        .host
        .approve_basis_of_estimate(ApproveBasisOfEstimateCommand {
            tender_id: harness.tender_id.clone(),
            basis_id: successor_basis.basis_id,
            version: successor_basis.version,
            manifest_sha256: successor_basis.manifest_sha256,
            rationale: "Approve the exact successor estimate basis.".into(),
        })
        .expect("approve exact successor Basis");

    let successor = harness
        .host
        .create_priced_cost_baseline(CreatePricedCostBaselineCommand {
            tender_id: harness.tender_id.clone(),
            basis_id: successor_basis.basis_id,
            basis_version: successor_basis.version,
            basis_manifest_sha256: successor_basis.manifest_sha256,
            rationale: "Supersede the stale baseline with the current approved Basis.".into(),
        })
        .expect("create exact Priced Cost Baseline successor");
    assert_eq!(successor.baseline_id, first.baseline_id);
    assert_eq!(successor.version, 2);
    assert_eq!(
        successor.supersedes_baseline_manifest_sha256.as_deref(),
        Some(first.manifest_sha256.as_str())
    );
    assert_eq!(
        successor.remediates_review_manifest_sha256.as_deref(),
        Some(failed_review_manifest_sha256.as_str())
    );
    assert!(successor.current);
    assert!(successor.review.is_none());
    assert!(successor.approval.is_none());
    harness.set_agent_scenario("priced-cost-baseline-review");
    let successor = harness
        .host
        .run_priced_cost_baseline_review(RunPricedCostBaselineReviewCommand {
            tender_id: harness.tender_id.clone(),
            baseline_id: successor.baseline_id,
            version: successor.version,
        })
        .await
        .expect("independently review remediated baseline")
        .baseline;
    let successor = harness
        .host
        .approve_priced_cost_baseline(ApprovePricedCostBaselineCommand {
            tender_id: harness.tender_id.clone(),
            baseline_id: successor.baseline_id,
            version: successor.version,
            manifest_sha256: successor.manifest_sha256,
            rationale: "Approve the exact independently reviewed remediated baseline.".into(),
        })
        .expect("approve remediated Priced Cost Baseline");
    assert!(successor.approved_for_commercial_pricing);

    harness
        .host
        .close_tender(&harness.tender_id)
        .expect("close baseline-successor Tender");
    assert_eq!(
        harness
            .host
            .inspect_tender_integrity(&harness.tender_id)
            .expect("cold-verify exact baseline successor lineage")
            .state,
        TenderIntegrityState::Ready
    );
}

#[tokio::test]
async fn priced_cost_baseline_findings_fail_closed_and_remain_cold_integrity_valid() {
    let harness = Harness::new("record-extraction");
    active_production(&harness).await;
    let basis = approve_exact_basis_for_pricing(&harness).await;
    let baseline = harness
        .host
        .create_priced_cost_baseline(CreatePricedCostBaselineCommand {
            tender_id: harness.tender_id.clone(),
            basis_id: basis.basis_id,
            basis_version: basis.version,
            basis_manifest_sha256: basis.manifest_sha256,
            rationale: "Bind the exact expected cost for independent challenge.".into(),
        })
        .expect("create review target");
    harness.set_agent_scenario("priced-cost-baseline-review-failed");
    let reviewed = harness
        .host
        .run_priced_cost_baseline_review(RunPricedCostBaselineReviewCommand {
            tender_id: harness.tender_id.clone(),
            baseline_id: baseline.baseline_id.clone(),
            version: baseline.version,
        })
        .await
        .expect("record attributable review findings")
        .baseline;
    assert_eq!(
        reviewed.review.as_ref().map(|review| review.outcome),
        Some(PricedCostBaselineReviewOutcome::Failed)
    );
    assert!(!reviewed
        .review
        .as_ref()
        .expect("failed review")
        .findings
        .is_empty());
    assert_eq!(
        harness
            .host
            .approve_priced_cost_baseline(ApprovePricedCostBaselineCommand {
                tender_id: harness.tender_id.clone(),
                baseline_id: reviewed.baseline_id,
                version: reviewed.version,
                manifest_sha256: reviewed.manifest_sha256,
                rationale: "A failed independent review must never be waived here.".into(),
            })
            .expect_err("review findings block baseline approval")
            .code,
        TenderErrorCode::InvalidCommand
    );
    harness
        .host
        .close_tender(&harness.tender_id)
        .expect("close failed-review Tender");
    let integrity = harness
        .host
        .inspect_tender_integrity(&harness.tender_id)
        .expect("cold-verify attributable failed review");
    assert_eq!(
        integrity.state,
        TenderIntegrityState::Ready,
        "{integrity:#?}"
    );
}

#[tokio::test]
async fn pricing_scenarios_use_reviewed_adjustments_ordered_eitl_decisions_and_revoke_stale_price()
{
    let harness = Harness::new("record-extraction");
    active_production(&harness).await;
    let basis = approve_exact_basis_for_pricing(&harness).await;
    let baseline = harness
        .host
        .create_priced_cost_baseline(CreatePricedCostBaselineCommand {
            tender_id: harness.tender_id.clone(),
            basis_id: basis.basis_id.clone(),
            basis_version: basis.version,
            basis_manifest_sha256: basis.manifest_sha256.clone(),
            rationale: "Bind expected delivery cost before commercial pricing.".into(),
        })
        .expect("create pricing baseline");
    harness.set_agent_scenario("priced-cost-baseline-review");
    let baseline = harness
        .host
        .run_priced_cost_baseline_review(RunPricedCostBaselineReviewCommand {
            tender_id: harness.tender_id.clone(),
            baseline_id: baseline.baseline_id.clone(),
            version: baseline.version,
        })
        .await
        .expect("review pricing baseline")
        .baseline;
    let baseline = harness
        .host
        .approve_priced_cost_baseline(ApprovePricedCostBaselineCommand {
            tender_id: harness.tender_id.clone(),
            baseline_id: baseline.baseline_id,
            version: baseline.version,
            manifest_sha256: baseline.manifest_sha256,
            rationale: "Approve exact expected cost independently of sell price.".into(),
        })
        .expect("approve pricing baseline");

    let calculation_workspace = harness
        .host
        .inspect_calculation_workspace(InspectCalculationWorkspaceCommand {
            tender_id: harness.tender_id.clone(),
            scenario_offset: 0,
            run_offset: 0,
        })
        .expect("inspect controlled calculation inputs");
    let calculation_scenario = calculation_workspace
        .recent_scenarios
        .first()
        .expect("active calculation scenario");
    let evidence = calculation_workspace
        .recent_runs
        .iter()
        .flat_map(|run| run.quantity.evidence.iter())
        .next()
        .expect("calculation Evidence")
        .clone();
    let contingency_run = run_cost_estimator_fixture(
        &harness,
        "cost-estimator-calculation",
        calculation_scenario.scenario_id.clone(),
        calculation_scenario.version,
        "Commercial contingency adjustment",
        &evidence,
    )
    .await;
    let contingency_run = harness
        .host
        .approve_controlled_boq_calculation_run(ApproveControlledBoqCalculationRunCommand {
            tender_id: harness.tender_id.clone(),
            calculation_run_id: contingency_run.calculation_run_id,
            manifest_sha256: contingency_run.manifest_sha256,
            rationale: "Approve the exact controlled contingency amount.".into(),
        })
        .expect("approve adjustment Calculation Run");
    let adjustment = harness
        .host
        .create_pricing_adjustment(CreatePricingAdjustmentCommand {
            tender_id: harness.tender_id.clone(),
            baseline_id: baseline.baseline_id.clone(),
            baseline_version: baseline.version,
            baseline_manifest_sha256: baseline.manifest_sha256.clone(),
            calculation_run_id: contingency_run.calculation_run_id,
            calculation_manifest_sha256: contingency_run.manifest_sha256,
            kind: PricingAdjustmentKind::Contingency,
            direction: PricingAdjustmentDirection::Add,
            scope: "Tender-wide delivery uncertainty only.".into(),
            rationale: "Keep contingency visible as a separate reviewed input.".into(),
            commercial_appetite: None,
            exclusions: Vec::new(),
            qualifications: Vec::new(),
            remediates: Vec::new(),
        })
        .expect("create exact adjustment");
    harness.set_agent_scenario("pricing-adjustment-review");
    let adjustment = harness
        .host
        .run_pricing_adjustment_review(RunPricingAdjustmentReviewCommand {
            tender_id: harness.tender_id.clone(),
            adjustment_id: adjustment.adjustment_id.clone(),
            version: adjustment.version,
        })
        .await
        .expect("review exact adjustment")
        .adjustment;
    let adjustment = harness
        .host
        .approve_pricing_adjustment(ApprovePricingAdjustmentCommand {
            tender_id: harness.tender_id.clone(),
            adjustment_id: adjustment.adjustment_id,
            version: adjustment.version,
            manifest_sha256: adjustment.manifest_sha256,
            rationale: "Tendering Manager approves the exact reviewed contingency.".into(),
        })
        .expect("approve exact adjustment");

    let changed_rate_scenario = harness
        .host
        .create_calculation_scenario(CreateCalculationScenarioCommand {
            tender_id: harness.tender_id.clone(),
            name: "Changed commercial FX rate".into(),
            quantity_unit: "mm".into(),
            rate_basis_unit: "m".into(),
            rate_currency: "USD".into(),
            exchange_rate: CalculationDecimalInput {
                state: CalculationInputState::Provided,
                value: Some("51".into()),
                evidence: vec![evidence.clone()],
            },
            exchange_rate_effective_date: Some("2026-08-02".into()),
            pricing_date: "2026-08-10".into(),
            exchange_rate_type: Some(ExchangeRateType::Spot),
            output_currency: baseline.currency.clone(),
            precision: 2,
            rounding_mode: CalculationRoundingMode::MidpointAwayFromZero,
            rationale:
                "Compare an exact changed Exchange Rate without overwriting the prior scenario."
                    .into(),
        })
        .expect("create immutable changed-rate calculation scenario");
    let changed_rate_run = run_cost_estimator_fixture(
        &harness,
        "cost-estimator-calculation",
        changed_rate_scenario.scenario_id.clone(),
        changed_rate_scenario.version,
        "Commercial contingency at changed Exchange Rate",
        &evidence,
    )
    .await;
    let changed_rate_run = harness
        .host
        .approve_controlled_boq_calculation_run(ApproveControlledBoqCalculationRunCommand {
            tender_id: harness.tender_id.clone(),
            calculation_run_id: changed_rate_run.calculation_run_id,
            manifest_sha256: changed_rate_run.manifest_sha256,
            rationale: "Approve the separately reproduced changed-rate amount.".into(),
        })
        .expect("approve changed-rate Calculation Run");
    let changed_rate_adjustment = harness
        .host
        .create_pricing_adjustment(CreatePricingAdjustmentCommand {
            tender_id: harness.tender_id.clone(),
            baseline_id: baseline.baseline_id.clone(),
            baseline_version: baseline.version,
            baseline_manifest_sha256: baseline.manifest_sha256.clone(),
            calculation_run_id: changed_rate_run.calculation_run_id,
            calculation_manifest_sha256: changed_rate_run.manifest_sha256,
            kind: PricingAdjustmentKind::CommercialStrategy,
            direction: PricingAdjustmentDirection::Add,
            scope: "Changed-rate comparison only.".into(),
            rationale: "Keep the changed FX basis in a separate reviewed adjustment.".into(),
            commercial_appetite: Some(
                "Protect delivery certainty while the changed-rate basis is reviewed.".into(),
            ),
            exclusions: vec!["Unpriced scope outside the issued BOQ.".into()],
            qualifications: vec!["Price validity follows the approved quotation window.".into()],
            remediates: Vec::new(),
        })
        .expect("create changed-rate adjustment");
    harness.set_agent_scenario("pricing-adjustment-review-failed");
    let failed_changed_rate_adjustment = harness
        .host
        .run_pricing_adjustment_review(RunPricingAdjustmentReviewCommand {
            tender_id: harness.tender_id.clone(),
            adjustment_id: changed_rate_adjustment.adjustment_id.clone(),
            version: changed_rate_adjustment.version,
        })
        .await
        .expect("record findings against changed-rate adjustment")
        .adjustment;
    assert_eq!(
        failed_changed_rate_adjustment
            .review
            .as_ref()
            .map(|review| review.outcome),
        Some(PricedCostBaselineReviewOutcome::Failed)
    );

    let remediated_rate_run = run_cost_estimator_fixture(
        &harness,
        "cost-estimator-calculation",
        changed_rate_scenario.scenario_id,
        changed_rate_scenario.version,
        "Remediated commercial strategy at changed Exchange Rate",
        &evidence,
    )
    .await;
    let remediated_rate_run = harness
        .host
        .approve_controlled_boq_calculation_run(ApproveControlledBoqCalculationRunCommand {
            tender_id: harness.tender_id.clone(),
            calculation_run_id: remediated_rate_run.calculation_run_id,
            manifest_sha256: remediated_rate_run.manifest_sha256,
            rationale: "Approve the corrected changed-rate commercial input.".into(),
        })
        .expect("approve remediated changed-rate Calculation Run");
    let changed_rate_adjustment = harness
        .host
        .create_pricing_adjustment(CreatePricingAdjustmentCommand {
            tender_id: harness.tender_id.clone(),
            baseline_id: baseline.baseline_id.clone(),
            baseline_version: baseline.version,
            baseline_manifest_sha256: baseline.manifest_sha256.clone(),
            calculation_run_id: remediated_rate_run.calculation_run_id,
            calculation_manifest_sha256: remediated_rate_run.manifest_sha256,
            kind: PricingAdjustmentKind::CommercialStrategy,
            direction: PricingAdjustmentDirection::Add,
            scope: "Corrected changed-rate commercial strategy input.".into(),
            rationale: "Remediate the exact failed review without rewriting its findings.".into(),
            commercial_appetite: Some(
                "Protect delivery certainty without hidden margin overrides.".into(),
            ),
            exclusions: vec!["Unpriced scope outside the issued BOQ.".into()],
            qualifications: vec!["Price validity follows the approved quotation window.".into()],
            remediates: vec![PricingAdjustmentReference {
                adjustment_id: failed_changed_rate_adjustment.adjustment_id.clone(),
                version: failed_changed_rate_adjustment.version,
                manifest_sha256: failed_changed_rate_adjustment.manifest_sha256.clone(),
            }],
        })
        .expect("create attributable adjustment remediation");
    assert_eq!(
        changed_rate_adjustment
            .supersedes_adjustment_manifest_sha256
            .as_deref(),
        Some(failed_changed_rate_adjustment.manifest_sha256.as_str())
    );
    harness.set_agent_scenario("pricing-adjustment-review");
    let changed_rate_adjustment = harness
        .host
        .run_pricing_adjustment_review(RunPricingAdjustmentReviewCommand {
            tender_id: harness.tender_id.clone(),
            adjustment_id: changed_rate_adjustment.adjustment_id.clone(),
            version: changed_rate_adjustment.version,
        })
        .await
        .expect("review remediated changed-rate adjustment")
        .adjustment;
    let changed_rate_adjustment = harness
        .host
        .approve_pricing_adjustment(ApprovePricingAdjustmentCommand {
            tender_id: harness.tender_id.clone(),
            adjustment_id: changed_rate_adjustment.adjustment_id,
            version: changed_rate_adjustment.version,
            manifest_sha256: changed_rate_adjustment.manifest_sha256,
            rationale: "Approve the exact changed-rate comparison input.".into(),
        })
        .expect("approve changed-rate adjustment");
    assert_eq!(
        changed_rate_adjustment.commercial_appetite.as_deref(),
        Some("Protect delivery certainty without hidden margin overrides.")
    );

    let strategy = harness
        .host
        .create_commercial_strategy(CreateCommercialStrategyCommand {
            tender_id: harness.tender_id.clone(),
            baseline_id: baseline.baseline_id.clone(),
            baseline_version: baseline.version,
            baseline_manifest_sha256: baseline.manifest_sha256.clone(),
            reviewed_inputs: vec![PricingAdjustmentReference {
                adjustment_id: changed_rate_adjustment.adjustment_id.clone(),
                version: changed_rate_adjustment.version,
                manifest_sha256: changed_rate_adjustment.manifest_sha256.clone(),
            }],
        })
        .expect("create exact commercial strategy");
    assert_eq!(
        strategy.commercial_appetite,
        changed_rate_adjustment
            .commercial_appetite
            .clone()
            .expect("reviewed commercial appetite")
    );
    assert_eq!(strategy.exclusions, changed_rate_adjustment.exclusions);
    assert_eq!(
        strategy.qualifications,
        changed_rate_adjustment.qualifications
    );
    assert_eq!(
        harness
            .host
            .create_pricing_scenario(CreatePricingScenarioCommand {
                tender_id: harness.tender_id.clone(),
                name: "Recommended sell scenario".into(),
                baseline_id: baseline.baseline_id.clone(),
                baseline_version: baseline.version,
                baseline_manifest_sha256: baseline.manifest_sha256.clone(),
                strategy_id: strategy.strategy_id.clone(),
                strategy_manifest_sha256: strategy.manifest_sha256.clone(),
                adjustments: vec![],
            })
            .expect_err("commercial strategy needs its own EITL approval")
            .code,
        TenderErrorCode::InvalidCommand
    );
    let strategy = harness
        .host
        .approve_commercial_strategy(ApproveCommercialStrategyCommand {
            tender_id: harness.tender_id.clone(),
            strategy_id: strategy.strategy_id,
            manifest_sha256: strategy.manifest_sha256,
            rationale: "Tendering Manager approves the exact commercial appetite.".into(),
        })
        .expect("approve commercial strategy");
    let scenario = harness
        .host
        .create_pricing_scenario(CreatePricingScenarioCommand {
            tender_id: harness.tender_id.clone(),
            name: "Recommended sell scenario".into(),
            baseline_id: baseline.baseline_id.clone(),
            baseline_version: baseline.version,
            baseline_manifest_sha256: baseline.manifest_sha256.clone(),
            strategy_id: strategy.strategy_id.clone(),
            strategy_manifest_sha256: strategy.manifest_sha256.clone(),
            adjustments: vec![PricingAdjustmentReference {
                adjustment_id: adjustment.adjustment_id,
                version: adjustment.version,
                manifest_sha256: adjustment.manifest_sha256,
            }],
        })
        .expect("create immutable calculated pricing scenario");
    let comparison = harness
        .host
        .create_pricing_scenario(CreatePricingScenarioCommand {
            tender_id: harness.tender_id.clone(),
            name: "Cost-only comparison".into(),
            baseline_id: baseline.baseline_id.clone(),
            baseline_version: baseline.version,
            baseline_manifest_sha256: baseline.manifest_sha256.clone(),
            strategy_id: strategy.strategy_id.clone(),
            strategy_manifest_sha256: strategy.manifest_sha256.clone(),
            adjustments: vec![],
        })
        .expect("coexisting immutable comparison scenario");
    let changed_rate_comparison = harness
        .host
        .create_pricing_scenario(CreatePricingScenarioCommand {
            tender_id: harness.tender_id.clone(),
            name: "Changed-rate sell comparison".into(),
            baseline_id: baseline.baseline_id.clone(),
            baseline_version: baseline.version,
            baseline_manifest_sha256: baseline.manifest_sha256.clone(),
            strategy_id: strategy.strategy_id.clone(),
            strategy_manifest_sha256: strategy.manifest_sha256.clone(),
            adjustments: vec![PricingAdjustmentReference {
                adjustment_id: changed_rate_adjustment.adjustment_id,
                version: changed_rate_adjustment.version,
                manifest_sha256: changed_rate_adjustment.manifest_sha256,
            }],
        })
        .expect("coexisting changed-rate pricing scenario");
    assert_eq!(comparison.calculation.final_amount, baseline.amount);
    assert_ne!(scenario.calculation.final_amount, baseline.amount);
    assert_ne!(
        changed_rate_comparison.calculation.final_amount, scenario.calculation.final_amount,
        "an exact changed Exchange Rate produces a distinct immutable result"
    );
    assert_eq!(
        harness
            .host
            .approve_tender_price(ApproveTenderPriceCommand {
                tender_id: harness.tender_id.clone(),
                pricing_scenario_id: scenario.pricing_scenario_id.clone(),
                version: scenario.version,
                manifest_sha256: scenario.manifest_sha256.clone(),
                calculation_manifest_sha256: scenario.calculation.manifest_sha256.clone(),
                rationale: "Final Price must not precede exact scenario selection.".into(),
            })
            .expect_err("selection is a separate EITL decision")
            .code,
        TenderErrorCode::InvalidCommand
    );
    let selected = harness
        .host
        .select_pricing_scenario(SelectPricingScenarioCommand {
            tender_id: harness.tender_id.clone(),
            pricing_scenario_id: scenario.pricing_scenario_id,
            version: scenario.version,
            manifest_sha256: scenario.manifest_sha256,
            rationale: "Select the evidence-linked recommended scenario.".into(),
        })
        .expect("select exact scenario");
    let priced = harness
        .host
        .approve_tender_price(ApproveTenderPriceCommand {
            tender_id: harness.tender_id.clone(),
            pricing_scenario_id: selected.pricing_scenario_id.clone(),
            version: selected.version,
            manifest_sha256: selected.manifest_sha256.clone(),
            calculation_manifest_sha256: selected.calculation.manifest_sha256.clone(),
            rationale: "EITL approves one exact calculated Final Price.".into(),
        })
        .expect("approve exact Final Price");
    let final_price = priced
        .approved_tender_price
        .as_ref()
        .expect("immutable Approved Tender Price");
    assert!(final_price.current);
    assert_eq!(final_price.amount, priced.calculation.final_amount);
    assert_eq!(
        final_price.calculation_manifest_sha256,
        priced.calculation.manifest_sha256
    );

    let comparison = harness
        .host
        .select_pricing_scenario(SelectPricingScenarioCommand {
            tender_id: harness.tender_id.clone(),
            pricing_scenario_id: comparison.pricing_scenario_id,
            version: comparison.version,
            manifest_sha256: comparison.manifest_sha256,
            rationale: "Supersede the first choice with the exact cost-only comparison.".into(),
        })
        .expect("record immutable scenario-selection successor");
    let repriced = harness
        .host
        .approve_tender_price(ApproveTenderPriceCommand {
            tender_id: harness.tender_id.clone(),
            pricing_scenario_id: comparison.pricing_scenario_id,
            version: comparison.version,
            manifest_sha256: comparison.manifest_sha256,
            calculation_manifest_sha256: comparison.calculation.manifest_sha256,
            rationale: "Approve the exact superseding Final Price decision.".into(),
        })
        .expect("approve immutable Final Price successor");
    assert!(
        repriced
            .selection
            .as_ref()
            .expect("successor selection")
            .current
    );
    assert!(
        repriced
            .approved_tender_price
            .as_ref()
            .expect("successor Final Price")
            .current
    );
    let superseded_workspace = harness
        .host
        .inspect_pricing_workspace(quantix_lib::InspectPricingWorkspaceCommand {
            tender_id: harness.tender_id.clone(),
        })
        .expect("inspect immutable pricing-decision succession");
    let superseded_price = superseded_workspace
        .scenarios
        .iter()
        .find(|value| value.pricing_scenario_id == priced.pricing_scenario_id)
        .and_then(|value| value.approved_tender_price.as_ref())
        .expect("prior Approved Tender Price remains inspectable");
    assert!(!superseded_price.current);
    assert_eq!(superseded_workspace.decision_history.len(), 2);
    assert!(superseded_workspace.decision_history[0].selection.current);
    assert!(
        !superseded_workspace.decision_history[1].selection.current,
        "the superseded exact selection remains visible as immutable history"
    );
    assert!(
        superseded_workspace.decision_history[1]
            .approved_tender_price
            .as_ref()
            .is_some_and(|price| !price.current),
        "the superseded Final Price remains visible but revoked"
    );

    let query_page = harness
        .host
        .inspect_tender_queries(InspectTenderQueriesCommand {
            tender_id: harness.tender_id.clone(),
            cursor: None,
            limit: 8,
        })
        .expect("inspect Query Register for a changed pricing dependency");
    let owner = query_page
        .owner_profiles
        .first()
        .expect("approved Query owner");
    harness
        .host
        .create_tender_query(CreateTenderQueryCommand {
            tender_id: harness.tender_id.clone(),
            query_type: TenderQueryType::Ambiguity,
            question: "Does the employer require a revised commercial validity basis?".into(),
            ambiguity_or_gap: "The approved pricing basis predates a new commercial clarification."
                .into(),
            owner_profile_id: owner.profile_id.clone(),
            owner_profile_version: owner.version,
            evidence: vec![evidence],
            affected_records: Vec::new(),
            affected_task_keys: vec!["*".into()],
            due_at: "2099-01-01T00:00:00.000Z".into(),
            material: true,
            release_blocking: true,
            proposed_treatments: vec![TenderQueryTreatmentProposalInput {
                treatment: TenderQueryTreatment::ExternalRfiDrafting,
                rationale: "Obtain an attributable commercial clarification before reliance."
                    .into(),
            }],
        })
        .expect("record changed pricing dependency");
    let workspace = harness
        .host
        .inspect_pricing_workspace(quantix_lib::InspectPricingWorkspaceCommand {
            tender_id: harness.tender_id.clone(),
        })
        .expect("inspect automatic pricing revocation");
    let revoked = workspace
        .scenarios
        .iter()
        .find(|value| value.pricing_scenario_id == repriced.pricing_scenario_id)
        .expect("priced scenario remains inspectable");
    assert!(!revoked.current);
    assert!(
        !revoked
            .selection
            .as_ref()
            .expect("selection history retained")
            .current
    );
    assert!(
        !revoked
            .approved_tender_price
            .as_ref()
            .expect("approval history retained")
            .current
    );
    harness
        .host
        .close_tender(&harness.tender_id)
        .expect("close pricing Tender before cold verification");
    let integrity = harness
        .host
        .inspect_tender_integrity(&harness.tender_id)
        .expect("cold-verify immutable pricing history and automatic revocation");
    assert_eq!(
        integrity.state,
        TenderIntegrityState::Ready,
        "{integrity:#?}"
    );

    let pricing_calculation_run_id = repriced.calculation.pricing_calculation_run_id.clone();
    let database = harness
        .application_home
        .join("tenders")
        .join(&harness.tender_id)
        .join("tender.sqlite");
    let connection = rusqlite::Connection::open(&database).expect("open exact pricing store");
    connection
        .execute_batch("DROP TRIGGER pricing_calculation_runs_no_update")
        .expect("disable pricing calculation immutability only for corruption probe");
    connection
        .execute(
            "UPDATE pricing_calculation_runs SET created_at = '2099-01-01T00:00:00.000Z'
             WHERE pricing_calculation_run_id = ?1",
            [&pricing_calculation_run_id],
        )
        .expect("tamper only the stored pricing calculation timestamp");
    drop(connection);
    let timestamp_corrupted_host =
        QuantixHost::with_setup_platform(&harness.application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(
        ensure_quantix_setup(&timestamp_corrupted_host).state,
        SetupState::Ready
    );
    assert_eq!(
        timestamp_corrupted_host
            .inspect_tender_integrity(&harness.tender_id)
            .expect("inspect pricing calculation timestamp corruption")
            .state,
        TenderIntegrityState::RecoveryRequired
    );
}

#[tokio::test]
async fn controlled_boq_calculation_is_reviewed_exact_replayable_and_tamper_evident() {
    let harness = Harness::new("record-extraction");
    active_production(&harness).await;
    let rule = harness
        .host
        .propose_boq_calculation_rule(ProposeBoqCalculationRuleCommand {
            tender_id: harness.tender_id.clone(),
            supported_rounding: vec![
                CalculationRoundingMode::MidpointAwayFromZero,
                CalculationRoundingMode::MidpointNearestEven,
            ],
            change_rationale: "Establish both controlled commercial rounding policies.".into(),
        })
        .expect("propose deterministic BOQ rule");
    assert!(rule.deterministic_tests.iter().all(|test| test.passed));
    assert_eq!(
        rule.deterministic_tests
            .iter()
            .filter(|test| test.case_name.starts_with("pricing"))
            .count(),
        4,
        "the approved rule must govern pricing add/deduct, rounding, and negative-total rejection"
    );
    assert_eq!(
        harness
            .host
            .approve_calculation_rule(ApproveCalculationRuleCommand {
                tender_id: harness.tender_id.clone(),
                rule_id: rule.rule_id.clone(),
                version: rule.version,
                manifest_sha256: rule.manifest_sha256.clone(),
                rationale: "Activation without independent review must fail closed.".into(),
            })
            .expect_err("independent review is mandatory")
            .code,
        TenderErrorCode::InvalidCommand
    );
    harness.set_agent_scenario("calculation-rule-review");
    let reviewed = harness
        .host
        .run_calculation_rule_review(RunCalculationRuleReviewCommand {
            tender_id: harness.tender_id.clone(),
            rule_id: rule.rule_id.clone(),
            version: rule.version,
        })
        .await
        .expect("review exact Calculation Rule");
    assert_eq!(
        reviewed.run.state,
        AgentRunState::Completed,
        "{:#?}",
        reviewed.run
    );
    assert_eq!(
        reviewed.rule.review.as_ref().map(|review| review.outcome),
        Some(CalculationRuleReviewOutcome::Passed)
    );
    let active_rule = harness
        .host
        .approve_calculation_rule(ApproveCalculationRuleCommand {
            tender_id: harness.tender_id.clone(),
            rule_id: rule.rule_id,
            version: rule.version,
            manifest_sha256: rule.manifest_sha256,
            rationale: "EITL activates the exact independently reviewed deterministic rule.".into(),
        })
        .expect("activate exact reviewed Calculation Rule");
    assert!(active_rule.active);

    let source = harness
        .host
        .inspect_document_register(&harness.tender_id)
        .expect("inspect Source Artifact Register")
        .documents
        .into_iter()
        .next()
        .expect("parsed source");
    let location = harness
        .host
        .inspect_evidence(ParseSourceArtifactCommand {
            tender_id: harness.tender_id.clone(),
            artifact_id: source.artifact_id.clone(),
            version: source.version,
        })
        .expect("inspect exact Evidence")
        .locations
        .into_iter()
        .next()
        .expect("Evidence location");
    let evidence = AgentTaskInputReference {
        kind: "source_evidence".into(),
        reference: format!("{}#{}", source.artifact_id, location.ordinal),
        version: source.version,
    };
    let fx_scenario = harness
        .host
        .create_calculation_scenario(CreateCalculationScenarioCommand {
            tender_id: harness.tender_id.clone(),
            name: "Base USD to EGP".into(),
            quantity_unit: "mm".into(),
            rate_basis_unit: "m".into(),
            rate_currency: "USD".into(),
            exchange_rate: CalculationDecimalInput {
                state: CalculationInputState::Provided,
                value: Some("50".into()),
                evidence: vec![evidence.clone()],
            },
            exchange_rate_effective_date: Some("2026-08-01".into()),
            pricing_date: "2026-08-10".into(),
            exchange_rate_type: Some(ExchangeRateType::Spot),
            output_currency: "EGP".into(),
            precision: 2,
            rounding_mode: CalculationRoundingMode::MidpointAwayFromZero,
            rationale: "Approve exact FX direction and commercial rounding policy.".into(),
        })
        .expect("approve exact versioned scenario");
    assert_eq!(
        harness
            .host
            .create_calculation_scenario(CreateCalculationScenarioCommand {
                tender_id: harness.tender_id.clone(),
                name: "Invalid currency".into(),
                quantity_unit: "mm".into(),
                rate_basis_unit: "m".into(),
                rate_currency: "AAA".into(),
                exchange_rate: CalculationDecimalInput {
                    state: CalculationInputState::Provided,
                    value: Some("50".into()),
                    evidence: vec![evidence.clone()],
                },
                exchange_rate_effective_date: Some("2026-08-01".into()),
                pricing_date: "2026-08-10".into(),
                exchange_rate_type: Some(ExchangeRateType::Spot),
                output_currency: "EGP".into(),
                precision: 2,
                rounding_mode: CalculationRoundingMode::MidpointAwayFromZero,
                rationale: "Invalid currency must be rejected.".into(),
            })
            .expect_err("unrecognized currency is invalid")
            .code,
        TenderErrorCode::InvalidCommand
    );

    let exact = run_cost_estimator_fixture(
        &harness,
        "cost-estimator-calculation",
        fx_scenario.scenario_id.clone(),
        fx_scenario.version,
        "BOQ item 1 — cable containment",
        &evidence,
    )
    .await;
    assert_eq!(exact.status, ControlledBoqCalculationStatus::Completed);
    assert_eq!(exact.normalized_quantity.as_deref(), Some("1.25"));
    assert_eq!(exact.unrounded_source_amount.as_deref(), Some("3"));
    assert_eq!(exact.unrounded_output_amount.as_deref(), Some("150"));
    assert_eq!(exact.final_amount.as_deref(), Some("150.00"));
    assert!(harness
        .host
        .inspect_tender_record_authorities(&harness.tender_id)
        .expect("inspect authorities before value approval")
        .into_iter()
        .all(|authority| authority.authority_id != exact.calculation_run_id));
    let database = harness
        .application_home
        .join("tenders")
        .join(&harness.tender_id)
        .join("tender.sqlite");
    let connection = rusqlite::Connection::open(&database).expect("open approval-boundary store");
    let original_manifest: String = connection
        .query_row(
            "SELECT manifest_json FROM calculation_runs WHERE calculation_run_id = ?1",
            [&exact.calculation_run_id],
            |row| row.get(0),
        )
        .expect("load exact run manifest before corruption");
    let immutable_trigger: String = connection
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'trigger'
             AND name = 'calculation_runs_no_update'",
            [],
            |row| row.get(0),
        )
        .expect("load Calculation Run immutability trigger");
    connection
        .execute_batch("DROP TRIGGER calculation_runs_no_update;")
        .expect("open corruption seam for approval regression");
    connection
        .execute(
            "UPDATE calculation_runs
             SET manifest_json = json_set(manifest_json, '$.final_amount', '999999.99')
             WHERE calculation_run_id = ?1",
            [&exact.calculation_run_id],
        )
        .expect("corrupt deterministic output before approval");
    drop(connection);
    assert_eq!(
        harness
            .host
            .approve_controlled_boq_calculation_run(ApproveControlledBoqCalculationRunCommand {
                tender_id: harness.tender_id.clone(),
                calculation_run_id: exact.calculation_run_id.clone(),
                manifest_sha256: exact.manifest_sha256.clone(),
                rationale: "A corrupted result must never become authority.".into(),
            })
            .expect_err("approval revalidates stored hash and arithmetic")
            .code,
        TenderErrorCode::IntegrityFailed
    );
    let connection =
        rusqlite::Connection::open(&database).expect("restore approval-boundary store");
    connection
        .execute(
            "UPDATE calculation_runs SET manifest_json = ?1 WHERE calculation_run_id = ?2",
            rusqlite::params![original_manifest, exact.calculation_run_id],
        )
        .expect("restore exact run manifest");
    connection
        .execute_batch(&immutable_trigger)
        .expect("restore Calculation Run immutability trigger");
    drop(connection);
    let approved_exact = harness
        .host
        .approve_controlled_boq_calculation_run(ApproveControlledBoqCalculationRunCommand {
            tender_id: harness.tender_id.clone(),
            calculation_run_id: exact.calculation_run_id.clone(),
            manifest_sha256: exact.manifest_sha256.clone(),
            rationale: "EITL approves the exact evidence-backed canonical value.".into(),
        })
        .expect("approve exact canonical Calculation Run");
    assert!(approved_exact.approval.is_some());
    let authority = harness
        .host
        .inspect_tender_record_authorities(&harness.tender_id)
        .expect("inspect approved Calculation Run authority")
        .into_iter()
        .find(|authority| authority.authority_id == exact.calculation_run_id)
        .expect("only the approved run is registered as a deterministic authority");
    assert_eq!(authority.value, "150.00 EGP");

    let zero = run_cost_estimator_fixture(
        &harness,
        "cost-estimator-calculation-zero",
        fx_scenario.scenario_id.clone(),
        fx_scenario.version,
        "Explicit zero quantity",
        &evidence,
    )
    .await;
    assert_eq!(zero.status, ControlledBoqCalculationStatus::Completed);
    assert_eq!(zero.final_amount.as_deref(), Some("0.00"));
    let missing = run_cost_estimator_fixture(
        &harness,
        "cost-estimator-calculation-missing",
        fx_scenario.scenario_id.clone(),
        fx_scenario.version,
        "Visible missing quantity",
        &evidence,
    )
    .await;
    assert_eq!(missing.status, ControlledBoqCalculationStatus::MissingInput);
    assert!(missing.final_amount.is_none());
    let ambiguous = run_cost_estimator_fixture(
        &harness,
        "cost-estimator-calculation-ambiguous",
        fx_scenario.scenario_id.clone(),
        fx_scenario.version,
        "Visible ambiguous quantity",
        &evidence,
    )
    .await;
    assert_eq!(
        ambiguous.status,
        ControlledBoqCalculationStatus::AmbiguousInput
    );
    assert!(ambiguous.final_amount.is_none());
    let unavailable = run_cost_estimator_fixture(
        &harness,
        "cost-estimator-calculation-unavailable",
        fx_scenario.scenario_id.clone(),
        fx_scenario.version,
        "Visible unavailable quantity",
        &evidence,
    )
    .await;
    assert_eq!(
        unavailable.status,
        ControlledBoqCalculationStatus::UnavailableInput
    );
    assert!(unavailable.final_amount.is_none());
    let invalid = run_cost_estimator_fixture(
        &harness,
        "cost-estimator-calculation-invalid",
        fx_scenario.scenario_id,
        fx_scenario.version,
        "Visible invalid quantity",
        &evidence,
    )
    .await;
    assert_eq!(invalid.status, ControlledBoqCalculationStatus::InvalidInput);
    assert!(invalid.final_amount.is_none());

    for (state, expected_status, name) in [
        (
            CalculationInputState::Missing,
            ControlledBoqCalculationStatus::MissingInput,
            "Missing exchange rate",
        ),
        (
            CalculationInputState::Ambiguous,
            ControlledBoqCalculationStatus::AmbiguousInput,
            "Ambiguous exchange rate",
        ),
        (
            CalculationInputState::Unavailable,
            ControlledBoqCalculationStatus::UnavailableInput,
            "Unavailable exchange rate",
        ),
    ] {
        let scenario = harness
            .host
            .create_calculation_scenario(CreateCalculationScenarioCommand {
                tender_id: harness.tender_id.clone(),
                name: name.into(),
                quantity_unit: "mm".into(),
                rate_basis_unit: "m".into(),
                rate_currency: "USD".into(),
                exchange_rate: CalculationDecimalInput {
                    state,
                    value: None,
                    evidence: if state == CalculationInputState::Missing {
                        Vec::new()
                    } else {
                        vec![evidence.clone()]
                    },
                },
                exchange_rate_effective_date: None,
                pricing_date: "2026-08-10".into(),
                exchange_rate_type: Some(ExchangeRateType::Spot),
                output_currency: "EGP".into(),
                precision: 2,
                rounding_mode: CalculationRoundingMode::MidpointAwayFromZero,
                rationale: format!("Record {name} as a distinct governed scenario state."),
            })
            .expect("approve visible exchange-rate state");
        let run = run_cost_estimator_fixture(
            &harness,
            "cost-estimator-calculation",
            scenario.scenario_id,
            scenario.version,
            name,
            &evidence,
        )
        .await;
        assert_eq!(run.status, expected_status);
        assert!(run.final_amount.is_none());
    }

    let mismatch_scenario = harness
        .host
        .create_calculation_scenario(CreateCalculationScenarioCommand {
            tender_id: harness.tender_id.clone(),
            name: "Dimension mismatch".into(),
            quantity_unit: "m".into(),
            rate_basis_unit: "each".into(),
            rate_currency: "EGP".into(),
            exchange_rate: CalculationDecimalInput {
                state: CalculationInputState::NotApplicable,
                value: None,
                evidence: Vec::new(),
            },
            exchange_rate_effective_date: None,
            pricing_date: "2026-08-10".into(),
            exchange_rate_type: None,
            output_currency: "EGP".into(),
            precision: 2,
            rounding_mode: CalculationRoundingMode::MidpointAwayFromZero,
            rationale: "Record dimension incompatibility visibly rather than coercing it.".into(),
        })
        .expect("approve explicit incompatible scenario");
    let mismatch = run_cost_estimator_fixture(
        &harness,
        "cost-estimator-calculation",
        mismatch_scenario.scenario_id,
        mismatch_scenario.version,
        "Visible dimensional mismatch",
        &evidence,
    )
    .await;
    assert_eq!(
        mismatch.status,
        ControlledBoqCalculationStatus::DimensionMismatch
    );
    assert!(mismatch.final_amount.is_none());

    for index in 1..=4 {
        harness
            .host
            .create_calculation_scenario(CreateCalculationScenarioCommand {
                tender_id: harness.tender_id.clone(),
                name: format!("Bounded scenario page {index}"),
                quantity_unit: "each".into(),
                rate_basis_unit: "each".into(),
                rate_currency: "EGP".into(),
                exchange_rate: CalculationDecimalInput {
                    state: CalculationInputState::NotApplicable,
                    value: None,
                    evidence: Vec::new(),
                },
                exchange_rate_effective_date: None,
                pricing_date: "2026-08-10".into(),
                exchange_rate_type: None,
                output_currency: "EGP".into(),
                precision: 2,
                rounding_mode: CalculationRoundingMode::MidpointAwayFromZero,
                rationale: "Exercise bounded immutable scenario navigation.".into(),
            })
            .expect("create scenario pagination fixture");
    }

    let inspection = harness
        .host
        .inspect_calculation_workspace(InspectCalculationWorkspaceCommand {
            tender_id: harness.tender_id.clone(),
            scenario_offset: 0,
            run_offset: 0,
        })
        .expect("inspect canonical Calculation Runs");
    assert_eq!(inspection.total_run_count, 10);
    assert_eq!(inspection.total_scenario_count, 9);
    assert_eq!(inspection.recent_scenarios.len(), 8);
    assert!(inspection.has_older_scenarios);
    assert_eq!(inspection.recent_runs.len(), 8);
    assert!(inspection.has_older_runs);
    let older_runs = harness
        .host
        .inspect_calculation_workspace(InspectCalculationWorkspaceCommand {
            tender_id: harness.tender_id.clone(),
            scenario_offset: 0,
            run_offset: 8,
        })
        .expect("inspect bounded older Calculation Runs");
    assert_eq!(older_runs.recent_runs.len(), 2);
    assert!(!older_runs.has_older_runs);
    assert!(older_runs.recent_runs.iter().any(|run| {
        run.calculation_run_id == exact.calculation_run_id && run.approval.is_some()
    }));
    let older_scenarios = harness
        .host
        .inspect_calculation_workspace(InspectCalculationWorkspaceCommand {
            tender_id: harness.tender_id.clone(),
            scenario_offset: 8,
            run_offset: 0,
        })
        .expect("inspect bounded older Calculation Scenarios");
    assert_eq!(older_scenarios.recent_scenarios.len(), 1);
    assert!(!older_scenarios.has_older_scenarios);

    harness
        .host
        .close_tender(&harness.tender_id)
        .expect("close Tender before cold replay");
    let integrity = harness
        .host
        .inspect_tender_integrity(&harness.tender_id)
        .expect("replay calculation integrity");
    assert_eq!(
        integrity.state,
        TenderIntegrityState::Ready,
        "{integrity:#?}"
    );

    let database = harness
        .application_home
        .join("tenders")
        .join(&harness.tender_id)
        .join("tender.sqlite");
    let connection = rusqlite::Connection::open(database).expect("open calculation store");
    connection
        .execute(
            "INSERT INTO tender_record_authorities (
               authority_id, kind, value, description, manifest_sha256,
               tender_revision, created_by, created_at
             ) VALUES ('00000000000000000000000000000000', 'calculation_run',
                       '1.00 EGP', 'Orphan injected authority',
                       '0000000000000000000000000000000000000000000000000000000000000000',
                       (SELECT current_revision FROM tender WHERE singleton = 1),
                       'tamper-fixture', '2026-08-10T00:00:00Z')",
            [],
        )
        .expect("inject orphan Calculation authority");
    drop(connection);
    let integrity = harness
        .host
        .inspect_tender_integrity(&harness.tender_id)
        .expect("detect orphan Calculation authority");
    assert_eq!(integrity.state, TenderIntegrityState::RecoveryRequired);
}

#[tokio::test]
async fn cost_estimator_output_cannot_publish_as_a_newer_tender_revision() {
    let harness = Harness::new("record-extraction");
    active_production(&harness).await;
    let rule = harness
        .host
        .propose_boq_calculation_rule(ProposeBoqCalculationRuleCommand {
            tender_id: harness.tender_id.clone(),
            supported_rounding: vec![CalculationRoundingMode::MidpointAwayFromZero],
            change_rationale: "Establish one exact controlled commercial rounding policy.".into(),
        })
        .expect("propose controlled rule for revision race");
    harness.set_agent_scenario("calculation-rule-review");
    harness
        .host
        .run_calculation_rule_review(RunCalculationRuleReviewCommand {
            tender_id: harness.tender_id.clone(),
            rule_id: rule.rule_id.clone(),
            version: rule.version,
        })
        .await
        .expect("review controlled rule for revision race");
    harness
        .host
        .approve_calculation_rule(ApproveCalculationRuleCommand {
            tender_id: harness.tender_id.clone(),
            rule_id: rule.rule_id,
            version: rule.version,
            manifest_sha256: rule.manifest_sha256,
            rationale: "Activate the independently reviewed exact rule.".into(),
        })
        .expect("activate controlled rule for revision race");
    let source = harness
        .host
        .inspect_document_register(&harness.tender_id)
        .expect("inspect source for revision race")
        .documents
        .into_iter()
        .next()
        .expect("parsed source for revision race");
    let location = harness
        .host
        .inspect_evidence(ParseSourceArtifactCommand {
            tender_id: harness.tender_id.clone(),
            artifact_id: source.artifact_id.clone(),
            version: source.version,
        })
        .expect("inspect evidence for revision race")
        .locations
        .into_iter()
        .next()
        .expect("evidence for revision race");
    let evidence = AgentTaskInputReference {
        kind: "source_evidence".into(),
        reference: format!("{}#{}", source.artifact_id, location.ordinal),
        version: source.version,
    };
    let scenario = harness
        .host
        .create_calculation_scenario(CreateCalculationScenarioCommand {
            tender_id: harness.tender_id.clone(),
            name: "Revision-race scenario".into(),
            quantity_unit: "mm".into(),
            rate_basis_unit: "m".into(),
            rate_currency: "USD".into(),
            exchange_rate: CalculationDecimalInput {
                state: CalculationInputState::Provided,
                value: Some("50".into()),
                evidence: vec![evidence.clone()],
            },
            exchange_rate_effective_date: Some("2026-08-01".into()),
            pricing_date: "2026-08-10".into(),
            exchange_rate_type: Some(ExchangeRateType::Spot),
            output_currency: "EGP".into(),
            precision: 2,
            rounding_mode: CalculationRoundingMode::MidpointAwayFromZero,
            rationale: "Bind the revision-race fixture to one exact scenario.".into(),
        })
        .expect("approve revision-race scenario");
    harness.set_agent_scenario("cost-estimator-calculation-delayed");
    let host = harness.host.clone();
    let command = RunCostEstimatorCalculationCommand {
        tender_id: harness.tender_id.clone(),
        scenario_id: scenario.scenario_id,
        scenario_version: scenario.version,
        description: "Stale Cost Estimator output".into(),
        quantity_evidence: vec![evidence.clone()],
        unit_rate_evidence: vec![evidence],
    };
    let calculation =
        tokio::spawn(async move { host.run_cost_estimator_calculation(command).await });
    wait_for_fixture_path(
        &harness
            .codex
            .with_extension("cost-estimator-output-waiting"),
    )
    .await;
    let database = harness
        .application_home
        .join("tenders")
        .join(&harness.tender_id)
        .join("tender.sqlite");
    let connection = rusqlite::Connection::open(database).expect("open revision-race store");
    let prior_revision: u32 = connection
        .query_row(
            "SELECT current_revision FROM tender WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
        .expect("read revision-race basis");
    assert_eq!(
        connection
            .execute(
                "UPDATE tender SET current_revision = ?1 WHERE singleton = 1",
                [prior_revision + 1],
            )
            .expect("advance revision during provider turn"),
        1
    );
    fs::write(
        harness
            .codex
            .with_extension("cost-estimator-output-release"),
        b"release",
    )
    .expect("release stale Cost Estimator output");
    let result = calculation
        .await
        .expect("join delayed Cost Estimator")
        .expect("stale completion is recorded as a terminal run");
    assert_eq!(result.run.state, AgentRunState::Failed);
    assert!(result.calculation.is_none());
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM calculation_runs", [], |row| {
                row.get::<_, u32>(0)
            })
            .expect("count canonical calculations after stale completion"),
        0
    );
}

#[tokio::test]
async fn failed_calculation_rule_review_allows_one_exact_successor() {
    let harness = Harness::new("record-extraction");
    active_production(&harness).await;
    let first = harness
        .host
        .propose_boq_calculation_rule(ProposeBoqCalculationRuleCommand {
            tender_id: harness.tender_id.clone(),
            supported_rounding: vec![
                CalculationRoundingMode::MidpointAwayFromZero,
                CalculationRoundingMode::MidpointNearestEven,
            ],
            change_rationale: "Establish both controlled commercial rounding policies.".into(),
        })
        .expect("propose first rule version");
    harness.set_agent_scenario("calculation-rule-review-failed");
    let failed = harness
        .host
        .run_calculation_rule_review(RunCalculationRuleReviewCommand {
            tender_id: harness.tender_id.clone(),
            rule_id: first.rule_id.clone(),
            version: first.version,
        })
        .await
        .expect("publish attributable failed review");
    assert_eq!(
        failed.rule.review.as_ref().map(|review| review.outcome),
        Some(CalculationRuleReviewOutcome::Failed)
    );
    assert_eq!(
        harness
            .host
            .propose_boq_calculation_rule(ProposeBoqCalculationRuleCommand {
                tender_id: harness.tender_id.clone(),
                supported_rounding: vec![
                    CalculationRoundingMode::MidpointAwayFromZero,
                    CalculationRoundingMode::MidpointNearestEven,
                ],
                change_rationale: "Retry the unchanged rule.".into(),
            })
            .expect_err("an unchanged failed rule cannot be re-reviewed")
            .code,
        TenderErrorCode::InvalidCommand
    );
    let successor = harness
        .host
        .propose_boq_calculation_rule(ProposeBoqCalculationRuleCommand {
            tender_id: harness.tender_id.clone(),
            supported_rounding: vec![CalculationRoundingMode::MidpointAwayFromZero],
            change_rationale:
                "Resolve the independent finding by allowing one unambiguous rounding policy."
                    .into(),
        })
        .expect("failed review permits exact successor");
    assert_eq!(successor.rule_id, first.rule_id);
    assert_eq!(successor.version, first.version + 1);
    harness.set_agent_scenario("calculation-rule-review");
    let reviewed = harness
        .host
        .run_calculation_rule_review(RunCalculationRuleReviewCommand {
            tender_id: harness.tender_id.clone(),
            rule_id: successor.rule_id.clone(),
            version: successor.version,
        })
        .await
        .expect("review successor");
    harness
        .host
        .approve_calculation_rule(ApproveCalculationRuleCommand {
            tender_id: harness.tender_id.clone(),
            rule_id: successor.rule_id,
            version: successor.version,
            manifest_sha256: successor.manifest_sha256,
            rationale: "Activate only the exact successor that passed independent review.".into(),
        })
        .expect("activate successor");
    assert_eq!(reviewed.run.state, AgentRunState::Completed);
    harness
        .host
        .close_tender(&harness.tender_id)
        .expect("close before cold verification");
    let integrity = harness
        .host
        .inspect_tender_integrity(&harness.tender_id)
        .expect("verify failed historical review and active successor");
    assert_eq!(
        integrity.state,
        TenderIntegrityState::Ready,
        "{integrity:#?}"
    );
}

#[tokio::test]
async fn late_applicable_query_terminalizes_the_stale_basis_turn_without_publication() {
    let harness = Harness::new("record-extraction");
    active_production(&harness).await;
    let prepared = prepare_estimate_basis(&harness).await;
    let query_page = harness
        .host
        .inspect_tender_queries(InspectTenderQueriesCommand {
            tender_id: harness.tender_id.clone(),
            cursor: None,
            limit: 8,
        })
        .expect("inspect exact Query owner");
    let owner = query_page.owner_profiles.first().expect("Query owner");

    harness.set_agent_scenario("cost-estimator-basis-delayed");
    let host = harness.host.clone();
    let command = RunCostEstimatorBasisCommand {
        tender_id: harness.tender_id.clone(),
        quotation_evidence: vec![prepared.quotation_evidence],
        calculation_run_ids: vec![
            prepared.calculation_run_id,
            prepared.quotation_calculation_run_id,
            prepared.total_calculation_run_id,
        ],
    };
    let pending = tokio::spawn(async move { host.run_cost_estimator_basis(command).await });
    wait_for_fixture_path(&harness.codex.with_extension("cost-estimator-basis-waiting")).await;

    harness
        .host
        .create_tender_query(CreateTenderQueryCommand {
            tender_id: harness.tender_id.clone(),
            query_type: TenderQueryType::MissingInformation,
            question: "Which late commercial qualification affects the estimate?".into(),
            ambiguity_or_gap:
                "A new applicable estimating dependency arrived during the provider turn.".into(),
            owner_profile_id: owner.profile_id.clone(),
            owner_profile_version: owner.version,
            evidence: vec![prepared.calculation_evidence],
            affected_records: Vec::new(),
            affected_task_keys: vec!["*".into()],
            due_at: "2099-01-01T00:00:00.000Z".into(),
            material: true,
            release_blocking: true,
            proposed_treatments: vec![TenderQueryTreatmentProposalInput {
                treatment: TenderQueryTreatment::ExternalRfiDrafting,
                rationale: "Resolve the new exact commercial dependency before reliance.".into(),
            }],
        })
        .expect("register late exact estimating Query");
    fs::write(
        harness.codex.with_extension("cost-estimator-basis-release"),
        b"release",
    )
    .expect("release stale Basis output");

    let result = pending
        .await
        .expect("join delayed Basis turn")
        .expect("stale Basis completion remains inspectable");
    assert_eq!(result.run.state, AgentRunState::Failed);
    assert!(result.basis.is_none());
    let inspected = harness
        .host
        .inspect_estimate_workspace(InspectEstimateWorkspaceCommand {
            tender_id: harness.tender_id.clone(),
            basis_offset: 0,
            boq_candidate_cursor: None,
        })
        .expect("inspect after stale Basis completion");
    assert!(inspected.basis.is_none());
}

#[tokio::test]
async fn same_version_query_decision_terminalizes_a_basis_turn_that_never_observed_it() {
    let harness = Harness::new("record-extraction");
    active_production(&harness).await;
    let prepared = prepare_estimate_basis(&harness).await;
    let query_page = harness
        .host
        .inspect_tender_queries(InspectTenderQueriesCommand {
            tender_id: harness.tender_id.clone(),
            cursor: None,
            limit: 8,
        })
        .expect("inspect exact Query owner");
    let owner = query_page.owner_profiles.first().expect("Query owner");
    let unresolved = harness
        .host
        .create_tender_query(CreateTenderQueryCommand {
            tender_id: harness.tender_id.clone(),
            query_type: TenderQueryType::Ambiguity,
            question: "Does BOQ-001 require a late commercial qualification?".into(),
            ambiguity_or_gap: "The exact qualification decision is initially unresolved.".into(),
            owner_profile_id: owner.profile_id.clone(),
            owner_profile_version: owner.version,
            evidence: vec![prepared.calculation_evidence.clone()],
            affected_records: Vec::new(),
            affected_task_keys: vec!["*".into()],
            due_at: "2099-01-01T00:00:00.000Z".into(),
            material: false,
            release_blocking: false,
            proposed_treatments: vec![TenderQueryTreatmentProposalInput {
                treatment: TenderQueryTreatment::Qualification,
                rationale: "Qualify the exact BOQ row only if EITL approves it.".into(),
            }],
        })
        .expect("register unresolved Query before the Basis turn");

    harness.set_agent_scenario("cost-estimator-basis-unresolved-query-delayed");
    let host = harness.host.clone();
    let command = RunCostEstimatorBasisCommand {
        tender_id: harness.tender_id.clone(),
        quotation_evidence: vec![prepared.quotation_evidence],
        calculation_run_ids: vec![
            prepared.calculation_run_id,
            prepared.quotation_calculation_run_id,
            prepared.total_calculation_run_id,
        ],
    };
    let pending = tokio::spawn(async move { host.run_cost_estimator_basis(command).await });
    wait_for_fixture_path(&harness.codex.with_extension("cost-estimator-basis-waiting")).await;
    harness
        .host
        .decide_tender_query_treatment(DecideTenderQueryTreatmentCommand {
            tender_id: harness.tender_id.clone(),
            query_id: unresolved.query_id,
            query_version: unresolved.version,
            treatment: TenderQueryTreatment::Qualification,
            rationale: "EITL approves the exact qualification while the old view is in flight."
                .into(),
            treatment_details: "A successor turn must observe this authority before use.".into(),
            closes_query: true,
        })
        .expect("decide the same immutable Query version");
    fs::write(
        harness.codex.with_extension("cost-estimator-basis-release"),
        b"release",
    )
    .expect("release stale same-version Basis output");

    let result = pending
        .await
        .expect("join delayed same-version Basis turn")
        .expect("stale same-version completion remains inspectable");
    assert_eq!(result.run.state, AgentRunState::Failed);
    assert!(result.basis.is_none());
}

#[tokio::test]
async fn basis_of_estimate_is_complete_reconciled_independently_reviewed_and_exactly_approved() {
    let harness = Harness::new("record-extraction");
    active_production(&harness).await;
    let rule = harness
        .host
        .propose_boq_calculation_rule(ProposeBoqCalculationRuleCommand {
            tender_id: harness.tender_id.clone(),
            supported_rounding: vec![CalculationRoundingMode::MidpointAwayFromZero],
            change_rationale: "Establish the exact estimate arithmetic policy.".into(),
        })
        .expect("propose estimate Calculation Rule");
    harness.set_agent_scenario("calculation-rule-review");
    harness
        .host
        .run_calculation_rule_review(RunCalculationRuleReviewCommand {
            tender_id: harness.tender_id.clone(),
            rule_id: rule.rule_id.clone(),
            version: rule.version,
        })
        .await
        .expect("independently review estimate Calculation Rule");
    harness
        .host
        .approve_calculation_rule(ApproveCalculationRuleCommand {
            tender_id: harness.tender_id.clone(),
            rule_id: rule.rule_id,
            version: rule.version,
            manifest_sha256: rule.manifest_sha256,
            rationale: "Activate only the exact independently reviewed rule.".into(),
        })
        .expect("activate estimate Calculation Rule");

    let source = harness
        .host
        .inspect_document_register(&harness.tender_id)
        .expect("inspect estimate source")
        .documents
        .into_iter()
        .next()
        .expect("registered estimate source");
    let locations = harness
        .host
        .inspect_evidence(ParseSourceArtifactCommand {
            tender_id: harness.tender_id.clone(),
            artifact_id: source.artifact_id.clone(),
            version: source.version,
        })
        .expect("inspect estimate Evidence")
        .locations;
    let boq_location = locations.first().expect("BOQ row Evidence");
    let quote_location = locations.get(1).unwrap_or(boq_location);
    let quote_evidence = TenderEvidenceReference {
        artifact_id: source.artifact_id.clone(),
        version: source.version,
        ordinal: quote_location.ordinal,
    };
    let calculation_evidence = AgentTaskInputReference {
        kind: "source_evidence".into(),
        reference: format!("{}#{}", source.artifact_id, boq_location.ordinal),
        version: source.version,
    };
    let boq_evidence = import_exact_boq_table(&harness).await;
    let boq_calculation_evidence = boq_evidence
        .iter()
        .map(|reference| AgentTaskInputReference {
            kind: "source_evidence".into(),
            reference: format!("{}#{}", reference.artifact_id, reference.ordinal),
            version: reference.version,
        })
        .collect::<Vec<_>>();
    let quotation_calculation_evidence = std::iter::once(AgentTaskInputReference {
        kind: "source_evidence".into(),
        reference: format!("{}#{}", quote_evidence.artifact_id, quote_evidence.ordinal),
        version: quote_evidence.version,
    })
    .chain(boq_calculation_evidence.iter().cloned())
    .collect::<Vec<_>>();
    let scenario = harness
        .host
        .create_calculation_scenario(CreateCalculationScenarioCommand {
            tender_id: harness.tender_id.clone(),
            name: "BOQ account base".into(),
            quantity_unit: "mm".into(),
            rate_basis_unit: "m".into(),
            rate_currency: "USD".into(),
            exchange_rate: CalculationDecimalInput {
                state: CalculationInputState::Provided,
                value: Some("50".into()),
                evidence: vec![calculation_evidence.clone()],
            },
            exchange_rate_effective_date: Some("2026-08-01".into()),
            pricing_date: "2026-08-10".into(),
            exchange_rate_type: Some(ExchangeRateType::Spot),
            output_currency: "EGP".into(),
            precision: 2,
            rounding_mode: CalculationRoundingMode::MidpointAwayFromZero,
            rationale: "Bind the exact pricing date, currency, FX and rounding basis.".into(),
        })
        .expect("approve estimate Calculation Scenario");
    let calculated = run_cost_estimator_fixture_with_evidence(
        &harness,
        "cost-estimator-calculation",
        scenario.scenario_id.clone(),
        scenario.version,
        "BOQ-001 cable containment",
        &boq_calculation_evidence,
    )
    .await;
    let calculated = harness
        .host
        .approve_controlled_boq_calculation_run(ApproveControlledBoqCalculationRunCommand {
            tender_id: harness.tender_id.clone(),
            calculation_run_id: calculated.calculation_run_id,
            manifest_sha256: calculated.manifest_sha256,
            rationale: "Approve the exact BOQ row build-up and canonical total.".into(),
        })
        .expect("approve exact estimate Calculation Run");
    let quotation_calculation = run_cost_estimator_fixture_with_evidence(
        &harness,
        "cost-estimator-calculation",
        scenario.scenario_id.clone(),
        scenario.version,
        "Supplier quotation normalization for BOQ-001",
        &quotation_calculation_evidence,
    )
    .await;
    let quotation_calculation = harness
        .host
        .approve_controlled_boq_calculation_run(ApproveControlledBoqCalculationRunCommand {
            tender_id: harness.tender_id.clone(),
            calculation_run_id: quotation_calculation.calculation_run_id,
            manifest_sha256: quotation_calculation.manifest_sha256,
            rationale: "Approve the distinct quotation-normalization run.".into(),
        })
        .expect("approve quotation-normalization run");
    let comparison_total = run_cost_estimator_fixture(
        &harness,
        "cost-estimator-calculation",
        scenario.scenario_id,
        scenario.version,
        "Independent comparison total for BOQ-001",
        &calculation_evidence,
    )
    .await;
    let comparison_total = harness
        .host
        .approve_controlled_boq_calculation_run(ApproveControlledBoqCalculationRunCommand {
            tender_id: harness.tender_id.clone(),
            calculation_run_id: comparison_total.calculation_run_id,
            manifest_sha256: comparison_total.manifest_sha256,
            rationale: "Approve the distinct comparison-total run.".into(),
        })
        .expect("approve comparison-total run");

    let query_page = harness
        .host
        .inspect_tender_queries(InspectTenderQueriesCommand {
            tender_id: harness.tender_id.clone(),
            cursor: None,
            limit: 8,
        })
        .expect("inspect estimate Query Register");
    let owner = query_page
        .owner_profiles
        .first()
        .expect("approved Query owner");
    let assumption = harness
        .host
        .create_tender_query(CreateTenderQueryCommand {
            tender_id: harness.tender_id.clone(),
            query_type: TenderQueryType::MissingInformation,
            question: "Which productivity basis governs BOQ-001?".into(),
            ambiguity_or_gap: "The source does not state the installation productivity basis."
                .into(),
            owner_profile_id: owner.profile_id.clone(),
            owner_profile_version: owner.version,
            evidence: vec![calculation_evidence.clone()],
            affected_records: Vec::new(),
            affected_task_keys: vec!["*".into()],
            due_at: "2099-01-01T00:00:00.000Z".into(),
            material: false,
            release_blocking: false,
            proposed_treatments: vec![TenderQueryTreatmentProposalInput {
                treatment: TenderQueryTreatment::ApprovedAssumption,
                rationale:
                    "Use the stated conservative productivity basis for this estimate version."
                        .into(),
            }],
        })
        .expect("register material estimate assumption");
    let _assumption_decision = harness
        .host
        .decide_tender_query_treatment(DecideTenderQueryTreatmentCommand {
            tender_id: harness.tender_id.clone(),
            query_id: assumption.query_id.clone(),
            query_version: assumption.version,
            treatment: TenderQueryTreatment::ApprovedAssumption,
            rationale: "EITL approves this exact material estimating assumption.".into(),
            treatment_details: "Use one conservative crew productivity basis only for BOQ-001."
                .into(),
            closes_query: true,
        })
        .expect("approve exact material assumption");

    harness.set_agent_scenario("cost-estimator-basis");
    let proposed = harness
        .host
        .run_cost_estimator_basis(RunCostEstimatorBasisCommand {
            tender_id: harness.tender_id.clone(),
            quotation_evidence: vec![quote_evidence],
            calculation_run_ids: vec![
                calculated.calculation_run_id.clone(),
                quotation_calculation.calculation_run_id,
                comparison_total.calculation_run_id,
            ],
        })
        .await
        .expect("Cost Estimator publishes one exact Basis of Estimate");
    assert_eq!(
        proposed.run.state,
        AgentRunState::Completed,
        "{:#?}\nfixture error: {}",
        proposed.run,
        fs::read_to_string(harness.codex.with_extension("fixture-error"))
            .unwrap_or_else(|_| "none".into())
    );
    let basis = proposed.basis.expect("canonical Basis of Estimate");
    assert!(basis.complete);
    assert!(basis.reconciled);
    assert!(!basis.relied_upon);
    assert_eq!(basis.boq_rows.len(), 1);
    assert_eq!(
        basis.boq_rows[0].evidence.len(),
        2,
        "the Host carries the entire designated BOQ row, not one representative cell",
    );
    assert_eq!(
        basis.boq_rows[0]
            .evidence
            .iter()
            .cloned()
            .collect::<std::collections::HashSet<_>>(),
        boq_evidence
            .iter()
            .cloned()
            .collect::<std::collections::HashSet<_>>(),
        "unrelated parsed tables stay outside the exact designated BOQ inventory",
    );
    assert_eq!(basis.cbs_components.len(), 1);
    assert_eq!(basis.resource_build_ups.len(), 1);
    assert_eq!(basis.aggregate_calculation.inputs.len(), 1);
    assert!(!basis.aggregate_calculation.approved_for_reliance);
    assert_eq!(basis.quotations.len(), 1);
    assert_eq!(basis.material_assumptions.len(), 1);
    assert_eq!(
        harness
            .host
            .approve_basis_of_estimate(ApproveBasisOfEstimateCommand {
                tender_id: harness.tender_id.clone(),
                basis_id: basis.basis_id.clone(),
                version: basis.version,
                manifest_sha256: basis.manifest_sha256.clone(),
                rationale: "Review is mandatory before reliance.".into(),
            })
            .expect_err("Basis approval requires exact independent review")
            .code,
        TenderErrorCode::InvalidCommand
    );

    harness.set_agent_scenario("basis-of-estimate-review");
    let reviewed = harness
        .host
        .run_basis_of_estimate_review(RunBasisOfEstimateReviewCommand {
            tender_id: harness.tender_id.clone(),
            basis_id: basis.basis_id.clone(),
            version: basis.version,
        })
        .await
        .expect("independently reproduce and review exact estimate basis");
    assert_eq!(reviewed.run.state, AgentRunState::Completed);
    assert_ne!(
        reviewed.run.profile.profile_id,
        proposed.run.profile.profile_id
    );
    assert_eq!(
        reviewed.basis.review.as_ref().map(|review| review.outcome),
        Some(BasisOfEstimateReviewOutcome::Passed)
    );
    let approved = harness
        .host
        .approve_basis_of_estimate(ApproveBasisOfEstimateCommand {
            tender_id: harness.tender_id.clone(),
            basis_id: basis.basis_id,
            version: basis.version,
            manifest_sha256: basis.manifest_sha256,
            rationale: "EITL approves the exact reviewed Basis and bound material assumption."
                .into(),
        })
        .expect("approve exact Basis of Estimate");
    assert!(approved.relied_upon);
    assert!(approved.approval.is_some());
    assert!(approved.aggregate_calculation.approved_for_reliance);

    harness
        .host
        .create_tender_query(CreateTenderQueryCommand {
            tender_id: harness.tender_id.clone(),
            query_type: TenderQueryType::MissingInformation,
            question: "Which post-approval commercial qualification affects this estimate?".into(),
            ambiguity_or_gap:
                "A new applicable Query must stale the exact approved Query inventory.".into(),
            owner_profile_id: owner.profile_id.clone(),
            owner_profile_version: owner.version,
            evidence: vec![calculation_evidence],
            affected_records: Vec::new(),
            affected_task_keys: vec!["*".into()],
            due_at: "2099-01-01T00:00:00.000Z".into(),
            material: false,
            release_blocking: false,
            proposed_treatments: vec![TenderQueryTreatmentProposalInput {
                treatment: TenderQueryTreatment::Qualification,
                rationale: "Carry the late exact qualification into a successor Basis.".into(),
            }],
        })
        .expect("register post-approval estimating Query");
    let stale = harness
        .host
        .inspect_estimate_workspace(InspectEstimateWorkspaceCommand {
            tender_id: harness.tender_id.clone(),
            basis_offset: 0,
            boq_candidate_cursor: None,
        })
        .expect("inspect Query-staled Basis")
        .basis
        .expect("approved Basis remains inspectable");
    assert!(!stale.current);
    assert!(!stale.relied_upon);

    harness
        .host
        .close_tender(&harness.tender_id)
        .expect("close exact estimate before cold verification");
    let integrity = harness
        .host
        .inspect_tender_integrity(&harness.tender_id)
        .expect("cold-open Basis of Estimate integrity");
    assert_eq!(
        integrity.state,
        TenderIntegrityState::Ready,
        "{integrity:#?}"
    );

    harness
        .host
        .close_tender(&harness.tender_id)
        .expect("close verified Basis before authority corruption probes");
    let database = harness
        .application_home
        .join("tenders")
        .join(&harness.tender_id)
        .join("tender.sqlite");
    let connection = rusqlite::Connection::open(database).expect("open exact Basis store");
    let original_reviewer_grant: String = connection
        .query_row(
            "SELECT permission_grant_json FROM agent_runs WHERE run_id = ?1",
            [&reviewed.run.run_id],
            |row| row.get(0),
        )
        .expect("load exact reviewer grant");
    connection
        .execute_batch("DROP TRIGGER agent_runs_terminal_facts_no_rewrite;")
        .expect("disable Agent Run immutability only for corruption probes");
    connection
        .execute(
            "UPDATE agent_runs
             SET permission_grant_json = json_set(
               permission_grant_json, '$.network_allowed', json('true')
             )
             WHERE run_id = ?1",
            [&proposed.run.run_id],
        )
        .expect("corrupt Cost Estimator authority ceiling");
    let corrupted_author_host =
        QuantixHost::with_setup_platform(&harness.application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(
        ensure_quantix_setup(&corrupted_author_host).state,
        SetupState::Ready
    );
    assert_eq!(
        corrupted_author_host
            .inspect_tender_integrity(&harness.tender_id)
            .expect("inspect corrupted Cost Estimator grant")
            .state,
        TenderIntegrityState::RecoveryRequired,
    );
    connection
        .execute(
            "UPDATE agent_runs
             SET permission_grant_json = json_set(
               permission_grant_json, '$.network_allowed', json('false')
             )
             WHERE run_id = ?1",
            [&proposed.run.run_id],
        )
        .expect("restore exact Cost Estimator authority ceiling");
    connection
        .execute(
            "UPDATE agent_runs
             SET permission_grant_json = json_set(
               permission_grant_json, '$.data_views[0].sha256', ?2
             )
             WHERE run_id = ?1",
            rusqlite::params![reviewed.run.run_id, "0".repeat(64)],
        )
        .expect("corrupt exact Basis reviewer Data View");
    let corrupted_reviewer_host =
        QuantixHost::with_setup_platform(&harness.application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(
        ensure_quantix_setup(&corrupted_reviewer_host).state,
        SetupState::Ready,
    );
    assert_eq!(
        corrupted_reviewer_host
            .inspect_tender_integrity(&harness.tender_id)
            .expect("inspect corrupted Basis reviewer Data View")
            .state,
        TenderIntegrityState::RecoveryRequired,
    );
    connection
        .execute(
            "UPDATE agent_runs SET permission_grant_json = ?2 WHERE run_id = ?1",
            rusqlite::params![reviewed.run.run_id, original_reviewer_grant],
        )
        .expect("restore exact reviewer grant");
    connection
        .execute_batch("DROP TRIGGER estimate_aggregate_calculation_runs_no_update;")
        .expect("disable aggregate immutability only for corruption probe");
    connection
        .execute(
            "UPDATE estimate_aggregate_calculation_runs
             SET final_amount = '999.99' WHERE aggregate_run_id = ?1",
            [&approved.aggregate_calculation.aggregate_run_id],
        )
        .expect("corrupt canonical aggregate output");
    let corrupted_aggregate_host =
        QuantixHost::with_setup_platform(&harness.application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(
        ensure_quantix_setup(&corrupted_aggregate_host).state,
        SetupState::Ready,
    );
    assert_eq!(
        corrupted_aggregate_host
            .inspect_tender_integrity(&harness.tender_id)
            .expect("inspect corrupted canonical aggregate")
            .state,
        TenderIntegrityState::RecoveryRequired,
    );
}

#[tokio::test]
async fn boq_table_candidates_are_cursor_paged_without_hiding_later_tables() {
    let harness = Harness::new("record-extraction");
    let source = harness._root.path().join("paged-boq-candidates");
    fs::create_dir(&source).expect("paged BOQ source directory");
    for index in 0..17 {
        fs::write(
            source.join(format!("boq-{index:02}.pdf")),
            b"%PDF-1.7\nBOQ_TABLE\n%%EOF\n",
        )
        .expect("paged BOQ PDF fixture");
    }
    let imported = harness
        .host
        .import_tender_package(ImportTenderPackageCommand {
            tender_id: harness.tender_id.clone(),
            source_path: source.to_string_lossy().into_owned(),
        })
        .expect("import paged BOQ source");
    for document in &imported.documents {
        harness
            .host
            .parse_source_artifact(ParseSourceArtifactCommand {
                tender_id: harness.tender_id.clone(),
                artifact_id: document.artifact_id.clone(),
                version: document.version,
            })
            .await
            .expect("parse paged BOQ candidate");
    }

    let first = harness
        .host
        .inspect_estimate_workspace(InspectEstimateWorkspaceCommand {
            tender_id: harness.tender_id.clone(),
            basis_offset: 0,
            boq_candidate_cursor: None,
        })
        .expect("inspect first BOQ candidate page");
    assert_eq!(first.boq_table_candidates.len(), 16);
    let cursor = first
        .boq_table_candidate_next_cursor
        .expect("later BOQ candidate page");
    let first_keys = first
        .boq_table_candidates
        .iter()
        .map(|candidate| {
            (
                candidate.artifact_id.clone(),
                candidate.artifact_version,
                candidate.table_number,
            )
        })
        .collect::<std::collections::HashSet<_>>();
    let second = harness
        .host
        .inspect_estimate_workspace(InspectEstimateWorkspaceCommand {
            tender_id: harness.tender_id.clone(),
            basis_offset: 0,
            boq_candidate_cursor: Some(cursor),
        })
        .expect("inspect later BOQ candidate page");
    assert_eq!(second.boq_table_candidates.len(), 1);
    assert!(second.boq_table_candidate_next_cursor.is_none());
    assert!(second.boq_table_candidates.iter().all(|candidate| {
        !first_keys.contains(&(
            candidate.artifact_id.clone(),
            candidate.artifact_version,
            candidate.table_number,
        ))
    }));
}

#[tokio::test]
async fn explicit_headerless_boq_designation_retains_the_first_row_and_ignores_unselected_tables() {
    let harness = Harness::new("record-extraction");
    active_production(&harness).await;
    let prepared = prepare_estimate_basis(&harness).await;
    let headerless = import_exact_boq_table_named_with_headers(
        &harness,
        "estimate-headerless-boq",
        "headerless-boq.pdf",
        0,
    )
    .await;
    assert_eq!(headerless.len(), 4, "both headerless rows remain Evidence");

    harness.set_agent_scenario("cost-estimator-basis");
    let attempted = harness
        .host
        .run_cost_estimator_basis(RunCostEstimatorBasisCommand {
            tender_id: harness.tender_id.clone(),
            quotation_evidence: vec![prepared.quotation_evidence],
            calculation_run_ids: vec![
                prepared.calculation_run_id,
                prepared.quotation_calculation_run_id,
                prepared.total_calculation_run_id,
            ],
        })
        .await
        .expect("the Host exposes the expanded exact BOQ inventory before validation");
    assert_eq!(
        attempted.run.state,
        AgentRunState::Failed,
        "old calculations cannot cover the newly designated rows",
    );
    let row_kinds = attempted
        .run
        .task
        .exact_inputs
        .iter()
        .filter(|input| input.kind.starts_with("estimate_boq_row_"))
        .map(|input| input.kind.clone())
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(
        row_kinds.len(),
        3,
        "one prior BOQ row plus both headerless rows are derived exactly",
    );
    for reference in &headerless {
        assert!(attempted.run.task.exact_inputs.iter().any(|input| {
            input.kind.starts_with("estimate_boq_row_")
                && input.reference == format!("{}#{}", reference.artifact_id, reference.ordinal)
                && input.version == reference.version
        }));
    }

    harness
        .host
        .close_tender(&harness.tender_id)
        .expect("close headerless BOQ fixture");
    assert_eq!(
        harness
            .host
            .inspect_tender_integrity(&harness.tender_id)
            .expect("cold-verify explicit BOQ designations")
            .state,
        TenderIntegrityState::Ready,
    );
}

#[tokio::test]
async fn incomplete_basis_remains_visible_fails_independent_review_and_cannot_be_relied_upon() {
    let harness = Harness::new("record-extraction");
    active_production(&harness).await;
    let prepared = prepare_estimate_basis(&harness).await;

    harness.set_agent_scenario("cost-estimator-basis-incomplete");
    let proposed = harness
        .host
        .run_cost_estimator_basis(RunCostEstimatorBasisCommand {
            tender_id: harness.tender_id.clone(),
            quotation_evidence: vec![prepared.quotation_evidence.clone()],
            calculation_run_ids: vec![
                prepared.calculation_run_id.clone(),
                prepared.quotation_calculation_run_id.clone(),
                prepared.total_calculation_run_id.clone(),
            ],
        })
        .await
        .expect("Cost Estimator records the incomplete exact Basis");
    assert_eq!(proposed.run.state, AgentRunState::Completed);
    let basis = proposed
        .basis
        .expect("incomplete Basis remains inspectable");
    assert!(!basis.complete);
    assert!(basis.reconciled);
    assert_eq!(basis.blockers, vec!["basis_gaps"]);

    harness.set_agent_scenario("basis-of-estimate-review-failed");
    let reviewed = harness
        .host
        .run_basis_of_estimate_review(RunBasisOfEstimateReviewCommand {
            tender_id: harness.tender_id.clone(),
            basis_id: basis.basis_id.clone(),
            version: basis.version,
        })
        .await
        .expect("independent reviewer records exact blocking findings");
    assert_eq!(
        reviewed.basis.review.as_ref().map(|review| review.outcome),
        Some(BasisOfEstimateReviewOutcome::Failed)
    );
    assert_eq!(
        harness
            .host
            .approve_basis_of_estimate(ApproveBasisOfEstimateCommand {
                tender_id: harness.tender_id.clone(),
                basis_id: basis.basis_id,
                version: basis.version,
                manifest_sha256: basis.manifest_sha256,
                rationale: "An incomplete Basis must remain outside reliance.".into(),
            })
            .expect_err("EITL cannot approve a failed incomplete Basis")
            .code,
        TenderErrorCode::InvalidCommand
    );

    harness.set_agent_scenario("cost-estimator-basis-incomplete");
    let unchanged_successor = harness
        .host
        .run_cost_estimator_basis(RunCostEstimatorBasisCommand {
            tender_id: harness.tender_id.clone(),
            quotation_evidence: vec![prepared.quotation_evidence],
            calculation_run_ids: vec![
                prepared.calculation_run_id,
                prepared.quotation_calculation_run_id,
                prepared.total_calculation_run_id,
            ],
        })
        .await
        .expect("unchanged failed-review successor terminalizes attributably");
    assert_eq!(unchanged_successor.run.state, AgentRunState::Failed);
    assert!(unchanged_successor.basis.is_none());
    assert_eq!(
        unchanged_successor
            .run
            .failure
            .as_ref()
            .map(|failure| failure.category),
        Some(ProviderFailureCategory::OutputInvalid)
    );

    harness
        .host
        .close_tender(&harness.tender_id)
        .expect("close incomplete Basis before cold verification");
    assert_eq!(
        harness
            .host
            .inspect_tender_integrity(&harness.tender_id)
            .expect("cold-open incomplete Basis integrity")
            .state,
        TenderIntegrityState::Ready
    );
}

#[tokio::test]
async fn stale_unreviewed_basis_can_be_superseded_with_exact_source_lineage() {
    let harness = Harness::new("record-extraction");
    active_production(&harness).await;
    let prepared = prepare_estimate_basis(&harness).await;
    let command = || RunCostEstimatorBasisCommand {
        tender_id: harness.tender_id.clone(),
        quotation_evidence: vec![prepared.quotation_evidence.clone()],
        calculation_run_ids: vec![
            prepared.calculation_run_id.clone(),
            prepared.quotation_calculation_run_id.clone(),
            prepared.total_calculation_run_id.clone(),
        ],
    };

    harness.set_agent_scenario("cost-estimator-basis");
    let first = harness
        .host
        .run_cost_estimator_basis(command())
        .await
        .expect("publish unreviewed Basis before BOQ replacement")
        .basis
        .expect("first Basis");
    assert!(first.review.is_none());

    let replacement =
        import_exact_boq_table_named(&harness, "estimate-boq-replacement", "replacement-boq.pdf")
            .await;
    let prior_boq = prepared
        .boq_evidence
        .first()
        .expect("prior BOQ row Evidence");
    let replacement_boq = replacement.first().expect("replacement BOQ row Evidence");
    harness
        .host
        .confirm_source_relationship(ConfirmSourceRelationshipCommand {
            tender_id: harness.tender_id.clone(),
            prior_artifact_id: prior_boq.artifact_id.clone(),
            prior_version: prior_boq.version,
            replacement_artifact_id: replacement_boq.artifact_id.clone(),
            replacement_version: replacement_boq.version,
            relationship_kind: SourceRelationshipKind::Replacement,
        })
        .expect("confirm exact BOQ replacement");

    harness.set_agent_scenario("cost-estimator-basis");
    let stale_calculation_attempt = harness
        .host
        .run_cost_estimator_basis(command())
        .await
        .expect("stale Calculation Runs terminalize without corrupting the Tender");
    assert_eq!(stale_calculation_attempt.run.state, AgentRunState::Failed);
    assert!(stale_calculation_attempt.basis.is_none());

    let replacement_calculation_evidence = replacement
        .iter()
        .map(|reference| AgentTaskInputReference {
            kind: "source_evidence".into(),
            reference: format!("{}#{}", reference.artifact_id, reference.ordinal),
            version: reference.version,
        })
        .collect::<Vec<_>>();
    let replacement_row_run = run_cost_estimator_fixture_with_evidence(
        &harness,
        "cost-estimator-calculation",
        prepared.scenario_id.clone(),
        prepared.scenario_version,
        "BOQ-001 replacement cable containment",
        &replacement_calculation_evidence,
    )
    .await;
    let replacement_row_run = harness
        .host
        .approve_controlled_boq_calculation_run(ApproveControlledBoqCalculationRunCommand {
            tender_id: harness.tender_id.clone(),
            calculation_run_id: replacement_row_run.calculation_run_id,
            manifest_sha256: replacement_row_run.manifest_sha256,
            rationale: "Approve only the replacement-bound BOQ build-up.".into(),
        })
        .expect("approve replacement BOQ Calculation Run");
    let replacement_quote_evidence = std::iter::once(AgentTaskInputReference {
        kind: "source_evidence".into(),
        reference: format!(
            "{}#{}",
            prepared.quotation_evidence.artifact_id, prepared.quotation_evidence.ordinal
        ),
        version: prepared.quotation_evidence.version,
    })
    .chain(replacement_calculation_evidence.iter().cloned())
    .collect::<Vec<_>>();
    let replacement_quote_run = run_cost_estimator_fixture_with_evidence(
        &harness,
        "cost-estimator-calculation",
        prepared.scenario_id.clone(),
        prepared.scenario_version,
        "Supplier quotation normalization for replacement BOQ-001",
        &replacement_quote_evidence,
    )
    .await;
    let replacement_quote_run = harness
        .host
        .approve_controlled_boq_calculation_run(ApproveControlledBoqCalculationRunCommand {
            tender_id: harness.tender_id.clone(),
            calculation_run_id: replacement_quote_run.calculation_run_id,
            manifest_sha256: replacement_quote_run.manifest_sha256,
            rationale: "Approve only the replacement-row quotation normalization.".into(),
        })
        .expect("approve replacement quotation Calculation Run");

    harness.set_agent_scenario("cost-estimator-basis");
    let successor = harness
        .host
        .run_cost_estimator_basis(RunCostEstimatorBasisCommand {
            tender_id: harness.tender_id.clone(),
            quotation_evidence: vec![prepared.quotation_evidence.clone()],
            calculation_run_ids: vec![
                replacement_row_run.calculation_run_id,
                replacement_quote_run.calculation_run_id,
                prepared.total_calculation_run_id.clone(),
            ],
        })
        .await
        .expect("publish exact successor to stale unreviewed Basis")
        .basis
        .expect("successor Basis");
    assert_eq!(successor.version, 2);
    assert_eq!(
        successor.supersedes_basis_manifest_sha256.as_deref(),
        Some(first.manifest_sha256.as_str())
    );
    assert!(successor.remediates_review_manifest_sha256.is_none());
    assert!(successor.current);

    harness
        .host
        .close_tender(&harness.tender_id)
        .expect("close superseded Basis before cold verification");
    assert_eq!(
        harness
            .host
            .inspect_tender_integrity(&harness.tender_id)
            .expect("cold-open superseded Basis lineage")
            .state,
        TenderIntegrityState::Ready
    );
}

#[tokio::test]
async fn quotation_scope_mismatch_terminalizes_the_agent_without_publishing_a_basis() {
    let harness = Harness::new("record-extraction");
    active_production(&harness).await;
    let prepared = prepare_estimate_basis(&harness).await;

    harness.set_agent_scenario("cost-estimator-basis-quote-scope-mismatch");
    let result = harness
        .host
        .run_cost_estimator_basis(RunCostEstimatorBasisCommand {
            tender_id: harness.tender_id.clone(),
            quotation_evidence: vec![prepared.quotation_evidence],
            calculation_run_ids: vec![
                prepared.calculation_run_id,
                prepared.quotation_calculation_run_id,
                prepared.total_calculation_run_id,
            ],
        })
        .await
        .expect("invalid quote scope is an attributable terminal Agent outcome");
    assert_eq!(result.run.state, AgentRunState::Failed);
    assert!(result.basis.is_none());
    assert_eq!(
        result.run.failure.as_ref().map(|failure| failure.category),
        Some(ProviderFailureCategory::OutputInvalid)
    );

    harness
        .host
        .close_tender(&harness.tender_id)
        .expect("close rejected quote-scope run before cold verification");
    assert_eq!(
        harness
            .host
            .inspect_tender_integrity(&harness.tender_id)
            .expect("cold-open rejected quote-scope run integrity")
            .state,
        TenderIntegrityState::Ready
    );
}

#[tokio::test]
async fn quotation_normalization_must_consume_the_exact_quote_and_every_covered_boq_row() {
    let harness = Harness::new("record-extraction");
    active_production(&harness).await;
    let prepared = prepare_estimate_basis(&harness).await;
    let quote_only_evidence = AgentTaskInputReference {
        kind: "source_evidence".into(),
        reference: format!(
            "{}#{}",
            prepared.quotation_evidence.artifact_id, prepared.quotation_evidence.ordinal
        ),
        version: prepared.quotation_evidence.version,
    };
    let quote_only = run_cost_estimator_fixture(
        &harness,
        "cost-estimator-calculation",
        prepared.scenario_id,
        prepared.scenario_version,
        "Supplier quotation normalization without covered BOQ row Evidence",
        &quote_only_evidence,
    )
    .await;
    let quote_only = harness
        .host
        .approve_controlled_boq_calculation_run(ApproveControlledBoqCalculationRunCommand {
            tender_id: harness.tender_id.clone(),
            calculation_run_id: quote_only.calculation_run_id,
            manifest_sha256: quote_only.manifest_sha256,
            rationale: "Approve the exact run while preserving its deliberately narrow scope."
                .into(),
        })
        .expect("approve quote-only Calculation Run");

    harness.set_agent_scenario("cost-estimator-basis");
    let attempted = harness
        .host
        .run_cost_estimator_basis(RunCostEstimatorBasisCommand {
            tender_id: harness.tender_id,
            quotation_evidence: vec![prepared.quotation_evidence],
            calculation_run_ids: vec![
                prepared.calculation_run_id,
                quote_only.calculation_run_id,
                prepared.total_calculation_run_id,
            ],
        })
        .await
        .expect("invalid quote provenance terminalizes the Agent Run");
    assert_eq!(attempted.run.state, AgentRunState::Failed);
    assert!(attempted.basis.is_none());
}

#[tokio::test]
async fn allowance_without_an_exact_allowance_treatment_and_controlled_build_up_is_rejected() {
    let harness = Harness::new("record-extraction");
    active_production(&harness).await;
    let prepared = prepare_estimate_basis(&harness).await;
    harness.set_agent_scenario("cost-estimator-basis-uncontrolled-allowance");
    let attempted = harness
        .host
        .run_cost_estimator_basis(RunCostEstimatorBasisCommand {
            tender_id: harness.tender_id,
            quotation_evidence: vec![prepared.quotation_evidence],
            calculation_run_ids: vec![
                prepared.calculation_run_id,
                prepared.quotation_calculation_run_id,
                prepared.total_calculation_run_id,
            ],
        })
        .await
        .expect("uncontrolled allowance terminalizes the Agent Run");
    assert_eq!(attempted.run.state, AgentRunState::Failed);
    assert!(attempted.basis.is_none());
}

#[tokio::test]
async fn approved_allowance_roundtrips_with_its_exact_query_and_controlled_build_up() {
    let harness = Harness::new("record-extraction");
    active_production(&harness).await;
    let prepared = prepare_estimate_basis(&harness).await;
    let query_page = harness
        .host
        .inspect_tender_queries(InspectTenderQueriesCommand {
            tender_id: harness.tender_id.clone(),
            cursor: None,
            limit: 8,
        })
        .expect("inspect approved Query owner for the allowance");
    let owner = query_page.owner_profiles.first().expect("Query owner");
    let allowance_evidence = prepared
        .boq_evidence
        .first()
        .map(|reference| AgentTaskInputReference {
            kind: "source_evidence".into(),
            reference: format!("{}#{}", reference.artifact_id, reference.ordinal),
            version: reference.version,
        })
        .expect("exact BOQ allowance Evidence");
    let allowance = harness
        .host
        .create_tender_query(CreateTenderQueryCommand {
            tender_id: harness.tender_id.clone(),
            query_type: TenderQueryType::MissingInformation,
            question: "Which exact risk allowance governs BOQ-001?".into(),
            ambiguity_or_gap: "The BOQ evidence requires an explicit controlled risk allowance."
                .into(),
            owner_profile_id: owner.profile_id.clone(),
            owner_profile_version: owner.version,
            evidence: vec![allowance_evidence],
            affected_records: Vec::new(),
            affected_task_keys: vec!["*".into()],
            due_at: "2099-01-01T00:00:00.000Z".into(),
            material: false,
            release_blocking: false,
            proposed_treatments: vec![TenderQueryTreatmentProposalInput {
                treatment: TenderQueryTreatment::Allowance,
                rationale: "Carry one evidence-bound risk allowance in the controlled build-up."
                    .into(),
            }],
        })
        .expect("register exact risk-allowance Query");
    harness
        .host
        .decide_tender_query_treatment(DecideTenderQueryTreatmentCommand {
            tender_id: harness.tender_id.clone(),
            query_id: allowance.query_id,
            query_version: allowance.version,
            treatment: TenderQueryTreatment::Allowance,
            rationale: "EITL approves the exact evidence-bound risk allowance.".into(),
            treatment_details: "Use only the controlled BOQ-001 build-up and Calculation Run."
                .into(),
            closes_query: true,
        })
        .expect("approve exact risk allowance");

    harness.set_agent_scenario("cost-estimator-basis-allowance");
    let published = harness
        .host
        .run_cost_estimator_basis(RunCostEstimatorBasisCommand {
            tender_id: harness.tender_id.clone(),
            quotation_evidence: vec![prepared.quotation_evidence],
            calculation_run_ids: vec![
                prepared.calculation_run_id,
                prepared.quotation_calculation_run_id,
                prepared.total_calculation_run_id,
            ],
        })
        .await
        .expect("publish controlled allowance Basis");
    assert_eq!(
        published.run.state,
        AgentRunState::Completed,
        "{:#?}",
        published.run
    );
    let basis = published.basis.expect("controlled allowance Basis");
    assert!(basis.complete);
    assert!(basis.reconciled);
    assert_eq!(basis.allowances.len(), 1);

    harness.set_agent_scenario("basis-of-estimate-review");
    let reviewed = harness
        .host
        .run_basis_of_estimate_review(RunBasisOfEstimateReviewCommand {
            tender_id: harness.tender_id.clone(),
            basis_id: basis.basis_id.clone(),
            version: basis.version,
        })
        .await
        .expect("independently review the exact allowance authority");
    assert_eq!(
        reviewed.run.state,
        AgentRunState::Completed,
        "{:#?}",
        reviewed.run
    );
    assert_eq!(
        reviewed
            .basis
            .review
            .expect("exact allowance review")
            .outcome,
        BasisOfEstimateReviewOutcome::Passed
    );

    let reloaded = harness
        .host
        .inspect_estimate_workspace(InspectEstimateWorkspaceCommand {
            tender_id: harness.tender_id.clone(),
            basis_offset: 0,
            boq_candidate_cursor: None,
        })
        .expect("reload controlled allowance Basis")
        .basis
        .expect("controlled allowance Basis remains visible");
    assert_eq!(reloaded.manifest_sha256, basis.manifest_sha256);
    assert_eq!(reloaded.allowances.len(), 1);

    harness
        .host
        .close_tender(&harness.tender_id)
        .expect("close controlled allowance Tender before cold verification");
    let integrity = harness
        .host
        .inspect_tender_integrity(&harness.tender_id)
        .expect("cold-open controlled allowance integrity");
    assert_eq!(
        integrity.state,
        TenderIntegrityState::Ready,
        "{integrity:#?}"
    );
}

#[tokio::test]
async fn unresolved_query_keeps_the_exact_basis_incomplete_and_outside_reliance() {
    let harness = Harness::new("record-extraction");
    active_production(&harness).await;
    let prepared = prepare_estimate_basis(&harness).await;
    let query_page = harness
        .host
        .inspect_tender_queries(InspectTenderQueriesCommand {
            tender_id: harness.tender_id.clone(),
            cursor: None,
            limit: 8,
        })
        .expect("inspect Query owners for unresolved estimate input");
    let owner = query_page
        .owner_profiles
        .first()
        .expect("approved Query owner");
    let unresolved = harness
        .host
        .create_tender_query(CreateTenderQueryCommand {
            tender_id: harness.tender_id.clone(),
            query_type: TenderQueryType::Ambiguity,
            question: "Does the quoted scope include the BOQ-001 support steel?".into(),
            ambiguity_or_gap: "The quote and BOQ descriptions do not resolve support-steel scope."
                .into(),
            owner_profile_id: owner.profile_id.clone(),
            owner_profile_version: owner.version,
            evidence: vec![prepared.calculation_evidence.clone()],
            affected_records: Vec::new(),
            affected_task_keys: vec!["*".into()],
            due_at: "2099-01-01T00:00:00.000Z".into(),
            material: false,
            release_blocking: false,
            proposed_treatments: vec![TenderQueryTreatmentProposalInput {
                treatment: TenderQueryTreatment::Qualification,
                rationale: "Qualify BOQ-001 only after the Manager chooses the exact treatment."
                    .into(),
            }],
        })
        .expect("register unresolved estimate Query");

    harness.set_agent_scenario("cost-estimator-basis-unresolved-query");
    let proposed = harness
        .host
        .run_cost_estimator_basis(RunCostEstimatorBasisCommand {
            tender_id: harness.tender_id.clone(),
            quotation_evidence: vec![prepared.quotation_evidence],
            calculation_run_ids: vec![
                prepared.calculation_run_id,
                prepared.quotation_calculation_run_id,
                prepared.total_calculation_run_id,
            ],
        })
        .await
        .expect("Cost Estimator records query-qualified Basis");
    assert_eq!(proposed.run.state, AgentRunState::Completed);
    let basis = proposed.basis.expect("unresolved Basis remains visible");
    assert!(!basis.complete);
    assert!(basis.reconciled);
    assert_eq!(basis.blockers, vec!["unresolved_affected_query"]);
    assert_eq!(
        basis.material_assumptions.len(),
        1,
        "the prior exact approved assumption remains accounted while the new Query stays unresolved",
    );
    assert_eq!(
        harness
            .host
            .approve_basis_of_estimate(ApproveBasisOfEstimateCommand {
                tender_id: harness.tender_id.clone(),
                basis_id: basis.basis_id.clone(),
                version: basis.version,
                manifest_sha256: basis.manifest_sha256.clone(),
                rationale: "Unresolved estimate Queries cannot support reliance.".into(),
            })
            .expect_err("unresolved Query blocks exact Basis approval")
            .code,
        TenderErrorCode::InvalidCommand
    );

    harness
        .host
        .decide_tender_query_treatment(DecideTenderQueryTreatmentCommand {
            tender_id: harness.tender_id.clone(),
            query_id: unresolved.query_id,
            query_version: unresolved.version,
            treatment: TenderQueryTreatment::Qualification,
            rationale: "EITL qualifies the exact support-steel scope after Basis publication."
                .into(),
            treatment_details: "Carry the qualification into a successor Basis only.".into(),
            closes_query: true,
        })
        .expect("decide the exact previously unresolved Query");
    let stale = harness
        .host
        .inspect_estimate_workspace(InspectEstimateWorkspaceCommand {
            tender_id: harness.tender_id.clone(),
            basis_offset: 0,
            boq_candidate_cursor: None,
        })
        .expect("inspect Basis after same-version Query decision")
        .basis
        .expect("historical unresolved Basis remains visible");
    assert!(!stale.current);
    assert!(!stale.relied_upon);
    harness
        .host
        .close_tender(&harness.tender_id)
        .expect("close same-version Query decision Tender");
    assert_eq!(
        harness
            .host
            .inspect_tender_integrity(&harness.tender_id)
            .expect("cold-open historical unresolved Query snapshot")
            .state,
        TenderIntegrityState::Ready
    );
}

#[tokio::test]
async fn calculation_reconciliation_mismatch_is_visible_review_failed_and_not_reliable() {
    let harness = Harness::new("record-extraction");
    active_production(&harness).await;
    let prepared = prepare_estimate_basis(&harness).await;
    let zero_total = run_cost_estimator_fixture(
        &harness,
        "cost-estimator-calculation-zero",
        prepared.scenario_id.clone(),
        prepared.scenario_version,
        "Deliberately mismatched canonical estimate total",
        &prepared.calculation_evidence,
    )
    .await;
    let zero_total = harness
        .host
        .approve_controlled_boq_calculation_run(ApproveControlledBoqCalculationRunCommand {
            tender_id: harness.tender_id.clone(),
            calculation_run_id: zero_total.calculation_run_id,
            manifest_sha256: zero_total.manifest_sha256,
            rationale: "Approve the exact zero-value control run so reconciliation must detect the mismatch.".into(),
        })
        .expect("approve exact mismatched total Calculation Run");

    harness.set_agent_scenario("cost-estimator-basis-reconciliation-mismatch");
    let proposed = harness
        .host
        .run_cost_estimator_basis(RunCostEstimatorBasisCommand {
            tender_id: harness.tender_id.clone(),
            quotation_evidence: vec![prepared.quotation_evidence],
            calculation_run_ids: vec![
                prepared.calculation_run_id,
                prepared.quotation_calculation_run_id,
                zero_total.calculation_run_id,
            ],
        })
        .await
        .expect("Cost Estimator records the unreconciled exact Basis");
    assert_eq!(proposed.run.state, AgentRunState::Completed);
    let basis = proposed
        .basis
        .expect("unreconciled Basis remains inspectable");
    assert!(!basis.complete);
    assert!(!basis.reconciled);
    assert_eq!(basis.blockers, vec!["calculation_reconciliation"]);

    harness.set_agent_scenario("basis-of-estimate-review-failed");
    let reviewed = harness
        .host
        .run_basis_of_estimate_review(RunBasisOfEstimateReviewCommand {
            tender_id: harness.tender_id.clone(),
            basis_id: basis.basis_id.clone(),
            version: basis.version,
        })
        .await
        .expect("independent review preserves the reconciliation finding");
    assert_eq!(
        reviewed.basis.review.as_ref().map(|review| review.outcome),
        Some(BasisOfEstimateReviewOutcome::Failed)
    );
    assert_eq!(
        harness
            .host
            .approve_basis_of_estimate(ApproveBasisOfEstimateCommand {
                tender_id: harness.tender_id,
                basis_id: basis.basis_id,
                version: basis.version,
                manifest_sha256: basis.manifest_sha256,
                rationale: "An unreconciled Basis cannot be relied upon.".into(),
            })
            .expect_err("reconciliation mismatch blocks exact Basis approval")
            .code,
        TenderErrorCode::InvalidCommand
    );
}

#[tokio::test]
async fn missing_boq_rate_is_explicit_query_linked_and_cannot_support_reliance() {
    let harness = Harness::new("record-extraction");
    active_production(&harness).await;
    let prepared = prepare_estimate_basis(&harness).await;
    let query_page = harness
        .host
        .inspect_tender_queries(InspectTenderQueriesCommand {
            tender_id: harness.tender_id.clone(),
            cursor: None,
            limit: 8,
        })
        .expect("inspect Query owners for the missing rate");
    let owner = query_page
        .owner_profiles
        .first()
        .expect("approved Query owner");
    let _missing_rate = harness
        .host
        .create_tender_query(CreateTenderQueryCommand {
            tender_id: harness.tender_id.clone(),
            query_type: TenderQueryType::MissingInformation,
            question: "What approved rate governs BOQ-001?".into(),
            ambiguity_or_gap: "BOQ-001 has no attributable approved rate or build-up.".into(),
            owner_profile_id: owner.profile_id.clone(),
            owner_profile_version: owner.version,
            evidence: vec![prepared.calculation_evidence.clone()],
            affected_records: Vec::new(),
            affected_task_keys: vec!["*".into()],
            due_at: "2099-01-01T00:00:00.000Z".into(),
            material: false,
            release_blocking: false,
            proposed_treatments: vec![TenderQueryTreatmentProposalInput {
                treatment: TenderQueryTreatment::ExternalRfiDrafting,
                rationale: "Obtain an attributable rate before pricing BOQ-001.".into(),
            }],
        })
        .expect("register exact missing-rate Query");

    let zero_total = run_cost_estimator_fixture(
        &harness,
        "cost-estimator-calculation-zero",
        prepared.scenario_id.clone(),
        prepared.scenario_version,
        "Exact zero comparison total for an entirely unpriced BOQ inventory",
        &prepared.calculation_evidence,
    )
    .await;
    let zero_total = harness
        .host
        .approve_controlled_boq_calculation_run(ApproveControlledBoqCalculationRunCommand {
            tender_id: harness.tender_id.clone(),
            calculation_run_id: zero_total.calculation_run_id,
            manifest_sha256: zero_total.manifest_sha256,
            rationale: "Approve the zero comparison total while the BOQ rate remains missing."
                .into(),
        })
        .expect("approve zero missing-rate comparison total");

    harness.set_agent_scenario("cost-estimator-basis-missing-rate");
    let proposed = harness
        .host
        .run_cost_estimator_basis(RunCostEstimatorBasisCommand {
            tender_id: harness.tender_id.clone(),
            quotation_evidence: vec![prepared.quotation_evidence],
            calculation_run_ids: vec![
                prepared.quotation_calculation_run_id,
                zero_total.calculation_run_id,
            ],
        })
        .await
        .expect("Cost Estimator records explicit missing-rate disposition");
    assert_eq!(proposed.run.state, AgentRunState::Completed);
    let basis = proposed
        .basis
        .expect("missing-rate Basis remains inspectable");
    assert!(!basis.complete);
    assert!(basis.reconciled);
    assert_eq!(
        basis.blockers,
        vec!["boq_row_missing", "unresolved_affected_query"]
    );
    let missing = basis
        .boq_rows
        .iter()
        .find(|row| row.disposition == BoqRowDisposition::Missing)
        .expect("the exact BOQ row has an explicit missing disposition");
    assert!(missing.calculation_run_id.is_none());
    assert_eq!(missing.affected_queries.len(), 1);
    assert_eq!(
        harness
            .host
            .approve_basis_of_estimate(ApproveBasisOfEstimateCommand {
                tender_id: harness.tender_id,
                basis_id: basis.basis_id,
                version: basis.version,
                manifest_sha256: basis.manifest_sha256,
                rationale: "A BOQ row without an approved rate cannot support reliance.".into(),
            })
            .expect_err("missing rate blocks exact Basis approval")
            .code,
        TenderErrorCode::InvalidCommand
    );
}

fn approval_command(
    harness: &Harness,
    package: &BidDecisionPackageInspection,
    decision: BidDecisionApprovalDecision,
) -> DecideBidDecisionPackageCommand {
    DecideBidDecisionPackageCommand {
        tender_id: harness.tender_id.clone(),
        package_id: package.package_id.clone(),
        version: package.version,
        manifest_sha256: package.manifest_sha256.clone(),
        decision,
        rationale:
            "Tendering Manager reviewed the exact package, Evidence, findings, and consequences."
                .into(),
        conditions: vec!["Work Plan approval remains mandatory before production.".into()],
        exceptions: vec!["No exception grants production authority.".into()],
        required_rework: if decision == BidDecisionApprovalDecision::Return {
            vec!["Resolve the named package gaps and publish a successor version.".into()]
        } else {
            Vec::new()
        },
    }
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
    assert!(
        reopened.approval.is_none(),
        "review cannot approve a package"
    );
}

#[tokio::test]
async fn tendering_manager_accepts_one_exact_package_and_advances_to_tender_planning() {
    let harness = Harness::new("record-extraction");
    let package = ready_package(&harness).await;
    assert_eq!(package.lifecycle_phase, TenderLifecyclePhase::BidDecision);
    assert!(package.change_summary.added_record_count > 0);

    let result = harness
        .host
        .decide_bid_decision_package(approval_command(
            &harness,
            &package,
            BidDecisionApprovalDecision::Accept,
        ))
        .expect("accept exact independently reviewed package");
    assert_eq!(result.approval.decided_by, "engineer_user");
    assert_eq!(result.approval.acting_role, "tendering_manager");
    assert_eq!(
        result.approval.lifecycle_after,
        TenderLifecyclePhase::TenderPlanning
    );
    assert!(result.approval.evidence_count > 0);
    assert_eq!(result.package.approval, Some(result.approval.clone()));
    assert_eq!(
        result.package.lifecycle_phase,
        TenderLifecyclePhase::TenderPlanning
    );
    assert!(!result.package.decision_gate_ready);
    assert_eq!(
        harness
            .host
            .open_tender(&harness.tender_id)
            .expect("inspect lifecycle after Proceed")
            .lifecycle_phase,
        TenderLifecyclePhase::TenderPlanning
    );
    let audit_after_approval = harness
        .host
        .open_tender(&harness.tender_id)
        .expect("inspect audit count after Proceed")
        .audit_event_count;
    assert_eq!(
        harness
            .host
            .revise_tender(ReviseTenderCommand {
                tender_id: harness.tender_id.clone(),
                name: "Attempted post-Proceed mutation".into(),
            })
            .expect_err("Proceed closes the pre-bid writer plane")
            .code,
        TenderErrorCode::InvalidCommand
    );
    assert_eq!(
        harness
            .host
            .open_tender(&harness.tender_id)
            .expect("inspect audited lifecycle denial")
            .audit_event_count,
        audit_after_approval + 1
    );

    assert_eq!(
        harness
            .host
            .decide_bid_decision_package(approval_command(
                &harness,
                &package,
                BidDecisionApprovalDecision::Accept,
            ))
            .expect_err("double submission cannot create another Approval Record")
            .code,
        TenderErrorCode::InvalidCommand
    );
    let history = harness
        .host
        .inspect_bid_decision_approval_history(InspectBidDecisionApprovalHistoryCommand {
            tender_id: harness.tender_id.clone(),
            before_sequence: None,
            limit: 10,
        })
        .expect("inspect immutable decision history");
    assert_eq!(history.approvals, vec![result.approval.clone()]);

    let cold_host =
        QuantixHost::with_setup_platform(&harness.application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&cold_host).state, SetupState::Ready);
    let integrity = cold_host
        .inspect_tender_integrity(&harness.tender_id)
        .expect("cold-open Approval Record integrity");
    assert_eq!(
        integrity.state,
        TenderIntegrityState::Ready,
        "{integrity:#?}"
    );
    let reopened = cold_host
        .inspect_current_bid_decision_package(&harness.tender_id)
        .expect("inspect approved package")
        .expect("package remains preserved");
    assert_eq!(reopened.approval, Some(result.approval));
    assert_eq!(
        reopened.lifecycle_phase,
        TenderLifecyclePhase::TenderPlanning
    );
    drop(cold_host);
    harness
        .host
        .close_tender(&harness.tender_id)
        .expect("close Tender before Approval Record corruption injection");
    let database = harness
        .application_home
        .join("tenders")
        .join(&harness.tender_id)
        .join("tender.sqlite");
    rusqlite::Connection::open(database)
        .expect("open Tender Store")
        .execute_batch(
            "DROP TRIGGER bid_decision_approval_records_no_update;
             UPDATE bid_decision_approval_records
             SET approval_sha256 = 'ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff';",
        )
        .expect("corrupt immutable Approval Record hash");
    assert_eq!(
        harness
            .host
            .inspect_tender_integrity(&harness.tender_id)
            .expect("inspect corrupted Approval Record")
            .state,
        TenderIntegrityState::RecoveryRequired
    );
}

#[tokio::test]
async fn material_change_invalidates_proceed_and_reopens_an_exact_successor_path() {
    let harness = Harness::new("record-extraction");
    let package = ready_package(&harness).await;
    let accepted = harness
        .host
        .decide_bid_decision_package(approval_command(
            &harness,
            &package,
            BidDecisionApprovalDecision::Accept,
        ))
        .expect("accept exact package");
    let first_plan = harness
        .host
        .compose_tender_office(ComposeTenderOfficeCommand {
            tender_id: harness.tender_id.clone(),
        })
        .expect("compose Work Plan for the accepted package");
    let approved_plan = harness
        .host
        .decide_work_plan_proposal(DecideWorkPlanProposalCommand {
            tender_id: harness.tender_id.clone(),
            plan_id: first_plan.plan_id.clone(),
            version: first_plan.version,
            decision: WorkPlanDecision::Approve,
            rationale: "Approve production only for the exact accepted package.".into(),
        })
        .expect("approve exact initial Work Plan");
    assert!(approved_plan
        .profiles
        .iter()
        .all(|binding| binding.status == AgentProfileStatus::Proposed));
    harness
        .host
        .activate_tender_production(ActivateTenderProductionCommand {
            tender_id: harness.tender_id.clone(),
            plan_id: approved_plan.plan_id.clone(),
            plan_version: approved_plan.version,
            plan_manifest_sha256: approved_plan.manifest_sha256.clone(),
        })
        .expect("activate exact initial Work Plan");
    let audit_before_unproven_invalidation = harness
        .host
        .open_tender(&harness.tender_id)
        .expect("inspect audit count before invalidation denial")
        .audit_event_count;
    assert_eq!(
        harness
            .host
            .invalidate_bid_decision_approval(InvalidateBidDecisionApprovalCommand {
                tender_id: harness.tender_id.clone(),
                approval_id: accepted.approval.approval_id.clone(),
                approval_sha256: accepted.approval.approval_sha256.clone(),
                material_change_summary:
                    "A material addendum changes the commercial and delivery basis.".into(),
                affected_areas: vec!["commercial_terms".into(), "delivery_plan".into()],
            })
            .expect_err("an assertion without an exact changed dependency cannot reopen Proceed")
            .code,
        TenderErrorCode::InvalidCommand
    );
    assert_eq!(
        harness
            .host
            .open_tender(&harness.tender_id)
            .expect("inspect audited invalidation denial")
            .audit_event_count,
        audit_before_unproven_invalidation + 1
    );
    let current_records = inspect_all_records(&harness.host, &harness.tender_id);
    let mut evidence = current_records
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
    harness.set_agent_scenario("record-extraction-expanded");
    harness
        .host
        .run_tender_record_extraction(RunTenderRecordExtractionCommand {
            tender_id: harness.tender_id.clone(),
            evidence,
            authorities: Vec::new(),
        })
        .await
        .expect("an exact material dependency can be registered after Proceed");
    let stale_production = harness
        .host
        .inspect_tender_production(&harness.tender_id)
        .expect("inspect stale Active Production")
        .expect("Active Production remains attributable until invalidation");
    let ready_task = stale_production
        .tasks
        .iter()
        .find(|task| task.state == ProductionTaskState::Ready)
        .expect("ready task from the stale plan");
    let audit_before_schedule_denial = harness
        .host
        .open_tender(&harness.tender_id)
        .expect("inspect audit before stale scheduling denial")
        .audit_event_count;
    assert_eq!(
        harness
            .host
            .run_production_task(RunProductionTaskCommand {
                tender_id: harness.tender_id.clone(),
                production_task_id: ready_task.production_task_id.clone(),
            })
            .await
            .expect_err("stale Work Plan dependencies must block new production work")
            .code,
        TenderErrorCode::InvalidCommand
    );
    assert_eq!(
        harness
            .host
            .open_tender(&harness.tender_id)
            .expect("inspect audited stale scheduling denial")
            .audit_event_count,
        audit_before_schedule_denial + 1
    );
    let invalidated = harness
        .host
        .invalidate_bid_decision_approval(InvalidateBidDecisionApprovalCommand {
            tender_id: harness.tender_id.clone(),
            approval_id: accepted.approval.approval_id.clone(),
            approval_sha256: accepted.approval.approval_sha256.clone(),
            material_change_summary:
                "A material addendum changes the commercial and delivery basis.".into(),
            affected_areas: vec!["commercial_terms".into(), "delivery_plan".into()],
        })
        .expect("invalidate exact Proceed approval");
    assert_eq!(
        invalidated.package.lifecycle_phase,
        TenderLifecyclePhase::BidDecision
    );
    assert_eq!(
        invalidated
            .package
            .approval
            .as_ref()
            .and_then(|approval| approval.invalidation.clone()),
        Some(invalidated.invalidation.clone())
    );
    assert!(!invalidated.invalidation.changed_records.is_empty());
    let suspended_plan = harness
        .host
        .inspect_current_work_plan(&harness.tender_id)
        .expect("inspect suspended Work Plan")
        .expect("historical Work Plan remains inspectable");
    assert!(!suspended_plan.current);
    assert!(suspended_plan
        .profiles
        .iter()
        .all(|binding| binding.status == AgentProfileStatus::Suspended));
    let suspended_production = harness
        .host
        .inspect_tender_production(&harness.tender_id)
        .expect("inspect suspended production")
        .expect("historical production remains inspectable");
    assert!(!suspended_production.active);
    assert!(suspended_production.tasks.iter().all(|task| matches!(
        task.state,
        ProductionTaskState::ReadyForIntegration
            | ProductionTaskState::Indeterminate
            | ProductionTaskState::Suspended
    )));
    assert_eq!(
        harness
            .host
            .create_tender_engineer_entry(CreateTenderEngineerEntryCommand {
                tender_id: harness.tender_id.clone(),
                value: "Late unbound change".into(),
                description: "Cannot alter the captured exact diff before its successor.".into(),
            })
            .expect_err("material-change intake freezes until the exact successor publishes")
            .code,
        TenderErrorCode::InvalidCommand
    );
    let stale = harness
        .host
        .inspect_current_bid_decision_package(&harness.tender_id)
        .expect("inspect invalidated package after registered change")
        .expect("approved package remains preserved");
    assert!(!stale.current);
    let successor = harness
        .host
        .create_bid_decision_package(CreateBidDecisionPackageCommand {
            tender_id: harness.tender_id.clone(),
            base_version: Some(package.version),
            disposition_updates: Vec::new(),
            manager_capability_demands: Vec::new(),
        })
        .expect("publish material-change successor");
    assert_eq!(
        successor.material_change_basis,
        Some(invalidated.invalidation.clone())
    );
    assert!(successor.return_rework_basis.is_none());
    assert_eq!(successor.version, package.version + 1);
    let changed_record = invalidated
        .invalidation
        .changed_records
        .first()
        .expect("exact changed record")
        .clone();
    harness
        .host
        .decide_tender_record(DecideTenderRecordCommand {
            tender_id: harness.tender_id.clone(),
            record_id: changed_record.record_id,
            version: changed_record.version,
            decision: TenderRecordEngineerDecisionKind::Verify,
            rationale: "Verify the exact material-change obligation before re-Proceed.".into(),
        })
        .expect("verify material-change record after the captured successor publication");
    let current_records = inspect_all_records(&harness.host, &harness.tender_id);
    let successor = harness
        .host
        .create_bid_decision_package(CreateBidDecisionPackageCommand {
            tender_id: harness.tender_id.clone(),
            base_version: Some(successor.version),
            disposition_updates: complete_dispositions(&current_records),
            manager_capability_demands: Vec::new(),
        })
        .expect("publish a fully dispositioned successor after exact verification");
    harness.set_agent_scenario("bid-package-review");
    let successor = harness
        .host
        .run_bid_decision_package_review(RunBidDecisionPackageReviewCommand {
            tender_id: harness.tender_id.clone(),
            package_id: successor.package_id,
            version: successor.version,
        })
        .await
        .expect("review exact material-change successor")
        .package;
    assert!(successor.decision_gate_ready, "{successor:#?}");
    harness
        .host
        .decide_bid_decision_package(approval_command(
            &harness,
            &successor,
            BidDecisionApprovalDecision::Accept,
        ))
        .expect("accept exact material-change successor");
    let rebased_plan = harness
        .host
        .revise_work_plan_proposal(ReviseWorkPlanProposalCommand {
            tender_id: harness.tender_id.clone(),
            plan_id: first_plan.plan_id,
            base_version: first_plan.version,
            actions: vec![WorkPlanRevisionAction::RebasePackageBasis],
        })
        .expect("rebase the suspended Work Plan onto the exact successor package");
    assert_eq!(rebased_plan.version, first_plan.version + 1);
    assert_eq!(rebased_plan.bid_package_id, successor.package_id);
    assert_eq!(rebased_plan.bid_package_version, successor.version);
    assert!(rebased_plan
        .profiles
        .iter()
        .all(|binding| binding.status == AgentProfileStatus::Proposed));
    let reapproved_plan = harness
        .host
        .decide_work_plan_proposal(DecideWorkPlanProposalCommand {
            tender_id: harness.tender_id.clone(),
            plan_id: rebased_plan.plan_id,
            version: rebased_plan.version,
            decision: WorkPlanDecision::Approve,
            rationale: "Approve production against the exact successor package and Work Plan."
                .into(),
        })
        .expect("approve exact rebased Work Plan");
    assert!(reapproved_plan
        .profiles
        .iter()
        .all(|binding| binding.status == AgentProfileStatus::Proposed));

    harness
        .host
        .close_tender(&harness.tender_id)
        .expect("close before invalidation integrity inspection");
    assert_eq!(
        harness
            .host
            .inspect_tender_integrity(&harness.tender_id)
            .expect("inspect material-change lineage")
            .state,
        TenderIntegrityState::Ready
    );
    harness
        .host
        .close_tender(&harness.tender_id)
        .expect("close before dependency-snapshot corruption injection");
    let database = harness
        .application_home
        .join("tenders")
        .join(&harness.tender_id)
        .join("tender.sqlite");
    let connection = rusqlite::Connection::open(database).expect("open Tender Store");
    let (prior_inventory_json, prior_inventory_sha256): (String, String) = connection
        .query_row(
            "SELECT record_inventory_json, record_inventory_sha256
             FROM bid_decision_package_versions WHERE version = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("load accepted dependency snapshot");
    let successor_manifest_json: String = connection
        .query_row(
            "SELECT manifest_json FROM bid_decision_package_versions WHERE version = 2",
            [],
            |row| row.get(0),
        )
        .expect("load successor manifest");
    let mut successor_manifest: Value =
        serde_json::from_str(&successor_manifest_json).expect("parse successor manifest");
    successor_manifest["record_inventory_sha256"] = Value::String(prior_inventory_sha256.clone());
    let successor_manifest_json =
        serde_json::to_string(&successor_manifest).expect("canonical successor manifest");
    let successor_manifest_sha256 = Sha256::digest(successor_manifest_json.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    connection
        .execute("DROP TRIGGER bid_decision_package_versions_no_update", [])
        .expect("drop immutable version trigger for corruption fixture");
    connection
        .execute(
            "UPDATE bid_decision_package_versions
             SET record_inventory_json = ?1, record_inventory_sha256 = ?2,
                 manifest_json = ?3, manifest_sha256 = ?4
             WHERE version = 2",
            rusqlite::params![
                prior_inventory_json,
                prior_inventory_sha256,
                successor_manifest_json,
                successor_manifest_sha256,
            ],
        )
        .expect("forge unchanged successor dependency snapshot");
    drop(connection);
    assert_eq!(
        harness
            .host
            .inspect_tender_integrity(&harness.tender_id)
            .expect("inspect forged material-change successor")
            .state,
        TenderIntegrityState::RecoveryRequired
    );
}

#[tokio::test]
async fn active_agent_work_blocks_the_atomic_gate_until_its_output_is_repackaged() {
    let harness = Harness::new("record-extraction");
    let package = ready_package(&harness).await;
    let document = harness
        .host
        .inspect_document_register(&harness.tender_id)
        .expect("inspect parsed decision source")
        .documents
        .into_iter()
        .next()
        .expect("parsed decision source");
    let evidence = harness
        .host
        .inspect_evidence(ParseSourceArtifactCommand {
            tender_id: harness.tender_id.clone(),
            artifact_id: document.artifact_id.clone(),
            version: document.version,
        })
        .expect("inspect exact Evidence for delayed extraction")
        .locations
        .into_iter()
        .map(|location| TenderEvidenceReference {
            artifact_id: document.artifact_id.clone(),
            version: document.version,
            ordinal: location.ordinal,
        })
        .collect();
    harness.set_agent_scenario("record-extraction-delayed");
    let host = harness.host.clone();
    let tender_id = harness.tender_id.clone();
    let extraction = tokio::spawn(async move {
        host.run_tender_record_extraction(RunTenderRecordExtractionCommand {
            tender_id,
            evidence,
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
    assert_eq!(
        harness
            .host
            .decide_bid_decision_package(approval_command(
                &harness,
                &package,
                BidDecisionApprovalDecision::Accept,
            ))
            .expect_err("active Agent Run must block the formal gate")
            .code,
        TenderErrorCode::InvalidCommand
    );
    assert!(harness
        .host
        .inspect_current_bid_decision_package(&harness.tender_id)
        .expect("inspect package after active-run denial")
        .is_some_and(|package| package.approval.is_none()));
    fs::write(
        harness.codex.with_extension("record-output-release"),
        b"release",
    )
    .expect("release delayed extraction");
    extraction
        .await
        .expect("join delayed extraction")
        .expect("complete delayed extraction before any lifecycle transition");
    assert_eq!(
        harness
            .host
            .decide_bid_decision_package(approval_command(
                &harness,
                &package,
                BidDecisionApprovalDecision::Accept,
            ))
            .expect_err("published material changes stale the exact package")
            .code,
        TenderErrorCode::InvalidCommand
    );
}

#[tokio::test]
async fn return_preserves_the_pending_gate_and_requires_a_successor_exact_version() {
    let harness = Harness::new("record-extraction");
    let package = ready_package(&harness).await;
    let returned = harness
        .host
        .decide_bid_decision_package(approval_command(
            &harness,
            &package,
            BidDecisionApprovalDecision::Return,
        ))
        .expect("return exact package for required rework");
    assert_eq!(
        returned.package.lifecycle_phase,
        TenderLifecyclePhase::BidDecision
    );
    assert!(!returned.approval.required_rework.is_empty());
    assert!(!returned.package.decision_gate_ready);

    assert_eq!(
        harness
            .host
            .create_bid_decision_package(CreateBidDecisionPackageCommand {
                tender_id: harness.tender_id.clone(),
                base_version: Some(package.version),
                disposition_updates: Vec::new(),
                manager_capability_demands: Vec::new(),
            })
            .expect_err("unresolved Return rework keeps the gate pending")
            .code,
        TenderErrorCode::InvalidCommand
    );
    let rework = harness
        .host
        .resolve_bid_decision_return_rework(ResolveBidDecisionReturnReworkCommand {
            tender_id: harness.tender_id.clone(),
            approval_id: returned.approval.approval_id.clone(),
            resolutions: vec![
                "Rechecked the exact package and recorded the required controlled disposition."
                    .into(),
            ],
        })
        .expect("resolve every required Return item attributably");
    assert_eq!(rework.disposition.items.len(), 1);
    assert_eq!(
        rework.disposition.approval_sha256,
        returned.approval.approval_sha256
    );

    let successor = harness
        .host
        .create_bid_decision_package(CreateBidDecisionPackageCommand {
            tender_id: harness.tender_id.clone(),
            base_version: Some(package.version),
            disposition_updates: Vec::new(),
            manager_capability_demands: Vec::new(),
        })
        .expect("publish reworked successor package");
    assert_eq!(successor.version, 2);
    assert_eq!(successor.change_summary.prior_version, Some(1));
    assert_eq!(successor.prior_approval_count, 1);
    assert!(successor.approval.is_none());
    assert_eq!(
        successor.return_rework_basis,
        Some(rework.disposition.clone())
    );
    assert_eq!(
        harness
            .host
            .decide_bid_decision_package(approval_command(
                &harness,
                &package,
                BidDecisionApprovalDecision::Accept,
            ))
            .expect_err("stale package view cannot be approved")
            .code,
        TenderErrorCode::InvalidCommand
    );
    let mut tampered = approval_command(&harness, &successor, BidDecisionApprovalDecision::Accept);
    tampered.manifest_sha256 = "0".repeat(64);
    assert_eq!(
        harness
            .host
            .decide_bid_decision_package(tampered)
            .expect_err("changed manifest cannot be approved")
            .code,
        TenderErrorCode::InvalidCommand
    );

    harness.set_agent_scenario("bid-package-review");
    let successor = harness
        .host
        .run_bid_decision_package_review(RunBidDecisionPackageReviewCommand {
            tender_id: harness.tender_id.clone(),
            package_id: successor.package_id,
            version: successor.version,
        })
        .await
        .expect("review successor package")
        .package;
    let accepted = harness
        .host
        .decide_bid_decision_package(approval_command(
            &harness,
            &successor,
            BidDecisionApprovalDecision::Accept,
        ))
        .expect("accept reworked exact package");
    assert_eq!(accepted.approval.approval_sequence, 2);
    assert_eq!(
        accepted.approval.preceding_approval_hash,
        returned.approval.approval_sha256
    );
    let history = harness
        .host
        .inspect_bid_decision_approval_history(InspectBidDecisionApprovalHistoryCommand {
            tender_id: harness.tender_id.clone(),
            before_sequence: None,
            limit: 10,
        })
        .expect("inspect return and Proceed history");
    assert_eq!(history.approvals.len(), 2);
    assert_eq!(
        history.approvals[0].decision,
        BidDecisionApprovalDecision::Accept
    );
    assert_eq!(
        history.approvals[1].decision,
        BidDecisionApprovalDecision::Return
    );
}

#[tokio::test]
async fn reject_is_an_attributable_terminal_decline_that_preserves_the_tender() {
    let harness = Harness::new("record-extraction");
    let package = ready_package(&harness).await;
    let record_count = inspect_all_records(&harness.host, &harness.tender_id).len();
    let declined = harness
        .host
        .decide_bid_decision_package(approval_command(
            &harness,
            &package,
            BidDecisionApprovalDecision::Reject,
        ))
        .expect("record formal Decline");
    assert_eq!(
        declined.approval.decision,
        BidDecisionApprovalDecision::Reject
    );
    assert_eq!(
        declined.package.lifecycle_phase,
        TenderLifecyclePhase::Declined
    );
    assert_eq!(
        inspect_all_records(&harness.host, &harness.tender_id).len(),
        record_count,
        "Decline must preserve exact source analysis"
    );
    assert_eq!(
        harness
            .host
            .create_bid_decision_package(CreateBidDecisionPackageCommand {
                tender_id: harness.tender_id.clone(),
                base_version: Some(package.version),
                disposition_updates: Vec::new(),
                manager_capability_demands: Vec::new(),
            })
            .expect_err("Decline is terminal")
            .code,
        TenderErrorCode::InvalidCommand
    );
    assert_eq!(
        harness
            .host
            .invalidate_bid_decision_approval(InvalidateBidDecisionApprovalCommand {
                tender_id: harness.tender_id.clone(),
                approval_id: declined.approval.approval_id.clone(),
                approval_sha256: declined.approval.approval_sha256.clone(),
                material_change_summary: "Attempted reopening of a declined Tender.".into(),
                affected_areas: vec!["commercial_terms".into()],
            })
            .expect_err("Decline is not a reopenable Proceed approval")
            .code,
        TenderErrorCode::InvalidCommand
    );
    assert!(harness
        .host
        .inspect_current_bid_decision_package(&harness.tender_id)
        .expect("inspect preserved declined package")
        .is_some_and(|package| package.approval == Some(declined.approval)));
    assert_eq!(
        harness
            .host
            .revise_tender(ReviseTenderCommand {
                tender_id: harness.tender_id.clone(),
                name: "Attempted post-Decline mutation".into(),
            })
            .expect_err("Decline closes the pre-bid writer plane")
            .code,
        TenderErrorCode::InvalidCommand
    );
}

#[tokio::test]
async fn incomplete_or_unreviewed_package_cannot_proceed_or_decline() {
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
        .expect("create incomplete decision basis");
    let audit_before_denials = harness
        .host
        .open_tender(&harness.tender_id)
        .expect("inspect audit count before denied decisions")
        .audit_event_count;
    for decision in [
        BidDecisionApprovalDecision::Accept,
        BidDecisionApprovalDecision::Reject,
    ] {
        assert_eq!(
            harness
                .host
                .decide_bid_decision_package(approval_command(&harness, &package, decision))
                .expect_err("failed gate guard cannot mutate lifecycle")
                .code,
            TenderErrorCode::InvalidCommand
        );
    }
    let mut malformed_return =
        approval_command(&harness, &package, BidDecisionApprovalDecision::Return);
    malformed_return.required_rework.clear();
    assert_eq!(
        harness
            .host
            .decide_bid_decision_package(malformed_return)
            .expect_err("semantically invalid formal decision is denied")
            .code,
        TenderErrorCode::InvalidCommand
    );
    let mut empty_rationale =
        approval_command(&harness, &package, BidDecisionApprovalDecision::Accept);
    empty_rationale.rationale.clear();
    assert_eq!(
        harness
            .host
            .decide_bid_decision_package(empty_rationale)
            .expect_err("Host validation denial is audited in the valid Tender")
            .code,
        TenderErrorCode::InvalidCommand
    );
    assert_eq!(
        harness
            .host
            .open_tender(&harness.tender_id)
            .expect("inspect denied decision audit events")
            .audit_event_count,
        audit_before_denials + 4
    );
    let successor = harness
        .host
        .create_bid_decision_package(CreateBidDecisionPackageCommand {
            tender_id: harness.tender_id.clone(),
            base_version: Some(package.version),
            disposition_updates: Vec::new(),
            manager_capability_demands: Vec::new(),
        })
        .expect("publish a successor before any formal decision");
    assert_eq!(
        harness
            .host
            .decide_bid_decision_package(approval_command(
                &harness,
                &package,
                BidDecisionApprovalDecision::Return,
            ))
            .expect_err("stale exact package cannot be returned")
            .code,
        TenderErrorCode::InvalidCommand
    );
    assert!(harness
        .host
        .inspect_bid_decision_approval_history(InspectBidDecisionApprovalHistoryCommand {
            tender_id: harness.tender_id.clone(),
            before_sequence: None,
            limit: 10,
        })
        .expect("inspect history after failed guards")
        .approvals
        .is_empty());
    let returned = harness
        .host
        .decide_bid_decision_package(approval_command(
            &harness,
            &successor,
            BidDecisionApprovalDecision::Return,
        ))
        .expect("Tendering Manager can explicitly return incomplete work");
    assert_eq!(
        returned.package.lifecycle_phase,
        TenderLifecyclePhase::BidDecision
    );
    assert_eq!(
        returned.approval.decision,
        BidDecisionApprovalDecision::Return
    );
    assert_eq!(
        harness
            .host
            .create_bid_decision_package(CreateBidDecisionPackageCommand {
                tender_id: harness.tender_id.clone(),
                base_version: Some(successor.version),
                disposition_updates: Vec::new(),
                manager_capability_demands: Vec::new(),
            })
            .expect_err("Return rework cannot be bypassed by cloning the package")
            .code,
        TenderErrorCode::InvalidCommand
    );
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

#[tokio::test]
async fn agent_query_blocks_exact_work_until_manager_treatment_is_applied_and_reviewed() {
    let harness = Harness::new("record-extraction");
    let (_, production) = active_production(&harness).await;
    let target = production
        .tasks
        .iter()
        .find(|task| task.state == ProductionTaskState::Ready)
        .expect("ready production task")
        .clone();

    harness.set_agent_scenario("production-task-query-proposal");
    let proposed = harness
        .host
        .run_production_task(RunProductionTaskCommand {
            tender_id: harness.tender_id.clone(),
            production_task_id: target.production_task_id.clone(),
        })
        .await
        .expect("specialist proposes material Tender Query");
    assert_eq!(
        proposed.run.state,
        AgentRunState::Completed,
        "{:#?}; fixture={}",
        proposed.run,
        fs::read_to_string(harness.codex.with_extension("fixture-error"))
            .unwrap_or_else(|_| "none".into())
    );
    assert_eq!(proposed.task.state, ProductionTaskState::QueryBlocked);
    assert!(!proposed.task.ready_for_integration);

    let page = harness
        .host
        .inspect_tender_queries(InspectTenderQueriesCommand {
            tender_id: harness.tender_id.clone(),
            cursor: None,
            limit: 8,
        })
        .expect("inspect bounded Query Register");
    assert!(page.query_register_open);
    assert_eq!(page.total_current_count, 1);
    assert_eq!(page.release_blocking_count, 1);
    let query = page.items.first().expect("agent Tender Query").clone();
    assert_eq!(
        query.source_run_id.as_deref(),
        Some(proposed.run.run_id.as_str())
    );
    assert_eq!(query.owner_profile_id, proposed.run.profile.profile_id);
    assert_eq!(query.affected_task_keys, vec![target.task.task_key.clone()]);
    assert!(query.approved_treatment.is_none());
    assert!(query.invalidations.iter().any(|invalidation| {
        invalidation.target_kind == "production_task"
            && invalidation.target_id == target.production_task_id
    }));

    harness.set_agent_scenario("production-task");
    let owner_update = harness
        .host
        .run_production_task(RunProductionTaskCommand {
            tender_id: harness.tender_id.clone(),
            production_task_id: target.production_task_id.clone(),
        })
        .await
        .expect("run bounded specialist Query-control turn");
    assert_eq!(owner_update.run.state, AgentRunState::Completed);
    assert_eq!(owner_update.task.state, ProductionTaskState::QueryBlocked);
    let query = harness
        .host
        .inspect_tender_queries(InspectTenderQueriesCommand {
            tender_id: harness.tender_id.clone(),
            cursor: None,
            limit: 8,
        })
        .expect("inspect specialist Query successor")
        .items
        .into_iter()
        .next()
        .expect("specialist Query successor");
    assert_eq!(query.version, 2);
    assert_eq!(
        query.source_run_id.as_deref(),
        Some(owner_update.run.run_id.as_str())
    );
    assert!(query.evidence.len() > page.items[0].evidence.len());
    assert!(query.proposed_treatments.iter().any(|proposal| {
        proposal.proposed_by_run_id.as_deref() == Some(owner_update.run.run_id.as_str())
    }));

    let affected_record = inspect_all_records(&harness.host, &harness.tender_id)
        .into_iter()
        .next()
        .expect("current Tender Record for targeted Query invalidation");
    let query = harness
        .host
        .revise_tender_query(ReviseTenderQueryCommand {
            tender_id: harness.tender_id.clone(),
            query_id: query.query_id.clone(),
            base_version: query.version,
            query_type: query.query_type,
            question: query.question.clone(),
            ambiguity_or_gap: query.ambiguity_or_gap.clone(),
            owner_profile_id: query.owner_profile_id.clone(),
            owner_profile_version: query.owner_profile_version,
            evidence: query.evidence.clone(),
            affected_records: vec![TenderRecordVersionReference {
                record_id: affected_record.record_id.clone(),
                version: affected_record.version,
            }],
            affected_task_keys: query.affected_task_keys.clone(),
            due_at: query.due_at.clone(),
            material: query.material,
            release_blocking: query.release_blocking,
            proposed_treatments: vec![TenderQueryTreatmentProposalInput {
                treatment: TenderQueryTreatment::ApprovedAssumption,
                rationale: "Treat the exact record and production task together.".into(),
            }],
            response: None,
            response_evidence: Vec::new(),
        })
        .expect("bind exact affected Tender Record to Query successor");
    assert_eq!(
        query.affected_records,
        vec![TenderRecordVersionReference {
            record_id: affected_record.record_id.clone(),
            version: affected_record.version,
        }]
    );

    let decided = harness
        .host
        .decide_tender_query_treatment(DecideTenderQueryTreatmentCommand {
            tender_id: harness.tender_id.clone(),
            query_id: query.query_id.clone(),
            query_version: query.version,
            treatment: TenderQueryTreatment::ApprovedAssumption,
            rationale: "The Tendering Manager accepts the bounded assumption for this exact Query version.".into(),
            treatment_details: "Apply the stated responsibility assumption and preserve it in the next Artifact Version.".into(),
            closes_query: false,
        })
        .expect("approve exact Query Treatment");
    let decision = decided
        .approved_treatment
        .as_ref()
        .expect("approved treatment");
    assert_eq!(decision.treatment, TenderQueryTreatment::ApprovedAssumption);
    assert_eq!(
        harness
            .host
            .inspect_tender_production(&harness.tender_id)
            .expect("inspect released production")
            .expect("production")
            .tasks
            .iter()
            .find(|task| task.production_task_id == target.production_task_id)
            .expect("released task")
            .state,
        ProductionTaskState::RemediationReady
    );

    harness.set_agent_scenario("production-task");
    let mut remediated = harness
        .host
        .run_production_task(RunProductionTaskCommand {
            tender_id: harness.tender_id.clone(),
            production_task_id: target.production_task_id.clone(),
        })
        .await
        .expect("apply exact Query Treatment in a successor Artifact Version");
    assert!(remediated.run.task.exact_inputs.iter().any(|input| {
        input.kind == "approved_query_treatment"
            && input.reference == decision.decision_id
            && input.version == decision.query_version
    }));
    if remediated.task.state == ProductionTaskState::ReviewReady {
        remediated = harness
            .host
            .run_production_task(RunProductionTaskCommand {
                tender_id: harness.tender_id.clone(),
                production_task_id: target.production_task_id.clone(),
            })
            .await
            .expect("independently review treatment-bearing Artifact Version");
    }
    assert_eq!(
        remediated.task.state,
        ProductionTaskState::ReadyForIntegration,
        "{:#?}; fixture={}",
        remediated.run,
        fs::read_to_string(harness.codex.with_extension("fixture-error"))
            .unwrap_or_else(|_| "none".into())
    );
    let review_detail = harness
        .host
        .inspect_production_task_review(InspectProductionTaskReviewCommand {
            tender_id: harness.tender_id.clone(),
            production_task_id: target.production_task_id.clone(),
        })
        .expect("inspect exact treatment-bearing Artifact Versions");
    let artifact = review_detail
        .artifact_versions
        .last()
        .expect("successor artifact");
    assert_eq!(artifact.summary.version, 2);
    assert_eq!(
        artifact.payload.query_treatment_applications[0].decision_id,
        decision.decision_id
    );

    let revised = harness
        .host
        .revise_tender_query(ReviseTenderQueryCommand {
            tender_id: harness.tender_id.clone(),
            query_id: decided.query_id.clone(),
            base_version: decided.version,
            query_type: decided.query_type,
            question: decided.question.clone(),
            ambiguity_or_gap: "A later exact evidence observation changes the treatment target."
                .into(),
            owner_profile_id: decided.owner_profile_id.clone(),
            owner_profile_version: decided.owner_profile_version,
            evidence: decided.evidence.clone(),
            affected_records: decided.affected_records.clone(),
            affected_task_keys: decided.affected_task_keys.clone(),
            due_at: "2000-01-01T00:00:00.000Z".into(),
            material: decided.material,
            release_blocking: decided.release_blocking,
            proposed_treatments: vec![TenderQueryTreatmentProposalInput {
                treatment: TenderQueryTreatment::Qualification,
                rationale: "The new exact observation requires a revised Manager treatment.".into(),
            }],
            response: None,
            response_evidence: Vec::new(),
        })
        .expect("publish immutable Query successor");
    assert_eq!(revised.version, decided.version + 1);
    assert_eq!(
        revised.status,
        quantix_lib::TenderQueryStatus::TreatmentProposed
    );
    assert_eq!(
        harness
            .host
            .decide_tender_query_treatment(DecideTenderQueryTreatmentCommand {
                tender_id: harness.tender_id.clone(),
                query_id: decided.query_id.clone(),
                query_version: decided.version,
                treatment: TenderQueryTreatment::ApprovedAssumption,
                rationale: "Attempt to reuse a stale exact decision basis.".into(),
                treatment_details: "This must be denied and audited.".into(),
                closes_query: false,
            })
            .expect_err("stale exact Query version cannot be decided")
            .code,
        TenderErrorCode::InvalidCommand
    );
    let blocked = harness
        .host
        .decide_tender_query_treatment(DecideTenderQueryTreatmentCommand {
            tender_id: harness.tender_id.clone(),
            query_id: revised.query_id.clone(),
            query_version: revised.version,
            treatment: TenderQueryTreatment::ExternalRfiDrafting,
            rationale: "The changed evidence now requires a controlled external answer.".into(),
            treatment_details: "Keep dependent work blocked while the External RFI is drafted under the next workflow slice.".into(),
            closes_query: false,
        })
        .expect("record exact blocking External RFI treatment");
    assert_eq!(blocked.status, quantix_lib::TenderQueryStatus::Blocked);
    let page = harness
        .host
        .inspect_tender_queries(InspectTenderQueriesCommand {
            tender_id: harness.tender_id.clone(),
            cursor: None,
            limit: 8,
        })
        .expect("inspect overdue blocking Query");
    assert_eq!(page.overdue_count, 1);
    assert_eq!(page.release_blocking_count, 1);
    assert!(page.items[0].overdue);
    assert_eq!(
        harness
            .host
            .inspect_tender_production(&harness.tender_id)
            .expect("inspect External RFI block")
            .expect("active production")
            .tasks
            .iter()
            .find(|task| task.production_task_id == target.production_task_id)
            .expect("affected task")
            .state,
        ProductionTaskState::QueryBlocked
    );
    harness
        .host
        .close_tender(&harness.tender_id)
        .expect("close Query-controlled Tender");
    let integrity = harness
        .host
        .inspect_tender_integrity(&harness.tender_id)
        .expect("cold-verify Query Register and treatment lineage");
    assert_eq!(
        integrity.state,
        TenderIntegrityState::Ready,
        "{integrity:#?}"
    );
    let database = harness
        .application_home
        .join("tenders")
        .join(&harness.tender_id)
        .join("tender.sqlite");
    let connection = rusqlite::Connection::open(database).expect("open Query-controlled Tender");
    connection
        .execute_batch("DROP TRIGGER tender_query_versions_no_update")
        .expect("enable Query manifest tamper fixture");
    let manifest_json: String = connection
        .query_row(
            "SELECT manifest_json FROM tender_query_versions
             WHERE query_id = ?1 AND version = ?2",
            rusqlite::params![revised.query_id, revised.version],
            |row| row.get(0),
        )
        .expect("load exact Query manifest");
    let mut manifest: Value =
        serde_json::from_str(&manifest_json).expect("parse exact Query manifest");
    manifest["ambiguity_or_gap"] =
        Value::String("Selectively altered Query decision basis.".into());
    let manifest_json = serde_json::to_string(&manifest).expect("canonical forged Query manifest");
    let manifest_sha256 = Sha256::digest(manifest_json.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    connection
        .execute(
            "UPDATE tender_query_versions
             SET ambiguity_or_gap = 'Selectively altered Query decision basis.',
                 manifest_json = ?3, manifest_sha256 = ?4
             WHERE query_id = ?1 AND version = ?2",
            rusqlite::params![
                revised.query_id,
                revised.version,
                manifest_json,
                manifest_sha256
            ],
        )
        .expect("tamper exact Query manifest basis");
    drop(connection);
    let cold_host =
        QuantixHost::with_setup_platform(&harness.application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&cold_host).state, SetupState::Ready);
    assert_eq!(
        cold_host
            .inspect_tender_integrity(&harness.tender_id)
            .expect("inspect tampered Query manifest")
            .state,
        TenderIntegrityState::RecoveryRequired
    );
}

async fn external_rfi_drafting_query(harness: &Harness) -> TenderQuery {
    let (_, production) = active_production(harness).await;
    let target = production
        .tasks
        .iter()
        .find(|task| task.state == ProductionTaskState::Ready)
        .expect("ready task for External RFI Query")
        .clone();
    harness.set_agent_scenario("production-task-query-proposal");
    let proposed = harness
        .host
        .run_production_task(RunProductionTaskCommand {
            tender_id: harness.tender_id.clone(),
            production_task_id: target.production_task_id,
        })
        .await
        .expect("publish specialist Query proposal");
    assert_eq!(proposed.run.state, AgentRunState::Completed);
    let query = harness
        .host
        .inspect_tender_queries(InspectTenderQueriesCommand {
            tender_id: harness.tender_id.clone(),
            cursor: None,
            limit: 8,
        })
        .expect("inspect proposed External RFI Query")
        .items
        .into_iter()
        .next()
        .expect("External RFI Query");
    harness
        .host
        .decide_tender_query_treatment(DecideTenderQueryTreatmentCommand {
            tender_id: harness.tender_id.clone(),
            query_id: query.query_id.clone(),
            query_version: query.version,
            treatment: TenderQueryTreatment::ExternalRfiDrafting,
            rationale: "The exact ambiguity requires a controlled question to the Employer.".into(),
            treatment_details:
                "Draft, independently review, and obtain Manager approval before human issue."
                    .into(),
            closes_query: false,
        })
        .expect("authorize External RFI drafting")
}

fn external_rfi_create_command(
    harness: &Harness,
    query: &TenderQuery,
) -> CreateExternalRfiDraftCommand {
    let source = harness
        .host
        .inspect_document_register(&harness.tender_id)
        .expect("inspect exact Source Artifact Register")
        .documents
        .into_iter()
        .next()
        .expect("registered source for External RFI attachment");
    CreateExternalRfiDraftCommand {
        tender_id: harness.tender_id.clone(),
        query_refs: vec![ExternalRfiQueryReference {
            query_id: query.query_id.clone(),
            version: query.version,
            manifest_sha256: query.manifest_sha256.clone(),
        }],
        additional_evidence: Vec::new(),
        contractual_context: "The tender documents require the bidder to price and programme the exact stated obligation, but the cited wording leaves the responsibility boundary unresolved.".into(),
        response_need: "Confirm the responsible party and the exact basis the bidder must use in its submission.".into(),
        attachments: vec![AgentTaskInputReference {
            kind: "source_artifact".into(),
            reference: source.artifact_id,
            version: source.version,
        }],
        due_at: "2030-01-01T00:00:00Z".into(),
        recipient: ExternalRfiRecipient {
            organization: "Employer Procurement Team".into(),
            attention: "Tender Clarifications Manager".into(),
            email: Some("clarifications@example.com".into()),
        },
        affected_commitments: vec![
            "Tender price qualification".into(),
            "Submission programme basis".into(),
        ],
    }
}

#[tokio::test]
async fn external_rfi_is_versioned_reviewed_approved_exported_and_reconciled_through_intake() {
    let harness = Harness::new("record-extraction");
    let query = external_rfi_drafting_query(&harness).await;

    let failed_v1 = harness
        .host
        .create_external_rfi_draft(external_rfi_create_command(&harness, &query))
        .expect("create first External RFI draft");
    let failed = harness
        .host
        .revise_external_rfi_draft(ReviseExternalRfiDraftCommand {
            tender_id: harness.tender_id.clone(),
            rfi_id: failed_v1.rfi_id.clone(),
            base_version: failed_v1.version,
            query_refs: failed_v1.query_refs.clone(),
            additional_evidence: Vec::new(),
            contractual_context: format!(
                "{} The response must distinguish design responsibility from installation responsibility.",
                failed_v1.contractual_context
            ),
            response_need: failed_v1.response_need.clone(),
            attachments: failed_v1.attachments.clone(),
            due_at: failed_v1.due_at.clone(),
            recipient: failed_v1.recipient.clone(),
            affected_commitments: failed_v1.affected_commitments.clone(),
        })
        .expect("publish immutable External RFI successor");
    assert_eq!(failed.version, 2);
    harness.set_agent_scenario("external-rfi-review-failed");
    let failed_review = harness
        .host
        .run_external_rfi_review(RunExternalRfiReviewCommand {
            tender_id: harness.tender_id.clone(),
            rfi_id: failed.rfi_id.clone(),
            version: failed.version,
        })
        .await
        .unwrap_or_else(|error| {
            panic!(
                "record failed independent review: {error:?}; fixture={}",
                fs::read_to_string(harness.codex.with_extension("fixture-error"))
                    .unwrap_or_else(|_| "none".into())
            )
        });
    assert_eq!(
        failed_review.run.state,
        AgentRunState::Completed,
        "{failed_review:#?}; fixture={}",
        fs::read_to_string(harness.codex.with_extension("fixture-error"))
            .unwrap_or_else(|_| "none".into())
    );
    assert_eq!(
        failed_review
            .rfi
            .review
            .as_ref()
            .map(|review| review.outcome),
        Some(quantix_lib::ExternalRfiReviewOutcome::Failed)
    );
    assert_eq!(
        harness
            .host
            .approve_external_rfi_for_issue(ApproveExternalRfiForIssueCommand {
                tender_id: harness.tender_id.clone(),
                rfi_id: failed.rfi_id,
                version: failed.version,
                manifest_sha256: failed.manifest_sha256,
                rationale: "A failed review must never authorize issue.".into(),
            })
            .expect_err("failed review blocks Manager approval")
            .code,
        TenderErrorCode::InvalidCommand
    );

    let draft = harness
        .host
        .create_external_rfi_draft(external_rfi_create_command(&harness, &query))
        .expect("create reviewable External RFI draft");
    let stale_candidate = harness
        .host
        .create_external_rfi_draft(external_rfi_create_command(&harness, &query))
        .expect("create draft that will become stale after response interpretation");
    harness.set_agent_scenario("external-rfi-review");
    let reviewed = harness
        .host
        .run_external_rfi_review(RunExternalRfiReviewCommand {
            tender_id: harness.tender_id.clone(),
            rfi_id: draft.rfi_id.clone(),
            version: draft.version,
        })
        .await
        .expect("independently review exact External RFI draft");
    assert_eq!(
        reviewed.run.state,
        AgentRunState::Completed,
        "{reviewed:#?}; fixture={}",
        fs::read_to_string(harness.codex.with_extension("fixture-error"))
            .unwrap_or_else(|_| "none".into())
    );
    assert_eq!(
        reviewed.rfi.review.as_ref().map(|review| review.outcome),
        Some(quantix_lib::ExternalRfiReviewOutcome::Passed)
    );
    assert!(reviewed
        .run
        .profile
        .capabilities
        .contains(&"review_query_rfi_control".to_owned()));
    assert!(reviewed
        .run
        .task
        .exact_inputs
        .iter()
        .any(|input| input.kind == "work_plan_version"));
    assert!(reviewed.rfi.approval.is_none());

    let approved = harness
        .host
        .approve_external_rfi_for_issue(ApproveExternalRfiForIssueCommand {
            tender_id: harness.tender_id.clone(),
            rfi_id: draft.rfi_id.clone(),
            version: draft.version,
            manifest_sha256: draft.manifest_sha256.clone(),
            rationale: "The Tendering Manager approves this exact independently reviewed wording for human issue.".into(),
        })
        .expect("approve exact reviewed External RFI");
    assert!(approved.approved_for_issue);
    let approval = approved.approval.as_ref().expect("Manager approval");
    assert_eq!(
        harness
            .host
            .revise_external_rfi_draft(ReviseExternalRfiDraftCommand {
                tender_id: harness.tender_id.clone(),
                rfi_id: approved.rfi_id.clone(),
                base_version: approved.version,
                query_refs: approved.query_refs.clone(),
                additional_evidence: Vec::new(),
                contractual_context: approved.contractual_context.clone(),
                response_need: approved.response_need.clone(),
                attachments: approved.attachments.clone(),
                due_at: approved.due_at.clone(),
                recipient: approved.recipient.clone(),
                affected_commitments: approved.affected_commitments.clone(),
            })
            .expect_err("an approved and possibly human-issued RFI remains the visible exact head")
            .code,
        TenderErrorCode::InvalidCommand
    );
    let exported = harness
        .host
        .export_approved_external_rfi(ExportApprovedExternalRfiCommand {
            tender_id: harness.tender_id.clone(),
            rfi_id: approved.rfi_id.clone(),
            version: approved.version,
            approval_sha256: approval.approval_sha256.clone(),
        })
        .expect("export exact approved bytes for human issue");
    assert!(exported.bytes_verified);
    let exported_bytes = fs::read(&exported.path).expect("read exported RFI bytes");
    let exported_sha256 = Sha256::digest(&exported_bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    assert_eq!(exported_sha256, exported.bytes_sha256);
    let exported_text = String::from_utf8(exported_bytes).expect("External RFI text export");
    assert!(exported_text.contains("Quantix did not send or submit this RFI"));
    for _ in 1..64 {
        harness
            .host
            .export_approved_external_rfi(ExportApprovedExternalRfiCommand {
                tender_id: harness.tender_id.clone(),
                rfi_id: approved.rfi_id.clone(),
                version: approved.version,
                approval_sha256: approval.approval_sha256.clone(),
            })
            .expect("publish bounded repeated verified export");
    }
    assert_eq!(
        harness
            .host
            .export_approved_external_rfi(ExportApprovedExternalRfiCommand {
                tender_id: harness.tender_id.clone(),
                rfi_id: approved.rfi_id.clone(),
                version: approved.version,
                approval_sha256: approval.approval_sha256.clone(),
            })
            .expect_err("65th export exceeds the immutable per-approval bound")
            .code,
        TenderErrorCode::InvalidCommand
    );

    let original_source = harness
        .host
        .inspect_document_register(&harness.tender_id)
        .expect("inspect original Intake sources")
        .documents
        .into_iter()
        .next()
        .expect("original Tender source");
    assert_eq!(
        harness
            .host
            .register_external_rfi_response(RegisterExternalRfiResponseCommand {
                tender_id: harness.tender_id.clone(),
                rfi_id: approved.rfi_id.clone(),
                rfi_version: approved.version,
                approval_id: approval.approval_id.clone(),
                source_artifact_id: original_source.artifact_id,
                source_artifact_version: original_source.version,
            })
            .expect_err("an original Tender input is not a received RFI response")
            .code,
        TenderErrorCode::InvalidCommand
    );

    let response_source = harness._root.path().join("external-rfi-response");
    fs::create_dir(&response_source).expect("response package directory");
    for index in 0..65 {
        fs::write(
            response_source.join(format!("employer-response-{index:02}.pdf")),
            format!(
                "%PDF-1.7\nThe Employer response {index} confirms the bidder responsibility boundary.\n%%EOF\n"
            ),
        )
        .expect("External RFI response fixture");
    }
    let response_import = harness
        .host
        .import_tender_package(ImportTenderPackageCommand {
            tender_id: harness.tender_id.clone(),
            source_path: response_source.to_string_lossy().into_owned(),
        })
        .expect("register External RFI response through Intake");
    assert_eq!(response_import.documents.len(), 65);
    let response_candidates = harness
        .host
        .inspect_external_rfi_response_candidates(InspectExternalRfiResponseCandidatesCommand {
            tender_id: harness.tender_id.clone(),
            approval_id: approval.approval_id.clone(),
            cursor: None,
            limit: 64,
        })
        .expect("inspect the Host-authorized post-approval Intake response page");
    assert_eq!(response_candidates.items.len(), 64);
    assert!(response_candidates.next_cursor.is_some());
    assert!(response_candidates.items.iter().all(|candidate| {
        response_import.documents.iter().any(|document| {
            candidate.source_artifact_id == document.artifact_id
                && candidate.source_artifact_version == document.version
        })
    }));
    let later_response_candidates = harness
        .host
        .inspect_external_rfi_response_candidates(InspectExternalRfiResponseCandidatesCommand {
            tender_id: harness.tender_id.clone(),
            approval_id: approval.approval_id.clone(),
            cursor: response_candidates.next_cursor.clone(),
            limit: 64,
        })
        .expect("advance the Host-authorized post-approval Intake response page");
    assert_eq!(later_response_candidates.items.len(), 1);
    assert!(later_response_candidates.next_cursor.is_none());
    let advanced_query = harness
        .host
        .revise_tender_query(ReviseTenderQueryCommand {
            tender_id: harness.tender_id.clone(),
            query_id: query.query_id.clone(),
            base_version: query.version,
            query_type: query.query_type,
            question: query.question.clone(),
            ambiguity_or_gap: format!(
                "{} The external response remains pending.",
                query.ambiguity_or_gap
            ),
            owner_profile_id: query.owner_profile_id.clone(),
            owner_profile_version: query.owner_profile_version,
            evidence: query.evidence.clone(),
            affected_records: query.affected_records.clone(),
            affected_task_keys: query.affected_task_keys.clone(),
            due_at: query.due_at.clone(),
            material: query.material,
            release_blocking: query.release_blocking,
            proposed_treatments: query
                .proposed_treatments
                .iter()
                .map(|proposal| TenderQueryTreatmentProposalInput {
                    treatment: proposal.treatment,
                    rationale: proposal.rationale.clone(),
                })
                .collect(),
            response: None,
            response_evidence: Vec::new(),
        })
        .expect("advance the Query while the human-issued RFI is outstanding");
    let advanced_query = harness
        .host
        .decide_tender_query_treatment(DecideTenderQueryTreatmentCommand {
            tender_id: harness.tender_id.clone(),
            query_id: advanced_query.query_id.clone(),
            query_version: advanced_query.version,
            treatment: TenderQueryTreatment::ExternalRfiDrafting,
            rationale: "The outstanding issued RFI still controls this current Query basis.".into(),
            treatment_details:
                "Retain the exact issued basis while awaiting the registered response.".into(),
            closes_query: false,
        })
        .expect("retain External RFI control on the current Query head");
    let revised_stale = harness
        .host
        .revise_external_rfi_draft(ReviseExternalRfiDraftCommand {
            tender_id: harness.tender_id.clone(),
            rfi_id: stale_candidate.rfi_id.clone(),
            base_version: stale_candidate.version,
            query_refs: vec![ExternalRfiQueryReference {
                query_id: advanced_query.query_id.clone(),
                version: advanced_query.version,
                manifest_sha256: advanced_query.manifest_sha256.clone(),
            }],
            additional_evidence: stale_candidate.source_evidence.clone(),
            contractual_context: stale_candidate.contractual_context.clone(),
            response_need: stale_candidate.response_need.clone(),
            attachments: stale_candidate.attachments.clone(),
            due_at: stale_candidate.due_at.clone(),
            recipient: stale_candidate.recipient.clone(),
            affected_commitments: stale_candidate.affected_commitments.clone(),
        })
        .expect("revise a stale RFI identity onto the current Query basis");
    assert!(revised_stale.evidence_current);

    let mut first_response = None;
    for document in response_import.documents.iter().take(64) {
        let linked = harness
            .host
            .register_external_rfi_response(RegisterExternalRfiResponseCommand {
                tender_id: harness.tender_id.clone(),
                rfi_id: approved.rfi_id.clone(),
                rfi_version: approved.version,
                approval_id: approval.approval_id.clone(),
                source_artifact_id: document.artifact_id.clone(),
                source_artifact_version: document.version,
            })
            .expect("link bounded immutable Intake response to exact RFI");
        first_response.get_or_insert_with(|| {
            linked
                .responses
                .first()
                .expect("first External RFI response link")
                .clone()
        });
    }
    let overflow_response = &response_import.documents[64];
    assert_eq!(
        harness
            .host
            .register_external_rfi_response(RegisterExternalRfiResponseCommand {
                tender_id: harness.tender_id.clone(),
                rfi_id: approved.rfi_id.clone(),
                rfi_version: approved.version,
                approval_id: approval.approval_id.clone(),
                source_artifact_id: overflow_response.artifact_id.clone(),
                source_artifact_version: overflow_response.version,
            })
            .expect_err("65th response exceeds the immutable per-approval bound")
            .code,
        TenderErrorCode::InvalidCommand
    );
    let response = first_response.expect("External RFI response link");
    let interpretation_command = InterpretExternalRfiResponseCommand {
        tender_id: harness.tender_id.clone(),
        response_link_id: response.response_link_id.clone(),
        query_id: query.query_id.clone(),
        issued_query_version: query.version,
        base_query_version: advanced_query.version,
        base_query_manifest_sha256: advanced_query.manifest_sha256.clone(),
        material: true,
        interpretation: "The response confirms the bidder carries installation responsibility while the Employer retains design responsibility.".into(),
        treatment: TenderQueryTreatment::Qualification,
        rationale: "Preserve the exact responsibility split in the bid basis.".into(),
        treatment_details: "Qualify the price and programme against the confirmed responsibility split and rework dependent outputs.".into(),
        closes_query: false,
    };
    let database = harness
        .application_home
        .join("tenders")
        .join(&harness.tender_id)
        .join("tender.sqlite");
    let fault_connection = rusqlite::Connection::open(&database).expect("open fault connection");
    fault_connection
        .execute_batch(
            "CREATE TRIGGER injected_external_rfi_interpretation_failure
             BEFORE INSERT ON external_rfi_response_interpretations
             BEGIN SELECT RAISE(ABORT, 'injected interpretation publication failure'); END;",
        )
        .expect("inject late interpretation failure");
    harness
        .host
        .interpret_external_rfi_response(interpretation_command.clone())
        .expect_err("late publication failure rolls back the complete Query successor");
    let after_injected_failure = harness
        .host
        .inspect_tender_queries(InspectTenderQueriesCommand {
            tender_id: harness.tender_id.clone(),
            cursor: None,
            limit: 8,
        })
        .expect("inspect Query after injected interpretation failure")
        .items
        .into_iter()
        .find(|candidate| candidate.query_id == query.query_id)
        .expect("current Query after injected failure");
    assert_eq!(after_injected_failure.version, advanced_query.version);
    fault_connection
        .execute_batch("DROP TRIGGER injected_external_rfi_interpretation_failure;")
        .expect("remove interpretation fault injection");
    let interpreted = harness
        .host
        .interpret_external_rfi_response(interpretation_command)
        .expect("record Manager interpretation and exact Query successor");
    assert_eq!(interpreted.interpretations.len(), 1);
    let interpretation = &interpreted.interpretations[0];
    assert_eq!(interpretation.source_query_version, query.version);
    assert_eq!(interpretation.base_query_version, advanced_query.version);
    assert_eq!(
        interpretation.resulting_query_version,
        advanced_query.version + 1
    );
    assert!(interpretation.material);

    let page = harness
        .host
        .inspect_external_rfis(InspectExternalRfisCommand {
            tender_id: harness.tender_id.clone(),
            cursor: None,
            limit: 8,
        })
        .expect("inspect bounded External RFI Register");
    assert_eq!(page.total_current_count, 3);
    assert_eq!(page.approved_for_issue_count, 0, "the interpreted Query successor makes the issued draft historical rather than silently current");
    let stale = page
        .items
        .iter()
        .find(|item| item.rfi_id == stale_candidate.rfi_id)
        .expect("stale exact draft");
    assert!(!stale.evidence_current);
    assert_eq!(
        harness
            .host
            .run_external_rfi_review(RunExternalRfiReviewCommand {
                tender_id: harness.tender_id.clone(),
                rfi_id: stale.rfi_id.clone(),
                version: stale.version,
            })
            .await
            .expect_err("stale Evidence cannot be independently reviewed")
            .code,
        TenderErrorCode::InvalidCommand
    );

    harness
        .host
        .close_tender(&harness.tender_id)
        .expect("close External RFI Tender");
    let integrity = harness
        .host
        .inspect_tender_integrity(&harness.tender_id)
        .expect("cold-verify External RFI lineage");
    assert_eq!(
        integrity.state,
        TenderIntegrityState::Ready,
        "{integrity:#?}"
    );
    harness
        .host
        .close_tender(&harness.tender_id)
        .expect("close verified External RFI Tender before corruption probe");
    fault_connection
        .execute_batch("DROP TRIGGER agent_runs_terminal_facts_no_rewrite;")
        .expect("disable Agent Run immutability only for corruption fixture");
    fault_connection
        .execute(
            "UPDATE agent_runs
             SET permission_grant_json = json_set(
               permission_grant_json, '$.network_allowed', json('true')
             )
             WHERE run_id = ?1",
            [&reviewed.run.run_id],
        )
        .expect("corrupt the exact RFI review authority ceiling");
    let corrupted = harness
        .host
        .inspect_tender_integrity(&harness.tender_id)
        .expect("inspect corrupted External RFI review authority");
    assert_eq!(corrupted.state, TenderIntegrityState::RecoveryRequired);
    fault_connection
        .execute(
            "UPDATE agent_runs
             SET permission_grant_json = json_set(
               json_set(permission_grant_json, '$.network_allowed', json('false')),
               '$.data_views[0].sha256', ?2
             )
             WHERE run_id = ?1",
            rusqlite::params![reviewed.run.run_id, "0".repeat(64)],
        )
        .expect("restore authority and corrupt the exact review Data View digest");
    let fresh_host =
        QuantixHost::with_setup_platform(&harness.application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&fresh_host).state, SetupState::Ready);
    let digest_corrupted = fresh_host
        .inspect_tender_integrity(&harness.tender_id)
        .expect("inspect corrupted External RFI review Data View digest");
    assert_eq!(
        digest_corrupted.state,
        TenderIntegrityState::RecoveryRequired
    );
}

#[tokio::test]
async fn external_rfi_review_cannot_publish_after_its_approved_work_plan_is_suspended() {
    let harness = Harness::new("record-extraction");
    let query = external_rfi_drafting_query(&harness).await;
    let draft = harness
        .host
        .create_external_rfi_draft(external_rfi_create_command(&harness, &query))
        .expect("create External RFI for authorization race");
    harness.set_agent_scenario("external-rfi-review-delayed");
    let host = harness.host.clone();
    let tender_id = harness.tender_id.clone();
    let rfi_id = draft.rfi_id.clone();
    let version = draft.version;
    let review = tokio::spawn(async move {
        host.run_external_rfi_review(RunExternalRfiReviewCommand {
            tender_id,
            rfi_id,
            version,
        })
        .await
    });
    wait_for_fixture_path(&harness.codex.with_extension("external-rfi-review-waiting")).await;
    let database = harness
        .application_home
        .join("tenders")
        .join(&harness.tender_id)
        .join("tender.sqlite");
    let connection = rusqlite::Connection::open(&database).expect("open authorization race store");
    assert_eq!(
        connection
            .execute(
                "UPDATE production_activations SET status = 'suspended'
                 WHERE status = 'active'",
                [],
            )
            .expect("suspend exact approved activation"),
        1
    );
    fs::write(
        harness.codex.with_extension("external-rfi-review-release"),
        b"release",
    )
    .expect("release delayed External RFI review");
    let result = review
        .await
        .expect("join delayed External RFI review")
        .expect("record stale-authority review as a terminal run");
    assert_eq!(result.run.state, AgentRunState::Failed);
    assert!(result.rfi.review.is_none());
    assert_eq!(
        connection
            .execute(
                "UPDATE production_activations SET status = 'active'
                 WHERE status = 'suspended'",
                [],
            )
            .expect("restore activation after race fixture"),
        1
    );
    harness
        .host
        .close_tender(&harness.tender_id)
        .expect("close authorization race Tender");
    let integrity = harness
        .host
        .inspect_tender_integrity(&harness.tender_id)
        .expect("cold-verify rejected stale-authority review");
    assert_eq!(
        integrity.state,
        TenderIntegrityState::Ready,
        "{integrity:#?}"
    );
}

#[tokio::test]
async fn external_rfi_cold_integrity_rejects_a_successor_after_manager_approval() {
    let harness = Harness::new("record-extraction");
    let query = external_rfi_drafting_query(&harness).await;
    let draft = harness
        .host
        .create_external_rfi_draft(external_rfi_create_command(&harness, &query))
        .expect("create External RFI for approved-head corruption coverage");
    harness.set_agent_scenario("external-rfi-review");
    let reviewed = harness
        .host
        .run_external_rfi_review(RunExternalRfiReviewCommand {
            tender_id: harness.tender_id.clone(),
            rfi_id: draft.rfi_id.clone(),
            version: draft.version,
        })
        .await
        .expect("review External RFI before approved-head corruption");
    assert_eq!(reviewed.run.state, AgentRunState::Completed);
    harness
        .host
        .approve_external_rfi_for_issue(ApproveExternalRfiForIssueCommand {
            tender_id: harness.tender_id.clone(),
            rfi_id: draft.rfi_id.clone(),
            version: draft.version,
            manifest_sha256: draft.manifest_sha256.clone(),
            rationale: "Approve the exact reviewed RFI for the cold-integrity fixture.".into(),
        })
        .expect("approve exact External RFI before corruption");

    let database = harness
        .application_home
        .join("tenders")
        .join(&harness.tender_id)
        .join("tender.sqlite");
    let fault_connection =
        rusqlite::Connection::open(&database).expect("open approved-head corruption fixture store");
    fault_connection
        .execute_batch(
            "CREATE TEMP TABLE saved_external_rfi_review AS
               SELECT * FROM external_rfi_reviews;
             CREATE TEMP TABLE saved_external_rfi_approval AS
               SELECT * FROM external_rfi_approvals;
             DROP TRIGGER external_rfi_approvals_no_delete;
             DROP TRIGGER external_rfi_reviews_no_delete;
             DELETE FROM external_rfi_approvals;
             DELETE FROM external_rfi_reviews;",
        )
        .expect("temporarily remove immutable decision facts for corruption fixture");
    let successor = harness
        .host
        .revise_external_rfi_draft(ReviseExternalRfiDraftCommand {
            tender_id: harness.tender_id.clone(),
            rfi_id: draft.rfi_id.clone(),
            base_version: draft.version,
            query_refs: draft.query_refs.clone(),
            additional_evidence: Vec::new(),
            contractual_context: format!(
                "{} This forged successor must never coexist with the approval.",
                draft.contractual_context
            ),
            response_need: draft.response_need.clone(),
            attachments: draft.attachments.clone(),
            due_at: draft.due_at.clone(),
            recipient: draft.recipient.clone(),
            affected_commitments: draft.affected_commitments.clone(),
        })
        .expect("publish otherwise-valid successor while approval facts are hidden");
    assert_eq!(successor.version, 2);
    fault_connection
        .execute_batch(
            "INSERT INTO external_rfi_reviews
               SELECT * FROM saved_external_rfi_review;
             INSERT INTO external_rfi_approvals
               SELECT * FROM saved_external_rfi_approval;",
        )
        .expect("restore exact historical review and approval facts");
    harness
        .host
        .close_tender(&harness.tender_id)
        .expect("close approved-head corruption Tender");
    let integrity = harness
        .host
        .inspect_tender_integrity(&harness.tender_id)
        .expect("cold-inspect RFI approval superseded by a valid-looking successor");
    assert_eq!(integrity.state, TenderIntegrityState::RecoveryRequired);
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
