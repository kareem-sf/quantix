mod host;
mod setup;
mod tender_store;

pub use host::QuantixHost;
pub use setup::{
    ensure_quantix_setup, DeviceProtection, SetupIssue, SetupOutcome, SetupPlatform, SetupState,
    StoragePermissions, MINIMUM_SETUP_FREE_SPACE_BYTES,
};
pub use tender_store::{
    ContentVersionSummary, CreateTenderCommand, OpenTenderCommand, RegisterTenderContentCommand,
    ReviseTenderCommand, TenderCommandError, TenderErrorCode, TenderInspection, TenderSummary,
};

use tauri::Manager;

mod tauri_commands {
    use super::{
        ensure_quantix_setup as ensure_setup, CreateTenderCommand, OpenTenderCommand, QuantixHost,
        ReviseTenderCommand, SetupOutcome, TenderCommandError, TenderErrorCode, TenderSummary,
    };

    #[tauri::command]
    pub(super) async fn ensure_quantix_setup(
        host: tauri::State<'_, QuantixHost>,
    ) -> Result<SetupOutcome, &'static str> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || ensure_setup(&host))
            .await
            .map_err(|_| "Quantix Setup stopped unexpectedly")
    }

    #[tauri::command]
    pub(super) async fn create_tender(
        host: tauri::State<'_, QuantixHost>,
        command: CreateTenderCommand,
    ) -> Result<TenderSummary, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.create_tender(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn list_tenders(
        host: tauri::State<'_, QuantixHost>,
    ) -> Result<Vec<TenderSummary>, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.list_tenders())
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn open_tender(
        host: tauri::State<'_, QuantixHost>,
        command: OpenTenderCommand,
    ) -> Result<TenderSummary, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.open_tender(&command.tender_id))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn revise_tender(
        host: tauri::State<'_, QuantixHost>,
        command: ReviseTenderCommand,
    ) -> Result<TenderSummary, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.revise_tender(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }
}

pub fn configure_tauri_builder<R: tauri::Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder.invoke_handler(tauri::generate_handler![
        tauri_commands::ensure_quantix_setup,
        tauri_commands::create_tender,
        tauri_commands::list_tenders,
        tauri_commands::open_tender,
        tauri_commands::revise_tender
    ])
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    configure_tauri_builder(tauri::Builder::default())
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .setup(|app| {
            let application_home = app.path().home_dir()?.join(".quantix");
            app.manage(QuantixHost::new(application_home));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
