use std::{fs, io, path::Path, process::Command, sync::Arc};

use quantix_lib::{
    ensure_quantix_setup, ActivateTenderProductionCommand, AgentRunState, AgentTaskInputReference,
    AiProviderKind, ApproveExternalRfiForIssueCommand, BidDecisionApprovalDecision, CodexReadiness,
    ComplianceDisposition, ComplianceDispositionUpdate, ComposeTenderOfficeCommand,
    ConfirmAiExecutionSelectionCommand, CreateBidDecisionPackageCommand,
    CreateExternalRfiDraftCommand, CreatePortableTenderArchiveCommand, CreateTenderBackupCommand,
    CreateTenderCommand, DecideBidDecisionPackageCommand, DecideTenderQueryTreatmentCommand,
    DecideTenderRecordCommand, DecideWorkPlanProposalCommand, ErasedTenderCopyClass,
    ExternalRfiQueryReference, ExternalRfiRecipient, ImportTenderPackageCommand,
    InspectManagerWorkspaceCommand, InspectTenderAiExecutionCommand, InspectTenderQueriesCommand,
    IntakeExceptionCode, InterpretExternalRfiResponseCommand, ManagerIntakeStage,
    ManagerIntakeStatusKind, ManagerWorkspaceTenderState, ParseSourceArtifactCommand,
    ProductionTaskState, ProviderCleanupStatus, ProviderConnectionStatus,
    ProviderReasoningSelection, PurgeRecoveryRequiredTenderCommand, PurgeTrashedTenderCommand,
    QuantixHost, RecordEngineerWorkspaceMessageCommand, RegisterExternalRfiResponseCommand,
    RegistrationState, RunBidDecisionPackageReviewCommand, RunBootstrapAgentCommand,
    RunExternalRfiReviewCommand, RunProductionTaskCommand, RunTenderRecordExtractionCommand,
    RuntimeLayout, SelectManagerWorkspaceTenderCommand, SetupPlatform, SetupState,
    StartupReconciliationReport, StoragePermissions, TenderAiSelectionReadiness, TenderErrorCode,
    TenderEvidenceReference, TenderOfficeMessageAuthor, TenderOfficeMessageKind, TenderQuery,
    TenderQueryTreatment, TenderRecordEngineerDecisionKind, TenderRecordInspection,
    TenderRecordKind, TenderRecordVersionReference, TenderRetentionDecisionCommand,
    TenderRetentionState, TrashedTenderDecisionCommand, TrashedTenderState,
    UpdateAiExecutionSelectionCommand, WorkPlanDecision, WorkspaceActionKind,
    WorkspaceExternalRfiStatus, WorkspaceMessageReference, WorkspaceMessageReferenceKind,
    MINIMUM_SETUP_FREE_SPACE_BYTES,
};
use rusqlite::Connection;

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
}

fn run_storage_fixture(application_home: &Path, arguments: &[&str], failpoint: &str) -> bool {
    Command::new(env!("CARGO_BIN_EXE_quantix-storage-fixture"))
        .arg(application_home)
        .args(arguments)
        .env("QUANTIX_STORAGE_FAILPOINT", failpoint)
        .status()
        .expect("run supervised storage fixture")
        .success()
}

async fn establish_declined_tender(
    host: &QuantixHost,
    codex: &Path,
    application_home: &Path,
    source_root: &Path,
    tender_id: &str,
) {
    let source = source_root.join(format!("decline-source-{tender_id}"));
    fs::create_dir(&source).expect("decline source directory");
    fs::write(
        source.join("conditions.pdf"),
        b"%PDF-1.7\nTENDER_RECORD_GOLDEN\n%%EOF\n",
    )
    .expect("decline PDF fixture");
    fs::write(codex.with_extension("agent-scenario"), "manager-intake-bid")
        .expect("select complete Manager Intake fixture");
    fs::write(codex.with_extension("manager-output-release"), b"release")
        .expect("release Manager Intake output");
    host.import_tender_package(ImportTenderPackageCommand {
        tender_id: tender_id.into(),
        source_path: source.to_string_lossy().into_owned(),
    })
    .expect("import decline source");
    if let Err(error) = host.run_manager_intake_for_verification(tender_id).await {
        let workspace = host
            .inspect_manager_workspace(InspectManagerWorkspaceCommand {
                tender_id: Some(tender_id.into()),
            })
            .expect("inspect failed terminal Manager Intake");
        let database = application_home
            .join("tenders")
            .join(tender_id)
            .join("tender.sqlite");
        let agent_failure = Connection::open(database)
            .expect("open failed Manager Intake store")
            .query_row(
                "SELECT status, failure_json FROM agent_runs ORDER BY run_sequence DESC LIMIT 1",
                [],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
            )
            .ok();
        let fixture_error = fs::read_to_string(codex.with_extension("fixture-error")).ok();
        panic!(
            "complete Manager Intake before terminal decision: {error:?}; agent={agent_failure:?}; fixture={fixture_error:?}; {workspace:#?}"
        );
    }
    let intake = host
        .inspect_manager_workspace(InspectManagerWorkspaceCommand {
            tender_id: Some(tender_id.into()),
        })
        .expect("inspect completed terminal Manager Intake")
        .intake
        .expect("terminal Manager Intake status");
    if intake.stage != ManagerIntakeStage::BidDecisionReady {
        let database = application_home
            .join("tenders")
            .join(tender_id)
            .join("tender.sqlite");
        let connection = Connection::open(database).expect("open incomplete Manager Intake store");
        let mut statement = connection
            .prepare(
                "SELECT ar.run_sequence, ar.profile_id, ar.profile_version, ar.status,
                        ar.provider_thread_ref, ar.provider_turn_ref, ar.failure_json,
                        group_concat(pe.kind, ' -> ')
                 FROM agent_runs ar
                 LEFT JOIN provider_events pe ON pe.run_id = ar.run_id
                 GROUP BY ar.run_id
                 ORDER BY ar.run_sequence",
            )
            .expect("prepare incomplete Agent Runs");
        let runs = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, u32>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, u32>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<String>>(7)?,
                ))
            })
            .expect("query incomplete Agent Runs")
            .collect::<rusqlite::Result<Vec<_>>>()
            .expect("collect incomplete Agent Runs");
        let fixture_error = fs::read_to_string(codex.with_extension("fixture-error")).ok();
        panic!(
            "Manager Intake did not reach Bid Decision: {intake:#?}; runs={runs:#?}; fixture={fixture_error:?}"
        );
    }

    let records = inspect_all_records(host, tender_id);
    let package = host
        .create_bid_decision_package(CreateBidDecisionPackageCommand {
            tender_id: tender_id.into(),
            base_version: None,
            disposition_updates: complete_dispositions(&records),
            manager_capability_demands: Vec::new(),
        })
        .expect("create decline Bid Decision Package");
    fs::write(codex.with_extension("agent-scenario"), "bid-package-review")
        .expect("select Bid Decision review fixture");
    let review = host
        .run_bid_decision_package_review(RunBidDecisionPackageReviewCommand {
            tender_id: tender_id.into(),
            package_id: package.package_id,
            version: package.version,
        })
        .await;
    let package = match review {
        Ok(review) => review.package,
        Err(error) => {
            let integrity = host.inspect_tender_integrity(tender_id).ok();
            let fixture_error = fs::read_to_string(codex.with_extension("fixture-error")).ok();
            panic!(
                "review decline Bid Decision Package: {error:?}; fixture={fixture_error:?}; integrity={integrity:#?}"
            );
        }
    };
    host.decide_bid_decision_package(DecideBidDecisionPackageCommand {
        tender_id: tender_id.into(),
        package_id: package.package_id,
        version: package.version,
        manifest_sha256: package.manifest_sha256,
        decision: BidDecisionApprovalDecision::Reject,
        rationale: "Tendering Manager reviewed the exact package and records the Decline.".into(),
        conditions: Vec::new(),
        exceptions: Vec::new(),
        required_rework: Vec::new(),
    })
    .expect("record authentic terminal Decline");
}

async fn approve_fixture_ai_selection(host: &QuantixHost) {
    let settings = host
        .refresh_application_settings()
        .await
        .expect("refresh fixture provider");
    let connection = settings
        .provider_connections
        .iter()
        .find(|connection| connection.status == ProviderConnectionStatus::Ready)
        .expect("ready fixture provider");
    let model = connection.models.first().expect("fixture model");
    let reasoning = model
        .reasoning_options
        .iter()
        .find(|option| option.is_default)
        .or_else(|| model.reasoning_options.first())
        .expect("fixture reasoning");
    host.confirm_ai_execution_selection(ConfirmAiExecutionSelectionCommand {
        connection_id: connection.connection_id.clone(),
        model_id: model.model_id.clone(),
        reasoning: reasoning.selection.clone(),
    })
    .await
    .expect("approve fixture AI selection");
}

#[tokio::test]
async fn rate_limited_manager_wait_is_persisted_atomically() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let resources = user_home.path().join("resources");
    install_codex_fixture(&resources, "rate-limited");
    let host = QuantixHost::with_setup_platform_and_runtime(
        &application_home,
        Arc::new(ReadySetupPlatform),
        RuntimeLayout::bundled(resources),
    );
    host.accept_runtime_fixture();
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    approve_fixture_ai_selection(&host).await;
    install_ocr_fixture(&application_home);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Durable cooldown Tender".into(),
        })
        .expect("create Tender");
    let package = user_home.path().join("cooldown-source");
    fs::create_dir_all(&package).expect("create package");
    fs::write(
        package.join("ITT.pdf"),
        b"%PDF-1.7\nTENDER_RECORD_GOLDEN\n%%EOF\n",
    )
    .expect("write package");
    host.import_tender_package(ImportTenderPackageCommand {
        tender_id: tender.tender_id.clone(),
        source_path: package.to_string_lossy().into_owned(),
    })
    .expect("register package");

    host.run_manager_intake_for_verification(&tender.tender_id)
        .await
        .expect("rate-limited intake waits durably");

    let database = application_home
        .join("tenders")
        .join(&tender.tender_id)
        .join("tender.sqlite");
    let connection = Connection::open(database).expect("open Tender store");
    let ordinary_wait: (Option<String>, Option<i64>, u32) = connection
        .query_row(
            "SELECT blocking_agent_run_id, retry_not_before_epoch_seconds,
                    provider_retry_attempt_count
             FROM manager_intake_runs ORDER BY intake_run_sequence DESC LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("read non-rate-limited provider wait");
    assert_eq!(ordinary_wait, (None, None, 0));
    drop(connection);
    let source_run = host
        .run_rate_limited_bootstrap_agent_for_verification(RunBootstrapAgentCommand {
            tender_id: tender.tender_id.clone(),
            retry_of_run_id: None,
        })
        .await
        .expect("create completed rate-limited provider run");
    let source_run_id = source_run.run_id;
    host.persist_manager_rate_limit_for_verification(&tender.tender_id, &source_run_id)
        .expect("persist completed rate-limit result");
    let connection = Connection::open(
        application_home
            .join("tenders")
            .join(&tender.tender_id)
            .join("tender.sqlite"),
    )
    .expect("reopen Tender store");
    let now: i64 = connection
        .query_row("SELECT unixepoch('now')", [], |row| row.get(0))
        .expect("read SQLite current time");
    let (stage, blocking_run_id, deadline, attempts): (String, Option<String>, Option<i64>, u32) =
        connection
            .query_row(
                "SELECT stage, blocking_agent_run_id,
                    retry_not_before_epoch_seconds, provider_retry_attempt_count
             FROM manager_intake_runs ORDER BY intake_run_sequence DESC LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .expect("read durable Manager cooldown");
    assert_eq!(stage, "waiting_for_provider");
    let blocking_run_id = blocking_run_id.expect("blocking Agent Run");
    assert_eq!(blocking_run_id, source_run_id);
    let delay_seconds = deadline.expect("automatic retry deadline") - now;
    assert!((59..=60).contains(&delay_seconds));
    assert_eq!(attempts, 1);
    let audit_count: u32 = connection
        .query_row(
            "SELECT COUNT(*) FROM audit_events
             WHERE event_type = 'manager_intake_provider_cooldown_started'
               AND json_extract(payload_json, '$.change.blocking_agent_run_id') = ?1
               AND json_extract(payload_json, '$.change.provider_retry_attempt_count') = 1",
            [&blocking_run_id],
            |row| row.get(0),
        )
        .expect("read cooldown audit");
    assert_eq!(audit_count, 1);
    drop(connection);
    host.persist_manager_rate_limit_for_verification(&tender.tender_id, &source_run_id)
        .expect("duplicate completion is idempotent");
    let (attempts, audit_count): (u32, u32) = Connection::open(
        application_home
            .join("tenders")
            .join(&tender.tender_id)
            .join("tender.sqlite"),
    )
    .expect("open idempotent cooldown")
    .query_row(
        "SELECT mir.provider_retry_attempt_count,
                (SELECT COUNT(*) FROM audit_events
                 WHERE event_type = 'manager_intake_provider_cooldown_started')
         FROM manager_intake_runs mir ORDER BY intake_run_sequence DESC LIMIT 1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .expect("inspect idempotent cooldown");
    assert_eq!((attempts, audit_count), (1, 1));

    Connection::open(
        application_home
            .join("tenders")
            .join(&tender.tender_id)
            .join("tender.sqlite"),
    )
    .expect("open cooldown for resume")
    .execute(
        "UPDATE manager_intake_runs
         SET retry_not_before_epoch_seconds = unixepoch('now') - 1",
        [],
    )
    .expect("expire cooldown before legitimate resume");
    assert_eq!(
        host.begin_manager_intake_processing_for_verification(&tender.tender_id)
            .expect("resume expired cooldown"),
        ManagerIntakeStage::ReadingDocuments
    );
    let state_after_resume: (String, Option<String>, Option<i64>, u32, u32, u32) =
        Connection::open(
            application_home
                .join("tenders")
                .join(&tender.tender_id)
                .join("tender.sqlite"),
        )
        .expect("open resumed cooldown")
        .query_row(
            "SELECT mir.stage, mir.blocking_agent_run_id,
                    mir.retry_not_before_epoch_seconds,
                    mir.provider_retry_attempt_count,
                    (SELECT COUNT(*) FROM audit_events
                     WHERE event_type = 'manager_intake_provider_cooldown_started'),
                    (SELECT COUNT(*)
                     FROM manager_intake_provider_rate_limit_consumptions)
             FROM manager_intake_runs mir
             ORDER BY intake_run_sequence DESC LIMIT 1",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .expect("inspect resumed cooldown");
    assert_eq!(
        state_after_resume,
        ("reading_documents".into(), None, None, 1, 1, 1)
    );
    let reopened = QuantixHost::with_setup_platform_and_runtime(
        &application_home,
        Arc::new(ReadySetupPlatform),
        RuntimeLayout::bundled(user_home.path().join("resources")),
    );
    reopened.accept_runtime_fixture();
    assert_eq!(ensure_quantix_setup(&reopened).state, SetupState::Ready);
    reopened
        .persist_manager_rate_limit_for_verification(&tender.tender_id, &source_run_id)
        .expect("replayed source remains consumed after reopen");
    reopened
        .open_tender(&tender.tender_id)
        .expect("cold open accepts durable rate-limit consumption history");
    let state_after_replay: (String, Option<String>, Option<i64>, u32, u32, u32) =
        Connection::open(
            application_home
                .join("tenders")
                .join(&tender.tender_id)
                .join("tender.sqlite"),
        )
        .expect("open replayed cooldown")
        .query_row(
            "SELECT mir.stage, mir.blocking_agent_run_id,
                    mir.retry_not_before_epoch_seconds,
                    mir.provider_retry_attempt_count,
                    (SELECT COUNT(*) FROM audit_events
                     WHERE event_type = 'manager_intake_provider_cooldown_started'),
                    (SELECT COUNT(*)
                     FROM manager_intake_provider_rate_limit_consumptions)
             FROM manager_intake_runs mir
             ORDER BY intake_run_sequence DESC LIMIT 1",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .expect("inspect replayed cooldown");
    assert_eq!(state_after_replay, state_after_resume);

    let mut prior_run_id = source_run_id;
    for expected_attempt in 2..=4 {
        let source_run = host
            .run_rate_limited_bootstrap_agent_for_verification(RunBootstrapAgentCommand {
                tender_id: tender.tender_id.clone(),
                retry_of_run_id: None,
            })
            .await
            .expect("create distinct completed provider run");
        let next_run_id = source_run.run_id;
        assert_ne!(next_run_id, prior_run_id);
        host.persist_manager_rate_limit_for_verification(&tender.tender_id, &next_run_id)
            .expect("persist next completed rate-limit result");
        prior_run_id = next_run_id;

        let connection = Connection::open(
            application_home
                .join("tenders")
                .join(&tender.tender_id)
                .join("tender.sqlite"),
        )
        .expect("inspect bounded cooldown");
        let (stage, deadline, attempts): (String, Option<i64>, u32) = connection
            .query_row(
                "SELECT stage, retry_not_before_epoch_seconds,
                        provider_retry_attempt_count
                 FROM manager_intake_runs ORDER BY intake_run_sequence DESC LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("read bounded cooldown state");
        assert_eq!(attempts, expected_attempt);
        if expected_attempt <= 3 {
            assert_eq!(stage, "waiting_for_provider");
            assert!(deadline.is_some());
        } else {
            assert_eq!(stage, "failed");
            assert_eq!(deadline, None);
            let exhausted: u32 = connection
                .query_row(
                    "SELECT COUNT(*) FROM audit_events
                     WHERE event_type = 'manager_intake_provider_retry_exhausted'",
                    [],
                    |row| row.get(0),
                )
                .expect("read exhausted retry audit");
            assert_eq!(exhausted, 1);
        }
    }
}

#[tokio::test]
async fn engineer_retry_preserves_partial_provider_retry_consumption() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let resources = user_home.path().join("resources");
    install_codex_fixture(&resources, "rate-limited");
    let host = QuantixHost::with_setup_platform_and_runtime(
        &application_home,
        Arc::new(ReadySetupPlatform),
        RuntimeLayout::bundled(resources.clone()),
    );
    host.accept_runtime_fixture();
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    approve_fixture_ai_selection(&host).await;
    install_ocr_fixture(&application_home);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Partial cooldown retry Tender".into(),
        })
        .expect("create Tender");
    let package = user_home.path().join("partial-cooldown-source");
    fs::create_dir_all(&package).expect("create source package");
    fs::write(
        package.join("ITT.pdf"),
        b"%PDF-1.7\nTENDER_RECORD_GOLDEN\n%%EOF\n",
    )
    .expect("write source package");
    host.import_tender_package(ImportTenderPackageCommand {
        tender_id: tender.tender_id.clone(),
        source_path: package.to_string_lossy().into_owned(),
    })
    .expect("register source package");
    host.run_manager_intake_for_verification(&tender.tender_id)
        .await
        .expect("prepare Manager intake");
    let first = host
        .run_rate_limited_bootstrap_agent_for_verification(RunBootstrapAgentCommand {
            tender_id: tender.tender_id.clone(),
            retry_of_run_id: None,
        })
        .await
        .expect("create first rate-limited run");
    host.persist_manager_rate_limit_for_verification(&tender.tender_id, &first.run_id)
        .expect("persist first cooldown consumption");
    let database = application_home
        .join("tenders")
        .join(&tender.tender_id)
        .join("tender.sqlite");
    Connection::open(&database)
        .expect("open Manager intake store")
        .execute(
            "UPDATE manager_intake_runs
             SET stage = 'failed', blocking_agent_run_id = NULL,
                 retry_not_before_epoch_seconds = NULL,
                 failure_summary = 'A non-rate local failure interrupted intake.',
                 completed_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                 updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
            [],
        )
        .expect("persist non-rate Manager failure after partial cooldown");
    host.set_document_tools_verified_for_verification(false);
    host.retry_manager_intake_for_verification(&tender.tender_id)
        .expect("Engineer retries failed intake");
    for _ in 0..100 {
        let stage: String = Connection::open(&database)
            .expect("open retrying Manager intake")
            .query_row(
                "SELECT stage FROM manager_intake_runs
                 ORDER BY intake_run_sequence DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .expect("read retrying Manager stage");
        if stage == "waiting_for_provider" {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    let preserved: (String, u32, u32) = Connection::open(&database)
        .expect("open queued Manager retry")
        .query_row(
            "SELECT stage, provider_retry_attempt_count,
                    (SELECT COUNT(*)
                     FROM manager_intake_provider_rate_limit_consumptions)
             FROM manager_intake_runs ORDER BY intake_run_sequence DESC LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("inspect preserved partial retry count");
    assert_eq!(preserved, ("waiting_for_provider".into(), 1, 1));
    drop(host);

    let reopened = QuantixHost::with_setup_platform_and_runtime(
        &application_home,
        Arc::new(ReadySetupPlatform),
        RuntimeLayout::bundled(resources),
    );
    reopened.accept_runtime_fixture();
    assert_eq!(ensure_quantix_setup(&reopened).state, SetupState::Ready);
    reopened
        .open_tender(&tender.tender_id)
        .expect("cold reopen accepts preserved partial retry count");
    let second = reopened
        .run_rate_limited_bootstrap_agent_for_verification(RunBootstrapAgentCommand {
            tender_id: tender.tender_id.clone(),
            retry_of_run_id: None,
        })
        .await
        .expect("create next rate-limited run");
    reopened
        .persist_manager_rate_limit_for_verification(&tender.tender_id, &second.run_id)
        .expect("advance the preserved retry count");
    let advanced: (u32, u32) = Connection::open(database)
        .expect("open advanced Manager cooldown")
        .query_row(
            "SELECT provider_retry_attempt_count,
                    (SELECT COUNT(*)
                     FROM manager_intake_provider_rate_limit_consumptions)
             FROM manager_intake_runs ORDER BY intake_run_sequence DESC LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("inspect advanced partial retry count");
    assert_eq!(advanced, (2, 2));
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

fn complete_dispositions(records: &[TenderRecordInspection]) -> Vec<ComplianceDispositionUpdate> {
    records
        .iter()
        .filter(|record| {
            record.version == 1
                && matches!(
                    record.kind,
                    TenderRecordKind::Requirement
                        | TenderRecordKind::EvaluationCriterion
                        | TenderRecordKind::Deliverable
                        | TenderRecordKind::Deadline
                        | TenderRecordKind::Form
                        | TenderRecordKind::Clause
                )
        })
        .map(|record| ComplianceDispositionUpdate {
            record: TenderRecordVersionReference {
                record_id: record.record_id.clone(),
                version: record.version,
            },
            disposition: ComplianceDisposition::Comply,
            responsibility: "Tender Office Coordinator".into(),
            planned_treatment: "Carry this exact verified obligation into controlled planning."
                .into(),
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
async fn public_host_archives_only_a_safe_terminal_tender_and_restores_its_workspace() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let resources = user_home.path().join("resources");
    let codex = install_codex_fixture(&resources, "manager-intake-bid");
    let host = QuantixHost::with_setup_platform_and_runtime(
        &application_home,
        Arc::new(ReadySetupPlatform),
        RuntimeLayout::bundled(resources),
    );
    host.accept_runtime_fixture();
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    approve_fixture_ai_selection(&host).await;
    install_ocr_fixture(&application_home);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Terminal Archive Tender".into(),
        })
        .expect("create Tender");
    let initial = host
        .inspect_manager_workspace(InspectManagerWorkspaceCommand {
            tender_id: Some(tender.tender_id.clone()),
        })
        .expect("inspect active Tender");
    let conversation_id = initial
        .conversation
        .expect("Manager conversation")
        .conversation_id;
    assert!(
        !initial
            .selected_tender
            .expect("selected Tender")
            .can_archive
    );
    let refused_delete = host
        .trash_tender(TenderRetentionDecisionCommand {
            tender_id: tender.tender_id.clone(),
            rationale: "Delete only after the safe terminal boundary.".into(),
        })
        .expect_err("active Tender must not move to Trash");
    assert_eq!(refused_delete.code, TenderErrorCode::InvalidCommand);
    let refused = host
        .archive_tender(TenderRetentionDecisionCommand {
            tender_id: tender.tender_id.clone(),
            rationale: "Archive only after the safe terminal boundary.".into(),
        })
        .expect_err("active Tender must not archive");
    assert_eq!(refused.code, TenderErrorCode::InvalidCommand);

    establish_declined_tender(
        &host,
        &codex,
        &application_home,
        user_home.path(),
        &tender.tender_id,
    )
    .await;
    let terminal = host
        .inspect_manager_workspace(InspectManagerWorkspaceCommand {
            tender_id: Some(tender.tender_id.clone()),
        })
        .expect("inspect terminal Tender");
    assert!(
        terminal
            .selected_tender
            .as_ref()
            .expect("selected Tender")
            .can_archive,
        "terminal={terminal:#?}; integrity={:#?}",
        host.inspect_tender_integrity(&tender.tender_id)
    );
    assert!(
        host.inspect_manager_workspace(InspectManagerWorkspaceCommand {
            tender_id: Some(tender.tender_id.clone()),
        })
        .expect("inspect terminal delete boundary")
        .selected_tender
        .expect("terminal Tender")
        .can_delete
    );

    let decision = host
        .archive_tender(TenderRetentionDecisionCommand {
            tender_id: tender.tender_id.clone(),
            rationale: "Keep the declined Tender available as read-only history.".into(),
        })
        .expect("archive terminal Tender");
    assert_eq!(decision.state, TenderRetentionState::Archived);
    assert_eq!(decision.decided_by, "engineer_user");
    assert_eq!(decision.acting_role, "tendering_engineer");
    let archived = host
        .inspect_manager_workspace(InspectManagerWorkspaceCommand {
            tender_id: Some(tender.tender_id.clone()),
        })
        .expect("open archived Tender");
    assert_eq!(
        archived.selected_tender.expect("archived Tender").state,
        ManagerWorkspaceTenderState::Archived
    );
    assert_eq!(
        archived
            .conversation
            .expect("archived conversation")
            .conversation_id,
        conversation_id
    );
    let read_only = host
        .record_engineer_workspace_message(RecordEngineerWorkspaceMessageCommand {
            tender_id: tender.tender_id.clone(),
            body: "Do not mutate archived history.".into(),
            attachment_refs: Vec::new(),
            context_refs: Vec::new(),
        })
        .expect_err("archived Tender stays read-only");
    assert_eq!(read_only.code, TenderErrorCode::InvalidCommand);

    host.restore_archived_tender(TenderRetentionDecisionCommand {
        tender_id: tender.tender_id.clone(),
        rationale: "Return the same Tender to active use.".into(),
    })
    .expect("restore archived Tender");
    let restored = host
        .select_manager_workspace_tender(SelectManagerWorkspaceTenderCommand {
            tender_id: tender.tender_id.clone(),
        })
        .expect("select restored Tender");
    assert_eq!(
        restored.selected_tender.expect("restored Tender").state,
        ManagerWorkspaceTenderState::Active
    );
    assert_eq!(
        restored
            .conversation
            .expect("restored conversation")
            .conversation_id,
        conversation_id
    );

    let trashed = host
        .trash_tender(TenderRetentionDecisionCommand {
            tender_id: tender.tender_id.clone(),
            rationale: "Move the declined Tender into recoverable Trash.".into(),
        })
        .expect("move terminal Tender to Trash");
    assert_eq!(trashed.state, TrashedTenderState::Trashed);
    assert_eq!(trashed.tender_id, tender.tender_id);
    assert_eq!(trashed.tender_name, "Terminal Archive Tender");
    assert_eq!(trashed.acting_role, "tendering_engineer");
    assert!(host
        .inspect_manager_workspace(InspectManagerWorkspaceCommand {
            tender_id: Some(tender.tender_id.clone()),
        })
        .is_err());
    assert_eq!(
        host.inspect_trashed_tenders()
            .expect("inspect recoverable Trash")
            .into_iter()
            .find(|record| record.deletion_id == trashed.deletion_id)
            .expect("exact Trash record")
            .state,
        TrashedTenderState::Trashed
    );

    let restored_from_trash = host
        .restore_trashed_tender(TrashedTenderDecisionCommand {
            deletion_id: trashed.deletion_id,
            rationale: "Restore the same verified Tender Store.".into(),
        })
        .expect("restore Tender from Trash");
    assert_eq!(restored_from_trash.state, TrashedTenderState::Restored);
    let restored_workspace = host
        .select_manager_workspace_tender(SelectManagerWorkspaceTenderCommand {
            tender_id: tender.tender_id.clone(),
        })
        .expect("select same restored Tender");
    assert_eq!(
        restored_workspace
            .conversation
            .expect("restored conversation")
            .conversation_id,
        conversation_id
    );

    let backup = host
        .create_tender_backup(CreateTenderBackupCommand {
            tender_id: tender.tender_id.clone(),
        })
        .expect("create managed Tender backup");
    let portable = host
        .create_portable_tender_archive(CreatePortableTenderArchiveCommand {
            tender_id: tender.tender_id.clone(),
        })
        .expect("create managed Portable Tender Archive");
    let exports = application_home.join("exports").join(&tender.tender_id);
    fs::create_dir(&exports).expect("create Tender delivery export root");
    fs::write(exports.join("delivery.txt"), b"managed export").expect("write managed export");
    let agent_workspace = application_home.join("staging").join(format!(
        "agent-{}-{}",
        tender.tender_id,
        "1".repeat(32)
    ));
    fs::create_dir(&agent_workspace).expect("create Agent workspace fixture");
    fs::write(agent_workspace.join("context.json"), b"{}").expect("write Agent fixture");
    let quarantine = application_home.join("staging").join(format!(
        "quarantine-agent-{}-{}",
        tender.tender_id,
        "2".repeat(32)
    ));
    fs::create_dir(&quarantine).expect("create quarantine fixture");
    fs::write(quarantine.join("partial.json"), b"{}").expect("write quarantine fixture");
    let log = application_home
        .join("logs")
        .join("tenders")
        .join(&tender.tender_id)
        .join("2026-08-21")
        .join("000001.jsonl");
    fs::create_dir_all(log.parent().expect("Tender log parent"))
        .expect("create Tender log fixture directory");
    fs::write(&log, b"Tender-scoped diagnostic").expect("write Tender log fixture");
    let application_log = application_home.join("logs/application/2026-08-21/000001.jsonl");
    fs::create_dir_all(application_log.parent().expect("application log parent"))
        .expect("create application log fixture directory");
    fs::write(&application_log, b"Application diagnostic").expect("write application log fixture");
    let unrelated_log = application_home
        .join("logs/tenders")
        .join("f".repeat(32))
        .join("2026-08-21/000001.jsonl");
    fs::create_dir_all(unrelated_log.parent().expect("unrelated log parent"))
        .expect("create unrelated log fixture directory");
    fs::write(&unrelated_log, b"Unrelated Tender diagnostic").expect("write unrelated log fixture");

    let permanently_trashed = host
        .trash_tender(TenderRetentionDecisionCommand {
            tender_id: tender.tender_id.clone(),
            rationale: "Prepare the exact Tender for permanent deletion.".into(),
        })
        .expect("move Tender into Trash for permanent deletion");
    let invalid_confirmation = host
        .purge_trashed_tender(PurgeTrashedTenderCommand {
            deletion_id: permanently_trashed.deletion_id.clone(),
            rationale: "This must not delete without the exact Tender name.".into(),
            confirmation_tender_name: "Wrong Tender".into(),
        })
        .expect_err("reject a mismatched permanent-deletion confirmation");
    assert_eq!(invalid_confirmation.code, TenderErrorCode::InvalidCommand);
    assert!(host
        .inspect_trashed_tenders()
        .expect("inspect Trash after rejected confirmation")
        .into_iter()
        .any(|record| record.deletion_id == permanently_trashed.deletion_id));
    let receipt = host
        .purge_trashed_tender(PurgeTrashedTenderCommand {
            deletion_id: permanently_trashed.deletion_id.clone(),
            rationale: "Erase every identifiable Quantix-controlled copy.".into(),
            confirmation_tender_name: "Terminal Archive Tender".into(),
        })
        .expect("permanently delete Quantix-controlled Tender copies");
    assert!(receipt.local_deletion_completed);
    assert_eq!(
        receipt.provider_cleanup_status,
        ProviderCleanupStatus::Pending
    );
    assert!(receipt.provider_thread_count > 0);
    assert_eq!(receipt.confirmed_provider_thread_deletions, 0);
    assert!(receipt
        .erased_copy_classes
        .contains(&ErasedTenderCopyClass::TenderStore));
    assert!(!application_home
        .join("backups")
        .join(format!("{}.qtbackup", backup.backup_id))
        .exists());
    assert!(!application_home
        .join("archives")
        .join(portable.relative_path)
        .exists());
    assert!(!exports.exists());
    assert!(!agent_workspace.exists());
    assert!(!quarantine.exists());
    assert!(!log.exists());
    assert!(application_log.exists());
    assert!(unrelated_log.exists());
    assert!(host
        .inspect_trashed_tenders()
        .expect("inspect empty Trash after deletion")
        .into_iter()
        .all(|record| record.deletion_id != permanently_trashed.deletion_id));
    assert_eq!(
        host.inspect_deletion_receipts()
            .expect("inspect content-free Deletion Receipt")
            .into_iter()
            .find(|candidate| candidate.receipt_id == receipt.receipt_id)
            .expect("exact Deletion Receipt"),
        receipt
    );
    let cannot_restore = host
        .restore_trashed_tender(TrashedTenderDecisionCommand {
            deletion_id: permanently_trashed.deletion_id,
            rationale: "A receipt must never restore a Tender.".into(),
        })
        .expect_err("permanently deleted Tender cannot be restored");
    assert_eq!(cannot_restore.code, TenderErrorCode::InvalidCommand);
}

#[tokio::test]
async fn recovery_purge_copies_readable_provider_references_to_pending_cleanup_jobs() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let resources = user_home.path().join("recovery-provider-resources");
    let codex = install_codex_fixture(&resources, "manager-intake-bid");
    let host = QuantixHost::with_setup_platform_and_runtime(
        &application_home,
        Arc::new(ReadySetupPlatform),
        RuntimeLayout::bundled(resources),
    );
    host.accept_runtime_fixture();
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    approve_fixture_ai_selection(&host).await;
    install_ocr_fixture(&application_home);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Readable Provider Recovery".into(),
        })
        .expect("create provider recovery fixture Tender");
    establish_declined_tender(
        &host,
        &codex,
        &application_home,
        user_home.path(),
        &tender.tender_id,
    )
    .await;
    host.close_tender(&tender.tender_id)
        .expect("close provider recovery fixture Tender");
    Connection::open(
        application_home
            .join("tenders")
            .join(&tender.tender_id)
            .join("tender.sqlite"),
    )
    .expect("open provider recovery fixture Store")
    .execute_batch("DROP TRIGGER audit_events_no_update")
    .expect("create schema-mismatched recovery Store");
    assert_eq!(
        host.inspect_tender_integrity(&tender.tender_id)
            .expect("inspect schema-mismatched Store")
            .state,
        quantix_lib::TenderIntegrityState::RecoveryRequired
    );

    let receipt = host
        .purge_recovery_required_tender(PurgeRecoveryRequiredTenderCommand {
            tender_id: tender.tender_id.clone(),
            rationale: "Delete the damaged Store and clean its provider threads".into(),
            confirmation_tender_name: "Readable Provider Recovery".into(),
        })
        .expect("purge recovery Store with readable provider references");
    assert!(receipt.provider_thread_count > 0);
    assert_eq!(
        receipt.provider_reference_discovery,
        quantix_lib::ProviderReferenceDiscoveryState::Complete
    );
    assert_eq!(
        receipt.provider_cleanup_status,
        ProviderCleanupStatus::Pending
    );
    let durable_jobs: u32 = Connection::open(application_home.join("installation.sqlite"))
        .expect("open provider cleanup ledger")
        .query_row(
            "SELECT COUNT(*) FROM provider_cleanup_jobs WHERE deletion_id = ?1",
            [&receipt.deletion_id],
            |row| row.get(0),
        )
        .expect("count durable provider cleanup jobs");
    assert_eq!(durable_jobs, receipt.provider_thread_count);
}

#[tokio::test]
async fn permanent_deletion_reconciles_each_local_publication_boundary_without_tender_content() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    for failpoint in ["purge_after_decision", "purge_after_local_delete"] {
        let application_home = user_home.path().join(failpoint);
        let resources = user_home.path().join(format!("{failpoint}-resources"));
        let codex = install_codex_fixture(&resources, "manager-intake-bid");
        let host = QuantixHost::with_setup_platform_and_runtime(
            &application_home,
            Arc::new(ReadySetupPlatform),
            RuntimeLayout::bundled(resources),
        );
        host.accept_runtime_fixture();
        assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
        approve_fixture_ai_selection(&host).await;
        install_ocr_fixture(&application_home);
        let tender_name = format!("Confidential deletion fixture {failpoint}");
        let tender = host
            .create_tender(CreateTenderCommand {
                name: tender_name.clone(),
            })
            .expect("create deletion fixture Tender");
        establish_declined_tender(
            &host,
            &codex,
            &application_home,
            user_home.path(),
            &tender.tender_id,
        )
        .await;
        let trashed = host
            .trash_tender(TenderRetentionDecisionCommand {
                tender_id: tender.tender_id.clone(),
                rationale: "Prepare the exact Tender for permanent deletion.".into(),
            })
            .expect("move fixture Tender into Trash");
        drop(host);

        assert!(!run_storage_fixture(
            &application_home,
            &["purge-trash", &trashed.deletion_id, &tender_name],
            failpoint,
        ));
        let cleanup_jobs_before_reconciliation: u32 =
            Connection::open(application_home.join("installation.sqlite"))
                .expect("open pre-reconciliation cleanup ledger")
                .query_row(
                    "SELECT COUNT(*) FROM provider_cleanup_jobs WHERE deletion_id = ?1",
                    [&trashed.deletion_id],
                    |row| row.get(0),
                )
                .expect("provider cleanup jobs are durable before local deletion completion");
        assert!(cleanup_jobs_before_reconciliation > 0);

        let restarted =
            QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
        assert_eq!(ensure_quantix_setup(&restarted).state, SetupState::Ready);
        assert!(restarted
            .inspect_trashed_tenders()
            .expect("reconcile interrupted permanent deletion")
            .is_empty());
        let receipts = restarted
            .inspect_deletion_receipts()
            .expect("inspect reconciled Deletion Receipt");
        assert_eq!(receipts.len(), 1);
        assert!(receipts[0].local_deletion_completed);
        let receipt_json: String = Connection::open(application_home.join("installation.sqlite"))
            .expect("open installation catalogue")
            .query_row("SELECT receipt_json FROM deletion_receipts", [], |row| {
                row.get(0)
            })
            .expect("read minimal Deletion Receipt");
        assert!(!receipt_json.contains(&tender_name));
        assert!(!receipt_json.contains("Prepare the exact Tender"));
    }
}

#[tokio::test]
async fn public_host_projection_exposes_registered_intake_stage_and_package_provenance() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform_and_runtime(
        &application_home,
        Arc::new(ReadySetupPlatform),
        RuntimeLayout::bundled(user_home.path().join("resources")),
    );
    host.accept_runtime_fixture();
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Intake Projection Tender".into(),
        })
        .expect("create Tender");
    let package = user_home.path().join("source-package");
    let instructions = package.join("01 Instructions");
    fs::create_dir_all(&instructions).expect("create package structure");
    fs::write(
        instructions.join("ITT.pdf"),
        b"%PDF-1.7\n1 0 obj\n(Intake projection)\nendobj\n%%EOF\n",
    )
    .expect("write source document");
    fs::write(package.join("legacy.bin"), b"unsupported source").expect("write exception source");
    host.import_tender_package(ImportTenderPackageCommand {
        tender_id: tender.tender_id.clone(),
        source_path: package.to_string_lossy().into_owned(),
    })
    .expect("register package");

    let projection = host
        .inspect_manager_workspace(InspectManagerWorkspaceCommand {
            tender_id: Some(tender.tender_id),
        })
        .expect("inspect Manager workspace");
    let intake = projection.intake.expect("Manager intake status");
    assert_eq!(intake.stage, ManagerIntakeStage::WaitingForLocalTools);
    assert_eq!(intake.status, ManagerIntakeStatusKind::Waiting);
    assert_eq!(
        projection.current_action.kind,
        WorkspaceActionKind::ObserveIntake
    );
    assert_eq!(projection.files.tender_document_count, 2);
    let source = projection
        .files
        .tender_documents
        .iter()
        .find(|document| document.registration_state == RegistrationState::Registered)
        .expect("registered source provenance");
    assert_eq!(source.package_path, "01 Instructions/ITT.pdf");
    assert!(source.sha256.is_some());
    assert_eq!(source.registration_state, RegistrationState::Registered);
    assert_eq!(source.exception, None);
    let exception = projection
        .files
        .tender_documents
        .iter()
        .find(|document| document.package_path == "legacy.bin")
        .expect("exception source provenance");
    assert_eq!(exception.registration_state, RegistrationState::Exception);
    assert_eq!(exception.exception, Some(IntakeExceptionCode::Unsupported));
}

#[tokio::test]
async fn public_host_exposes_and_persists_the_live_codex_selection() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let resources = user_home.path().join("resources");
    install_codex_fixture(&resources, "success");
    let host = QuantixHost::with_setup_platform_and_runtime(
        &application_home,
        Arc::new(ReadySetupPlatform),
        RuntimeLayout::bundled(resources),
    );
    host.accept_runtime_fixture();
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    assert_eq!(
        host.inspect_codex_subscription(tokio_util::sync::CancellationToken::new())
            .await,
        CodexReadiness::Ready
    );

    let settings = host
        .refresh_application_settings()
        .await
        .expect("refresh live settings");
    let connection = settings
        .provider_connections
        .iter()
        .find(|connection| connection.connection_id == "codex_chatgpt")
        .expect("live Codex connection");
    assert_eq!(connection.models[0].model_id, "gpt-5.6-terra");
    let saved = host
        .update_ai_execution_selection(UpdateAiExecutionSelectionCommand {
            connection_id: connection.connection_id.clone(),
            model_id: connection.models[0].model_id.clone(),
            reasoning: ProviderReasoningSelection::Effort("medium".into()),
        })
        .await
        .expect("save exact selection");
    let selection = saved
        .ai_execution_selection
        .expect("persisted execution selection");
    assert_eq!(selection.provider, AiProviderKind::Codex);
    assert_eq!(selection.model_id, "gpt-5.6-terra");
    assert_eq!(
        selection.reasoning,
        ProviderReasoningSelection::Effort("medium".into())
    );
}

#[tokio::test]
async fn public_host_confirms_codex_selection_before_document_tools_are_ready() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let resources = user_home.path().join("resources");
    install_codex_fixture(&resources, "success");
    let host = QuantixHost::with_setup_platform_and_runtime(
        &application_home,
        Arc::new(ReadySetupPlatform),
        RuntimeLayout::bundled(resources),
    );
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    assert_eq!(
        host.inspect_codex_subscription(tokio_util::sync::CancellationToken::new())
            .await,
        CodexReadiness::Ready
    );

    let settings = host
        .refresh_application_settings()
        .await
        .expect("refresh live settings");
    let connection = settings
        .provider_connections
        .iter()
        .find(|connection| connection.connection_id == "codex_chatgpt")
        .expect("live Codex connection");
    let confirmed = host
        .confirm_ai_execution_selection(ConfirmAiExecutionSelectionCommand {
            connection_id: connection.connection_id.clone(),
            model_id: connection.models[0].model_id.clone(),
            reasoning: ProviderReasoningSelection::Effort("medium".into()),
        })
        .await
        .expect("confirm Codex selection without document tools");

    assert!(confirmed.ai_execution_selection.is_some());
    assert!(confirmed.ai_execution_approval.is_some());
}

#[tokio::test]
async fn approving_global_ai_selection_refreshes_existing_tender_binding() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let resources = user_home.path().join("resources");
    install_codex_fixture(&resources, "success");
    let host = QuantixHost::with_setup_platform_and_runtime(
        &application_home,
        Arc::new(ReadySetupPlatform),
        RuntimeLayout::bundled(resources),
    );
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    assert_eq!(
        host.inspect_codex_subscription(tokio_util::sync::CancellationToken::new())
            .await,
        CodexReadiness::Ready
    );
    let settings = host
        .refresh_application_settings()
        .await
        .expect("refresh live settings");
    let connection = settings
        .provider_connections
        .iter()
        .find(|connection| connection.connection_id == "codex_chatgpt")
        .expect("live Codex connection");
    host.update_ai_execution_selection(UpdateAiExecutionSelectionCommand {
        connection_id: connection.connection_id.clone(),
        model_id: connection.models[0].model_id.clone(),
        reasoning: ProviderReasoningSelection::Effort("medium".into()),
    })
    .await
    .expect("save exact global selection");
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Existing AI Binding Fixture".into(),
        })
        .expect("create Tender with pending approval");
    let before = host
        .inspect_tender_ai_execution(InspectTenderAiExecutionCommand {
            tender_id: tender.tender_id.clone(),
        })
        .expect("inspect pending Tender binding");
    assert_eq!(
        before.readiness,
        TenderAiSelectionReadiness::ApprovalRequired
    );

    host.confirm_ai_execution_selection(ConfirmAiExecutionSelectionCommand {
        connection_id: connection.connection_id.clone(),
        model_id: connection.models[0].model_id.clone(),
        reasoning: ProviderReasoningSelection::Effort("medium".into()),
    })
    .await
    .expect("approve exact global selection");
    let after = host
        .inspect_tender_ai_execution(InspectTenderAiExecutionCommand {
            tender_id: tender.tender_id,
        })
        .expect("inspect refreshed Tender binding");
    assert_eq!(after.readiness, TenderAiSelectionReadiness::Ready);
}

#[tokio::test]
async fn already_parsed_manager_intake_advances_to_provider_wait_without_duplicate_work() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let resources = user_home.path().join("resources");
    install_codex_fixture(&resources, "manager-intake-bid");
    let host = QuantixHost::with_setup_platform_and_runtime(
        &application_home,
        Arc::new(ReadySetupPlatform),
        RuntimeLayout::bundled(resources),
    );
    host.accept_runtime_fixture();
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    install_ocr_fixture(&application_home);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Already Parsed Intake Tender".into(),
        })
        .expect("create Tender");
    let package = user_home.path().join("already-parsed-source");
    fs::create_dir_all(&package).expect("create source package");
    fs::write(
        package.join("ITT.pdf"),
        b"%PDF-1.7\nBid security is required.\n%%EOF\n",
    )
    .expect("write source document");
    host.import_tender_package(ImportTenderPackageCommand {
        tender_id: tender.tender_id.clone(),
        source_path: package.to_string_lossy().into_owned(),
    })
    .expect("register source package");

    let database = application_home
        .join("tenders")
        .join(&tender.tender_id)
        .join("tender.sqlite");
    let (artifact_id, version): (String, u32) = Connection::open(&database)
        .expect("open Tender store")
        .query_row(
            "SELECT artifact_id, version FROM source_artifact_versions",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("read registered document identity");
    host.parse_source_artifact(ParseSourceArtifactCommand {
        tender_id: tender.tender_id.clone(),
        artifact_id,
        version,
    })
    .await
    .expect("parse document before Manager intake");
    host.set_document_tools_verified_for_verification(false);

    for _ in 0..2 {
        host.run_manager_intake_for_verification(&tender.tender_id)
            .await
            .expect("advance already-parsed intake");
        let projection = host
            .select_manager_workspace_tender(SelectManagerWorkspaceTenderCommand {
                tender_id: tender.tender_id.clone(),
            })
            .expect("read truthful projection after Manager cycle");
        assert_eq!(
            projection.intake.expect("Manager intake").stage,
            ManagerIntakeStage::WaitingForProviderApproval
        );
    }

    let persisted: (String, u32, u32, u32, u32) = Connection::open(database)
        .expect("reopen Tender store")
        .query_row(
            "SELECT mir.stage, mir.parseable_document_count,
                    mir.parsed_document_count,
                    (SELECT COUNT(*) FROM parse_attempts),
                    (SELECT COUNT(*) FROM agent_runs)
             FROM manager_intake_runs mir
             ORDER BY mir.intake_run_sequence DESC LIMIT 1",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .expect("inspect idempotent already-parsed transition");
    assert_eq!(
        persisted,
        ("waiting_for_provider_approval".into(), 1, 1, 1, 0)
    );
}

#[tokio::test]
async fn public_host_runs_real_manager_intake_while_engineer_switches_tenders() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let resources = user_home.path().join("resources");
    let codex = install_codex_fixture(&resources, "manager-intake");
    let host = QuantixHost::with_setup_platform_and_runtime(
        &application_home,
        Arc::new(ReadySetupPlatform),
        RuntimeLayout::bundled(resources),
    );
    host.accept_runtime_fixture();
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    approve_fixture_ai_selection(&host).await;
    install_ocr_fixture(&application_home);
    let intake_tender = host
        .create_tender(CreateTenderCommand {
            name: "Live Intake Tender".into(),
        })
        .expect("create intake Tender");
    let other_tender = host
        .create_tender(CreateTenderCommand {
            name: "Other Tender".into(),
        })
        .expect("create other Tender");
    let package = user_home.path().join("manager-source-package");
    fs::create_dir_all(&package).expect("create package");
    fs::write(
        package.join("ITT.pdf"),
        b"%PDF-1.7\nTENDER_RECORD_GOLDEN\nSubmission deadline 15 May 2026 at 14:00 Cairo time.\nSubmission deadline 16 May 2026 at 14:00 Cairo time.\n%%EOF\n",
    )
    .expect("write package");
    host.import_tender_package(ImportTenderPackageCommand {
        tender_id: intake_tender.tender_id.clone(),
        source_path: package.to_string_lossy().into_owned(),
    })
    .expect("register package");

    let intake_host = host.clone();
    let intake_id = intake_tender.tender_id.clone();
    let worker = tokio::spawn(async move {
        intake_host
            .run_manager_intake_for_verification(&intake_id)
            .await
    });
    let waiting = codex.with_extension("manager-output-waiting");
    for _ in 0..2_000 {
        if waiting.is_file() || worker.is_finished() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    if !waiting.is_file() {
        let result = worker.await.expect("join early Manager intake worker");
        let workspace = host
            .inspect_manager_workspace(InspectManagerWorkspaceCommand {
                tender_id: Some(intake_tender.tender_id.clone()),
            })
            .expect("inspect early Manager intake result");
        let fixture_error = fs::read_to_string(codex.with_extension("fixture-error")).ok();
        let database = application_home
            .join("tenders")
            .join(&intake_tender.tender_id)
            .join("tender.sqlite");
        let connection = Connection::open(database).expect("open early Manager intake store");
        let runs = connection
            .prepare("SELECT status, failure_json FROM agent_runs ORDER BY run_sequence")
            .and_then(|mut statement| {
                statement
                    .query_map([], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .expect("inspect early Agent Runs");
        panic!(
            "Manager did not reach its real Provider turn: result={result:?}; fixture={fixture_error:?}; runs={runs:#?}; workspace={workspace:#?}"
        );
    }
    host.select_manager_workspace_tender(SelectManagerWorkspaceTenderCommand {
        tender_id: other_tender.tender_id.clone(),
    })
    .expect("switch Tender during intake");
    let switched = host
        .inspect_manager_workspace(InspectManagerWorkspaceCommand::default())
        .expect("inspect switched Tender");
    assert_eq!(
        switched.selected_tender.expect("selected Tender").tender_id,
        other_tender.tender_id
    );
    fs::write(codex.with_extension("manager-output-release"), b"release")
        .expect("release Manager output");
    worker
        .await
        .expect("join intake worker")
        .expect("complete Manager intake");

    let completed = host
        .inspect_manager_workspace(InspectManagerWorkspaceCommand {
            tender_id: Some(intake_tender.tender_id),
        })
        .expect("inspect completed intake");
    assert_eq!(
        completed.intake.expect("intake").stage,
        ManagerIntakeStage::WaitingForEngineer
    );
    let conversation = completed.conversation.expect("Manager conversation");
    let meaningful_id = conversation
        .latest_meaningful_message_id
        .expect("meaningful Manager question");
    let question = conversation
        .messages
        .iter()
        .find(|message| message.message_id == meaningful_id)
        .expect("Manager question");
    assert_eq!(question.author, TenderOfficeMessageAuthor::Manager);
    assert!(question
        .references
        .iter()
        .any(|reference| { reference.kind == WorkspaceMessageReferenceKind::TenderRecord }));
    assert!(question
        .references
        .iter()
        .any(|reference| { reference.kind == WorkspaceMessageReferenceKind::SourceEvidence }));
}

#[tokio::test]
async fn public_host_clean_intake_reaches_canonical_bid_recommendation() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let resources = user_home.path().join("resources");
    let codex = install_codex_fixture(&resources, "manager-intake-bid");
    fs::write(codex.with_extension("manager-output-release"), b"release")
        .expect("pre-release Manager output");
    let host = QuantixHost::with_setup_platform_and_runtime(
        &application_home,
        Arc::new(ReadySetupPlatform),
        RuntimeLayout::bundled(resources),
    );
    host.accept_runtime_fixture();
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    approve_fixture_ai_selection(&host).await;
    install_ocr_fixture(&application_home);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Clean Bid Tender".into(),
        })
        .expect("create Tender");
    let package = user_home.path().join("clean-source-package");
    fs::create_dir_all(&package).expect("create package");
    fs::write(
        package.join("ITT.pdf"),
        b"%PDF-1.7\nBid security is required.\n%%EOF\n",
    )
    .expect("write package");
    host.import_tender_package(ImportTenderPackageCommand {
        tender_id: tender.tender_id.clone(),
        source_path: package.to_string_lossy().into_owned(),
    })
    .expect("register package");
    host.run_manager_intake_for_verification(&tender.tender_id)
        .await
        .expect("complete clean Manager intake");

    let database = application_home
        .join("tenders")
        .join(&tender.tender_id)
        .join("tender.sqlite");
    let inspect_work_counts = || {
        Connection::open(&database)
            .expect("open completed Manager intake store")
            .query_row(
                "SELECT mir.parseable_document_count,
                        mir.parsed_document_count,
                        mir.extraction_run_count,
                        (SELECT COUNT(*) FROM parse_attempts),
                        (SELECT COUNT(*) FROM manager_intake_extraction_batches),
                        (SELECT COUNT(*) FROM tender_records),
                        (SELECT COUNT(*) FROM agent_runs),
                        (SELECT COUNT(*) FROM manager_intake_outcomes)
                 FROM manager_intake_runs mir
                 ORDER BY mir.intake_run_sequence DESC LIMIT 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, u32>(0)?,
                        row.get::<_, u32>(1)?,
                        row.get::<_, u32>(2)?,
                        row.get::<_, u32>(3)?,
                        row.get::<_, u32>(4)?,
                        row.get::<_, u32>(5)?,
                        row.get::<_, u32>(6)?,
                        row.get::<_, u32>(7)?,
                    ))
                },
            )
            .expect("inspect completed Manager intake work counts")
    };
    let before_reentry = inspect_work_counts();
    assert_eq!(before_reentry.0, 1, "one registered parseable document");
    assert_eq!(before_reentry.1, 1, "the registered document is parsed");
    assert_eq!(before_reentry.3, 1, "one governed parse attempt");
    assert!(before_reentry.2 > 0, "extraction completed");
    assert_eq!(
        before_reentry.2, before_reentry.4,
        "every counted extraction has one durable batch publication"
    );
    assert!(before_reentry.5 > 0, "Tender Records were published");
    assert!(
        before_reentry.6 > 0,
        "configured-provider Agent Runs were published"
    );
    assert_eq!(before_reentry.7, 1, "one Manager outcome was published");

    host.run_manager_intake_for_verification(&tender.tender_id)
        .await
        .expect("re-enter completed Manager intake");
    assert_eq!(
        inspect_work_counts(),
        before_reentry,
        "re-entry must not duplicate parse, extraction, Tender Record, Agent Run, or outcome publication"
    );

    let projection = host
        .inspect_manager_workspace(InspectManagerWorkspaceCommand {
            tender_id: Some(tender.tender_id),
        })
        .expect("inspect completed intake");
    assert_eq!(
        projection.intake.expect("intake").stage,
        ManagerIntakeStage::BidDecisionReady
    );
    let conversation = projection.conversation.expect("Manager conversation");
    let meaningful_id = conversation
        .latest_meaningful_message_id
        .expect("meaningful recommendation");
    let recommendation = conversation
        .messages
        .iter()
        .find(|message| message.message_id == meaningful_id)
        .expect("Manager recommendation");
    assert_eq!(
        recommendation.body,
        "Recommendation: Proceed.\n\nThe independently reviewed current Tender record supports controlled bid planning."
    );
    assert!(recommendation
        .references
        .iter()
        .any(|reference| { reference.kind == WorkspaceMessageReferenceKind::TenderRecord }));
    assert!(recommendation
        .references
        .iter()
        .any(|reference| { reference.kind == WorkspaceMessageReferenceKind::SourceEvidence }));
}

#[test]
fn public_host_projection_resumes_selection_and_persists_the_manager_conversation() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let empty = host
        .inspect_manager_workspace(InspectManagerWorkspaceCommand::default())
        .expect("inspect empty Manager workspace");
    assert!(empty.selected_tender.is_none());
    assert!(empty.conversation.is_none());
    assert_eq!(empty.current_action.kind, WorkspaceActionKind::StartTender);
    assert_eq!(empty.current_action.action_label, "Choose Tender Package");

    let first = host
        .create_tender(CreateTenderCommand {
            name: "First Tender".into(),
        })
        .expect("create first Tender");
    let second = host
        .create_tender(CreateTenderCommand {
            name: "Second Tender".into(),
        })
        .expect("create second Tender");
    host.select_manager_workspace_tender(SelectManagerWorkspaceTenderCommand {
        tender_id: second.tender_id.clone(),
    })
    .expect("select second Tender");

    let resumed = host
        .inspect_manager_workspace(InspectManagerWorkspaceCommand::default())
        .expect("inspect Manager workspace");
    assert_eq!(resumed.catalogue.len(), 2);
    assert_eq!(
        resumed
            .selected_tender
            .as_ref()
            .expect("last active Tender")
            .tender_id,
        second.tender_id
    );
    assert_eq!(
        resumed.current_action.kind,
        WorkspaceActionKind::AddTenderPackage
    );
    assert_eq!(resumed.work.needs_engineer, 0);
    assert_eq!(resumed.work.cancelled, 0);
    assert_eq!(resumed.files.tender_document_count, 0);
    assert_eq!(resumed.team.active_agent_runs, 0);

    host.select_manager_workspace_tender(SelectManagerWorkspaceTenderCommand {
        tender_id: first.tender_id.clone(),
    })
    .expect("select first Tender");
    let reopened = host
        .inspect_manager_workspace(InspectManagerWorkspaceCommand::default())
        .expect("resume selected Tender");
    assert_eq!(
        reopened
            .selected_tender
            .as_ref()
            .expect("selected Tender")
            .tender_id,
        first.tender_id
    );

    let updated = host
        .record_engineer_workspace_message(RecordEngineerWorkspaceMessageCommand {
            tender_id: first.tender_id.clone(),
            body: "Check the insurance exclusions first.".into(),
            attachment_refs: Vec::new(),
            context_refs: Vec::new(),
        })
        .expect("record Engineer message");
    let conversation = updated.conversation.expect("durable Manager conversation");
    assert_eq!(
        conversation.messages.first().expect("system status").author,
        TenderOfficeMessageAuthor::System
    );
    let message = conversation.messages.last().expect("Engineer message");
    assert_eq!(message.author, TenderOfficeMessageAuthor::Engineer);
    assert_eq!(message.kind, TenderOfficeMessageKind::Routine);
    assert_eq!(message.body, "Check the insurance exclusions first.");

    let foreign_reference = host
        .record_engineer_workspace_message(RecordEngineerWorkspaceMessageCommand {
            tender_id: first.tender_id.clone(),
            body: "Use this foreign Agent Run.".into(),
            attachment_refs: Vec::new(),
            context_refs: vec![WorkspaceMessageReference {
                kind: WorkspaceMessageReferenceKind::AgentRun,
                reference: "ffffffffffffffffffffffffffffffff".into(),
                version: 1,
                evidence_ordinal: None,
                label: "Foreign run".into(),
                detail: None,
            }],
        })
        .expect_err("references must belong to the selected Tender store");
    assert_eq!(foreign_reference.code, TenderErrorCode::InvalidCommand);
}

#[test]
fn engineer_messages_accept_artifact_version_and_tender_task_reference_kinds() {
    assert_eq!(
        serde_json::to_string(&WorkspaceMessageReferenceKind::ArtifactVersion).expect("serialize"),
        "\"artifact_version\""
    );
    assert_eq!(
        serde_json::to_string(&WorkspaceMessageReferenceKind::TenderTask).expect("serialize"),
        "\"tender_task\""
    );

    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Reference Kind Tender".into(),
        })
        .expect("create Tender");
    let package = user_home.path().join("reference-kind-source");
    fs::create_dir_all(&package).expect("create source package");
    fs::write(
        package.join("ITT.pdf"),
        b"%PDF-1.7\nTENDER_RECORD_GOLDEN\n%%EOF\n",
    )
    .expect("write source package");
    let imported = host
        .import_tender_package(ImportTenderPackageCommand {
            tender_id: tender.tender_id.clone(),
            source_path: package.to_string_lossy().into_owned(),
        })
        .expect("register source package");
    let artifact = imported
        .documents
        .first()
        .expect("registered source artifact")
        .clone();

    let database = application_home
        .join("tenders")
        .join(&tender.tender_id)
        .join("tender.sqlite");
    let (profile_id, profile_version): (String, u32) = Connection::open(&database)
        .expect("open reference-kind Tender Store")
        .query_row(
            "SELECT profile_id, version FROM agent_profile_versions
             WHERE identity = 'Tendering Manager' LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("read seeded Manager profile");
    let task_id = "1".repeat(32);
    Connection::open(&database)
        .expect("open Tender Store for the Task fixture")
        .execute(
            "INSERT INTO tender_tasks (
               task_id, profile_id, profile_version, objective, exact_inputs_json,
               output_contract_json, review_policy, deadline, permissions_json,
               resource_budget_json, created_at
             ) VALUES (?1, ?2, ?3, 'Produce the verified estimate.', '[]', '{}',
                       'independent_review', '2030-01-01T00:00:00Z',
                       '{\"data_scopes\":[],\"data_classifications\":[],\"allowed_actions\":[],\"allowed_tools\":[],\"network_allowed\":false,\"workspace_write_allowed\":true}',
                       '{\"provider_turns\":1,\"duration_seconds\":120,\"output_bytes\":262144}',
                       strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
            rusqlite::params![task_id, profile_id, profile_version],
        )
        .expect("seed exact Tender Task fixture");

    let before = host
        .inspect_manager_workspace(InspectManagerWorkspaceCommand {
            tender_id: Some(tender.tender_id.clone()),
        })
        .expect("inspect projection before recording");
    let before_messages = before.conversation.expect("prior conversation").messages;

    let updated = host
        .record_engineer_workspace_message(RecordEngineerWorkspaceMessageCommand {
            tender_id: tender.tender_id.clone(),
            body: "Link the exact Artifact Version and Task for this request.".into(),
            attachment_refs: Vec::new(),
            context_refs: vec![
                WorkspaceMessageReference {
                    kind: WorkspaceMessageReferenceKind::ArtifactVersion,
                    reference: artifact.artifact_id.clone(),
                    version: artifact.version,
                    evidence_ordinal: None,
                    label: artifact.package_path.clone(),
                    detail: None,
                },
                WorkspaceMessageReference {
                    kind: WorkspaceMessageReferenceKind::TenderTask,
                    reference: task_id.clone(),
                    version: 1,
                    evidence_ordinal: None,
                    label: "Produce the verified estimate.".into(),
                    detail: None,
                },
            ],
        })
        .expect("record Engineer message with the new reference kinds");

    let conversation = updated.conversation.expect("updated conversation");
    assert_eq!(conversation.messages.len(), before_messages.len() + 1);
    assert_eq!(
        conversation.messages[..before_messages.len()],
        before_messages[..],
        "projection messages other than the new one stay unchanged"
    );
    let message = conversation.messages.last().expect("recorded message");
    assert_eq!(message.author, TenderOfficeMessageAuthor::Engineer);
    assert!(message
        .references
        .iter()
        .any(|reference| reference.kind == WorkspaceMessageReferenceKind::ArtifactVersion));
    assert!(message
        .references
        .iter()
        .any(|reference| reference.kind == WorkspaceMessageReferenceKind::TenderTask));

    let foreign_artifact = host
        .record_engineer_workspace_message(RecordEngineerWorkspaceMessageCommand {
            tender_id: tender.tender_id.clone(),
            body: "Use this foreign Artifact Version.".into(),
            attachment_refs: Vec::new(),
            context_refs: vec![WorkspaceMessageReference {
                kind: WorkspaceMessageReferenceKind::ArtifactVersion,
                reference: "f".repeat(32),
                version: 9,
                evidence_ordinal: None,
                label: "Foreign artifact".into(),
                detail: None,
            }],
        })
        .expect_err("artifact_version references must exist in the Tender Store");
    assert_eq!(foreign_artifact.code, TenderErrorCode::InvalidCommand);
    let foreign_task = host
        .record_engineer_workspace_message(RecordEngineerWorkspaceMessageCommand {
            tender_id: tender.tender_id.clone(),
            body: "Use this foreign Tender Task.".into(),
            attachment_refs: Vec::new(),
            context_refs: vec![WorkspaceMessageReference {
                kind: WorkspaceMessageReferenceKind::TenderTask,
                reference: "e".repeat(32),
                version: 1,
                evidence_ordinal: None,
                label: "Foreign task".into(),
                detail: None,
            }],
        })
        .expect_err("tender_task references must exist in the Tender Store");
    assert_eq!(foreign_task.code, TenderErrorCode::InvalidCommand);
}

#[test]
fn selection_failure_cannot_follow_a_committed_engineer_message() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Failure Boundary Tender".into(),
        })
        .expect("create Tender");
    host.select_manager_workspace_tender(SelectManagerWorkspaceTenderCommand {
        tender_id: tender.tender_id.clone(),
    })
    .expect("establish selection");
    let before = host
        .inspect_manager_workspace(InspectManagerWorkspaceCommand {
            tender_id: Some(tender.tender_id.clone()),
        })
        .expect("inspect conversation")
        .conversation
        .expect("conversation")
        .messages
        .len();

    let connection = Connection::open(application_home.join("installation.sqlite"))
        .expect("open installation catalogue");
    connection
        .execute_batch(
            "CREATE TRIGGER manager_workspace_selection_test_failure
             BEFORE UPDATE ON manager_workspace_selection
             BEGIN SELECT RAISE(ABORT, 'injected selection failure'); END;",
        )
        .expect("inject selection failure");
    assert!(host
        .record_engineer_workspace_message(RecordEngineerWorkspaceMessageCommand {
            tender_id: tender.tender_id.clone(),
            body: "Do not record this message.".into(),
            attachment_refs: Vec::new(),
            context_refs: Vec::new(),
        })
        .is_err());
    connection
        .execute_batch("DROP TRIGGER manager_workspace_selection_test_failure;")
        .expect("remove selection failure");

    let after = host
        .inspect_manager_workspace(InspectManagerWorkspaceCommand {
            tender_id: Some(tender.tender_id),
        })
        .expect("inspect conversation after failure")
        .conversation
        .expect("conversation")
        .messages
        .len();
    assert_eq!(after, before);
}

#[test]
fn startup_reconciliation_stays_silent_for_a_healthy_workspace() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    host.create_tender(CreateTenderCommand {
        name: "Healthy Startup Tender".into(),
    })
    .expect("create Tender");

    assert_eq!(
        host.inspect_startup_reconciliation(),
        StartupReconciliationReport::default()
    );
}

#[test]
fn startup_reconciliation_reports_the_last_startup_cleanup_once() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Interrupted Backup Tender".into(),
        })
        .expect("create Tender");
    host.close_tender(&tender.tender_id).expect("close Tender");
    drop(host);

    assert!(!run_storage_fixture(
        &application_home,
        &["backup", &tender.tender_id],
        "backup_after_verify",
    ));
    let staged = application_home
        .join("staging")
        .join("tender-22222222222222222222222222222222");
    fs::create_dir(&staged).expect("stage interrupted Tender candidate");
    fs::write(staged.join("partial"), b"not committed").expect("write interrupted candidate");

    let restarted =
        QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    restarted
        .list_tenders()
        .expect("reconcile startup before listing");
    let report = restarted.inspect_startup_reconciliation();
    assert_eq!(report.removed_tender_candidates, 1);
    assert_eq!(report.interrupted_backup_operations, 1);
    assert_eq!(report.interrupted_recovery_operations, 0);
    assert_eq!(report.completed_retention_operations, 0);
    let records = restarted
        .inspect_tender_backups(&tender.tender_id)
        .expect("inspect closed backup");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].diagnostic_code.as_deref(), Some("interrupted"));

    restarted
        .list_tenders()
        .expect("list again after reconciliation");
    assert_eq!(restarted.inspect_startup_reconciliation(), report);
}

struct WorkspaceHarness {
    _root: tempfile::TempDir,
    codex: std::path::PathBuf,
    host: QuantixHost,
    tender_id: String,
}

impl WorkspaceHarness {
    fn new(agent_scenario: &str) -> Self {
        let root = tempfile::tempdir().expect("temporary workspace harness");
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
        host.approve_runtime_fixture_ai_selection()
            .expect("approve fixture AI selection");
        install_ocr_fixture(&application_home);
        let tender = host
            .create_tender(CreateTenderCommand {
                name: "Controlled External RFI Tender".into(),
            })
            .expect("create workspace harness Tender");
        Self {
            _root: root,
            codex,
            host,
            tender_id: tender.tender_id,
        }
    }

    fn set_agent_scenario(&self, scenario: &str) {
        fs::write(self.codex.with_extension("agent-scenario"), scenario)
            .expect("update fake app-server scenario");
        self.host
            .approve_runtime_fixture_ai_selection()
            .expect("restore approved fixture provider readiness");
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
            .expect("import decision source");
        let mut evidence = Vec::new();
        for document in &imported.documents {
            self.host
                .parse_source_artifact(ParseSourceArtifactCommand {
                    tender_id: self.tender_id.clone(),
                    artifact_id: document.artifact_id.clone(),
                    version: document.version,
                })
                .await
                .expect("parse source");
            evidence.extend(
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
                    }),
            );
        }
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

    fn verify_records(&self, records: &[TenderRecordInspection]) {
        for record in records.iter().filter(|record| record.version == 1) {
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
                    rationale: "Exact pre-bid basis verified for planning.".into(),
                })
                .expect("verify exact Tender Record");
        }
    }

    async fn activate_production(&self) {
        let records = self.extract_records().await;
        self.verify_records(&records);
        let package = self
            .host
            .create_bid_decision_package(CreateBidDecisionPackageCommand {
                tender_id: self.tender_id.clone(),
                base_version: None,
                disposition_updates: complete_dispositions(&records),
                manager_capability_demands: Vec::new(),
            })
            .expect("create decision package");
        self.set_agent_scenario("bid-package-review");
        let package = self
            .host
            .run_bid_decision_package_review(RunBidDecisionPackageReviewCommand {
                tender_id: self.tender_id.clone(),
                package_id: package.package_id,
                version: package.version,
            })
            .await
            .expect("review decision package")
            .package;
        self.host
            .decide_bid_decision_package(DecideBidDecisionPackageCommand {
                tender_id: self.tender_id.clone(),
                package_id: package.package_id.clone(),
                version: package.version,
                manifest_sha256: package.manifest_sha256.clone(),
                decision: BidDecisionApprovalDecision::Accept,
                rationale:
                    "Tendering Manager reviewed the exact package, Evidence, findings, and consequences."
                        .into(),
                conditions: vec!["Work Plan approval remains mandatory before production.".into()],
                exceptions: vec!["No exception grants production authority.".into()],
                required_rework: Vec::new(),
            })
            .expect("accept exact package");
        let plan = self
            .host
            .compose_tender_office(ComposeTenderOfficeCommand {
                tender_id: self.tender_id.clone(),
            })
            .expect("compose exact Work Plan");
        let approved = self
            .host
            .decide_work_plan_proposal(DecideWorkPlanProposalCommand {
                tender_id: self.tender_id.clone(),
                plan_id: plan.plan_id,
                version: plan.version,
                decision: WorkPlanDecision::Approve,
                rationale: "Approve the exact bounded production plan.".into(),
            })
            .expect("approve exact Work Plan");
        self.host
            .activate_tender_production(ActivateTenderProductionCommand {
                tender_id: self.tender_id.clone(),
                plan_id: approved.plan_id.clone(),
                plan_version: approved.version,
                plan_manifest_sha256: approved.manifest_sha256.clone(),
            })
            .expect("activate exact approved Work Plan");
    }

    async fn external_rfi_drafting_query(&self) -> TenderQuery {
        let production = self
            .host
            .inspect_tender_production(&self.tender_id)
            .expect("inspect active production")
            .expect("active production");
        let target = production
            .tasks
            .iter()
            .find(|task| task.state == ProductionTaskState::Ready)
            .expect("ready task for External RFI Query")
            .clone();
        self.set_agent_scenario("production-task-query-proposal");
        let proposed = self
            .host
            .run_production_task(RunProductionTaskCommand {
                tender_id: self.tender_id.clone(),
                production_task_id: target.production_task_id,
            })
            .await
            .expect("publish specialist Query proposal");
        assert_eq!(proposed.run.state, AgentRunState::Completed);
        let query = self
            .host
            .inspect_tender_queries(InspectTenderQueriesCommand {
                tender_id: self.tender_id.clone(),
                cursor: None,
                limit: 8,
            })
            .expect("inspect proposed External RFI Query")
            .items
            .into_iter()
            .next()
            .expect("External RFI Query");
        self.host
            .decide_tender_query_treatment(DecideTenderQueryTreatmentCommand {
                tender_id: self.tender_id.clone(),
                query_id: query.query_id.clone(),
                query_version: query.version,
                treatment: TenderQueryTreatment::ExternalRfiDrafting,
                rationale: "The exact ambiguity requires a controlled question to the Employer."
                    .into(),
                treatment_details:
                    "Draft, independently review, and obtain Manager approval before human issue."
                        .into(),
                closes_query: false,
            })
            .expect("authorize External RFI drafting")
    }

    fn external_rfi_create_command(&self, query: &TenderQuery) -> CreateExternalRfiDraftCommand {
        let source = self
            .host
            .inspect_document_register(&self.tender_id)
            .expect("inspect exact Source Artifact Register")
            .documents
            .into_iter()
            .next()
            .expect("registered source for External RFI attachment");
        CreateExternalRfiDraftCommand {
            tender_id: self.tender_id.clone(),
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

    fn inspect_workspace(&self) -> quantix_lib::ManagerWorkspaceProjection {
        self.host
            .inspect_manager_workspace(InspectManagerWorkspaceCommand {
                tender_id: Some(self.tender_id.clone()),
            })
            .expect("inspect Manager workspace")
    }
}

#[tokio::test]
async fn manager_workspace_projection_surfaces_an_rfi_action_when_a_draft_awaits_review() {
    let harness = WorkspaceHarness::new("record-extraction");
    harness.activate_production().await;
    let query = harness.external_rfi_drafting_query().await;

    let before_draft = harness.inspect_workspace();
    assert_eq!(
        before_draft.current_action.kind,
        WorkspaceActionKind::DraftExternalRfi,
        "a routed external question offers the gather action"
    );
    assert!(before_draft.current_action.requires_engineer);
    assert!(before_draft.external_rfis.is_empty());

    let draft = harness
        .host
        .create_external_rfi_draft(harness.external_rfi_create_command(&query))
        .expect("create External RFI draft");

    let projection = harness.inspect_workspace();
    assert_eq!(
        projection.current_action.kind,
        WorkspaceActionKind::ReviewExternalRfi
    );
    assert_eq!(
        projection.current_action.action_label,
        "Review External RFI"
    );
    assert!(projection.current_action.requires_engineer);
    assert!(
        projection
            .selected_tender
            .expect("selected Tender")
            .needs_engineer
    );
    let summaries = &projection.external_rfis;
    assert_eq!(summaries.len(), 1);
    let summary = &summaries[0];
    assert_eq!(summary.rfi_id, draft.rfi_id);
    assert_eq!(summary.version, 1);
    assert_eq!(summary.status, WorkspaceExternalRfiStatus::AwaitingReview);
    assert_eq!(summary.question_count, 1);
    assert_eq!(summary.response_count, 0);
    assert!(!summary.approval_pending);
    assert!(!summary.export_pending);
    assert!(!summary.interpretation_pending);

    harness
        .host
        .close_tender(&harness.tender_id)
        .expect("close projection Tender");
    let integrity = harness
        .host
        .inspect_tender_integrity(&harness.tender_id)
        .expect("cold-verify the projection fixture Tender");
    assert_eq!(
        integrity.state,
        quantix_lib::TenderIntegrityState::Ready,
        "{integrity:#?}"
    );
}

#[tokio::test]
async fn manager_workspace_approval_and_interpretation_record_attributable_manager_messages() {
    let harness = WorkspaceHarness::new("record-extraction");
    harness.activate_production().await;
    let query = harness.external_rfi_drafting_query().await;
    let draft = harness
        .host
        .create_external_rfi_draft(harness.external_rfi_create_command(&query))
        .expect("create External RFI draft");

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
    assert_eq!(reviewed.run.state, AgentRunState::Completed);

    let approved = harness
        .host
        .approve_external_rfi_for_issue(ApproveExternalRfiForIssueCommand {
            tender_id: harness.tender_id.clone(),
            rfi_id: draft.rfi_id.clone(),
            version: draft.version,
            manifest_sha256: draft.manifest_sha256.clone(),
            rationale: "The Tendering Manager approves this exact reviewed wording.".into(),
        })
        .expect("approve exact reviewed External RFI");
    assert!(approved.approved_for_issue);

    let approval_projection = harness.inspect_workspace();
    let approval_messages = approval_projection
        .conversation
        .expect("Manager conversation")
        .messages
        .into_iter()
        .filter(|message| {
            message.author == TenderOfficeMessageAuthor::Manager
                && message.body.contains("Approved External RFI")
        })
        .collect::<Vec<_>>();
    assert_eq!(approval_messages.len(), 1, "exactly one approval message");
    let approval_message = &approval_messages[0];
    assert_eq!(approval_message.kind, TenderOfficeMessageKind::Status);
    assert!(approval_message.body.contains(&draft.rfi_id));
    assert!(approval_message.body.contains(&draft.version.to_string()));

    let response_source = harness._root.path().join("external-rfi-response");
    fs::create_dir(&response_source).expect("response package directory");
    fs::write(
        response_source.join("employer-response.pdf"),
        "%PDF-1.7\nThe Employer response confirms the bidder responsibility boundary.\n%%EOF\n",
    )
    .expect("External RFI response fixture");
    let response_import = harness
        .host
        .import_tender_package(ImportTenderPackageCommand {
            tender_id: harness.tender_id.clone(),
            source_path: response_source.to_string_lossy().into_owned(),
        })
        .expect("register External RFI response through Intake");
    let response_document = response_import
        .documents
        .first()
        .expect("imported response document")
        .clone();
    let linked = harness
        .host
        .register_external_rfi_response(RegisterExternalRfiResponseCommand {
            tender_id: harness.tender_id.clone(),
            rfi_id: approved.rfi_id.clone(),
            rfi_version: approved.version,
            approval_id: approved
                .approval
                .as_ref()
                .expect("approval")
                .approval_id
                .clone(),
            source_artifact_id: response_document.artifact_id.clone(),
            source_artifact_version: response_document.version,
        })
        .expect("link immutable Intake response to exact RFI");
    let response = linked.responses.first().expect("response link").clone();

    let interpreted = harness
        .host
        .interpret_external_rfi_response(InterpretExternalRfiResponseCommand {
            tender_id: harness.tender_id.clone(),
            response_link_id: response.response_link_id.clone(),
            query_id: query.query_id.clone(),
            issued_query_version: query.version,
            base_query_version: query.version,
            base_query_manifest_sha256: query.manifest_sha256.clone(),
            material: true,
            interpretation: "The response confirms the bidder carries installation responsibility."
                .into(),
            treatment: TenderQueryTreatment::Qualification,
            rationale: "Preserve the exact responsibility split in the bid basis.".into(),
            treatment_details: "Qualify the price against the confirmed responsibility split."
                .into(),
            closes_query: false,
        })
        .expect("record Manager interpretation and exact Query successor");
    assert_eq!(interpreted.interpretations.len(), 1);

    let interpretation_projection = harness.inspect_workspace();
    let messages = interpretation_projection
        .conversation
        .expect("Manager conversation")
        .messages;
    let interpretation_messages = messages
        .iter()
        .filter(|message| {
            message.author == TenderOfficeMessageAuthor::Manager
                && message.body.contains("Interpreted the received response")
        })
        .collect::<Vec<_>>();
    assert_eq!(
        interpretation_messages.len(),
        1,
        "exactly one interpretation message"
    );
    let interpretation_message = interpretation_messages[0];
    assert_eq!(interpretation_message.kind, TenderOfficeMessageKind::Status);
    assert!(interpretation_message.body.contains(&draft.rfi_id));
    assert!(interpretation_message.body.contains(&query.query_id));
    assert!(
        messages
            .iter()
            .filter(
                |message| message.author == TenderOfficeMessageAuthor::Manager
                    && message.body.contains("Approved External RFI")
            )
            .count()
            == 1
    );
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

fn install_ocr_fixture(application_home: &Path) {
    let executable = application_home
        .join("runtimes")
        .join("ocr")
        .join(if cfg!(windows) { "Scripts" } else { "bin" })
        .join(executable_name("python"));
    fs::create_dir_all(executable.parent().expect("OCR executable parent"))
        .expect("OCR executable directory");
    fs::copy(
        Path::new(env!("CARGO_BIN_EXE_quantix-runtime-fixture")),
        &executable,
    )
    .expect("install OCR fixture");
    fs::write(executable.with_extension("version"), "3.9.2\n").expect("OCR fixture version");
    let models = application_home.join("models").join("ocr");
    fs::create_dir_all(&models).expect("model directory");
    for artifact in [
        "PP-OCRv6_det_small.onnx",
        "PP-OCRv6_rec_small.onnx",
        "ch_ppocr_mobile_v2.0_cls_mobile.onnx",
    ] {
        fs::write(models.join(artifact), format!("{artifact} fixture model"))
            .expect("model fixture");
    }
}

fn executable_name(name: &str) -> String {
    if std::env::consts::EXE_EXTENSION.is_empty() {
        name.to_owned()
    } else {
        format!("{name}.{}", std::env::consts::EXE_EXTENSION)
    }
}
