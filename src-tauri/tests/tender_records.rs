use std::{fs, io, path::Path, sync::Arc};

use quantix_lib::{
    ensure_quantix_setup, AgentRunRecoveryDisposition, AgentRunState, BootstrapAuthority,
    BootstrapRole, ChangeAssessmentClassification, ChangeAssessmentStatus,
    ConfirmSourceRelationshipCommand, CreateTenderCommand, CreateTenderEngineerEntryCommand,
    DecideChangeAssessmentCommand, DecideTenderRecordCommand, ImportTenderPackageCommand,
    InspectChangeAssessmentsCommand, OutputValidationIssue, ParseSourceArtifactCommand,
    ProviderFailureCategory, QuantixHost, ResolveIndeterminateAgentRunCommand, ReviseTenderCommand,
    RunTenderRecordExtractionCommand, RunTenderRecordReviewCommand, RuntimeLayout, SetupPlatform,
    SetupState, SourceRelationshipKind, StoragePermissions, TenderErrorCode,
    TenderEvidenceReference, TenderIntegrityState, TenderLifecyclePhase,
    TenderRecordAuthorityReference, TenderRecordBasisKind, TenderRecordEngineerDecisionKind,
    TenderRecordInspection, TenderRecordKind, TenderRecordReviewOutcome, TenderRecordTrustClass,
    VerificationStatus, MINIMUM_SETUP_FREE_SPACE_BYTES,
};
use sha2::{Digest, Sha256};

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

struct RuntimeHarness {
    _root: tempfile::TempDir,
    application_home: std::path::PathBuf,
    codex: std::path::PathBuf,
    host: QuantixHost,
    tender_id: String,
}

struct ParsedEvidence {
    artifact_id: String,
    version: u32,
    references: Vec<TenderEvidenceReference>,
}

impl RuntimeHarness {
    fn new(agent_scenario: &str) -> Self {
        let root = tempfile::tempdir().expect("temporary Tender Records runtime harness");
        let application_home = root.path().join(".quantix");
        let resources = root.path().join("resources");
        let codex = install_codex_fixture(&resources, agent_scenario);
        let host = QuantixHost::with_setup_platform_and_runtime(
            &application_home,
            Arc::new(ReadySetupPlatform),
            RuntimeLayout::bundled(resources),
        );
        host.accept_runtime_fixture();
        assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
        host.approve_runtime_fixture_ai_selection()
            .expect("approve fixture AI selection");
        install_ocr_fixture(&application_home);
        let tender = host
            .create_tender(CreateTenderCommand {
                name: "Bilingual evidence-backed Tender".into(),
            })
            .expect("create Tender");
        Self {
            _root: root,
            application_home,
            codex,
            host,
            tender_id: tender.tender_id,
        }
    }

    fn set_agent_scenario(&self, scenario: &str) {
        fs::write(self.codex.with_extension("agent-scenario"), scenario)
            .expect("update fake app-server scenario");
        self.host
            .approve_runtime_fixture_ai_selection()
            .expect("restore approved fixture provider readiness");
    }

    async fn parsed_pdf_evidence(&self, name: &str, marker: &[u8]) -> ParsedEvidence {
        self.parsed_pdf_evidence_package(name, &[(name, marker)])
            .await
            .into_iter()
            .next()
            .expect("single parsed PDF Evidence")
    }

    async fn parsed_pdf_evidence_package(
        &self,
        package_name: &str,
        documents: &[(&str, &[u8])],
    ) -> Vec<ParsedEvidence> {
        let source = self._root.path().join(format!("{package_name}-source"));
        fs::create_dir(&source).expect("source directory");
        for (name, marker) in documents {
            let mut bytes = b"%PDF-1.7\n".to_vec();
            bytes.extend_from_slice(marker);
            bytes.extend_from_slice(b"\n%%EOF\n");
            fs::write(source.join(format!("{name}.pdf")), bytes).expect("PDF fixture");
        }
        let imported = self
            .host
            .import_tender_package(ImportTenderPackageCommand {
                tender_id: self.tender_id.clone(),
                source_path: source.to_string_lossy().into_owned(),
            })
            .expect("import PDF");
        let mut parsed = Vec::with_capacity(documents.len());
        for (name, _) in documents {
            let package_path = format!("{name}.pdf");
            let document = imported
                .documents
                .iter()
                .find(|document| document.package_path == package_path)
                .expect("registered package PDF");
            self.host
                .parse_source_artifact(ParseSourceArtifactCommand {
                    tender_id: self.tender_id.clone(),
                    artifact_id: document.artifact_id.clone(),
                    version: document.version,
                })
                .await
                .expect("parse PDF");
            let references = self
                .host
                .inspect_evidence(ParseSourceArtifactCommand {
                    tender_id: self.tender_id.clone(),
                    artifact_id: document.artifact_id.clone(),
                    version: document.version,
                })
                .expect("inspect parsed evidence")
                .locations
                .into_iter()
                .map(|location| TenderEvidenceReference {
                    artifact_id: document.artifact_id.clone(),
                    version: document.version,
                    ordinal: location.ordinal,
                })
                .collect();
            parsed.push(ParsedEvidence {
                artifact_id: document.artifact_id.clone(),
                version: document.version,
                references,
            });
        }
        parsed
    }
}

fn inspect_all_records(host: &QuantixHost, tender_id: &str) -> Vec<TenderRecordInspection> {
    let mut cursor = None;
    let mut records = Vec::new();
    loop {
        let page = host
            .inspect_tender_record_page(tender_id, cursor.as_deref(), 4)
            .expect("inspect bounded Tender Record page");
        records.extend(page.records);
        let Some(next_cursor) = page.next_cursor else {
            return records;
        };
        cursor = Some(next_cursor);
    }
}

fn records_for_run(
    host: &QuantixHost,
    tender_id: &str,
    run_id: &str,
) -> Vec<TenderRecordInspection> {
    inspect_all_records(host, tender_id)
        .into_iter()
        .filter(|record| record.author_run_id == run_id)
        .collect()
}

fn tender_record_extraction_runs(
    application_home: &Path,
    tender_id: &str,
) -> Vec<(String, Option<String>, String)> {
    let database = application_home
        .join("tenders")
        .join(tender_id)
        .join("tender.sqlite");
    let connection = rusqlite::Connection::open(database).expect("open Tender Store database");
    let runs = connection
        .prepare(
            "SELECT agent_runs.run_id, agent_runs.retry_of_run_id, agent_runs.status
             FROM agent_runs
             JOIN tender_tasks USING (task_id)
             WHERE tender_tasks.objective LIKE 'Propose evidence-backed requirements%'
             ORDER BY agent_runs.run_sequence",
        )
        .expect("prepare extraction runs")
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .expect("query extraction runs")
        .collect::<rusqlite::Result<Vec<_>>>()
        .expect("collect extraction runs");
    runs
}

#[tokio::test]
async fn manager_intake_automatically_repairs_one_invalid_extraction() {
    let harness = RuntimeHarness::new("manager-intake-repair-invalid-then-valid");
    harness
        .parsed_pdf_evidence("repair-invalid-then-valid", b"TENDER_RECORD_GOLDEN")
        .await;
    fs::write(
        harness.codex.with_extension("manager-output-release"),
        b"release",
    )
    .expect("release Manager outcome");

    harness
        .host
        .run_manager_intake_for_verification(&harness.tender_id)
        .await
        .expect("repair one invalid extraction and finish Manager intake");

    let runs = tender_record_extraction_runs(&harness.application_home, &harness.tender_id);
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0].2, "failed");
    assert_eq!(runs[1].1.as_deref(), Some(runs[0].0.as_str()));
    assert_eq!(runs[1].2, "completed");
    assert!(
        records_for_run(&harness.host, &harness.tender_id, &runs[0].0).is_empty(),
        "the rejected attempt must never publish records"
    );
    assert!(
        !records_for_run(&harness.host, &harness.tender_id, &runs[1].0).is_empty(),
        "only the completed repair may publish records"
    );

    let database = harness
        .application_home
        .join("tenders")
        .join(&harness.tender_id)
        .join("tender.sqlite");
    let connection = rusqlite::Connection::open(database).expect("open Tender Store database");
    let (source_inputs, source_feedback, repair_inputs, repair_feedback): (
        String,
        Option<String>,
        String,
        Option<String>,
    ) = connection
        .query_row(
            "SELECT source_task.exact_inputs_json, source_task.repair_feedback_json,
                    repair_task.exact_inputs_json, repair_task.repair_feedback_json
             FROM agent_runs AS source_run
             JOIN tender_tasks AS source_task ON source_task.task_id = source_run.task_id
             JOIN agent_runs AS repair_run ON repair_run.retry_of_run_id = source_run.run_id
             JOIN tender_tasks AS repair_task ON repair_task.task_id = repair_run.task_id
             WHERE source_run.run_id = ?1",
            [&runs[0].0],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .expect("load immutable repair task lineage");
    assert_eq!(source_inputs, repair_inputs);
    assert!(source_feedback.is_none());
    let repair_feedback = repair_feedback.expect("repair feedback is immutable task data");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&repair_feedback)
            .expect("canonical repair feedback")
            .pointer("/rejected_run_id")
            .and_then(serde_json::Value::as_str),
        Some(runs[0].0.as_str())
    );
    let extraction_run_count: u32 = connection
        .query_row(
            "SELECT extraction_run_count FROM manager_intake_runs",
            [],
            |row| row.get(0),
        )
        .expect("read Manager extraction count");
    assert_eq!(extraction_run_count, 1);
}

#[tokio::test]
async fn manager_intake_stops_after_one_failed_repair() {
    let harness = RuntimeHarness::new("manager-intake-repair-invalid-twice");
    harness
        .parsed_pdf_evidence("repair-invalid-twice", b"TENDER_RECORD_GOLDEN")
        .await;

    let error = harness
        .host
        .run_manager_intake_for_verification(&harness.tender_id)
        .await
        .expect_err("two invalid extraction attempts stop without a third call");
    assert_eq!(error.code, TenderErrorCode::InvalidCommand);

    let runs = tender_record_extraction_runs(&harness.application_home, &harness.tender_id);
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0].2, "failed");
    assert_eq!(runs[1].1.as_deref(), Some(runs[0].0.as_str()));
    assert_eq!(runs[1].2, "failed");
    assert!(records_for_run(&harness.host, &harness.tender_id, &runs[0].0).is_empty());
    assert!(records_for_run(&harness.host, &harness.tender_id, &runs[1].0).is_empty());

    let database = harness
        .application_home
        .join("tenders")
        .join(&harness.tender_id)
        .join("tender.sqlite");
    let connection = rusqlite::Connection::open(database).expect("open Tender Store database");
    let rejected_count: u32 = connection
        .query_row(
            "SELECT COUNT(*) FROM agent_run_rejected_outputs",
            [],
            |row| row.get(0),
        )
        .expect("count rejected provider outputs");
    assert_eq!(rejected_count, 2);
    let extraction_run_count: u32 = connection
        .query_row(
            "SELECT extraction_run_count FROM manager_intake_runs",
            [],
            |row| row.get(0),
        )
        .expect("read Manager extraction count");
    assert_eq!(extraction_run_count, 0);
}

#[tokio::test]
async fn rejected_output_is_persisted_with_failure_atomically() {
    let harness = RuntimeHarness::new("record-extraction-parity-duplicate-stable-key");
    let evidence = harness
        .parsed_pdf_evidence("rejected-output", b"TENDER_RECORD_GOLDEN")
        .await;

    let extraction = harness
        .host
        .run_tender_record_extraction(RunTenderRecordExtractionCommand {
            tender_id: harness.tender_id.clone(),
            evidence: evidence.references,
            authorities: Vec::new(),
        })
        .await
        .expect("terminalize domain-invalid provider output");

    assert_eq!(extraction.run.state, AgentRunState::Failed);
    let failure = extraction.run.failure.expect("OutputInvalid failure");
    assert_eq!(failure.category, ProviderFailureCategory::OutputInvalid);
    assert_eq!(failure.validation_issues.len(), 1);
    assert_eq!(failure.validation_issues[0].code, "duplicate_stable_key");
    assert_eq!(failure.validation_issues[0].path, "/records/1/stable_key");
    assert!(extraction.run.proposed_result.is_none());
    let runs = harness
        .host
        .inspect_agent_runs(&harness.tender_id)
        .expect("inspect rejected extraction run");
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].state, AgentRunState::Failed);
    assert!(records_for_run(&harness.host, &harness.tender_id, &extraction.run.run_id).is_empty());

    let database = harness
        .application_home
        .join("tenders")
        .join(&harness.tender_id)
        .join("tender.sqlite");
    let connection = rusqlite::Connection::open(database).expect("open Tender Store database");
    let rejected_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM agent_run_rejected_outputs WHERE run_id = ?1",
            [&extraction.run.run_id],
            |row| row.get(0),
        )
        .expect("count rejected output rows");
    assert_eq!(rejected_count, 1);
    let (payload_json, payload_sha256, validation_issues_json): (String, String, String) =
        connection
            .query_row(
                "SELECT payload_json, payload_sha256, validation_issues_json
             FROM agent_run_rejected_outputs WHERE run_id = ?1",
                [&extraction.run.run_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("load rejected output");
    assert_eq!(
        payload_json,
        serde_json_canonicalizer::to_string(
            &serde_json::from_str::<serde_json::Value>(&payload_json)
                .expect("stored rejected payload is JSON"),
        )
        .expect("canonicalize stored rejected payload")
    );
    assert_eq!(
        payload_sha256,
        Sha256::digest(payload_json.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    );
    assert_eq!(
        serde_json::from_str::<Vec<OutputValidationIssue>>(&validation_issues_json)
            .expect("stored validation issues"),
        failure.validation_issues
    );
    let rejected = harness
        .host
        .rejected_agent_output(&harness.tender_id, &extraction.run.run_id)
        .expect("inspect rejected provider output");
    assert_eq!(rejected.payload_json, payload_json);
    assert_eq!(rejected.payload_sha256, payload_sha256);
    assert_eq!(rejected.validation_issues, failure.validation_issues);
    assert!(connection
        .execute(
            "UPDATE agent_run_rejected_outputs SET payload_sha256 = ?1 WHERE run_id = ?2",
            ["0".repeat(64), extraction.run.run_id.clone()],
        )
        .is_err());
    assert!(connection
        .execute(
            "DELETE FROM agent_run_rejected_outputs WHERE run_id = ?1",
            [&extraction.run.run_id],
        )
        .is_err());
}

#[tokio::test]
async fn cancellation_before_completion_discards_invalid_provider_output() {
    let harness = RuntimeHarness::new("record-extraction-parity-duplicate-stable-key-delayed");
    let evidence = harness
        .parsed_pdf_evidence("rejected-output-cancel", b"TENDER_RECORD_GOLDEN")
        .await;
    let host = harness.host.clone();
    let tender_id = harness.tender_id.clone();
    let extraction_task = tokio::spawn(async move {
        host.run_tender_record_extraction(RunTenderRecordExtractionCommand {
            tender_id,
            evidence: evidence.references,
            authorities: Vec::new(),
        })
        .await
    });
    let waiting = harness.codex.with_extension("record-output-waiting");
    for _ in 0..1_000 {
        if waiting.is_file() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    assert!(
        waiting.is_file(),
        "provider did not reach delayed output boundary"
    );
    let run_id = harness
        .host
        .inspect_agent_runs(&harness.tender_id)
        .expect("inspect active extraction run")
        .into_iter()
        .find(|run| run.state == AgentRunState::Running)
        .expect("running extraction run")
        .run_id;
    let database = harness
        .application_home
        .join("tenders")
        .join(&harness.tender_id)
        .join("tender.sqlite");
    let connection = rusqlite::Connection::open(database).expect("open Tender Store database");
    connection
        .execute(
            "INSERT INTO agent_run_cancellations (run_id, requested_by, requested_at)
             VALUES (?1, 'engineer_user', '2026-08-23T00:00:00Z')",
            [&run_id],
        )
        .expect("record cancellation before completion");
    fs::write(
        harness.codex.with_extension("record-output-release"),
        b"release",
    )
    .expect("release invalid provider output");

    let extraction = extraction_task
        .await
        .expect("join cancelled extraction")
        .expect("terminalize cancelled extraction");
    assert_eq!(extraction.run.state, AgentRunState::Interrupted);
    assert_eq!(
        extraction.run.failure.map(|failure| failure.category),
        Some(ProviderFailureCategory::Interrupted)
    );
    assert!(extraction.run.proposed_result.is_none());
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM agent_run_rejected_outputs WHERE run_id = ?1",
                [&run_id],
                |row| row.get::<_, i64>(0),
            )
            .expect("count rejected outputs"),
        0
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM proposed_agent_results WHERE run_id = ?1",
                [&run_id],
                |row| row.get::<_, i64>(0),
            )
            .expect("count proposed outputs"),
        0
    );
    assert!(records_for_run(&harness.host, &harness.tender_id, &run_id).is_empty());
}

#[tokio::test]
async fn rejected_output_insert_failure_rolls_back_agent_completion() {
    let harness = RuntimeHarness::new("record-extraction-parity-duplicate-stable-key-delayed");
    let evidence = harness
        .parsed_pdf_evidence("rejected-output-rollback", b"TENDER_RECORD_GOLDEN")
        .await;
    let host = harness.host.clone();
    let tender_id = harness.tender_id.clone();
    let extraction_task = tokio::spawn(async move {
        host.run_tender_record_extraction(RunTenderRecordExtractionCommand {
            tender_id,
            evidence: evidence.references,
            authorities: Vec::new(),
        })
        .await
    });
    let waiting = harness.codex.with_extension("record-output-waiting");
    for _ in 0..1_000 {
        if waiting.is_file() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    assert!(
        waiting.is_file(),
        "provider did not reach delayed output boundary"
    );
    let run_id = harness
        .host
        .inspect_agent_runs(&harness.tender_id)
        .expect("inspect active extraction run")
        .into_iter()
        .find(|run| run.state == AgentRunState::Running)
        .expect("running extraction run")
        .run_id;
    let database = harness
        .application_home
        .join("tenders")
        .join(&harness.tender_id)
        .join("tender.sqlite");
    let connection = rusqlite::Connection::open(database).expect("open Tender Store database");
    let state_before: String = connection
        .query_row(
            "SELECT status FROM agent_runs WHERE run_id = ?1",
            [&run_id],
            |row| row.get(0),
        )
        .expect("inspect running state");
    let event_count_before: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM provider_events WHERE run_id = ?1",
            [&run_id],
            |row| row.get(0),
        )
        .expect("count provider events before completion");
    let audit_count_before: i64 = connection
        .query_row("SELECT COUNT(*) FROM audit_events", [], |row| row.get(0))
        .expect("count audit events before completion");
    connection
        .execute_batch(
            "CREATE TRIGGER rejected_output_insert_abort
             BEFORE INSERT ON agent_run_rejected_outputs
             BEGIN
               SELECT RAISE(ABORT, 'test rejected output insert failure');
             END;",
        )
        .expect("install rejected-output insert abort trigger");
    fs::write(
        harness.codex.with_extension("record-output-release"),
        b"release",
    )
    .expect("release invalid provider output");

    extraction_task
        .await
        .expect("join failed extraction")
        .expect_err("rejected-output insertion aborts completion");
    assert_eq!(
        connection
            .query_row(
                "SELECT status FROM agent_runs WHERE run_id = ?1",
                [&run_id],
                |row| row.get::<_, String>(0),
            )
            .expect("inspect rolled-back run state"),
        state_before
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM provider_events WHERE run_id = ?1",
                [&run_id],
                |row| row.get::<_, i64>(0),
            )
            .expect("count rolled-back provider events"),
        event_count_before
    );
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM audit_events", [], |row| row
                .get::<_, i64>(0))
            .expect("count rolled-back audit events"),
        audit_count_before
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM agent_run_rejected_outputs WHERE run_id = ?1",
                [&run_id],
                |row| row.get::<_, i64>(0),
            )
            .expect("count rolled-back rejected outputs"),
        0
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM proposed_agent_results WHERE run_id = ?1",
                [&run_id],
                |row| row.get::<_, i64>(0),
            )
            .expect("count rolled-back proposed outputs"),
        0
    );
    assert!(records_for_run(&harness.host, &harness.tender_id, &run_id).is_empty());
}

#[tokio::test]
async fn schema_domain_parity_reports_stable_paths() {
    let cases = [
        (
            "record-extraction-parity-duplicate-stable-key",
            "duplicate_stable_key",
            "/records/1/stable_key",
            "Record stable keys must be unique.",
        ),
        (
            "record-extraction-parity-whitespace-title",
            "blank_title",
            "/records/0/title",
            "Record titles cannot be blank.",
        ),
        (
            "record-extraction-parity-utf8-title",
            "title_too_long",
            "/records/0/title",
            "Record titles must not exceed 500 bytes.",
        ),
        (
            "record-extraction-parity-utf8-field-value",
            "field_value_too_long",
            "/records/0/fields/0/value",
            "Field content exceeds its byte limit.",
        ),
        (
            "record-extraction-parity-duplicate-field",
            "duplicate_field_name",
            "/records/0/fields/1/name",
            "Record field names must be unique.",
        ),
        (
            "record-extraction-parity-duplicate-evidence",
            "duplicate_evidence",
            "/records/0/fields/0/basis/evidence",
            "Evidence references must be unique.",
        ),
        (
            "record-extraction-parity-evidence-metadata",
            "schema_rejection",
            "",
            "Provider output is not a valid Tender Record proposal.",
        ),
        (
            "record-extraction-parity-authoring-format",
            "invalid_authoring_format",
            "/records/0/generation_instruction/requested_authoring_format",
            "Generation instruction authoring format is invalid.",
        ),
        (
            "record-extraction-parity-deadline",
            "invalid_deadline",
            "/records/2/fields/0",
            "Deadline fields require a valid parsed deadline.",
        ),
        (
            "record-extraction-parity-contradiction",
            "invalid_contradiction_evidence",
            "/records/2/contradictions/0/evidence",
            "Contradictions require at least two distinct evidence references.",
        ),
        (
            "record-extraction-parity-foreign-authority",
            "schema_rejection",
            "/records/0/fields/0/basis/authority",
            "Authority handle is not available to this Tender task.",
        ),
    ];
    for (scenario, code, path, message) in cases {
        let harness = RuntimeHarness::new(scenario);
        let evidence = harness
            .parsed_pdf_evidence("parity", b"TENDER_RECORD_GOLDEN")
            .await;
        let result = harness
            .host
            .run_tender_record_extraction(RunTenderRecordExtractionCommand {
                tender_id: harness.tender_id.clone(),
                evidence: evidence.references,
                authorities: Vec::new(),
            })
            .await
            .expect("run parity scenario");
        let failure = result.run.failure.expect("invalid output failure");
        assert_eq!(
            failure.category,
            ProviderFailureCategory::OutputInvalid,
            "{scenario}"
        );
        if code == "schema_rejection" {
            assert!(
                failure.validation_issues.is_empty(),
                "{scenario}: {failure:#?}"
            );
            continue;
        }
        assert!(
            !failure.validation_issues.is_empty(),
            "{scenario} did not retain its validation report: {failure:#?}"
        );
        assert_eq!(
            failure.validation_issues[0],
            OutputValidationIssue {
                code: code.into(),
                path: path.into(),
                message: message.into(),
            },
            "{scenario}"
        );
    }
}

#[test]
fn a_new_tender_activates_only_the_restricted_bootstrap_team() {
    let root = tempfile::tempdir().expect("temporary Tender Records harness");
    let application_home = root.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Evidence-backed Tender".into(),
        })
        .expect("create Tender");

    let team = host
        .inspect_bootstrap_team(&tender.tender_id)
        .expect("inspect Bootstrap Team");

    assert_eq!(
        team.iter().map(|member| member.role).collect::<Vec<_>>(),
        vec![
            BootstrapRole::TenderingManager,
            BootstrapRole::DocumentController,
            BootstrapRole::TenderAnalyst,
            BootstrapRole::IndependentReviewer,
        ]
    );
    assert!(team.iter().all(|member| member.active));
    assert!(team.iter().all(|member| {
        member.authority == BootstrapAuthority::PreBidAnalysis
            && !member.profile.permissions.network_allowed
    }));
}

#[tokio::test]
async fn agent_record_proposals_preserve_exact_original_evidence_and_explicit_gaps() {
    let harness = RuntimeHarness::new("record-extraction");
    let evidence = harness
        .parsed_pdf_evidence("conditions", b"TENDER_RECORD_GOLDEN")
        .await;

    let extraction = harness
        .host
        .run_tender_record_extraction(RunTenderRecordExtractionCommand {
            tender_id: harness.tender_id.clone(),
            evidence: evidence.references,
            authorities: Vec::new(),
        })
        .await
        .expect("run bounded Tender Record extraction");

    assert_eq!(
        extraction.run.state,
        AgentRunState::Completed,
        "{:#?}\nfixture error: {}",
        extraction.run,
        fs::read_to_string(harness.codex.with_extension("fixture-error"))
            .unwrap_or_else(|_| "none".into())
    );
    assert_eq!(
        extraction.run.profile.identity, "Tender Analyst",
        "record extraction must use the restricted Bootstrap Team role"
    );
    let records = records_for_run(&harness.host, &harness.tender_id, &extraction.run.run_id);
    assert_eq!(extraction.published_record_count as usize, records.len());
    assert!(
        records.iter().any(|record| {
            record.kind == TenderRecordKind::Requirement
                && record.verification_status == VerificationStatus::Proposed
                && record.trust_class == TenderRecordTrustClass::AiProposal
                && record.fields.iter().any(|field| {
                    field.evidence.iter().any(|evidence| {
                        !evidence.location.original_text.is_empty()
                            && evidence
                                .location
                                .translated_text
                                .as_deref()
                                .is_some_and(|translation| !translation.is_empty())
                    })
                })
        }),
        "{:#?}",
        records
    );
    assert!(records.iter().any(|record| {
        record.kind == TenderRecordKind::Assumption
            && record.trust_class == TenderRecordTrustClass::UnresolvedGap
            && record.fields.iter().all(|field| field.evidence.is_empty())
    }));
    assert!(records.iter().any(|record| {
        record.stable_key == "authoritative_notice"
            && record.trust_class == TenderRecordTrustClass::AiProposal
            && record.fields.iter().all(|field| {
                field.value.as_deref().is_some_and(|value| {
                    field
                        .evidence
                        .iter()
                        .any(|evidence| evidence.location.original_text == value)
                })
            })
    }));
    let deadline = records
        .iter()
        .find(|record| record.kind == TenderRecordKind::Deadline)
        .expect("deadline record");
    assert!(deadline.fields.iter().any(|field| {
        field.original_expression.as_deref() == Some("15 May 2026 at 14:00 Cairo time")
            && field.timezone.as_deref() == Some("Africa/Cairo")
            && field.normalized_value.as_deref() == Some("2026-05-15T14:00:00+03:00")
            && field.uncertainty.is_some()
    }));
    assert!(deadline
        .contradictions
        .iter()
        .any(|contradiction| contradiction.evidence.len() >= 2));
}

#[tokio::test]
async fn record_extraction_accepts_insignificant_json_whitespace() {
    let harness = RuntimeHarness::new("record-extraction-insignificant-space");
    let evidence = harness
        .parsed_pdf_evidence("insignificant-space", b"TENDER_RECORD_GOLDEN")
        .await;

    let extraction = harness
        .host
        .run_tender_record_extraction(RunTenderRecordExtractionCommand {
            tender_id: harness.tender_id.clone(),
            evidence: evidence.references,
            authorities: Vec::new(),
        })
        .await
        .expect("emit whitespace-variant record candidate");
    let candidate = fs::read_to_string(harness.codex.with_extension("candidate-json"))
        .expect("fixture whitespace-variant candidate");
    assert!(candidate.starts_with("{ "), "{candidate}");
    let value: serde_json::Value =
        serde_json::from_str(&candidate).expect("valid whitespace-variant candidate");
    let canonical = serde_json_canonicalizer::to_string(&value).expect("canonical candidate");
    assert_ne!(candidate, canonical);

    assert_eq!(
        extraction.run.state,
        AgentRunState::Completed,
        "{:#?}; expected canonicalization before domain validation",
        extraction.run
    );
    assert!(extraction.published_record_count > 0);
}

#[tokio::test]
async fn exact_attributable_engineer_entries_can_support_material_fields() {
    let harness = RuntimeHarness::new("record-extraction");
    let evidence = harness
        .parsed_pdf_evidence("engineer-entry", b"TENDER_RECORD_GOLDEN")
        .await;
    let authority = harness
        .host
        .create_tender_engineer_entry(CreateTenderEngineerEntryCommand {
            tender_id: harness.tender_id.clone(),
            value: "C40/50".into(),
            description: "Engineer-selected concrete strength class for the pre-bid basis.".into(),
        })
        .expect("create immutable Engineer entry");

    let extraction = harness
        .host
        .run_tender_record_extraction(RunTenderRecordExtractionCommand {
            tender_id: harness.tender_id.clone(),
            evidence: evidence.references,
            authorities: vec![TenderRecordAuthorityReference {
                authority_id: authority.authority_id.clone(),
            }],
        })
        .await
        .expect("extract with exact Engineer authority");
    let records = records_for_run(&harness.host, &harness.tender_id, &extraction.run.run_id);
    let field = records
        .iter()
        .flat_map(|record| &record.fields)
        .find(|field| field.basis_kind == TenderRecordBasisKind::EngineerEntry)
        .expect("Engineer-backed material field");

    assert_eq!(field.value.as_deref(), Some("C40/50"));
    assert_eq!(
        field.basis_reference.as_deref(),
        Some(authority.authority_id.as_str())
    );
    assert_eq!(field.basis_authority.as_ref(), Some(&authority));
}

#[tokio::test]
async fn record_history_is_exposed_only_through_stable_bounded_pages() {
    let harness = RuntimeHarness::new("record-extraction");
    let evidence = harness
        .parsed_pdf_evidence("paged-records", b"TENDER_RECORD_GOLDEN")
        .await;
    harness
        .host
        .run_tender_record_extraction(RunTenderRecordExtractionCommand {
            tender_id: harness.tender_id.clone(),
            evidence: evidence.references,
            authorities: Vec::new(),
        })
        .await
        .expect("extract paged records");

    let first = harness
        .host
        .inspect_tender_record_page(&harness.tender_id, None, 2)
        .expect("first bounded page");
    assert_eq!(first.records.len(), 2);
    let cursor = first.next_cursor.expect("next page cursor");
    let second = harness
        .host
        .inspect_tender_record_page(&harness.tender_id, Some(&cursor), 2)
        .expect("second bounded page");
    assert_eq!(second.records.len(), 2);
    assert!(first.records.iter().all(|left| {
        second.records.iter().all(|right| {
            (left.record_id.as_str(), left.version) != (right.record_id.as_str(), right.version)
        })
    }));
    assert!(harness
        .host
        .inspect_tender_record_page(&harness.tender_id, None, 5)
        .is_err());
}

#[test]
fn local_engineer_entries_do_not_require_codex_runtime_readiness() {
    let root = tempfile::tempdir().expect("temporary local Engineer entry harness");
    let application_home = root.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Local authority Tender".into(),
        })
        .expect("create Tender without runtime readiness");

    let entry = host
        .create_tender_engineer_entry(CreateTenderEngineerEntryCommand {
            tender_id: tender.tender_id,
            value: "Engineer exact value".into(),
            description: "Attributable local engineering basis.".into(),
        })
        .expect("create local Engineer entry without Codex");

    assert_eq!(entry.created_by, "engineer_user");
}

#[tokio::test]
async fn unsupported_critical_claims_fail_provenance_validation_without_publication() {
    let harness = RuntimeHarness::new("record-extraction-invalid");
    let evidence = harness
        .parsed_pdf_evidence("unsupported", b"TENDER_RECORD_GOLDEN")
        .await;

    let extraction = harness
        .host
        .run_tender_record_extraction(RunTenderRecordExtractionCommand {
            tender_id: harness.tender_id.clone(),
            evidence: evidence.references,
            authorities: Vec::new(),
        })
        .await
        .expect("record invalid provider output as a failed Agent Run");

    assert_eq!(extraction.run.state, AgentRunState::Failed);
    assert_eq!(
        extraction
            .run
            .failure
            .as_ref()
            .map(|failure| failure.category),
        Some(ProviderFailureCategory::OutputInvalid)
    );
    assert_eq!(extraction.published_record_count, 0);
    assert!(inspect_all_records(&harness.host, &harness.tender_id).is_empty());
}

#[tokio::test]
async fn duplicate_citations_cannot_manufacture_a_contradiction() {
    let harness = RuntimeHarness::new("record-extraction-duplicate-citation");
    let evidence = harness
        .parsed_pdf_evidence("duplicate-citation", b"TENDER_RECORD_GOLDEN")
        .await;

    let extraction = harness
        .host
        .run_tender_record_extraction(RunTenderRecordExtractionCommand {
            tender_id: harness.tender_id.clone(),
            evidence: evidence.references,
            authorities: Vec::new(),
        })
        .await
        .expect("record duplicate-citation output as a failed Agent Run");

    assert_eq!(extraction.run.state, AgentRunState::Failed);
    assert_eq!(
        extraction.run.failure.map(|failure| failure.category),
        Some(ProviderFailureCategory::OutputInvalid)
    );
    assert!(inspect_all_records(&harness.host, &harness.tender_id).is_empty());
}

#[tokio::test]
async fn extraction_output_cannot_publish_after_the_tender_revision_changes() {
    let harness = RuntimeHarness::new("record-extraction-delayed");
    let evidence = harness
        .parsed_pdf_evidence("stale-run", b"TENDER_RECORD_GOLDEN")
        .await;
    let host = harness.host.clone();
    let tender_id = harness.tender_id.clone();
    let extraction_task = tokio::spawn(async move {
        host.run_tender_record_extraction(RunTenderRecordExtractionCommand {
            tender_id,
            evidence: evidence.references,
            authorities: Vec::new(),
        })
        .await
    });
    let waiting = harness.codex.with_extension("record-output-waiting");
    for _ in 0..1_000 {
        if waiting.is_file() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    assert!(
        waiting.is_file(),
        "provider did not reach delayed output boundary"
    );
    harness
        .host
        .revise_tender(ReviseTenderCommand {
            tender_id: harness.tender_id.clone(),
            name: "Tender revised while extraction was running".into(),
        })
        .expect("revise Tender while provider turn is in flight");
    fs::write(
        harness.codex.with_extension("record-output-release"),
        b"release",
    )
    .expect("release delayed provider output");

    let extraction = extraction_task
        .await
        .expect("join extraction task")
        .expect("record stale output as terminal failed run");
    assert_eq!(extraction.run.state, AgentRunState::Failed);
    assert_eq!(
        extraction.run.failure.map(|failure| failure.category),
        Some(ProviderFailureCategory::OutputInvalid)
    );
    assert_eq!(extraction.published_record_count, 0);
    assert!(inspect_all_records(&harness.host, &harness.tender_id).is_empty());
}

#[tokio::test]
async fn record_runs_require_fresh_exact_workflow_reruns_not_bootstrap_linked_retries() {
    let harness = RuntimeHarness::new("record-extraction-malformed-after-turn");
    let evidence = harness
        .parsed_pdf_evidence("record-retry-boundary", b"TENDER_RECORD_GOLDEN")
        .await;
    let extraction = harness
        .host
        .run_tender_record_extraction(RunTenderRecordExtractionCommand {
            tender_id: harness.tender_id.clone(),
            evidence: evidence.references,
            authorities: Vec::new(),
        })
        .await
        .expect("persist indeterminate record extraction");

    assert_eq!(extraction.run.state, AgentRunState::Indeterminate);
    assert!(!extraction.run.linked_retry_supported);
    assert!(harness
        .host
        .resolve_indeterminate_agent_run(ResolveIndeterminateAgentRunCommand {
            tender_id: harness.tender_id.clone(),
            run_id: extraction.run.run_id.clone(),
            disposition: AgentRunRecoveryDisposition::RetryTask,
            rationale: "Attempt a generic retry.".into(),
        })
        .is_err());
    let closed = harness
        .host
        .resolve_indeterminate_agent_run(ResolveIndeterminateAgentRunCommand {
            tender_id: harness.tender_id.clone(),
            run_id: extraction.run.run_id,
            disposition: AgentRunRecoveryDisposition::CloseTask,
            rationale: "Close and rerun from an exact evidence selection.".into(),
        })
        .expect("close unsupported linked retry path");
    assert_eq!(closed.disposition, AgentRunRecoveryDisposition::CloseTask);
}

#[tokio::test]
async fn confirmed_addendum_precedence_remains_attached_to_conflicting_records() {
    let harness = RuntimeHarness::new("record-extraction");
    let mut sources = harness
        .parsed_pdf_evidence_package(
            "original-with-addendum",
            &[
                ("original", b"TENDER_RECORD_GOLDEN"),
                ("addendum", b"TENDER_RECORD_GOLDEN"),
            ],
        )
        .await
        .into_iter();
    let prior = sources.next().expect("original Evidence");
    let addendum = sources.next().expect("addendum Evidence");
    harness
        .host
        .confirm_source_relationship(ConfirmSourceRelationshipCommand {
            tender_id: harness.tender_id.clone(),
            prior_artifact_id: prior.artifact_id.clone(),
            prior_version: prior.version,
            replacement_artifact_id: addendum.artifact_id.clone(),
            replacement_version: addendum.version,
            relationship_kind: SourceRelationshipKind::Addendum,
        })
        .expect("confirm addendum relationship");
    let assessment = harness
        .host
        .inspect_change_assessments(InspectChangeAssessmentsCommand {
            tender_id: harness.tender_id.clone(),
            before_sequence: None,
            limit: 4,
        })
        .expect("inspect pre-record addendum assessment")
        .active
        .expect("pending pre-record addendum assessment");
    assert!(assessment.impacts.is_empty());
    let assessment = harness
        .host
        .decide_change_assessment(DecideChangeAssessmentCommand {
            tender_id: harness.tender_id.clone(),
            assessment_id: assessment.assessment_id,
            assessment_manifest_sha256: assessment.manifest_sha256,
            classification: ChangeAssessmentClassification::Material,
            rationale: "The addendum is authoritative source precedence for future extraction."
                .into(),
        })
        .expect("record exact source precedence without invented rework");
    assert_eq!(assessment.status, ChangeAssessmentStatus::Resolved);
    let mut evidence = prior.references;
    evidence.extend(addendum.references);

    let extraction = harness
        .host
        .run_tender_record_extraction(RunTenderRecordExtractionCommand {
            tender_id: harness.tender_id.clone(),
            evidence,
            authorities: Vec::new(),
        })
        .await
        .expect("extract related-source records");
    let records = records_for_run(&harness.host, &harness.tender_id, &extraction.run.run_id);
    let deadline = records
        .iter()
        .find(|record| record.kind == TenderRecordKind::Deadline)
        .expect("deadline record");

    assert!(deadline.source_relationships.iter().any(|relationship| {
        relationship.relationship_kind == SourceRelationshipKind::Addendum
            && relationship.prior_artifact_id == prior.artifact_id
            && relationship.replacement_artifact_id == addendum.artifact_id
    }));
    assert!(deadline
        .contradictions
        .iter()
        .any(|contradiction| contradiction.evidence.len() >= 2));
    assert_eq!(deadline.verification_status, VerificationStatus::Proposed);
    harness
        .host
        .close_tender(&harness.tender_id)
        .expect("close source-precedence Tender");
    let integrity = harness
        .host
        .inspect_tender_integrity(&harness.tender_id)
        .expect("cold-verify source-precedence assessment");
    assert_eq!(
        integrity.state,
        TenderIntegrityState::Ready,
        "{integrity:#?}"
    );
}

#[tokio::test]
async fn later_addenda_make_affected_verified_records_visibly_stale() {
    let harness = RuntimeHarness::new("manager-intake");
    let prior = harness
        .parsed_pdf_evidence("stale-original", b"TENDER_RECORD_GOLDEN")
        .await;
    fs::write(
        harness.codex.with_extension("manager-output-release"),
        b"release",
    )
    .expect("release initial Manager outcome");
    harness
        .host
        .run_manager_intake_for_verification(&harness.tender_id)
        .await
        .expect("complete initial Manager Intake before later addendum");
    let records = inspect_all_records(&harness.host, &harness.tender_id);
    let requirement = records
        .iter()
        .find(|record| {
            record.kind == TenderRecordKind::Requirement
                && record.verification_status == VerificationStatus::Verified
        })
        .expect("verified affected requirement");
    let addendum = harness
        .parsed_pdf_evidence("stale-addendum", b"TENDER_RECORD_GOLDEN")
        .await;
    harness
        .host
        .confirm_source_relationship(ConfirmSourceRelationshipCommand {
            tender_id: harness.tender_id.clone(),
            prior_artifact_id: prior.artifact_id,
            prior_version: prior.version,
            replacement_artifact_id: addendum.artifact_id,
            replacement_version: addendum.version,
            relationship_kind: SourceRelationshipKind::Addendum,
        })
        .expect("confirm later addendum");

    let preserved = inspect_all_records(&harness.host, &harness.tender_id)
        .into_iter()
        .find(|record| record.record_id == requirement.record_id)
        .expect("affected record while assessment is pending");
    assert_eq!(preserved.verification_status, VerificationStatus::Verified);
    let assessment = harness
        .host
        .inspect_change_assessments(InspectChangeAssessmentsCommand {
            tender_id: harness.tender_id.clone(),
            before_sequence: None,
            limit: 4,
        })
        .expect("inspect later addendum assessment")
        .active
        .expect("pending later addendum assessment");
    harness
        .host
        .decide_change_assessment(DecideChangeAssessmentCommand {
            tender_id: harness.tender_id.clone(),
            assessment_id: assessment.assessment_id,
            assessment_manifest_sha256: assessment.manifest_sha256,
            classification: ChangeAssessmentClassification::Material,
            rationale: "The new addendum changes evidence supporting the verified requirement."
                .into(),
        })
        .expect("classify later addendum as material");
    assert_eq!(
        harness
            .host
            .open_tender(&harness.tender_id)
            .expect("inspect targeted Intake recovery lifecycle")
            .lifecycle_phase,
        TenderLifecyclePhase::Intake
    );

    let stale = inspect_all_records(&harness.host, &harness.tender_id)
        .into_iter()
        .find(|record| record.record_id == requirement.record_id)
        .expect("affected record after addendum");
    assert_eq!(stale.verification_status, VerificationStatus::Stale);
    assert_eq!(stale.trust_class, TenderRecordTrustClass::PriorDecision);
}

#[tokio::test]
async fn independent_review_is_attributable_version_bound_and_cannot_edit_the_proposal() {
    let harness = RuntimeHarness::new("record-extraction");
    let evidence = harness
        .parsed_pdf_evidence("review", b"TENDER_RECORD_GOLDEN")
        .await;
    let evidence_references = evidence.references.clone();
    let extraction = harness
        .host
        .run_tender_record_extraction(RunTenderRecordExtractionCommand {
            tender_id: harness.tender_id.clone(),
            evidence: evidence.references,
            authorities: Vec::new(),
        })
        .await
        .expect("extract review target");
    let records = records_for_run(&harness.host, &harness.tender_id, &extraction.run.run_id);
    let proposed = records
        .iter()
        .find(|record| record.kind == TenderRecordKind::Requirement)
        .expect("evidence-backed requirement")
        .clone();
    harness.set_agent_scenario("record-review");

    let reviewed = harness
        .host
        .run_tender_record_review(RunTenderRecordReviewCommand {
            tender_id: harness.tender_id.clone(),
            record_id: proposed.record_id.clone(),
            version: proposed.version,
        })
        .await
        .expect("independent exact-version review");

    assert_eq!(reviewed.run.state, AgentRunState::Completed);
    assert_eq!(reviewed.run.profile.identity, "Independent Reviewer");
    assert_ne!(
        reviewed.record.author_profile_id,
        reviewed.run.profile.profile_id
    );
    assert_eq!(reviewed.record.title, proposed.title);
    assert_eq!(reviewed.record.fields, proposed.fields);
    assert_eq!(
        reviewed.record.verification_status,
        VerificationStatus::Verified
    );
    assert_eq!(
        reviewed.record.trust_class,
        TenderRecordTrustClass::Verified
    );
    assert!(reviewed.record.reviews.iter().any(|review| {
        review.outcome == TenderRecordReviewOutcome::Verified
            && review.reviewer_run_id.as_deref() == Some(reviewed.run.run_id.as_str())
    }));
    assert!(harness
        .host
        .run_tender_record_review(RunTenderRecordReviewCommand {
            tender_id: harness.tender_id.clone(),
            record_id: proposed.record_id.clone(),
            version: proposed.version,
        })
        .await
        .is_err());

    harness.set_agent_scenario("record-extraction");
    harness
        .host
        .run_tender_record_extraction(RunTenderRecordExtractionCommand {
            tender_id: harness.tender_id.clone(),
            evidence: evidence_references,
            authorities: Vec::new(),
        })
        .await
        .expect("propose a new exact version");
    let history = inspect_all_records(&harness.host, &harness.tender_id);
    let current = history
        .iter()
        .filter(|record| record.record_id == proposed.record_id)
        .max_by_key(|record| record.version)
        .expect("current requirement version");
    assert_eq!(current.version, proposed.version + 1);
    assert_eq!(current.verification_status, VerificationStatus::Proposed);
    assert!(current.reviews.is_empty());
    assert!(history.iter().any(|record| {
        record.record_id == proposed.record_id
            && record.version == proposed.version
            && record.verification_status == VerificationStatus::Superseded
            && record.trust_class == TenderRecordTrustClass::PriorDecision
            && !record.reviews.is_empty()
    }));
    assert!(history.iter().any(|record| {
        record.kind == TenderRecordKind::Assumption
            && record.version == 1
            && record.verification_status == VerificationStatus::Superseded
            && record.trust_class == TenderRecordTrustClass::UnresolvedGap
            && record.reviews.is_empty()
    }));
}

#[tokio::test]
async fn engineer_decision_during_review_terminalizes_the_stale_review_run() {
    let harness = RuntimeHarness::new("record-extraction");
    let evidence = harness
        .parsed_pdf_evidence("review-race", b"TENDER_RECORD_GOLDEN")
        .await;
    let extraction = harness
        .host
        .run_tender_record_extraction(RunTenderRecordExtractionCommand {
            tender_id: harness.tender_id.clone(),
            evidence: evidence.references,
            authorities: Vec::new(),
        })
        .await
        .expect("extract review race target");
    let proposed = records_for_run(&harness.host, &harness.tender_id, &extraction.run.run_id)
        .into_iter()
        .find(|record| record.kind == TenderRecordKind::Requirement)
        .expect("review race requirement");
    harness.set_agent_scenario("record-review-delayed");
    let host = harness.host.clone();
    let tender_id = harness.tender_id.clone();
    let record_id = proposed.record_id.clone();
    let version = proposed.version;
    let review_task = tokio::spawn(async move {
        host.run_tender_record_review(RunTenderRecordReviewCommand {
            tender_id,
            record_id,
            version,
        })
        .await
    });
    let waiting = harness.codex.with_extension("record-review-waiting");
    for _ in 0..1_000 {
        if waiting.is_file() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    assert!(
        waiting.is_file(),
        "provider did not reach delayed review boundary"
    );
    harness
        .host
        .decide_tender_record(DecideTenderRecordCommand {
            tender_id: harness.tender_id.clone(),
            record_id: proposed.record_id.clone(),
            version: proposed.version,
            decision: TenderRecordEngineerDecisionKind::Verify,
            rationale: "Engineer decision landed before the independent review output.".into(),
        })
        .expect("record exact Engineer decision during provider review");
    fs::write(
        harness.codex.with_extension("record-review-release"),
        b"release",
    )
    .expect("release delayed review output");

    let reviewed = review_task
        .await
        .expect("join delayed review")
        .expect("terminalize stale review output");
    assert_eq!(reviewed.run.state, AgentRunState::Failed);
    assert_eq!(
        reviewed.run.failure.map(|failure| failure.category),
        Some(ProviderFailureCategory::OutputInvalid)
    );
    assert_eq!(
        reviewed.record.trust_class,
        TenderRecordTrustClass::EngineerVerified
    );
    assert_eq!(reviewed.record.reviews.len(), 1);
    assert_eq!(reviewed.record.reviews[0].reviewer_kind, "engineer_user");
    assert!(harness
        .host
        .inspect_agent_runs(&harness.tender_id)
        .expect("inspect terminal review race run")
        .iter()
        .all(|run| run.state != AgentRunState::Running));
}

#[tokio::test]
async fn missing_provenance_blocks_independent_verification_but_engineer_can_approve_an_assumption()
{
    let harness = RuntimeHarness::new("record-extraction");
    let evidence = harness
        .parsed_pdf_evidence("assumption", b"TENDER_RECORD_GOLDEN")
        .await;
    let extraction = harness
        .host
        .run_tender_record_extraction(RunTenderRecordExtractionCommand {
            tender_id: harness.tender_id.clone(),
            evidence: evidence.references,
            authorities: Vec::new(),
        })
        .await
        .expect("extract explicit gap");
    let records = records_for_run(&harness.host, &harness.tender_id, &extraction.run.run_id);
    let assumption = records
        .iter()
        .find(|record| record.kind == TenderRecordKind::Assumption)
        .expect("explicit assumption")
        .clone();
    harness.set_agent_scenario("record-review");

    let blocked = harness
        .host
        .run_tender_record_review(RunTenderRecordReviewCommand {
            tender_id: harness.tender_id.clone(),
            record_id: assumption.record_id.clone(),
            version: assumption.version,
        })
        .await
        .expect("record invalid verification as failed review run");
    assert_eq!(blocked.run.state, AgentRunState::Failed);
    assert_eq!(
        blocked.record.verification_status,
        VerificationStatus::Proposed
    );
    assert!(blocked.record.reviews.is_empty());
    assert!(harness
        .host
        .decide_tender_record(DecideTenderRecordCommand {
            tender_id: harness.tender_id.clone(),
            record_id: assumption.record_id.clone(),
            version: assumption.version,
            decision: TenderRecordEngineerDecisionKind::Verify,
            rationale: "Attempt to verify a record without exact provenance.".into(),
        })
        .is_err());

    let approved =
        harness
            .host
            .decide_tender_record(DecideTenderRecordCommand {
                tender_id: harness.tender_id.clone(),
                record_id: assumption.record_id,
                version: assumption.version,
                decision: TenderRecordEngineerDecisionKind::ApproveAssumption,
                rationale:
                    "Approved as the controlled pre-bid basis pending the Tender Query response."
                        .into(),
            })
            .expect("Engineer approval of explicit assumption");
    assert_eq!(
        approved.record.verification_status,
        VerificationStatus::Verified
    );
    assert_eq!(
        approved.record.trust_class,
        TenderRecordTrustClass::ApprovedAssumption
    );
    assert_eq!(approved.review.decided_by, "engineer_user");
    assert!(harness
        .host
        .run_tender_record_review(RunTenderRecordReviewCommand {
            tender_id: harness.tender_id.clone(),
            record_id: approved.record.record_id,
            version: approved.record.version,
        })
        .await
        .is_err());
}

#[tokio::test]
async fn exact_record_and_review_ledgers_survive_cold_open_integrity_validation() {
    let harness = RuntimeHarness::new("record-extraction");
    let evidence = harness
        .parsed_pdf_evidence("cold-open", b"TENDER_RECORD_GOLDEN")
        .await;
    let extraction = harness
        .host
        .run_tender_record_extraction(RunTenderRecordExtractionCommand {
            tender_id: harness.tender_id.clone(),
            evidence: evidence.references,
            authorities: Vec::new(),
        })
        .await
        .expect("extract cold-open target");
    let records = records_for_run(&harness.host, &harness.tender_id, &extraction.run.run_id);
    let requirement = records
        .iter()
        .find(|record| record.kind == TenderRecordKind::Requirement)
        .expect("requirement target");
    harness.set_agent_scenario("record-review");
    harness
        .host
        .run_tender_record_review(RunTenderRecordReviewCommand {
            tender_id: harness.tender_id.clone(),
            record_id: requirement.record_id.clone(),
            version: requirement.version,
        })
        .await
        .expect("review cold-open target");

    let cold_host =
        QuantixHost::with_setup_platform(&harness.application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&cold_host).state, SetupState::Ready);
    assert_eq!(
        cold_host
            .inspect_tender_integrity(&harness.tender_id)
            .expect("cold-open integrity")
            .state,
        TenderIntegrityState::Ready
    );
    let reopened = inspect_all_records(&cold_host, &harness.tender_id);
    assert!(reopened.iter().any(|record| {
        record.record_id == requirement.record_id
            && record.verification_status == VerificationStatus::Verified
    }));
}

fn install_codex_fixture(resources: &Path, scenario: &str) -> std::path::PathBuf {
    let runtime_bin = resources.join("runtime").join("bin");
    fs::create_dir_all(&runtime_bin).expect("fake runtime bin");
    let codex = runtime_bin.join(executable_name("codex"));
    fs::copy(
        Path::new(env!("CARGO_BIN_EXE_quantix-runtime-fixture")),
        &codex,
    )
    .expect("copy fake app-server");
    fs::write(codex.with_extension("agent-scenario"), scenario)
        .expect("write fake app-server scenario");
    codex
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
