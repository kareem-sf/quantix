use std::io::{Seek, SeekFrom, Write};
use std::{
    io,
    path::Path,
    process::Command,
    sync::{Arc, Barrier},
    thread,
    time::Duration,
};

use quantix_lib::{
    ensure_quantix_setup, CreateTenderCommand, DeviceProtection, QuantixHost,
    RegisterTenderContentCommand, ReviseTenderCommand, SetupPlatform, SetupState,
    StoragePermissions, TenderErrorCode, TenderIntegrityIssue, TenderIntegrityState,
    TenderRecoveryChoice, MINIMUM_SETUP_FREE_SPACE_BYTES,
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

#[test]
fn missing_referenced_content_puts_the_tender_in_recovery_required() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "New Capital Utilities".into(),
        })
        .expect("create Tender");
    host.register_tender_content(RegisterTenderContentCommand {
        tender_id: tender.tender_id.clone(),
        logical_id: "employer-requirements".into(),
        media_type: "text/plain".into(),
        bytes: b"immutable contractual requirements".to_vec(),
    })
    .expect("register canonical content");
    host.close_tender(&tender.tender_id).expect("close Tender");

    let tender_root = application_home.join("tenders").join(&tender.tender_id);
    let connection = rusqlite::Connection::open(tender_root.join("tender.sqlite"))
        .expect("Tender Store database");
    let integrity: String = connection
        .query_row("SELECT integrity FROM content_objects", [], |row| {
            row.get(0)
        })
        .expect("registered content integrity");
    drop(connection);
    cacache::remove_hash_sync(
        tender_root.join("content"),
        &integrity.parse().expect("valid stored integrity"),
    )
    .expect("remove referenced content bytes");

    let report = host
        .inspect_tender_integrity(&tender.tender_id)
        .expect("inspect missing referenced content");
    assert_eq!(report.state, TenderIntegrityState::RecoveryRequired);
    assert_eq!(
        report.issues,
        vec![TenderIntegrityIssue::ReferencedContentMissing]
    );
    assert_eq!(
        host.open_tender(&tender.tender_id)
            .expect_err("Tender with missing canonical bytes cannot open")
            .code,
        TenderErrorCode::RecoveryRequired
    );
}

#[test]
fn altered_tender_store_schema_is_reported_as_recovery_evidence() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Suez Logistics Zone".into(),
        })
        .expect("create Tender");
    host.close_tender(&tender.tender_id).expect("close Tender");

    let database = application_home
        .join("tenders")
        .join(&tender.tender_id)
        .join("tender.sqlite");
    rusqlite::Connection::open(database)
        .expect("Tender Store database")
        .execute_batch("DROP TRIGGER audit_events_no_update")
        .expect("alter exact Tender Store schema");

    let report = host
        .inspect_tender_integrity(&tender.tender_id)
        .expect("inspect schema recovery evidence");
    assert_eq!(report.state, TenderIntegrityState::RecoveryRequired);
    assert_eq!(report.issues, vec![TenderIntegrityIssue::SchemaMismatch]);
    assert_eq!(
        host.open_tender(&tender.tender_id)
            .expect_err("altered schema cannot open for work")
            .code,
        TenderErrorCode::RecoveryRequired
    );
}

#[test]
fn cold_open_removes_only_a_proven_uncommitted_parse_candidate() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Alexandria Marine Terminal".into(),
        })
        .expect("create Tender");
    host.close_tender(&tender.tender_id).expect("close Tender");

    let orphan = application_home
        .join("tenders")
        .join(&tender.tender_id)
        .join("staging")
        .join("parse-11111111111111111111111111111111");
    std::fs::create_dir_all(orphan.join("candidate")).expect("stage interrupted candidate");
    std::fs::write(orphan.join("candidate").join("partial.json"), b"partial")
        .expect("write uncommitted candidate");

    let reopened = host
        .open_tender(&tender.tender_id)
        .expect("reconcile before accepting Tender work");
    assert_eq!(reopened, tender);
    assert!(!orphan.exists());
}

#[test]
fn altered_audit_payload_puts_the_tender_in_read_only_recovery_required() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Cairo Airport Enabling Works".into(),
        })
        .expect("create Tender");
    host.close_tender(&tender.tender_id).expect("close Tender");

    let database = application_home
        .join("tenders")
        .join(&tender.tender_id)
        .join("tender.sqlite");
    let connection = rusqlite::Connection::open(database).expect("Tender Store database");
    let trigger_sql: String = connection
        .query_row(
            "SELECT sql FROM sqlite_schema WHERE type = 'trigger' AND name = 'audit_events_no_update'",
            [],
            |row| row.get(0),
        )
        .expect("immutable Audit Event trigger");
    connection
        .execute_batch("DROP TRIGGER audit_events_no_update")
        .expect("inject Audit Event mutation capability");
    connection
        .execute(
            "UPDATE audit_events SET payload_json = '{}' WHERE sequence = 1",
            [],
        )
        .expect("alter one Audit Event payload");
    connection
        .execute_batch(&trigger_sql)
        .expect("restore exact Audit Event trigger");
    drop(connection);

    let integrity = host
        .inspect_tender_integrity(&tender.tender_id)
        .expect("inspect Recovery Required evidence");
    assert_eq!(integrity.state, TenderIntegrityState::RecoveryRequired);
    assert_eq!(
        integrity.issues,
        vec![TenderIntegrityIssue::AuditChainInvalid]
    );
    assert_eq!(
        integrity.recovery_choices,
        vec![
            TenderRecoveryChoice::RestoreVerifiedBackup,
            TenderRecoveryChoice::PurgeTender,
        ]
    );

    let open_error = host
        .open_tender(&tender.tender_id)
        .expect_err("Recovery Required Tender cannot open for work");
    assert_eq!(open_error.code, TenderErrorCode::RecoveryRequired);
    let mutation_error = host
        .revise_tender(ReviseTenderCommand {
            tender_id: tender.tender_id,
            name: "Must not publish".into(),
        })
        .expect_err("Recovery Required Tender is read-only");
    assert_eq!(mutation_error.code, TenderErrorCode::RecoveryRequired);
}

#[test]
fn catalogue_keeps_healthy_tenders_available_and_surfaces_recovery_evidence() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let healthy = host
        .create_tender(CreateTenderCommand {
            name: "Healthy Tender".into(),
        })
        .expect("create healthy Tender");
    let damaged = host
        .create_tender(CreateTenderCommand {
            name: "Damaged Tender".into(),
        })
        .expect("create Tender to damage");
    host.close_tender(&healthy.tender_id)
        .expect("close healthy Tender");
    host.close_tender(&damaged.tender_id)
        .expect("close damaged Tender");
    rusqlite::Connection::open(
        application_home
            .join("tenders")
            .join(&damaged.tender_id)
            .join("tender.sqlite"),
    )
    .expect("damaged Tender Store")
    .execute_batch("DROP TRIGGER audit_events_no_update")
    .expect("inject schema damage");

    let catalogue = host
        .list_tenders()
        .expect("one damaged Tender must not hide the catalogue");
    let healthy_entry = catalogue
        .iter()
        .find(|entry| entry.tender_id == healthy.tender_id)
        .expect("healthy Tender entry");
    assert_eq!(healthy_entry.summary.as_ref(), Some(&healthy));
    assert_eq!(healthy_entry.integrity.state, TenderIntegrityState::Ready);
    let damaged_entry = catalogue
        .iter()
        .find(|entry| entry.tender_id == damaged.tender_id)
        .expect("damaged Tender entry");
    assert!(damaged_entry.summary.is_none());
    assert_eq!(
        damaged_entry.integrity.issues,
        vec![TenderIntegrityIssue::SchemaMismatch]
    );
}

#[test]
fn corrupt_sqlite_pages_are_reported_as_recovery_evidence() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Damaged SQLite Tender".into(),
        })
        .expect("create Tender");
    host.close_tender(&tender.tender_id).expect("close Tender");
    let database = application_home
        .join("tenders")
        .join(&tender.tender_id)
        .join("tender.sqlite");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .open(database)
        .expect("open database for failure injection");
    file.seek(SeekFrom::Start(0)).expect("seek database header");
    file.write_all(&[0_u8; 128]).expect("corrupt SQLite header");
    file.sync_all().expect("persist injected corruption");
    drop(file);

    let report = host
        .inspect_tender_integrity(&tender.tender_id)
        .expect("inspect corrupt SQLite evidence");
    assert_eq!(report.state, TenderIntegrityState::RecoveryRequired);
    assert_eq!(
        report.issues,
        vec![TenderIntegrityIssue::DatabaseIntegrityInvalid]
    );
    assert_eq!(
        host.open_tender(&tender.tender_id)
            .expect_err("corrupt SQLite database cannot open")
            .code,
        TenderErrorCode::RecoveryRequired
    );
}

#[test]
fn startup_removes_an_abandoned_unpublished_tender_candidate_before_work() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let staged = application_home
        .join("staging")
        .join("tender-11111111111111111111111111111111");
    std::fs::create_dir(&staged).expect("stage interrupted Tender candidate");
    std::fs::write(staged.join("partial"), b"not committed").expect("write interrupted candidate");

    let committed = host
        .create_tender(CreateTenderCommand {
            name: "Committed After Restart".into(),
        })
        .expect("reconcile before accepting new Tender work");

    assert_eq!(
        host.inspect_startup_reconciliation()
            .removed_tender_candidates,
        1
    );
    assert_eq!(
        host.open_tender(&committed.tender_id)
            .expect("committed Tender remains intact"),
        committed
    );
}

#[test]
fn failed_integrity_inspection_latches_an_open_tender_read_only() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Open Tender Before Damage".into(),
        })
        .expect("create Tender");
    host.open_tender(&tender.tender_id)
        .expect("cache verified Tender writer");
    let database = application_home
        .join("tenders")
        .join(&tender.tender_id)
        .join("tender.sqlite");
    let connection = rusqlite::Connection::open(database).expect("Tender Store database");
    let trigger_sql: String = connection
        .query_row(
            "SELECT sql FROM sqlite_schema WHERE type = 'trigger' AND name = 'audit_events_no_update'",
            [],
            |row| row.get(0),
        )
        .expect("Audit Event trigger");
    connection
        .execute_batch("DROP TRIGGER audit_events_no_update")
        .expect("inject Audit Event mutation capability");
    connection
        .execute(
            "UPDATE audit_events SET payload_json = '{}' WHERE sequence = 1",
            [],
        )
        .expect("damage cached Tender");
    connection
        .execute_batch(&trigger_sql)
        .expect("restore exact trigger");
    drop(connection);

    assert_eq!(
        host.inspect_tender_integrity(&tender.tender_id)
            .expect("detect corruption")
            .state,
        TenderIntegrityState::RecoveryRequired
    );
    assert_eq!(
        host.revise_tender(ReviseTenderCommand {
            tender_id: tender.tender_id,
            name: "Must remain read-only".into(),
        })
        .expect_err("detected corruption must disable cached writer")
        .code,
        TenderErrorCode::RecoveryRequired
    );
}

#[test]
fn concurrent_mutations_cannot_outlive_a_failed_integrity_inspection() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Concurrent Recovery Latch".into(),
        })
        .expect("create Tender");
    host.register_tender_content(RegisterTenderContentCommand {
        tender_id: tender.tender_id.clone(),
        logical_id: "inspection-window".into(),
        media_type: "application/octet-stream".into(),
        bytes: vec![0x5a; 16 * 1024 * 1024],
    })
    .expect("register enough content to exercise concurrent inspection");

    let database = application_home
        .join("tenders")
        .join(&tender.tender_id)
        .join("tender.sqlite");
    let connection = rusqlite::Connection::open(database).expect("Tender Store database");
    let trigger_sql: String = connection
        .query_row(
            "SELECT sql FROM sqlite_schema WHERE type = 'trigger' AND name = 'audit_events_no_update'",
            [],
            |row| row.get(0),
        )
        .expect("Audit Event trigger");
    connection
        .execute_batch("DROP TRIGGER audit_events_no_update")
        .expect("inject Audit Event mutation capability");
    connection
        .execute(
            "UPDATE audit_events SET payload_json = '{}' WHERE sequence = 1",
            [],
        )
        .expect("damage cached Tender");
    connection
        .execute_batch(&trigger_sql)
        .expect("restore exact trigger");
    drop(connection);

    let start = Arc::new(Barrier::new(2));
    let inspection_host = host.clone();
    let inspection_tender_id = tender.tender_id.clone();
    let inspection_start = Arc::clone(&start);
    let inspection = thread::spawn(move || {
        inspection_start.wait();
        inspection_host
            .inspect_tender_integrity(&inspection_tender_id)
            .expect("detect corruption")
    });
    start.wait();
    thread::sleep(Duration::from_millis(10));

    let mutations = (0..8)
        .map(|index| {
            let mutation_host = host.clone();
            let tender_id = tender.tender_id.clone();
            thread::spawn(move || {
                mutation_host.revise_tender(ReviseTenderCommand {
                    tender_id,
                    name: format!("Must remain read-only {index}"),
                })
            })
        })
        .collect::<Vec<_>>();

    assert_eq!(
        inspection.join().expect("inspection thread").state,
        TenderIntegrityState::RecoveryRequired
    );
    for mutation in mutations {
        assert_eq!(
            mutation
                .join()
                .expect("mutation thread")
                .expect_err("no mutation may commit after the failed inspection")
                .code,
            TenderErrorCode::RecoveryRequired
        );
    }
}

#[test]
fn cold_open_rejects_a_linked_content_cache_without_touching_its_target() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Linked Content Cache".into(),
        })
        .expect("create Tender");
    host.close_tender(&tender.tender_id).expect("close Tender");

    let content_v2 = application_home
        .join("tenders")
        .join(&tender.tender_id)
        .join("content")
        .join("content-v2");
    if content_v2.exists() {
        std::fs::remove_dir_all(&content_v2)
            .expect("remove empty cache root for failure injection");
    }
    let external = user_home.path().join("external-content-cache");
    let digest = "a".repeat(64);
    let external_object = external
        .join("sha256")
        .join(&digest[0..2])
        .join(&digest[2..4])
        .join(&digest[4..]);
    std::fs::create_dir_all(external_object.parent().expect("object parent"))
        .expect("create external cache layout");
    std::fs::write(&external_object, b"must survive").expect("write external sentinel");
    create_directory_link(&external, &content_v2).expect("link cache root outside Tender Store");

    assert_eq!(
        host.open_tender(&tender.tender_id)
            .expect_err("linked cache root must fail closed")
            .code,
        TenderErrorCode::RecoveryRequired
    );
    assert_eq!(
        std::fs::read(external_object).expect("external sentinel must survive reconciliation"),
        b"must survive"
    );
}

#[test]
fn integrity_inspection_rejects_a_linked_hash_root_and_latches_the_writer() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Linked Hash Root".into(),
        })
        .expect("create Tender");
    host.open_tender(&tender.tender_id)
        .expect("cache verified Tender writer");

    let content_v2 = application_home
        .join("tenders")
        .join(&tender.tender_id)
        .join("content")
        .join("content-v2");
    std::fs::create_dir_all(&content_v2).expect("create cache root for failure injection");
    let hash_root = content_v2.join("sha256");
    if hash_root.exists() {
        std::fs::remove_dir_all(&hash_root).expect("remove empty hash root for failure injection");
    }
    let external = user_home.path().join("external-hash-root");
    std::fs::create_dir_all(&external).expect("create external hash root");
    let sentinel = external.join("must-remain.txt");
    std::fs::write(&sentinel, b"must survive").expect("write external sentinel");
    create_directory_link(&external, &hash_root).expect("link hash root outside Tender Store");

    let report = host
        .inspect_tender_integrity(&tender.tender_id)
        .expect("inspect linked hash root");
    assert_eq!(report.state, TenderIntegrityState::RecoveryRequired);
    assert_eq!(
        report.issues,
        vec![TenderIntegrityIssue::StorageLayoutInvalid]
    );
    assert_eq!(
        host.revise_tender(ReviseTenderCommand {
            tender_id: tender.tender_id,
            name: "Must remain read-only".into(),
        })
        .expect_err("linked hash root must latch the writer")
        .code,
        TenderErrorCode::RecoveryRequired
    );
    assert_eq!(
        std::fs::read(sentinel).expect("external sentinel must survive inspection"),
        b"must survive"
    );
}

#[test]
fn inspection_io_failure_exposes_recovery_evidence_and_latches_the_writer() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Inspection I/O Failure".into(),
        })
        .expect("create Tender");
    host.open_tender(&tender.tender_id)
        .expect("cache verified Tender writer");

    std::env::set_var("QUANTIX_STORAGE_INSPECTION_FAIL_TENDER", &tender.tender_id);
    let report = host
        .inspect_tender_integrity(&tender.tender_id)
        .expect("inspection failure must become recovery evidence");
    std::env::remove_var("QUANTIX_STORAGE_INSPECTION_FAIL_TENDER");

    assert_eq!(report.state, TenderIntegrityState::RecoveryRequired);
    assert!(!report.issues.is_empty());
    assert_eq!(
        host.revise_tender(ReviseTenderCommand {
            tender_id: tender.tender_id,
            name: "Must remain read-only".into(),
        })
        .expect_err("inspection I/O failure must latch the writer")
        .code,
        TenderErrorCode::RecoveryRequired
    );
}

#[test]
fn missing_cached_tender_directory_exposes_recovery_and_latches_the_detached_writer() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Detached Cached Tender".into(),
        })
        .expect("create Tender");
    host.open_tender(&tender.tender_id)
        .expect("cache verified Tender writer");

    std::env::set_var(
        "QUANTIX_STORAGE_INSPECTION_FAIL_TENDER",
        format!("not_found:{}", tender.tender_id),
    );
    let report = host
        .inspect_tender_integrity(&tender.tender_id)
        .expect("missing cached directory must become recovery evidence");
    std::env::remove_var("QUANTIX_STORAGE_INSPECTION_FAIL_TENDER");
    assert_eq!(report.state, TenderIntegrityState::RecoveryRequired);
    assert_eq!(
        report.issues,
        vec![TenderIntegrityIssue::InspectionUnavailable]
    );
    assert_eq!(
        host.revise_tender(ReviseTenderCommand {
            tender_id: tender.tender_id,
            name: "Must remain read-only".into(),
        })
        .expect_err("detached cached writer must be latched")
        .code,
        TenderErrorCode::RecoveryRequired
    );
}

#[test]
fn cold_open_removes_unreferenced_content_and_preserves_committed_objects() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Content Reconciliation".into(),
        })
        .expect("create Tender");
    host.register_tender_content(RegisterTenderContentCommand {
        tender_id: tender.tender_id.clone(),
        logical_id: "committed".into(),
        media_type: "text/plain".into(),
        bytes: b"committed bytes".to_vec(),
    })
    .expect("commit canonical content");
    host.close_tender(&tender.tender_id).expect("close Tender");
    let content_root = application_home
        .join("tenders")
        .join(&tender.tender_id)
        .join("content");
    let _orphan_integrity = cacache::write_hash_sync(&content_root, b"uncommitted bytes")
        .expect("stage unreferenced cache object");

    let reopened = host
        .open_tender(&tender.tender_id)
        .expect("reconcile content cache before work");
    assert_eq!(reopened.audit_event_count, tender.audit_event_count + 2);
    assert_ne!(reopened.audit_chain_head, tender.audit_chain_head);
    assert_eq!(
        host.inspect_tender_integrity(&tender.tender_id)
            .expect("verify committed object through the Host")
            .state,
        TenderIntegrityState::Ready
    );
    assert_eq!(
        host.inspect_tender(&tender.tender_id)
            .expect("inspect committed content")
            .content_object_count,
        1
    );
}

#[test]
fn host_death_at_content_publication_boundaries_never_publishes_a_partial_object() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Content Boundary Tender".into(),
        })
        .expect("create Tender");
    host.register_tender_content(RegisterTenderContentCommand {
        tender_id: tender.tender_id.clone(),
        logical_id: "committed".into(),
        media_type: "text/plain".into(),
        bytes: b"committed before failure injection".to_vec(),
    })
    .expect("register committed baseline");
    let baseline = host
        .inspect_tender(&tender.tender_id)
        .expect("inspect baseline");
    host.close_tender(&tender.tender_id).expect("close Tender");
    drop(host);

    assert!(!run_storage_fixture(
        &application_home,
        &["register", &tender.tender_id, "lost-before-commit"],
        "content_after_cache_write",
    ));
    let restarted =
        QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    let reopened = restarted
        .open_tender(&tender.tender_id)
        .expect("reconcile cache candidate after Host death");
    assert_eq!(
        restarted
            .inspect_tender(&tender.tender_id)
            .expect("inspect reconciled content")
            .content_object_count,
        baseline.content_object_count
    );
    assert_eq!(
        reopened.audit_event_count,
        baseline.summary.audit_event_count + 1
    );
    assert_eq!(
        restarted
            .inspect_tender_integrity(&tender.tender_id)
            .expect("verify Tender after pre-commit death")
            .state,
        TenderIntegrityState::Ready
    );
    restarted
        .close_tender(&tender.tender_id)
        .expect("close reconciled Tender");
    drop(restarted);

    assert!(!run_storage_fixture(
        &application_home,
        &["register", &tender.tender_id, "survives-after-commit"],
        "content_after_database_commit",
    ));
    let restarted =
        QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    restarted
        .open_tender(&tender.tender_id)
        .expect("reopen committed content after Host death");
    assert_eq!(
        restarted
            .inspect_tender(&tender.tender_id)
            .expect("inspect post-commit content")
            .content_object_count,
        baseline.content_object_count + 1
    );
    assert_eq!(
        restarted
            .inspect_tender_integrity(&tender.tender_id)
            .expect("verify post-commit Tender")
            .state,
        TenderIntegrityState::Ready
    );
}

#[test]
fn host_death_at_tender_and_catalogue_publication_boundaries_reconciles_on_restart() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let setup = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&setup).state, SetupState::Ready);
    drop(setup);

    assert!(!run_storage_fixture(
        &application_home,
        &["create", "Lost Staged Tender"],
        "tender_after_stage",
    ));
    let restarted =
        QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert!(restarted
        .list_tenders()
        .expect("reconcile unpublished Tender candidate")
        .is_empty());
    assert_eq!(
        restarted
            .inspect_startup_reconciliation()
            .removed_tender_candidates,
        1
    );
    drop(restarted);

    assert!(!run_storage_fixture(
        &application_home,
        &["create", "Published Before Host Death"],
        "tender_after_publish",
    ));
    let restarted =
        QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    let catalogue = restarted
        .list_tenders()
        .expect("discover atomically published Tender");
    assert_eq!(catalogue.len(), 1);
    assert_eq!(
        catalogue[0]
            .summary
            .as_ref()
            .map(|summary| summary.name.as_str()),
        Some("Published Before Host Death")
    );
    drop(restarted);

    assert!(!run_storage_fixture(
        &application_home,
        &["list"],
        "catalogue_after_commit",
    ));
    let restarted =
        QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    let catalogue = restarted
        .list_tenders()
        .expect("rebuild derived catalogue after Host death");
    assert_eq!(catalogue.len(), 1);
    assert_eq!(catalogue[0].integrity.state, TenderIntegrityState::Ready);
}

fn run_storage_fixture(application_home: &Path, arguments: &[&str], failpoint: &str) -> bool {
    Command::new(env!("CARGO_BIN_EXE_quantix-storage-fixture"))
        .arg(application_home)
        .args(arguments)
        .env("QUANTIX_STORAGE_FAILPOINT", failpoint)
        .status()
        .expect("run supervised storage fixture")
        .success()
}

#[cfg(unix)]
fn create_directory_link(target: &Path, link: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn create_directory_link(target: &Path, link: &Path) -> io::Result<()> {
    junction::create(target, link)
}
