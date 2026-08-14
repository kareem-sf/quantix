use std::{io, path::Path, sync::Arc};

use quantix_lib::{
    ensure_quantix_setup, CreateTenderCommand, DeviceProtection, InspectManagerWorkspaceCommand,
    QuantixHost, RecordEngineerWorkspaceMessageCommand, SelectManagerWorkspaceTenderCommand,
    SetupPlatform, SetupState, StoragePermissions, TenderOfficeMessageAuthor,
    TenderOfficeMessageKind, WorkspaceActionKind, MINIMUM_SETUP_FREE_SPACE_BYTES,
};
use rusqlite::Connection;

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
fn public_host_projection_resumes_selection_and_persists_the_manager_conversation() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let empty = host
        .inspect_manager_workspace(InspectManagerWorkspaceCommand::default())
        .expect("inspect empty Manager workspace");
    assert!(empty.selected_tender.is_none());
    assert!(empty.conversation.is_none());
    assert_eq!(empty.current_action.kind, WorkspaceActionKind::StartTender);
    assert_eq!(empty.current_action.action_label, "Choose Tender Package");

    let first = host
        .create_tender(CreateTenderCommand {
            name: "First Tender".into(),
        })
        .expect("create first Tender");
    let second = host
        .create_tender(CreateTenderCommand {
            name: "Second Tender".into(),
        })
        .expect("create second Tender");
    host.select_manager_workspace_tender(SelectManagerWorkspaceTenderCommand {
        tender_id: second.tender_id.clone(),
    })
    .expect("select second Tender");

    let resumed = host
        .inspect_manager_workspace(InspectManagerWorkspaceCommand::default())
        .expect("inspect Manager workspace");
    assert_eq!(resumed.catalogue.len(), 2);
    assert_eq!(
        resumed
            .selected_tender
            .as_ref()
            .expect("last active Tender")
            .tender_id,
        second.tender_id
    );
    assert_eq!(
        resumed.current_action.kind,
        WorkspaceActionKind::AddTenderPackage
    );
    assert_eq!(resumed.work.needs_engineer, 0);
    assert_eq!(resumed.work.cancelled, 0);
    assert_eq!(resumed.files.tender_document_count, 0);
    assert_eq!(resumed.team.active_agent_runs, 0);

    host.select_manager_workspace_tender(SelectManagerWorkspaceTenderCommand {
        tender_id: first.tender_id.clone(),
    })
    .expect("select first Tender");
    let reopened = host
        .inspect_manager_workspace(InspectManagerWorkspaceCommand::default())
        .expect("resume selected Tender");
    assert_eq!(
        reopened
            .selected_tender
            .as_ref()
            .expect("selected Tender")
            .tender_id,
        first.tender_id
    );

    let updated = host
        .record_engineer_workspace_message(RecordEngineerWorkspaceMessageCommand {
            tender_id: first.tender_id,
            body: "Check the insurance exclusions first.".into(),
        })
        .expect("record Engineer message");
    let conversation = updated.conversation.expect("durable Manager conversation");
    assert_eq!(
        conversation.messages.first().expect("system status").author,
        TenderOfficeMessageAuthor::System
    );
    let message = conversation.messages.last().expect("Engineer message");
    assert_eq!(message.author, TenderOfficeMessageAuthor::Engineer);
    assert_eq!(message.kind, TenderOfficeMessageKind::Routine);
    assert_eq!(message.body, "Check the insurance exclusions first.");
}

#[test]
fn selection_failure_cannot_follow_a_committed_engineer_message() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Failure Boundary Tender".into(),
        })
        .expect("create Tender");
    host.select_manager_workspace_tender(SelectManagerWorkspaceTenderCommand {
        tender_id: tender.tender_id.clone(),
    })
    .expect("establish selection");
    let before = host
        .inspect_manager_workspace(InspectManagerWorkspaceCommand {
            tender_id: Some(tender.tender_id.clone()),
        })
        .expect("inspect conversation")
        .conversation
        .expect("conversation")
        .messages
        .len();

    let connection = Connection::open(application_home.join("installation.sqlite"))
        .expect("open installation catalogue");
    connection
        .execute_batch(
            "CREATE TRIGGER manager_workspace_selection_test_failure
             BEFORE UPDATE ON manager_workspace_selection
             BEGIN SELECT RAISE(ABORT, 'injected selection failure'); END;",
        )
        .expect("inject selection failure");
    assert!(host
        .record_engineer_workspace_message(RecordEngineerWorkspaceMessageCommand {
            tender_id: tender.tender_id.clone(),
            body: "Do not record this message.".into(),
        })
        .is_err());
    connection
        .execute_batch("DROP TRIGGER manager_workspace_selection_test_failure;")
        .expect("remove selection failure");

    let after = host
        .inspect_manager_workspace(InspectManagerWorkspaceCommand {
            tender_id: Some(tender.tender_id),
        })
        .expect("inspect conversation after failure")
        .conversation
        .expect("conversation")
        .messages
        .len();
    assert_eq!(after, before);
}
