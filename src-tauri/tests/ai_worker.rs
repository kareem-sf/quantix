use std::fs;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use quantix_lib::{
    ensure_quantix_setup, worker_python_path, ManagedWorkerRuntimeState, QuantixHost,
    RuntimeLayout, SetupPlatform, SetupState, StoragePermissions, WorkerApproval,
    WorkerFailureCategory, WorkerOperation, WorkerRunRequest, WorkerToolDescriptor,
    MINIMUM_SETUP_FREE_SPACE_BYTES,
};
use serde_json::json;

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

struct WorkerHarness {
    _root: tempfile::TempDir,
    application_home: PathBuf,
    host: QuantixHost,
}

impl WorkerHarness {
    fn new() -> Self {
        let root = tempfile::tempdir().expect("temporary worker harness");
        let application_home = root.path().join(".quantix");
        let resources = root.path().join("resources");
        let runtime = resources.join("runtime");
        let runtime_bin = runtime.join("bin");
        let worker_project = runtime.join("ai-worker");
        fs::create_dir_all(&runtime_bin).expect("runtime bin");
        fs::create_dir_all(&worker_project).expect("worker project");

        let fixture = Path::new(env!("CARGO_BIN_EXE_quantix-runtime-fixture"));
        fs::copy(fixture, runtime_bin.join("uv.exe")).expect("copy fixture uv");
        fs::write(runtime_bin.join("uv.version"), "0.12.2\n").expect("uv version");
        fs::write(runtime_bin.join("ocr.version"), "3.9.2\n").expect("ocr version");

        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("runtime")
            .join("ai-worker");
        for name in [
            "pyproject.toml",
            "uv.lock",
            ".python-version",
            "python-downloads.json",
        ] {
            fs::copy(source.join(name), worker_project.join(name)).expect("copy worker source");
        }

        let host = QuantixHost::with_setup_platform_and_runtime(
            &application_home,
            Arc::new(ReadySetupPlatform),
            RuntimeLayout::bundled(&resources),
        );
        assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
        Self {
            _root: root,
            application_home,
            host,
        }
    }

    async fn prepared(&self) {
        let prepared = self
            .host
            .prepare_worker_runtime(tokio_util::sync::CancellationToken::new())
            .await
            .expect("worker prepared");
        assert_eq!(prepared.state, ManagedWorkerRuntimeState::Ready);
    }

    fn scenario(&self, scenario: &str) {
        fs::write(
            worker_python_path(&self.application_home).with_extension("agent-scenario"),
            scenario,
        )
        .expect("scenario sidecar");
    }
}

fn request(operation: WorkerOperation) -> WorkerRunRequest {
    WorkerRunRequest {
        route: "openai".to_owned(),
        base_url: None,
        api_key: "test-key".to_owned(),
        model_id: "test-model".to_owned(),
        reasoning: None,
        instructions: "helpful".to_owned(),
        output_schema: None,
        tools: Vec::new(),
        input: "Do the work.".to_owned(),
        operation,
        timeout: Duration::from_secs(30),
    }
}

fn approval_tool() -> WorkerToolDescriptor {
    WorkerToolDescriptor {
        name: "lookup_item".to_owned(),
        description: Some("look up an item".to_owned()),
        parameters: json!({"type": "object", "properties": {"query": {"type": "string"}}}),
    }
}

#[tokio::test]
async fn probe_reports_usage_and_text() {
    let harness = WorkerHarness::new();
    harness.prepared().await;
    harness.scenario("probe-success");
    let outcome = harness
        .host
        .run_worker_operation_for_test(
            request(WorkerOperation::Probe),
            tokio_util::sync::CancellationToken::new(),
            |_, _, _| unreachable!("a probe never calls tools"),
        )
        .await
        .expect("probe succeeds");
    match outcome {
        quantix_lib::WorkerOutcome::Probe { text, usage } => {
            assert_eq!(text, "OK");
            assert_eq!(usage.input_tokens, 9);
            assert_eq!(usage.output_tokens, 3);
        }
        other => panic!("unexpected outcome {other:?}"),
    }
}

#[tokio::test]
async fn probe_surfaces_provider_auth_failures() {
    let harness = WorkerHarness::new();
    harness.prepared().await;
    harness.scenario("probe-auth-failure");
    let error = harness
        .host
        .run_worker_operation_for_test(
            request(WorkerOperation::Probe),
            tokio_util::sync::CancellationToken::new(),
            |_, _, _| unreachable!("a probe never calls tools"),
        )
        .await
        .expect_err("auth failure");
    assert_eq!(error.category, WorkerFailureCategory::Auth);
}

#[tokio::test]
async fn turn_produces_text_and_usage() {
    let harness = WorkerHarness::new();
    harness.prepared().await;
    harness.scenario("turn-success");
    let outcome = harness
        .host
        .run_worker_operation_for_test(
            request(WorkerOperation::Turn),
            tokio_util::sync::CancellationToken::new(),
            |_, _, _| unreachable!("no tools were offered"),
        )
        .await
        .expect("turn succeeds");
    match outcome {
        quantix_lib::WorkerOutcome::Turn {
            output,
            text,
            usage,
        } => {
            assert_eq!(text, "worker final text");
            assert_eq!(output, None);
            assert_eq!(usage.input_tokens, 21);
        }
        other => panic!("unexpected outcome {other:?}"),
    }
}

#[tokio::test]
async fn structured_output_reaches_the_host_as_json() {
    let harness = WorkerHarness::new();
    harness.prepared().await;
    harness.scenario("turn-success");
    let mut request = request(WorkerOperation::Turn);
    request.output_schema = Some(json!({
        "type": "object",
        "properties": {"summary": {"type": "string"}},
        "required": ["summary"],
    }));
    let outcome = harness
        .host
        .run_worker_operation_for_test(
            request,
            tokio_util::sync::CancellationToken::new(),
            |_, _, _| unreachable!("no tools were offered"),
        )
        .await
        .expect("structured turn succeeds");
    match outcome {
        quantix_lib::WorkerOutcome::Turn { output, .. } => {
            assert_eq!(output, Some(json!({"summary": "worker structured output"})));
        }
        other => panic!("unexpected outcome {other:?}"),
    }
}

#[tokio::test]
async fn tool_calls_pause_for_the_host_and_relay_results() {
    let harness = WorkerHarness::new();
    harness.prepared().await;
    harness.scenario("tool-roundtrip");
    let mut request = request(WorkerOperation::Turn);
    request.tools = vec![approval_tool()];
    let outcome = harness
        .host
        .run_worker_operation_for_test(
            request,
            tokio_util::sync::CancellationToken::new(),
            |tool_call_id, name, arguments| {
                // The correlation id is what lets the host write an audit record for
                // the call it is approving.
                assert!(!tool_call_id.is_empty());
                assert_eq!(name, "lookup_item");
                assert_eq!(arguments, &json!({"query": "fixture"}));
                WorkerApproval::Approved(json!({"item": "fixture-item"}))
            },
        )
        .await
        .expect("tool turn succeeds");
    match outcome {
        quantix_lib::WorkerOutcome::Turn { text, .. } => {
            assert_eq!(text, "worker finished after tools");
        }
        other => panic!("unexpected outcome {other:?}"),
    }
    let approvals = fs::read_to_string(
        worker_python_path(&harness.application_home).with_extension("worker-approvals"),
    )
    .expect("observed approvals");
    let approvals: serde_json::Value = serde_json::from_str(&approvals).expect("approval json");
    assert_eq!(approvals[0]["approved"], json!(true));
    assert_eq!(approvals[0]["result"], json!({"item": "fixture-item"}));
}

#[tokio::test]
async fn denied_tool_calls_reach_the_worker_with_the_denial_message() {
    let harness = WorkerHarness::new();
    harness.prepared().await;
    harness.scenario("tool-denied");
    let mut request = request(WorkerOperation::Turn);
    request.tools = vec![approval_tool()];
    let outcome = harness
        .host
        .run_worker_operation_for_test(
            request,
            tokio_util::sync::CancellationToken::new(),
            |_, name, _| {
                assert_eq!(name, "lookup_item");
                WorkerApproval::Denied(
                    "Denied by Quantix: outside this run's permissions".to_owned(),
                )
            },
        )
        .await
        .expect("denied tool turn still completes");
    match outcome {
        quantix_lib::WorkerOutcome::Turn { text, .. } => {
            assert_eq!(text, "worker finished after tools");
        }
        other => panic!("unexpected outcome {other:?}"),
    }
    let approvals = fs::read_to_string(
        worker_python_path(&harness.application_home).with_extension("worker-approvals"),
    )
    .expect("observed approvals");
    let approvals: serde_json::Value = serde_json::from_str(&approvals).expect("approval json");
    assert_eq!(approvals[0]["approved"], json!(false));
    assert_eq!(
        approvals[0]["denial_message"],
        json!("Denied by Quantix: outside this run's permissions")
    );
}

#[tokio::test]
async fn tool_round_ceiling_fails_closed() {
    let harness = WorkerHarness::new();
    harness.prepared().await;
    harness.scenario("tool-round-loop");
    let mut request = request(WorkerOperation::Turn);
    request.tools = vec![approval_tool()];
    let error = harness
        .host
        .run_worker_operation_for_test(
            request,
            tokio_util::sync::CancellationToken::new(),
            |_, _, _| WorkerApproval::Approved(json!({})),
        )
        .await
        .expect_err("round ceiling");
    assert_eq!(error.category, WorkerFailureCategory::Budget);
}

#[tokio::test]
async fn malformed_worker_output_fails_closed() {
    let harness = WorkerHarness::new();
    harness.prepared().await;
    harness.scenario("malformed-output");
    let error = harness
        .host
        .run_worker_operation_for_test(
            request(WorkerOperation::Probe),
            tokio_util::sync::CancellationToken::new(),
            |_, _, _| unreachable!("a probe never calls tools"),
        )
        .await
        .expect_err("malformed frame");
    assert_eq!(error.category, WorkerFailureCategory::Protocol);
}

#[tokio::test]
async fn cancellation_stops_the_worker() {
    let harness = WorkerHarness::new();
    harness.prepared().await;
    harness.scenario("worker-hang");
    let cancellation = tokio_util::sync::CancellationToken::new();
    let operation = harness.host.run_worker_operation_for_test(
        request(WorkerOperation::Turn),
        cancellation.clone(),
        |_, _, _| unreachable!("no tools were offered"),
    );
    cancellation.cancel();
    let error = operation.await.expect_err("cancelled operation");
    assert_eq!(error.category, WorkerFailureCategory::Cancelled);
}
