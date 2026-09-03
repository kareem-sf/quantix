use std::fs;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use quantix_lib::{
    ensure_quantix_setup, worker_python_path, ManagedWorkerRuntimeState, QuantixHost,
    RuntimeLayout, SetupPlatform, SetupState, StoragePermissions, MINIMUM_SETUP_FREE_SPACE_BYTES,
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

    fn venv_python(&self) -> PathBuf {
        worker_python_path(&self.application_home)
    }

    fn provenance(&self) -> PathBuf {
        self.application_home
            .join("runtimes")
            .join("ai-worker-provenance.json")
    }

    fn marker(&self) -> PathBuf {
        self.application_home
            .join("runtimes")
            .join("ai-worker-preparing")
    }
}

fn runtime_root(harness: &WorkerHarness) -> PathBuf {
    harness.application_home.join("runtimes")
}

#[tokio::test]
async fn fresh_home_installs_the_worker_and_short_circuits() {
    let harness = WorkerHarness::new();
    let inspected = harness.host.inspect_worker_runtime();
    assert_eq!(inspected.state, ManagedWorkerRuntimeState::NotInstalled);

    let prepared = harness
        .host
        .prepare_worker_runtime(tokio_util::sync::CancellationToken::new())
        .await
        .expect("worker prepare");
    assert_eq!(prepared.state, ManagedWorkerRuntimeState::Ready);
    assert!(harness.venv_python().is_file());
    assert!(harness.provenance().is_file());
    assert!(!harness.marker().exists());

    let again = harness
        .host
        .prepare_worker_runtime(tokio_util::sync::CancellationToken::new())
        .await
        .expect("second prepare");
    assert_eq!(again.state, ManagedWorkerRuntimeState::Ready);
}

#[tokio::test]
async fn outdated_provenance_triggers_resync() {
    let harness = WorkerHarness::new();
    harness
        .host
        .prepare_worker_runtime(tokio_util::sync::CancellationToken::new())
        .await
        .expect("initial prepare");
    let provenance = fs::read_to_string(harness.provenance()).expect("provenance");
    let mut value: serde_json::Value = serde_json::from_str(&provenance).expect("provenance json");
    value["lock_sha256"] = serde_json::Value::String("0".repeat(64));
    fs::write(harness.provenance(), value.to_string()).expect("rewrite provenance");

    let inspected = harness.host.inspect_worker_runtime();
    assert_eq!(inspected.state, ManagedWorkerRuntimeState::Outdated);

    let prepared = harness
        .host
        .prepare_worker_runtime(tokio_util::sync::CancellationToken::new())
        .await
        .expect("resync");
    assert_eq!(prepared.state, ManagedWorkerRuntimeState::Ready);
}

#[tokio::test]
async fn interrupted_marker_is_reported_then_converges() {
    let harness = WorkerHarness::new();
    fs::create_dir_all(runtime_root(&harness)).expect("runtimes dir");
    fs::write(harness.marker(), b"preparing").expect("marker");

    let inspected = harness.host.inspect_worker_runtime();
    assert_eq!(
        inspected.state,
        ManagedWorkerRuntimeState::InterruptedPreparation
    );

    let prepared = harness
        .host
        .prepare_worker_runtime(tokio_util::sync::CancellationToken::new())
        .await
        .expect("converge");
    assert_eq!(prepared.state, ManagedWorkerRuntimeState::Ready);
    assert!(!harness.marker().exists());
}

#[tokio::test]
async fn cancellation_cleans_the_marker() {
    let harness = WorkerHarness::new();
    let cancellation = tokio_util::sync::CancellationToken::new();
    cancellation.cancel();
    let error = harness
        .host
        .prepare_worker_runtime(cancellation)
        .await
        .expect_err("cancelled prepare");
    assert!(matches!(error, quantix_lib::ManagedRuntimeError::Cancelled));
    assert!(!harness.marker().exists());
    let inspected = harness.host.inspect_worker_runtime();
    assert_eq!(inspected.state, ManagedWorkerRuntimeState::NotInstalled);
}
