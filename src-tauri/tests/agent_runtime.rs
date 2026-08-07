use std::{fs, io, path::Path, sync::Arc, time::Duration};

use quantix_lib::{
    ensure_quantix_setup, AgentRunState, CreateTenderCommand, DeviceProtection,
    InterruptAgentRunCommand, ProviderFailureCategory, QuantixHost, RunBootstrapAgentCommand,
    RuntimeLayout, SetupPlatform, SetupState, StoragePermissions, TenderErrorCode,
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

struct Harness {
    _root: tempfile::TempDir,
    application_home: std::path::PathBuf,
    codex: std::path::PathBuf,
    host: QuantixHost,
    tender_id: String,
}

impl Harness {
    fn new(scenario: &str) -> Self {
        let root = tempfile::tempdir().expect("temporary agent runtime harness");
        let application_home = root.path().join(".quantix");
        let resources = root.path().join("resources");
        let codex = install_codex_fixture(&resources, scenario);
        let host = QuantixHost::with_setup_platform_and_runtime(
            &application_home,
            Arc::new(ReadySetupPlatform),
            RuntimeLayout::bundled(resources),
        );
        host.accept_runtime_fixture();
        assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
        let tender = host
            .create_tender(CreateTenderCommand {
                name: "Cairo Metro Systems Tender".into(),
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

    fn set_scenario(&self, scenario: &str) {
        fs::write(self.codex.with_extension("agent-scenario"), scenario)
            .expect("write fake app-server scenario");
    }
}

#[tokio::test]
async fn one_bootstrap_agent_turn_registers_only_a_validated_proposed_result() {
    let harness = Harness::new("success");

    let run = harness
        .host
        .run_bootstrap_agent(RunBootstrapAgentCommand {
            tender_id: harness.tender_id.clone(),
            retry_of_run_id: None,
        })
        .await
        .expect("run one Bootstrap Agent Profile turn");

    assert_eq!(run.state, AgentRunState::Completed, "{run:#?}");
    assert_eq!(run.profile.version, 1);
    assert_eq!(run.profile.identity, "Bootstrap Tender Analyst");
    assert_eq!(run.profile.profession, "Tender Engineer");
    assert_eq!(
        run.profile.capabilities,
        vec!["analyze_tender_intake_readiness"]
    );
    assert_eq!(run.task.profile_id, run.profile.profile_id);
    assert_eq!(run.task.profile_version, 1);
    assert_eq!(run.task.exact_inputs.len(), 1);
    assert_eq!(run.task.exact_inputs[0].kind, "tender_revision");
    assert_eq!(run.task.exact_inputs[0].version, 1);
    assert!(!run.task.permissions.network_allowed);
    assert!(run.task.permissions.allowed_tools.is_empty());
    assert!(!run.task.permissions.workspace_write_allowed);
    assert_eq!(run.task.resource_budget.provider_turns, 1);
    assert_eq!(run.provider_thread_ref.as_deref(), Some("thr_fixture_1"));
    assert_eq!(run.provider_turn_ref.as_deref(), Some("turn_fixture_1"));
    assert_eq!(
        run.events
            .iter()
            .map(|event| event.sequence)
            .collect::<Vec<_>>(),
        (1..=run.events.len() as u32).collect::<Vec<_>>()
    );
    assert!(run
        .events
        .iter()
        .any(|event| event.kind.as_str() == "control_request_denied"));
    assert_eq!(run.usage.input_tokens, Some(120));
    assert_eq!(run.usage.output_tokens, Some(35));
    assert_eq!(run.usage.context_window, Some(200_000));
    let result = run.proposed_result.expect("validated Proposed result");
    assert_eq!(result.verification_status, VerificationStatus::Proposed);
    assert_eq!(
        result.payload_json,
        r#"{"recommended_next_action":"Verify the imported package before detailed analysis.","summary":"The Tender is ready for controlled intake analysis."}"#
    );
    assert!(run.failure.is_none());
    let provider_environment =
        fs::read_to_string(harness.codex.with_extension("agent-environment"))
            .expect("read restricted provider environment");
    assert!(provider_environment
        .lines()
        .any(|name| name == "CODEX_HOME"));
    assert!(!provider_environment.lines().any(|name| matches!(
        name,
        "PATH" | "GH_TOKEN" | "GITHUB_TOKEN" | "OPENAI_API_KEY" | "AWS_SECRET_ACCESS_KEY"
    )));

    let inspected = harness
        .host
        .inspect_agent_runs(&harness.tender_id)
        .expect("inspect Agent Runs");
    assert_eq!(inspected.len(), 1);
    assert_eq!(inspected[0].run_id, run.run_id);
    let canonical_view = serde_json::to_string(&inspected).expect("serialize inspection");
    assert!(!canonical_view.contains("streamed-delta-must-not-be-canonical"));
    assert!(!canonical_view.contains("hidden-reasoning-must-not-be-canonical"));
    assert!(!canonical_view.contains("raw-protocol"));
}

#[tokio::test]
async fn retry_creates_a_linked_run_and_a_distinct_provider_turn() {
    let harness = Harness::new("success");
    let first = harness
        .host
        .run_bootstrap_agent(RunBootstrapAgentCommand {
            tender_id: harness.tender_id.clone(),
            retry_of_run_id: None,
        })
        .await
        .expect("run initial Agent Turn");
    harness.set_scenario("success-retry");

    let retry = harness
        .host
        .run_bootstrap_agent(RunBootstrapAgentCommand {
            tender_id: harness.tender_id.clone(),
            retry_of_run_id: Some(first.run_id.clone()),
        })
        .await
        .expect("run linked retry");

    assert_eq!(retry.state, AgentRunState::Completed, "{retry:#?}");
    assert_eq!(
        retry.retry_of_run_id.as_deref(),
        Some(first.run_id.as_str())
    );
    assert_ne!(retry.run_id, first.run_id);
    assert_eq!(retry.task.task_id, first.task.task_id);
    assert_eq!(retry.profile, first.profile);
    assert_eq!(retry.provider_thread_ref, first.provider_thread_ref);
    assert_ne!(retry.provider_turn_ref, first.provider_turn_ref);
    assert!(retry
        .events
        .iter()
        .any(|event| event.kind.as_str() == "thread_resumed"));
    assert_eq!(
        fs::read_to_string(harness.codex.with_extension("agent-start-count"))
            .expect("read app-server start count"),
        "1",
        "linked runs must share one app-scoped app-server process"
    );
}

#[tokio::test]
async fn app_scoped_provider_resets_its_per_run_deadline_after_idle_time() {
    let harness = Harness::new("success");
    let first = harness
        .host
        .run_bootstrap_agent(RunBootstrapAgentCommand {
            tender_id: harness.tender_id.clone(),
            retry_of_run_id: None,
        })
        .await
        .expect("run initial Agent Turn");
    tokio::time::sleep(Duration::from_millis(2_100)).await;
    harness.set_scenario("success-retry");

    let retry = harness
        .host
        .run_bootstrap_agent(RunBootstrapAgentCommand {
            tender_id: harness.tender_id.clone(),
            retry_of_run_id: Some(first.run_id),
        })
        .await
        .expect("reuse app-server after its prior operation deadline");

    assert_eq!(retry.state, AgentRunState::Completed, "{retry:#?}");
    assert_eq!(
        fs::read_to_string(harness.codex.with_extension("agent-start-count"))
            .expect("read app-server start count"),
        "1"
    );
}

#[tokio::test]
async fn completed_agent_message_without_a_phase_is_a_valid_candidate() {
    let harness = Harness::new("phase-null-final");

    let run = harness
        .host
        .run_bootstrap_agent(RunBootstrapAgentCommand {
            tender_id: harness.tender_id.clone(),
            retry_of_run_id: None,
        })
        .await
        .expect("run phase-null Provider Turn");

    assert_eq!(run.state, AgentRunState::Completed, "{run:#?}");
    assert!(run.proposed_result.is_some());
}

#[tokio::test]
async fn recoverable_provider_error_can_retry_and_complete_the_same_turn() {
    let harness = Harness::new("retry-then-success");

    let run = harness
        .host
        .run_bootstrap_agent(RunBootstrapAgentCommand {
            tender_id: harness.tender_id.clone(),
            retry_of_run_id: None,
        })
        .await
        .expect("run internally retried Provider Turn");

    assert_eq!(run.state, AgentRunState::Completed, "{run:#?}");
    assert!(run.failure.is_none());
    assert!(run.proposed_result.is_some());
    assert!(run.events.iter().any(|event| {
        event.kind.as_str() == "warning"
            && event.summary == "Provider reported a recoverable error and will retry"
    }));
}

#[tokio::test]
async fn output_that_breaks_the_task_contract_fails_without_a_candidate() {
    let harness = Harness::new("output-invalid");

    let run = harness
        .host
        .run_bootstrap_agent(RunBootstrapAgentCommand {
            tender_id: harness.tender_id.clone(),
            retry_of_run_id: None,
        })
        .await
        .expect("record output-contract failure");

    assert_eq!(run.state, AgentRunState::Failed, "{run:#?}");
    assert_eq!(
        run.failure.as_ref().map(|failure| failure.category),
        Some(ProviderFailureCategory::OutputInvalid)
    );
    assert!(run
        .failure
        .as_ref()
        .is_some_and(|failure| failure.retry_safe));
    assert!(run.provider_thread_ref.is_some());
    assert!(run.provider_turn_ref.is_some());
    assert!(run.proposed_result.is_none());
    assert!(run.events.iter().any(|event| {
        event.kind.as_str() == "terminal" && event.summary == "Provider Turn completed"
    }));
}

#[tokio::test]
async fn malformed_protocol_before_turn_acceptance_is_a_retryable_failure() {
    let harness = Harness::new("malformed-before-turn");

    let run = harness
        .host
        .run_bootstrap_agent(RunBootstrapAgentCommand {
            tender_id: harness.tender_id.clone(),
            retry_of_run_id: None,
        })
        .await
        .expect("record malformed pre-turn protocol");

    assert_eq!(run.state, AgentRunState::Failed, "{run:#?}");
    assert_eq!(
        run.failure.as_ref().map(|failure| failure.category),
        Some(ProviderFailureCategory::ProtocolInvalid)
    );
    assert!(run
        .failure
        .as_ref()
        .is_some_and(|failure| failure.retry_safe));
    assert!(run.provider_thread_ref.is_none());
    assert!(run.provider_turn_ref.is_none());
    assert!(run.proposed_result.is_none());
}

#[tokio::test]
async fn malformed_protocol_after_turn_acceptance_is_indeterminate_and_quarantined() {
    let harness = Harness::new("malformed-after-turn");

    let run = harness
        .host
        .run_bootstrap_agent(RunBootstrapAgentCommand {
            tender_id: harness.tender_id.clone(),
            retry_of_run_id: None,
        })
        .await
        .expect("record malformed accepted Turn");

    assert_eq!(run.state, AgentRunState::Indeterminate, "{run:#?}");
    let failure = run.failure.as_ref().expect("indeterminate failure");
    assert_eq!(failure.category, ProviderFailureCategory::OutcomeUnknown);
    assert!(!failure.retry_safe);
    assert_eq!(
        failure.redacted_detail.as_deref(),
        Some("Codex returned an incompatible or malformed protocol message.")
    );
    assert!(run.provider_thread_ref.is_some());
    assert!(run.provider_turn_ref.is_some());
    assert!(run.proposed_result.is_none());
    assert!(harness
        .application_home
        .join("tenders")
        .join(&harness.tender_id)
        .join("staging")
        .join(format!("quarantine-agent-{}", run.run_id))
        .is_dir());
}

#[tokio::test]
async fn unresolved_indeterminate_run_blocks_new_and_linked_execution() {
    let harness = Harness::new("malformed-after-turn");
    let indeterminate = harness
        .host
        .run_bootstrap_agent(RunBootstrapAgentCommand {
            tender_id: harness.tender_id.clone(),
            retry_of_run_id: None,
        })
        .await
        .expect("record indeterminate Agent Run");
    assert_eq!(indeterminate.state, AgentRunState::Indeterminate);
    harness.set_scenario("success");

    for retry_of_run_id in [None, Some(indeterminate.run_id.clone())] {
        let error = harness
            .host
            .run_bootstrap_agent(RunBootstrapAgentCommand {
                tender_id: harness.tender_id.clone(),
                retry_of_run_id,
            })
            .await
            .expect_err("unresolved indeterminate outcome must block execution");
        assert_eq!(error.code, TenderErrorCode::InvalidCommand);
    }
    assert_eq!(
        fs::read_to_string(harness.codex.with_extension("agent-start-count"))
            .expect("read app-server start count"),
        "1"
    );
}

#[tokio::test]
async fn provider_usage_limit_is_normalized_as_a_retryable_failed_run() {
    let harness = Harness::new("rate-limited");

    let run = harness
        .host
        .run_bootstrap_agent(RunBootstrapAgentCommand {
            tender_id: harness.tender_id.clone(),
            retry_of_run_id: None,
        })
        .await
        .expect("record provider usage limit");

    assert_eq!(run.state, AgentRunState::Failed, "{run:#?}");
    let failure = run.failure.as_ref().expect("normalized rate-limit failure");
    assert_eq!(failure.category, ProviderFailureCategory::RateLimited);
    assert!(failure.retry_safe);
    assert_eq!(
        run.usage.rate_limit_reached.as_deref(),
        Some("usage_limit_exceeded")
    );
    assert!(run.proposed_result.is_none());
}

#[tokio::test]
async fn provider_process_failure_before_turn_acceptance_is_retryable() {
    let harness = Harness::new("process-failure-before-turn");

    let run = harness
        .host
        .run_bootstrap_agent(RunBootstrapAgentCommand {
            tender_id: harness.tender_id.clone(),
            retry_of_run_id: None,
        })
        .await
        .expect("record provider process failure");

    assert_eq!(run.state, AgentRunState::Failed, "{run:#?}");
    let failure = run.failure.as_ref().expect("process failure");
    assert_eq!(failure.category, ProviderFailureCategory::ProcessFailed);
    assert!(failure.retry_safe);
    assert!(run.provider_thread_ref.is_none());
    assert!(run.provider_turn_ref.is_none());
    assert!(run.proposed_result.is_none());
}

#[tokio::test]
async fn engineer_interruption_reaches_a_terminal_interrupted_run() {
    let harness = Harness::new("interrupt");
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
                .expect("inspect running Agent Run")
                .into_iter()
                .find(|run| run.state == AgentRunState::Running)
            {
                break run.run_id;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("Agent Run becomes inspectably running");

    assert!(harness
        .host
        .interrupt_agent_run(InterruptAgentRunCommand {
            tender_id: harness.tender_id.clone(),
            run_id: run_id.clone(),
        })
        .expect("interrupt running Agent Run"));
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
async fn host_restart_marks_an_unfinished_run_indeterminate_and_quarantines_it() {
    let Harness {
        _root,
        application_home,
        codex,
        host,
        tender_id,
    } = Harness::new("hang");
    let running_host = host.clone();
    let running_tender_id = tender_id.clone();
    let running = tokio::spawn(async move {
        running_host
            .run_bootstrap_agent(RunBootstrapAgentCommand {
                tender_id: running_tender_id,
                retry_of_run_id: None,
            })
            .await
    });

    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let accepted = codex.with_extension("turn-waiting").is_file()
                && host
                    .inspect_agent_runs(&tender_id)
                    .expect("inspect acceptance checkpoint")
                    .into_iter()
                    .any(|run| {
                        run.state == AgentRunState::Running && run.provider_turn_ref.is_some()
                    });
            if accepted {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("accepted fake Provider Turn remains active");
    let running_run = host
        .inspect_agent_runs(&tender_id)
        .expect("inspect unfinished Agent Run")
        .into_iter()
        .find(|run| run.state == AgentRunState::Running)
        .expect("running Agent Run");
    let run_id = running_run.run_id;
    assert_eq!(
        running_run.provider_thread_ref.as_deref(),
        Some("thr_fixture_1")
    );
    assert_eq!(
        running_run.provider_turn_ref.as_deref(),
        Some("turn_fixture_1")
    );

    running.abort();
    assert!(running
        .await
        .expect_err("simulate abrupt Host stop")
        .is_cancelled());
    drop(host);

    let resources = codex
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .expect("fixture resources")
        .to_path_buf();
    let restarted = QuantixHost::with_setup_platform_and_runtime(
        &application_home,
        Arc::new(ReadySetupPlatform),
        RuntimeLayout::bundled(resources),
    );
    restarted.accept_runtime_fixture();
    let runs = restarted
        .inspect_agent_runs(&tender_id)
        .expect("reconcile interrupted run on Tender open");
    let run = runs
        .into_iter()
        .find(|run| run.run_id == run_id)
        .expect("reconciled Agent Run");

    assert_eq!(run.state, AgentRunState::Indeterminate, "{run:#?}");
    assert_eq!(
        run.failure.as_ref().map(|failure| failure.category),
        Some(ProviderFailureCategory::OutcomeUnknown)
    );
    assert!(run.proposed_result.is_none());
    assert!(application_home
        .join("tenders")
        .join(&tender_id)
        .join("staging")
        .join(format!("quarantine-agent-{run_id}"))
        .is_dir());
}

#[tokio::test]
async fn host_restart_before_turn_acceptance_is_a_retryable_failure() {
    let Harness {
        _root,
        application_home,
        codex,
        host,
        tender_id,
    } = Harness::new("hang-before-thread");
    let running_host = host.clone();
    let running_tender_id = tender_id.clone();
    let running = tokio::spawn(async move {
        running_host
            .run_bootstrap_agent(RunBootstrapAgentCommand {
                tender_id: running_tender_id,
                retry_of_run_id: None,
            })
            .await
    });

    tokio::time::timeout(Duration::from_secs(5), async {
        while !codex.with_extension("thread-waiting").is_file() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("fake thread request remains unaccepted");
    let run_id = host
        .inspect_agent_runs(&tender_id)
        .expect("inspect pre-accept Agent Run")
        .into_iter()
        .find(|run| run.state == AgentRunState::Running)
        .expect("running Agent Run")
        .run_id;

    running.abort();
    assert!(running
        .await
        .expect_err("simulate abrupt Host stop")
        .is_cancelled());
    drop(host);

    let resources = codex
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .expect("fixture resources")
        .to_path_buf();
    let restarted = QuantixHost::with_setup_platform_and_runtime(
        &application_home,
        Arc::new(ReadySetupPlatform),
        RuntimeLayout::bundled(resources),
    );
    restarted.accept_runtime_fixture();
    let run = restarted
        .inspect_agent_runs(&tender_id)
        .expect("reconcile unaccepted operation")
        .into_iter()
        .find(|run| run.run_id == run_id)
        .expect("reconciled Agent Run");

    assert_eq!(run.state, AgentRunState::Failed, "{run:#?}");
    let failure = run.failure.as_ref().expect("restart failure");
    assert_eq!(failure.category, ProviderFailureCategory::ProcessFailed);
    assert!(failure.retry_safe);
    assert!(run.provider_turn_ref.is_none());
    assert!(!application_home
        .join("tenders")
        .join(&tender_id)
        .join("staging")
        .join(format!("quarantine-agent-{run_id}"))
        .exists());
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

fn executable_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    }
}
