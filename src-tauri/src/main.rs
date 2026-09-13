#![windows_subsystem = "windows"]

use serde_json::json;
use std::sync::Arc;
use tauri::Manager;
use tauri::WebviewWindowBuilder;
use tauri_plugin_dialog::DialogExt;
mod diagnostics;
mod paths;
mod service;
mod external;
mod reset;
mod splash;
mod tray;

#[tauri::command]
fn connection_info(app: tauri::AppHandle) -> Result<service::ConnectionInfo, String> {
    service::connection(&app)
}

#[tauri::command]
async fn choose_package(app: tauri::AppHandle, kind: String) -> Result<Option<String>, String> {
    if kind != "directory" && kind != "zip" {
        return Err("Choose a folder or ZIP archive.".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let dialog = app.dialog().file().set_title("Choose a Tender package");
        let selected = if kind == "directory" {
            dialog.blocking_pick_folder()
        } else {
            dialog.add_filter("ZIP archive", &["zip"]).blocking_pick_file()
        };
        selected
            .map(|file| file.into_path().map(|path| path.to_string_lossy().into_owned())
                .map_err(|_| "Choose a file stored on this computer.".to_string()))
            .transpose()
    })
    .await
    .map_err(|_| "The file chooser could not be opened.".to_string())?
}

#[tauri::command]
async fn close_quantix(app: tauri::AppHandle) -> Result<(), String> {
    let started = app
        .try_state::<service::ShutdownState>()
        .ok_or_else(|| tray::SHUTDOWN_RETRY_MESSAGE.to_string())?
        .try_begin();
    if !started {
        return Err(tray::SHUTDOWN_RETRY_MESSAGE.to_string());
    }
    let worker_app = app.clone();
    let result = match tauri::async_runtime::spawn_blocking(move || service::shutdown_for_exit(&worker_app)).await {
        Ok(result) => result,
        Err(_) => Err(tray::SHUTDOWN_RETRY_MESSAGE.to_string()),
    };
    if result.is_err() {
        if let Some(state) = app.try_state::<service::ShutdownState>() {
            state.finish();
        }
        return Err(tray::SHUTDOWN_RETRY_MESSAGE.to_string());
    }
    app.exit(0);
    if let Some(state) = app.try_state::<service::ShutdownState>() {
        state.finish();
    }
    Ok(())
}

fn main() {
    if let Some(code) = reset::before_startup() {
        std::process::exit(code);
    }
    let logger = diagnostics::DiagnosticLogger::new("desktop");
    let panic_logger = logger.clone();
    std::panic::set_hook(Box::new(move |panic| {
        let mut fields = json!({"phase":"native_runtime"});
        if let Some(location) = panic.location() {
            fields["line"] = json!(location.line());
            fields["column"] = json!(location.column());
        }
        panic_logger.record("panic", "error", fields);
    }));
    logger.record("native_startup", "info", json!({"phase":"native_runtime", "outcome":"starting"}));
    let mut builder = tauri::Builder::default()
        .manage(logger.clone())
        .manage(service::ShutdownState::default())
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            let _ = tray::restore_main_window(app);
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::Builder::new().open_js_links_on_click(false).build());
    builder = builder.setup(|app| {
        let logger = app.state::<Arc<diagnostics::DiagnosticLogger>>().inner().clone();
        logger.record("native_setup", "info", json!({"phase":"native_setup", "outcome":"started"}));
        let window_config = app
            .config()
            .app
            .windows
            .first()
            .cloned()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "Quantix's main window configuration is missing."))?;
        #[cfg(any(target_os = "windows", target_os = "linux"))]
        let webview_directory = Some(paths::StoragePaths::normal().map_err(std::io::Error::other)?.webview);
        #[cfg(not(any(target_os = "windows", target_os = "linux")))]
        let webview_directory: Option<std::path::PathBuf> = None;
        // The main window starts hidden; the splash reveals it once the
        // interface is ready. Without a splash, show it straight away.
        let splash_open = match splash::open(app.handle(), webview_directory.clone()) {
            Ok(()) => true,
            Err(_) => {
                logger.record("native_splash_unavailable", "info", json!({"phase":"native_setup", "outcome":"skipped"}));
                false
            }
        };
        let window_builder = WebviewWindowBuilder::from_config(app.handle(), &window_config)?;
        #[cfg(any(target_os = "windows", target_os = "linux"))]
        let window_builder = match webview_directory {
            Some(directory) => window_builder.data_directory(directory),
            None => window_builder,
        };
        #[cfg(not(any(target_os = "windows", target_os = "linux")))]
        logger.record(
            "webview_storage_unavailable",
            "info",
            json!({"phase":"native_setup", "outcome":"platform_unsupported"}),
        );
        let window = window_builder.build()?;
        if !splash_open {
            let _ = window.show();
        }
        logger.record("native_window_ready", "info", json!({"phase":"native_window", "outcome":"ready"}));
        let tray_ready = match tray::initialize_tray(app.handle()) {
            Ok(()) => {
                logger.record("native_tray_ready", "info", json!({"phase":"native_tray", "outcome":"ready"}));
                true
            }
            Err(error) => {
                logger.record_failure("native_tray_init_failed", "tray_setup", "tray_error", None);
                logger.record("native_tray_unavailable", "error", json!({
                    "phase":"native_tray",
                    "outcome":"unavailable",
                    "detail":"The system tray is unavailable. Keep this window open while work is active."
                }));
                let _ = window.set_title("Quantix — system tray unavailable; keep this window open");
                let _ = error;
                false
            }
        };
        tray::install_window_close_handler(&window, tray_ready);
        #[cfg(not(debug_assertions))]
        if let Err(error) = service::start(app.handle()) {
            logger.record_error("native_setup_failed", "service_start", error.as_ref());
            return Err(error);
        }
        logger.record("native_setup", "info", json!({"phase":"native_setup", "outcome":"ready"}));
        Ok(())
    });
    builder
        .invoke_handler(tauri::generate_handler![connection_info, choose_package, close_quantix, external::open_external_url, external::prepare_sign_in_browser, reset::reset_support, reset::finish_reset, splash::finish_splash])
        .build(tauri::generate_context!())
        .expect("Quantix could not open its desktop window")
        .run(|app, event| {
            if let Some(logger) = app.try_state::<Arc<diagnostics::DiagnosticLogger>>().map(|state| state.inner().clone()) {
                if matches!(event, tauri::RunEvent::ExitRequested { .. }) {
                    logger.record("native_exit_requested", "info", json!({"phase":"native_runtime", "outcome":"requested"}));
                    // Explicit tray Quit and OS exit share the bounded service
                    // shutdown path; ordinary window close is intercepted
                    // earlier and never reaches this branch.
                    service::stop(app);
                }
                if matches!(event, tauri::RunEvent::Exit) {
                    logger.record("native_exit", "info", json!({"phase":"native_runtime", "outcome":"completed"}));
                }
            }
        });
    logger.record("native_process_exit", "info", json!({"phase":"native_runtime", "outcome":"completed"}));
}
