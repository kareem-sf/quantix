use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use crate::setup::{ensure_application_home, SetupOutcome, SetupPlatform, SystemSetupPlatform};

struct QuantixHostInner {
    application_home: PathBuf,
    setup_platform: Arc<dyn SetupPlatform>,
    setup_lock: Mutex<()>,
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
}
