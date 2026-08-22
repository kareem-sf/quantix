use std::{
    io,
    path::Path,
    sync::{Arc, Barrier},
};

use quantix_lib::{
    ensure_quantix_setup, CreateTenderCommand, QuantixHost, RegisterTenderContentCommand,
    ReviseTenderCommand, SetupPlatform, SetupState, StoragePermissions, TenderErrorCode,
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
}

#[test]
fn engineer_can_create_close_and_reopen_a_tender_through_host_commands() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);

    let created = host
        .create_tender(CreateTenderCommand {
            name: "New Cairo Medical Centre".into(),
        })
        .expect("create Tender");
    assert_eq!(created.name, "New Cairo Medical Centre");
    assert_eq!(created.revision, 1);
    // Creation records both `tender_created` and the durable Tender AI seed.
    assert_eq!(created.audit_event_count, 2);
    assert_eq!(created.tender_id.len(), 32);
    assert!(created
        .tender_id
        .chars()
        .all(|character| character.is_ascii_lowercase() || character.is_ascii_digit()));

    host.close_tender(&created.tender_id).expect("close Tender");
    let reopened = host
        .open_tender(&created.tender_id)
        .expect("reopen Tender from its self-contained store");

    assert_eq!(reopened, created);
    let catalogue = host.list_tenders().expect("Tender Catalogue");
    assert_eq!(catalogue.len(), 1);
    assert_eq!(catalogue[0].summary.as_ref(), Some(&created));
}

#[test]
fn installation_catalogue_is_rebuilt_from_tender_store_truth() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let created = host
        .create_tender(CreateTenderCommand {
            name: "Alexandria Logistics Hub".into(),
        })
        .expect("create Tender");
    host.close_tender(&created.tender_id).expect("close Tender");

    let catalogue = rusqlite::Connection::open(application_home.join("installation.sqlite"))
        .expect("installation catalogue");
    catalogue
        .execute(
            "DELETE FROM tender_catalogue WHERE tender_id = ?1",
            [&created.tender_id],
        )
        .expect("simulate rebuildable catalogue loss");
    drop(catalogue);

    let rebuilt = host.list_tenders().expect("rebuilt Tender Catalogue");
    assert_eq!(rebuilt.len(), 1);
    assert_eq!(rebuilt[0].summary.as_ref(), Some(&created));
    let catalogue = rusqlite::Connection::open(application_home.join("installation.sqlite"))
        .expect("rebuilt installation catalogue");
    let rebuilt_name: String = catalogue
        .query_row(
            "SELECT name FROM tender_catalogue WHERE tender_id = ?1",
            [&created.tender_id],
            |row| row.get(0),
        )
        .expect("catalogue row rebuilt from Tender Store");
    assert_eq!(rebuilt_name, created.name);
}

#[test]
fn tender_mutation_commits_an_immutable_revision_and_audit_event_together() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let created = host
        .create_tender(CreateTenderCommand {
            name: "Cairo Rail Extension".into(),
        })
        .expect("create Tender");

    let revised = host
        .revise_tender(ReviseTenderCommand {
            tender_id: created.tender_id.clone(),
            name: "Cairo Rail Extension — Package A".into(),
        })
        .expect("revise Tender");

    assert_eq!(revised.tender_id, created.tender_id);
    assert_eq!(revised.name, "Cairo Rail Extension — Package A");
    assert_eq!(revised.revision, 2);
    assert_eq!(revised.audit_event_count, created.audit_event_count + 1);
    assert_ne!(revised.audit_chain_head, created.audit_chain_head);
    host.close_tender(&revised.tender_id).expect("close Tender");
    assert_eq!(
        host.open_tender(&revised.tender_id)
            .expect("reopen revised Tender"),
        revised
    );
}

#[test]
fn immutable_content_versions_share_one_verified_sha256_object() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Suez Industrial Utilities".into(),
        })
        .expect("create Tender");
    let command = RegisterTenderContentCommand {
        tender_id: tender.tender_id.clone(),
        logical_id: "tender-brief".into(),
        media_type: "text/plain".into(),
        bytes: b"Quantix evidence v1".to_vec(),
    };

    let first = host
        .register_tender_content(command.clone())
        .expect("register first immutable content version");
    assert_eq!(first.logical_id, "tender-brief");
    assert_eq!(first.revision, 1);
    assert_eq!(first.size_bytes, 19);
    assert_eq!(
        first.sha256,
        "2ac916e71f8dad1572b90c3927939a81cba2aabd07c33d780f4480aa49e2fdf3"
    );

    let second = host
        .register_tender_content(command)
        .expect("register second immutable content version");
    assert_eq!(second.revision, 2);
    assert_eq!(second.sha256, first.sha256);
    let inspection = host
        .inspect_tender(&tender.tender_id)
        .expect("inspect canonical Tender state");
    assert_eq!(inspection.content_object_count, 1);
    assert_eq!(inspection.content_version_count, 2);
    assert_eq!(
        inspection.summary.audit_event_count,
        tender.audit_event_count + 2
    );
}

#[test]
fn failed_content_publication_is_audited_without_advancing_a_logical_pointer() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Upper Egypt Water Treatment".into(),
        })
        .expect("create Tender");
    host.open_tender(&tender.tender_id)
        .expect("open verified Tender Store before publication attempt");
    let content_root = application_home
        .join("tenders")
        .join(&tender.tender_id)
        .join("content");
    std::fs::remove_dir(&content_root).expect("remove empty content directory");
    std::fs::write(&content_root, b"block content publication")
        .expect("replace content directory with a file");

    let result = host.register_tender_content(RegisterTenderContentCommand {
        tender_id: tender.tender_id.clone(),
        logical_id: "failed-publication".into(),
        media_type: "text/plain".into(),
        bytes: b"must never become canonical".to_vec(),
    });

    assert!(result.is_err());
    let inspection = host
        .inspect_tender(&tender.tender_id)
        .expect("inspect unchanged canonical Tender state");
    assert_eq!(inspection.content_object_count, 0);
    assert_eq!(inspection.content_version_count, 0);
    assert_eq!(
        inspection.summary.audit_event_count,
        tender.audit_event_count + 1
    );
    assert_ne!(inspection.summary.audit_chain_head, tender.audit_chain_head);
}

#[test]
fn sqlite_rejects_audit_event_update_and_deletion() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Red Sea Port Expansion".into(),
        })
        .expect("create Tender");
    host.close_tender(&tender.tender_id).expect("close Tender");
    let database = application_home
        .join("tenders")
        .join(&tender.tender_id)
        .join("tender.sqlite");
    let connection = rusqlite::Connection::open(database).expect("Tender Store database");

    assert!(connection
        .execute(
            "UPDATE audit_events SET event_type = 'rewritten' WHERE sequence = 1",
            [],
        )
        .is_err());
    assert!(connection
        .execute("DELETE FROM audit_events WHERE sequence = 1", [])
        .is_err());
    drop(connection);

    assert_eq!(
        host.open_tender(&tender.tender_id)
            .expect("reopen Tender with intact Audit Event"),
        tender
    );
}

#[test]
fn altered_tender_store_schema_fails_closed_on_reopen() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "North Coast Desalination".into(),
        })
        .expect("create Tender");
    host.close_tender(&tender.tender_id).expect("close Tender");
    let database = application_home
        .join("tenders")
        .join(&tender.tender_id)
        .join("tender.sqlite");
    let connection = rusqlite::Connection::open(database).expect("Tender Store database");
    connection
        .execute_batch("DROP TRIGGER audit_events_no_update;")
        .expect("alter exact Tender Store schema");
    drop(connection);

    let error = host
        .open_tender(&tender.tender_id)
        .expect_err("altered schema must fail closed");
    assert_eq!(error.code, TenderErrorCode::RecoveryRequired);
}

#[test]
fn canonical_tender_revisions_reject_update_and_deletion() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Giza Healthcare Campus".into(),
        })
        .expect("create Tender");
    host.close_tender(&tender.tender_id).expect("close Tender");
    let database = application_home
        .join("tenders")
        .join(&tender.tender_id)
        .join("tender.sqlite");
    let connection = rusqlite::Connection::open(database).expect("Tender Store database");

    assert!(connection
        .execute(
            "UPDATE tender_revisions SET name = 'rewritten' WHERE revision = 1",
            [],
        )
        .is_err());
    assert!(connection
        .execute("DELETE FROM tender_revisions WHERE revision = 1", [])
        .is_err());
}

#[test]
fn host_serializes_concurrent_tender_mutations_through_one_writer() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Nile Valley Infrastructure".into(),
        })
        .expect("create Tender");

    let mutations = (0..8)
        .map(|index| {
            let host = host.clone();
            let tender_id = tender.tender_id.clone();
            std::thread::spawn(move || {
                host.revise_tender(ReviseTenderCommand {
                    tender_id,
                    name: format!("Nile Valley Infrastructure revision {index}"),
                })
            })
        })
        .collect::<Vec<_>>();
    let mutation_count = mutations.len() as u64;
    for mutation in mutations {
        mutation
            .join()
            .expect("mutation thread")
            .expect("serialized Tender mutation");
    }

    let inspection = host
        .inspect_tender(&tender.tender_id)
        .expect("inspect serialized Tender mutations");
    assert_eq!(inspection.summary.revision, 9);
    assert_eq!(
        inspection.summary.audit_event_count,
        tender.audit_event_count + mutation_count
    );
}

#[test]
fn host_serializes_concurrent_cold_opens_through_one_writer() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Cairo Metro Systems Package".into(),
        })
        .expect("create Tender");
    host.close_tender(&tender.tender_id).expect("close Tender");

    let barrier = Arc::new(Barrier::new(8));
    let opens = (0..8)
        .map(|_| {
            let host = host.clone();
            let tender_id = tender.tender_id.clone();
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                host.open_tender(&tender_id)
            })
        })
        .collect::<Vec<_>>();
    for open in opens {
        assert_eq!(
            open.join()
                .expect("open thread")
                .expect("serialized cold open"),
            tender
        );
    }
}

#[test]
fn requested_directory_identity_must_match_the_immutable_tender_identity() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Delta Utilities Package".into(),
        })
        .expect("create Tender");
    host.close_tender(&tender.tender_id).expect("close Tender");

    let conflicting_id = if tender.tender_id == "00000000000000000000000000000000" {
        "11111111111111111111111111111111"
    } else {
        "00000000000000000000000000000000"
    };
    std::fs::rename(
        application_home.join("tenders").join(&tender.tender_id),
        application_home.join("tenders").join(conflicting_id),
    )
    .expect("place store beneath a conflicting directory identity");

    let error = host
        .open_tender(conflicting_id)
        .expect_err("directory identity mismatch must fail closed");
    assert_eq!(error.code, TenderErrorCode::RecoveryRequired);
}

#[test]
fn linked_tender_root_outside_application_home_fails_closed() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Sinai Infrastructure Package".into(),
        })
        .expect("create Tender");
    host.close_tender(&tender.tender_id).expect("close Tender");

    let tender_root = application_home.join("tenders").join(&tender.tender_id);
    let outside_root = user_home.path().join("outside-quantix");
    std::fs::rename(&tender_root, &outside_root).expect("move store outside application home");
    create_directory_link(&outside_root, &tender_root).expect("create linked Tender root");

    let error = host
        .open_tender(&tender.tender_id)
        .expect_err("linked Tender root must fail closed");
    assert_eq!(error.code, TenderErrorCode::RecoveryRequired);
    let catalogue = host
        .list_tenders()
        .expect("unsafe Tender must not hide the healthy catalogue boundary");
    assert_eq!(catalogue.len(), 1);
    assert!(catalogue[0].summary.is_none());
    assert_eq!(
        catalogue[0].integrity.issues,
        vec![quantix_lib::TenderIntegrityIssue::StorageLayoutInvalid]
    );
}

#[cfg(unix)]
fn create_directory_link(target: &Path, link: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn create_directory_link(target: &Path, link: &Path) -> io::Result<()> {
    junction::create(target, link)
}

#[test]
fn invalid_bounded_commands_publish_no_canonical_state() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);

    let error = host
        .create_tender(CreateTenderCommand {
            name: "x".repeat(201),
        })
        .expect_err("oversized Tender name");
    assert_eq!(error.code, TenderErrorCode::InvalidCommand);
    assert!(host
        .list_tenders()
        .expect("empty Tender Catalogue")
        .is_empty());
}

#[test]
fn invalid_targeted_commands_are_audited_without_changing_canonical_records() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "New Administrative Capital Package".into(),
        })
        .expect("create Tender");

    let revision_error = host
        .revise_tender(ReviseTenderCommand {
            tender_id: tender.tender_id.clone(),
            name: "   ".into(),
        })
        .expect_err("blank revision name must be denied");
    assert_eq!(revision_error.code, TenderErrorCode::InvalidCommand);

    let content_error = host
        .register_tender_content(RegisterTenderContentCommand {
            tender_id: tender.tender_id.clone(),
            logical_id: "unsafe/logical/id".into(),
            media_type: "text/plain".into(),
            bytes: b"must not be registered".to_vec(),
        })
        .expect_err("unsafe logical identity must be denied");
    assert_eq!(content_error.code, TenderErrorCode::InvalidCommand);

    let inspection = host
        .inspect_tender(&tender.tender_id)
        .expect("inspect unchanged canonical records and denial history");
    assert_eq!(inspection.summary.revision, 1);
    assert_eq!(inspection.summary.name, tender.name);
    assert_eq!(inspection.content_object_count, 0);
    assert_eq!(inspection.content_version_count, 0);
    assert_eq!(
        inspection.summary.audit_event_count,
        tender.audit_event_count + 2
    );
    assert_ne!(inspection.summary.audit_chain_head, tender.audit_chain_head);
}
