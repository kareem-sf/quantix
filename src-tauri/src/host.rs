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
pub enum HostConnectionState {
    Connected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum HostRuntime {
    LocalTauriDesktop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum HostCommandInterface {
    NamedDomainCommands,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum RendererAssetSource {
    BundledLocal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct QuantixHostStatus {
    pub connection: HostConnectionState,
    pub runtime: HostRuntime,
    pub command_interface: HostCommandInterface,
    pub renderer_assets: RendererAssetSource,
}

pub fn inspect_quantix_host(host: &QuantixHost) -> QuantixHostStatus {
    let _application_home = host.application_home();

    QuantixHostStatus {
        connection: HostConnectionState::Connected,
        runtime: HostRuntime::LocalTauriDesktop,
        command_interface: HostCommandInterface::NamedDomainCommands,
        renderer_assets: RendererAssetSource::BundledLocal,
    }
}
