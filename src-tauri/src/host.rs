use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex, OnceLock, Weak,
    },
    time::{SystemTime, UNIX_EPOCH},
};

use tokio::sync::{
    OwnedMutexGuard, OwnedRwLockReadGuard, OwnedRwLockWriteGuard, RwLock, Semaphore,
};
use tokio_util::sync::CancellationToken;

use crate::agent_runtime::{
    AgentProvider, ProviderRateLimit, ProviderRateLimitState, ProviderUsage,
};
use crate::diagnostics::{
    DiagnosticComponent, DiagnosticCorrelation, DiagnosticScope, DiagnosticSeverity,
    DiagnosticsStore, RecordDiagnosticFact,
};
use crate::process_supervisor::ProcessSupervisor;
use crate::runtime_readiness::{
    RuntimeLayout, RuntimePreparationProgress, RuntimePreparationStep, RuntimeReadiness,
};
use crate::setup::{ensure_application_home, SetupOutcome, SetupPlatform, SystemSetupPlatform};
use crate::tender_intake::{
    CancelPackageIntakeCommand, PackageIntakeControl, PackageIntakeOperationKind,
    PackageIntakeProgress, TenderPackageSourceKind,
};
use crate::tender_store::{
    OpenTenderStores, StartupReconciliationReport, TenderCommandError, TenderErrorCode, TenderId,
};

struct QuantixHostInner {
    application_home: PathBuf,
    setup_platform: Arc<dyn SetupPlatform>,
    setup_lock: Mutex<()>,
    startup_reconciled: Mutex<bool>,
    startup_reconciliation: Mutex<StartupReconciliationReport>,
    catalogue_lock: Mutex<()>,
    manager_tender_start_lock: Mutex<()>,
    open_tender_stores: OpenTenderStores,
    recovery_required_tenders: Mutex<HashSet<TenderId>>,
    recovery_operation_lock: Mutex<()>,
    runtime_layout: RuntimeLayout,
    process_supervisor: ProcessSupervisor,
    ordinary_work: Arc<RwLock<()>>,
    update_installation_lease: Mutex<Option<OwnedRwLockWriteGuard<()>>>,
    runtime_preparation: Mutex<Option<ActiveRuntimePreparation>>,
    runtime_preparation_progress: Mutex<RuntimePreparationProgress>,
    runtime_readiness_inspection: Mutex<Option<Arc<RuntimeReadinessInspection>>>,
    active_package_intake: Mutex<Option<ActivePackageIntake>>,
    active_parses: Mutex<HashMap<ParseTargetKey, ActiveParse>>,
    active_agent_runs: Mutex<HashMap<String, ActiveAgentRun>>,
    agent_capacity: Arc<Semaphore>,
    active_manager_intakes: Mutex<HashMap<String, OrdinaryWorkLease>>,
    manager_intake_execution: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    production_schedulers: Mutex<HashMap<String, OrdinaryWorkLease>>,
    agent_provider: tokio::sync::Mutex<Option<AgentProvider>>,
    provider_cleanup_execution: tokio::sync::Mutex<()>,
    provider_rate_limit: Mutex<Option<ProviderRateLimit>>,
    document_tools_verified: AtomicBool,
    chatgpt_login_state: Mutex<crate::chatgpt_login::ChatGptLoginFlowState>,
    update_installation_active: AtomicBool,
    diagnostics: DiagnosticsStore,
    renderer_diagnostic_rate: Mutex<(u64, u32)>,
}

impl Drop for QuantixHostInner {
    fn drop(&mut self) {
        self.diagnostics.shutdown();
    }
}

/// A single in-flight runtime inspection shared by concurrent public readers.
/// The result is delivered to followers and then discarded; subsequent calls
/// always start a fresh filesystem validation.
pub(crate) struct RuntimeReadinessInspection {
    result: tokio::sync::watch::Sender<Option<RuntimeReadiness>>,
}

static APPLICATION_WORK_LEASES: OnceLock<Mutex<HashMap<PathBuf, Weak<RwLock<()>>>>> =
    OnceLock::new();

fn shared_application_work_lease(application_home: &Path) -> Arc<RwLock<()>> {
    let key = application_home
        .canonicalize()
        .or_else(|_| {
            let parent = application_home
                .parent()
                .ok_or(std::io::Error::from(std::io::ErrorKind::InvalidInput))?
                .canonicalize()?;
            let name = application_home
                .file_name()
                .ok_or(std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
            Ok::<PathBuf, std::io::Error>(parent.join(name))
        })
        .unwrap_or_else(|_| application_home.to_path_buf());
    let mut leases = APPLICATION_WORK_LEASES
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    leases.retain(|_, lease| lease.strong_count() > 0);
    if let Some(lease) = leases.get(&key).and_then(Weak::upgrade) {
        return lease;
    }
    let lease = Arc::new(RwLock::new(()));
    leases.insert(key, Arc::downgrade(&lease));
    lease
}

struct ActiveAgentRun {
    tender_id: String,
    run_id: Option<String>,
    cancellation: CancellationToken,
    _capacity: tokio::sync::OwnedSemaphorePermit,
    _ordinary_work: OrdinaryWorkLease,
}

struct ActiveRuntimePreparation {
    cancellation: CancellationToken,
    _ordinary_work: OrdinaryWorkLease,
}

struct ActiveParse {
    cancellation: CancellationToken,
    _ordinary_work: OrdinaryWorkLease,
}

struct ActivePackageIntake {
    operation_id: String,
    control: PackageIntakeControl,
    _ordinary_work: OrdinaryWorkLease,
}

static PACKAGE_INTAKE_SEQUENCE: AtomicU64 = AtomicU64::new(1);

fn next_package_intake_operation_id() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let sequence = PACKAGE_INTAKE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("package-intake-{timestamp}-{sequence}")
}

pub(crate) struct OrdinaryWorkLease {
    _guard: OwnedRwLockReadGuard<()>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ParseTargetKey {
    tender_id: String,
    artifact_id: String,
    version: u32,
}

impl ParseTargetKey {
    pub(crate) fn new(tender_id: &str, artifact_id: &str, version: u32) -> Self {
        Self {
            tender_id: tender_id.to_owned(),
            artifact_id: artifact_id.to_owned(),
            version,
        }
    }
}

#[derive(Clone)]
pub struct QuantixHost {
    inner: Arc<QuantixHostInner>,
}

impl QuantixHost {
    pub fn new(application_home: impl AsRef<Path>, resource_directory: impl AsRef<Path>) -> Self {
        Self::with_setup_platform_and_runtime(
            application_home,
            Arc::new(SystemSetupPlatform),
            RuntimeLayout::bundled(resource_directory),
        )
    }

    #[cfg(any(test, feature = "runtime-fixture"))]
    pub fn with_setup_platform(
        application_home: impl AsRef<Path>,
        setup_platform: Arc<dyn SetupPlatform>,
    ) -> Self {
        let runtime_resources = application_home.as_ref().join("unbundled-resources");
        let host = Self::with_setup_platform_and_runtime(
            application_home,
            setup_platform,
            RuntimeLayout::bundled(runtime_resources),
        );
        host.accept_runtime_fixture();
        host
    }

    pub fn with_setup_platform_and_runtime(
        application_home: impl AsRef<Path>,
        setup_platform: Arc<dyn SetupPlatform>,
        runtime_layout: RuntimeLayout,
    ) -> Self {
        // Initialize the process-global sqlite-vec auto-extension before any
        // Tauri worker can concurrently create or inspect a Tender Store.
        // Lazy registration from those workers can re-enter SQLite startup
        // and leave the first Tender start spinning before its staging root
        // is created.
        let _ = crate::tender_store::register_sqlite_vec();
        let application_home = application_home.as_ref().to_path_buf();
        let ordinary_work = shared_application_work_lease(&application_home);
        let diagnostics = DiagnosticsStore::new(&application_home);
        Self {
            inner: Arc::new(QuantixHostInner {
                application_home,
                setup_platform,
                setup_lock: Mutex::new(()),
                startup_reconciled: Mutex::new(false),
                startup_reconciliation: Mutex::new(Default::default()),
                catalogue_lock: Mutex::new(()),
                manager_tender_start_lock: Mutex::new(()),
                open_tender_stores: Mutex::new(Default::default()),
                recovery_required_tenders: Mutex::new(Default::default()),
                recovery_operation_lock: Mutex::new(()),
                runtime_layout,
                process_supervisor: ProcessSupervisor,
                ordinary_work,
                update_installation_lease: Mutex::new(None),
                runtime_preparation: Mutex::new(None),
                runtime_preparation_progress: Mutex::new(RuntimePreparationProgress::idle()),
                runtime_readiness_inspection: Mutex::new(None),
                active_package_intake: Mutex::new(None),
                active_parses: Mutex::new(HashMap::new()),
                active_agent_runs: Mutex::new(HashMap::new()),
                agent_capacity: Arc::new(Semaphore::new(2)),
                active_manager_intakes: Mutex::new(HashMap::new()),
                manager_intake_execution: Mutex::new(HashMap::new()),
                production_schedulers: Mutex::new(HashMap::new()),
                agent_provider: tokio::sync::Mutex::new(None),
                provider_cleanup_execution: tokio::sync::Mutex::new(()),
                provider_rate_limit: Mutex::new(None),
                document_tools_verified: AtomicBool::new(false),
                chatgpt_login_state: Mutex::new(
                    crate::chatgpt_login::ChatGptLoginFlowState::default(),
                ),
                update_installation_active: AtomicBool::new(false),
                diagnostics,
                renderer_diagnostic_rate: Mutex::new((0, 0)),
            }),
        }
    }

    pub fn application_home(&self) -> &Path {
        &self.inner.application_home
    }

    pub(crate) fn diagnostics(&self) -> &DiagnosticsStore {
        &self.inner.diagnostics
    }

    pub(crate) fn allow_renderer_diagnostic(&self) -> bool {
        const WINDOW_SECONDS: u64 = 60;
        const MAX_EVENTS_PER_WINDOW: u32 = 12;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let mut rate = self
            .inner
            .renderer_diagnostic_rate
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if now.saturating_sub(rate.0) >= WINDOW_SECONDS {
            *rate = (now, 0);
        }
        if rate.1 >= MAX_EVENTS_PER_WINDOW {
            return false;
        }
        rate.1 += 1;
        true
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn record_application_diagnostic(
        &self,
        severity: DiagnosticSeverity,
        component: DiagnosticComponent,
        event_name: &'static str,
        summary: &'static str,
        operation_id: Option<String>,
        duration_ms: Option<u64>,
        outcome: Option<&'static str>,
        error_code: Option<String>,
    ) {
        let mut fact = RecordDiagnosticFact::new(severity, component, event_name, summary);
        fact.correlation.operation_id = operation_id;
        fact.duration_ms = duration_ms;
        fact.outcome = outcome.map(str::to_owned);
        fact.error_code = error_code;
        fact.success = match outcome {
            Some("completed" | "ready") => Some(true),
            Some("failed" | "interrupted" | "cancelled") => Some(false),
            _ => None,
        };
        self.inner.diagnostics.record_application(fact);
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn record_tender_diagnostic(
        &self,
        tender_id: &str,
        severity: DiagnosticSeverity,
        component: DiagnosticComponent,
        event_name: &'static str,
        summary: &'static str,
        operation_id: Option<String>,
        duration_ms: Option<u64>,
        outcome: Option<&'static str>,
        error_code: Option<String>,
    ) {
        let mut fact = RecordDiagnosticFact::new(severity, component, event_name, summary);
        fact.correlation.operation_id = operation_id;
        fact.duration_ms = duration_ms;
        fact.outcome = outcome.map(str::to_owned);
        fact.error_code = error_code;
        fact.success = match outcome {
            Some("completed" | "ready") => Some(true),
            Some("failed" | "interrupted" | "cancelled") => Some(false),
            _ => None,
        };
        self.inner.diagnostics.record_tender(tender_id, fact);
    }

    pub(crate) fn setup_platform(&self) -> &dyn SetupPlatform {
        self.inner.setup_platform.as_ref()
    }

    pub(crate) fn ensure_setup(&self) -> SetupOutcome {
        let Ok(_ordinary_work) = self.begin_setup_work() else {
            return SetupOutcome::blocked(
                crate::setup::SetupState::RepairRequired,
                crate::setup::SetupIssue::UpdateInstallationActive,
            );
        };
        let _guard = self
            .inner
            .setup_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let outcome = ensure_application_home(
            &self.inner.application_home,
            self.inner.setup_platform.as_ref(),
        );
        if matches!(
            outcome.state,
            crate::setup::SetupState::Ready | crate::setup::SetupState::Warning
        ) && crate::update::update_work_is_blocked(&self.inner.application_home)
        {
            return SetupOutcome::blocked(
                crate::setup::SetupState::RepairRequired,
                crate::setup::SetupIssue::UpdateInstallationActive,
            );
        }
        if matches!(
            outcome.state,
            crate::setup::SetupState::Ready | crate::setup::SetupState::Warning
        ) && self.inner.diagnostics.activate().is_ok()
        {
            let _ = self
                .inner
                .diagnostics
                .record_application(RecordDiagnosticFact {
                    severity: if outcome.issues.is_empty() {
                        DiagnosticSeverity::Info
                    } else {
                        DiagnosticSeverity::Warning
                    },
                    scope: DiagnosticScope::Application,
                    component: DiagnosticComponent::Startup,
                    event_name: "setup_inspected".into(),
                    summary: if outcome.issues.is_empty() {
                        "Application storage checks completed".into()
                    } else {
                        "Application storage checks completed with warnings".into()
                    },
                    correlation: DiagnosticCorrelation::default(),
                    duration_ms: None,
                    outcome: Some("completed".into()),
                    error_code: None,
                    deep: false,
                    request_id: None,
                    size_bytes: None,
                    redaction_count: 0,
                    original_error_event: None,
                    initiated_by: None,
                    action: None,
                    success: Some(true),
                });
        }
        #[cfg(any(test, feature = "runtime-fixture"))]
        if self.document_tools_are_verified()
            && matches!(
                outcome.state,
                crate::setup::SetupState::Ready | crate::setup::SetupState::Warning
            )
        {
            let _ = crate::application_settings::seed_runtime_fixture_ai_selection(
                &self.inner.application_home,
            );
        }
        outcome
    }

    pub(crate) fn validate_setup_for_update_restart(&self) -> SetupOutcome {
        let _guard = self
            .inner
            .setup_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        // Restart validation must inspect the same Application Home invariants as Setup,
        // while intentionally bypassing only the active-update work gate that ordinary
        // Setup is required to report as UpdateInstallationActive.
        ensure_application_home(
            &self.inner.application_home,
            self.inner.setup_platform.as_ref(),
        )
    }

    pub(crate) fn reconcile_startup_once(&self) -> Result<(), TenderCommandError> {
        let _ordinary_work = self.begin_setup_work()?;
        let mut reconciled = self
            .inner
            .startup_reconciled
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        if !*reconciled {
            crate::tender_store::backups::reconcile_interrupted_backup_operations(
                &self.inner.application_home,
            )?;
            self.reconcile_trash_records()?;
            let removed_tender_candidates =
                crate::tender_store::reconcile_application_staging(&self.inner.application_home)?;
            *self
                .inner
                .startup_reconciliation
                .lock()
                .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))? =
                StartupReconciliationReport {
                    removed_tender_candidates,
                };
            *reconciled = true;
        }
        Ok(())
    }

    pub fn inspect_startup_reconciliation(&self) -> StartupReconciliationReport {
        self.inner
            .startup_reconciliation
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub(crate) fn open_tender_stores(&self) -> &OpenTenderStores {
        &self.inner.open_tender_stores
    }

    pub(crate) fn recovery_required_tenders(&self) -> &Mutex<HashSet<TenderId>> {
        &self.inner.recovery_required_tenders
    }

    pub(crate) fn recovery_operation_lock(&self) -> &Mutex<()> {
        &self.inner.recovery_operation_lock
    }

    pub(crate) fn catalogue_lock(&self) -> &Mutex<()> {
        &self.inner.catalogue_lock
    }

    pub(crate) fn manager_tender_start_lock(&self) -> &Mutex<()> {
        &self.inner.manager_tender_start_lock
    }

    pub(crate) fn runtime_layout(&self) -> &RuntimeLayout {
        &self.inner.runtime_layout
    }

    pub(crate) fn process_supervisor(&self) -> &ProcessSupervisor {
        &self.inner.process_supervisor
    }

    pub(crate) fn agent_provider(&self) -> &tokio::sync::Mutex<Option<AgentProvider>> {
        &self.inner.agent_provider
    }

    pub(crate) fn provider_cleanup_execution(&self) -> &tokio::sync::Mutex<()> {
        &self.inner.provider_cleanup_execution
    }

    pub(crate) fn chatgpt_login_state(
        &self,
    ) -> &Mutex<crate::chatgpt_login::ChatGptLoginFlowState> {
        &self.inner.chatgpt_login_state
    }

    pub(crate) fn observe_provider_usage(&self, usage: &ProviderUsage) {
        if let Some(rate_limit) = usage.rate_limit.as_ref() {
            *self
                .inner
                .provider_rate_limit
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(rate_limit.clone());
        }
    }

    pub(crate) fn provider_subscription_capacity_is_exhausted(&self) -> bool {
        let rate_limit = self
            .inner
            .provider_rate_limit
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        let Some(rate_limit) = rate_limit else {
            return false;
        };
        if rate_limit.state == ProviderRateLimitState::Available {
            return false;
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .and_then(|duration| i64::try_from(duration.as_secs()).ok());
        let Some(now) = now else {
            return true;
        };
        let reset_times = [rate_limit.primary.as_ref(), rate_limit.secondary.as_ref()]
            .into_iter()
            .flatten()
            .map(|window| window.resets_at_epoch_seconds)
            .collect::<Vec<_>>();
        reset_times.is_empty()
            || reset_times
                .into_iter()
                .any(|reset_at| reset_at.is_none_or(|reset_at| reset_at > now))
    }

    pub(crate) fn claim_production_scheduler(&self, tender_id: &str) -> bool {
        let Ok(ordinary_work) = self.begin_ordinary_work() else {
            return false;
        };
        let mut schedulers = self
            .inner
            .production_schedulers
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if schedulers.contains_key(tender_id) {
            return false;
        }
        schedulers.insert(tender_id.to_owned(), ordinary_work);
        true
    }

    pub(crate) fn release_production_scheduler(&self, tender_id: &str) {
        self.inner
            .production_schedulers
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(tender_id);
    }

    pub(crate) fn runtime_preparation_is_active(&self) -> bool {
        self.inner
            .runtime_preparation
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .is_some()
    }

    pub(crate) fn begin_runtime_preparation(&self) -> Option<CancellationToken> {
        let ordinary_work = self.begin_ordinary_work().ok()?;
        let mut preparation = self
            .inner
            .runtime_preparation
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if preparation.is_some() {
            return None;
        }
        let cancellation = CancellationToken::new();
        *preparation = Some(ActiveRuntimePreparation {
            cancellation: cancellation.clone(),
            _ordinary_work: ordinary_work,
        });
        Some(cancellation)
    }

    pub(crate) fn finish_runtime_preparation(&self) {
        *self
            .inner
            .runtime_preparation
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    }

    pub(crate) fn begin_runtime_preparation_progress(&self) {
        self.inner
            .runtime_preparation_progress
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .begin();
    }

    pub(crate) fn activate_runtime_preparation_step(&self, step: RuntimePreparationStep) {
        self.inner
            .runtime_preparation_progress
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .activate(step);
    }

    pub(crate) fn finish_runtime_preparation_progress(&self, succeeded: bool) {
        self.inner
            .runtime_preparation_progress
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .finish(succeeded);
    }

    pub(crate) fn finish_runtime_preparation_cancelled(&self) {
        self.inner
            .runtime_preparation_progress
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .finish_cancelled();
    }

    pub fn inspect_runtime_preparation_progress(&self) -> RuntimePreparationProgress {
        self.inner
            .runtime_preparation_progress
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
            .observe_model_files(&self.inner.application_home)
    }

    pub(crate) fn begin_runtime_readiness_inspection(
        &self,
    ) -> (Arc<RuntimeReadinessInspection>, bool) {
        let mut active = self
            .inner
            .runtime_readiness_inspection
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(flight) = active.as_ref() {
            return (flight.clone(), false);
        }
        let (sender, _receiver) = tokio::sync::watch::channel(None);
        let flight = Arc::new(RuntimeReadinessInspection { result: sender });
        *active = Some(flight.clone());
        (flight, true)
    }

    pub(crate) async fn await_runtime_readiness_inspection(
        &self,
        flight: &RuntimeReadinessInspection,
    ) -> RuntimeReadiness {
        let mut receiver = flight.result.subscribe();
        loop {
            let result = receiver.borrow().clone();
            if let Some(result) = result {
                let mut active = self
                    .inner
                    .runtime_readiness_inspection
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                if active
                    .as_ref()
                    .is_some_and(|current| std::ptr::eq(Arc::as_ptr(current), flight))
                {
                    *active = None;
                }
                return result;
            }
            if receiver.changed().await.is_err() {
                return RuntimeReadiness {
                    state: crate::runtime_readiness::RuntimeReadinessState::RepairRequired,
                    issues: vec![
                        crate::runtime_readiness::RuntimeReadinessIssue::RuntimeProbeFailed,
                    ],
                    uv_version: None,
                    ocr_version: None,
                    repair_available: false,
                };
            }
        }
    }

    pub(crate) fn finish_runtime_readiness_inspection(
        &self,
        flight: &Arc<RuntimeReadinessInspection>,
        result: RuntimeReadiness,
    ) {
        flight.result.send_replace(Some(result));
    }

    /// Claims the single package-intake slot and returns the clonable control
    /// that the synchronous importer uses to publish snapshots and observe
    /// cancellation. The control remains owned by the Host so inspect/cancel
    /// commands always address the currently running operation.
    pub(crate) fn begin_package_intake(
        &self,
        kind: PackageIntakeOperationKind,
        source_kind: TenderPackageSourceKind,
        source_name: impl Into<String>,
    ) -> Result<PackageIntakeControl, TenderCommandError> {
        let ordinary_work = self.begin_ordinary_work()?;
        let mut active = self
            .inner
            .active_package_intake
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        if active.is_some() {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let operation_id = next_package_intake_operation_id();
        let control =
            PackageIntakeControl::new(operation_id.clone(), kind, source_kind, source_name);
        *active = Some(ActivePackageIntake {
            operation_id: operation_id.clone(),
            control: control.clone(),
            _ordinary_work: ordinary_work,
        });
        drop(active);
        self.record_application_diagnostic(
            DiagnosticSeverity::Info,
            DiagnosticComponent::Package,
            "package_intake_started",
            "A governed package intake operation started",
            Some(operation_id),
            None,
            Some("started"),
            None,
        );
        Ok(control)
    }

    pub(crate) fn inspect_package_intake_progress(&self) -> Option<PackageIntakeProgress> {
        self.inner
            .active_package_intake
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_ref()
            .map(|active| active.control.snapshot())
    }

    pub(crate) fn cancel_package_intake(&self, command: CancelPackageIntakeCommand) -> bool {
        let active = self
            .inner
            .active_package_intake
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(active) = active.as_ref() else {
            return false;
        };
        if active.operation_id != command.operation_id || !active.control.is_cancellable() {
            return false;
        }
        active.control.request_cancel();
        true
    }

    pub(crate) fn finish_package_intake(&self, operation_id: &str) {
        let mut active = self
            .inner
            .active_package_intake
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if active
            .as_ref()
            .is_some_and(|active| active.operation_id == operation_id)
        {
            *active = None;
        }
    }

    pub(crate) fn cancel_active_runtime_preparation(&self) -> bool {
        let preparation = self
            .inner
            .runtime_preparation
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(preparation) = preparation.as_ref() {
            preparation.cancellation.cancel();
            true
        } else {
            false
        }
    }

    pub(crate) fn begin_active_parse(
        &self,
        key: ParseTargetKey,
    ) -> Result<CancellationToken, TenderCommandError> {
        let ordinary_work = self.begin_ordinary_work()?;
        let mut active = self
            .inner
            .active_parses
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        if active.contains_key(&key) {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let cancellation = CancellationToken::new();
        active.insert(
            key,
            ActiveParse {
                cancellation: cancellation.clone(),
                _ordinary_work: ordinary_work,
            },
        );
        Ok(cancellation)
    }

    pub(crate) fn finish_active_parse(&self, key: &ParseTargetKey) {
        self.inner
            .active_parses
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(key);
    }

    pub(crate) fn cancel_active_parse(&self, key: &ParseTargetKey) -> bool {
        let active = self
            .inner
            .active_parses
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(parse) = active.get(key) {
            parse.cancellation.cancel();
            true
        } else {
            false
        }
    }

    pub(crate) async fn begin_active_agent_run(
        &self,
        tender_id: &str,
    ) -> Result<(String, CancellationToken), TenderCommandError> {
        // Capacity is deliberately acquired asynchronously. A full Host does
        // not reject a valid Tender operation; it waits in the provider queue
        // while the authoritative Tender state remains inspectable.
        let capacity = self
            .inner
            .agent_capacity
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        let ordinary_work = self.begin_ordinary_work()?;
        let mut active = self
            .inner
            .active_agent_runs
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        let lease_id = (1..=2)
            .map(|sequence| format!("agent-run-lease-{sequence}"))
            .find(|candidate| !active.contains_key(candidate))
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let cancellation = CancellationToken::new();
        active.insert(
            lease_id.clone(),
            ActiveAgentRun {
                tender_id: tender_id.to_owned(),
                run_id: None,
                cancellation: cancellation.clone(),
                _capacity: capacity,
                _ordinary_work: ordinary_work,
            },
        );
        Ok((lease_id, cancellation))
    }

    pub(crate) fn identify_active_agent_run(
        &self,
        lease_id: &str,
        run_id: &str,
    ) -> Result<(), TenderCommandError> {
        let mut active = self
            .inner
            .active_agent_runs
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        let active = active
            .get_mut(lease_id)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        active.run_id = Some(run_id.to_owned());
        Ok(())
    }

    pub(crate) fn finish_active_agent_run(&self, lease_id: &str) {
        self.inner
            .active_agent_runs
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(lease_id);
    }

    pub(crate) fn cancel_active_agent_run(&self, tender_id: &str, run_id: &str) -> bool {
        let active = self
            .inner
            .active_agent_runs
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for active in active.values() {
            if active.tender_id == tender_id && active.run_id.as_deref() == Some(run_id) {
                active.cancellation.cancel();
                return true;
            }
        }
        false
    }

    pub(crate) fn agent_run_is_active(&self, tender_id: &str, run_id: &str) -> bool {
        self.inner
            .active_agent_runs
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .values()
            .any(|active| active.tender_id == tender_id && active.run_id.as_deref() == Some(run_id))
    }

    pub(crate) fn begin_manager_intake(&self, tender_id: &str) -> Result<bool, TenderCommandError> {
        let ordinary_work = self.begin_ordinary_work()?;
        let mut active = self
            .inner
            .active_manager_intakes
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        if active.contains_key(tender_id) {
            return Ok(false);
        }
        active.insert(tender_id.to_owned(), ordinary_work);
        Ok(true)
    }

    pub(crate) fn finish_manager_intake(&self, tender_id: &str) {
        self.inner
            .active_manager_intakes
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(tender_id);
    }

    pub(crate) async fn manager_intake_execution_guard(
        &self,
        tender_id: &str,
    ) -> Result<OwnedMutexGuard<()>, TenderCommandError> {
        let mutex = {
            let mut guards = self
                .inner
                .manager_intake_execution
                .lock()
                .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
            guards
                .entry(tender_id.to_owned())
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
                .clone()
        };
        Ok(mutex.lock_owned().await)
    }

    pub(crate) fn set_document_tools_verified(&self, verified: bool) {
        self.inner
            .document_tools_verified
            .store(verified, Ordering::Release);
    }

    pub(crate) fn document_tools_are_verified(&self) -> bool {
        self.inner.document_tools_verified.load(Ordering::Acquire)
    }

    pub(crate) fn require_document_tools(&self) -> Result<(), TenderCommandError> {
        if self.document_tools_are_verified() && !self.update_installation_is_active() {
            Ok(())
        } else {
            Err(TenderCommandError::new(
                TenderErrorCode::LocalDocumentToolsRequired,
            ))
        }
    }

    pub(crate) fn begin_ordinary_work(&self) -> Result<OrdinaryWorkLease, TenderCommandError> {
        if self.update_installation_is_active() {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let guard = Arc::clone(&self.inner.ordinary_work)
            .try_read_owned()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        if self.update_installation_is_active() {
            drop(guard);
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        Ok(OrdinaryWorkLease { _guard: guard })
    }

    pub(crate) fn begin_recovery_deletion_work(
        &self,
    ) -> Result<OwnedRwLockWriteGuard<()>, TenderCommandError> {
        if self.update_installation_is_active() {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let guard = Arc::clone(&self.inner.ordinary_work)
            .try_write_owned()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        if self.update_installation_is_active() {
            drop(guard);
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        Ok(guard)
    }

    pub(crate) fn begin_setup_work(&self) -> Result<OrdinaryWorkLease, TenderCommandError> {
        if self
            .inner
            .update_installation_active
            .load(Ordering::Acquire)
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let guard = Arc::clone(&self.inner.ordinary_work)
            .try_read_owned()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        if self
            .inner
            .update_installation_active
            .load(Ordering::Acquire)
        {
            drop(guard);
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        Ok(OrdinaryWorkLease { _guard: guard })
    }

    pub(crate) fn update_installation_is_active(&self) -> bool {
        self.inner
            .update_installation_active
            .load(Ordering::Acquire)
            || crate::update::update_work_is_blocked(self.application_home())
    }

    pub(crate) fn claim_update_installation(&self) -> bool {
        if self
            .inner
            .update_installation_active
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return false;
        }
        let Ok(guard) = Arc::clone(&self.inner.ordinary_work).try_write_owned() else {
            self.inner
                .update_installation_active
                .store(false, Ordering::Release);
            return false;
        };
        let Ok(mut slot) = self.inner.update_installation_lease.lock() else {
            self.inner
                .update_installation_active
                .store(false, Ordering::Release);
            return false;
        };
        if slot.is_some() {
            self.inner
                .update_installation_active
                .store(false, Ordering::Release);
            return false;
        }
        *slot = Some(guard);
        true
    }

    pub(crate) fn release_update_installation(&self) {
        *self
            .inner
            .update_installation_lease
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
        self.inner
            .update_installation_active
            .store(false, Ordering::Release);
    }

    pub(crate) fn update_installation_lease_is_held(&self) -> bool {
        self.inner
            .update_installation_lease
            .lock()
            .map(|lease| lease.is_some())
            .unwrap_or(false)
    }

    pub(crate) const fn acting_engineer_user(&self) -> &'static str {
        "engineer_user"
    }

    pub(crate) fn update_environment_is_quiescent(&self) -> bool {
        !self.runtime_preparation_is_active()
            && self
                .inner
                .active_package_intake
                .lock()
                .map(|active| active.is_none())
                .unwrap_or(false)
            && self
                .inner
                .active_parses
                .lock()
                .map(|active| active.is_empty())
                .unwrap_or(false)
            && self
                .inner
                .active_agent_runs
                .lock()
                .map(|active| active.is_empty())
                .unwrap_or(false)
            && self
                .inner
                .active_manager_intakes
                .lock()
                .map(|active| active.is_empty())
                .unwrap_or(false)
            && self
                .inner
                .production_schedulers
                .lock()
                .map(|active| active.is_empty())
                .unwrap_or(false)
            && self.inner.recovery_operation_lock.try_lock().is_ok()
    }

    #[cfg(any(test, feature = "runtime-fixture"))]
    pub fn accept_runtime_fixture(&self) {
        self.set_document_tools_verified(true);
    }

    pub(crate) fn generate_tender_id(&self) -> Result<TenderId, TenderCommandError> {
        let connection = rusqlite::Connection::open_in_memory()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        for _ in 0..16 {
            let tender_id: String = connection
                .query_row("SELECT lower(hex(randomblob(16)))", [], |row| row.get(0))
                .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
            let tender_id = TenderId::parse(&tender_id)?;
            if !self
                .application_home()
                .join("tenders")
                .join(tender_id.as_str())
                .exists()
                && !self
                    .application_home()
                    .join("staging")
                    .join(format!("tender-{}", tender_id.as_str()))
                    .exists()
            {
                return Ok(tender_id);
            }
        }
        Err(TenderCommandError::new(TenderErrorCode::StoreUnavailable))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_runtime::{ProviderRateLimitWindow, ProviderUsage};

    #[test]
    fn package_intake_slot_is_exclusive_and_cancellation_requires_the_exact_id() {
        let root = tempfile::tempdir().expect("temporary application home");
        let host = QuantixHost::new(root.path(), root.path());
        let control = host
            .begin_package_intake(
                PackageIntakeOperationKind::StartTender,
                TenderPackageSourceKind::Directory,
                "Tender Package",
            )
            .expect("first package operation should claim the slot");
        let operation_id = control.snapshot().operation_id;

        let concurrent = host
            .begin_package_intake(
                PackageIntakeOperationKind::AddPackage,
                TenderPackageSourceKind::ZipArchive,
                "additional.zip",
            )
            .expect_err("a second package operation must be rejected");
        assert_eq!(concurrent.code, TenderErrorCode::InvalidCommand);

        assert!(!host.cancel_package_intake(CancelPackageIntakeCommand {
            operation_id: "stale-operation-id".into(),
        }));
        assert!(!control.snapshot().cancellation_requested);
        assert!(host.cancel_package_intake(CancelPackageIntakeCommand {
            operation_id: operation_id.clone(),
        }));
        assert!(control.snapshot().cancellation_requested);

        host.finish_package_intake("stale-operation-id");
        assert_eq!(
            host.inspect_package_intake_progress()
                .expect("stale completion must not clear the active operation")
                .operation_id,
            operation_id,
        );
        host.finish_package_intake(&operation_id);
        assert!(host.inspect_package_intake_progress().is_none());
    }

    #[test]
    fn package_intake_cannot_be_cancelled_after_finalization_begins() {
        let root = tempfile::tempdir().expect("temporary application home");
        let host = QuantixHost::new(root.path(), root.path());
        let control = host
            .begin_package_intake(
                PackageIntakeOperationKind::AddPackage,
                TenderPackageSourceKind::Directory,
                "Tender Package",
            )
            .expect("package operation should claim the slot");
        let operation_id = control.snapshot().operation_id;
        control.mark_finalization();

        assert!(!host.cancel_package_intake(CancelPackageIntakeCommand { operation_id }));
        assert!(!control.snapshot().cancellation_requested);
    }

    #[tokio::test]
    async fn runtime_readiness_inspection_is_single_flight_without_stale_reuse() {
        let root = tempfile::tempdir().expect("temporary application home");
        let host = QuantixHost::new(root.path(), root.path());
        let (leader_flight, leader) = host.begin_runtime_readiness_inspection();
        assert!(leader);
        let (follower_flight, follower) = host.begin_runtime_readiness_inspection();
        assert!(!follower);
        assert!(Arc::ptr_eq(&leader_flight, &follower_flight));

        let expected = RuntimeReadiness::state(
            crate::runtime_readiness::RuntimeReadinessState::RepairRequired,
            crate::runtime_readiness::RuntimeReadinessIssue::RuntimeProbeFailed,
        );
        host.finish_runtime_readiness_inspection(&leader_flight, expected.clone());

        let (late_flight, late_leader) = host.begin_runtime_readiness_inspection();
        assert!(
            !late_leader,
            "completed flight must remain joinable until consumed"
        );
        assert!(Arc::ptr_eq(&leader_flight, &late_flight));
        assert_eq!(
            host.await_runtime_readiness_inspection(&late_flight).await,
            expected
        );

        let (_next_flight, next_leader) = host.begin_runtime_readiness_inspection();
        assert!(next_leader, "completed inspection must not become a cache");
    }

    #[test]
    fn recovery_deletion_requires_all_active_work_to_finish() {
        let root = tempfile::tempdir().expect("temporary application home");
        let host = QuantixHost::new(root.path(), root.path());
        let control = host
            .begin_package_intake(
                PackageIntakeOperationKind::AddPackage,
                TenderPackageSourceKind::Directory,
                "Tender Package",
            )
            .expect("package operation should claim ordinary work");
        let error = host
            .begin_recovery_deletion_work()
            .expect_err("active package work must block recovery deletion");
        assert_eq!(error.code, TenderErrorCode::InvalidCommand);

        host.finish_package_intake(&control.snapshot().operation_id);
        let deletion = host
            .begin_recovery_deletion_work()
            .expect("recovery deletion can begin after active work finishes");
        drop(deletion);
    }

    #[tokio::test]
    async fn manager_intake_execution_is_serial_per_tender_and_parallel_across_tenders() {
        let root = tempfile::tempdir().expect("temporary application home");
        let host = QuantixHost::new(root.path(), root.path());
        let first = host
            .manager_intake_execution_guard("tender-a")
            .await
            .expect("first Tender should acquire its execution guard");

        let other = tokio::time::timeout(
            std::time::Duration::from_millis(50),
            host.manager_intake_execution_guard("tender-b"),
        )
        .await
        .expect("different Tenders must not share an execution guard")
        .expect("second Tender should acquire its execution guard");

        let same = tokio::time::timeout(
            std::time::Duration::from_millis(20),
            host.manager_intake_execution_guard("tender-a"),
        )
        .await;
        assert!(same.is_err(), "the same Tender must remain serialized");

        drop(other);
        drop(first);
        tokio::time::timeout(
            std::time::Duration::from_millis(50),
            host.manager_intake_execution_guard("tender-a"),
        )
        .await
        .expect("same Tender should proceed after its prior run releases")
        .expect("released Tender guard should be reacquirable");
    }

    #[test]
    fn provider_subscription_capacity_is_shared_at_the_host_boundary() {
        let root = tempfile::tempdir().expect("temporary application home");
        let host = QuantixHost::new(root.path(), root.path());
        host.observe_provider_usage(&ProviderUsage {
            rate_limit: Some(ProviderRateLimit {
                state: ProviderRateLimitState::Exhausted,
                primary: Some(ProviderRateLimitWindow {
                    used_percent: 100,
                    window_minutes: Some(5),
                    resets_at_epoch_seconds: None,
                }),
                secondary: None,
            }),
            ..ProviderUsage::default()
        });

        assert!(host.provider_subscription_capacity_is_exhausted());

        host.observe_provider_usage(&ProviderUsage {
            rate_limit: Some(ProviderRateLimit {
                state: ProviderRateLimitState::Available,
                primary: None,
                secondary: None,
            }),
            ..ProviderUsage::default()
        });
        assert!(!host.provider_subscription_capacity_is_exhausted());
    }
}
