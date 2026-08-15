use std::{fs, io, path::Path, sync::Arc, time::Duration};

use quantix_lib::{
    ensure_quantix_setup, AgentAccessRequestStatus, AgentAccessResolution,
    AgentRunRecoveryDisposition, AgentRunState, AiProviderKind, ApproveAgentAccessCommand,
    CodexReadiness, CreateTenderCommand, DataClassification, DeviceProtection,
    InspectAgentRunCommand, InspectAgentRunHistoryCommand, InterruptAgentRunCommand,
    PermissionDenialReason, ProviderEventKind, ProviderFailureCategory, ProviderRateLimitState,
    ProviderReasoningSelection, QuantixHost, RequestAgentAccessCommand, ResolveAgentAccessCommand,
    ResolveIndeterminateAgentRunCommand, ReviseTenderCommand, RunBootstrapAgentCommand,
    RuntimeLayout, SetupPlatform, SetupState, StoragePermissions, TenderErrorCode,
    TenderIntegrityIssue, TenderIntegrityState, UpdateAiExecutionSelectionCommand,
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

    fn database(&self) -> rusqlite::Connection {
        rusqlite::Connection::open(
            self.application_home
                .join("tenders")
                .join(&self.tender_id)
                .join("tender.sqlite"),
        )
        .expect("open Tender Store database")
    }
}

#[tokio::test]
async fn signed_out_codex_is_rejected_before_a_provider_thread_is_created() {
    let harness = Harness::new("signed-out");

    let run = harness
        .host
        .run_bootstrap_agent(RunBootstrapAgentCommand {
            tender_id: harness.tender_id.clone(),
            retry_of_run_id: None,
        })
        .await
        .expect("record visible authentication failure");

    assert_eq!(run.state, AgentRunState::Failed, "{run:#?}");
    assert!(run.provider_thread_ref.is_none());
    assert!(run.provider_turn_ref.is_none());
    let failure = serde_json::to_value(run.failure.expect("Provider Failure"))
        .expect("serialize Provider Failure");
    assert_eq!(failure["category"], "authentication_required");
    assert_eq!(
        failure["required_user_action"],
        "Connect the Engineer User's Codex-managed ChatGPT subscription, then retry."
    );
}

#[tokio::test]
async fn codex_authentication_rpc_error_is_normalized_before_provider_work() {
    let harness = Harness::new("account-auth-error");

    let run = harness
        .host
        .run_bootstrap_agent(RunBootstrapAgentCommand {
            tender_id: harness.tender_id.clone(),
            retry_of_run_id: None,
        })
        .await
        .expect("record normalized authentication failure");

    assert_eq!(run.state, AgentRunState::Failed, "{run:#?}");
    assert!(run.provider_thread_ref.is_none());
    assert!(run.provider_turn_ref.is_none());
    let failure = run.failure.expect("Provider Failure");
    assert_eq!(
        failure.category,
        ProviderFailureCategory::AuthenticationRequired
    );
    assert!(failure.retry_safe);
    assert!(!serde_json::to_string(&run.events)
        .expect("serialize redacted events")
        .contains("fixture expired access token"));
}

#[tokio::test]
async fn non_subscription_codex_auth_is_rejected_before_provider_work() {
    let harness = Harness::new("api-key");

    let run = harness
        .host
        .run_bootstrap_agent(RunBootstrapAgentCommand {
            tender_id: harness.tender_id.clone(),
            retry_of_run_id: None,
        })
        .await
        .expect("record visible subscription failure");

    assert_eq!(run.state, AgentRunState::Failed, "{run:#?}");
    assert!(run.provider_thread_ref.is_none());
    let failure = serde_json::to_value(run.failure.expect("Provider Failure"))
        .expect("serialize Provider Failure");
    assert_eq!(failure["category"], "subscription_required");
    assert_eq!(
        failure["required_user_action"],
        "Connect an eligible Codex-managed ChatGPT subscription, then retry."
    );
}

#[tokio::test]
async fn codex_without_a_usable_model_is_rejected_before_provider_work() {
    let harness = Harness::new("missing-capability");

    let run = harness
        .host
        .run_bootstrap_agent(RunBootstrapAgentCommand {
            tender_id: harness.tender_id.clone(),
            retry_of_run_id: None,
        })
        .await
        .expect("record incompatible provider capability");

    assert_eq!(run.state, AgentRunState::Failed, "{run:#?}");
    assert!(run.provider_thread_ref.is_none());
    let failure = run.failure.expect("Provider Failure");
    assert_eq!(failure.category, ProviderFailureCategory::ProtocolInvalid);
    assert_eq!(
        failure.required_user_action,
        "Repair the pinned Codex runtime before retrying."
    );
}

#[tokio::test]
async fn codex_model_capability_is_discovered_across_paginated_results() {
    let harness = Harness::new("model-second-page");

    let run = harness
        .host
        .run_bootstrap_agent(RunBootstrapAgentCommand {
            tender_id: harness.tender_id.clone(),
            retry_of_run_id: None,
        })
        .await
        .expect("discover a text model on a later page");

    assert_eq!(run.state, AgentRunState::Completed, "{run:#?}");
    assert_eq!(run.provider_selection.provider, AiProviderKind::Codex);
    assert_eq!(run.provider_selection.model_id, "gpt-5.6-terra");
    assert_eq!(
        run.provider_selection.reasoning,
        ProviderReasoningSelection::CodexEffort("medium".into())
    );
    assert!(run.proposed_result.is_some());
}

#[tokio::test]
async fn app_server_version_drift_is_rejected_before_provider_work() {
    let harness = Harness::new("unsupported-provider-version");

    let run = harness
        .host
        .run_bootstrap_agent(RunBootstrapAgentCommand {
            tender_id: harness.tender_id.clone(),
            retry_of_run_id: None,
        })
        .await
        .expect("record incompatible app-server version");

    assert_eq!(run.state, AgentRunState::Failed, "{run:#?}");
    assert!(run.provider_thread_ref.is_none());
    let failure = run.failure.expect("Provider Failure");
    assert_eq!(failure.category, ProviderFailureCategory::ProtocolInvalid);
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
    assert!(run.task.permissions.workspace_write_allowed);
    assert_eq!(run.task.resource_budget.provider_turns, 1);
    assert_eq!(run.permission_grant.policy_version, 1);
    assert_eq!(run.permission_grant.capability_catalogue_version, 1);
    assert_eq!(run.permission_grant.work_plan_version, 1);
    assert_eq!(run.permission_grant.profile_id, run.profile.profile_id);
    assert_eq!(run.permission_grant.profile_version, run.profile.version);
    assert_eq!(run.permission_grant.task_id, run.task.task_id);
    assert_eq!(run.permission_grant.data_scopes, vec!["tender_metadata"]);
    assert_eq!(
        run.permission_grant.data_classifications,
        vec![DataClassification::TenderInternal]
    );
    assert!(!run.permission_grant.network_allowed);
    assert!(run.permission_grant.typed_tools.is_empty());
    assert_eq!(run.permission_grant.data_views.len(), 1);
    let data_view = &run.permission_grant.data_views[0];
    assert_eq!(data_view.schema_version, 1);
    assert_eq!(data_view.data_scope, "tender_metadata");
    assert_eq!(
        data_view.data_classification,
        DataClassification::TenderInternal
    );
    assert_eq!(data_view.relative_path, "inputs/tender-metadata-v1.json");
    assert_eq!(data_view.sha256.len(), 64);
    assert_eq!(run.permission_grant.workspace.workspace_id, run.run_id);
    assert_eq!(run.permission_grant.workspace.read_only_inputs, "inputs");
    assert_eq!(run.permission_grant.workspace.working_area, "working");
    assert_eq!(run.permission_grant.workspace.staged_outputs, "outputs");
    assert_eq!(run.provider_thread_ref.as_deref(), Some("thr_fixture_1"));
    assert_eq!(run.provider_turn_ref.as_deref(), Some("turn_fixture_1"));
    assert_eq!(
        run.events
            .iter()
            .map(|event| event.sequence)
            .collect::<Vec<_>>(),
        (1..=run.events.len() as u32).collect::<Vec<_>>()
    );
    let denied = run
        .events
        .iter()
        .find(|event| event.kind.as_str() == "control_request_denied")
        .expect("Host denial is inspectable");
    assert_eq!(denied.correlation_id.as_deref(), Some("control_fixture_1"));
    assert_eq!(
        denied.opaque_reference.as_deref(),
        Some("item/commandExecution/requestApproval")
    );
    assert_eq!(
        denied.denial_reason,
        Some(PermissionDenialReason::ProhibitedAction)
    );
    assert_eq!(run.usage.input_tokens, Some(120));
    assert_eq!(run.usage.output_tokens, Some(35));
    assert_eq!(run.usage.context_window, Some(200_000));
    let result = run.proposed_result.expect("validated Proposed result");
    assert_eq!(result.verification_status, VerificationStatus::Proposed);
    assert_eq!(result.data_scopes, vec!["tender_metadata"]);
    assert_eq!(
        result.data_classification,
        DataClassification::TenderInternal
    );
    assert_eq!(
        result.payload_json,
        r#"{"recommended_next_action":"Verify the imported package before detailed analysis.","summary":"Cairo Metro Systems Tender is ready for controlled intake analysis."}"#
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
    let workspace_facts = fs::read_to_string(harness.codex.with_extension("agent-workspace"))
        .expect("read materialized Agent Run Workspace facts");
    let workspace_facts: serde_json::Value =
        serde_json::from_str(&workspace_facts).expect("valid workspace facts");
    let workspace = workspace_facts["workspace"]
        .as_str()
        .expect("workspace path");
    assert!(workspace.starts_with(
        harness
            .application_home
            .join("staging")
            .to_string_lossy()
            .as_ref()
    ));
    assert!(!workspace.starts_with(
        harness
            .application_home
            .join("tenders")
            .join(&harness.tender_id)
            .to_string_lossy()
            .as_ref()
    ));
    assert_eq!(workspace_facts["input_read_only"], true);
    assert_eq!(workspace_facts["working_directory"], true);
    assert_eq!(workspace_facts["output_directory"], true);
    assert_eq!(
        workspace_facts.pointer("/data_view/data_scope"),
        Some(&serde_json::json!("tender_metadata"))
    );
    assert_eq!(
        workspace_facts.pointer("/data_view/data_classification"),
        Some(&serde_json::json!("tender_internal"))
    );
    assert_eq!(
        workspace_facts.pointer("/data_view/tender/name"),
        Some(&serde_json::json!("Cairo Metro Systems Tender"))
    );

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
async fn non_default_live_selection_is_bound_to_the_run_and_survives_later_changes() {
    let harness = Harness::new("selected-non-default");
    assert_eq!(
        harness
            .host
            .inspect_codex_subscription(tokio_util::sync::CancellationToken::new())
            .await,
        CodexReadiness::Ready
    );
    harness
        .host
        .update_ai_execution_selection(UpdateAiExecutionSelectionCommand {
            connection_id: "codex_chatgpt".into(),
            model_id: "gpt-5.6-sol".into(),
            reasoning: ProviderReasoningSelection::CodexEffort("high".into()),
        })
        .await
        .expect("select non-default live model");

    let run = harness
        .host
        .run_bootstrap_agent(RunBootstrapAgentCommand {
            tender_id: harness.tender_id.clone(),
            retry_of_run_id: None,
        })
        .await
        .expect("run with selected model");
    assert_eq!(run.provider_selection.model_id, "gpt-5.6-sol");
    assert_eq!(
        run.provider_selection.reasoning,
        ProviderReasoningSelection::CodexEffort("high".into())
    );

    harness
        .host
        .update_ai_execution_selection(UpdateAiExecutionSelectionCommand {
            connection_id: "codex_chatgpt".into(),
            model_id: "gpt-5.6-terra".into(),
            reasoning: ProviderReasoningSelection::CodexEffort("medium".into()),
        })
        .await
        .expect("change future-run selection");
    let historical = harness
        .host
        .inspect_agent_run(InspectAgentRunCommand {
            tender_id: harness.tender_id.clone(),
            run_id: run.run_id,
        })
        .expect("inspect historical run");
    assert_eq!(historical.provider_selection.model_id, "gpt-5.6-sol");
    assert_eq!(
        historical.provider_selection.reasoning,
        ProviderReasoningSelection::CodexEffort("high".into())
    );
}

#[tokio::test]
async fn one_provider_process_keeps_provider_threads_isolated_between_tenders() {
    let harness = Harness::new("success");
    let first = harness
        .host
        .run_bootstrap_agent(RunBootstrapAgentCommand {
            tender_id: harness.tender_id.clone(),
            retry_of_run_id: None,
        })
        .await
        .expect("run first Tender turn");
    let second_tender = harness
        .host
        .create_tender(CreateTenderCommand {
            name: "Alexandria Water Treatment Tender".into(),
        })
        .expect("create second Tender");
    let second = harness
        .host
        .run_bootstrap_agent(RunBootstrapAgentCommand {
            tender_id: second_tender.tender_id,
            retry_of_run_id: None,
        })
        .await
        .expect("run second Tender turn");

    assert_eq!(first.state, AgentRunState::Completed, "{first:#?}");
    assert_eq!(second.state, AgentRunState::Completed, "{second:#?}");
    assert_ne!(first.provider_thread_ref, second.provider_thread_ref);
    assert_eq!(
        fs::read_to_string(harness.codex.with_extension("agent-start-count"))
            .expect("read app-server start count"),
        "1"
    );
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
    let latest = harness
        .host
        .inspect_agent_run_history(InspectAgentRunHistoryCommand {
            tender_id: harness.tender_id.clone(),
            before_sequence: None,
            limit: 1,
        })
        .expect("inspect bounded latest Agent Run page");
    assert_eq!(latest.total_count, 2);
    assert_eq!(latest.items.len(), 1);
    assert_eq!(latest.items[0].run.run_id, retry.run_id);
    let older = harness
        .host
        .inspect_agent_run_history(InspectAgentRunHistoryCommand {
            tender_id: harness.tender_id.clone(),
            before_sequence: latest.next_before_sequence,
            limit: 1,
        })
        .expect("inspect bounded older Agent Run page");
    assert_eq!(older.items.len(), 1);
    assert_eq!(older.items[0].run.run_id, first.run_id);
    assert_eq!(older.next_before_sequence, None);
    assert_eq!(
        harness
            .host
            .inspect_agent_run(InspectAgentRunCommand {
                tender_id: harness.tender_id.clone(),
                run_id: retry.run_id.clone(),
            })
            .expect("inspect one exact Agent Run")
            .run_id,
        retry.run_id
    );
    let activity = harness
        .host
        .inspect_agent_run_activity(&harness.tender_id)
        .expect("inspect bounded Agent Run activity counters");
    assert_eq!(activity.run_count, 2);
    assert_eq!(activity.running_count, 0);
    assert!(activity.event_count >= 2);
}

#[tokio::test]
async fn retry_materializes_the_original_tasks_exact_tender_revision() {
    let harness = Harness::new("success");
    let first = harness
        .host
        .run_bootstrap_agent(RunBootstrapAgentCommand {
            tender_id: harness.tender_id.clone(),
            retry_of_run_id: None,
        })
        .await
        .expect("run original exact Tender revision");
    harness
        .host
        .revise_tender(ReviseTenderCommand {
            tender_id: harness.tender_id.clone(),
            name: "Cairo Metro Systems Tender - Addendum 1".into(),
        })
        .expect("register current Tender revision");
    harness.set_scenario("success-retry");

    let retry = harness
        .host
        .run_bootstrap_agent(RunBootstrapAgentCommand {
            tender_id: harness.tender_id.clone(),
            retry_of_run_id: Some(first.run_id),
        })
        .await
        .expect("retry original immutable Tender Task");

    assert_eq!(retry.task.exact_inputs[0].version, 1);
    assert_eq!(
        retry.proposed_result.expect("retry result").payload_json,
        r#"{"recommended_next_action":"Verify the imported package before detailed analysis.","summary":"Cairo Metro Systems Tender is ready for controlled intake analysis."}"#
    );
    let workspace_facts: serde_json::Value = serde_json::from_slice(
        &fs::read(harness.codex.with_extension("agent-workspace")).expect("retry workspace facts"),
    )
    .expect("valid retry workspace facts");
    assert_eq!(
        workspace_facts.pointer("/provider_data_view/source/version"),
        Some(&serde_json::json!(1))
    );
    assert_eq!(
        workspace_facts.pointer("/provider_data_view/tender/name"),
        Some(&serde_json::json!("Cairo Metro Systems Tender"))
    );
}

#[tokio::test]
async fn incompatible_cumulative_thread_exposure_starts_a_fresh_provider_thread() {
    let harness = Harness::new("success");
    let first = harness
        .host
        .run_bootstrap_agent(RunBootstrapAgentCommand {
            tender_id: harness.tender_id.clone(),
            retry_of_run_id: None,
        })
        .await
        .expect("run initial Agent Turn");
    harness
        .host
        .revise_tender(ReviseTenderCommand {
            tender_id: harness.tender_id.clone(),
            name: "Cairo Metro Systems Tender — Addendum 1".into(),
        })
        .expect("register a new exact Tender revision");
    harness.set_scenario("success-new-thread");

    let second = harness
        .host
        .run_bootstrap_agent(RunBootstrapAgentCommand {
            tender_id: harness.tender_id.clone(),
            retry_of_run_id: None,
        })
        .await
        .expect("run against the changed exact input");

    assert_eq!(first.provider_thread_ref.as_deref(), Some("thr_fixture_1"));
    assert_eq!(second.provider_thread_ref.as_deref(), Some("thr_fixture_2"));
    assert!(second
        .events
        .iter()
        .any(|event| event.kind.as_str() == "thread_established"));
    assert!(!second
        .events
        .iter()
        .any(|event| event.kind.as_str() == "thread_resumed"));
    assert_eq!(
        second.permission_grant.data_views[0].exact_inputs[0].version,
        2
    );
}

#[tokio::test]
async fn failed_remote_archive_does_not_claim_the_provider_thread_was_archived() {
    let harness = Harness::new("success");
    let first = harness
        .host
        .run_bootstrap_agent(RunBootstrapAgentCommand {
            tender_id: harness.tender_id.clone(),
            retry_of_run_id: None,
        })
        .await
        .expect("establish original Provider Thread");
    harness
        .host
        .revise_tender(ReviseTenderCommand {
            tender_id: harness.tender_id.clone(),
            name: "Cairo Metro Systems Tender - Addendum 1".into(),
        })
        .expect("make original Thread Exposure incompatible");
    harness.set_scenario("archive-failure");

    let failed = harness
        .host
        .run_bootstrap_agent(RunBootstrapAgentCommand {
            tender_id: harness.tender_id.clone(),
            retry_of_run_id: None,
        })
        .await
        .expect("archive failure is a recorded Agent Run outcome");

    assert_eq!(failed.state, AgentRunState::Failed, "{failed:#?}");
    let database = harness.database();
    let thread_status: String = database
        .query_row(
            "SELECT status FROM provider_threads WHERE thread_ref = ?1",
            [first.provider_thread_ref.expect("original Provider Thread")],
            |row| row.get(0),
        )
        .expect("Provider Thread status");
    assert_eq!(thread_status, "archive_pending");
    let archive_audits: u32 = database
        .query_row(
            "SELECT COUNT(*) FROM audit_events WHERE event_type = 'provider_thread_archived'",
            [],
            |row| row.get(0),
        )
        .expect("archive audit count");
    assert_eq!(archive_audits, 0);
}

#[tokio::test]
async fn pending_provider_thread_archive_reconciles_after_a_post_ack_checkpoint_failure() {
    let harness = Harness::new("success");
    let first = harness
        .host
        .run_bootstrap_agent(RunBootstrapAgentCommand {
            tender_id: harness.tender_id.clone(),
            retry_of_run_id: None,
        })
        .await
        .expect("establish original Provider Thread");
    harness
        .host
        .revise_tender(ReviseTenderCommand {
            tender_id: harness.tender_id.clone(),
            name: "Cairo Metro Systems Tender - Addendum 1".into(),
        })
        .expect("make original Thread Exposure incompatible");
    let database = harness.database();
    database
        .execute_batch(
            "CREATE TRIGGER fail_archive_checkpoint
             BEFORE UPDATE OF status ON provider_threads
             WHEN NEW.status = 'archived'
             BEGIN SELECT RAISE(ABORT, 'fixture checkpoint failure'); END;",
        )
        .expect("install one checkpoint failure");
    harness.set_scenario("success-new-thread");

    let failed = harness
        .host
        .run_bootstrap_agent(RunBootstrapAgentCommand {
            tender_id: harness.tender_id.clone(),
            retry_of_run_id: None,
        })
        .await
        .expect("record post-ack checkpoint failure");

    assert_eq!(failed.state, AgentRunState::Failed, "{failed:#?}");
    let status: String = database
        .query_row(
            "SELECT status FROM provider_threads WHERE thread_ref = ?1",
            [first.provider_thread_ref.expect("original Provider Thread")],
            |row| row.get(0),
        )
        .expect("pending Provider Thread archive");
    assert_eq!(status, "archive_pending");
    database
        .execute_batch("DROP TRIGGER fail_archive_checkpoint;")
        .expect("remove fixture checkpoint failure");
    harness.set_scenario("archive-already-complete");

    let reconciled = harness
        .host
        .run_bootstrap_agent(RunBootstrapAgentCommand {
            tender_id: harness.tender_id.clone(),
            retry_of_run_id: None,
        })
        .await
        .expect("confirm remote archive and start a fresh Provider Thread");

    assert_eq!(
        reconciled.state,
        AgentRunState::Completed,
        "{reconciled:#?}"
    );
    assert_eq!(
        reconciled.provider_thread_ref.as_deref(),
        Some("thr_fixture_2")
    );
    let archive_audits: u32 = database
        .query_row(
            "SELECT COUNT(*) FROM audit_events WHERE event_type = 'provider_thread_archived'",
            [],
            |row| row.get(0),
        )
        .expect("confirmed archive audit count");
    assert_eq!(archive_audits, 1);
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
    tokio::time::sleep(Duration::from_millis(3_100)).await;
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
async fn provider_declared_retry_fails_closed_without_hidden_side_effects() {
    let harness = Harness::new("retry-then-success");

    let run = harness
        .host
        .run_bootstrap_agent(RunBootstrapAgentCommand {
            tender_id: harness.tender_id.clone(),
            retry_of_run_id: None,
        })
        .await
        .expect("record the indeterminate Provider Turn");

    assert_eq!(run.state, AgentRunState::Indeterminate, "{run:#?}");
    assert_eq!(
        run.failure.as_ref().map(|failure| failure.category),
        Some(ProviderFailureCategory::OutcomeUnknown)
    );
    assert!(run.proposed_result.is_none());
    assert!(run
        .events
        .iter()
        .all(|event| { event.summary != "Provider reported a recoverable error and will retry" }));
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
async fn prompt_injected_provider_requests_cannot_expand_run_authority() {
    let harness = Harness::new("hostile-control-requests");

    let run = harness
        .host
        .run_bootstrap_agent(RunBootstrapAgentCommand {
            tender_id: harness.tender_id.clone(),
            retry_of_run_id: None,
        })
        .await
        .expect("record Host decisions for hostile provider requests");

    assert_eq!(run.state, AgentRunState::Completed, "{run:#?}");
    let denials = run
        .events
        .iter()
        .filter(|event| event.kind.as_str() == "control_request_denied")
        .collect::<Vec<_>>();
    assert_eq!(denials.len(), 8);
    assert!(denials.iter().all(|event| event.correlation_id.is_some()));
    assert!(denials
        .iter()
        .any(|event| { event.denial_reason == Some(PermissionDenialReason::ProhibitedAction) }));
    assert!(denials
        .iter()
        .any(|event| event.denial_reason == Some(PermissionDenialReason::ToolNotGranted)));
    assert!(denials
        .iter()
        .any(|event| event.denial_reason == Some(PermissionDenialReason::DefaultDeny)));
    let mut correlations = denials
        .iter()
        .filter_map(|event| event.correlation_id.as_deref())
        .collect::<Vec<_>>();
    correlations.sort_unstable();
    correlations.dedup();
    assert_eq!(correlations.len(), denials.len());
}

#[tokio::test]
async fn eitl_access_approval_is_persisted_and_expires_when_the_agent_run_ends() {
    let harness = Harness::new("access-tool");
    let running_host = harness.host.clone();
    let tender_id = harness.tender_id.clone();
    let running_tender_id = tender_id.clone();
    let running = tokio::spawn(async move {
        running_host
            .run_bootstrap_agent(RunBootstrapAgentCommand {
                tender_id: running_tender_id,
                retry_of_run_id: None,
            })
            .await
    });
    let active = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let run = harness
                .host
                .inspect_agent_runs(&tender_id)
                .expect("inspect active Agent Run")
                .into_iter()
                .find(|run| run.state == AgentRunState::Running && run.provider_turn_ref.is_some());
            if harness
                .codex
                .with_extension("access-tool-waiting")
                .is_file()
            {
                if let Some(run) = run {
                    break run;
                }
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("Provider Turn becomes active");

    let denied_request = harness
        .host
        .request_agent_access(RequestAgentAccessCommand {
            tender_id: tender_id.clone(),
            run_id: active.run_id.clone(),
            exact_inputs: active.permission_grant.access_ceiling.exact_inputs.clone(),
            data_scopes: active.permission_grant.access_ceiling.data_scopes.clone(),
            data_classifications: active
                .permission_grant
                .access_ceiling
                .data_classifications
                .clone(),
            allowed_actions: active
                .permission_grant
                .access_ceiling
                .allowed_actions
                .clone(),
            allowed_tools: active.permission_grant.access_ceiling.allowed_tools.clone(),
            purpose: "Engineer chooses not to grant this valid one-run request".into(),
            recurring: false,
        })
        .expect("create valid request for explicit Engineer denial");
    let denied = harness
        .host
        .resolve_agent_access(ResolveAgentAccessCommand {
            tender_id: tender_id.clone(),
            request_id: denied_request.request.request_id.clone(),
            run_id: active.run_id.clone(),
            resolution: AgentAccessResolution::Deny,
        })
        .expect("Engineer explicitly denies valid Access Request");
    assert_eq!(denied.status, AgentAccessRequestStatus::Denied);
    assert_eq!(
        denied.denial_reason,
        Some(PermissionDenialReason::EngineerDenied)
    );
    assert_eq!(
        harness
            .host
            .approve_agent_access(ApproveAgentAccessCommand {
                tender_id: tender_id.clone(),
                request_id: denied_request.request.request_id,
                run_id: active.run_id.clone(),
                expires_at: active.permission_grant.expires_at.clone(),
            })
            .expect_err("a decided request cannot later be approved")
            .code,
        TenderErrorCode::InvalidCommand
    );

    let request = harness
        .host
        .request_agent_access(RequestAgentAccessCommand {
            tender_id: tender_id.clone(),
            run_id: active.run_id.clone(),
            exact_inputs: active.permission_grant.access_ceiling.exact_inputs.clone(),
            data_scopes: active.permission_grant.access_ceiling.data_scopes.clone(),
            data_classifications: active
                .permission_grant
                .access_ceiling
                .data_classifications
                .clone(),
            allowed_actions: active
                .permission_grant
                .access_ceiling
                .allowed_actions
                .clone(),
            allowed_tools: active.permission_grant.access_ceiling.allowed_tools.clone(),
            purpose: "Continue with the exact approved Tender metadata".into(),
            recurring: false,
        })
        .expect("create blocked Access Request");
    assert_eq!(request.status, AgentAccessRequestStatus::Blocked);
    assert_eq!(
        active.permission_grant.typed_tools,
        Vec::new(),
        "the Typed Tool must be absent from the immutable base grant"
    );
    assert_eq!(
        request.request.allowed_tools,
        vec!["quantix_read_tender_metadata"]
    );

    let approved = harness
        .host
        .approve_agent_access(ApproveAgentAccessCommand {
            tender_id: tender_id.clone(),
            request_id: request.request.request_id.clone(),
            run_id: active.run_id.clone(),
            expires_at: active.permission_grant.expires_at.clone(),
        })
        .expect("record EITL one-run Access Approval");
    assert_eq!(approved.status, AgentAccessRequestStatus::Approved);
    assert_eq!(
        approved
            .one_run_grant
            .as_ref()
            .map(|grant| grant.approved_by.as_str()),
        Some("engineer_user")
    );
    assert_eq!(
        harness
            .host
            .inspect_agent_runs(&tender_id)
            .expect("inspect approved access")
            .into_iter()
            .find(|run| run.run_id == active.run_id)
            .expect("active Agent Run")
            .access_requests[1]
            .status,
        AgentAccessRequestStatus::Approved
    );

    fs::write(harness.codex.with_extension("access-approved"), b"approved")
        .expect("release approved Typed Tool call");
    let completed = running
        .await
        .expect("join EITL-controlled Agent Run")
        .expect("record completed Agent Run");
    assert_eq!(completed.state, AgentRunState::Completed, "{completed:#?}");
    assert_eq!(
        completed.access_requests[1].status,
        AgentAccessRequestStatus::Expired
    );
    assert!(completed.events.iter().any(|event| {
        event.correlation_id.as_deref() == Some("access_tool_before_approval")
            && event.denial_reason == Some(PermissionDenialReason::ToolNotGranted)
    }));
    assert!(completed.events.iter().any(|event| {
        event.kind == ProviderEventKind::ControlRequestResolved
            && event.correlation_id.as_deref() == Some("access_tool_after_approval")
            && event.opaque_reference.as_deref() == Some("quantix_read_tender_metadata")
            && event.request_fingerprint.is_none()
            && event.denial_reason.is_none()
    }));
    let tool_audits: u32 = harness
        .database()
        .query_row(
            "SELECT COUNT(*) FROM audit_events
             WHERE event_type = 'agent_typed_tool_executed'",
            [],
            |row| row.get(0),
        )
        .expect("Typed Tool audit count");
    assert_eq!(tool_audits, 1);
}

#[tokio::test]
async fn engineer_can_revoke_approved_access_before_the_provider_uses_it() {
    let harness = Harness::new("access-tool-revoked");
    let running_host = harness.host.clone();
    let tender_id = harness.tender_id.clone();
    let running_tender_id = tender_id.clone();
    let running = tokio::spawn(async move {
        running_host
            .run_bootstrap_agent(RunBootstrapAgentCommand {
                tender_id: running_tender_id,
                retry_of_run_id: None,
            })
            .await
    });
    let active = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let run = harness
                .host
                .inspect_agent_runs(&tender_id)
                .expect("inspect active Agent Run")
                .into_iter()
                .find(|run| run.state == AgentRunState::Running && run.provider_turn_ref.is_some());
            if harness
                .codex
                .with_extension("access-tool-waiting")
                .is_file()
            {
                if let Some(run) = run {
                    break run;
                }
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("Provider Turn becomes active");
    let request = harness
        .host
        .request_agent_access(RequestAgentAccessCommand {
            tender_id: tender_id.clone(),
            run_id: active.run_id.clone(),
            exact_inputs: active.permission_grant.access_ceiling.exact_inputs.clone(),
            data_scopes: active.permission_grant.access_ceiling.data_scopes.clone(),
            data_classifications: active
                .permission_grant
                .access_ceiling
                .data_classifications
                .clone(),
            allowed_actions: active
                .permission_grant
                .access_ceiling
                .allowed_actions
                .clone(),
            allowed_tools: active.permission_grant.access_ceiling.allowed_tools.clone(),
            purpose: "Read the exact Tender metadata once".into(),
            recurring: false,
        })
        .expect("create one-run Access Request");
    harness
        .host
        .approve_agent_access(ApproveAgentAccessCommand {
            tender_id: tender_id.clone(),
            request_id: request.request.request_id.clone(),
            run_id: active.run_id.clone(),
            expires_at: active.permission_grant.expires_at.clone(),
        })
        .expect("approve one-run access");

    let revoked = harness
        .host
        .resolve_agent_access(ResolveAgentAccessCommand {
            tender_id: tender_id.clone(),
            request_id: request.request.request_id,
            run_id: active.run_id,
            resolution: AgentAccessResolution::Revoke,
        })
        .expect("Engineer revokes approved access");
    assert_eq!(revoked.status, AgentAccessRequestStatus::Revoked);
    assert_eq!(
        revoked.denial_reason,
        Some(PermissionDenialReason::AccessRevoked)
    );
    fs::write(harness.codex.with_extension("access-approved"), b"revoked")
        .expect("release post-revocation Typed Tool call");

    let completed = running
        .await
        .expect("join EITL-controlled Agent Run")
        .expect("record completed Agent Run");
    assert_eq!(completed.state, AgentRunState::Completed, "{completed:#?}");
    assert!(completed.events.iter().any(|event| {
        event.correlation_id.as_deref() == Some("access_tool_after_approval")
            && event.denial_reason == Some(PermissionDenialReason::ToolNotGranted)
    }));
    assert_eq!(
        completed.access_requests[0].status,
        AgentAccessRequestStatus::Revoked
    );
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
        .join("staging")
        .join(format!(
            "quarantine-agent-{}-{}",
            harness.tender_id, run.run_id
        ))
        .is_dir());
}

#[tokio::test]
async fn lost_turn_start_response_is_indeterminate_and_quarantined() {
    let harness = Harness::new("turn-start-response-lost");

    let run = harness
        .host
        .run_bootstrap_agent(RunBootstrapAgentCommand {
            tender_id: harness.tender_id.clone(),
            retry_of_run_id: None,
        })
        .await
        .expect("record uncertain Provider Turn acceptance");

    assert_eq!(run.state, AgentRunState::Indeterminate, "{run:#?}");
    let failure = run.failure.as_ref().expect("indeterminate failure");
    assert_eq!(failure.category, ProviderFailureCategory::OutcomeUnknown);
    assert!(!failure.retry_safe);
    assert!(run.provider_thread_ref.is_some());
    assert!(run.provider_turn_ref.is_none());
    assert!(run.proposed_result.is_none());
    assert!(harness
        .application_home
        .join("staging")
        .join(format!(
            "quarantine-agent-{}-{}",
            harness.tender_id, run.run_id
        ))
        .is_dir());

    harness.set_scenario("success");
    let retry = harness
        .host
        .run_bootstrap_agent(RunBootstrapAgentCommand {
            tender_id: harness.tender_id.clone(),
            retry_of_run_id: Some(run.run_id),
        })
        .await
        .expect_err("uncertain acceptance must block a linked retry");
    assert_eq!(retry.code, TenderErrorCode::InvalidCommand);
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
async fn engineer_disposition_permits_one_attributable_linked_retry() {
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

    harness
        .host
        .close_tender(&harness.tender_id)
        .expect("close Tender before Host restart");
    let resources = harness
        .codex
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .expect("fixture resources")
        .to_path_buf();
    let restarted = QuantixHost::with_setup_platform_and_runtime(
        &harness.application_home,
        Arc::new(ReadySetupPlatform),
        RuntimeLayout::bundled(resources),
    );
    let before_decision = restarted
        .inspect_agent_runs(&harness.tender_id)
        .expect("inspect indeterminate run without Provider readiness");
    assert_eq!(before_decision[0].state, AgentRunState::Indeterminate);

    let decision = restarted
        .resolve_indeterminate_agent_run(ResolveIndeterminateAgentRunCommand {
            tender_id: harness.tender_id.clone(),
            run_id: indeterminate.run_id.clone(),
            disposition: AgentRunRecoveryDisposition::RetryTask,
            rationale: "The quarantined candidate was inspected and must not be published.".into(),
        })
        .expect("record attributable Engineer disposition");
    assert_eq!(decision.run_id, indeterminate.run_id);
    assert_eq!(decision.disposition, AgentRunRecoveryDisposition::RetryTask);
    assert_eq!(decision.decided_by, "engineer_user");
    let inspected = restarted
        .inspect_agent_runs(&harness.tender_id)
        .expect("inspect recovery decision");
    assert_eq!(
        inspected[0].recovery_decision.as_ref(),
        Some(&decision),
        "the Engineer disposition must be visible at the Agent Run boundary"
    );

    harness.set_scenario("success");
    restarted.accept_runtime_fixture();
    let unlinked = restarted
        .run_bootstrap_agent(RunBootstrapAgentCommand {
            tender_id: harness.tender_id.clone(),
            retry_of_run_id: None,
        })
        .await
        .expect_err("retry disposition requires the exact linked run");
    assert_eq!(unlinked.code, TenderErrorCode::InvalidCommand);
    let retry = restarted
        .run_bootstrap_agent(RunBootstrapAgentCommand {
            tender_id: harness.tender_id.clone(),
            retry_of_run_id: Some(indeterminate.run_id.clone()),
        })
        .await
        .expect("run one separate linked retry after Engineer disposition");
    assert_eq!(retry.state, AgentRunState::Completed, "{retry:#?}");
    assert_eq!(
        retry.retry_of_run_id.as_deref(),
        Some(indeterminate.run_id.as_str())
    );
    let second_retry = restarted
        .run_bootstrap_agent(RunBootstrapAgentCommand {
            tender_id: harness.tender_id.clone(),
            retry_of_run_id: Some(indeterminate.run_id.clone()),
        })
        .await
        .expect_err("one Engineer disposition permits only one linked retry");
    assert_eq!(second_retry.code, TenderErrorCode::InvalidCommand);

    let duplicate = restarted
        .resolve_indeterminate_agent_run(ResolveIndeterminateAgentRunCommand {
            tender_id: harness.tender_id.clone(),
            run_id: indeterminate.run_id,
            disposition: AgentRunRecoveryDisposition::RetryTask,
            rationale: "A second disposition must not rewrite history.".into(),
        })
        .expect_err("Engineer disposition is immutable");
    assert_eq!(duplicate.code, TenderErrorCode::InvalidCommand);
}

#[tokio::test]
async fn close_task_disposition_never_authorizes_a_linked_retry() {
    let harness = Harness::new("malformed-after-turn");
    let indeterminate = harness
        .host
        .run_bootstrap_agent(RunBootstrapAgentCommand {
            tender_id: harness.tender_id.clone(),
            retry_of_run_id: None,
        })
        .await
        .expect("record indeterminate Agent Run");
    harness
        .host
        .resolve_indeterminate_agent_run(ResolveIndeterminateAgentRunCommand {
            tender_id: harness.tender_id.clone(),
            run_id: indeterminate.run_id.clone(),
            disposition: AgentRunRecoveryDisposition::CloseTask,
            rationale: "The Engineer closed this task without accepting quarantined output.".into(),
        })
        .expect("close the uncertain task");

    harness.set_scenario("success");
    let linked = harness
        .host
        .run_bootstrap_agent(RunBootstrapAgentCommand {
            tender_id: harness.tender_id.clone(),
            retry_of_run_id: Some(indeterminate.run_id),
        })
        .await
        .expect_err("closed uncertain task cannot be retried");
    assert_eq!(linked.code, TenderErrorCode::InvalidCommand);

    let separate = harness
        .host
        .run_bootstrap_agent(RunBootstrapAgentCommand {
            tender_id: harness.tender_id.clone(),
            retry_of_run_id: None,
        })
        .await
        .expect("separate task may run after uncertainty is closed");
    assert_eq!(separate.state, AgentRunState::Completed, "{separate:#?}");
}

#[tokio::test]
async fn semantically_invalid_agent_manifest_requires_tender_recovery() {
    let harness = Harness::new("malformed-after-turn");
    let run = harness
        .host
        .run_bootstrap_agent(RunBootstrapAgentCommand {
            tender_id: harness.tender_id.clone(),
            retry_of_run_id: None,
        })
        .await
        .expect("record Agent Run");
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
        run.usage.rate_limit.as_ref().map(|limit| limit.state),
        Some(ProviderRateLimitState::Exhausted)
    );
    let primary = run
        .usage
        .rate_limit
        .as_ref()
        .and_then(|limit| limit.primary.as_ref())
        .expect("preserved rate-limit recovery window");
    assert_eq!(primary.window_minutes, Some(15));
    assert_eq!(primary.resets_at_epoch_seconds, Some(1_780_000_900));
    assert!(run.proposed_result.is_none());
}

#[tokio::test]
async fn chatgpt_rate_limit_updates_are_normalized_without_raw_account_data() {
    let harness = Harness::new("rate-limit-update-success");

    let run = harness
        .host
        .run_bootstrap_agent(RunBootstrapAgentCommand {
            tender_id: harness.tender_id.clone(),
            retry_of_run_id: None,
        })
        .await
        .expect("record normalized rate-limit observation");

    assert_eq!(run.state, AgentRunState::Completed, "{run:#?}");
    let rate_limit = run
        .usage
        .rate_limit
        .as_ref()
        .expect("normalized Codex capacity");
    assert_eq!(rate_limit.state, ProviderRateLimitState::Exhausted);
    let primary = rate_limit.primary.as_ref().expect("primary limit window");
    assert_eq!(primary.used_percent, 100);
    assert_eq!(primary.window_minutes, Some(15));
    assert_eq!(primary.resets_at_epoch_seconds, Some(1_780_000_900));
    assert!(rate_limit.secondary.is_none());
    assert!(run.events.iter().any(|event| {
        event.kind.as_str() == "rate_limit_observed"
            && event.summary == "Codex subscription capacity observed"
    }));
    let inspection = serde_json::to_string(&run).expect("serialize Agent Run inspection");
    assert!(!inspection.contains("plus"));
    assert!(!inspection.contains("usedPercent"));
    assert!(!inspection.contains("rate_limit_reached"));
}

#[tokio::test]
async fn authentication_state_loss_without_a_turn_outcome_is_indeterminate() {
    let harness = Harness::new("auth-notification-loss");

    let run = harness
        .host
        .run_bootstrap_agent(RunBootstrapAgentCommand {
            tender_id: harness.tender_id.clone(),
            retry_of_run_id: None,
        })
        .await
        .expect("record indeterminate authentication-state loss");

    assert_eq!(run.state, AgentRunState::Indeterminate, "{run:#?}");
    assert!(run.provider_thread_ref.is_some());
    assert!(run.provider_turn_ref.is_some());
    let failure = run.failure.as_ref().expect("outcome-unknown failure");
    assert_eq!(failure.category, ProviderFailureCategory::OutcomeUnknown);
    assert!(!failure.retry_safe);
    assert!(run.proposed_result.is_none());
    assert!(harness
        .application_home
        .join("staging")
        .join(format!(
            "quarantine-agent-{}-{}",
            harness.tender_id, run.run_id
        ))
        .is_dir());
}

#[tokio::test]
async fn missing_subscription_plan_without_a_turn_outcome_is_indeterminate() {
    let harness = Harness::new("subscription-notification-loss");

    let run = harness
        .host
        .run_bootstrap_agent(RunBootstrapAgentCommand {
            tender_id: harness.tender_id.clone(),
            retry_of_run_id: None,
        })
        .await
        .expect("record indeterminate subscription-state loss");

    assert_eq!(run.state, AgentRunState::Indeterminate, "{run:#?}");
    assert!(run.provider_thread_ref.is_some());
    assert!(run.provider_turn_ref.is_some());
    let failure = run.failure.as_ref().expect("outcome-unknown failure");
    assert_eq!(failure.category, ProviderFailureCategory::OutcomeUnknown);
    assert!(!failure.retry_safe);
    assert!(run.proposed_result.is_none());
    assert!(harness
        .application_home
        .join("staging")
        .join(format!(
            "quarantine-agent-{}-{}",
            harness.tender_id, run.run_id
        ))
        .is_dir());
}

#[tokio::test]
async fn authentication_loss_during_a_turn_fails_visibly_without_a_hidden_retry() {
    let harness = Harness::new("auth-loss");

    let run = harness
        .host
        .run_bootstrap_agent(RunBootstrapAgentCommand {
            tender_id: harness.tender_id.clone(),
            retry_of_run_id: None,
        })
        .await
        .expect("record authentication loss");

    assert_eq!(run.state, AgentRunState::Failed, "{run:#?}");
    assert!(run.provider_thread_ref.is_some());
    assert!(run.provider_turn_ref.is_some());
    let failure = run.failure.expect("Provider Failure");
    assert_eq!(
        serde_json::to_value(failure.category).expect("serialize category"),
        "authentication_required"
    );
    assert!(failure.retry_safe);
    assert_eq!(
        failure.required_user_action,
        "Reconnect the Engineer User's Codex-managed ChatGPT subscription before creating a linked retry."
    );
    assert!(run.proposed_result.is_none());
    assert_eq!(
        fs::read_to_string(harness.codex.with_extension("agent-start-count"))
            .expect("read app-server start count"),
        "1"
    );
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
async fn provider_usage_event_is_inspectable_while_the_turn_is_running() {
    let harness = Harness::new("usage-stream");
    let host = harness.host.clone();
    let tender_id = harness.tender_id.clone();
    let running = tokio::spawn(async move {
        host.run_bootstrap_agent(RunBootstrapAgentCommand {
            tender_id,
            retry_of_run_id: None,
        })
        .await
    });

    let live_run = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(run) = harness
                .host
                .inspect_agent_runs(&harness.tender_id)
                .expect("inspect running Provider Turn")
                .into_iter()
                .find(|run| {
                    run.state == AgentRunState::Running
                        && [
                            ProviderEventKind::RateLimitObserved,
                            ProviderEventKind::UsageObserved,
                        ]
                        .into_iter()
                        .all(|kind| run.events.iter().any(|event| event.kind == kind))
                })
            {
                break run;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("usage observation becomes inspectable before terminal completion");

    assert_eq!(live_run.usage.input_tokens, Some(120));
    assert_eq!(live_run.usage.cached_input_tokens, Some(20));
    assert_eq!(live_run.usage.output_tokens, Some(35));
    assert_eq!(live_run.usage.reasoning_output_tokens, Some(10));
    assert_eq!(live_run.usage.total_tokens, Some(155));
    assert_eq!(live_run.usage.context_window, Some(200_000));
    let rate_limit = live_run
        .usage
        .rate_limit
        .as_ref()
        .expect("live normalized subscription capacity");
    assert_eq!(rate_limit.state, ProviderRateLimitState::Exhausted);
    let primary = rate_limit.primary.as_ref().expect("live primary window");
    assert_eq!(primary.used_percent, 100);
    assert_eq!(primary.window_minutes, Some(15));
    assert_eq!(primary.resets_at_epoch_seconds, Some(1_780_000_900));

    assert!(harness
        .host
        .interrupt_agent_run(InterruptAgentRunCommand {
            tender_id: harness.tender_id.clone(),
            run_id: live_run.run_id,
        })
        .expect("interrupt the streaming fixture"));
    let run = running
        .await
        .expect("Agent Run task joins")
        .expect("interrupted Agent Run is recorded");
    assert_eq!(run.state, AgentRunState::Interrupted, "{run:#?}");
}

#[tokio::test]
async fn host_restart_preserves_streamed_usage_on_an_indeterminate_run() {
    let Harness {
        _root,
        application_home,
        codex,
        host,
        tender_id,
    } = Harness::new("usage-stream");
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

    let run_id = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(run) = host
                .inspect_agent_runs(&tender_id)
                .expect("inspect streamed Provider usage")
                .into_iter()
                .find(|run| {
                    run.state == AgentRunState::Running
                        && run.usage.input_tokens == Some(120)
                        && run.usage.rate_limit.is_some()
                })
            {
                break run.run_id;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("normalized usage is persisted before Host restart");

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
        .expect("reconcile Agent Run with streamed usage")
        .into_iter()
        .find(|run| run.run_id == run_id)
        .expect("reconciled Agent Run");

    assert_eq!(run.state, AgentRunState::Indeterminate, "{run:#?}");
    assert_eq!(run.usage.input_tokens, Some(120));
    assert_eq!(run.usage.total_tokens, Some(155));
    let rate_limit = run
        .usage
        .rate_limit
        .expect("preserved normalized subscription capacity");
    assert_eq!(rate_limit.state, ProviderRateLimitState::Exhausted);
    assert_eq!(
        rate_limit
            .primary
            .expect("preserved primary window")
            .used_percent,
        100
    );
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
    let cancellation_facts: u32 = harness
        .database()
        .query_row(
            "SELECT COUNT(*) FROM agent_run_cancellations WHERE run_id = ?1",
            [&run_id],
            |row| row.get(0),
        )
        .expect("persisted cancellation fact");
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
async fn engineer_interruption_before_turn_acceptance_discloses_no_data_view() {
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
            if harness.codex.with_extension("thread-waiting").is_file() {
                if let Some(run) = harness
                    .host
                    .inspect_agent_runs(&harness.tender_id)
                    .expect("inspect pre-Turn Agent Run")
                    .into_iter()
                    .find(|run| run.state == AgentRunState::Running)
                {
                    assert!(run.provider_turn_ref.is_none());
                    break run.run_id;
                }
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("Provider Thread request becomes pending");

    assert!(harness
        .host
        .interrupt_agent_run(InterruptAgentRunCommand {
            tender_id: harness.tender_id.clone(),
            run_id: run_id.clone(),
        })
        .expect("interrupt before Provider Turn acceptance"));
    assert_eq!(
        harness
            .host
            .request_agent_access(RequestAgentAccessCommand {
                tender_id: harness.tender_id.clone(),
                run_id: run_id.clone(),
                exact_inputs: Vec::new(),
                data_scopes: Vec::new(),
                data_classifications: Vec::new(),
                allowed_actions: Vec::new(),
                allowed_tools: Vec::new(),
                purpose: "This request must be rejected after interruption".into(),
                recurring: false,
            })
            .expect_err("interrupted runs cannot create Access Requests")
            .code,
        TenderErrorCode::InvalidCommand
    );
    let run = running
        .await
        .expect("Agent Run task joins")
        .expect("interrupted Agent Run is recorded");

    assert_eq!(run.state, AgentRunState::Interrupted, "{run:#?}");
    assert!(run.provider_turn_ref.is_none());
    assert!(!harness.codex.with_extension("agent-workspace").exists());
}

#[tokio::test]
async fn persisted_interruption_wins_over_a_racing_provider_completion() {
    let harness = Harness::new("complete-after-interrupt");
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
            if harness
                .codex
                .with_extension("completion-race-waiting")
                .is_file()
            {
                if let Some(run) = harness
                    .host
                    .inspect_agent_runs(&harness.tender_id)
                    .expect("inspect racing Agent Run")
                    .into_iter()
                    .find(|run| run.state == AgentRunState::Running)
                {
                    break run.run_id;
                }
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("Provider completion race becomes active");

    assert!(harness
        .host
        .interrupt_agent_run(InterruptAgentRunCommand {
            tender_id: harness.tender_id.clone(),
            run_id: run_id.clone(),
        })
        .expect("persist interruption before Provider completion"));
    let run = running
        .await
        .expect("Agent Run task joins")
        .expect("racing Agent Run is recorded");

    assert_eq!(run.state, AgentRunState::Interrupted, "{run:#?}");
    assert!(run.proposed_result.is_none());
    assert_eq!(
        run.failure.as_ref().map(|failure| failure.category),
        Some(ProviderFailureCategory::Interrupted)
    );
    assert!(run.events.iter().any(|event| {
        event.kind == ProviderEventKind::Terminal
            && event.summary == "Provider outcome discarded after Engineer interruption"
    }));
    assert!(run.events.iter().all(|event| {
        event.kind != ProviderEventKind::Terminal || event.summary != "Provider Turn completed"
    }));
}

#[tokio::test]
async fn provider_readiness_time_consumes_the_absolute_run_duration_budget() {
    let harness = Harness::new("delayed-readiness");

    let run = harness
        .host
        .run_bootstrap_agent(RunBootstrapAgentCommand {
            tender_id: harness.tender_id.clone(),
            retry_of_run_id: None,
        })
        .await
        .expect("record absolute-duration failure");

    assert_eq!(run.state, AgentRunState::Failed, "{run:#?}");
    assert_eq!(
        run.failure.as_ref().map(|failure| failure.category),
        Some(ProviderFailureCategory::PermissionDenied)
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
        .join("staging")
        .join(format!("quarantine-agent-{tender_id}-{run_id}"))
        .is_dir());
}

#[tokio::test]
async fn host_restart_during_turn_start_dispatch_is_indeterminate() {
    let Harness {
        _root,
        application_home,
        codex,
        host,
        tender_id,
    } = Harness::new("hang-after-turn-request");
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
            let dispatched = codex
                .with_extension("turn-accepted-without-response")
                .is_file()
                && host
                    .inspect_agent_runs(&tender_id)
                    .expect("inspect Provider Turn dispatch checkpoint")
                    .into_iter()
                    .any(|run| {
                        run.state == AgentRunState::Running
                            && run.provider_turn_ref.is_none()
                            && run
                                .events
                                .iter()
                                .any(|event| event.kind == ProviderEventKind::TurnRequested)
                    });
            if dispatched {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("Provider Turn dispatch remains unacknowledged");
    let run_id = host
        .inspect_agent_runs(&tender_id)
        .expect("inspect unacknowledged Provider Turn")
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
        .expect("reconcile uncertain Provider Turn dispatch")
        .into_iter()
        .find(|run| run.run_id == run_id)
        .expect("reconciled Agent Run");

    assert_eq!(run.state, AgentRunState::Indeterminate, "{run:#?}");
    let failure = run.failure.as_ref().expect("restart failure");
    assert_eq!(failure.category, ProviderFailureCategory::OutcomeUnknown);
    assert!(!failure.retry_safe);
    assert!(run.provider_turn_ref.is_none());
    assert!(application_home
        .join("staging")
        .join(format!("quarantine-agent-{tender_id}-{run_id}"))
        .is_dir());
}

#[tokio::test]
async fn provider_control_denial_is_durable_across_an_abrupt_host_restart() {
    let Harness {
        _root,
        application_home,
        codex,
        host,
        tender_id,
    } = Harness::new("deny-then-hang");
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
        while !codex.with_extension("denial-waiting").is_file() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("Provider Control Request was denied before the simulated crash");
    let run_id = host
        .inspect_agent_runs(&tender_id)
        .expect("inspect active denied Agent Run")
        .into_iter()
        .find(|run| run.state == AgentRunState::Running)
        .expect("running Agent Run")
        .run_id;

    running.abort();
    assert!(running
        .await
        .expect_err("simulate abrupt Host stop after denial")
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
        .expect("reconcile denied Agent Run")
        .into_iter()
        .find(|run| run.run_id == run_id)
        .expect("reconciled Agent Run");

    let denial = run
        .events
        .iter()
        .find(|event| event.kind.as_str() == "control_request_denied")
        .expect("crash-durable Provider Control Request denial");
    assert_eq!(denial.correlation_id.as_deref(), Some("control_fixture_1"));
    assert_eq!(
        denial.denial_reason,
        Some(PermissionDenialReason::ProhibitedAction)
    );
    let database = rusqlite::Connection::open(
        application_home
            .join("tenders")
            .join(&tender_id)
            .join("tender.sqlite"),
    )
    .expect("open reconciled Tender Store");
    let denial_audits: u32 = database
        .query_row(
            "SELECT COUNT(*) FROM audit_events
             WHERE event_type = 'provider_control_request_denied'",
            [],
            |row| row.get(0),
        )
        .expect("denial audit count");
    assert_eq!(denial_audits, 1);
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
        .join("staging")
        .join(format!("quarantine-agent-{tender_id}-{run_id}"))
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
