use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use crate::setup::{ensure_application_home, SetupOutcome, SetupPlatform, SystemSetupPlatform};
use crate::tender_store::{OpenTenderStores, TenderCommandError, TenderErrorCode, TenderId};

struct QuantixHostInner {
    application_home: PathBuf,
    setup_platform: Arc<dyn SetupPlatform>,
    setup_lock: Mutex<()>,
    catalogue_lock: Mutex<()>,
    open_tender_stores: OpenTenderStores,
}

#[derive(Clone)]
pub struct QuantixHost {
    inner: Arc<QuantixHostInner>,
}

impl QuantixHost {
    pub fn new(application_home: impl AsRef<Path>) -> Self {
        Self::with_setup_platform(application_home, Arc::new(SystemSetupPlatform))
    }

    pub fn with_setup_platform(
        application_home: impl AsRef<Path>,
        setup_platform: Arc<dyn SetupPlatform>,
    ) -> Self {
        Self {
            inner: Arc::new(QuantixHostInner {
                application_home: application_home.as_ref().to_path_buf(),
                setup_platform,
                setup_lock: Mutex::new(()),
                catalogue_lock: Mutex::new(()),
                open_tender_stores: Mutex::new(Default::default()),
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
