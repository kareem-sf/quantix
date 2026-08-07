use std::path::{Path, PathBuf};

use serde::Serialize;
use ts_rs::TS;

#[derive(Debug)]
pub struct QuantixHost {
    application_home: PathBuf,
}

impl QuantixHost {
    pub fn new(application_home: impl AsRef<Path>) -> Self {
        Self {
            application_home: application_home.as_ref().to_path_buf(),
        }
    }

    pub fn application_home(&self) -> &Path {
        &self.application_home
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum TenderOfficeReadiness {
    ReadyForSetup,
}

pub fn inspect_tender_office_readiness(
    host: &QuantixHost,
) -> Result<TenderOfficeReadiness, &'static str> {
    if host.application_home().is_absolute() {
        Ok(TenderOfficeReadiness::ReadyForSetup)
    } else {
        Err("Quantix Application Home must be an absolute path")
    }
}
