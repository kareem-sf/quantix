use std::{fs, io, path::Path, sync::Arc, time::Duration};

use quantix_lib::{
    configure_tauri_builder, current_update_platform, ensure_quantix_setup, QuantixHost,
    RuntimeLayout, RuntimeReadinessIssue, RuntimeReadinessState, SetupPlatform, SetupState,
    SignedArtifactIdentity, StoragePermissions, UpdateCandidate, UpdateCompatibilityManifest,
    UpdateDecision, UpdateDiagnostic, UpdateImpact, UpdateReleaseInformation,
    MINIMUM_SETUP_FREE_SPACE_BYTES,
};
use sha2::{Digest, Sha256};
use tauri::test::{get_ipc_response, mock_builder, mock_context, noop_assets, INVOKE_KEY};
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

struct RuntimeHarness {
    _root: tempfile::TempDir,
    application_home: std::path::PathBuf,
    runtime_bin: std::path::PathBuf,
    host: QuantixHost,
}

impl RuntimeHarness {
    fn new() -> Self {
        let root = tempfile::tempdir().expect("temporary runtime harness");
        let application_home = root.path().join(".quantix");
        let resources = root.path().join("resources");
        let runtime = resources.join("runtime");
        let runtime_bin = runtime.join("bin");
        let ocr_project = runtime.join("ocr");
        fs::create_dir_all(&runtime_bin).expect("runtime bin");
        fs::create_dir_all(&ocr_project).expect("OCR project");

        let fixture = Path::new(env!("CARGO_BIN_EXE_quantix-runtime-fixture"));
        fs::copy(fixture, runtime_bin.join(executable_name("uv"))).expect("copy fixture tool");
        fs::write(runtime_bin.join("uv.version"), "0.12.2\n").expect("uv version");
        fs::write(runtime_bin.join("ocr.version"), "3.9.2\n").expect("OCR version");

        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("runtime")
            .join("ocr");
        for name in [
            "pyproject.toml",
            "uv.lock",
            ".python-version",
            "readiness.pdf",
            "approved-model-sources.json",
            "python-downloads.json",
            "prepare_models.py",
            "ocr_document.py",
        ] {
            fs::copy(source.join(name), ocr_project.join(name)).expect("copy runtime source");
        }
        fs::write(
            runtime.join("runtime-provenance.json"),
            serde_json::to_vec(&serde_json::json!({
                "schema_version": 3,
                "platform": std::env::consts::OS,
                "architecture": std::env::consts::ARCH,
                "uv": {
                    "version": "0.12.2",
                    "sha256": sha256(&runtime_bin.join(executable_name("uv"))),
                },
                "ocr_project_files": hashed_files(&ocr_project),
            }))
            .expect("serialize runtime provenance"),
        )
        .expect("write runtime provenance");

        let host = QuantixHost::with_setup_platform_and_runtime(
            &application_home,
            Arc::new(ReadySetupPlatform),
            RuntimeLayout::bundled(resources),
        );
        assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
        Self {
            _root: root,
            application_home,
            runtime_bin,
            host,
        }
    }

    fn managed_ocr_python(&self) -> std::path::PathBuf {
        self.application_home
            .join("runtimes")
            .join("ocr")
            .join(if cfg!(windows) { "Scripts" } else { "bin" })
            .join(executable_name("python"))
    }
}

fn application_only_update() -> UpdateCandidate {
    UpdateCandidate {
        current_version: "0.1.0".into(),
        version: "0.2.0".into(),
        platform: current_update_platform().expect("supported test platform"),
        artifact: SignedArtifactIdentity {
            sha256: "a".repeat(64),
            signature_sha256: "b".repeat(64),
        },
        compatibility: UpdateCompatibilityManifest {
            installation_schema_version: 24,
            tender_schema_version: 35,
            ocr_version: "3.9.2".into(),
            runtime_manifest_schema_version: 3,
        },
        release: UpdateReleaseInformation {
            published_at: "2026-08-12T10:00:00Z".into(),
            title: "Quantix 0.2.0".into(),
            notes: "Application-only update".into(),
        },
        impact: UpdateImpact {
            summary: "Application-only update".into(),
            stored_data_may_change: false,
        },
    }
}

#[tokio::test]
async fn readiness_reports_each_engineer_actionable_runtime_state() {
    let harness = RuntimeHarness::new();

    let missing_ocr = harness.host.inspect_runtime_readiness().await;
    assert_eq!(missing_ocr.state, RuntimeReadinessState::MissingExecutable);
    assert_eq!(
        missing_ocr.issues,
        vec![RuntimeReadinessIssue::OcrExecutableMissing]
    );
    assert!(missing_ocr.repair_available);

    let ready = harness.host.repair_runtime_readiness().await;
    assert_eq!(ready.state, RuntimeReadinessState::Ready, "{ready:?}");
    assert!(ready.issues.is_empty());
    assert_eq!(ready.uv_version.as_deref(), Some("0.12.2"));
    assert_eq!(ready.ocr_version.as_deref(), Some("3.9.2"));
    let confirmed_ready = harness.host.inspect_runtime_readiness().await;
    assert_eq!(confirmed_ready.state, RuntimeReadinessState::Ready);

    let unexpected_model = harness
        .application_home
        .join("models")
        .join("ocr")
        .join("unexpected.bin");
    fs::write(&unexpected_model, b"unexpected").expect("write unexpected model file");
    let unlisted_model = harness.host.inspect_runtime_readiness().await;
    assert_eq!(unlisted_model.state, RuntimeReadinessState::MissingModel);
    fs::remove_file(unexpected_model).expect("remove unexpected model file");

    fs::write(harness.runtime_bin.join("uv.version"), "0.11.1\n").expect("incompatible uv version");
    let incompatible = harness.host.inspect_runtime_readiness().await;
    assert_eq!(
        incompatible.state,
        RuntimeReadinessState::IncompatibleVersion
    );
    assert_eq!(
        incompatible.issues,
        vec![RuntimeReadinessIssue::UvVersionIncompatible]
    );

    fs::write(harness.runtime_bin.join("uv.version"), "0.12.2\n").expect("restore uv version");
    fs::write(
        harness
            .application_home
            .join("models")
            .join("ocr")
            .join("PP-OCRv6_det_small.onnx"),
        b"altered model",
    )
    .expect("alter model fixture");
    let missing_model = harness.host.inspect_runtime_readiness().await;
    assert_eq!(missing_model.state, RuntimeReadinessState::MissingModel);
    assert_eq!(
        missing_model.issues,
        vec![RuntimeReadinessIssue::OcrModelsMissing]
    );
    assert!(missing_model.repair_available);

    fs::remove_file(harness.runtime_bin.join(executable_name("uv"))).expect("remove uv fixture");
    let missing_bundled_tools = harness.host.inspect_runtime_readiness().await;
    assert_eq!(
        missing_bundled_tools.state,
        RuntimeReadinessState::MissingExecutable
    );
    assert_eq!(
        missing_bundled_tools.issues,
        vec![RuntimeReadinessIssue::UvExecutableMissing,]
    );
    assert!(!missing_bundled_tools.repair_available);
}

#[tokio::test]
async fn runtime_preparation_does_not_launch_the_ocr_engine_to_read_its_version() {
    let harness = RuntimeHarness::new();

    let ready = harness.host.repair_runtime_readiness().await;

    assert_eq!(ready.state, RuntimeReadinessState::Ready, "{ready:?}");
    assert_eq!(ready.ocr_version.as_deref(), Some("3.9.2"));
    assert!(
        !harness
            .managed_ocr_python()
            .with_extension("version-ready")
            .exists(),
        "the heavyweight OCR engine must not run during version inspection",
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn in_flight_runtime_probe_holds_the_global_ordinary_work_lease_until_children_exit() {
    let harness = RuntimeHarness::new();
    let offered = harness
        .host
        .present_update(application_only_update())
        .expect("present exact update");
    let update_id = offered.offer.expect("offer identity").update_id;
    harness
        .host
        .decide_update(
            update_id.clone(),
            UpdateDecision::Approve,
            "Approve only after every readiness child exits".into(),
        )
        .expect("approve exact update");
    fs::write(harness.runtime_bin.join("uv.version-delay"), "5000\n")
        .expect("delay the public uv version readiness seam");
    let probe_ready = harness.runtime_bin.join("uv.version-ready");

    let probing_host = harness.host.clone();
    let probe = tokio::spawn(async move { probing_host.inspect_runtime_readiness().await });
    for _ in 0..500 {
        if probe_ready.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(probe_ready.exists(), "the child readiness probe started");

    let resources = harness
        .runtime_bin
        .parent()
        .and_then(Path::parent)
        .expect("runtime resource root");
    let contender = QuantixHost::with_setup_platform_and_runtime(
        &harness.application_home,
        Arc::new(ReadySetupPlatform),
        RuntimeLayout::bundled(resources),
    );
    assert_eq!(ensure_quantix_setup(&contender).state, SetupState::Ready);
    assert_eq!(
        contender
            .authorize_update_installation(&update_id)
            .expect_err("update installation cannot race a spawned readiness child")
            .diagnostic,
        UpdateDiagnostic::ActiveWork
    );

    let readiness = probe.await.expect("readiness task completed");
    assert_eq!(
        readiness.state,
        RuntimeReadinessState::MissingExecutable,
        "the fixture still reports missing OCR after its retained-lease probe"
    );
}

#[tokio::test]
async fn interrupted_preparation_is_fail_closed_after_restart() {
    let harness = RuntimeHarness::new();
    let connection =
        rusqlite::Connection::open(harness.application_home.join("installation.sqlite"))
            .expect("installation catalogue");
    connection
        .execute(
            "UPDATE runtime_preparation SET status = 'preparing' WHERE singleton = 1",
            [],
        )
        .expect("record interrupted preparation");

    let resources = harness
        .runtime_bin
        .parent()
        .and_then(Path::parent)
        .expect("runtime resource root");
    let restarted_host = QuantixHost::with_setup_platform_and_runtime(
        &harness.application_home,
        Arc::new(ReadySetupPlatform),
        RuntimeLayout::bundled(resources),
    );
    let restarted = restarted_host.inspect_runtime_readiness().await;
    assert_eq!(
        restarted.state,
        RuntimeReadinessState::InterruptedPreparation
    );
    assert_eq!(
        restarted.issues,
        vec![RuntimeReadinessIssue::RuntimePreparationInterrupted]
    );
    assert!(restarted.repair_available);
}

#[tokio::test]
async fn bundled_ocr_provenance_is_verified_before_execution() {
    let harness = RuntimeHarness::new();
    let project_file = harness
        .runtime_bin
        .parent()
        .expect("runtime root")
        .join("ocr")
        .join("pyproject.toml");
    fs::write(&project_file, b"[project]\nname = 'altered'\n").expect("alter bundled OCR project");

    let readiness = harness.host.inspect_runtime_readiness().await;
    assert_eq!(readiness.state, RuntimeReadinessState::RepairRequired);
    assert_eq!(
        readiness.issues,
        vec![RuntimeReadinessIssue::RuntimeResourceIntegrityFailed]
    );
    assert!(!readiness.repair_available);
}

#[tokio::test]
async fn managed_environment_drift_requires_repair() {
    let harness = RuntimeHarness::new();
    let ready = harness.host.repair_runtime_readiness().await;
    assert_eq!(ready.state, RuntimeReadinessState::Ready, "{ready:?}");
    fs::write(
        harness
            .application_home
            .join("runtimes")
            .join("ocr")
            .join("fixture_dependency.py"),
        b"fixture=false",
    )
    .expect("alter transitive environment file");
    let contamination = harness
        .application_home
        .join("runtimes")
        .join("ocr")
        .join("unmanaged-contamination.bin");
    fs::write(&contamination, b"unmanaged").expect("write unmanaged environment file");

    let readiness = harness.host.inspect_runtime_readiness().await;
    assert_eq!(readiness.state, RuntimeReadinessState::RepairRequired);
    assert_eq!(
        readiness.issues,
        vec![RuntimeReadinessIssue::OcrEnvironmentInvalid]
    );
    assert!(readiness.repair_available);

    let repaired = harness.host.repair_runtime_readiness().await;
    assert_eq!(repaired.state, RuntimeReadinessState::Ready, "{repaired:?}");
    assert!(!contamination.exists());
    assert_eq!(
        fs::read(
            harness
                .application_home
                .join("runtimes")
                .join("ocr")
                .join("fixture_dependency.py")
        )
        .expect("read repaired dependency"),
        b"fixture=true\n"
    );
}

#[cfg(windows)]
#[tokio::test]
async fn repair_unlinks_an_external_managed_python_junction_without_touching_its_target() {
    let harness = RuntimeHarness::new();
    let ready = harness.host.repair_runtime_readiness().await;
    assert_eq!(ready.state, RuntimeReadinessState::Ready, "{ready:?}");

    let external = harness._root.path().join("external-python");
    fs::create_dir_all(&external).expect("external Python directory");
    let external_file = external.join("must-remain.txt");
    fs::write(&external_file, b"outside").expect("external Python marker");
    let hostile = harness
        .application_home
        .join("runtimes")
        .join("python")
        .join("hostile-external");
    junction::create(&external, &hostile).expect("external Python junction");

    let invalid = harness.host.inspect_runtime_readiness().await;
    assert_eq!(invalid.state, RuntimeReadinessState::RepairRequired);
    assert_eq!(
        invalid.issues,
        vec![RuntimeReadinessIssue::OcrEnvironmentInvalid]
    );

    let repaired = harness.host.repair_runtime_readiness().await;
    assert_eq!(repaired.state, RuntimeReadinessState::Ready, "{repaired:?}");
    assert!(!hostile.exists());
    assert_eq!(
        fs::read(external_file).expect("external marker survived repair"),
        b"outside"
    );
}

#[cfg(windows)]
#[tokio::test]
async fn model_junctions_fail_closed_instead_of_hashing_external_files() {
    let harness = RuntimeHarness::new();
    let ready = harness.host.repair_runtime_readiness().await;
    assert_eq!(ready.state, RuntimeReadinessState::Ready, "{ready:?}");
    let external = harness._root.path().join("external-models");
    fs::create_dir_all(&external).expect("external model directory");
    fs::write(external.join("outside.bin"), b"outside").expect("external model fixture");
    let link = harness
        .application_home
        .join("models")
        .join("ocr")
        .join("linked-models");
    junction::create(&external, &link).expect("model junction");

    let readiness = harness.host.inspect_runtime_readiness().await;
    assert_eq!(readiness.state, RuntimeReadinessState::MissingModel);
    assert_eq!(
        readiness.issues,
        vec![RuntimeReadinessIssue::OcrModelsMissing]
    );

    junction::delete(link).expect("remove model junction");
}

#[tokio::test]
async fn engineer_cancellation_never_publishes_partial_readiness() {
    let harness = RuntimeHarness::new();
    fs::write(harness.runtime_bin.join("uv.delay"), "5000\n").expect("uv fixture delay");

    let repairing_host = harness.host.clone();
    let repair = tokio::spawn(async move { repairing_host.repair_runtime_readiness().await });
    tokio::time::sleep(Duration::from_millis(150)).await;
    let concurrent = harness.host.repair_runtime_readiness().await;
    assert_eq!(concurrent.state, RuntimeReadinessState::Preparing);
    assert_eq!(
        concurrent.issues,
        vec![RuntimeReadinessIssue::RuntimePreparationActive]
    );
    assert!(harness.host.cancel_runtime_preparation());
    let outcome = repair.await.expect("repair task");

    assert_eq!(outcome.state, RuntimeReadinessState::RepairRequired);
    assert_eq!(
        outcome.issues,
        vec![RuntimeReadinessIssue::RuntimePreparationCancelled]
    );
    assert!(!harness
        .application_home
        .join("runtimes")
        .join("ocr-readiness.json")
        .exists());
    assert!(!harness.host.cancel_runtime_preparation());
}

#[tokio::test]
async fn engineer_cancellation_during_the_final_probe_cannot_publish_ready() {
    let harness = RuntimeHarness::new();
    fs::write(harness.runtime_bin.join("uv.version-delay"), "5000\n").expect("uv fixture delay");
    let probe_ready = harness.runtime_bin.join("uv.version-ready");

    let repairing_host = harness.host.clone();
    let repair = tokio::spawn(async move { repairing_host.repair_runtime_readiness().await });
    for _ in 0..200 {
        if probe_ready.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(probe_ready.exists(), "final local-tool probe started");
    assert!(harness.host.cancel_runtime_preparation());
    let outcome = repair.await.expect("repair task");

    assert_eq!(outcome.state, RuntimeReadinessState::RepairRequired);
    assert_eq!(
        outcome.issues,
        vec![RuntimeReadinessIssue::RuntimePreparationCancelled]
    );
    assert!(!harness.host.cancel_runtime_preparation());
}

#[test]
fn renderer_receives_only_named_runtime_readiness_facts() {
    let harness = RuntimeHarness::new();
    let app = configure_tauri_builder(mock_builder())
        .manage(harness.host.clone())
        .build(mock_context(noop_assets()))
        .expect("test Tauri application");
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("test webview");

    let response = get_ipc_response(
        &webview,
        tauri::webview::InvokeRequest {
            cmd: "inspect_document_tool_readiness".into(),
            callback: tauri::ipc::CallbackFn(0),
            error: tauri::ipc::CallbackFn(1),
            url: if cfg!(any(windows, target_os = "android")) {
                "http://tauri.localhost"
            } else {
                "tauri://localhost"
            }
            .parse()
            .expect("Tauri IPC URL"),
            body: tauri::ipc::InvokeBody::default(),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        },
    )
    .expect("inspect_document_tool_readiness response")
    .deserialize::<serde_json::Value>()
    .expect("runtime readiness JSON");

    assert_eq!(response["state"], "missing_executable");
    assert_eq!(
        response["issues"],
        serde_json::json!(["ocr_executable_missing"])
    );
    assert!(response.get("executable").is_none());
    assert!(response.get("arguments").is_none());
    assert!(response.get("path").is_none());
}

fn executable_name(name: &str) -> String {
    if std::env::consts::EXE_EXTENSION.is_empty() {
        name.to_owned()
    } else {
        format!("{name}.{}", std::env::consts::EXE_EXTENSION)
    }
}

fn sha256(path: &Path) -> String {
    let bytes = fs::read(path).expect("read fixture digest source");
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn hashed_files(root: &Path) -> Vec<serde_json::Value> {
    let mut files = WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .map(Result::unwrap)
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| {
            let relative = entry
                .path()
                .strip_prefix(root)
                .unwrap()
                .components()
                .map(|component| component.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            serde_json::json!({
                "path": relative,
                "size_bytes": entry.metadata().unwrap().len(),
                "sha256": sha256(entry.path()),
            })
        })
        .collect::<Vec<_>>();
    files.sort_by(|left, right| {
        left["path"]
            .as_str()
            .unwrap()
            .cmp(right["path"].as_str().unwrap())
    });
    files
}
