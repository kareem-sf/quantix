use std::{io, path::Path, sync::Arc};

use quantix_lib::{
    ensure_quantix_setup, CreateTenderCommand, DeviceProtection, QuantixHost,
    RegisterTenderContentCommand, SetupPlatform, StoragePermissions,
    MINIMUM_SETUP_FREE_SPACE_BYTES,
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

    fn device_protection(&self, _path: &Path) -> DeviceProtection {
        DeviceProtection::Protected
    }
}

fn main() {
    let mut arguments = std::env::args().skip(1);
    let application_home = arguments.next().expect("Application Home argument");
    let action = arguments.next().expect("storage action argument");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    let _ = ensure_quantix_setup(&host);
    match action.as_str() {
        "create" => {
            host.create_tender(CreateTenderCommand {
                name: arguments.next().expect("Tender name argument"),
            })
            .expect("create Tender fixture action");
        }
        "register" => {
            let tender_id = arguments.next().expect("Tender identity argument");
            let logical_id = arguments.next().expect("logical identity argument");
            host.register_tender_content(RegisterTenderContentCommand {
                tender_id,
                logical_id: logical_id.clone(),
                media_type: "text/plain".into(),
                bytes: format!("fixture bytes for {logical_id}").into_bytes(),
            })
            .expect("register content fixture action");
        }
        "list" => {
            host.list_tenders().expect("list Tender fixture action");
        }
        _ => panic!("unknown storage fixture action"),
    }
}
