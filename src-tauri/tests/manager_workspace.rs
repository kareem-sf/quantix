use std::{fs, io, path::Path, process::Command, sync::Arc};

use quantix_lib::{
    ensure_quantix_setup, AiProviderKind, BidDecisionApprovalDecision, CancelProviderLoginCommand,
    CodexReadiness, ComplianceDisposition, ComplianceDispositionUpdate,
    CreateBidDecisionPackageCommand, CreatePortableTenderArchiveCommand, CreateTenderBackupCommand,
    CreateTenderCommand, DecideBidDecisionPackageCommand, DeviceProtection, ErasedTenderCopyClass,
    ImportTenderPackageCommand, InspectManagerWorkspaceCommand, ManagerIntakeStage,
    ManagerIntakeStatusKind, ManagerWorkspaceTenderState, ProviderCleanupStatus,
    ProviderConnectionStatus, ProviderLoginMethod, ProviderLoginStatus, ProviderReasoningSelection,
    PurgeTrashedTenderCommand, QuantixHost, RecordEngineerWorkspaceMessageCommand,
    RunBidDecisionPackageReviewCommand, RuntimeLayout, SelectManagerWorkspaceTenderCommand,
    SetupPlatform, SetupState, StartProviderLoginCommand, StoragePermissions, TenderErrorCode,
    TenderOfficeMessageAuthor, TenderOfficeMessageKind, TenderRecordInspection, TenderRecordKind,
    TenderRecordVersionReference, TenderRetentionDecisionCommand, TenderRetentionState,
    TrashedTenderDecisionCommand, TrashedTenderState, UpdateAiExecutionSelectionCommand,
    WorkspaceActionKind, WorkspaceMessageReferenceKind, MINIMUM_SETUP_FREE_SPACE_BYTES,
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

    fn device_protection(&self, _path: &Path) -> DeviceProtection {
        DeviceProtection::Protected
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
        .join(format!("tender-{}.log", tender.tender_id));
    fs::write(&log, b"Tender-scoped diagnostic").expect("write Tender log fixture");

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

#[test]
fn public_host_projection_exposes_registered_intake_stage_and_package_provenance() {
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
    assert_eq!(intake.stage, ManagerIntakeStage::WaitingForProvider);
    assert_eq!(intake.status, ManagerIntakeStatusKind::Waiting);
    assert_eq!(
        projection.current_action.kind,
        WorkspaceActionKind::ConfigureAiProvider
    );
    assert_eq!(projection.files.tender_document_count, 1);
    let source = projection
        .files
        .tender_documents
        .first()
        .expect("registered source provenance");
    assert_eq!(source.package_path, "01 Instructions/ITT.pdf");
    assert!(source.sha256.is_some());
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
            reasoning: ProviderReasoningSelection::CodexEffort("medium".into()),
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
        ProviderReasoningSelection::CodexEffort("medium".into())
    );
}

#[tokio::test]
async fn public_host_completes_managed_browser_login_refreshes_catalogue_and_logs_out() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let resources = user_home.path().join("resources");
    install_codex_fixture(&resources, "managed-login");
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
        CodexReadiness::AuthenticationRequired
    );

    let started = host
        .start_provider_login(StartProviderLoginCommand {
            method: ProviderLoginMethod::Browser,
        })
        .await
        .expect("start managed browser login");
    assert_eq!(
        started
            .active_provider_login
            .expect("active managed login")
            .status,
        ProviderLoginStatus::AwaitingUser
    );

    let mut connected = None;
    for _ in 0..1_000 {
        let settings = host
            .refresh_application_settings()
            .await
            .expect("refresh managed login");
        if settings.provider_connections.iter().any(|connection| {
            connection.connection_id == "codex_chatgpt"
                && connection.status == ProviderConnectionStatus::Ready
        }) {
            connected = Some(settings);
            break;
        }
        tokio::task::yield_now().await;
    }
    let connected = connected.expect("managed login completed");
    let connection = connected
        .provider_connections
        .iter()
        .find(|connection| connection.connection_id == "codex_chatgpt")
        .expect("connected Codex account");
    assert_eq!(
        connection.account_label.as_deref(),
        Some("engineer@example.com")
    );
    assert_eq!(connection.account_plan.as_deref(), Some("plus"));
    assert_eq!(connection.models[0].model_id, "gpt-5.6-terra");

    let disconnected = host
        .logout_provider()
        .await
        .expect("logout managed account");
    let connection = disconnected
        .provider_connections
        .iter()
        .find(|connection| connection.connection_id == "codex_chatgpt")
        .expect("disconnected Codex connection");
    assert_eq!(
        connection.status,
        ProviderConnectionStatus::AuthenticationRequired
    );
    assert!(disconnected.active_provider_login.is_none());
}

#[tokio::test]
async fn public_host_cancels_the_exact_managed_device_login() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let resources = user_home.path().join("resources");
    install_codex_fixture(&resources, "managed-login");
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
        CodexReadiness::AuthenticationRequired
    );

    let started = host
        .start_provider_login(StartProviderLoginCommand {
            method: ProviderLoginMethod::DeviceCode,
        })
        .await
        .expect("start managed device login");
    let login = started.active_provider_login.expect("active device login");
    assert_eq!(login.user_code.as_deref(), Some("ABCD-EFGH"));
    host.cancel_provider_login(CancelProviderLoginCommand {
        login_id: login.login_id,
    })
    .await
    .expect("cancel exact managed login");

    let mut cancelled = None;
    for _ in 0..1_000 {
        let settings = host
            .refresh_application_settings()
            .await
            .expect("refresh cancelled login");
        if settings
            .active_provider_login
            .as_ref()
            .is_some_and(|login| login.status == ProviderLoginStatus::Cancelled)
        {
            cancelled = Some(settings);
            break;
        }
        tokio::task::yield_now().await;
    }
    let cancelled = cancelled.expect("managed device login cancelled");
    assert_eq!(
        cancelled.provider_connections[0].status,
        ProviderConnectionStatus::AuthenticationRequired
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
            tender_id: first.tender_id,
            body: "Check the insurance exclusions first.".into(),
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
