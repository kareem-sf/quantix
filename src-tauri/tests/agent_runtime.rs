use std::{fs, io, path::Path, sync::Arc, time::Duration};

use quantix_lib::{
    ensure_quantix_setup, AgentRunState, CreateTenderCommand, ImportTenderPackageCommand,
    InspectAgentRunCommand, InspectTenderAiExecutionCommand, InterruptAgentRunCommand,
    ParseSourceArtifactCommand, ProviderEventKind, ProviderFailureCategory, QuantixHost,
    RunBootstrapAgentCommand, RunTenderRecordExtractionCommand, RuntimeLayout, SetupPlatform,
    SetupState, StoragePermissions, TenderAiSelectionReadiness, TenderErrorCode,
    TenderEvidenceReference, TenderIntegrityIssue, TenderIntegrityState,
    UpdateTenderAiExecutionSelectionCommand, MINIMUM_SETUP_FREE_SPACE_BYTES,
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
}

struct Harness {
    _root: tempfile::TempDir,
    application_home: std::path::PathBuf,
    host: QuantixHost,
    tender_id: String,
}

impl Harness {
    fn new(agent_scenario: &str) -> Self {
        let root = tempfile::tempdir().expect("temporary Agent Run harness");
        let application_home = root.path().join(".quantix");
        let resources = root.path().join("resources");
        install_codex_fixture(&resources, agent_scenario);
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
                name: "Cairo Metro Systems Tender".into(),
            })
            .expect("create Tender");
        let harness = Self {
            _root: root,
            application_home,
            host,
            tender_id: tender.tender_id,
        };
        harness.bind_current_ai_selection();
        harness
    }

    fn bind_current_ai_selection(&self) {
        let selection = self
            .host
            .inspect_application_settings()
            .expect("inspect fixture AI settings")
            .ai_execution_selection;
        let binding = self
            .host
            .inspect_tender_ai_execution(InspectTenderAiExecutionCommand {
                tender_id: self.tender_id.clone(),
            })
            .expect("inspect Tender AI binding");
        self.host
            .update_tender_ai_execution(UpdateTenderAiExecutionSelectionCommand {
                tender_id: self.tender_id.clone(),
                expected_revision: binding.revision,
                selection,
            })
            .expect("bind fixture AI selection to Tender");
    }

    fn database(&self) -> rusqlite::Connection {
        rusqlite::Connection::open(
            self.application_home
                .join("tenders")
                .join(&self.tender_id)
                .join("tender.sqlite"),
        )
        .expect("open Tender Store database")
    }

    async fn parsed_pdf_evidence(&self) -> Vec<TenderEvidenceReference> {
        let source = self._root.path().join("records-source");
        fs::create_dir(&source).expect("source directory");
        fs::write(
            source.join("records.pdf"),
            b"%PDF-1.7\nTENDER_RECORD_GOLDEN\n%%EOF\n",
        )
        .expect("PDF fixture");
        let imported = self
            .host
            .import_tender_package(ImportTenderPackageCommand {
                tender_id: self.tender_id.clone(),
                source_path: source.to_string_lossy().into_owned(),
            })
            .expect("import evidence");
        let document = imported.documents.first().expect("registered PDF");
        self.host
            .parse_source_artifact(ParseSourceArtifactCommand {
                tender_id: self.tender_id.clone(),
                artifact_id: document.artifact_id.clone(),
                version: document.version,
            })
            .await
            .expect("parse evidence");
        self.host
            .inspect_evidence(ParseSourceArtifactCommand {
                tender_id: self.tender_id.clone(),
                artifact_id: document.artifact_id.clone(),
                version: document.version,
            })
            .expect("inspect evidence")
            .locations
            .into_iter()
            .map(|location| TenderEvidenceReference {
                artifact_id: document.artifact_id.clone(),
                version: document.version,
                ordinal: location.ordinal,
            })
            .collect()
    }
}

#[tokio::test]
async fn repaired_extraction_records_truthful_boundaries() {
    let harness = Harness::new("record-extraction-invalid-then-valid");
    let evidence = harness.parsed_pdf_evidence().await;
    let repaired = harness
        .host
        .run_tender_record_extraction(RunTenderRecordExtractionCommand {
            tender_id: harness.tender_id.clone(),
            evidence,
            authorities: Vec::new(),
        })
        .await
        .expect("repair invalid extraction once");
    assert_eq!(
        repaired.run.state,
        AgentRunState::Completed,
        "{:#?}",
        repaired.run
    );
    let runs = harness
        .host
        .inspect_agent_runs(&harness.tender_id)
        .expect("inspect runs");
    let initial_id = repaired
        .run
        .retry_of_run_id
        .as_deref()
        .expect("repair lineage");
    let initial = runs
        .iter()
        .find(|run| run.run_id == initial_id)
        .expect("initial extraction");
    assert_eq!(initial.state, AgentRunState::Failed);
    for run in [initial, &repaired.run] {
        let kinds = run
            .events
            .iter()
            .map(|event| event.kind)
            .collect::<Vec<_>>();
        assert_eq!(kinds.first(), Some(&ProviderEventKind::RunStarted));
        assert_eq!(kinds.last(), Some(&ProviderEventKind::Terminal));
        assert_eq!(
            kinds
                .iter()
                .filter(|kind| **kind == ProviderEventKind::Terminal)
                .count(),
            1
        );
    }
    let initial_kinds = initial
        .events
        .iter()
        .map(|event| event.kind)
        .collect::<Vec<_>>();
    assert!(
        initial_kinds
            .iter()
            .position(|kind| *kind == ProviderEventKind::CandidateRejected)
            .expect("candidate rejected")
            < initial_kinds
                .iter()
                .position(|kind| *kind == ProviderEventKind::Terminal)
                .expect("terminal")
    );
    let repair_kinds = repaired
        .run
        .events
        .iter()
        .map(|event| event.kind)
        .collect::<Vec<_>>();
    assert!(
        repair_kinds
            .iter()
            .position(|kind| *kind == ProviderEventKind::CandidateValidated)
            .expect("candidate validated")
            < repair_kinds
                .iter()
                .position(|kind| *kind == ProviderEventKind::ResultCommitted)
                .expect("result committed")
    );
    assert!(
        repair_kinds
            .iter()
            .position(|kind| *kind == ProviderEventKind::ResultCommitted)
            .expect("result committed")
            < repair_kinds
                .iter()
                .position(|kind| *kind == ProviderEventKind::Terminal)
                .expect("terminal")
    );
    assert!(!std::fs::read_to_string(
        harness
            .application_home
            .join("logs")
            .join("tenders")
            .join(&harness.tender_id)
            .join("diagnostics.ndjson")
    )
    .unwrap_or_default()
    .contains("provider_turn_completed"));
}

#[tokio::test]
async fn missing_chatgpt_connection_fails_before_a_backend_turn_is_established() {
    let harness = Harness::new("signed-out");

    let run = harness
        .host
        .run_bootstrap_agent(RunBootstrapAgentCommand {
            tender_id: harness.tender_id.clone(),
            retry_of_run_id: None,
        })
        .await
        .expect("persist visible authentication failure");

    assert_eq!(run.state, AgentRunState::Failed, "{run:#?}");
    assert!(run.provider_thread_ref.is_none());
    assert!(run.provider_turn_ref.is_none());
    assert!(run.proposed_result.is_none());
    let failure = run.failure.as_ref().expect("Provider Failure");
    assert_eq!(
        failure.category,
        ProviderFailureCategory::AuthenticationRequired
    );
    assert!(failure.retry_safe);
    assert_eq!(
        failure.required_user_action,
        "Connect your ChatGPT subscription in Settings before retrying."
    );
    assert_eq!(
        run.events
            .iter()
            .map(|event| event.kind)
            .collect::<Vec<_>>(),
        vec![ProviderEventKind::RunStarted, ProviderEventKind::Terminal]
    );

    let persisted = harness
        .host
        .inspect_agent_run(InspectAgentRunCommand {
            tender_id: harness.tender_id.clone(),
            run_id: run.run_id.clone(),
        })
        .expect("inspect persisted Agent Run");
    assert_eq!(persisted.state, AgentRunState::Failed);
    assert_eq!(persisted.failure, run.failure);
}

#[tokio::test]
async fn agent_run_preserves_its_tender_scoped_provider_selection_after_unbinding() {
    let harness = Harness::new("success");
    let expected_selection = harness
        .host
        .inspect_tender_ai_execution(InspectTenderAiExecutionCommand {
            tender_id: harness.tender_id.clone(),
        })
        .expect("inspect bound Tender selection")
        .selection
        .expect("fixture selection");

    let run = harness
        .host
        .run_bootstrap_agent(RunBootstrapAgentCommand {
            tender_id: harness.tender_id.clone(),
            retry_of_run_id: None,
        })
        .await
        .expect("persist Agent Run");
    assert_eq!(run.provider_selection, expected_selection);

    let binding = harness
        .host
        .inspect_tender_ai_execution(InspectTenderAiExecutionCommand {
            tender_id: harness.tender_id.clone(),
        })
        .expect("inspect current binding");
    let unbound = harness
        .host
        .update_tender_ai_execution(UpdateTenderAiExecutionSelectionCommand {
            tender_id: harness.tender_id.clone(),
            expected_revision: binding.revision,
            selection: None,
        })
        .expect("remove future-run provider selection");
    assert_eq!(unbound.readiness, TenderAiSelectionReadiness::LocalOnly);

    let historical = harness
        .host
        .inspect_agent_run(InspectAgentRunCommand {
            tender_id: harness.tender_id.clone(),
            run_id: run.run_id,
        })
        .expect("inspect historical Agent Run");
    assert_eq!(historical.provider_selection, expected_selection);
}

#[tokio::test]
async fn engineer_interruption_persists_a_terminal_interrupted_run() {
    let harness = Harness::new("hang-before-thread");
    let host = harness.host.clone();
    let tender_id = harness.tender_id.clone();
    let running = tokio::spawn(async move {
        host.run_bootstrap_agent(RunBootstrapAgentCommand {
            tender_id,
            retry_of_run_id: None,
        })
        .await
    });

    let run_id = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(run) = harness
                .host
                .inspect_agent_runs(&harness.tender_id)
                .expect("inspect active Agent Runs")
                .into_iter()
                .find(|run| run.state == AgentRunState::Running)
            {
                break run.run_id;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("Agent Run becomes inspectably active");

    assert!(harness
        .host
        .interrupt_agent_run(InterruptAgentRunCommand {
            tender_id: harness.tender_id.clone(),
            run_id: run_id.clone(),
        })
        .expect("interrupt active Agent Run"));
    let cancellation_facts: u32 = harness
        .database()
        .query_row(
            "SELECT COUNT(*) FROM agent_run_cancellations WHERE run_id = ?1",
            [&run_id],
            |row| row.get(0),
        )
        .expect("inspect persisted cancellation fact");
    assert_eq!(cancellation_facts, 1);

    let run = running
        .await
        .expect("Agent Run task joins")
        .expect("Agent Run reaches terminal state");
    assert_eq!(run.run_id, run_id);
    assert_eq!(run.state, AgentRunState::Interrupted, "{run:#?}");
    assert_eq!(
        run.failure.as_ref().map(|failure| failure.category),
        Some(ProviderFailureCategory::Interrupted)
    );
    assert!(run.proposed_result.is_none());
}

#[tokio::test]
async fn semantically_invalid_agent_manifest_requires_tender_recovery() {
    let harness = Harness::new("success");
    let run = harness
        .host
        .run_bootstrap_agent(RunBootstrapAgentCommand {
            tender_id: harness.tender_id.clone(),
            retry_of_run_id: None,
        })
        .await
        .expect("persist Agent Run");
    harness
        .host
        .close_tender(&harness.tender_id)
        .expect("close Tender");

    let connection = harness.database();
    let trigger_sql: String = connection
        .query_row(
            "SELECT sql FROM sqlite_schema
             WHERE type = 'trigger' AND name = 'agent_runs_terminal_facts_no_rewrite'",
            [],
            |row| row.get(0),
        )
        .expect("Agent Run immutability trigger");
    connection
        .execute_batch("DROP TRIGGER agent_runs_terminal_facts_no_rewrite")
        .expect("inject manifest mutation capability");
    connection
        .execute(
            "UPDATE agent_runs SET permission_grant_json = '{}' WHERE run_id = ?1",
            [&run.run_id],
        )
        .expect("replace permission manifest with semantically invalid JSON");
    connection
        .execute_batch(&trigger_sql)
        .expect("restore exact Agent Run trigger");
    drop(connection);

    let integrity = harness
        .host
        .inspect_tender_integrity(&harness.tender_id)
        .expect("inspect semantic manifests");
    assert_eq!(integrity.state, TenderIntegrityState::RecoveryRequired);
    assert_eq!(
        integrity.issues,
        vec![TenderIntegrityIssue::ManifestInvalid]
    );
    assert_eq!(
        harness
            .host
            .open_tender(&harness.tender_id)
            .expect_err("invalid canonical manifest cannot reopen")
            .code,
        TenderErrorCode::RecoveryRequired
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
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    }
}
