use std::{fs, io, path::Path, sync::Arc};

use serde_json::Value;
use sha2::{Digest, Sha256};

use quantix_lib::{
    ensure_quantix_setup, AgentProfileStatus, AgentRunState, BidDecisionApprovalDecision,
    BidDecisionPackageInspection, BidDecisionPackageReviewOutcome, BidRecommendationOutcome,
    ComplianceDisposition, ComplianceDispositionUpdate, ComposeTenderOfficeCommand,
    CreateBidDecisionPackageCommand, CreateTenderCommand, CreateTenderEngineerEntryCommand,
    DecideBidDecisionPackageCommand, DecideTenderRecordCommand, DecideWorkPlanProposalCommand,
    DeviceProtection, ImportTenderPackageCommand, InspectBidDecisionApprovalHistoryCommand,
    InvalidateBidDecisionApprovalCommand, ManagerCapabilityDemandInput, ParseSourceArtifactCommand,
    ProviderFailureCategory, QuantixHost, ResolveBidDecisionReturnReworkCommand,
    ReviseTenderCommand, ReviseWorkPlanProposalCommand, RunBidDecisionPackageReviewCommand,
    RunTenderRecordExtractionCommand, RuntimeLayout, SetupPlatform, SetupState, StoragePermissions,
    TenderErrorCode, TenderEvidenceReference, TenderIntegrityState, TenderLifecyclePhase,
    TenderRecordEngineerDecisionKind, TenderRecordInspection, TenderRecordKind,
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
            .contains(&"2026-05-15T14:00:00+03:00".into())
    }));
    assert!(proposal
        .tasks
        .iter()
        .all(|task| task.deadline == "2026-05-15T14:00:00+03:00"));
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
            binding.profile.capabilities.len() == 1
                && binding.profile.permissions.data_scopes.len() == 1
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
    assert_eq!(recombined.profile.permissions.data_scopes.len(), 2);
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
        .all(|binding| binding.status == AgentProfileStatus::Active));
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
        .all(|binding| binding.status == AgentProfileStatus::Active));
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
        .all(|binding| binding.status == AgentProfileStatus::Active));

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
