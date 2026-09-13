use std::path::PathBuf;

/// The normal application-owned storage layout shared by the native shell
/// and the local service. Resolving paths is side-effect free; callers create
/// directories only when starting the corresponding component.
#[derive(Clone, Debug)]
pub struct StoragePaths {
    pub root: PathBuf,
    // Created only by the release sidecar launch in `service::start`.
    #[cfg_attr(debug_assertions, allow(dead_code))]
    pub runtime: PathBuf,
    pub connection_file: PathBuf,
    pub logs: PathBuf,
    #[cfg_attr(debug_assertions, allow(dead_code))]
    pub tmp: PathBuf,
    pub webview: PathBuf,
}

impl StoragePaths {
    pub fn normal() -> Result<Self, String> {
        let home = user_home()?;
        Ok(Self::from_home(home))
    }

    pub fn from_home(home: PathBuf) -> Self {
        let root = home.join(".quantix");
        let runtime = root.join("runtime");
        Self {
            connection_file: runtime.join("connection.json"),
            logs: root.join("logs"),
            tmp: runtime.join("tmp"),
            webview: runtime.join("webview"),
            root,
            runtime,
        }
    }
}

fn user_home() -> Result<PathBuf, String> {
    #[cfg(windows)]
    let value = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"));
    #[cfg(not(windows))]
    let value = std::env::var_os("HOME");

    let home = value
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| "The current user's home directory could not be resolved.".to_string())?;
    if !home.is_absolute() {
        return Err("The current user's home directory is not an absolute path.".to_string());
    }
    Ok(home)
}
