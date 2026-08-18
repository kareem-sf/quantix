use std::{fs, io, io::Write, path::Path, sync::Arc};

use quantix_lib::{
    ensure_quantix_setup, CreateTenderCommand, DeviceProtection, EvidenceLanguage,
    EvidenceLocationKind, ImportTenderPackageCommand, ParseExceptionCode,
    ParseSourceArtifactCommand, ParseState, QuantixHost, SearchEvidenceCommand,
    SearchEvidenceSemanticCommand, SetupPlatform, SetupState, StoragePermissions, TenderErrorCode,
    TextDirection, MINIMUM_SETUP_FREE_SPACE_BYTES,
};
use rusqlite::Connection;
use zip::{write::SimpleFileOptions, ZipWriter};

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

#[tokio::test]
async fn exact_search_navigates_to_the_immutable_evidence_location() {
    let harness = Harness::new();
    let source = harness._root.path().join("search-source");
    fs::create_dir(&source).expect("search source directory");
    fs::write(
        source.join("conditions.pdf"),
        b"%PDF-1.7\n1 0 obj\n(Search conditions)\nendobj\n%%EOF\n",
    )
    .expect("search PDF fixture");
    let imported = harness
        .host
        .import_tender_package(ImportTenderPackageCommand {
            tender_id: harness.tender_id.clone(),
            source_path: source.to_string_lossy().into_owned(),
        })
        .expect("import search PDF");
    let document = imported
        .documents
        .iter()
        .find(|document| document.package_path == "conditions.pdf")
        .expect("registered search PDF");
    harness
        .host
        .parse_source_artifact(ParseSourceArtifactCommand {
            tender_id: harness.tender_id.clone(),
            artifact_id: document.artifact_id.clone(),
            version: document.version,
        })
        .await
        .expect("parse search PDF");

    let english = harness
        .host
        .search_evidence(SearchEvidenceCommand {
            tender_id: harness.tender_id.clone(),
            query: "Bid security".into(),
        })
        .expect("search English evidence");
    assert_eq!(english.matches.len(), 1);
    assert_eq!(english.matches[0].artifact_id, document.artifact_id);
    assert_eq!(english.matches[0].version, 1);
    assert_eq!(english.matches[0].package_path, "conditions.pdf");
    assert_eq!(
        english.matches[0].location.kind,
        EvidenceLocationKind::Paragraph
    );
    assert_eq!(
        english.matches[0]
            .location
            .provenance
            .iter()
            .map(|region| region.page_number)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );

    let arabic = harness
        .host
        .search_evidence(SearchEvidenceCommand {
            tender_id: harness.tender_id.clone(),
            query: "ضمان العطاء".into(),
        })
        .expect("search Arabic evidence");
    assert_eq!(arabic.matches.len(), 1);
    assert_eq!(arabic.matches[0].location.provenance[0].page_number, 2);
    assert_eq!(
        arabic.matches[0].location.direction,
        TextDirection::RightToLeft
    );

    let semantic = harness
        .host
        .search_evidence_semantic(SearchEvidenceSemanticCommand {
            tender_id: harness.tender_id.clone(),
            query: "security required".into(),
            distance_threshold: 0.4,
            limit: 10,
        })
        .await
        .expect("search semantic evidence");
    assert_eq!(semantic.query, "security required");
    assert_eq!(semantic.matches.len(), 1);
    assert_eq!(semantic.matches[0].artifact_id, document.artifact_id);
    assert_eq!(semantic.matches[0].package_path, "conditions.pdf");
    assert!(semantic.matches[0].distance <= 0.4);

    let excluded = harness
        .host
        .search_evidence_semantic(SearchEvidenceSemanticCommand {
            tender_id: harness.tender_id.clone(),
            query: "security required".into(),
            distance_threshold: 0.0,
            limit: 10,
        })
        .await
        .expect("apply semantic distance threshold");
    assert!(excluded.matches.is_empty());

    let invalid = harness
        .host
        .search_evidence_semantic(SearchEvidenceSemanticCommand {
            tender_id: harness.tender_id.clone(),
            query: "security required".into(),
            distance_threshold: 0.4,
            limit: 0,
        })
        .await
        .unwrap_err();
    assert_eq!(invalid.code, TenderErrorCode::InvalidCommand);

    harness
        .host
        .close_tender(&harness.tender_id)
        .expect("close semantic search Tender");
    let reopened = harness
        .host
        .search_evidence_semantic(SearchEvidenceSemanticCommand {
            tender_id: harness.tender_id.clone(),
            query: "security required".into(),
            distance_threshold: 0.4,
            limit: 10,
        })
        .await
        .expect("search persisted semantic evidence after cold open");
    assert_eq!(reopened.matches.len(), 1);
}

#[tokio::test]
async fn docx_preserves_section_and_paragraph_locations_without_invented_pages() {
    let harness = Harness::new();
    let source = harness._root.path().join("docx-source");
    fs::create_dir(&source).expect("DOCX source directory");
    fs::write(
        source.join("scope.docx"),
        office_package("word/document.xml"),
    )
    .expect("DOCX fixture");
    let imported = harness
        .host
        .import_tender_package(ImportTenderPackageCommand {
            tender_id: harness.tender_id.clone(),
            source_path: source.to_string_lossy().into_owned(),
        })
        .expect("import DOCX");
    let document = imported
        .documents
        .iter()
        .find(|document| document.package_path == "scope.docx")
        .expect("registered DOCX");

    let parsed = harness
        .host
        .parse_source_artifact(ParseSourceArtifactCommand {
            tender_id: harness.tender_id.clone(),
            artifact_id: document.artifact_id.clone(),
            version: document.version,
        })
        .await
        .expect("parse DOCX");
    assert_eq!(parsed.state, ParseState::Parsed);
    let evidence = harness
        .host
        .inspect_evidence(ParseSourceArtifactCommand {
            tender_id: harness.tender_id.clone(),
            artifact_id: document.artifact_id.clone(),
            version: document.version,
        })
        .expect("inspect DOCX evidence");
    let paragraph = evidence
        .locations
        .iter()
        .find(|location| location.original_text == "Works include concrete and finishes.")
        .expect("DOCX paragraph");
    assert_eq!(paragraph.kind, EvidenceLocationKind::Paragraph);
    assert_eq!(paragraph.section.as_deref(), Some("Scope of Works"));
    assert_eq!(paragraph.paragraph_number, Some(1));
    assert!(paragraph.provenance.is_empty());
    assert!(evidence.locations.iter().any(|location| {
        location.kind == EvidenceLocationKind::Section && location.original_text == "Scope of Works"
    }));
    assert!(evidence.locations.iter().any(|location| {
        location.original_text == "تشمل الأعمال الخرسانة والتشطيبات."
            && location.language == EvidenceLanguage::Arabic
            && location.direction == TextDirection::RightToLeft
            && location.provenance.is_empty()
    }));
}

#[tokio::test]
async fn xlsx_preserves_sheet_table_and_cell_locations() {
    let harness = Harness::new();
    let source = harness._root.path().join("xlsx-source");
    fs::create_dir(&source).expect("XLSX source directory");
    fs::write(source.join("boq.xlsx"), office_package("xl/workbook.xml")).expect("XLSX fixture");
    let imported = harness
        .host
        .import_tender_package(ImportTenderPackageCommand {
            tender_id: harness.tender_id.clone(),
            source_path: source.to_string_lossy().into_owned(),
        })
        .expect("import XLSX");
    let document = imported
        .documents
        .iter()
        .find(|document| document.package_path == "boq.xlsx")
        .expect("registered XLSX");

    let parsed = harness
        .host
        .parse_source_artifact(ParseSourceArtifactCommand {
            tender_id: harness.tender_id.clone(),
            artifact_id: document.artifact_id.clone(),
            version: document.version,
        })
        .await
        .expect("parse XLSX");
    assert_eq!(parsed.state, ParseState::Parsed);
    let evidence = harness
        .host
        .inspect_evidence(ParseSourceArtifactCommand {
            tender_id: harness.tender_id.clone(),
            artifact_id: document.artifact_id.clone(),
            version: document.version,
        })
        .expect("inspect XLSX evidence");
    assert!(evidence.locations.iter().any(|location| {
        location.kind == EvidenceLocationKind::Sheet
            && location.sheet_name.as_deref() == Some("Pricing")
            && location.original_text == "Pricing"
    }));
    assert!(evidence.locations.iter().any(|location| {
        location.kind == EvidenceLocationKind::Table
            && location.sheet_name.as_deref() == Some("Pricing")
            && location.provenance[0].page_number == 1
    }));
    let cell = evidence
        .locations
        .iter()
        .find(|location| location.original_text == "450.75")
        .expect("priced XLSX cell");
    assert_eq!(cell.kind, EvidenceLocationKind::Cell);
    assert_eq!(cell.sheet_name.as_deref(), Some("Pricing"));
    assert_eq!(cell.cell_range.as_deref(), Some("C4"));
    assert_eq!(cell.provenance[0].page_number, 1);
    assert!(evidence.locations.iter().any(|location| {
        location.original_text == "خرسانة"
            && location.cell_range.as_deref() == Some("B4")
            && location.language == EvidenceLanguage::Arabic
            && location.direction == TextDirection::RightToLeft
    }));
}

#[tokio::test]
async fn malformed_or_excessive_candidate_output_never_becomes_evidence() {
    let harness = Harness::new();
    let source = harness._root.path().join("invalid-output-source");
    fs::create_dir(&source).expect("invalid output source directory");
    fs::write(
        source.join("malformed.pdf"),
        b"%PDF-1.7\nMALFORMED_OUTPUT\n%%EOF\n",
    )
    .expect("malformed-output PDF");
    fs::write(
        source.join("excessive.pdf"),
        b"%PDF-1.7\nEXCESSIVE_OUTPUT\n%%EOF\n",
    )
    .expect("excessive-output PDF");
    fs::write(
        source.join("invalid-reference.pdf"),
        b"%PDF-1.7\nINVALID_REFERENCE_OUTPUT\n%%EOF\n",
    )
    .expect("invalid-reference PDF");
    let imported = harness
        .host
        .import_tender_package(ImportTenderPackageCommand {
            tender_id: harness.tender_id.clone(),
            source_path: source.to_string_lossy().into_owned(),
        })
        .expect("import invalid-output PDFs");

    for (package_path, expected_exception) in [
        ("malformed.pdf", ParseExceptionCode::MalformedOutput),
        ("excessive.pdf", ParseExceptionCode::OutputLimitExceeded),
        ("invalid-reference.pdf", ParseExceptionCode::MalformedOutput),
    ] {
        let document = imported
            .documents
            .iter()
            .find(|document| document.package_path == package_path)
            .expect("registered invalid-output PDF");
        let command = ParseSourceArtifactCommand {
            tender_id: harness.tender_id.clone(),
            artifact_id: document.artifact_id.clone(),
            version: document.version,
        };
        let result = harness
            .host
            .parse_source_artifact(command.clone())
            .await
            .expect("record rejected candidate");
        assert_eq!(result.state, ParseState::Quarantined);
        assert_eq!(result.exception, Some(expected_exception));
        assert_eq!(result.location_count, 0);
        assert_eq!(
            harness.host.inspect_evidence(command).unwrap_err().code,
            TenderErrorCode::NotFound
        );
    }

    let register = harness
        .host
        .inspect_document_register(&harness.tender_id)
        .expect("inspect failure parse states");
    for (package_path, expected_exception) in [
        ("malformed.pdf", ParseExceptionCode::MalformedOutput),
        ("excessive.pdf", ParseExceptionCode::OutputLimitExceeded),
        ("invalid-reference.pdf", ParseExceptionCode::MalformedOutput),
    ] {
        let document = register
            .documents
            .iter()
            .find(|document| document.package_path == package_path)
            .expect("failure Document Register row");
        assert_eq!(document.parse_state, ParseState::Quarantined);
        assert_eq!(document.parse_exception, Some(expected_exception));
    }
    assert!(!harness
        .application_home
        .join("tenders")
        .join(&harness.tender_id)
        .join("staging")
        .read_dir()
        .expect("staging directory")
        .any(|entry| entry.is_ok()));
}

#[tokio::test]
async fn loss_and_unsupported_sources_are_explicit_and_never_become_evidence() {
    let harness = Harness::new();
    let source = harness._root.path().join("loss-source");
    fs::create_dir(&source).expect("loss source directory");
    fs::write(source.join("empty.pdf"), b"%PDF-1.7\nLOSS_OUTPUT\n%%EOF\n")
        .expect("loss-output PDF");
    fs::write(source.join("notes.txt"), b"unsupported source").expect("unsupported source fixture");
    let imported = harness
        .host
        .import_tender_package(ImportTenderPackageCommand {
            tender_id: harness.tender_id.clone(),
            source_path: source.to_string_lossy().into_owned(),
        })
        .expect("import loss and unsupported sources");

    let empty = imported
        .documents
        .iter()
        .find(|document| document.package_path == "empty.pdf")
        .expect("registered loss PDF");
    let empty_command = ParseSourceArtifactCommand {
        tender_id: harness.tender_id.clone(),
        artifact_id: empty.artifact_id.clone(),
        version: empty.version,
    };
    let loss = harness
        .host
        .parse_source_artifact(empty_command.clone())
        .await
        .expect("record loss output");
    assert_eq!(loss.state, ParseState::Quarantined);
    assert_eq!(loss.exception, Some(ParseExceptionCode::LossDetected));
    assert_eq!(
        harness
            .host
            .inspect_evidence(empty_command)
            .unwrap_err()
            .code,
        TenderErrorCode::NotFound
    );

    let unsupported = imported
        .documents
        .iter()
        .find(|document| document.package_path == "notes.txt")
        .expect("unsupported register row");
    assert_eq!(unsupported.parse_state, ParseState::Unsupported);
    assert_eq!(
        unsupported.parse_exception,
        Some(ParseExceptionCode::Unsupported)
    );
    assert_eq!(
        harness
            .host
            .parse_source_artifact(ParseSourceArtifactCommand {
                tender_id: harness.tender_id.clone(),
                artifact_id: unsupported.artifact_id.clone(),
                version: unsupported.version,
            })
            .await
            .unwrap_err()
            .code,
        TenderErrorCode::InvalidCommand
    );
}

#[tokio::test]
async fn corrupt_or_encrypted_ocr_failures_remain_attributable_and_noncanonical() {
    let harness = Harness::new();
    let source = harness._root.path().join("failed-source");
    fs::create_dir(&source).expect("failed source directory");
    fs::write(
        source.join("corrupt.pdf"),
        b"%PDF-1.7\nPROCESS_FAILURE\n%%EOF\n",
    )
    .expect("corrupt PDF fixture");
    fs::write(
        source.join("encrypted.pdf"),
        b"%PDF-1.7\n/Encrypt 5 0 R\n%%EOF\n",
    )
    .expect("encrypted PDF fixture");
    let imported = harness
        .host
        .import_tender_package(ImportTenderPackageCommand {
            tender_id: harness.tender_id.clone(),
            source_path: source.to_string_lossy().into_owned(),
        })
        .expect("import failed PDFs");

    for package_path in ["corrupt.pdf", "encrypted.pdf"] {
        let document = imported
            .documents
            .iter()
            .find(|document| document.package_path == package_path)
            .expect("registered failed PDF");
        let command = ParseSourceArtifactCommand {
            tender_id: harness.tender_id.clone(),
            artifact_id: document.artifact_id.clone(),
            version: document.version,
        };
        let result = harness
            .host
            .parse_source_artifact(command.clone())
            .await
            .expect("record process failure");
        assert_eq!(result.state, ParseState::Failed);
        assert_eq!(result.exception, Some(ParseExceptionCode::ProcessFailed));
        assert_eq!(
            harness.host.inspect_evidence(command).unwrap_err().code,
            TenderErrorCode::NotFound
        );
    }
    let register = harness
        .host
        .inspect_document_register(&harness.tender_id)
        .expect("inspect failed parse states");
    assert!(register.documents.iter().all(|document| {
        document.parse_state == ParseState::Failed
            && document.parse_exception == Some(ParseExceptionCode::ProcessFailed)
    }));
}

#[tokio::test]
async fn publication_failure_becomes_terminal_and_cleans_staging() {
    let harness = Harness::new();
    let source = harness._root.path().join("publication-failure-source");
    fs::create_dir(&source).expect("publication failure source directory");
    fs::write(
        source.join("conditions.pdf"),
        b"%PDF-1.7\n1 0 obj\n(Publication failure)\nendobj\n%%EOF\n",
    )
    .expect("publication failure PDF");
    let imported = harness
        .host
        .import_tender_package(ImportTenderPackageCommand {
            tender_id: harness.tender_id.clone(),
            source_path: source.to_string_lossy().into_owned(),
        })
        .expect("import publication failure PDF");
    let document = imported
        .documents
        .iter()
        .find(|document| document.package_path == "conditions.pdf")
        .expect("registered publication failure PDF");
    let connection = Connection::open(
        harness
            .application_home
            .join("tenders")
            .join(&harness.tender_id)
            .join("tender.sqlite"),
    )
    .expect("open publication failure side channel");
    connection
        .execute_batch(
            "CREATE TRIGGER test_reject_parsed_document
             BEFORE INSERT ON parsed_documents
             BEGIN
               SELECT RAISE(ABORT, 'injected publication failure');
             END;",
        )
        .expect("install deterministic publication failure");
    let command = ParseSourceArtifactCommand {
        tender_id: harness.tender_id.clone(),
        artifact_id: document.artifact_id.clone(),
        version: document.version,
    };

    let result = harness
        .host
        .parse_source_artifact(command.clone())
        .await
        .expect("record publication failure");
    assert_eq!(result.state, ParseState::Failed);
    assert_eq!(
        result.exception,
        Some(ParseExceptionCode::PublicationFailed)
    );
    assert_eq!(
        harness
            .host
            .inspect_evidence(command)
            .expect_err("failed publication must not expose evidence")
            .code,
        TenderErrorCode::NotFound
    );
    let register = harness
        .host
        .inspect_document_register(&harness.tender_id)
        .expect("inspect publication failure parse state");
    assert_eq!(register.documents[0].parse_state, ParseState::Failed);
    assert_eq!(
        register.documents[0].parse_exception,
        Some(ParseExceptionCode::PublicationFailed)
    );
    assert!(!harness
        .application_home
        .join("tenders")
        .join(&harness.tender_id)
        .join("staging")
        .read_dir()
        .expect("publication failure staging")
        .any(|entry| entry.is_ok()));
}

#[tokio::test]
async fn engineer_interruption_terminates_the_ocr_attempt_without_publication() {
    let harness = Harness::new();
    let source = harness._root.path().join("interrupted-source");
    fs::create_dir(&source).expect("interrupted source directory");
    fs::write(
        source.join("slow.pdf"),
        b"%PDF-1.7\nINTERRUPTED_PROCESS\n%%EOF\n",
    )
    .expect("interrupted PDF fixture");
    let imported = harness
        .host
        .import_tender_package(ImportTenderPackageCommand {
            tender_id: harness.tender_id.clone(),
            source_path: source.to_string_lossy().into_owned(),
        })
        .expect("import interrupted PDF");
    let document = imported
        .documents
        .iter()
        .find(|document| document.package_path == "slow.pdf")
        .expect("registered interrupted PDF");
    let command = ParseSourceArtifactCommand {
        tender_id: harness.tender_id.clone(),
        artifact_id: document.artifact_id.clone(),
        version: document.version,
    };
    let parse_host = harness.host.clone();
    let parse_command = command.clone();
    let parse = tokio::spawn(async move { parse_host.parse_source_artifact(parse_command).await });

    let mut cancelled = false;
    for _ in 0..100 {
        if harness
            .host
            .cancel_source_artifact_parse(command.clone())
            .expect("cancel parse command")
        {
            cancelled = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(cancelled, "parse never became cancellable");
    let result = parse
        .await
        .expect("parse task")
        .expect("interrupted parse result");
    assert_eq!(result.state, ParseState::Interrupted);
    assert_eq!(result.exception, Some(ParseExceptionCode::Interrupted));
    assert_eq!(
        harness.host.inspect_evidence(command).unwrap_err().code,
        TenderErrorCode::NotFound
    );
    let register = harness
        .host
        .inspect_document_register(&harness.tender_id)
        .expect("inspect interrupted parse state");
    let document = register
        .documents
        .iter()
        .find(|document| document.package_path == "slow.pdf")
        .expect("interrupted Document Register row");
    assert_eq!(document.parse_state, ParseState::Interrupted);
    assert_eq!(
        document.parse_exception,
        Some(ParseExceptionCode::Interrupted)
    );
}

#[tokio::test]
async fn restart_reconciles_a_parse_without_terminal_facts_as_interrupted() {
    let harness = Harness::new();
    let source = harness._root.path().join("restart-source");
    fs::create_dir(&source).expect("restart source directory");
    fs::write(
        source.join("slow.pdf"),
        b"%PDF-1.7\nINTERRUPTED_PROCESS\n%%EOF\n",
    )
    .expect("restart PDF fixture");
    let imported = harness
        .host
        .import_tender_package(ImportTenderPackageCommand {
            tender_id: harness.tender_id.clone(),
            source_path: source.to_string_lossy().into_owned(),
        })
        .expect("import restart PDF");
    let document = imported
        .documents
        .iter()
        .find(|document| document.package_path == "slow.pdf")
        .expect("registered restart PDF");
    let command = ParseSourceArtifactCommand {
        tender_id: harness.tender_id.clone(),
        artifact_id: document.artifact_id.clone(),
        version: document.version,
    };
    let parse_host = harness.host.clone();
    let parse_command = command.clone();
    let parse = tokio::spawn(async move { parse_host.parse_source_artifact(parse_command).await });

    let mut running = false;
    for _ in 0..100 {
        let register = harness
            .host
            .inspect_document_register(&harness.tender_id)
            .expect("inspect running parse");
        if register.documents[0].parse_state == ParseState::Running {
            running = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(
        running,
        "parse attempt never reached persisted running state"
    );
    parse.abort();
    assert!(parse.await.expect_err("aborted parse task").is_cancelled());

    let application_home = harness.application_home.clone();
    let tender_id = harness.tender_id.clone();
    drop(harness.host);
    let restarted =
        QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&restarted).state, SetupState::Ready);
    let register = restarted
        .inspect_document_register(&tender_id)
        .expect("inspect reconciled parse after restart");
    assert_eq!(register.documents[0].parse_state, ParseState::Interrupted);
    assert_eq!(
        register.documents[0].parse_exception,
        Some(ParseExceptionCode::Interrupted)
    );
    assert_eq!(
        restarted
            .inspect_evidence(command)
            .expect_err("interrupted restart must not publish evidence")
            .code,
        TenderErrorCode::NotFound
    );
    assert!(!application_home
        .join("tenders")
        .join(tender_id)
        .join("staging")
        .read_dir()
        .expect("reconciled staging directory")
        .any(|entry| entry.is_ok()));
}

struct Harness {
    _root: tempfile::TempDir,
    application_home: std::path::PathBuf,
    host: QuantixHost,
    tender_id: String,
}

impl Harness {
    fn new() -> Self {
        let root = tempfile::tempdir().expect("temporary parsing harness");
        let application_home = root.path().join(".quantix");
        let host =
            QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
        assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
        install_ocr_fixture(&application_home);
        let tender = host
            .create_tender(CreateTenderCommand {
                name: "Bilingual evidence Tender".into(),
            })
            .expect("create Tender");
        Self {
            _root: root,
            application_home,
            host,
            tender_id: tender.tender_id,
        }
    }
}

#[tokio::test]
async fn registered_pdf_is_parsed_into_exact_bilingual_evidence() {
    let harness = Harness::new();
    let source = harness._root.path().join("source");
    fs::create_dir(&source).expect("source directory");
    fs::write(
        source.join("conditions.pdf"),
        b"%PDF-1.7\n1 0 obj\n(Bilingual conditions)\nendobj\n%%EOF\n",
    )
    .expect("PDF fixture");
    let imported = harness
        .host
        .import_tender_package(ImportTenderPackageCommand {
            tender_id: harness.tender_id.clone(),
            source_path: source.to_string_lossy().into_owned(),
        })
        .expect("import PDF");
    let document = imported
        .documents
        .iter()
        .find(|document| document.package_path == "conditions.pdf")
        .expect("registered PDF");
    let artifact_id = document.artifact_id.clone();
    fs::remove_dir_all(&source).expect("disconnect original package");

    let parsed = harness
        .host
        .parse_source_artifact(ParseSourceArtifactCommand {
            tender_id: harness.tender_id.clone(),
            artifact_id: artifact_id.clone(),
            version: 1,
        })
        .await
        .expect("parse registered PDF");

    assert_eq!(parsed.state, ParseState::Parsed);
    assert_eq!(parsed.location_count, 8);
    assert_eq!(parsed.pipeline_version.as_deref(), Some("1"));
    assert_eq!(parsed.markdown_sha256.as_ref().map(String::len), Some(64));
    let evidence = harness
        .host
        .inspect_evidence(ParseSourceArtifactCommand {
            tender_id: harness.tender_id.clone(),
            artifact_id: artifact_id.clone(),
            version: 1,
        })
        .expect("inspect parsed evidence");
    assert_eq!(evidence.state, ParseState::Parsed);
    assert_eq!(
        evidence
            .locations
            .iter()
            .take(4)
            .map(|location| (location.kind, location.original_text.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (EvidenceLocationKind::Section, "Commercial Conditions",),
            (EvidenceLocationKind::Paragraph, "Bid security is required.",),
            (EvidenceLocationKind::Paragraph, "ضمان العطاء مطلوب."),
            (EvidenceLocationKind::Table, "Item\nPrice\nConcrete\n125000",),
        ]
    );
    assert!(evidence.locations.iter().any(|location| {
        location.kind == EvidenceLocationKind::Paragraph
            && location.original_text == "Bid security is required."
            && location.provenance[0].page_number == 1
            && location.language == EvidenceLanguage::English
            && location.direction == TextDirection::LeftToRight
    }));
    assert!(evidence.locations.iter().any(|location| {
        location.kind == EvidenceLocationKind::Paragraph
            && location.original_text == "ضمان العطاء مطلوب."
            && location.provenance[0].page_number == 2
            && location.language == EvidenceLanguage::Arabic
            && location.direction == TextDirection::RightToLeft
    }));
    assert!(evidence.locations.iter().any(|location| {
        location.kind == EvidenceLocationKind::Section
            && location.original_text == "Commercial Conditions"
    }));
    assert!(evidence.locations.iter().any(|location| {
        location.kind == EvidenceLocationKind::Cell
            && location.cell_range.as_deref() == Some("B2")
            && location.original_text == "125000"
    }));
    let register = harness
        .host
        .inspect_document_register(&harness.tender_id)
        .expect("inspect Document Register");
    let registered = register
        .documents
        .iter()
        .find(|document| document.artifact_id == artifact_id)
        .expect("parsed Document Register row");
    assert_eq!(registered.parse_state, ParseState::Parsed);
    assert_eq!(registered.parse_exception, None);

    let database = harness
        .application_home
        .join("tenders")
        .join(&harness.tender_id)
        .join("tender.sqlite");
    let connection = Connection::open(database).expect("open Tender database side channel");
    assert!(connection
        .execute(
            "UPDATE evidence_locations SET original_text = 'rewritten'\
             WHERE artifact_id = ?1 AND version = 1 AND ordinal = 1",
            [&artifact_id],
        )
        .is_err());
    assert!(connection
        .execute(
            "DELETE FROM parsed_documents WHERE artifact_id = ?1 AND version = 1",
            [&artifact_id],
        )
        .is_err());
    let unchanged = harness
        .host
        .inspect_evidence(ParseSourceArtifactCommand {
            tender_id: harness.tender_id.clone(),
            artifact_id: artifact_id.clone(),
            version: 1,
        })
        .expect("inspect immutable evidence after rejected rewrite");
    assert_ne!(unchanged.locations[0].original_text, "rewritten");
    assert!(!harness
        .application_home
        .join("tenders")
        .join(&harness.tender_id)
        .join("staging")
        .read_dir()
        .expect("staging directory")
        .any(|entry| entry.is_ok()));
}

fn install_ocr_fixture(application_home: &Path) {
    let executable = application_home
        .join("runtimes")
        .join("ocr")
        .join(if cfg!(windows) { "Scripts" } else { "bin" })
        .join(executable_name("python"));
    fs::create_dir_all(executable.parent().expect("OCR executable parent"))
        .expect("OCR executable directory");
    fs::copy(
        Path::new(env!("CARGO_BIN_EXE_quantix-runtime-fixture")),
        &executable,
    )
    .expect("install OCR fixture");
    fs::write(executable.with_extension("version"), "3.9.2\n").expect("OCR fixture version");
    let models = application_home.join("models").join("ocr");
    fs::create_dir_all(&models).expect("model directory");
    for artifact in [
        "PP-OCRv6_det_small.onnx",
        "PP-OCRv6_rec_small.onnx",
        "ch_ppocr_mobile_v2.0_cls_mobile.onnx",
    ] {
        fs::write(models.join(artifact), format!("{artifact} fixture model"))
            .expect("model fixture");
    }
}

fn executable_name(name: &str) -> String {
    if std::env::consts::EXE_EXTENSION.is_empty() {
        name.to_owned()
    } else {
        format!("{name}.{}", std::env::consts::EXE_EXTENSION)
    }
}

fn office_package(required_entry: &str) -> Vec<u8> {
    let mut bytes = std::io::Cursor::new(Vec::new());
    {
        let mut zip = ZipWriter::new(&mut bytes);
        let stored = SimpleFileOptions::default();
        zip.start_file("[Content_Types].xml", stored)
            .expect("content types entry");
        zip.write_all(b"<Types/>").expect("content types");
        zip.start_file(required_entry, stored)
            .expect("required OOXML entry");
        zip.write_all(
            "<document><text>Bid requirement</text><text>متطلب العطاء</text></document>".as_bytes(),
        )
        .expect("bilingual OOXML document");
        zip.finish().expect("finish OOXML package");
    }
    bytes.into_inner()
}
