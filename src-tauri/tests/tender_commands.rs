use std::{io, path::Path, sync::Arc};

use quantix_lib::{
    configure_tauri_builder, DeviceProtection, QuantixHost, SetupPlatform, StoragePermissions,
    TenderSummary, MINIMUM_SETUP_FREE_SPACE_BYTES,
};
use tauri::test::{get_ipc_response, mock_builder, mock_context, noop_assets, INVOKE_KEY};

struct ReadySetupPlatform;

impl SetupPlatform for ReadySetupPlatform {
    fn available_space(&self, _path: &Path) -> io::Result<u64> {
        Ok(MINIMUM_SETUP_FREE_SPACE_BYTES)
    }

    fn is_writable(&self, _path: &Path) -> io::Result<bool> {
        Ok(true)
    }

    fn storage_permissions(&self, _path: &Path) -> io::Result<StoragePermissions> {
        Ok(StoragePermissions::Restrictive)
    }

    fn device_protection(&self, _path: &Path) -> DeviceProtection {
        DeviceProtection::Protected
    }
}

#[test]
fn engineer_can_create_and_list_a_tender_through_named_tauri_commands() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let app = configure_tauri_builder(mock_builder())
        .manage(QuantixHost::with_setup_platform(
            &application_home,
            Arc::new(ReadySetupPlatform),
        ))
        .build(mock_context(noop_assets()))
        .expect("test Tauri application");
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("test webview");

    let created = get_ipc_response(
        &webview,
        request(
            "create_tender",
            serde_json::json!({ "command": { "name": "Cairo Metro Works" } }),
        ),
    )
    .expect("create_tender response")
    .deserialize::<TenderSummary>()
    .expect("typed Tender summary");
    let listed = get_ipc_response(&webview, request("list_tenders", serde_json::json!({})))
        .expect("list_tenders response")
        .deserialize::<Vec<TenderSummary>>()
        .expect("typed Tender Catalogue");

    assert_eq!(listed, vec![created]);
}

fn request(command: &str, body: serde_json::Value) -> tauri::webview::InvokeRequest {
    tauri::webview::InvokeRequest {
        cmd: command.into(),
        callback: tauri::ipc::CallbackFn(0),
        error: tauri::ipc::CallbackFn(1),
        url: if cfg!(any(windows, target_os = "android")) {
            "http://tauri.localhost"
        } else {
            "tauri://localhost"
        }
        .parse()
        .expect("Tauri IPC URL"),
        body: tauri::ipc::InvokeBody::Json(body),
        headers: Default::default(),
        invoke_key: INVOKE_KEY.to_string(),
    }
}
