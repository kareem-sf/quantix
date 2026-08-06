mod host;

pub use host::{
    inspect_quantix_host, HostCommandInterface, HostConnectionState, HostRuntime, QuantixHost,
    QuantixHostStatus, RendererAssetSource,
};

use tauri::Manager;

mod tauri_commands {
    use super::{inspect_quantix_host as inspect_host, QuantixHost, QuantixHostStatus};

    #[tauri::command]
    pub(super) fn inspect_quantix_host(host: tauri::State<'_, QuantixHost>) -> QuantixHostStatus {
        inspect_host(host.inner())
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let application_home = app.path().home_dir()?.join(".quantix");
            app.manage(QuantixHost::new(application_home));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            tauri_commands::inspect_quantix_host
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
