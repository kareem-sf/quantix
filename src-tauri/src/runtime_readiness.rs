use std::{
    ffi::OsString,
    fs,
    path::{Component, Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use rusqlite::{params, Connection, TransactionBehavior};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;
use ts_rs::TS;
use walkdir::WalkDir;

use crate::{
    agent_runtime::{CodexReadiness, CODEX_PROTOCOL_SCHEMA, CODEX_VERSION},
    process_supervisor::{ProcessOutput, ProcessSpec, ProcessSupervisor, ProcessTermination},
    setup::SetupState,
    QuantixHost,
};

const UV_VERSION: &str = "0.12.2";
pub(crate) const DOCLING_VERSION: &str = "2.118.0";
const PYTHON_VERSION: &str = "3.12.13";
pub(crate) const RUNTIME_PROVENANCE_SCHEMA: u32 = 2;
const DOCLING_MANIFEST_SCHEMA: u32 = 2;
const VERSION_TIMEOUT: Duration = Duration::from_secs(10);
const ENVIRONMENT_CHECK_TIMEOUT: Duration = Duration::from_secs(60);
const PREPARATION_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const MODEL_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const SMOKE_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const PROBE_OUTPUT_LIMIT: usize = 256 * 1024;
const PREPARATION_OUTPUT_LIMIT: usize = 1024 * 1024;
const MAX_SMOKE_JSON_BYTES: u64 = 16 * 1024 * 1024;
const SMOKE_OCR_TEXT: &str =
    "Docling bundles PDF document conversion to JSON and Markdown in an easy self contained package";
const DOCLING_MODEL_PROFILE: [&str; 5] = [
    "layout",
    "tableformer",
    "code_formula",
    "picture_classifier",
    "rapidocr",
];
#[derive(Debug, Clone)]
pub struct RuntimeLayout {
    runtime_resources: PathBuf,
}

impl RuntimeLayout {
    pub fn bundled(resource_directory: impl AsRef<Path>) -> Self {
        Self {
            runtime_resources: resource_directory.as_ref().join("runtime"),
        }
    }

    pub(crate) fn codex_executable(&self) -> PathBuf {
        self.runtime_resources
            .join("bin")
            .join(executable_name("codex"))
    }

    fn uv_executable(&self) -> PathBuf {
        self.runtime_resources
            .join("bin")
            .join(executable_name("uv"))
    }

    pub(crate) fn docling_project(&self) -> PathBuf {
        self.runtime_resources.join("docling")
    }

    fn readiness_document(&self) -> PathBuf {
        self.docling_project().join("readiness.pdf")
    }

    fn provenance_manifest(&self) -> PathBuf {
        self.runtime_resources.join("runtime-provenance.json")
    }

    fn codex_schema(&self) -> PathBuf {
        self.runtime_resources
            .join("codex_app_server_protocol.schemas.json")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum RuntimeReadinessState {
    Ready,
    Preparing,
    MissingExecutable,
    IncompatibleVersion,
    MissingModel,
    AuthenticationRequired,
    InterruptedPreparation,
    RepairRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum RuntimeReadinessIssue {
    SetupIncomplete,
    CodexExecutableMissing,
    UvExecutableMissing,
    DoclingExecutableMissing,
    CodexVersionIncompatible,
    UvVersionIncompatible,
    DoclingVersionIncompatible,
    RuntimeResourceIntegrityFailed,
    DoclingEnvironmentInvalid,
    DoclingModelsMissing,
    CodexAuthenticationRequired,
    CodexSubscriptionRequired,
    RuntimePreparationActive,
    RuntimePreparationInterrupted,
    RuntimePreparationFailed,
    RuntimeProbeFailed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct RuntimeReadiness {
    pub state: RuntimeReadinessState,
    pub issues: Vec<RuntimeReadinessIssue>,
    pub codex_version: Option<String>,
    pub uv_version: Option<String>,
    pub docling_version: Option<String>,
    pub repair_available: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum RuntimePreparationActivityStatus {
    Pending,
    Active,
    Complete,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum RuntimePreparationStatus {
    Idle,
    Preparing,
    Ready,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum RuntimePreparationStep {
    ValidateResources,
    VerifyBundledTools,
    ResetEnvironment,
    SynchronizeEnvironment,
    VerifyDocling,
    PrepareDocumentModels,
    RunDocumentCheck,
    PublishRuntime,
    FinalRuntimeCheck,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct RuntimePreparationActivity {
    pub step: RuntimePreparationStep,
    pub title: String,
    pub detail: String,
    pub status: RuntimePreparationActivityStatus,
    pub started_at_epoch_ms: Option<u64>,
    pub finished_at_epoch_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct RuntimePreparationProgress {
    pub status: RuntimePreparationStatus,
    pub activities: Vec<RuntimePreparationActivity>,
    pub started_at_epoch_ms: Option<u64>,
    pub updated_at_epoch_ms: Option<u64>,
    pub model_files_written: Option<u64>,
    pub model_bytes_written: Option<u64>,
}

impl RuntimePreparationProgress {
    pub(crate) fn idle() -> Self {
        Self {
            status: RuntimePreparationStatus::Idle,
            activities: runtime_preparation_activities(),
            started_at_epoch_ms: None,
            updated_at_epoch_ms: None,
            model_files_written: None,
            model_bytes_written: None,
        }
    }

    pub(crate) fn begin(&mut self) {
        let now = epoch_milliseconds();
        *self = Self {
            status: RuntimePreparationStatus::Preparing,
            activities: runtime_preparation_activities(),
            started_at_epoch_ms: Some(now),
            updated_at_epoch_ms: Some(now),
            model_files_written: None,
            model_bytes_written: None,
        };
    }

    pub(crate) fn activate(&mut self, step: RuntimePreparationStep) {
        let now = epoch_milliseconds();
        for activity in &mut self.activities {
            if activity.status == RuntimePreparationActivityStatus::Active {
                activity.status = RuntimePreparationActivityStatus::Complete;
                activity.finished_at_epoch_ms = Some(now);
            }
            if activity.step == step {
                activity.status = RuntimePreparationActivityStatus::Active;
                activity.started_at_epoch_ms.get_or_insert(now);
                activity.finished_at_epoch_ms = None;
            }
        }
        self.updated_at_epoch_ms = Some(now);
    }

    pub(crate) fn finish(&mut self, succeeded: bool) {
        let now = epoch_milliseconds();
        for activity in &mut self.activities {
            if activity.status == RuntimePreparationActivityStatus::Active {
                activity.status = if succeeded {
                    RuntimePreparationActivityStatus::Complete
                } else {
                    RuntimePreparationActivityStatus::Failed
                };
                activity.finished_at_epoch_ms = Some(now);
            }
        }
        self.status = if succeeded {
            RuntimePreparationStatus::Ready
        } else {
            RuntimePreparationStatus::Failed
        };
        self.updated_at_epoch_ms = Some(now);
    }

    pub(crate) fn observe_model_files(mut self, application_home: &Path) -> Self {
        if !self.activities.iter().any(|activity| {
            activity.step == RuntimePreparationStep::PrepareDocumentModels
                && activity.status == RuntimePreparationActivityStatus::Active
        }) {
            return self;
        }
        let staged_models = application_home
            .join("models")
            .join(".docling-models-preparation");
        let mut file_count = 0_u64;
        let mut byte_count = 0_u64;
        for entry in WalkDir::new(staged_models)
            .follow_links(false)
            .into_iter()
            .filter_map(Result::ok)
        {
            if entry.file_type().is_file() {
                file_count = file_count.saturating_add(1);
                byte_count = byte_count
                    .saturating_add(entry.metadata().map_or(0, |metadata| metadata.len()));
            }
        }
        self.model_files_written = Some(file_count);
        self.model_bytes_written = Some(byte_count);
        self
    }
}

fn runtime_preparation_activities() -> Vec<RuntimePreparationActivity> {
    [
        (
            RuntimePreparationStep::ValidateResources,
            "Validate bundled resources",
            "Checking the signed runtime manifest and required local files.",
        ),
        (
            RuntimePreparationStep::VerifyBundledTools,
            "Verify bundled AI tools",
            "Checking the exact Codex and uv executable versions.",
        ),
        (
            RuntimePreparationStep::ResetEnvironment,
            "Prepare managed directories",
            "Creating a clean, Quantix-managed Python and Docling environment.",
        ),
        (
            RuntimePreparationStep::SynchronizeEnvironment,
            "Install Python and Docling",
            "Synchronizing the locked Python environment and Docling dependencies.",
        ),
        (
            RuntimePreparationStep::VerifyDocling,
            "Verify Docling",
            "Checking the installed Docling executable and exact version.",
        ),
        (
            RuntimePreparationStep::PrepareDocumentModels,
            "Prepare document models",
            "Downloading and validating the local models used to read Tender documents.",
        ),
        (
            RuntimePreparationStep::RunDocumentCheck,
            "Run document self-check",
            "Converting a bundled sample document entirely with the local runtime.",
        ),
        (
            RuntimePreparationStep::PublishRuntime,
            "Publish verified runtime",
            "Validating model files and atomically publishing the runtime manifest.",
        ),
        (
            RuntimePreparationStep::FinalRuntimeCheck,
            "Complete readiness check",
            "Rechecking installed tools, models, and the configured AI connection.",
        ),
    ]
    .into_iter()
    .map(|(step, title, detail)| RuntimePreparationActivity {
        step,
        title: title.to_owned(),
        detail: detail.to_owned(),
        status: RuntimePreparationActivityStatus::Pending,
        started_at_epoch_ms: None,
        finished_at_epoch_ms: None,
    })
    .collect()
}

fn epoch_milliseconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

impl RuntimeReadiness {
    fn state(state: RuntimeReadinessState, issue: RuntimeReadinessIssue) -> Self {
        Self {
            state,
            issues: vec![issue],
            codex_version: None,
            uv_version: None,
            docling_version: None,
            repair_available: false,
        }
    }

    fn with_versions(
        state: RuntimeReadinessState,
        issues: Vec<RuntimeReadinessIssue>,
        versions: &RuntimeVersions,
        repair_available: bool,
    ) -> Self {
        Self {
            state,
            issues,
            codex_version: versions.codex.clone(),
            uv_version: versions.uv.clone(),
            docling_version: versions.docling.clone(),
            repair_available,
        }
    }
}

#[derive(Default)]
struct RuntimeVersions {
    codex: Option<String>,
    uv: Option<String>,
    docling: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PersistedPreparationState {
    NotStarted,
    Preparing,
    Ready,
    Failed,
}

struct PersistedPreparation {
    state: PersistedPreparationState,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DoclingRuntimeManifest {
    schema_version: u32,
    codex_version: String,
    uv_version: String,
    docling_version: String,
    python_version: String,
    model_profile: Vec<String>,
    project_files: Vec<HashedFile>,
    environment: TreeFingerprint,
    managed_python: TreeFingerprint,
    model_files: Vec<HashedFile>,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HashedFile {
    path: String,
    size_bytes: u64,
    sha256: String,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TreeFingerprint {
    file_count: u64,
    size_bytes: u64,
    sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeProvenanceManifest {
    schema_version: u32,
    platform: String,
    architecture: String,
    codex: RuntimeArtifact,
    uv: RuntimeArtifact,
    codex_schema_sha256: String,
    docling_project_files: Vec<HashedFile>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeArtifact {
    version: String,
    sha256: String,
}

enum DoclingValidationError {
    Environment,
    Models,
}

#[derive(Debug, thiserror::Error)]
enum RuntimeError {
    #[error("runtime resources are unavailable")]
    MissingResource,
    #[error("runtime process failed")]
    ProcessFailed,
    #[error("runtime process was cancelled")]
    Cancelled,
    #[error("runtime output is invalid")]
    InvalidOutput,
    #[error("runtime persistence failed")]
    PersistenceFailed,
}

struct RuntimePreparationGuard {
    host: QuantixHost,
    cancellation: CancellationToken,
}

impl RuntimePreparationGuard {
    fn new(host: QuantixHost, cancellation: CancellationToken) -> Self {
        Self { host, cancellation }
    }

    fn cancellation(&self) -> CancellationToken {
        self.cancellation.clone()
    }
}

impl Drop for RuntimePreparationGuard {
    fn drop(&mut self) {
        self.host.finish_runtime_preparation();
    }
}

impl QuantixHost {
    #[doc(hidden)]
    pub async fn verify_offline_runtime_for_acceptance(&self) -> bool {
        self.set_runtime_verified(false);
        if !matches!(
            self.ensure_setup().state,
            SetupState::Ready | SetupState::Warning
        ) {
            return false;
        }
        let layout = self.runtime_layout();
        let codex = layout.codex_executable();
        let uv = layout.uv_executable();
        if !is_real_file(&codex)
            || !is_real_file(&uv)
            || validate_runtime_provenance(layout).is_err()
            || validate_docling_runtime(self.application_home(), layout).is_err()
        {
            return false;
        }
        let cancellation = CancellationToken::new();
        let codex_version = probe_version(
            self.process_supervisor(),
            self.application_home(),
            &codex,
            cancellation.clone(),
        )
        .await;
        let uv_version = probe_version(
            self.process_supervisor(),
            self.application_home(),
            &uv,
            cancellation,
        )
        .await;
        let verified = codex_version
            .as_deref()
            .is_ok_and(|version| version == CODEX_VERSION)
            && uv_version
                .as_deref()
                .is_ok_and(|version| version == UV_VERSION)
            && is_real_file(&docling_executable(self.application_home()));
        self.set_runtime_verified(verified);
        verified
    }

    pub async fn inspect_runtime_readiness(&self) -> RuntimeReadiness {
        if self.update_installation_is_active() {
            return RuntimeReadiness::state(
                RuntimeReadinessState::Preparing,
                RuntimeReadinessIssue::RuntimePreparationActive,
            );
        }
        let Ok(_ordinary_work) = self.begin_ordinary_work() else {
            return RuntimeReadiness::state(
                RuntimeReadinessState::Preparing,
                RuntimeReadinessIssue::RuntimePreparationActive,
            );
        };
        // Public inspection must revalidate the managed runtime every time so
        // filesystem drift and hostile links cannot be hidden by a prior Ready
        // result. The fixture-only verified shortcut belongs solely to the
        // post-update validation seam below.
        self.set_runtime_verified(false);
        if !matches!(
            self.ensure_setup().state,
            SetupState::Ready | SetupState::Warning
        ) {
            return RuntimeReadiness::state(
                RuntimeReadinessState::RepairRequired,
                RuntimeReadinessIssue::SetupIncomplete,
            );
        }
        self.inspect_runtime_readiness_for_update().await
    }

    pub(crate) async fn inspect_runtime_readiness_for_update(&self) -> RuntimeReadiness {
        #[cfg(feature = "runtime-fixture")]
        if self.runtime_is_verified() {
            return RuntimeReadiness {
                state: RuntimeReadinessState::Ready,
                issues: Vec::new(),
                codex_version: Some(CODEX_VERSION.to_owned()),
                uv_version: Some(UV_VERSION.to_owned()),
                docling_version: Some(DOCLING_VERSION.to_owned()),
                repair_available: false,
            };
        }
        self.set_runtime_verified(false);
        if !self
            .application_home()
            .join("installation.sqlite")
            .is_file()
        {
            return RuntimeReadiness::state(
                RuntimeReadinessState::RepairRequired,
                RuntimeReadinessIssue::SetupIncomplete,
            );
        }
        if self.runtime_preparation_is_active() {
            return RuntimeReadiness::state(
                RuntimeReadinessState::Preparing,
                RuntimeReadinessIssue::RuntimePreparationActive,
            );
        }
        let persisted = match read_preparation(self.application_home()) {
            Ok(persisted) => persisted,
            Err(_) => {
                return RuntimeReadiness::state(
                    RuntimeReadinessState::RepairRequired,
                    RuntimeReadinessIssue::RuntimeProbeFailed,
                )
            }
        };
        if persisted.state == PersistedPreparationState::Preparing {
            let mut readiness = RuntimeReadiness::state(
                RuntimeReadinessState::InterruptedPreparation,
                RuntimeReadinessIssue::RuntimePreparationInterrupted,
            );
            readiness.repair_available = true;
            return readiness;
        }
        if persisted.state == PersistedPreparationState::Failed {
            let mut readiness = RuntimeReadiness::state(
                RuntimeReadinessState::RepairRequired,
                RuntimeReadinessIssue::RuntimePreparationFailed,
            );
            readiness.repair_available = true;
            return readiness;
        }

        let readiness = self.probe_runtime(CancellationToken::new()).await;
        self.set_runtime_verified(readiness.state == RuntimeReadinessState::Ready);
        readiness
    }

    pub async fn repair_runtime_readiness(&self) -> RuntimeReadiness {
        self.set_runtime_verified(false);
        if !matches!(
            self.ensure_setup().state,
            SetupState::Ready | SetupState::Warning
        ) {
            return RuntimeReadiness::state(
                RuntimeReadinessState::RepairRequired,
                RuntimeReadinessIssue::SetupIncomplete,
            );
        }
        let cancellation = match self.begin_runtime_preparation() {
            Some(cancellation) => cancellation,
            None => {
                return RuntimeReadiness::state(
                    RuntimeReadinessState::Preparing,
                    RuntimeReadinessIssue::RuntimePreparationActive,
                )
            }
        };
        let preparation = RuntimePreparationGuard::new(self.clone(), cancellation);
        self.begin_runtime_preparation_progress();
        if write_preparation(
            self.application_home(),
            PersistedPreparationState::Preparing,
            &RuntimeVersions::default(),
        )
        .is_err()
        {
            self.finish_runtime_preparation_progress(false);
            return RuntimeReadiness::state(
                RuntimeReadinessState::RepairRequired,
                RuntimeReadinessIssue::RuntimePreparationFailed,
            );
        }

        let cancellation = preparation.cancellation();
        let prepared = self.prepare_docling(cancellation.clone()).await;
        match prepared {
            Ok(versions) => {
                if cancellation.is_cancelled() {
                    return self.fail_runtime_preparation();
                }
                self.activate_runtime_preparation_step(RuntimePreparationStep::FinalRuntimeCheck);
                let readiness = self.probe_runtime(cancellation.clone()).await;
                if cancellation.is_cancelled() {
                    return self.fail_runtime_preparation();
                }
                let preparation_state = if matches!(
                    readiness.state,
                    RuntimeReadinessState::Ready | RuntimeReadinessState::AuthenticationRequired
                ) {
                    PersistedPreparationState::Ready
                } else {
                    PersistedPreparationState::Failed
                };
                if write_preparation(self.application_home(), preparation_state, &versions).is_err()
                    || cancellation.is_cancelled()
                {
                    return self.fail_runtime_preparation();
                }
                self.set_runtime_verified(readiness.state == RuntimeReadinessState::Ready);
                self.finish_runtime_preparation_progress(matches!(
                    readiness.state,
                    RuntimeReadinessState::Ready | RuntimeReadinessState::AuthenticationRequired
                ));
                readiness
            }
            Err(_) => self.fail_runtime_preparation(),
        }
    }

    pub fn cancel_runtime_preparation(&self) -> bool {
        self.cancel_active_runtime_preparation()
    }

    fn fail_runtime_preparation(&self) -> RuntimeReadiness {
        self.set_runtime_verified(false);
        self.finish_runtime_preparation_progress(false);
        let _ = write_preparation(
            self.application_home(),
            PersistedPreparationState::Failed,
            &RuntimeVersions::default(),
        );
        let mut readiness = RuntimeReadiness::state(
            RuntimeReadinessState::RepairRequired,
            RuntimeReadinessIssue::RuntimePreparationFailed,
        );
        readiness.repair_available = true;
        readiness
    }

    async fn probe_runtime(&self, cancellation: CancellationToken) -> RuntimeReadiness {
        let layout = self.runtime_layout();
        let codex = layout.codex_executable();
        let uv = layout.uv_executable();
        let mut missing = Vec::new();
        if !is_real_file(&codex) {
            missing.push(RuntimeReadinessIssue::CodexExecutableMissing);
        }
        if !is_real_file(&uv) {
            missing.push(RuntimeReadinessIssue::UvExecutableMissing);
        }
        if !missing.is_empty() {
            return RuntimeReadiness::with_versions(
                RuntimeReadinessState::MissingExecutable,
                missing,
                &RuntimeVersions::default(),
                false,
            );
        }
        if validate_runtime_provenance(layout).is_err() {
            return RuntimeReadiness::with_versions(
                RuntimeReadinessState::RepairRequired,
                vec![RuntimeReadinessIssue::RuntimeResourceIntegrityFailed],
                &RuntimeVersions::default(),
                false,
            );
        }

        let mut versions = RuntimeVersions {
            codex: probe_version(
                self.process_supervisor(),
                self.application_home(),
                &codex,
                cancellation.clone(),
            )
            .await
            .ok(),
            uv: probe_version(
                self.process_supervisor(),
                self.application_home(),
                &uv,
                cancellation.clone(),
            )
            .await
            .ok(),
            docling: None,
        };
        let mut incompatible = Vec::new();
        if versions.codex.as_deref() != Some(CODEX_VERSION) {
            incompatible.push(RuntimeReadinessIssue::CodexVersionIncompatible);
        }
        if versions.uv.as_deref() != Some(UV_VERSION) {
            incompatible.push(RuntimeReadinessIssue::UvVersionIncompatible);
        }
        if !incompatible.is_empty() {
            return RuntimeReadiness::with_versions(
                RuntimeReadinessState::IncompatibleVersion,
                incompatible,
                &versions,
                false,
            );
        }

        let docling = docling_executable(self.application_home());
        if !is_real_file(&docling) {
            return RuntimeReadiness::with_versions(
                RuntimeReadinessState::MissingExecutable,
                vec![RuntimeReadinessIssue::DoclingExecutableMissing],
                &versions,
                true,
            );
        }
        match validate_docling_runtime(self.application_home(), layout) {
            Ok(()) => {}
            Err(DoclingValidationError::Environment) => {
                return RuntimeReadiness::with_versions(
                    RuntimeReadinessState::RepairRequired,
                    vec![RuntimeReadinessIssue::DoclingEnvironmentInvalid],
                    &versions,
                    true,
                )
            }
            Err(DoclingValidationError::Models) => {
                return RuntimeReadiness::with_versions(
                    RuntimeReadinessState::MissingModel,
                    vec![RuntimeReadinessIssue::DoclingModelsMissing],
                    &versions,
                    true,
                )
            }
        }
        if run_checked(
            self.process_supervisor(),
            docling_sync_spec(layout, self.application_home(), &uv, true),
            cancellation.clone(),
        )
        .await
        .is_err()
        {
            return RuntimeReadiness::with_versions(
                RuntimeReadinessState::RepairRequired,
                vec![RuntimeReadinessIssue::DoclingEnvironmentInvalid],
                &versions,
                true,
            );
        }
        versions.docling = probe_docling_version(
            self.process_supervisor(),
            self.application_home(),
            &docling,
            cancellation.clone(),
        )
        .await
        .ok();
        if versions.docling.as_deref() != Some(DOCLING_VERSION) {
            return RuntimeReadiness::with_versions(
                RuntimeReadinessState::IncompatibleVersion,
                vec![RuntimeReadinessIssue::DoclingVersionIncompatible],
                &versions,
                true,
            );
        }
        match self.inspect_codex_subscription(cancellation).await {
            CodexReadiness::Ready => RuntimeReadiness::with_versions(
                RuntimeReadinessState::Ready,
                Vec::new(),
                &versions,
                false,
            ),
            CodexReadiness::AuthenticationRequired => RuntimeReadiness::with_versions(
                RuntimeReadinessState::AuthenticationRequired,
                vec![RuntimeReadinessIssue::CodexAuthenticationRequired],
                &versions,
                false,
            ),
            CodexReadiness::SubscriptionRequired => RuntimeReadiness::with_versions(
                RuntimeReadinessState::AuthenticationRequired,
                vec![RuntimeReadinessIssue::CodexSubscriptionRequired],
                &versions,
                false,
            ),
            CodexReadiness::Unavailable => RuntimeReadiness::with_versions(
                RuntimeReadinessState::RepairRequired,
                vec![RuntimeReadinessIssue::RuntimeProbeFailed],
                &versions,
                true,
            ),
        }
    }

    async fn prepare_docling(
        &self,
        cancellation: CancellationToken,
    ) -> Result<RuntimeVersions, RuntimeError> {
        self.activate_runtime_preparation_step(RuntimePreparationStep::ValidateResources);
        let layout = self.runtime_layout();
        let codex = layout.codex_executable();
        let uv = layout.uv_executable();
        for resource in [
            &codex,
            &uv,
            &layout.docling_project().join("pyproject.toml"),
            &layout.docling_project().join("uv.lock"),
            &layout.docling_project().join(".python-version"),
            &layout.docling_project().join("python-downloads.json"),
            &layout.docling_project().join("convert_document.py"),
            &layout.readiness_document(),
        ] {
            if !is_real_file(resource) {
                return Err(RuntimeError::MissingResource);
            }
        }
        validate_runtime_provenance(layout)?;
        self.activate_runtime_preparation_step(RuntimePreparationStep::VerifyBundledTools);
        let codex_version = probe_version(
            self.process_supervisor(),
            self.application_home(),
            &codex,
            cancellation.clone(),
        )
        .await?;
        if codex_version != CODEX_VERSION {
            return Err(RuntimeError::InvalidOutput);
        }
        let uv_version = probe_version(
            self.process_supervisor(),
            self.application_home(),
            &uv,
            cancellation.clone(),
        )
        .await?;
        if uv_version != UV_VERSION {
            return Err(RuntimeError::InvalidOutput);
        }

        let runtime_root = self.application_home().join("runtimes");
        let environment = runtime_root.join("docling");
        let python = runtime_root.join("python");
        let uv_cache = runtime_root.join("uv-cache");
        let models_root = self.application_home().join("models");
        let models = models_root.join("docling");
        for directory in [&runtime_root, &uv_cache, &models_root] {
            fs::create_dir_all(directory).map_err(|_| RuntimeError::PersistenceFailed)?;
        }
        self.activate_runtime_preparation_step(RuntimePreparationStep::ResetEnvironment);
        runtime_fixture_trace("resetting managed Docling environment");
        reset_managed_directory(&environment, self.application_home())?;
        runtime_fixture_trace("resetting managed Python environment");
        reset_managed_directory(&python, self.application_home())?;
        runtime_fixture_trace("managed directories reset");

        self.activate_runtime_preparation_step(RuntimePreparationStep::SynchronizeEnvironment);
        run_checked(
            self.process_supervisor(),
            docling_sync_spec(layout, self.application_home(), &uv, false),
            cancellation.clone(),
        )
        .await?;
        runtime_fixture_trace("locked environment synchronized");

        let docling = docling_executable(self.application_home());
        let python = python_executable(self.application_home());
        if !is_real_file(&docling) || !is_real_file(&python) {
            return Err(RuntimeError::MissingResource);
        }
        self.activate_runtime_preparation_step(RuntimePreparationStep::VerifyDocling);
        let docling_version = probe_docling_version(
            self.process_supervisor(),
            self.application_home(),
            &docling,
            cancellation.clone(),
        )
        .await?;
        if docling_version != DOCLING_VERSION {
            return Err(RuntimeError::InvalidOutput);
        }
        runtime_fixture_trace("Docling version verified");
        let staged_models = prepare_model_staging(&models_root, &layout.docling_project())?;
        self.activate_runtime_preparation_step(RuntimePreparationStep::PrepareDocumentModels);
        run_checked(
            self.process_supervisor(),
            ProcessSpec {
                executable: python.clone(),
                arguments: vec![
                    layout
                        .docling_project()
                        .join("prepare_models.py")
                        .into_os_string(),
                    OsString::from("--output-dir"),
                    staged_models.as_os_str().to_owned(),
                ],
                current_directory: Some(runtime_root.clone()),
                environment: docling_environment(self.application_home()),
                inherit_environment: false,
                stdin: Vec::new(),
                timeout: MODEL_TIMEOUT,
                stdout_limit: PREPARATION_OUTPUT_LIMIT,
                stderr_limit: PREPARATION_OUTPUT_LIMIT,
            },
            cancellation.clone(),
        )
        .await?;
        runtime_fixture_trace("models prepared");

        self.activate_runtime_preparation_step(RuntimePreparationStep::RunDocumentCheck);
        let smoke = tempfile::Builder::new()
            .prefix("runtime-readiness-")
            .tempdir_in(self.application_home().join("staging"))
            .map_err(|_| RuntimeError::PersistenceFailed)?;
        run_checked(
            self.process_supervisor(),
            ProcessSpec {
                executable: python,
                arguments: [
                    layout
                        .docling_project()
                        .join("convert_document.py")
                        .into_os_string(),
                    OsString::from("--input"),
                    layout.readiness_document().into_os_string(),
                    OsString::from("--input-format"),
                    OsString::from("pdf"),
                    OsString::from("--output-dir"),
                    smoke.path().as_os_str().to_owned(),
                    OsString::from("--artifacts-path"),
                    staged_models.as_os_str().to_owned(),
                    OsString::from("--ocr-mode"),
                    OsString::from("full_page"),
                    OsString::from("--ocr-lang"),
                    OsString::from("ch"),
                    OsString::from("--document-timeout"),
                    OsString::from("120"),
                    OsString::from("--num-threads"),
                    OsString::from("2"),
                ]
                .into_iter()
                .collect(),
                current_directory: Some(runtime_root),
                environment: docling_environment(self.application_home())
                    .into_iter()
                    .chain([
                        (OsString::from("HF_HUB_OFFLINE"), OsString::from("1")),
                        (OsString::from("TRANSFORMERS_OFFLINE"), OsString::from("1")),
                    ])
                    .collect(),
                inherit_environment: false,
                stdin: Vec::new(),
                timeout: SMOKE_TIMEOUT,
                stdout_limit: PREPARATION_OUTPUT_LIMIT,
                stderr_limit: PREPARATION_OUTPUT_LIMIT,
            },
            cancellation,
        )
        .await?;
        validate_smoke_output(smoke.path())?;
        runtime_fixture_trace("smoke output verified");

        self.activate_runtime_preparation_step(RuntimePreparationStep::PublishRuntime);
        let manifest = build_model_manifest(
            &staged_models,
            &environment,
            &layout.docling_project(),
            &codex_version,
            &uv_version,
            &docling_version,
        )?;
        runtime_fixture_trace("runtime manifest built");
        promote_model_directory(&models, &staged_models)?;
        remove_model_staging_marker(&models_root)?;
        publish_model_manifest(self.application_home(), &manifest)?;
        runtime_fixture_trace("runtime readiness published");
        Ok(RuntimeVersions {
            codex: Some(codex_version),
            uv: Some(uv_version),
            docling: Some(docling_version),
        })
    }
}

#[cfg(feature = "runtime-fixture")]
fn runtime_fixture_trace(message: &str) {
    eprintln!("runtime fixture: {message}");
}

#[cfg(not(feature = "runtime-fixture"))]
fn runtime_fixture_trace(_message: &str) {}

fn executable_name(name: &str) -> String {
    if std::env::consts::EXE_EXTENSION.is_empty() {
        name.to_owned()
    } else {
        format!("{name}.{}", std::env::consts::EXE_EXTENSION)
    }
}

pub(crate) fn docling_executable(application_home: &Path) -> PathBuf {
    application_home
        .join("runtimes")
        .join("docling")
        .join(if cfg!(windows) { "Scripts" } else { "bin" })
        .join(executable_name("docling"))
}

pub(crate) fn python_executable(application_home: &Path) -> PathBuf {
    application_home
        .join("runtimes")
        .join("docling")
        .join(if cfg!(windows) { "Scripts" } else { "bin" })
        .join(executable_name("python"))
}

pub(crate) fn docling_environment(application_home: &Path) -> Vec<(OsString, OsString)> {
    let cache = application_home.join("runtimes").join("huggingface-cache");
    controlled_environment(application_home)
        .into_iter()
        .chain([
            (OsString::from("HF_HOME"), cache.clone().into_os_string()),
            (OsString::from("HF_HUB_CACHE"), cache.into_os_string()),
            (
                OsString::from("HF_HUB_DISABLE_TELEMETRY"),
                OsString::from("1"),
            ),
            (OsString::from("PYTHONNOUSERSITE"), OsString::from("1")),
        ])
        .collect()
}

fn controlled_environment(application_home: &Path) -> Vec<(OsString, OsString)> {
    let staging = application_home.join("staging");
    let mut environment = vec![
        (
            OsString::from("HOME"),
            application_home.as_os_str().to_owned(),
        ),
        (
            OsString::from("USERPROFILE"),
            application_home.as_os_str().to_owned(),
        ),
        (
            OsString::from("XDG_CACHE_HOME"),
            application_home
                .join("runtimes")
                .join("cache")
                .into_os_string(),
        ),
        (OsString::from("TEMP"), staging.clone().into_os_string()),
        (OsString::from("TMP"), staging.clone().into_os_string()),
        (OsString::from("TMPDIR"), staging.into_os_string()),
    ];
    for name in ["SystemRoot", "WINDIR"] {
        if let Some(value) = std::env::var_os(name) {
            environment.push((OsString::from(name), value));
        }
    }
    environment
}

fn os_arguments(arguments: &[&str]) -> Vec<OsString> {
    arguments.iter().map(OsString::from).collect()
}

fn docling_sync_spec(
    layout: &RuntimeLayout,
    application_home: &Path,
    uv: &Path,
    check_only: bool,
) -> ProcessSpec {
    let runtime_root = application_home.join("runtimes");
    let mut arguments = os_arguments(&[
        "sync",
        "--locked",
        "--no-dev",
        "--managed-python",
        "--python",
        PYTHON_VERSION,
        "--project",
    ]);
    arguments.extend([
        layout.docling_project().into_os_string(),
        OsString::from("--no-config"),
    ]);
    if check_only {
        arguments.extend(os_arguments(&["--check", "--offline"]));
    }
    let environment = controlled_environment(application_home)
        .into_iter()
        .chain([
            (
                OsString::from("UV_PROJECT_ENVIRONMENT"),
                runtime_root.join("docling").into_os_string(),
            ),
            (
                OsString::from("UV_PYTHON_INSTALL_DIR"),
                runtime_root.join("python").into_os_string(),
            ),
            (
                OsString::from("UV_CACHE_DIR"),
                runtime_root.join("uv-cache").into_os_string(),
            ),
            (OsString::from("UV_NO_CONFIG"), OsString::from("1")),
            (
                OsString::from("UV_PYTHON_DOWNLOADS_JSON_URL"),
                layout
                    .docling_project()
                    .join("python-downloads.json")
                    .into_os_string(),
            ),
        ])
        .collect();
    ProcessSpec {
        executable: uv.to_path_buf(),
        arguments,
        current_directory: Some(layout.docling_project()),
        environment,
        inherit_environment: false,
        stdin: Vec::new(),
        timeout: if check_only {
            ENVIRONMENT_CHECK_TIMEOUT
        } else {
            PREPARATION_TIMEOUT
        },
        stdout_limit: PREPARATION_OUTPUT_LIMIT,
        stderr_limit: PREPARATION_OUTPUT_LIMIT,
    }
}

fn is_real_file(path: &Path) -> bool {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return false;
    };
    if !metadata.is_file() || has_unsafe_link(&metadata) {
        return false;
    }
    true
}

fn has_unsafe_link(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return true;
        }
    }
    false
}

fn reset_managed_directory(path: &Path, allowed_root: &Path) -> Result<(), RuntimeError> {
    let relative = path
        .strip_prefix(allowed_root)
        .map_err(|_| RuntimeError::PersistenceFailed)?;
    if relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(RuntimeError::PersistenceFailed);
    }

    let allowed_metadata =
        fs::symlink_metadata(allowed_root).map_err(|_| RuntimeError::PersistenceFailed)?;
    if !allowed_metadata.is_dir() || has_unsafe_link(&allowed_metadata) {
        return Err(RuntimeError::PersistenceFailed);
    }
    let canonical_allowed = allowed_root
        .canonicalize()
        .map_err(|_| RuntimeError::PersistenceFailed)?;
    let mut current = allowed_root.to_path_buf();
    for component in relative
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .components()
    {
        let Component::Normal(name) = component else {
            return Err(RuntimeError::PersistenceFailed);
        };
        current.push(name);
        match fs::symlink_metadata(&current) {
            Ok(metadata) => {
                if has_unsafe_link(&metadata)
                    || !current
                        .canonicalize()
                        .map_err(|_| RuntimeError::PersistenceFailed)?
                        .starts_with(&canonical_allowed)
                {
                    return Err(RuntimeError::PersistenceFailed);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(_) => return Err(RuntimeError::PersistenceFailed),
        }
    }

    remove_managed_tree(path)?;
    fs::create_dir_all(path).map_err(|_| RuntimeError::PersistenceFailed)?;
    if !path
        .canonicalize()
        .map_err(|_| RuntimeError::PersistenceFailed)?
        .starts_with(canonical_allowed)
    {
        return Err(RuntimeError::PersistenceFailed);
    }
    Ok(())
}

fn remove_managed_tree(path: &Path) -> Result<(), RuntimeError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(RuntimeError::PersistenceFailed),
    };
    if remove_managed_link(path, &metadata)? {
        return Ok(());
    }
    if metadata.is_dir() {
        let entries = fs::read_dir(path)
            .map_err(|_| RuntimeError::PersistenceFailed)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| RuntimeError::PersistenceFailed)?;
        let mut links = Vec::new();
        let mut ordinary = Vec::new();
        for entry in entries {
            let entry_path = entry.path();
            let metadata =
                fs::symlink_metadata(&entry_path).map_err(|_| RuntimeError::PersistenceFailed)?;
            if has_unsafe_link(&metadata) {
                links.push(entry_path);
            } else {
                ordinary.push(entry_path);
            }
        }
        for entry in links.into_iter().chain(ordinary) {
            remove_managed_tree(&entry)?;
        }
        fs::remove_dir(path).map_err(|_| RuntimeError::PersistenceFailed)
    } else if metadata.is_file() {
        fs::remove_file(path).map_err(|_| RuntimeError::PersistenceFailed)
    } else {
        Err(RuntimeError::PersistenceFailed)
    }
}

fn remove_managed_link(path: &Path, metadata: &fs::Metadata) -> Result<bool, RuntimeError> {
    if !has_unsafe_link(metadata) {
        return Ok(false);
    }
    #[cfg(windows)]
    {
        let junction_target = junction::get_target(path);
        if junction_target.is_ok() {
            junction::delete(path).map_err(|_| RuntimeError::PersistenceFailed)?;
            fs::remove_dir(path).map_err(|_| RuntimeError::PersistenceFailed)?;
        } else if metadata.file_type().is_symlink() {
            if fs::metadata(path).is_ok_and(|target| target.is_dir()) {
                fs::remove_dir(path).map_err(|_| RuntimeError::PersistenceFailed)?;
            } else {
                fs::remove_file(path).map_err(|_| RuntimeError::PersistenceFailed)?;
            }
        } else if metadata.is_file() {
            fs::remove_file(path).map_err(|_| RuntimeError::PersistenceFailed)?;
        } else {
            return Err(RuntimeError::PersistenceFailed);
        }
    }
    #[cfg(not(windows))]
    fs::remove_file(path).map_err(|_| RuntimeError::PersistenceFailed)?;
    Ok(true)
}

async fn probe_version(
    supervisor: &ProcessSupervisor,
    application_home: &Path,
    executable: &Path,
    cancellation: CancellationToken,
) -> Result<String, RuntimeError> {
    probe_version_with_timeout(
        supervisor,
        application_home,
        executable,
        cancellation,
        VERSION_TIMEOUT,
    )
    .await
}

async fn probe_docling_version(
    supervisor: &ProcessSupervisor,
    application_home: &Path,
    executable: &Path,
    cancellation: CancellationToken,
) -> Result<String, RuntimeError> {
    probe_version_with_timeout(
        supervisor,
        application_home,
        executable,
        cancellation,
        ENVIRONMENT_CHECK_TIMEOUT,
    )
    .await
}

async fn probe_version_with_timeout(
    supervisor: &ProcessSupervisor,
    application_home: &Path,
    executable: &Path,
    cancellation: CancellationToken,
    timeout: Duration,
) -> Result<String, RuntimeError> {
    let output = run_checked(
        supervisor,
        ProcessSpec {
            executable: executable.to_path_buf(),
            arguments: os_arguments(&["--version"]),
            current_directory: executable.parent().map(Path::to_path_buf),
            environment: controlled_environment(application_home),
            inherit_environment: false,
            stdin: Vec::new(),
            timeout,
            stdout_limit: PROBE_OUTPUT_LIMIT,
            stderr_limit: PROBE_OUTPUT_LIMIT,
        },
        cancellation,
    )
    .await?;
    let text = std::str::from_utf8(&output.stdout).map_err(|_| RuntimeError::InvalidOutput)?;
    let version = text
        .split(|character: char| character.is_whitespace() || matches!(character, ',' | ';'))
        .map(|candidate| candidate.trim_start_matches('v'))
        .find_map(|candidate| Version::parse(candidate).ok())
        .ok_or(RuntimeError::InvalidOutput)?;
    Ok(version.to_string())
}

async fn run_checked(
    supervisor: &ProcessSupervisor,
    spec: ProcessSpec,
    cancellation: CancellationToken,
) -> Result<ProcessOutput, RuntimeError> {
    let output = supervisor
        .run(spec, cancellation)
        .await
        .map_err(|_| RuntimeError::ProcessFailed)?;
    match (output.termination, output.exit_code) {
        (ProcessTermination::Exited, Some(0)) => Ok(output),
        (ProcessTermination::Cancelled, _) => Err(RuntimeError::Cancelled),
        _ => Err(RuntimeError::ProcessFailed),
    }
}

fn build_model_manifest(
    models: &Path,
    environment: &Path,
    project: &Path,
    codex_version: &str,
    uv_version: &str,
    docling_version: &str,
) -> Result<DoclingRuntimeManifest, RuntimeError> {
    let model_files = collect_hashed_files(models)?;
    if model_files.is_empty() {
        return Err(RuntimeError::MissingResource);
    }
    let runtime_root = environment
        .parent()
        .ok_or(RuntimeError::PersistenceFailed)?;
    Ok(DoclingRuntimeManifest {
        schema_version: DOCLING_MANIFEST_SCHEMA,
        codex_version: codex_version.to_owned(),
        uv_version: uv_version.to_owned(),
        docling_version: docling_version.to_owned(),
        python_version: PYTHON_VERSION.to_owned(),
        model_profile: DOCLING_MODEL_PROFILE
            .iter()
            .map(ToString::to_string)
            .collect(),
        project_files: collect_hashed_files(project)?,
        environment: fingerprint_tree(environment, runtime_root)?,
        managed_python: fingerprint_tree(&runtime_root.join("python"), runtime_root)?,
        model_files,
    })
}

fn collect_hashed_files(root: &Path) -> Result<Vec<HashedFile>, RuntimeError> {
    let mut files = Vec::new();
    for entry in WalkDir::new(root).follow_links(false) {
        let entry = entry.map_err(|_| RuntimeError::PersistenceFailed)?;
        let metadata =
            fs::symlink_metadata(entry.path()).map_err(|_| RuntimeError::PersistenceFailed)?;
        if has_unsafe_link(&metadata) {
            return Err(RuntimeError::PersistenceFailed);
        }
        if metadata.is_file() {
            let relative = entry
                .path()
                .strip_prefix(root)
                .map_err(|_| RuntimeError::PersistenceFailed)?;
            let path = portable_relative_path(relative)?;
            files.push(HashedFile {
                path,
                size_bytes: metadata.len(),
                sha256: file_sha256(entry.path())?,
            });
        } else if !metadata.is_dir() {
            return Err(RuntimeError::PersistenceFailed);
        }
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(files)
}

fn fingerprint_tree(root: &Path, allowed_root: &Path) -> Result<TreeFingerprint, RuntimeError> {
    let mut records = Vec::new();
    let mut size_bytes = 0_u64;
    let root_metadata = fs::symlink_metadata(root).map_err(|_| RuntimeError::PersistenceFailed)?;
    if !root_metadata.is_dir() || has_unsafe_link(&root_metadata) {
        return Err(RuntimeError::PersistenceFailed);
    }
    let canonical_allowed = allowed_root
        .canonicalize()
        .map_err(|_| RuntimeError::PersistenceFailed)?;
    if !root
        .canonicalize()
        .map_err(|_| RuntimeError::PersistenceFailed)?
        .starts_with(&canonical_allowed)
    {
        return Err(RuntimeError::PersistenceFailed);
    }
    fingerprint_directory(
        root,
        root,
        &canonical_allowed,
        &mut records,
        &mut size_bytes,
    )?;
    records.sort_by(|left, right| left.0.cmp(&right.0));
    if records.is_empty() {
        return Err(RuntimeError::MissingResource);
    }
    let mut digest = Sha256::new();
    for (path, size, hash) in &records {
        update_length_prefixed(&mut digest, path.as_bytes());
        digest.update(size.to_le_bytes());
        update_length_prefixed(&mut digest, hash.as_bytes());
    }
    Ok(TreeFingerprint {
        file_count: records
            .len()
            .try_into()
            .map_err(|_| RuntimeError::PersistenceFailed)?,
        size_bytes,
        sha256: hex_digest(digest.finalize()),
    })
}

fn fingerprint_directory(
    root: &Path,
    directory: &Path,
    canonical_allowed: &Path,
    records: &mut Vec<(String, u64, String)>,
    size_bytes: &mut u64,
) -> Result<(), RuntimeError> {
    let mut entries = fs::read_dir(directory)
        .map_err(|_| RuntimeError::PersistenceFailed)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| RuntimeError::PersistenceFailed)?;
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(|_| RuntimeError::PersistenceFailed)?;
        let relative = portable_relative_path(
            path.strip_prefix(root)
                .map_err(|_| RuntimeError::PersistenceFailed)?,
        )?;
        if is_managed_link(&path, &metadata)? {
            let resolved = path
                .canonicalize()
                .map_err(|_| RuntimeError::PersistenceFailed)?;
            if !resolved.starts_with(canonical_allowed) {
                return Err(RuntimeError::PersistenceFailed);
            }
            let target = portable_relative_path(
                resolved
                    .strip_prefix(canonical_allowed)
                    .map_err(|_| RuntimeError::PersistenceFailed)?,
            )?;
            records.push((relative, 0, format!("link:{target}")));
        } else if has_unsafe_link(&metadata) {
            return Err(RuntimeError::PersistenceFailed);
        } else if metadata.is_file() {
            *size_bytes = size_bytes
                .checked_add(metadata.len())
                .ok_or(RuntimeError::PersistenceFailed)?;
            records.push((relative, metadata.len(), file_sha256(&path)?));
        } else if metadata.is_dir() {
            fingerprint_directory(root, &path, canonical_allowed, records, size_bytes)?;
        } else {
            return Err(RuntimeError::PersistenceFailed);
        }
    }
    Ok(())
}

fn is_managed_link(path: &Path, metadata: &fs::Metadata) -> Result<bool, RuntimeError> {
    if metadata.file_type().is_symlink() {
        return Ok(true);
    }
    #[cfg(windows)]
    if has_unsafe_link(metadata) {
        return Ok(junction::get_target(path).is_ok());
    }
    Ok(false)
}

fn update_length_prefixed(digest: &mut Sha256, bytes: &[u8]) {
    digest.update((bytes.len() as u64).to_le_bytes());
    digest.update(bytes);
}

fn file_sha256(path: &Path) -> Result<String, RuntimeError> {
    use std::io::Read;

    let mut file = fs::File::open(path).map_err(|_| RuntimeError::PersistenceFailed)?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| RuntimeError::PersistenceFailed)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(hex_digest(digest.finalize()))
}

fn hex_digest(bytes: impl AsRef<[u8]>) -> String {
    bytes
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn portable_relative_path(path: &Path) -> Result<String, RuntimeError> {
    let parts = path
        .components()
        .map(|component| match component {
            Component::Normal(part) => part
                .to_str()
                .map(str::to_owned)
                .ok_or(RuntimeError::PersistenceFailed),
            _ => Err(RuntimeError::PersistenceFailed),
        })
        .collect::<Result<Vec<_>, _>>()?;
    if parts.is_empty() {
        return Err(RuntimeError::PersistenceFailed);
    }
    Ok(parts.join("/"))
}

fn publish_model_manifest(
    application_home: &Path,
    manifest: &DoclingRuntimeManifest,
) -> Result<(), RuntimeError> {
    let runtime_root = application_home.join("runtimes");
    let staged = runtime_root.join("docling-readiness.json.staging");
    let published = runtime_root.join("docling-readiness.json");
    let bytes = serde_json::to_vec(manifest).map_err(|_| RuntimeError::PersistenceFailed)?;
    fs::write(&staged, bytes).map_err(|_| RuntimeError::PersistenceFailed)?;
    if published.exists() {
        fs::remove_file(&published).map_err(|_| RuntimeError::PersistenceFailed)?;
    }
    fs::rename(staged, published).map_err(|_| RuntimeError::PersistenceFailed)
}

fn promote_model_directory(models: &Path, staged: &Path) -> Result<(), RuntimeError> {
    let parent = models.parent().ok_or(RuntimeError::PersistenceFailed)?;
    if staged.parent() != Some(parent) {
        return Err(RuntimeError::PersistenceFailed);
    }
    let backup = parent.join(".docling-models-backup");
    if backup.exists() {
        fs::remove_dir_all(&backup).map_err(|_| RuntimeError::PersistenceFailed)?;
    }
    if models.exists() {
        fs::rename(models, &backup).map_err(|_| RuntimeError::PersistenceFailed)?;
    }
    if fs::rename(staged, models).is_err() {
        if backup.exists() {
            let _ = fs::rename(&backup, models);
        }
        return Err(RuntimeError::PersistenceFailed);
    }
    if backup.exists() {
        fs::remove_dir_all(backup).map_err(|_| RuntimeError::PersistenceFailed)?;
    }
    Ok(())
}

fn prepare_model_staging(
    models_root: &Path,
    docling_project: &Path,
) -> Result<PathBuf, RuntimeError> {
    let staged = models_root.join(".docling-models-preparation");
    let marker = models_root.join(".docling-models-preparation.sha256");
    let expected = file_sha256(&docling_project.join("approved-model-sources.json"))?;
    let can_resume = fs::read_to_string(&marker).is_ok_and(|value| value == expected)
        && fs::symlink_metadata(&staged)
            .is_ok_and(|metadata| metadata.is_dir() && !has_unsafe_link(&metadata));
    if !can_resume {
        reset_managed_directory(&staged, models_root)?;
        fs::write(&marker, &expected).map_err(|_| RuntimeError::PersistenceFailed)?;
    }
    if !staged
        .canonicalize()
        .map_err(|_| RuntimeError::PersistenceFailed)?
        .starts_with(
            models_root
                .canonicalize()
                .map_err(|_| RuntimeError::PersistenceFailed)?,
        )
    {
        return Err(RuntimeError::PersistenceFailed);
    }
    Ok(staged)
}

fn remove_model_staging_marker(models_root: &Path) -> Result<(), RuntimeError> {
    let marker = models_root.join(".docling-models-preparation.sha256");
    match fs::remove_file(marker) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(RuntimeError::PersistenceFailed),
    }
}

fn validate_runtime_provenance(layout: &RuntimeLayout) -> Result<(), RuntimeError> {
    let provenance = layout.provenance_manifest();
    let schema = layout.codex_schema();
    let codex = layout.codex_executable();
    let uv = layout.uv_executable();
    for resource in [&provenance, &schema, &codex, &uv] {
        if !is_real_file(resource) {
            return Err(RuntimeError::MissingResource);
        }
    }
    let bytes = fs::read(provenance).map_err(|_| RuntimeError::MissingResource)?;
    let manifest: RuntimeProvenanceManifest =
        serde_json::from_slice(&bytes).map_err(|_| RuntimeError::InvalidOutput)?;
    let codex_schema_hash = file_sha256(&schema)?;
    let embedded_schema_hash = {
        let mut digest = Sha256::new();
        digest.update(CODEX_PROTOCOL_SCHEMA.as_bytes());
        hex_digest(digest.finalize())
    };
    if manifest.schema_version != RUNTIME_PROVENANCE_SCHEMA
        || manifest.platform != std::env::consts::OS
        || manifest.architecture != std::env::consts::ARCH
        || manifest.codex.version != CODEX_VERSION
        || manifest.uv.version != UV_VERSION
        || manifest.codex.sha256.len() != 64
        || manifest.uv.sha256.len() != 64
        || manifest.codex_schema_sha256.len() != 64
        || manifest.docling_project_files.is_empty()
        || file_sha256(&layout.codex_executable())? != manifest.codex.sha256
        || file_sha256(&layout.uv_executable())? != manifest.uv.sha256
        || codex_schema_hash != manifest.codex_schema_sha256
        || embedded_schema_hash != manifest.codex_schema_sha256
        || collect_hashed_files(&layout.docling_project())? != manifest.docling_project_files
    {
        return Err(RuntimeError::InvalidOutput);
    }
    Ok(())
}

fn validate_docling_runtime(
    application_home: &Path,
    layout: &RuntimeLayout,
) -> Result<(), DoclingValidationError> {
    let bytes = fs::read(
        application_home
            .join("runtimes")
            .join("docling-readiness.json"),
    )
    .map_err(|_| DoclingValidationError::Environment)?;
    let manifest: DoclingRuntimeManifest =
        serde_json::from_slice(&bytes).map_err(|_| DoclingValidationError::Environment)?;
    if manifest.schema_version != DOCLING_MANIFEST_SCHEMA
        || manifest.codex_version != CODEX_VERSION
        || manifest.uv_version != UV_VERSION
        || manifest.docling_version != DOCLING_VERSION
        || manifest.python_version != PYTHON_VERSION
        || manifest.model_profile
            != DOCLING_MODEL_PROFILE
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        || manifest.project_files.is_empty()
        || manifest.model_files.is_empty()
    {
        return Err(DoclingValidationError::Environment);
    }
    let runtime_root = application_home.join("runtimes");
    if collect_hashed_files(&layout.docling_project())
        .map_err(|_| DoclingValidationError::Environment)?
        != manifest.project_files
        || fingerprint_tree(&runtime_root.join("docling"), &runtime_root)
            .map_err(|_| DoclingValidationError::Environment)?
            != manifest.environment
        || fingerprint_tree(&runtime_root.join("python"), &runtime_root)
            .map_err(|_| DoclingValidationError::Environment)?
            != manifest.managed_python
    {
        return Err(DoclingValidationError::Environment);
    }
    let models = application_home.join("models").join("docling");
    if manifest
        .model_files
        .iter()
        .any(|file| file.sha256.len() != 64)
        || collect_hashed_files(&models).map_err(|_| DoclingValidationError::Models)?
            != manifest.model_files
    {
        return Err(DoclingValidationError::Models);
    }
    Ok(())
}

fn validate_smoke_output(directory: &Path) -> Result<(), RuntimeError> {
    let json_file = WalkDir::new(directory)
        .max_depth(2)
        .into_iter()
        .filter_map(Result::ok)
        .find(|entry| {
            entry.file_type().is_file()
                && entry.path().extension().and_then(|value| value.to_str()) == Some("json")
        })
        .ok_or(RuntimeError::InvalidOutput)?;
    let metadata = json_file
        .metadata()
        .map_err(|_| RuntimeError::InvalidOutput)?;
    if metadata.len() == 0 || metadata.len() > MAX_SMOKE_JSON_BYTES {
        return Err(RuntimeError::InvalidOutput);
    }
    let value: serde_json::Value = serde_json::from_slice(
        &fs::read(json_file.path()).map_err(|_| RuntimeError::InvalidOutput)?,
    )
    .map_err(|_| RuntimeError::InvalidOutput)?;
    let has_ocr_text = value
        .get("texts")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|texts| {
            texts.iter().any(|text| {
                text.get("text")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|text| text.contains(SMOKE_OCR_TEXT))
            })
        });
    let has_pages = value
        .get("pages")
        .and_then(serde_json::Value::as_object)
        .is_some_and(|pages| !pages.is_empty());
    if value.get("schema_name").and_then(serde_json::Value::as_str) != Some("DoclingDocument")
        || value.get("name").and_then(serde_json::Value::as_str) != Some("readiness")
        || value
            .pointer("/origin/mimetype")
            .and_then(serde_json::Value::as_str)
            != Some("application/pdf")
        || !has_ocr_text
        || !has_pages
    {
        return Err(RuntimeError::InvalidOutput);
    }
    Ok(())
}

fn read_preparation(application_home: &Path) -> Result<PersistedPreparation, RuntimeError> {
    let connection = Connection::open(application_home.join("installation.sqlite"))
        .map_err(|_| RuntimeError::PersistenceFailed)?;
    let status: String = connection
        .query_row(
            "SELECT status FROM runtime_preparation WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
        .map_err(|_| RuntimeError::PersistenceFailed)?;
    let state = match status.as_str() {
        "not_started" => PersistedPreparationState::NotStarted,
        "preparing" => PersistedPreparationState::Preparing,
        "ready" => PersistedPreparationState::Ready,
        "failed" => PersistedPreparationState::Failed,
        _ => return Err(RuntimeError::PersistenceFailed),
    };
    Ok(PersistedPreparation { state })
}

fn write_preparation(
    application_home: &Path,
    state: PersistedPreparationState,
    versions: &RuntimeVersions,
) -> Result<(), RuntimeError> {
    let mut connection = Connection::open(application_home.join("installation.sqlite"))
        .map_err(|_| RuntimeError::PersistenceFailed)?;
    connection
        .busy_timeout(Duration::from_secs(5))
        .map_err(|_| RuntimeError::PersistenceFailed)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| RuntimeError::PersistenceFailed)?;
    let status = match state {
        PersistedPreparationState::NotStarted => "not_started",
        PersistedPreparationState::Preparing => "preparing",
        PersistedPreparationState::Ready => "ready",
        PersistedPreparationState::Failed => "failed",
    };
    transaction
        .execute(
            "UPDATE runtime_preparation SET
               status = ?1,
               codex_version = ?2,
               uv_version = ?3,
               docling_version = ?4,
               updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE singleton = 1",
            params![status, versions.codex, versions.uv, versions.docling],
        )
        .map_err(|_| RuntimeError::PersistenceFailed)?;
    transaction
        .commit()
        .map_err(|_| RuntimeError::PersistenceFailed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_output_requires_docling_schema_and_the_expected_ocr_text() {
        let valid = tempfile::tempdir().expect("smoke output directory");
        fs::write(
            valid.path().join("readiness.json"),
            serde_json::to_vec(&serde_json::json!({
                "schema_name": "DoclingDocument",
                "name": "readiness",
                "origin": { "mimetype": "application/pdf" },
                "texts": [{ "text": SMOKE_OCR_TEXT }],
                "pages": { "1": { "size": { "width": 1, "height": 1 } } },
            }))
            .expect("valid smoke JSON"),
        )
        .expect("write valid smoke JSON");
        assert!(validate_smoke_output(valid.path()).is_ok());

        let invalid = tempfile::tempdir().expect("invalid smoke output directory");
        fs::write(invalid.path().join("readiness.json"), b"{}").expect("write invalid smoke JSON");
        assert!(matches!(
            validate_smoke_output(invalid.path()),
            Err(RuntimeError::InvalidOutput)
        ));
    }
}
