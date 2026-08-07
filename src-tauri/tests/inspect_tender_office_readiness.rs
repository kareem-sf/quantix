use quantix_lib::{configure_tauri_builder, QuantixHost, TenderOfficeReadiness};
use tauri::test::{assert_ipc_response, mock_builder, mock_context, noop_assets, INVOKE_KEY};

#[test]
fn engineer_can_inspect_tender_office_readiness_through_tauri_ipc() {
    let application_home = tempfile::tempdir().expect("temporary application home");
    let app = configure_tauri_builder(mock_builder())
        .manage(QuantixHost::new(application_home.path()))
        .build(mock_context(noop_assets()))
        .expect("test Tauri application");
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("test webview");

    assert_ipc_response(
        &webview,
        tauri::webview::InvokeRequest {
            cmd: "inspect_tender_office_readiness".into(),
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
        Ok(TenderOfficeReadiness::ReadyForSetup),
    );
}
