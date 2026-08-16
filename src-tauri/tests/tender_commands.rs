use std::{io, path::Path, sync::Arc};

use quantix_lib::{
    configure_tauri_builder, AgentRunRecoveryDisposition, CreateTenderBackupCommand,
    CreateTenderCommand, DeviceProtection, PrepareTenderRecoveryCommand, QuantixHost,
    RuntimeLayout, SetupPlatform, StoragePermissions, TenderCommandError, TenderErrorCode,
    TenderIntegrityIssue, TenderIntegrityReport, TenderIntegrityState, TenderRecoveryChoice,
    MINIMUM_SETUP_FREE_SPACE_BYTES,
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
fn renderer_can_inspect_verified_backups_and_recovery_offers_through_named_commands() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform_and_runtime(
        &application_home,
        Arc::new(ReadySetupPlatform),
        RuntimeLayout::bundled(user_home.path().join("resources")),
    );
    host.accept_runtime_fixture();
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Backup Recovery IPC".into(),
        })
        .expect("create Tender");
    let backup = host
        .create_tender_backup(CreateTenderBackupCommand {
            tender_id: tender.tender_id.clone(),
        })
        .expect("create verified backup");
    let offer = host
        .prepare_tender_recovery(PrepareTenderRecoveryCommand {
            tender_id: tender.tender_id.clone(),
            backup_id: backup.backup_id.clone(),
        })
        .expect("prepare recovery offer");
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
            "inspect_tender_backups",
            serde_json::json!({ "command": { "tender_id": tender.tender_id } }),
        ),
        Ok(vec![backup]),
    );
    assert_ipc_response(
        &webview,
        request(
            "inspect_tender_recoveries",
            serde_json::json!({ "command": { "tender_id": tender.tender_id } }),
        ),
        Ok(vec![offer]),
    );
    for (command, body) in [
        (
            "create_tender_backup",
            serde_json::json!({ "command": { "tender_id": "invalid" } }),
        ),
        (
            "prepare_tender_recovery",
            serde_json::json!({
                "command": { "tender_id": "invalid", "backup_id": "invalid" }
            }),
        ),
        (
            "resolve_tender_recovery",
            serde_json::json!({
                "command": {
                    "tender_id": "invalid",
                    "recovery_id": "invalid",
                    "decision": "reject",
                    "rationale": "Engineer rejected the candidate"
                }
            }),
        ),
    ] {
        assert_ipc_response(
            &webview,
            request(command, body),
            Err(TenderCommandError {
                code: TenderErrorCode::InvalidCommand,
            }),
        );
    }
}

#[test]
fn renderer_can_submit_an_engineer_recovery_disposition() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform_and_runtime(
        &application_home,
        Arc::new(ReadySetupPlatform),
        RuntimeLayout::bundled(user_home.path().join("resources")),
    );
    host.accept_runtime_fixture();
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Cairo Recovery Decision IPC".into(),
        })
        .expect("create Tender");
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
            "resolve_indeterminate_agent_run",
            serde_json::json!({
                "command": {
                    "tender_id": tender.tender_id,
                    "run_id": "00000000000000000000000000000000",
                    "disposition": AgentRunRecoveryDisposition::RetryTask,
                    "rationale": "Engineer reviewed the restart evidence."
                }
            }),
        ),
        Err(TenderCommandError {
            code: TenderErrorCode::InvalidCommand,
        }),
    );
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

#[test]
fn renderer_can_inspect_recovery_required_without_storage_authority() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let creator = QuantixHost::with_setup_platform_and_runtime(
        &application_home,
        Arc::new(ReadySetupPlatform),
        RuntimeLayout::bundled(user_home.path().join("resources")),
    );
    creator.accept_runtime_fixture();
    let tender = creator
        .create_tender(CreateTenderCommand {
            name: "Cairo Recovery IPC".into(),
        })
        .expect("create Tender");
    creator
        .close_tender(&tender.tender_id)
        .expect("close Tender");
    let database = application_home
        .join("tenders")
        .join(&tender.tender_id)
        .join("tender.sqlite");
    rusqlite::Connection::open(database)
        .expect("Tender Store database")
        .execute_batch("DROP TRIGGER audit_events_no_update")
        .expect("inject schema mismatch");

    let host = QuantixHost::with_setup_platform_and_runtime(
        &application_home,
        Arc::new(ReadySetupPlatform),
        RuntimeLayout::bundled(user_home.path().join("resources")),
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
            "inspect_tender_integrity",
            serde_json::json!({ "command": { "tender_id": tender.tender_id } }),
        ),
        Ok(TenderIntegrityReport {
            tender_id: tender.tender_id,
            state: TenderIntegrityState::RecoveryRequired,
            issues: vec![TenderIntegrityIssue::SchemaMismatch],
            recovery_choices: vec![
                TenderRecoveryChoice::RestoreVerifiedBackup,
                TenderRecoveryChoice::PurgeTender,
            ],
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
