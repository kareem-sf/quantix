#![windows_subsystem = "windows"]

use serde::{Deserialize, Serialize};
use tauri::Manager;
use tauri_plugin_dialog::DialogExt;

#[derive(Clone, Deserialize, Serialize)]
struct ConnectionInfo {
    base_url: String,
    token: String,
}

#[tauri::command]
fn connection_info() -> Result<ConnectionInfo, String> {
    let path = std::env::var_os("QUANTIX_CONNECTION_FILE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../.quantix-dev/connection.json")
        });
    let bytes = std::fs::read(path)
        .map_err(|_| "The local workspace is not running. Start Quantix using its launcher.".to_string())?;
    let connection: ConnectionInfo = serde_json::from_slice(&bytes)
        .map_err(|_| "The local connection could not be read. Restart Quantix.".to_string())?;
    if !connection.base_url.starts_with("http://127.0.0.1:") || connection.token.len() < 32 {
        return Err("The local connection is invalid. Restart Quantix.".into());
    }
    Ok(connection)
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
fn close_quantix(app: tauri::AppHandle) {
    app.exit(0);
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![connection_info, choose_package, close_quantix])
        .run(tauri::generate_context!())
        .expect("Quantix could not open its desktop window");
}
