use std::{io, path::Path, sync::Arc};

use quantix_lib::{
    configure_tauri_builder, QuantixHost, SetupOutcome, SetupPlatform, SetupState,
    StoragePermissions, MINIMUM_SETUP_FREE_SPACE_BYTES,
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
}

#[test]
fn engineer_can_complete_quantix_setup_through_tauri_ipc() {
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

    assert_ipc_response(
        &webview,
        tauri::webview::InvokeRequest {
            cmd: "ensure_quantix_setup".into(),
            callback: tauri::ipc::CallbackFn(0),
            error: tauri::ipc::CallbackFn(1),
            url: if cfg!(any(windows, target_os = "android")) {
                "http://tauri.localhost"
            } else {
                "tauri://localhost"
            }
            .parse()
            .expect("Tauri IPC URL"),
            body: tauri::ipc::InvokeBody::default(),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        },
        Ok(SetupOutcome {
            state: SetupState::Ready,
            setup_performed: true,
            issues: Vec::new(),
        }),
    );
}
