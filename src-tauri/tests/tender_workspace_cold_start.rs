use std::{io, path::Path, process::Command, sync::Arc};

use quantix_lib::{
    ensure_quantix_setup, CreateTenderCommand, QuantixHost, SetupPlatform, SetupState,
    StoragePermissions, MINIMUM_SETUP_FREE_SPACE_BYTES,
};

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
fn fresh_process_workspace_refresh_accepts_healthy_tender_store() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Cold Start Workspace Fixture".into(),
        })
        .expect("create healthy Tender Store");
    drop(host);

    let output = Command::new(env!("CARGO_BIN_EXE_quantix-storage-fixture"))
        .args([
            application_home.to_str().expect("UTF-8 application home"),
            "workspace",
            &tender.tender_id,
        ])
        .output()
        .expect("run fresh-process workspace fixture");
    assert!(
        output.status.success(),
        "fresh-process workspace refresh failed: status={}\nstdout={}\nstderr={}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
