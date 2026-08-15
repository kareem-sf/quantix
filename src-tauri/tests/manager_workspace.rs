use std::{fs, io, path::Path, sync::Arc};

use quantix_lib::{
    ensure_quantix_setup, AiProviderKind, CodexReadiness, CreateTenderCommand, DeviceProtection,
    ImportTenderPackageCommand, InspectManagerWorkspaceCommand, ManagerIntakeStage,
    ManagerIntakeStatusKind, ProviderReasoningSelection, QuantixHost,
    RecordEngineerWorkspaceMessageCommand, RuntimeLayout, SelectManagerWorkspaceTenderCommand,
    SetupPlatform, SetupState, StoragePermissions, TenderOfficeMessageAuthor,
    TenderOfficeMessageKind, UpdateAiExecutionSelectionCommand, WorkspaceActionKind,
    WorkspaceMessageReferenceKind, MINIMUM_SETUP_FREE_SPACE_BYTES,
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
    assert_eq!(intake.stage, ManagerIntakeStage::PackageRegistered);
    assert_eq!(intake.status, ManagerIntakeStatusKind::Working);
    assert_eq!(
        projection.current_action.kind,
        WorkspaceActionKind::ObserveIntake
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
async fn public_host_runs_real_manager_intake_while_engineer_switches_tenders() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let resources = user_home.path().join("resources");
    let codex = install_codex_fixture(&resources, "manager-intake");
    install_docling_fixture(&application_home);
    let host = QuantixHost::with_setup_platform_and_runtime(
        &application_home,
        Arc::new(ReadySetupPlatform),
        RuntimeLayout::bundled(resources),
    );
    host.accept_runtime_fixture();
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
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
        b"%PDF-1.7\nSubmission deadline 15 May 2026 at 14:00 Cairo time.\nSubmission deadline 16 May 2026 at 14:00 Cairo time.\n%%EOF\n",
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
        if waiting.is_file() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    assert!(waiting.is_file(), "Manager reached its real Provider turn");
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
    install_docling_fixture(&application_home);
    let host = QuantixHost::with_setup_platform_and_runtime(
        &application_home,
        Arc::new(ReadySetupPlatform),
        RuntimeLayout::bundled(resources),
    );
    host.accept_runtime_fixture();
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
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

fn install_docling_fixture(application_home: &Path) {
    let executable = application_home
        .join("runtimes")
        .join("docling")
        .join(if cfg!(windows) { "Scripts" } else { "bin" })
        .join(executable_name("python"));
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
