use std::{io, path::Path, sync::Arc};

use quantix_lib::{
    ensure_quantix_setup, CreateTenderBackupCommand, CreateTenderCommand,
    InspectManagerWorkspaceCommand, PrepareTenderRecoveryCommand,
    PurgeRecoveryRequiredTenderCommand, PurgeTrashedTenderCommand, QuantixHost,
    RegisterTenderContentCommand, ResolveTenderRecoveryCommand, SetupPlatform, StoragePermissions,
    TenderRecoveryDecision, TrashRecoveryRequiredTenderCommand, MINIMUM_SETUP_FREE_SPACE_BYTES,
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
        "workspace" => {
            let tender_id = arguments.next().expect("Tender identity argument");
            let projection = host
                .inspect_manager_workspace(InspectManagerWorkspaceCommand {
                    tender_id: Some(tender_id.clone()),
                })
                .expect("inspect Manager workspace fixture action");
            let selected = projection
                .selected_tender
                .expect("fresh workspace selected Tender");
            assert_eq!(selected.tender_id, tender_id);
            assert!(projection
                .catalogue
                .iter()
                .any(|tender| tender.tender_id == tender_id
                    && tender.state == quantix_lib::ManagerWorkspaceTenderState::Active));
        }
        "backup" => {
            host.create_tender_backup(CreateTenderBackupCommand {
                tender_id: arguments.next().expect("Tender identity argument"),
            })
            .expect("backup Tender fixture action");
        }
        "prepare-recovery" => {
            host.prepare_tender_recovery(PrepareTenderRecoveryCommand {
                tender_id: arguments.next().expect("Tender identity argument"),
                backup_id: arguments.next().expect("backup identity argument"),
            })
            .expect("prepare Tender recovery fixture action");
        }
        "apply-recovery" => {
            host.resolve_tender_recovery(ResolveTenderRecoveryCommand {
                tender_id: arguments.next().expect("Tender identity argument"),
                recovery_id: arguments.next().expect("recovery identity argument"),
                decision: TenderRecoveryDecision::ApproveReplacement,
                rationale: "Fixture Engineer approved the verified exact replacement".into(),
            })
            .expect("apply Tender recovery fixture action");
        }
        "purge-trash" => {
            host.purge_trashed_tender(PurgeTrashedTenderCommand {
                deletion_id: arguments.next().expect("deletion identity argument"),
                rationale: "Fixture Engineer confirmed permanent deletion".into(),
                confirmation_tender_name: arguments.next().expect("Tender name argument"),
            })
            .expect("purge trashed Tender fixture action");
        }
        "purge-recovery" => {
            host.purge_recovery_required_tender(PurgeRecoveryRequiredTenderCommand {
                tender_id: arguments.next().expect("Tender identity argument"),
                rationale: "Fixture Engineer confirmed recovery Store deletion".into(),
                confirmation_tender_name: arguments.next().expect("Tender name argument"),
            })
            .expect("purge recovery-required Tender fixture action");
        }
        "trash-recovery" => {
            host.list_tenders()
                .expect("inspect damaged Tender before recovery Trash fixture action");
            host.trash_recovery_required_tender(TrashRecoveryRequiredTenderCommand {
                tender_id: arguments.next().expect("Tender identity argument"),
                rationale: "Fixture Engineer moved the recovery Store to Trash".into(),
            })
            .expect("trash recovery-required Tender fixture action");
        }
        _ => panic!("unknown storage fixture action"),
    }
}
