use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use tokio_util::sync::CancellationToken;

use crate::agent_runtime::CodexProvider;
use crate::process_supervisor::ProcessSupervisor;
use crate::runtime_readiness::RuntimeLayout;
use crate::setup::{ensure_application_home, SetupOutcome, SetupPlatform, SystemSetupPlatform};
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
    open_tender_stores: OpenTenderStores,
    recovery_required_tenders: Mutex<HashSet<TenderId>>,
    recovery_operation_lock: Mutex<()>,
    runtime_layout: RuntimeLayout,
    process_supervisor: ProcessSupervisor,
    runtime_preparation: Mutex<Option<CancellationToken>>,
    active_parses: Mutex<HashMap<ParseTargetKey, CancellationToken>>,
    active_agent_run: Mutex<Option<ActiveAgentRun>>,
    agent_provider: tokio::sync::Mutex<Option<CodexProvider>>,
    runtime_verified: AtomicBool,
}

struct ActiveAgentRun {
    tender_id: String,
    run_id: Option<String>,
    cancellation: CancellationToken,
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
        Self {
            inner: Arc::new(QuantixHostInner {
                application_home: application_home.as_ref().to_path_buf(),
                setup_platform,
                setup_lock: Mutex::new(()),
                startup_reconciled: Mutex::new(false),
                startup_reconciliation: Mutex::new(Default::default()),
                catalogue_lock: Mutex::new(()),
                open_tender_stores: Mutex::new(Default::default()),
                recovery_required_tenders: Mutex::new(Default::default()),
                recovery_operation_lock: Mutex::new(()),
                runtime_layout,
                process_supervisor: ProcessSupervisor,
                runtime_preparation: Mutex::new(None),
                active_parses: Mutex::new(HashMap::new()),
                active_agent_run: Mutex::new(None),
                agent_provider: tokio::sync::Mutex::new(None),
                runtime_verified: AtomicBool::new(false),
            }),
        }
    }

    pub fn application_home(&self) -> &Path {
        &self.inner.application_home
    }

    pub(crate) fn setup_platform(&self) -> &dyn SetupPlatform {
        self.inner.setup_platform.as_ref()
    }

    pub(crate) fn ensure_setup(&self) -> SetupOutcome {
        let _guard = self
            .inner
            .setup_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        ensure_application_home(
            &self.inner.application_home,
            self.inner.setup_platform.as_ref(),
        )
    }

    pub(crate) fn reconcile_startup_once(&self) -> Result<(), TenderCommandError> {
        let mut reconciled = self
            .inner
            .startup_reconciled
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        if !*reconciled {
            crate::tender_store::backups::reconcile_interrupted_backup_operations(
                &self.inner.application_home,
            )?;
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

    pub(crate) fn runtime_layout(&self) -> &RuntimeLayout {
        &self.inner.runtime_layout
    }

    pub(crate) fn process_supervisor(&self) -> &ProcessSupervisor {
        &self.inner.process_supervisor
    }

    pub(crate) fn agent_provider(&self) -> &tokio::sync::Mutex<Option<CodexProvider>> {
        &self.inner.agent_provider
    }

    pub(crate) fn runtime_preparation_is_active(&self) -> bool {
        self.inner
            .runtime_preparation
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .is_some()
    }

    pub(crate) fn begin_runtime_preparation(&self) -> Option<CancellationToken> {
        let mut preparation = self
            .inner
            .runtime_preparation
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if preparation.is_some() {
            return None;
        }
        let cancellation = CancellationToken::new();
        *preparation = Some(cancellation.clone());
        Some(cancellation)
    }

    pub(crate) fn finish_runtime_preparation(&self) {
        *self
            .inner
            .runtime_preparation
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    }

    pub(crate) fn cancel_active_runtime_preparation(&self) -> bool {
        let preparation = self
            .inner
            .runtime_preparation
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(cancellation) = preparation.as_ref() {
            cancellation.cancel();
            true
        } else {
            false
        }
    }

    pub(crate) fn begin_active_parse(
        &self,
        key: ParseTargetKey,
    ) -> Result<CancellationToken, TenderCommandError> {
        let mut active = self
            .inner
            .active_parses
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        if active.contains_key(&key) {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let cancellation = CancellationToken::new();
        active.insert(key, cancellation.clone());
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
        if let Some(cancellation) = active.get(key) {
            cancellation.cancel();
            true
        } else {
            false
        }
    }

    pub(crate) fn begin_active_agent_run(
        &self,
        tender_id: &str,
    ) -> Result<CancellationToken, TenderCommandError> {
        let mut active = self
            .inner
            .active_agent_run
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        if active.is_some() {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let cancellation = CancellationToken::new();
        *active = Some(ActiveAgentRun {
            tender_id: tender_id.to_owned(),
            run_id: None,
            cancellation: cancellation.clone(),
        });
        Ok(cancellation)
    }

    pub(crate) fn identify_active_agent_run(&self, run_id: &str) -> Result<(), TenderCommandError> {
        let mut active = self
            .inner
            .active_agent_run
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        let active = active
            .as_mut()
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        active.run_id = Some(run_id.to_owned());
        Ok(())
    }

    pub(crate) fn finish_active_agent_run(&self) {
        *self
            .inner
            .active_agent_run
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    }

    pub(crate) fn cancel_active_agent_run(&self, tender_id: &str, run_id: &str) -> bool {
        let active = self
            .inner
            .active_agent_run
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(active) = active.as_ref() {
            if active.tender_id == tender_id && active.run_id.as_deref() == Some(run_id) {
                active.cancellation.cancel();
                return true;
            }
        }
        false
    }

    pub(crate) fn agent_run_is_active(&self, tender_id: &str, run_id: &str) -> bool {
        self.inner
            .active_agent_run
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_ref()
            .is_some_and(|active| {
                active.tender_id == tender_id && active.run_id.as_deref() == Some(run_id)
            })
    }

    pub(crate) fn set_runtime_verified(&self, verified: bool) {
        self.inner
            .runtime_verified
            .store(verified, Ordering::Release);
    }

    pub(crate) fn runtime_is_verified(&self) -> bool {
        self.inner.runtime_verified.load(Ordering::Acquire)
    }

    pub(crate) fn require_runtime_verified(&self) -> Result<(), TenderCommandError> {
        if self.runtime_is_verified() {
            Ok(())
        } else {
            Err(TenderCommandError::new(TenderErrorCode::RuntimeRequired))
        }
    }

    #[cfg(any(test, feature = "runtime-fixture"))]
    pub fn accept_runtime_fixture(&self) {
        self.set_runtime_verified(true);
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
