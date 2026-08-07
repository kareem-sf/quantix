use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use tokio_util::sync::CancellationToken;

use crate::process_supervisor::ProcessSupervisor;
use crate::runtime_readiness::RuntimeLayout;
use crate::setup::{ensure_application_home, SetupOutcome, SetupPlatform, SystemSetupPlatform};
use crate::tender_store::{OpenTenderStores, TenderCommandError, TenderErrorCode, TenderId};

struct QuantixHostInner {
    application_home: PathBuf,
    setup_platform: Arc<dyn SetupPlatform>,
    setup_lock: Mutex<()>,
    catalogue_lock: Mutex<()>,
    open_tender_stores: OpenTenderStores,
    runtime_layout: RuntimeLayout,
    process_supervisor: ProcessSupervisor,
    runtime_preparation: Mutex<Option<CancellationToken>>,
    active_parses: Mutex<HashMap<ParseTargetKey, CancellationToken>>,
    runtime_verified: AtomicBool,
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
                catalogue_lock: Mutex::new(()),
                open_tender_stores: Mutex::new(Default::default()),
                runtime_layout,
                process_supervisor: ProcessSupervisor,
                runtime_preparation: Mutex::new(None),
                active_parses: Mutex::new(HashMap::new()),
                runtime_verified: AtomicBool::new(false),
            }),
        }
    }

    pub fn application_home(&self) -> &Path {
        &self.inner.application_home
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

    pub(crate) fn open_tender_stores(&self) -> &OpenTenderStores {
        &self.inner.open_tender_stores
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

    #[cfg(feature = "runtime-fixture")]
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
