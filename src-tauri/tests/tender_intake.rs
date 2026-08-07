use std::{
    fs,
    io::{self, Write},
    path::Path,
    sync::Arc,
};

use quantix_lib::{
    ensure_quantix_setup, ConfirmSourceRelationshipCommand, CreateTenderCommand, DeviceProtection,
    ImportTenderPackageCommand, IntakeExceptionCode, QuantixHost, RegistrationState, SetupPlatform,
    SetupState, SourceRelationshipKind, StoragePermissions, SupersessionState,
    TenderPackageSourceKind, MINIMUM_SETUP_FREE_SPACE_BYTES,
};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

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

struct Harness {
    _root: tempfile::TempDir,
    application_home: std::path::PathBuf,
    host: QuantixHost,
    tender_id: String,
}

impl Harness {
    fn new() -> Self {
        let root = tempfile::tempdir().expect("temporary intake harness");
        let application_home = root.path().join(".quantix");
        let host =
            QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
        assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
        let tender = host
            .create_tender(CreateTenderCommand {
                name: "Quantix intake golden Tender".into(),
            })
            .expect("create Tender");
        Self {
            _root: root,
            application_home,
            host,
            tender_id: tender.tender_id,
        }
    }

    fn import(&self, source: &Path) -> quantix_lib::TenderPackageImportResult {
        self.host
            .import_tender_package(ImportTenderPackageCommand {
                tender_id: self.tender_id.clone(),
                source_path: source.to_string_lossy().into_owned(),
            })
            .expect("import Tender Package")
    }

    fn database(&self) -> rusqlite::Connection {
        rusqlite::Connection::open(
            self.application_home
                .join("tenders")
                .join(&self.tender_id)
                .join("tender.sqlite"),
        )
        .expect("Tender database")
    }
}

fn pdf(label: &str) -> Vec<u8> {
    format!("%PDF-1.7\n1 0 obj\n({label})\nendobj\n%%EOF\n").into_bytes()
}

#[test]
fn directory_intake_registers_safe_bytes_and_keeps_each_exception_visible() {
    let harness = Harness::new();
    let source = harness._root.path().join("source-package");
    fs::create_dir(&source).expect("source directory");
    fs::write(source.join("scope.pdf"), pdf("scope")).expect("safe PDF");
    fs::write(source.join("notes.txt"), b"not supported in v0").expect("unsupported file");
    fs::write(source.join("rates.xlsm"), b"macro workbook").expect("macro file");
    fs::File::create(source.join("oversized.pdf"))
        .and_then(|file| file.set_len(16 * 1024 * 1024 + 1))
        .expect("sparse oversized file");

    let imported = harness.import(&source);

    assert_eq!(imported.source_kind, TenderPackageSourceKind::Directory);
    assert_eq!(imported.discovered_count, 4);
    assert_eq!(imported.registered_count, 1);
    assert_eq!(imported.exception_count, 3);
    assert!(imported.query_register_open);
    assert_eq!(imported.documents.len(), 4);
    let scope = imported
        .documents
        .iter()
        .find(|document| document.package_path == "scope.pdf")
        .expect("scope Document Register row");
    assert_eq!(scope.registration_state, RegistrationState::Registered);
    assert_eq!(scope.media_type.as_deref(), Some("application/pdf"));
    assert_eq!(scope.sha256.as_ref().map(String::len), Some(64));
    assert_eq!(scope.version, 1);
    assert_eq!(scope.language, "undetermined");
    assert_eq!(scope.supersession_state, SupersessionState::Unconfirmed);
    assert_eq!(
        imported
            .documents
            .iter()
            .find(|document| document.package_path == "notes.txt")
            .and_then(|document| document.exception),
        Some(IntakeExceptionCode::Unsupported)
    );
    assert_eq!(
        imported
            .documents
            .iter()
            .find(|document| document.package_path == "rates.xlsm")
            .and_then(|document| document.exception),
        Some(IntakeExceptionCode::MacroBearing)
    );
    assert_eq!(
        imported
            .documents
            .iter()
            .find(|document| document.package_path == "oversized.pdf")
            .and_then(|document| document.exception),
        Some(IntakeExceptionCode::FileSizeExceeded)
    );

    fs::remove_dir_all(&source).expect("disconnect original source");
    let reopened = harness
        .host
        .inspect_document_register(&harness.tender_id)
        .expect("Document Register survives disconnected source");
    assert!(reopened.query_register_open);
    assert_eq!(reopened.documents, imported.documents);

    let database = harness.database();
    let (objects, documents, queries): (i64, i64, i64) = database
        .query_row(
            "SELECT
               (SELECT COUNT(*) FROM content_objects),
               (SELECT COUNT(*) FROM source_artifact_versions),
               (SELECT COUNT(*) FROM query_register)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("canonical intake counts");
    assert_eq!((objects, documents, queries), (1, 4, 1));
    let failed_events: i64 = database
        .query_row(
            "SELECT COUNT(*) FROM audit_events
             WHERE event_type = 'source_artifact_registration_failed'",
            [],
            |row| row.get(0),
        )
        .expect("registration failure Audit Events");
    assert_eq!(failed_events, 3);
    assert!(database
        .execute(
            "UPDATE source_artifact_versions SET language = 'rewritten'",
            [],
        )
        .is_err());
    assert!(database.execute("DELETE FROM intake_runs", []).is_err());
}

#[test]
fn zip_intake_fails_each_unsafe_or_suspicious_entry_closed() {
    let harness = Harness::new();
    let archive_path = harness._root.path().join("package.zip");
    let archive = fs::File::create(&archive_path).expect("archive file");
    let mut zip = ZipWriter::new(archive);
    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    zip.start_file("safe.pdf", stored).expect("safe entry");
    zip.write_all(&pdf("safe")).expect("safe bytes");
    zip.start_file("../escape.pdf", stored)
        .expect("traversal entry");
    zip.write_all(&pdf("escape")).expect("traversal bytes");
    zip.start_file("dupe.pdf", stored).expect("duplicate entry");
    zip.write_all(&pdf("duplicate")).expect("duplicate bytes");
    zip.start_file("nested.zip", stored).expect("nested entry");
    zip.write_all(b"PK\x03\x04nested").expect("nested bytes");
    zip.start_file("macros.xlsm", stored).expect("macro entry");
    zip.write_all(b"PK\x03\x04macro").expect("macro bytes");
    zip.add_symlink("linked.pdf", "../outside.pdf", stored)
        .expect("symlink entry");
    zip.start_file("bomb.pdf", deflated).expect("bomb entry");
    let mut bomb = pdf("bomb");
    bomb.extend(vec![b'A'; 512 * 1024]);
    zip.write_all(&bomb).expect("bomb bytes");
    zip.finish().expect("complete archive");
    let archive_bytes = fs::read(&archive_path).expect("read archive for duplicate fixture");
    let archive_bytes = archive_bytes.windows(b"dupe.pdf".len()).enumerate().fold(
        archive_bytes.clone(),
        |mut bytes, (index, window)| {
            if window == b"dupe.pdf" {
                bytes[index..index + b"safe.pdf".len()].copy_from_slice(b"safe.pdf");
            }
            bytes
        },
    );
    fs::write(&archive_path, archive_bytes).expect("duplicate-path archive fixture");

    let imported = harness.import(&archive_path);

    assert_eq!(imported.source_kind, TenderPackageSourceKind::ZipArchive);
    assert_eq!(
        imported.discovered_count, 7,
        "archive result: {:#?}",
        imported.documents
    );
    assert_eq!(imported.registered_count, 1);
    let codes = imported
        .documents
        .iter()
        .filter_map(|document| document.exception)
        .collect::<Vec<_>>();
    assert!(codes.contains(&IntakeExceptionCode::UnsafePath));
    assert!(codes.contains(&IntakeExceptionCode::DuplicatePath));
    assert!(codes.contains(&IntakeExceptionCode::NestedArchive));
    assert!(codes.contains(&IntakeExceptionCode::MacroBearing));
    assert!(codes.contains(&IntakeExceptionCode::UnsafeLink));
    assert!(codes.contains(&IntakeExceptionCode::ExpansionRatioExceeded));
    assert!(!harness._root.path().join("escape.pdf").exists());
}

#[test]
fn deceptive_eocd_in_zip_comment_fails_closed() {
    let harness = Harness::new();
    let archive_path = harness._root.path().join("deceptive-comment.zip");
    let archive = fs::File::create(&archive_path).expect("archive file");
    let mut zip = ZipWriter::new(archive);
    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);

    zip.start_file("safe.pdf", stored).expect("safe entry");
    zip.write_all(&pdf("safe")).expect("safe bytes");
    let mut deceptive_comment = [0_u8; 22];
    deceptive_comment[..4].copy_from_slice(b"PK\x05\x06");
    zip.set_raw_comment(deceptive_comment.into())
        .expect("adversarial archive comment");
    zip.finish().expect("complete archive");

    let imported = harness.import(&archive_path);

    assert_eq!(imported.discovered_count, 1);
    assert_eq!(imported.registered_count, 0);
    assert_eq!(imported.documents.len(), 1);
    assert_eq!(
        imported.documents[0].exception,
        Some(IntakeExceptionCode::Corrupt)
    );
}

#[test]
fn corrupt_archive_is_an_attributable_exception_and_does_not_partially_publish() {
    let harness = Harness::new();
    let archive_path = harness._root.path().join("corrupt.zip");
    fs::write(&archive_path, b"not a ZIP archive").expect("corrupt archive");

    let imported = harness.import(&archive_path);

    assert_eq!(imported.discovered_count, 1);
    assert_eq!(imported.registered_count, 0);
    assert_eq!(imported.exception_count, 1);
    assert_eq!(
        imported.documents[0].exception,
        Some(IntakeExceptionCode::Corrupt)
    );
    assert!(imported.query_register_open);
    let objects: i64 = harness
        .database()
        .query_row("SELECT COUNT(*) FROM content_objects", [], |row| row.get(0))
        .expect("no partial content publication");
    assert_eq!(objects, 0);
}

#[test]
fn confirmed_replacements_are_explicit_and_do_not_rewrite_history() {
    let harness = Harness::new();
    let source = harness._root.path().join("addenda");
    fs::create_dir(&source).expect("addenda directory");
    fs::write(source.join("original.pdf"), pdf("original")).expect("original");
    fs::write(source.join("addendum.pdf"), pdf("addendum")).expect("addendum");
    fs::write(source.join("replacement.pdf"), pdf("replacement")).expect("replacement");
    let imported = harness.import(&source);
    let original = imported
        .documents
        .iter()
        .find(|document| document.package_path == "original.pdf")
        .expect("original row");
    let addendum = imported
        .documents
        .iter()
        .find(|document| document.package_path == "addendum.pdf")
        .expect("addendum row");
    let replacement = imported
        .documents
        .iter()
        .find(|document| document.package_path == "replacement.pdf")
        .expect("replacement row");

    let addendum_register = harness
        .host
        .confirm_source_relationship(ConfirmSourceRelationshipCommand {
            tender_id: harness.tender_id.clone(),
            prior_artifact_id: original.artifact_id.clone(),
            prior_version: original.version,
            replacement_artifact_id: addendum.artifact_id.clone(),
            replacement_version: addendum.version,
            relationship_kind: SourceRelationshipKind::Addendum,
        })
        .expect("confirm addendum");
    assert_eq!(
        addendum_register
            .documents
            .iter()
            .find(|document| document.artifact_id == original.artifact_id)
            .map(|document| document.supersession_state),
        Some(SupersessionState::Current)
    );

    let register = harness
        .host
        .confirm_source_relationship(ConfirmSourceRelationshipCommand {
            tender_id: harness.tender_id.clone(),
            prior_artifact_id: original.artifact_id.clone(),
            prior_version: original.version,
            replacement_artifact_id: replacement.artifact_id.clone(),
            replacement_version: replacement.version,
            relationship_kind: SourceRelationshipKind::Replacement,
        })
        .expect("confirm replacement");

    assert_eq!(register.documents.len(), 3);
    assert_eq!(
        register
            .documents
            .iter()
            .find(|document| document.artifact_id == original.artifact_id)
            .map(|document| document.supersession_state),
        Some(SupersessionState::Superseded)
    );
    assert_eq!(
        register
            .documents
            .iter()
            .find(|document| document.artifact_id == replacement.artifact_id)
            .map(|document| document.supersession_state),
        Some(SupersessionState::Current)
    );
    let database = harness.database();
    let versions: i64 = database
        .query_row("SELECT COUNT(*) FROM source_artifact_versions", [], |row| {
            row.get(0)
        })
        .expect("immutable source history");
    let relationships: i64 = database
        .query_row("SELECT COUNT(*) FROM source_relationships", [], |row| {
            row.get(0)
        })
        .expect("explicit relationship");
    assert_eq!((versions, relationships), (3, 2));
}
