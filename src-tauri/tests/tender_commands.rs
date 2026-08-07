use std::{io, path::Path, sync::Arc};

use quantix_lib::{
    configure_tauri_builder, DeviceProtection, QuantixHost, RuntimeLayout, SetupPlatform,
    StoragePermissions, TenderCommandError, TenderErrorCode, MINIMUM_SETUP_FREE_SPACE_BYTES,
};
use tauri::test::{assert_ipc_response, mock_builder, mock_context, noop_assets, INVOKE_KEY};

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
fn renderer_cannot_start_tender_work_before_runtime_verification() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform_and_runtime(
        &application_home,
        Arc::new(ReadySetupPlatform),
        RuntimeLayout::bundled(user_home.path().join("resources")),
    );
    assert_eq!(
        host.create_tender(quantix_lib::CreateTenderCommand {
            name: "Cairo Metro Works".into(),
        }),
        Err(TenderCommandError {
            code: TenderErrorCode::RuntimeRequired,
        })
    );
    let app = configure_tauri_builder(mock_builder())
        .manage(host)
        .build(mock_context(noop_assets()))
        .expect("test Tauri application");
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("test webview");

    assert_ipc_response(
        &webview,
        request(
            "create_tender",
            serde_json::json!({ "command": { "name": "Cairo Metro Works" } }),
        ),
        Err(TenderCommandError {
            code: TenderErrorCode::RuntimeRequired,
        }),
    );
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
