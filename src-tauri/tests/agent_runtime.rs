use std::{fs, io, path::Path, sync::Arc, time::Duration};

use quantix_lib::{
    ensure_quantix_setup, AgentRunState, ChatGptConnectionState, ChatGptLoginPhase,
    CreateTenderCommand, DiagnosticEvent, ImportTenderPackageCommand, InspectAgentRunCommand,
    InspectTenderAiExecutionCommand, InterruptAgentRunCommand, ParseSourceArtifactCommand,
    ProviderConnectionStatus, ProviderEventKind, ProviderFailureCategory, QuantixHost,
    RunBootstrapAgentCommand, RunTenderRecordExtractionCommand, RuntimeLayout, SetupPlatform,
    SetupState, StartChatGptLoginStatus, StoragePermissions, TenderAiSelectionReadiness,
    TenderErrorCode, TenderEvidenceReference, TenderIntegrityIssue, TenderIntegrityState,
    UpdateTenderAiExecutionSelectionCommand, MINIMUM_SETUP_FREE_SPACE_BYTES,
};
use walkdir::WalkDir;

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
    fixture_executable: std::path::PathBuf,
    host: QuantixHost,
    tender_id: String,
}

impl Harness {
    fn new(agent_scenario: &str) -> Self {
        let root = tempfile::tempdir().expect("temporary Agent Run harness");
        let application_home = root.path().join(".quantix");
        let resources = root.path().join("resources");
        let fixture_executable = install_codex_fixture(&resources, agent_scenario);
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
            fixture_executable,
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

fn diagnostic_facts_for_run(harness: &Harness, run_id: &str) -> Vec<DiagnosticEvent> {
    harness
        .host
        .drain_diagnostics_for_test()
        .expect("flush diagnostics");
    let tender_log_root = harness
        .application_home
        .join("logs")
        .join("tenders")
        .join(&harness.tender_id);
    let mut facts = WalkDir::new(tender_log_root)
        .into_iter()
        .map(|entry| entry.expect("diagnostic log entry"))
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| fs::read_to_string(entry.path()).expect("read diagnostic log"))
        .flat_map(|contents| contents.lines().map(str::to_owned).collect::<Vec<_>>())
        .map(|line| serde_json::from_str::<DiagnosticEvent>(&line).expect("diagnostic fact"))
        .filter(|fact| fact.correlation.run_id.as_deref() == Some(run_id))
        .collect::<Vec<_>>();
    facts.sort_by_key(|fact| fact.session_sequence);
    facts
}

async fn prepare_rate_limited_manager_intake(harness: &Harness) {
    let source = harness._root.path().join("manager-cooldown-resume-source");
    fs::create_dir_all(&source).expect("create Manager cooldown source");
    fs::write(
        source.join("ITT.pdf"),
        b"%PDF-1.7\nTENDER_RECORD_GOLDEN\n%%EOF\n",
    )
    .expect("write Manager cooldown source");
    harness
        .host
        .import_tender_package(ImportTenderPackageCommand {
            tender_id: harness.tender_id.clone(),
            source_path: source.to_string_lossy().into_owned(),
        })
        .expect("register Manager cooldown package");
    harness
        .host
        .run_manager_intake_for_verification(&harness.tender_id)
        .await
        .expect("prepare Manager intake");
    let source_run = harness
        .host
        .run_rate_limited_bootstrap_agent_for_verification(RunBootstrapAgentCommand {
            tender_id: harness.tender_id.clone(),
            retry_of_run_id: None,
        })
        .await
        .expect("create rate-limited provider run");
    harness
        .host
        .persist_manager_rate_limit_for_verification(&harness.tender_id, &source_run.run_id)
        .expect("persist Manager cooldown");
}

fn agent_run_count(application_home: &Path, tender_id: &str) -> u32 {
    rusqlite::Connection::open(
        application_home
            .join("tenders")
            .join(tender_id)
            .join("tender.sqlite"),
    )
    .expect("open Tender Store for Agent Run count")
    .query_row("SELECT COUNT(*) FROM agent_runs", [], |row| row.get(0))
    .expect("count Agent Runs")
}

fn set_manager_cooldown_deadline(harness: &Harness, offset_seconds: i64) {
    let connection = harness.database();
    let trigger_sql: String = connection
        .query_row(
            "SELECT sql FROM sqlite_schema
             WHERE type = 'trigger'
               AND name = 'manager_intake_provider_rate_limit_consumptions_no_update'",
            [],
            |row| row.get(0),
        )
        .expect("load rate-limit consumption immutability trigger");
    connection
        .execute_batch("DROP TRIGGER manager_intake_provider_rate_limit_consumptions_no_update")
        .expect("enable coherent cooldown time fixture");
    connection
        .execute(
            "UPDATE manager_intake_provider_rate_limit_consumptions
             SET retry_not_before_epoch_seconds = unixepoch('now') + ?1
             WHERE source_run_id = (
               SELECT blocking_agent_run_id FROM manager_intake_runs
               ORDER BY intake_run_sequence DESC LIMIT 1
             )",
            [offset_seconds],
        )
        .expect("set consumed cooldown deadline");
    connection
        .execute(
            "UPDATE manager_intake_runs
             SET retry_not_before_epoch_seconds = unixepoch('now') + ?1",
            [offset_seconds],
        )
        .expect("set projected cooldown deadline");
    connection
        .execute_batch(&trigger_sql)
        .expect("restore rate-limit consumption immutability trigger");
}

async fn wait_for_exact_agent_run_count(
    application_home: &Path,
    tender_id: &str,
    expected: u32,
    context: &str,
) {
    for _ in 0..300 {
        let actual = agent_run_count(application_home, tender_id);
        if actual == expected {
            tokio::time::sleep(Duration::from_millis(50)).await;
            assert_eq!(
                agent_run_count(application_home, tender_id),
                expected,
                "{context}: cooldown wake-up must not make a duplicate provider call"
            );
            return;
        }
        assert!(
            actual < expected,
            "{context}: cooldown wake-up made more than one provider call"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let connection = rusqlite::Connection::open(
        application_home
            .join("tenders")
            .join(tender_id)
            .join("tender.sqlite"),
    )
    .expect("open timed-out Tender Store");
    let (stage, deadline): (String, Option<i64>) = connection
        .query_row(
            "SELECT stage, retry_not_before_epoch_seconds
             FROM manager_intake_runs ORDER BY intake_run_sequence DESC LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("inspect timed-out Manager intake");
    panic!(
        "{context}: cooldown wake-up did not make the expected provider call; stage={stage}, deadline={deadline:?}"
    );
}

#[tokio::test]
async fn manager_cooldown_resumes_once_and_survives_restart() {
    let future = Harness::new("rate-limited");
    prepare_rate_limited_manager_intake(&future).await;
    let baseline = agent_run_count(&future.application_home, &future.tender_id);
    set_manager_cooldown_deadline(&future, 3600);

    future
        .host
        .resume_manager_intakes_for_verification()
        .expect("schedule future cooldown");
    future
        .host
        .resume_manager_intakes_for_verification()
        .expect("deduplicate future cooldown schedule");
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(
        agent_run_count(&future.application_home, &future.tender_id),
        baseline,
        "a future cooldown must not call the provider"
    );

    set_manager_cooldown_deadline(&future, 1);
    let Harness {
        _root: root,
        application_home,
        host,
        tender_id,
        ..
    } = future;
    drop(host);
    let reopened = QuantixHost::with_setup_platform_and_runtime(
        &application_home,
        Arc::new(ReadySetupPlatform),
        RuntimeLayout::bundled(root.path().join("resources")),
    );
    reopened.accept_runtime_fixture();
    assert_eq!(ensure_quantix_setup(&reopened).state, SetupState::Ready);
    reopened
        .approve_runtime_fixture_ai_selection()
        .expect("approve reopened fixture AI selection");
    reopened
        .resume_manager_intakes_for_verification()
        .expect("reconstruct future cooldown after restart");
    reopened
        .resume_manager_intakes_for_verification()
        .expect("deduplicate reconstructed cooldown");
    wait_for_exact_agent_run_count(
        &application_home,
        &tender_id,
        baseline + 1,
        "reopened future wait",
    )
    .await;

    let expired = Harness::new("rate-limited");
    prepare_rate_limited_manager_intake(&expired).await;
    let expired_baseline = agent_run_count(&expired.application_home, &expired.tender_id);
    set_manager_cooldown_deadline(&expired, -1);
    let Harness {
        _root: expired_root,
        application_home: expired_application_home,
        host: expired_host,
        tender_id: expired_tender_id,
        ..
    } = expired;
    drop(expired_host);
    let reopened_expired = QuantixHost::with_setup_platform_and_runtime(
        &expired_application_home,
        Arc::new(ReadySetupPlatform),
        RuntimeLayout::bundled(expired_root.path().join("resources")),
    );
    reopened_expired.accept_runtime_fixture();
    assert_eq!(
        ensure_quantix_setup(&reopened_expired).state,
        SetupState::Ready
    );
    reopened_expired
        .approve_runtime_fixture_ai_selection()
        .expect("approve expired fixture AI selection");
    reopened_expired
        .resume_manager_intakes_for_verification()
        .expect("resume expired cooldown");
    reopened_expired
        .resume_manager_intakes_for_verification()
        .expect("deduplicate expired cooldown start");
    wait_for_exact_agent_run_count(
        &expired_application_home,
        &expired_tender_id,
        expired_baseline + 1,
        "expired wait",
    )
    .await;
}

#[tokio::test]
async fn rebind_cannot_bypass_future_cooldown() {
    let harness = Harness::new("rate-limited");
    let source = harness._root.path().join("manager-cooldown-source");
    fs::create_dir_all(&source).expect("create Manager source");
    fs::write(
        source.join("ITT.pdf"),
        b"%PDF-1.7\nTENDER_RECORD_GOLDEN\n%%EOF\n",
    )
    .expect("write Manager source");
    harness
        .host
        .import_tender_package(ImportTenderPackageCommand {
            tender_id: harness.tender_id.clone(),
            source_path: source.to_string_lossy().into_owned(),
        })
        .expect("register Manager package");
    harness
        .host
        .run_manager_intake_for_verification(&harness.tender_id)
        .await
        .expect("persist initial cooldown");
    let connection = harness.database();
    drop(connection);
    let source_run = harness
        .host
        .run_rate_limited_bootstrap_agent_for_verification(RunBootstrapAgentCommand {
            tender_id: harness.tender_id.clone(),
            retry_of_run_id: None,
        })
        .await
        .expect("create rate-limited provider run");
    harness
        .host
        .persist_manager_rate_limit_for_verification(&harness.tender_id, &source_run.run_id)
        .expect("persist initial cooldown");
    let connection = harness.database();
    let initial_runs: u32 = connection
        .query_row("SELECT COUNT(*) FROM agent_runs", [], |row| row.get(0))
        .expect("count initial runs");
    drop(connection);
    set_manager_cooldown_deadline(&harness, 3600);

    harness
        .host
        .rebind_manager_intake_provider_for_verification(&harness.tender_id)
        .await
        .expect("future cooldown is an admitted no-op");
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(
        harness
            .database()
            .query_row("SELECT COUNT(*) FROM agent_runs", [], |row| row
                .get::<_, u32>(0))
            .expect("count runs after blocked rebind"),
        initial_runs
    );

    set_manager_cooldown_deadline(&harness, -1);
    fs::write(
        harness.fixture_executable.with_extension("agent-scenario"),
        "manager-intake",
    )
    .expect("restore available provider fixture");
    harness
        .host
        .approve_runtime_fixture_ai_selection()
        .expect("refresh available provider selection");
    harness
        .host
        .rebind_manager_intake_provider_for_verification(&harness.tender_id)
        .await
        .expect("expired cooldown resumes");
    for _ in 0..100 {
        let count: u32 = harness
            .database()
            .query_row("SELECT COUNT(*) FROM agent_runs", [], |row| row.get(0))
            .expect("count resumed runs");
        if count > initial_runs {
            assert_eq!(count, initial_runs + 1);
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("expired cooldown did not admit exactly one Agent Run");
}

#[tokio::test]
async fn generic_retry_cannot_bypass_manager_owned_recovery_during_cooldown() {
    let harness = Harness::new("rate-limited");
    prepare_rate_limited_manager_intake(&harness).await;
    set_manager_cooldown_deadline(&harness, 3600);
    let manager_run_id: String = harness
        .database()
        .query_row(
            "SELECT runs.run_id
             FROM agent_runs AS runs
             JOIN tender_tasks AS tasks USING (task_id)
             WHERE EXISTS (
               SELECT 1 FROM json_each(tasks.exact_inputs_json)
               WHERE json_extract(value, '$.kind') = 'manager_intake_run'
             )
             ORDER BY runs.run_sequence LIMIT 1",
            [],
            |row| row.get(0),
        )
        .expect("find Manager-owned extraction run");
    let before = agent_run_count(&harness.application_home, &harness.tender_id);

    let error = harness
        .host
        .run_bootstrap_agent(RunBootstrapAgentCommand {
            tender_id: harness.tender_id.clone(),
            retry_of_run_id: Some(manager_run_id.clone()),
        })
        .await
        .expect_err("generic retry must not own Manager extraction recovery");

    assert_eq!(error.code, TenderErrorCode::InvalidCommand);
    assert_eq!(
        agent_run_count(&harness.application_home, &harness.tender_id),
        before
    );
    assert_eq!(
        harness
            .database()
            .query_row(
                "SELECT COUNT(*) FROM agent_runs WHERE retry_of_run_id = ?1",
                [&manager_run_id],
                |row| row.get::<_, u32>(0),
            )
            .expect("count forbidden Manager retry children"),
        0
    );
}

#[tokio::test]
async fn local_tools_preflight_preserves_a_future_manager_cooldown() {
    let harness = Harness::new("rate-limited");
    prepare_rate_limited_manager_intake(&harness).await;
    set_manager_cooldown_deadline(&harness, 3600);
    let expected: (String, Option<String>, Option<i64>, u32) = harness
        .database()
        .query_row(
            "SELECT stage, blocking_agent_run_id,
                    retry_not_before_epoch_seconds, provider_retry_attempt_count
             FROM manager_intake_runs ORDER BY intake_run_sequence DESC LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .expect("read authoritative future cooldown");
    let before = agent_run_count(&harness.application_home, &harness.tender_id);

    harness
        .host
        .set_document_tools_verified_for_verification(false);
    harness
        .host
        .start_manager_intake_background_for_verification(&harness.tender_id)
        .expect("start background intake with unavailable tools");
    tokio::time::sleep(Duration::from_millis(100)).await;
    harness
        .host
        .set_document_tools_verified_for_verification(true);
    harness
        .host
        .run_manager_intake_for_verification(&harness.tender_id)
        .await
        .expect("restored tools before deadline remain blocked");

    let actual: (String, Option<String>, Option<i64>, u32) = harness
        .database()
        .query_row(
            "SELECT stage, blocking_agent_run_id,
                    retry_not_before_epoch_seconds, provider_retry_attempt_count
             FROM manager_intake_runs ORDER BY intake_run_sequence DESC LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .expect("read preserved future cooldown");
    assert_eq!(actual, expected);
    assert_eq!(
        agent_run_count(&harness.application_home, &harness.tender_id),
        before,
        "preflight transitions must not create provider work before the deadline"
    );
}

#[tokio::test]
async fn cold_open_rejects_cooldown_consumption_projection_mismatch() {
    let harness = Harness::new("rate-limited");
    prepare_rate_limited_manager_intake(&harness).await;
    harness
        .database()
        .execute(
            "UPDATE manager_intake_runs SET provider_retry_attempt_count = 2",
            [],
        )
        .expect("inject cooldown consumption projection mismatch");

    let cold =
        QuantixHost::with_setup_platform(&harness.application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&cold).state, SetupState::Ready);
    assert_eq!(
        cold.inspect_tender_integrity(&harness.tender_id)
            .expect("inspect mismatched cooldown projection")
            .state,
        TenderIntegrityState::RecoveryRequired
    );
}

#[tokio::test]
async fn repaired_extraction_records_truthful_boundaries() {
    let harness = Harness::new("manager-intake-repair-invalid-then-valid");
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
    assert_eq!(
        initial_kinds,
        vec![
            ProviderEventKind::RunStarted,
            ProviderEventKind::ThreadEstablished,
            ProviderEventKind::TurnRequested,
            ProviderEventKind::TurnStarted,
            ProviderEventKind::UsageObserved,
            ProviderEventKind::ControlRequestDenied,
            ProviderEventKind::CandidateRejected,
            ProviderEventKind::Terminal,
        ]
    );
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
    assert_eq!(
        repair_kinds,
        vec![
            ProviderEventKind::RunStarted,
            ProviderEventKind::ThreadResumed,
            ProviderEventKind::TurnRequested,
            ProviderEventKind::TurnStarted,
            ProviderEventKind::UsageObserved,
            ProviderEventKind::ControlRequestDenied,
            ProviderEventKind::CandidateValidated,
            ProviderEventKind::ResultCommitted,
            ProviderEventKind::Terminal,
        ]
    );
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
    let initial_facts = diagnostic_facts_for_run(&harness, &initial.run_id);
    let repair_facts = diagnostic_facts_for_run(&harness, &repaired.run.run_id);
    for facts in [&initial_facts, &repair_facts] {
        let transport_completed = facts
            .iter()
            .position(|fact| fact.event_name == "provider_transport_completed")
            .expect("provider transport completed fact");
        assert!(!facts
            .iter()
            .any(|fact| fact.event_name == "provider_turn_completed"));
        assert_eq!(transport_completed, 1);
        assert_eq!(
            facts
                .iter()
                .map(|fact| fact.event_name.as_str())
                .collect::<Vec<_>>(),
            vec!["provider_turn_started", "provider_transport_completed"]
        );
    }
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
async fn chatgpt_login_completes_through_the_app_server_actor() {
    let (_root, host, _application_home) = login_host("managed-login");

    let started = host
        .start_chatgpt_login()
        .await
        .expect("browser login starts");
    assert_eq!(started.status, StartChatGptLoginStatus::AwaitingBrowser);

    let view = wait_for_chatgpt_connection(&host, ChatGptConnectionState::Connected).await;
    assert_eq!(view.chatgpt.login_phase, ChatGptLoginPhase::Completed);
    assert_eq!(
        view.chatgpt.account_id.as_deref(),
        Some("engineer@example.com")
    );
    assert_eq!(view.chatgpt.plan_type.as_deref(), Some("plus"));
    let connection = view
        .provider_connections
        .iter()
        .find(|connection| connection.connection_id == "codex_chatgpt")
        .expect("Codex connection persisted by readiness refresh");
    assert_eq!(connection.status, ProviderConnectionStatus::Ready);
    assert!(connection
        .models
        .iter()
        .any(|model| model.model_id == "gpt-5.6-terra"));
    assert!(view.ai_execution_selection.is_some());
}

#[tokio::test]
async fn chatgpt_device_login_reports_a_one_time_code_and_can_be_cancelled() {
    let (_root, host, _application_home) = login_host("managed-login");

    let started = host
        .start_chatgpt_device_login()
        .await
        .expect("device login starts");
    assert_eq!(started.user_code, "ABCD-EFGH");
    assert!(started.verification_url.contains("device"));

    host.cancel_chatgpt_login().await;

    let view = wait_for_chatgpt_login_phase(&host, ChatGptLoginPhase::Cancelled).await;
    assert_eq!(
        view.chatgpt.state,
        ChatGptConnectionState::Absent,
        "a cancelled login must not persist a connection"
    );
    assert!(view.ai_execution_selection.is_none());
}

#[tokio::test]
async fn chatgpt_disconnect_signs_out_the_app_server_actor_and_clears_the_saved_selection() {
    let (_root, host, _application_home) = login_host("managed-login");

    host.start_chatgpt_login()
        .await
        .expect("browser login starts");
    let view = wait_for_chatgpt_connection(&host, ChatGptConnectionState::Connected).await;
    assert!(view.ai_execution_selection.is_some());

    let disconnected = host.disconnect_chatgpt().await.expect("disconnect");
    assert_eq!(disconnected.chatgpt.state, ChatGptConnectionState::Absent);
    assert!(disconnected.ai_execution_selection.is_none());
    assert!(disconnected.ai_execution_approval.is_none());
    assert_eq!(
        disconnected.provider_connections[0].status,
        ProviderConnectionStatus::AuthenticationRequired
    );

    let restarted = host
        .start_chatgpt_login()
        .await
        .expect("sign-in after disconnect must not short-circuit as connected");
    assert_eq!(restarted.status, StartChatGptLoginStatus::AwaitingBrowser);
    host.cancel_chatgpt_login().await;
}

fn login_host(agent_scenario: &str) -> (tempfile::TempDir, QuantixHost, std::path::PathBuf) {
    let root = tempfile::tempdir().expect("temporary login harness");
    let application_home = root.path().join(".quantix");
    let resources = root.path().join("resources");
    install_codex_fixture(&resources, agent_scenario);
    let host = QuantixHost::with_setup_platform_and_runtime(
        &application_home,
        Arc::new(ReadySetupPlatform),
        RuntimeLayout::bundled(resources),
    );
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    host.accept_runtime_fixture();
    (root, host, application_home)
}

async fn wait_for_chatgpt_connection(
    host: &QuantixHost,
    expected: ChatGptConnectionState,
) -> quantix_lib::ApplicationSettingsView {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    loop {
        let view = host
            .refresh_application_settings()
            .await
            .expect("refresh settings");
        if view.chatgpt.state == expected {
            return view;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "connection never reached {expected:?}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn wait_for_chatgpt_login_phase(
    host: &QuantixHost,
    expected: ChatGptLoginPhase,
) -> quantix_lib::ApplicationSettingsView {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    loop {
        let view = host
            .refresh_application_settings()
            .await
            .expect("refresh settings");
        if view.chatgpt.login_phase == expected {
            return view;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "login phase never reached {expected:?}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[tokio::test]
async fn schema_invalid_provider_output_is_a_completed_transport_and_rejected_candidate() {
    let harness = Harness::new("output-invalid");
    let run = harness
        .host
        .run_bootstrap_agent(RunBootstrapAgentCommand {
            tender_id: harness.tender_id.clone(),
            retry_of_run_id: None,
        })
        .await
        .expect("persist schema rejection");

    assert_eq!(run.state, AgentRunState::Failed, "{run:#?}");
    assert_eq!(run.proposed_result, None);
    assert_eq!(
        run.events
            .iter()
            .map(|event| event.kind)
            .collect::<Vec<_>>(),
        vec![
            ProviderEventKind::RunStarted,
            ProviderEventKind::ThreadEstablished,
            ProviderEventKind::TurnRequested,
            ProviderEventKind::TurnStarted,
            ProviderEventKind::UsageObserved,
            ProviderEventKind::ControlRequestDenied,
            ProviderEventKind::CandidateRejected,
            ProviderEventKind::Terminal,
        ]
    );
    let rejected = run
        .events
        .iter()
        .find(|event| event.kind == ProviderEventKind::CandidateRejected)
        .expect("schema candidate rejection");
    assert_eq!(rejected.summary, "schema_rejection");
    let facts = diagnostic_facts_for_run(&harness, &run.run_id);
    assert!(facts
        .iter()
        .any(|fact| fact.event_name == "provider_transport_completed"));
    assert!(!facts
        .iter()
        .any(|fact| fact.event_name == "provider_turn_failed"));
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
async fn manager_intake_cancellation_discards_a_schema_valid_candidate() {
    let harness = Harness::new("manager-intake-cancellation");
    harness.parsed_pdf_evidence().await;
    let host = harness.host.clone();
    let tender_id = harness.tender_id.clone();
    let intake =
        tokio::spawn(async move { host.run_manager_intake_for_verification(&tender_id).await });

    let run_id = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let connection = harness.database();
            let run_id = connection
                .query_row(
                    "SELECT runs.run_id
                     FROM agent_runs AS runs
                     JOIN agent_profile_versions AS profiles
                       ON profiles.profile_id = runs.profile_id
                      AND profiles.version = runs.profile_version
                     WHERE runs.status = 'running'
                       AND profiles.capabilities_json LIKE '%present_manager_intake_outcome%'
                     ORDER BY runs.run_sequence DESC LIMIT 1",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .ok();
            drop(connection);
            if let Some(run_id) = run_id {
                break run_id;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("Manager Intake outcome is running");

    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if harness
                .fixture_executable
                .with_extension("manager-output-waiting")
                .is_file()
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("schema-valid Manager Intake candidate reaches delayed output boundary");
    let connection = harness.database();
    connection
        .execute(
            "INSERT INTO agent_run_cancellations (run_id, requested_by, requested_at)
             VALUES (?1, 'engineer_user', '2026-08-23T00:00:00Z')",
            [&run_id],
        )
        .expect("commit cancellation before schema-valid candidate completion");
    drop(connection);
    fs::write(
        harness
            .fixture_executable
            .with_extension("manager-output-release"),
        b"release",
    )
    .expect("release schema-valid Manager Intake candidate");
    let _ = intake.await.expect("Manager Intake task joins");

    let run = harness
        .host
        .inspect_agent_run(InspectAgentRunCommand {
            tender_id: harness.tender_id.clone(),
            run_id: run_id.clone(),
        })
        .expect("inspect terminal Manager Intake run");
    assert_eq!(run.state, AgentRunState::Interrupted, "{run:#?}");
    assert!(run.proposed_result.is_none());
    assert!(!run.events.iter().any(|event| {
        matches!(
            event.kind,
            ProviderEventKind::CandidateValidated
                | ProviderEventKind::CandidateRejected
                | ProviderEventKind::ResultCommitted
        )
    }));
    assert_eq!(
        run.events
            .iter()
            .filter(|event| event.kind == ProviderEventKind::Terminal)
            .count(),
        1
    );
    let connection = harness.database();
    let outcome_count: u32 = connection
        .query_row("SELECT COUNT(*) FROM manager_intake_outcomes", [], |row| {
            row.get(0)
        })
        .expect("inspect Manager Intake outcomes");
    assert_eq!(outcome_count, 0);
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

#[tokio::test]
async fn cold_open_rejects_invalid_terminal_event_grammar() {
    for mutation in ["missing", "duplicate", "middle"] {
        let harness = Harness::new("success");
        let run = harness
            .host
            .run_bootstrap_agent(RunBootstrapAgentCommand {
                tender_id: harness.tender_id.clone(),
                retry_of_run_id: None,
            })
            .await
            .expect("persist terminal Agent Run");
        harness
            .host
            .close_tender(&harness.tender_id)
            .expect("close Tender before mutation");
        let connection = harness.database();
        match mutation {
            "missing" => {
                let sql: String = connection
                    .query_row(
                        "SELECT sql FROM sqlite_schema
                         WHERE type = 'trigger' AND name = 'provider_events_no_delete'",
                        [],
                        |row| row.get(0),
                    )
                    .expect("event delete trigger");
                connection
                    .execute_batch("DROP TRIGGER provider_events_no_delete")
                    .expect("enable test mutation");
                connection
                    .execute(
                        "DELETE FROM provider_events WHERE run_id = ?1 AND kind = 'terminal'",
                        [&run.run_id],
                    )
                    .expect("remove terminal");
                connection
                    .execute_batch(&sql)
                    .expect("restore delete trigger");
            }
            "duplicate" => {
                connection
                    .execute(
                        "INSERT INTO provider_events (
                             run_id, sequence, kind, summary, correlation_id,
                             request_fingerprint, denial_reason, opaque_reference, created_at
                         )
                         SELECT run_id, sequence + 1, 'terminal', 'duplicate_terminal', NULL,
                                NULL, NULL, NULL, created_at
                         FROM provider_events
                         WHERE run_id = ?1 AND kind = 'terminal'",
                        [&run.run_id],
                    )
                    .expect("append duplicate terminal");
            }
            "middle" => {
                let sql: String = connection
                    .query_row(
                        "SELECT sql FROM sqlite_schema
                         WHERE type = 'trigger' AND name = 'provider_events_no_update'",
                        [],
                        |row| row.get(0),
                    )
                    .expect("event update trigger");
                connection
                    .execute_batch("DROP TRIGGER provider_events_no_update")
                    .expect("enable test mutation");
                connection
                    .execute(
                        "UPDATE provider_events SET kind = 'terminal'
                         WHERE run_id = ?1 AND kind = 'usage_observed'",
                        [&run.run_id],
                    )
                    .expect("move terminal into transport history");
                connection
                    .execute_batch(&sql)
                    .expect("restore update trigger");
            }
            _ => unreachable!(),
        }
        drop(connection);
        assert_eq!(
            harness
                .host
                .inspect_tender_integrity(&harness.tender_id)
                .expect("inspect terminal grammar")
                .state,
            TenderIntegrityState::RecoveryRequired,
            "{mutation} terminal grammar"
        );
        assert_eq!(
            harness
                .host
                .open_tender(&harness.tender_id)
                .expect_err("cold open rejects malformed terminal grammar")
                .code,
            TenderErrorCode::RecoveryRequired,
            "{mutation} terminal grammar"
        );
    }
}

#[tokio::test]
async fn cold_open_rejects_candidate_and_result_suffix_mutations() {
    for (scenario, mutation) in [
        ("success", "duplicate_candidate_validated"),
        ("success", "middle_result_committed"),
        ("success", "reordered_candidate_result"),
        ("success", "observational_after_candidate"),
        ("output-invalid", "missing_candidate_rejected"),
    ] {
        let harness = Harness::new(scenario);
        let run = harness
            .host
            .run_bootstrap_agent(RunBootstrapAgentCommand {
                tender_id: harness.tender_id.clone(),
                retry_of_run_id: None,
            })
            .await
            .expect("persist terminal Agent Run");
        harness
            .host
            .close_tender(&harness.tender_id)
            .expect("close Tender before mutation");
        let connection = harness.database();
        let trigger_name = if mutation == "missing_candidate_rejected" {
            "provider_events_no_delete"
        } else {
            "provider_events_no_update"
        };
        let trigger_sql: String = connection
            .query_row(
                "SELECT sql FROM sqlite_schema WHERE type = 'trigger' AND name = ?1",
                [trigger_name],
                |row| row.get(0),
            )
            .expect("event immutability trigger");
        connection
            .execute_batch(&format!("DROP TRIGGER {trigger_name}"))
            .expect("enable test mutation");
        match mutation {
            "duplicate_candidate_validated" => {
                connection
                    .execute(
                        "UPDATE provider_events SET kind = 'candidate_validated'
                         WHERE run_id = ?1 AND kind = 'usage_observed'",
                        [&run.run_id],
                    )
                    .expect("duplicate candidate stage");
            }
            "middle_result_committed" => {
                connection
                    .execute(
                        "UPDATE provider_events SET kind = 'result_committed'
                         WHERE run_id = ?1 AND kind = 'usage_observed'",
                        [&run.run_id],
                    )
                    .expect("insert middle result stage");
            }
            "reordered_candidate_result" => {
                connection
                    .execute(
                        "UPDATE provider_events SET kind = 'warning'
                         WHERE run_id = ?1 AND kind = 'candidate_validated'",
                        [&run.run_id],
                    )
                    .expect("temporarily move candidate stage");
                connection
                    .execute(
                        "UPDATE provider_events SET kind = 'candidate_validated'
                         WHERE run_id = ?1 AND kind = 'result_committed'",
                        [&run.run_id],
                    )
                    .expect("move result before candidate");
                connection
                    .execute(
                        "UPDATE provider_events SET kind = 'result_committed'
                         WHERE run_id = ?1 AND kind = 'warning' AND summary = 'candidate_validated'",
                        [&run.run_id],
                    )
                    .expect("finish reordered suffix");
            }
            "observational_after_candidate" => {
                connection
                    .execute(
                        "UPDATE provider_events SET kind = 'usage_observed'
                         WHERE run_id = ?1 AND kind = 'result_committed'",
                        [&run.run_id],
                    )
                    .expect("append observational event after candidate");
            }
            "missing_candidate_rejected" => {
                connection
                    .execute(
                        "DELETE FROM provider_events
                         WHERE run_id = ?1 AND kind = 'candidate_rejected'",
                        [&run.run_id],
                    )
                    .expect("remove candidate rejection");
            }
            _ => unreachable!(),
        }
        connection
            .execute_batch(&trigger_sql)
            .expect("restore event immutability trigger");
        drop(connection);
        assert_eq!(
            harness
                .host
                .inspect_tender_integrity(&harness.tender_id)
                .expect("inspect candidate/result suffix")
                .state,
            TenderIntegrityState::RecoveryRequired,
            "{mutation}"
        );
        assert_eq!(
            harness
                .host
                .open_tender(&harness.tender_id)
                .expect_err("cold open rejects malformed candidate/result suffix")
                .code,
            TenderErrorCode::RecoveryRequired,
            "{mutation}"
        );
    }
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
