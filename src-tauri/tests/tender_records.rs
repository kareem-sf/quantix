use std::{collections::HashSet, fs, io, path::Path, sync::Arc};

use quantix_lib::{
    ensure_quantix_setup, AgentRunRecoveryDisposition, AgentRunState, BootstrapAuthority,
    BootstrapRole, ChangeAssessmentClassification, ChangeAssessmentImpactKind,
    ChangeAssessmentStatus, ConfirmSourceRelationshipCommand, CreateBidDecisionPackageCommand,
    CreateTenderCommand, CreateTenderEngineerEntryCommand, DecideChangeAssessmentCommand,
    DecideTenderRecordCommand, ImportTenderPackageCommand, InspectChangeAssessmentsCommand,
    OutputValidationIssue, ParseSourceArtifactCommand, ProviderEventKind, ProviderFailureCategory,
    QuantixHost, ResolveIndeterminateAgentRunCommand, ReviseTenderCommand,
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

fn fixture_record_extraction_turn_count(codex: &Path) -> u32 {
    fs::read_to_string(codex.with_extension("record-extraction-turn-count"))
        .expect("read fixture extraction turn count")
        .trim()
        .parse()
        .expect("parse fixture extraction turn count")
}

async fn wait_until_agent_grant_expires(application_home: &Path, tender_id: &str, run_id: &str) {
    let database = application_home
        .join("tenders")
        .join(tender_id)
        .join("tender.sqlite");
    let connection = rusqlite::Connection::open(database).expect("open Tender Store database");
    let permission_grant_json: String = connection
        .query_row(
            "SELECT permission_grant_json FROM agent_runs WHERE run_id = ?1",
            [run_id],
            |row| row.get(0),
        )
        .expect("load Agent Run Permission Grant");
    drop(connection);
    let permission_grant: serde_json::Value =
        serde_json::from_str(&permission_grant_json).expect("parse Agent Run Permission Grant");
    let issued_at = permission_grant
        .get("issued_at")
        .and_then(serde_json::Value::as_str)
        .expect("Permission Grant issue time")
        .parse::<jiff::Timestamp>()
        .expect("parse Permission Grant issue time");
    let duration_seconds = permission_grant
        .pointer("/resource_budget/duration_seconds")
        .and_then(serde_json::Value::as_u64)
        .expect("Permission Grant duration");
    let elapsed = std::time::Duration::try_from(issued_at.duration_until(jiff::Timestamp::now()))
        .unwrap_or_default();
    let until_expired = std::time::Duration::from_secs(duration_seconds).saturating_sub(elapsed);
    tokio::time::sleep(until_expired + std::time::Duration::from_millis(250)).await;
}

fn set_fixture_provider_unavailable(application_home: &Path) {
    let database = application_home.join("installation.sqlite");
    let connection = rusqlite::Connection::open(database).expect("open installation database");
    let (connection_id, connection_json): (String, String) = connection
        .query_row(
            "SELECT connection_id, connection_json FROM provider_connections LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("load fixture Provider connection");
    let mut provider: serde_json::Value =
        serde_json::from_str(&connection_json).expect("parse fixture Provider connection");
    provider["status"] = serde_json::Value::String("temporarily_unavailable".into());
    provider["status_summary"] =
        serde_json::Value::String("Fixture Provider temporarily unavailable.".into());
    connection
        .execute(
            "UPDATE provider_connections
             SET connection_json = ?2, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE connection_id = ?1",
            rusqlite::params![
                connection_id,
                serde_json_canonicalizer::to_string(&provider)
                    .expect("canonical fixture Provider connection")
            ],
        )
        .expect("make fixture Provider unavailable");
}

fn persist_repair_thread_checkpoint(application_home: &Path, tender_id: &str, repair_run_id: &str) {
    let database = application_home
        .join("tenders")
        .join(tender_id)
        .join("tender.sqlite");
    let connection = rusqlite::Connection::open(database).expect("open Tender Store database");
    let (thread_ref, permission_grant_json): (String, String) = connection
        .query_row(
            "SELECT provider_threads.thread_ref, agent_runs.permission_grant_json
             FROM provider_threads, agent_runs
             WHERE provider_threads.status = 'active' AND agent_runs.run_id = ?1",
            [repair_run_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("load persisted repair thread checkpoint inputs");
    let permission_grant: serde_json::Value = serde_json::from_str(&permission_grant_json)
        .expect("parse persisted repair Permission Grant");
    let exposure_json = serde_json_canonicalizer::to_string(
        permission_grant
            .get("thread_exposure")
            .expect("Permission Grant thread exposure"),
    )
    .expect("canonical repair thread exposure");
    connection
        .execute(
            "UPDATE agent_runs SET provider_thread_ref = ?2 WHERE run_id = ?1",
            rusqlite::params![repair_run_id, thread_ref],
        )
        .expect("persist repair thread ref before simulated process loss");
    connection
        .execute(
            "INSERT INTO provider_thread_exposures (
               thread_ref, run_id, exposure_json, created_at
             ) VALUES (?1, ?2, ?3, '2026-08-23T00:00:00Z')",
            rusqlite::params![thread_ref, repair_run_id, exposure_json],
        )
        .expect("persist repair thread exposure before simulated process loss");
    connection
        .execute(
            "INSERT INTO provider_events (
               run_id, sequence, kind, summary, correlation_id,
               request_fingerprint, denial_reason, opaque_reference, created_at
             ) VALUES (
               ?1, 2, 'thread_resumed', 'Provider Thread resumed', NULL,
               NULL, NULL, ?2, '2026-08-23T00:00:00Z'
             )",
            rusqlite::params![repair_run_id, thread_ref],
        )
        .expect("persist repair thread event before simulated process loss");
}

fn extraction_run_inspections(
    host: &QuantixHost,
    tender_id: &str,
    extraction_runs: &[(String, Option<String>, String)],
) -> Vec<quantix_lib::AgentRunInspection> {
    let all_runs = host
        .inspect_agent_runs(tender_id)
        .expect("inspect extraction Agent Runs");
    extraction_runs
        .iter()
        .map(|(run_id, _, _)| {
            all_runs
                .iter()
                .find(|run| run.run_id == *run_id)
                .cloned()
                .expect("inspect exact extraction Agent Run")
        })
        .collect()
}

fn assert_independent_provider_turn_ownership(inspections: &[quantix_lib::AgentRunInspection]) {
    assert_eq!(inspections.len(), 2);
    let source_turn = inspections[0]
        .provider_turn_ref
        .as_deref()
        .expect("source Provider Turn ref");
    let repair_turn = inspections[1]
        .provider_turn_ref
        .as_deref()
        .expect("repair Provider Turn ref");
    assert_ne!(
        source_turn, repair_turn,
        "each Agent Run owns one Provider Turn"
    );
    for run in inspections {
        assert_eq!(run.usage.total_tokens, Some(155), "{run:#?}");
        assert!(
            run.events
                .iter()
                .any(|event| event.kind == ProviderEventKind::TurnStarted),
            "{run:#?}"
        );
        assert_eq!(
            run.events.last().map(|event| event.kind),
            Some(ProviderEventKind::Terminal),
            "{run:#?}"
        );
    }
}

fn rejected_output(
    connection: &rusqlite::Connection,
    run_id: &str,
) -> (serde_json::Value, String, serde_json::Value) {
    let (payload_json, payload_sha256, validation_issues_json): (String, String, String) =
        connection
            .query_row(
                "SELECT payload_json, payload_sha256, validation_issues_json
                 FROM agent_run_rejected_outputs WHERE run_id = ?1",
                [run_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("load rejected Agent Run output");
    (
        serde_json::from_str(&payload_json).expect("parse canonical rejected proposal"),
        payload_sha256,
        serde_json::from_str(&validation_issues_json).expect("parse canonical validation issues"),
    )
}

fn reopen_runtime_host(application_home: &Path, resources: &Path) -> QuantixHost {
    let host = QuantixHost::with_setup_platform_and_runtime(
        application_home,
        Arc::new(ReadySetupPlatform),
        RuntimeLayout::bundled(resources.to_path_buf()),
    );
    host.accept_runtime_fixture();
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    host.approve_runtime_fixture_ai_selection()
        .expect("approve fixture AI selection after restart");
    host
}

fn provider_handle_strings(value: &serde_json::Value, found: &mut Vec<String>) {
    match value {
        serde_json::Value::String(value)
            if value.len() == 5
                && matches!(value.as_bytes().first(), Some(b'e' | b'a'))
                && value.as_bytes()[1..].iter().all(u8::is_ascii_digit) =>
        {
            found.push(value.clone());
        }
        serde_json::Value::Array(values) => {
            for value in values {
                provider_handle_strings(value, found);
            }
        }
        serde_json::Value::Object(values) => {
            for value in values.values() {
                provider_handle_strings(value, found);
            }
        }
        _ => {}
    }
}

fn canonical_evidence_references(
    value: &serde_json::Value,
    found: &mut HashSet<(String, u32, u32)>,
) {
    match value {
        serde_json::Value::Array(values) => {
            for value in values {
                canonical_evidence_references(value, found);
            }
        }
        serde_json::Value::Object(values) => {
            if let (Some(artifact_id), Some(version), Some(ordinal)) = (
                values
                    .get("artifact_id")
                    .and_then(serde_json::Value::as_str),
                values.get("version").and_then(serde_json::Value::as_u64),
                values.get("ordinal").and_then(serde_json::Value::as_u64),
            ) {
                found.insert((
                    artifact_id.to_owned(),
                    u32::try_from(version).expect("canonical Evidence version"),
                    u32::try_from(ordinal).expect("canonical Evidence ordinal"),
                ));
            }
            for value in values.values() {
                canonical_evidence_references(value, found);
            }
        }
        _ => {}
    }
}

#[tokio::test]
async fn byte_batch_plan_is_deterministic_at_boundaries() {
    let harness = RuntimeHarness::new("manager-intake");
    let parsed = harness
        .parsed_pdf_evidence_package(
            "byte-plan-boundary",
            &[
                ("byte-plan-a", b"BYTE_PLAN_A"),
                ("byte-plan-b", b"BYTE_PLAN_B"),
                ("byte-plan-c", b"BYTE_PLAN_C"),
            ],
        )
        .await;
    let mut expected = parsed
        .iter()
        .flat_map(|document| document.references.clone())
        .collect::<Vec<_>>();
    expected.sort_by(|left, right| {
        (&left.artifact_id, left.version, left.ordinal).cmp(&(
            &right.artifact_id,
            right.version,
            right.ordinal,
        ))
    });
    let exact_fit = harness
        .host
        .preview_manager_intake_byte_plan_for_verification(&harness.tender_id, u64::MAX)
        .expect("preview one exact-fit batch");
    assert_eq!(exact_fit.len(), 1, "the unbounded preview should fit once");
    let exact_budget = exact_fit[0].3;

    let plan = harness
        .host
        .persist_manager_intake_byte_plan_for_verification(&harness.tender_id, exact_budget)
        .expect("persist exact-fit plan");
    assert_eq!(plan.len(), 1, "a request exactly at the budget must fit");
    assert_eq!(plan[0].0, expected);
    assert_eq!(plan[0].3, exact_budget);

    let overflow = RuntimeHarness::new("manager-intake");
    let overflow_parsed = overflow
        .parsed_pdf_evidence_package(
            "byte-plan-boundary",
            &[
                ("byte-plan-a", b"BYTE_PLAN_A"),
                ("byte-plan-b", b"BYTE_PLAN_B"),
                ("byte-plan-c", b"BYTE_PLAN_C"),
            ],
        )
        .await;
    let mut overflow_expected = overflow_parsed
        .iter()
        .flat_map(|document| document.references.clone())
        .collect::<Vec<_>>();
    overflow_expected.sort_by(|left, right| {
        (&left.artifact_id, left.version, left.ordinal).cmp(&(
            &right.artifact_id,
            right.version,
            right.ordinal,
        ))
    });
    let overflow_plan = overflow
        .host
        .persist_manager_intake_byte_plan_for_verification(&overflow.tender_id, exact_budget - 1)
        .expect("split one-byte overflow");
    assert!(overflow_plan.len() > 1, "one byte over must split");
    let flattened = overflow_plan
        .iter()
        .flat_map(|batch| batch.0.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        flattened, overflow_expected,
        "no Evidence may be omitted or reordered"
    );
    assert_eq!(
        flattened.iter().collect::<HashSet<_>>().len(),
        flattened.len()
    );
    assert!(overflow_plan
        .iter()
        .all(|batch| batch.3 <= exact_budget - 1));
    assert_eq!(
        overflow_plan
            .iter()
            .map(|batch| batch.2.clone())
            .collect::<HashSet<_>>()
            .len(),
        overflow_plan.len(),
        "batch fingerprints must be stable and distinct",
    );

    let oversized = RuntimeHarness::new("manager-intake");
    oversized
        .parsed_pdf_evidence("byte-plan-oversized", b"BYTE_PLAN_OVERSIZED")
        .await;
    let error = oversized
        .host
        .persist_manager_intake_byte_plan_for_verification(&oversized.tender_id, 1)
        .expect_err("one individually oversized Evidence item must be typed");
    assert_eq!(error.code, TenderErrorCode::RequestBudgetExceeded);
}

#[tokio::test]
async fn byte_batch_estimate_matches_the_production_request_body() {
    let harness = RuntimeHarness::new("manager-intake");
    harness
        .parsed_pdf_evidence("byte-plan-request-parity", b"BYTE_PLAN_REQUEST_PARITY")
        .await;
    let plan = harness
        .host
        .preview_manager_intake_byte_plan_for_verification(&harness.tender_id, u64::MAX)
        .expect("preview byte-budgeted request");
    assert_eq!(plan.len(), 1);
    assert!(
        plan[0].4 > 0,
        "the production request body must be non-empty"
    );
    assert_eq!(
        plan[0].3 - plan[0].4,
        72 * 1024,
        "only the documented fixed transport overhead and output headroom may differ from the exact serialized request body",
    );
}

#[tokio::test]
async fn byte_batch_plan_rejects_a_changed_tender_context_without_replanning() {
    let harness = RuntimeHarness::new("manager-intake");
    harness
        .parsed_pdf_evidence("byte-plan-context-drift", b"TENDER_RECORD_GOLDEN")
        .await;
    let planned = harness
        .host
        .persist_manager_intake_byte_plan_for_verification(&harness.tender_id, u64::MAX)
        .expect("persist immutable request context");
    let database = harness
        .application_home
        .join("tenders")
        .join(&harness.tender_id)
        .join("tender.sqlite");
    let original_contexts = rusqlite::Connection::open(&database)
        .expect("open planned Tender Store")
        .prepare(
            "SELECT canonical_inputs_json FROM manager_intake_extraction_plan_batches
             ORDER BY ordinal",
        )
        .expect("prepare immutable context query")
        .query_map([], |row| row.get::<_, String>(0))
        .expect("query immutable contexts")
        .collect::<rusqlite::Result<Vec<_>>>()
        .expect("collect immutable contexts");
    harness
        .host
        .revise_tender(ReviseTenderCommand {
            tender_id: harness.tender_id.clone(),
            name: "Tender revision changed after intake planning".into(),
        })
        .expect("revise Tender after request planning");
    let run_count_before =
        tender_record_extraction_runs(&harness.application_home, &harness.tender_id).len();
    let result = harness
        .host
        .run_manager_intake_for_verification(&harness.tender_id)
        .await;
    assert!(result.is_err(), "changed context must fail before dispatch");
    assert_eq!(
        tender_record_extraction_runs(&harness.application_home, &harness.tender_id).len(),
        run_count_before,
        "a changed request must not create an extraction run",
    );
    let connection = rusqlite::Connection::open(database).expect("reopen planned Tender Store");
    let persisted_contexts = connection
        .prepare(
            "SELECT canonical_inputs_json FROM manager_intake_extraction_plan_batches
             ORDER BY ordinal",
        )
        .expect("prepare persisted context query")
        .query_map([], |row| row.get::<_, String>(0))
        .expect("query persisted contexts")
        .collect::<rusqlite::Result<Vec<_>>>()
        .expect("collect persisted contexts");
    assert_eq!(persisted_contexts, original_contexts);
    assert_eq!(persisted_contexts.len(), planned.len());
}

#[tokio::test]
async fn byte_batch_plan_is_persisted_and_resumed() {
    let harness = RuntimeHarness::new("manager-intake");
    harness
        .parsed_pdf_evidence_package(
            "byte-plan-restart",
            &[
                ("byte-plan-restart-a", b"TENDER_RECORD_GOLDEN"),
                ("byte-plan-restart-b", b"TENDER_RECORD_GOLDEN"),
                ("byte-plan-restart-c", b"TENDER_RECORD_GOLDEN"),
            ],
        )
        .await;
    let preview = harness
        .host
        .preview_manager_intake_byte_plan_for_verification(&harness.tender_id, u64::MAX)
        .expect("preview persisted plan");
    let budget = preview[0].3 - 1;
    let planned = harness
        .host
        .persist_manager_intake_byte_plan_for_verification(&harness.tender_id, budget)
        .expect("persist byte plan");
    assert!(planned.len() > 1);

    let database = harness
        .application_home
        .join("tenders")
        .join(&harness.tender_id)
        .join("tender.sqlite");
    let connection = rusqlite::Connection::open(&database).expect("open planned Tender Store");
    let immutable_update = connection.execute(
        "UPDATE manager_intake_extraction_plan_batches SET estimated_request_bytes = estimated_request_bytes + 1",
        [],
    );
    assert!(immutable_update.is_err());
    let immutable_delete =
        connection.execute("DELETE FROM manager_intake_extraction_plan_batches", []);
    assert!(immutable_delete.is_err());
    drop(connection);

    let resources = harness._root.path().join("resources");
    let reopened = reopen_runtime_host(&harness.application_home, &resources);
    let resumed = reopened
        .persist_manager_intake_byte_plan_for_verification(&harness.tender_id, u64::MAX)
        .expect("resume immutable persisted plan without rebuilding");
    assert_eq!(
        resumed, planned,
        "restart must load the original immutable plan"
    );
    fs::write(
        harness.codex.with_extension("manager-output-release"),
        b"release",
    )
    .expect("release Manager outcome after plan restart");
    let resumed_result = reopened
        .run_manager_intake_for_verification(&harness.tender_id)
        .await;
    assert!(
        resumed_result.is_err(),
        "the deterministic fixture stops after proving the first planned receipt"
    );
    let remaining = reopened
        .persist_manager_intake_byte_plan_for_verification(&harness.tender_id, 1)
        .expect("load immutable plan and subtract its completed receipt");
    assert_eq!(remaining.len(), planned.len() - 1);
    assert!(!remaining.iter().any(|batch| batch.2 == planned[0].2));
    let connection = rusqlite::Connection::open(database).expect("reopen completed byte plan");
    let (plan_count, completed_count, distinct_completed): (u32, u32, u32) = connection
        .query_row(
            "SELECT
               (SELECT COUNT(*) FROM manager_intake_extraction_plan_batches),
               (SELECT COUNT(*) FROM manager_intake_extraction_batches),
               (SELECT COUNT(DISTINCT batch_fingerprint) FROM manager_intake_extraction_batches)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("count persisted and completed byte batches");
    assert_eq!(plan_count as usize, planned.len());
    assert_eq!(completed_count, 1, "one completed batch is subtracted once");
    assert_eq!(
        distinct_completed, completed_count,
        "no batch may be duplicated"
    );
    let immutable_binding_update = connection.execute(
        "UPDATE manager_intake_extraction_plan_run_bindings
         SET request_context_sha256 = lower(request_context_sha256)",
        [],
    );
    assert!(immutable_binding_update.is_err());
    let immutable_binding_delete = connection.execute(
        "DELETE FROM manager_intake_extraction_plan_run_bindings",
        [],
    );
    assert!(immutable_binding_delete.is_err());
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
    assert_eq!(fixture_record_extraction_turn_count(&harness.codex), 2);
    let inspections = extraction_run_inspections(&harness.host, &harness.tender_id, &runs);
    assert_independent_provider_turn_ownership(&inspections);
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
    let repair_feedback: serde_json::Value =
        serde_json::from_str(&repair_feedback).expect("canonical repair feedback");
    assert_eq!(repair_feedback["rejected_run_id"], runs[0].0);
    let (source_rejected_payload, source_rejected_hash, source_rejected_issues) =
        rejected_output(&connection, &runs[0].0);
    assert_eq!(
        source_rejected_issues,
        serde_json::to_value(
            inspections[0]
                .failure
                .as_ref()
                .expect("source OutputInvalid failure")
                .validation_issues
                .clone()
        )
        .expect("serialize source validation issues")
    );
    assert_eq!(
        repair_feedback["rejected_payload_sha256"],
        source_rejected_hash
    );
    assert_eq!(repair_feedback["validation_issues"], source_rejected_issues);
    let materialized_feedback: serde_json::Value = serde_json::from_slice(
        &fs::read(harness.codex.with_extension("repair-feedback-observed"))
            .expect("read materialized repair feedback observed by fixture"),
    )
    .expect("parse materialized repair feedback");
    assert_eq!(
        materialized_feedback["rejected_proposal"],
        source_rejected_payload
    );
    assert_eq!(
        materialized_feedback["rejected_payload_sha256"],
        repair_feedback["rejected_payload_sha256"]
    );
    assert_eq!(
        materialized_feedback["validation_issues"],
        repair_feedback["validation_issues"]
    );
    assert!(inspections[0].task.repair_feedback.is_none());
    assert_eq!(
        inspections[1]
            .task
            .repair_feedback
            .as_ref()
            .expect("repair feedback on repair task")
            .rejected_payload_sha256,
        source_rejected_hash
    );
    let direct_child_count: u32 = connection
        .query_row(
            "SELECT COUNT(*) FROM agent_runs WHERE retry_of_run_id = ?1",
            [&runs[0].0],
            |row| row.get(0),
        )
        .expect("count direct repair children");
    assert_eq!(direct_child_count, 1);
    let retry_index_sql: String = connection
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'index' AND name = 'agent_runs_one_direct_retry'",
            [],
            |row| row.get(0),
        )
        .expect("read direct repair uniqueness index");
    assert!(
        retry_index_sql.contains("WHERE retry_of_run_id IS NOT NULL"),
        "{retry_index_sql}"
    );
    assert!(
        connection
            .execute(
                "INSERT INTO agent_runs (
                   run_id, task_id, profile_id, profile_version, retry_of_run_id,
                   permission_grant_json, status, started_at
                 )
                 SELECT ?1, task_id, profile_id, profile_version, retry_of_run_id,
                        permission_grant_json, 'running', started_at
                 FROM agent_runs WHERE run_id = ?2",
                rusqlite::params!["0".repeat(32), runs[1].0],
            )
            .is_err(),
        "the partial index must reject a second direct repair child"
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
    assert_eq!(fixture_record_extraction_turn_count(&harness.codex), 2);
    let inspections = extraction_run_inspections(&harness.host, &harness.tender_id, &runs);
    assert_independent_provider_turn_ownership(&inspections);
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
    for (index, run) in inspections.iter().enumerate() {
        let (_, rejected_hash, rejected_issues) = rejected_output(&connection, &run.run_id);
        assert_eq!(
            rejected_issues,
            serde_json::to_value(
                run.failure
                    .as_ref()
                    .expect("invalid repair attempt failure")
                    .validation_issues
                    .clone()
            )
            .expect("serialize invalid repair validation issues")
        );
        assert_eq!(rejected_hash.len(), 64);
        if index == 0 {
            assert!(run.task.repair_feedback.is_none());
        } else {
            let feedback = run
                .task
                .repair_feedback
                .as_ref()
                .expect("repair task feedback");
            let (_, source_hash, source_issues) = rejected_output(&connection, &runs[0].0);
            assert_eq!(feedback.rejected_run_id, runs[0].0);
            assert_eq!(feedback.rejected_payload_sha256, source_hash);
            assert_eq!(
                serde_json::to_value(&feedback.validation_issues)
                    .expect("serialize immutable repair issues"),
                source_issues
            );
        }
    }
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
async fn manager_intake_restart_recovers_the_failed_source_before_creating_a_repair() {
    let harness = RuntimeHarness::new("manager-intake-repair-invalid-then-valid");
    harness
        .parsed_pdf_evidence("repair-restart-before-child", b"TENDER_RECORD_GOLDEN")
        .await;
    fs::write(
        harness.codex.with_extension("manager-output-release"),
        b"release",
    )
    .expect("release Manager outcome");
    let database = harness
        .application_home
        .join("tenders")
        .join(&harness.tender_id)
        .join("tender.sqlite");
    let connection = rusqlite::Connection::open(&database).expect("open Tender Store database");
    connection
        .execute_batch(
            "CREATE TRIGGER fail_first_repair_prepare
             BEFORE INSERT ON agent_runs
             WHEN NEW.retry_of_run_id IS NOT NULL
             BEGIN
               SELECT RAISE(ABORT, 'simulate process loss before repair preparation');
             END;",
        )
        .expect("install repair preparation crash boundary");

    harness
        .host
        .run_manager_intake_for_verification(&harness.tender_id)
        .await
        .expect_err("simulated process loss stops before the repair child is committed");
    connection
        .execute_batch("DROP TRIGGER fail_first_repair_prepare;")
        .expect("remove repair preparation crash boundary");
    let runs_before_restart =
        tender_record_extraction_runs(&harness.application_home, &harness.tender_id);
    assert_eq!(runs_before_restart.len(), 1);
    assert_eq!(runs_before_restart[0].2, "failed");
    drop(connection);

    let RuntimeHarness {
        _root,
        application_home,
        codex,
        host,
        tender_id,
    } = harness;
    drop(host);
    let restarted = reopen_runtime_host(&application_home, &_root.path().join("resources"));
    restarted
        .run_manager_intake_for_verification(&tender_id)
        .await
        .expect("restart resumes the persisted semantic lineage");

    let runs = tender_record_extraction_runs(&application_home, &tender_id);
    assert_eq!(runs.len(), 2, "restart must not create a second source run");
    assert_eq!(runs[0].0, runs_before_restart[0].0);
    assert_eq!(runs[1].1.as_deref(), Some(runs[0].0.as_str()));
    assert_eq!(runs[1].2, "completed");
    assert_eq!(fixture_record_extraction_turn_count(&codex), 2);
    assert!(
        !records_for_run(&restarted, &tender_id, &runs[1].0).is_empty(),
        "the recovered repair publishes the batch"
    );
}

#[tokio::test]
async fn manager_intake_restart_resumes_the_same_persisted_repair_before_provider_dispatch() {
    let harness = RuntimeHarness::new("manager-intake-repair-invalid-then-valid");
    harness
        .parsed_pdf_evidence("repair-restart-running-child", b"TENDER_RECORD_GOLDEN")
        .await;
    fs::write(
        harness.codex.with_extension("repair-before-turn-pause"),
        b"pause",
    )
    .expect("pause persisted repair before Provider dispatch");
    fs::write(
        harness.codex.with_extension("manager-output-release"),
        b"release",
    )
    .expect("release Manager outcome");
    let host = harness.host.clone();
    let tender_id = harness.tender_id.clone();
    let intake =
        tokio::spawn(async move { host.run_manager_intake_for_verification(&tender_id).await });
    let waiting = harness.codex.with_extension("repair-before-turn-waiting");
    for _ in 0..1_000 {
        if waiting.is_file() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    assert!(
        waiting.is_file(),
        "repair did not reach the persisted pre-dispatch boundary"
    );
    let runs_before_restart =
        tender_record_extraction_runs(&harness.application_home, &harness.tender_id);
    assert_eq!(runs_before_restart.len(), 2);
    assert_eq!(runs_before_restart[0].2, "failed");
    assert_eq!(runs_before_restart[1].2, "running");
    assert_eq!(
        runs_before_restart[1].1.as_deref(),
        Some(runs_before_restart[0].0.as_str())
    );
    intake.abort();
    let _ = intake.await;

    let RuntimeHarness {
        _root,
        application_home,
        codex,
        host,
        tender_id,
    } = harness;
    drop(host);
    fs::remove_file(codex.with_extension("repair-before-turn-pause"))
        .expect("clear repair pause before restart");
    let restarted = reopen_runtime_host(&application_home, &_root.path().join("resources"));
    restarted
        .run_manager_intake_for_verification(&tender_id)
        .await
        .expect("restart resumes the safely restartable persisted repair");

    let runs = tender_record_extraction_runs(&application_home, &tender_id);
    assert_eq!(
        runs.len(),
        2,
        "restart must preserve the bounded attempt pair"
    );
    assert_eq!(runs[0].0, runs_before_restart[0].0);
    assert_eq!(runs[1].0, runs_before_restart[1].0);
    assert_eq!(runs[1].1, runs_before_restart[1].1);
    assert_eq!(runs[1].2, "completed");
    assert_eq!(fixture_record_extraction_turn_count(&codex), 2);
}

#[tokio::test]
async fn manager_intake_restart_blocks_a_repair_after_thread_checkpoint_without_a_turn() {
    let harness = RuntimeHarness::new("manager-intake-repair-invalid-then-valid");
    harness
        .parsed_pdf_evidence("repair-restart-thread-checkpoint", b"TENDER_RECORD_GOLDEN")
        .await;
    fs::write(
        harness.codex.with_extension("repair-before-turn-pause"),
        b"pause",
    )
    .expect("pause persisted repair before Provider dispatch");
    let host = harness.host.clone();
    let tender_id = harness.tender_id.clone();
    let intake =
        tokio::spawn(async move { host.run_manager_intake_for_verification(&tender_id).await });
    let waiting = harness.codex.with_extension("repair-before-turn-waiting");
    for _ in 0..1_000 {
        if waiting.is_file() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    assert!(waiting.is_file(), "repair did not reach pre-dispatch pause");
    let runs_before_restart =
        tender_record_extraction_runs(&harness.application_home, &harness.tender_id);
    let repair_run_id = runs_before_restart[1].0.clone();
    persist_repair_thread_checkpoint(
        &harness.application_home,
        &harness.tender_id,
        &repair_run_id,
    );
    intake.abort();
    let _ = intake.await;

    let RuntimeHarness {
        _root,
        application_home,
        codex,
        host,
        tender_id,
    } = harness;
    drop(host);
    fs::remove_file(codex.with_extension("repair-before-turn-pause"))
        .expect("clear pre-dispatch pause");
    let restarted = reopen_runtime_host(&application_home, &_root.path().join("resources"));
    let reopened_inspection = restarted
        .inspect_agent_runs(&tender_id)
        .expect("open Tender Store and reconcile interrupted repair");
    assert_eq!(
        reopened_inspection
            .iter()
            .find(|run| run.run_id == repair_run_id)
            .map(|run| run.state),
        Some(AgentRunState::Indeterminate)
    );
    let reopened_runs = tender_record_extraction_runs(&application_home, &tender_id);
    assert_eq!(reopened_runs.len(), 2);
    assert_eq!(reopened_runs[1].0, repair_run_id);
    assert_eq!(
        reopened_runs[1].2, "indeterminate",
        "a thread checkpoint makes Provider acceptance uncertain"
    );
    let error = restarted
        .run_manager_intake_for_verification(&tender_id)
        .await
        .expect_err("indeterminate repair blocks automatic replay");
    assert_eq!(error.code, TenderErrorCode::InvalidCommand);
    assert_eq!(
        tender_record_extraction_runs(&application_home, &tender_id),
        reopened_runs,
        "blocked recovery cannot execute the child or create another source"
    );
    assert_eq!(fixture_record_extraction_turn_count(&codex), 1);
}

#[tokio::test]
async fn manager_intake_same_host_retries_a_repair_that_expires_while_waiting_for_provider() {
    let harness = RuntimeHarness::new("manager-intake-repair-invalid-then-valid");
    harness
        .parsed_pdf_evidence("repair-same-host-expiry", b"TENDER_RECORD_GOLDEN")
        .await;
    fs::write(
        harness.codex.with_extension("repair-before-turn-pause"),
        b"pause",
    )
    .expect("pause persisted repair before Provider dispatch");
    fs::write(
        harness.codex.with_extension("manager-output-release"),
        b"release",
    )
    .expect("release Manager outcome");
    let host = harness.host.clone();
    let tender_id = harness.tender_id.clone();
    let intake =
        tokio::spawn(async move { host.run_manager_intake_for_verification(&tender_id).await });
    let waiting = harness.codex.with_extension("repair-before-turn-waiting");
    for _ in 0..1_000 {
        if waiting.is_file() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    assert!(waiting.is_file(), "repair did not reach pre-dispatch pause");
    let runs_before_restart =
        tender_record_extraction_runs(&harness.application_home, &harness.tender_id);
    assert_eq!(runs_before_restart.len(), 2);
    let source_run_id = runs_before_restart[0].0.clone();
    let repair_run_id = runs_before_restart[1].0.clone();
    intake.abort();
    let _ = intake.await;

    let RuntimeHarness {
        _root,
        application_home,
        codex,
        host,
        tender_id,
    } = harness;
    drop(host);
    fs::remove_file(codex.with_extension("repair-before-turn-pause"))
        .expect("clear repair pause before restart");
    let restarted = reopen_runtime_host(&application_home, &_root.path().join("resources"));
    let reopened = restarted
        .inspect_agent_runs(&tender_id)
        .expect("open Tender Store while repair is safely restartable");
    assert_eq!(
        reopened
            .iter()
            .find(|run| run.run_id == repair_run_id)
            .map(|run| run.state),
        Some(AgentRunState::Running)
    );

    set_fixture_provider_unavailable(&application_home);
    restarted
        .run_manager_intake_for_verification(&tender_id)
        .await
        .expect("Provider unavailability leaves intake waiting");
    let database = application_home
        .join("tenders")
        .join(&tender_id)
        .join("tender.sqlite");
    let connection = rusqlite::Connection::open(&database).expect("open Tender Store database");
    let stage: String = connection
        .query_row(
            "SELECT stage FROM manager_intake_runs ORDER BY intake_run_sequence DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .expect("load waiting Manager intake stage");
    assert_eq!(stage, "waiting_for_provider");
    drop(connection);

    wait_until_agent_grant_expires(&application_home, &tender_id, &repair_run_id).await;
    restarted
        .approve_runtime_fixture_ai_selection()
        .expect("restore fixture Provider readiness");
    restarted
        .run_manager_intake_for_verification(&tender_id)
        .await
        .expect("same Host terminalizes the expired repair and retries its transport");

    let runs = tender_record_extraction_runs(&application_home, &tender_id);
    assert_eq!(runs.len(), 3);
    assert_eq!(runs[0].0, source_run_id);
    assert_eq!(runs[1].0, repair_run_id);
    assert_eq!(runs[1].1.as_deref(), Some(source_run_id.as_str()));
    assert_eq!(runs[1].2, "failed");
    assert_eq!(runs[2].1.as_deref(), Some(repair_run_id.as_str()));
    assert_eq!(runs[2].2, "completed");
    let inspection = restarted
        .inspect_agent_runs(&tender_id)
        .expect("inspect same-Host repair recovery");
    let expired = inspection
        .iter()
        .find(|run| run.run_id == repair_run_id)
        .expect("inspect expired repair");
    assert_eq!(
        expired.failure.as_ref().map(|failure| failure.category),
        Some(ProviderFailureCategory::ProcessFailed)
    );
    assert_eq!(
        expired
            .events
            .iter()
            .filter(|event| event.kind == ProviderEventKind::Terminal)
            .count(),
        1
    );
    assert_eq!(fixture_record_extraction_turn_count(&codex), 2);
}

#[tokio::test]
async fn manager_intake_same_host_blocks_a_repair_after_a_thread_checkpoint() {
    let harness = RuntimeHarness::new("manager-intake-repair-invalid-then-valid");
    harness
        .parsed_pdf_evidence(
            "repair-same-host-thread-checkpoint",
            b"TENDER_RECORD_GOLDEN",
        )
        .await;
    fs::write(
        harness.codex.with_extension("repair-before-turn-pause"),
        b"pause",
    )
    .expect("pause persisted repair before Provider dispatch");
    let host = harness.host.clone();
    let tender_id = harness.tender_id.clone();
    let intake =
        tokio::spawn(async move { host.run_manager_intake_for_verification(&tender_id).await });
    let waiting = harness.codex.with_extension("repair-before-turn-waiting");
    for _ in 0..1_000 {
        if waiting.is_file() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    assert!(waiting.is_file(), "repair did not reach pre-dispatch pause");
    let runs_before_restart =
        tender_record_extraction_runs(&harness.application_home, &harness.tender_id);
    let repair_run_id = runs_before_restart[1].0.clone();
    intake.abort();
    let _ = intake.await;

    let RuntimeHarness {
        _root,
        application_home,
        codex,
        host,
        tender_id,
    } = harness;
    drop(host);
    fs::remove_file(codex.with_extension("repair-before-turn-pause"))
        .expect("clear repair pause before restart");
    let restarted = reopen_runtime_host(&application_home, &_root.path().join("resources"));
    let reopened = restarted
        .inspect_agent_runs(&tender_id)
        .expect("open Tender Store while repair is safely restartable");
    assert_eq!(
        reopened
            .iter()
            .find(|run| run.run_id == repair_run_id)
            .map(|run| run.state),
        Some(AgentRunState::Running)
    );
    persist_repair_thread_checkpoint(&application_home, &tender_id, &repair_run_id);

    let error = restarted
        .run_manager_intake_for_verification(&tender_id)
        .await
        .expect_err("same-Host thread checkpoint blocks automatic replay");
    assert_eq!(error.code, TenderErrorCode::InvalidCommand);
    let runs = tender_record_extraction_runs(&application_home, &tender_id);
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[1].0, repair_run_id);
    assert_eq!(runs[1].2, "indeterminate");
    let inspection = restarted
        .inspect_agent_runs(&tender_id)
        .expect("inspect same-Host indeterminate repair");
    let indeterminate = inspection
        .iter()
        .find(|run| run.run_id == repair_run_id)
        .expect("inspect checkpointed repair");
    assert_eq!(
        indeterminate
            .failure
            .as_ref()
            .map(|failure| failure.category),
        Some(ProviderFailureCategory::OutcomeUnknown)
    );
    assert_eq!(
        indeterminate
            .events
            .iter()
            .filter(|event| event.kind == ProviderEventKind::Terminal)
            .count(),
        1
    );
    assert_eq!(fixture_record_extraction_turn_count(&codex), 1);
}

#[tokio::test]
async fn manager_intake_retries_a_terminal_noncandidate_source_without_spending_repair() {
    let harness = RuntimeHarness::new("process-failure-before-turn");
    harness
        .parsed_pdf_evidence("retry-noncandidate-source", b"TENDER_RECORD_GOLDEN")
        .await;

    harness
        .host
        .run_manager_intake_for_verification(&harness.tender_id)
        .await
        .expect("terminal retry-safe process failure waits for a later intake cycle");
    let first_runs = tender_record_extraction_runs(&harness.application_home, &harness.tender_id);
    assert_eq!(first_runs.len(), 1);
    assert_eq!(first_runs[0].2, "failed");
    harness.set_agent_scenario("manager-intake-repair-invalid-then-valid");
    fs::write(
        harness.codex.with_extension("manager-output-release"),
        b"release",
    )
    .expect("release Manager outcome");

    harness
        .host
        .run_manager_intake_for_verification(&harness.tender_id)
        .await
        .expect("later intake cycle starts a fresh transport attempt");
    let runs = tender_record_extraction_runs(&harness.application_home, &harness.tender_id);
    assert_eq!(runs.len(), 3);
    assert!(runs[0].1.is_none());
    assert!(runs[1].1.is_none());
    assert_eq!(runs[1].2, "failed");
    assert_eq!(runs[2].1.as_deref(), Some(runs[1].0.as_str()));
    assert_eq!(runs[2].2, "completed");
    assert_eq!(fixture_record_extraction_turn_count(&harness.codex), 2);
}

#[tokio::test]
async fn manager_intake_retries_an_expired_predispatch_repair_without_a_second_semantic_child() {
    let harness = RuntimeHarness::new("manager-intake-repair-invalid-then-valid");
    harness
        .parsed_pdf_evidence("retry-expired-repair", b"TENDER_RECORD_GOLDEN")
        .await;
    fs::write(
        harness.codex.with_extension("repair-before-turn-pause"),
        b"pause",
    )
    .expect("pause persisted repair before Provider dispatch");
    fs::write(
        harness.codex.with_extension("manager-output-release"),
        b"release",
    )
    .expect("release Manager outcome");
    let host = harness.host.clone();
    let tender_id = harness.tender_id.clone();
    let intake =
        tokio::spawn(async move { host.run_manager_intake_for_verification(&tender_id).await });
    let waiting = harness.codex.with_extension("repair-before-turn-waiting");
    for _ in 0..1_000 {
        if waiting.is_file() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    assert!(waiting.is_file(), "repair did not reach pre-dispatch pause");
    let runs_before_restart =
        tender_record_extraction_runs(&harness.application_home, &harness.tender_id);
    assert_eq!(runs_before_restart.len(), 2);
    let source_run_id = runs_before_restart[0].0.clone();
    let first_repair_run_id = runs_before_restart[1].0.clone();
    intake.abort();
    let _ = intake.await;
    wait_until_agent_grant_expires(
        &harness.application_home,
        &harness.tender_id,
        &first_repair_run_id,
    )
    .await;

    let RuntimeHarness {
        _root,
        application_home,
        codex,
        host,
        tender_id,
    } = harness;
    drop(host);
    fs::remove_file(codex.with_extension("repair-before-turn-waiting"))
        .expect("clear first pre-dispatch waiting marker");
    let restarted = reopen_runtime_host(&application_home, &_root.path().join("resources"));
    let retry_host = restarted.clone();
    let retry_tender_id = tender_id.clone();
    let retry = tokio::spawn(async move {
        retry_host
            .run_manager_intake_for_verification(&retry_tender_id)
            .await
    });
    let waiting = codex.with_extension("repair-before-turn-waiting");
    for _ in 0..1_000 {
        if waiting.is_file() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    assert!(
        waiting.is_file(),
        "transport retry did not reach pre-dispatch pause: pause={}, finished={}, runs={:?}",
        codex.with_extension("repair-before-turn-pause").is_file(),
        retry.is_finished(),
        tender_record_extraction_runs(&application_home, &tender_id),
    );
    let retry_runs = tender_record_extraction_runs(&application_home, &tender_id);
    assert_eq!(retry_runs.len(), 3);
    let transport_retry_run_id = retry_runs[2].0.clone();
    assert_eq!(
        retry_runs[2].1.as_deref(),
        Some(first_repair_run_id.as_str())
    );
    assert_eq!(retry_runs[2].2, "running");
    retry.abort();
    let _ = retry.await;
    drop(restarted);
    fs::remove_file(codex.with_extension("repair-before-turn-pause"))
        .expect("clear pre-dispatch pause");

    let resumed = reopen_runtime_host(&application_home, &_root.path().join("resources"));
    resumed
        .run_manager_intake_for_verification(&tender_id)
        .await
        .expect("later intake cycle resumes the fresh repair transport attempt");

    let runs = tender_record_extraction_runs(&application_home, &tender_id);
    assert_eq!(runs.len(), 3);
    assert_eq!(runs[0].0, source_run_id);
    assert_eq!(runs[1].0, first_repair_run_id);
    assert_eq!(runs[1].1.as_deref(), Some(source_run_id.as_str()));
    assert_eq!(runs[1].2, "failed");
    assert_eq!(runs[2].0, transport_retry_run_id);
    assert_eq!(runs[2].1.as_deref(), Some(first_repair_run_id.as_str()));
    assert_eq!(runs[2].2, "completed");
    let database = application_home
        .join("tenders")
        .join(&tender_id)
        .join("tender.sqlite");
    let connection = rusqlite::Connection::open(database).expect("open Tender Store database");
    let semantic_child_count: u32 = connection
        .query_row(
            "SELECT COUNT(*) FROM agent_runs WHERE retry_of_run_id = ?1",
            [&source_run_id],
            |row| row.get(0),
        )
        .expect("count direct semantic repair children");
    assert_eq!(semantic_child_count, 1);
    assert_eq!(fixture_record_extraction_turn_count(&codex), 2);
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
    assert_eq!(extraction.published_record_count, 0);
    let runs = harness
        .host
        .inspect_agent_runs(&harness.tender_id)
        .expect("inspect rejected extraction attempts");
    assert_eq!(
        runs.len(),
        2,
        "one semantic repair and no third extraction call"
    );
    assert_eq!(fixture_record_extraction_turn_count(&harness.codex), 2);
    assert_eq!(runs[0].state, AgentRunState::Failed);
    assert!(runs[0].retry_of_run_id.is_none());
    assert_eq!(runs[1].state, AgentRunState::Failed);
    assert_eq!(
        runs[1].retry_of_run_id.as_deref(),
        Some(runs[0].run_id.as_str())
    );
    assert_eq!(runs[1].run_id, extraction.run.run_id);
    for run in &runs {
        assert!(run.proposed_result.is_none());
        assert!(records_for_run(&harness.host, &harness.tender_id, &run.run_id).is_empty());
    }
    assert!(inspect_all_records(&harness.host, &harness.tender_id).is_empty());

    let database = harness
        .application_home
        .join("tenders")
        .join(&harness.tender_id)
        .join("tender.sqlite");
    let connection = rusqlite::Connection::open(database).expect("open Tender Store database");
    let rejected_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM agent_run_rejected_outputs",
            [],
            |row| row.get(0),
        )
        .expect("count rejected output rows");
    assert_eq!(rejected_count, 2);
    for run in &runs {
        let run_failure = run.failure.as_ref().expect("OutputInvalid failure");
        assert_eq!(run_failure.category, ProviderFailureCategory::OutputInvalid);
        assert_eq!(run_failure.validation_issues.len(), 1);
        assert_eq!(
            run_failure.validation_issues[0].code,
            "duplicate_stable_key"
        );
        assert_eq!(
            run_failure.validation_issues[0].path,
            "/records/1/stable_key"
        );
        let (payload_json, payload_sha256, validation_issues_json): (String, String, String) =
            connection
                .query_row(
                    "SELECT payload_json, payload_sha256, validation_issues_json
             FROM agent_run_rejected_outputs WHERE run_id = ?1",
                    [&run.run_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .expect("load rejected output");
        let rejected_value: serde_json::Value =
            serde_json::from_str(&payload_json).expect("provider-shaped rejected payload");
        let mut rejected_handles = Vec::new();
        provider_handle_strings(&rejected_value, &mut rejected_handles);
        assert!(
            !rejected_handles.is_empty(),
            "rejected evidence must retain the provider-shaped candidate"
        );
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
            run_failure.validation_issues
        );
        let rejected = harness
            .host
            .rejected_agent_output(&harness.tender_id, &run.run_id)
            .expect("inspect rejected provider output");
        assert_eq!(rejected.payload_json, payload_json);
        assert_eq!(rejected.payload_sha256, payload_sha256);
        assert_eq!(rejected.validation_issues, run_failure.validation_issues);
    }
    assert!(connection
        .execute(
            "UPDATE agent_run_rejected_outputs SET payload_sha256 = ?1 WHERE run_id = ?2",
            ["0".repeat(64), runs[0].run_id.clone()],
        )
        .is_err());
    assert!(connection
        .execute(
            "DELETE FROM agent_run_rejected_outputs WHERE run_id = ?1",
            [&runs[0].run_id],
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
    let proposed = extraction
        .run
        .proposed_result
        .as_ref()
        .expect("successful extraction proposed result");
    let proposed_value: serde_json::Value =
        serde_json::from_str(&proposed.payload_json).expect("canonical successful proposal");
    assert_eq!(
        proposed.payload_json,
        serde_json_canonicalizer::to_string(&proposed_value)
            .expect("canonicalize successful proposal")
    );
    let mut leaked_handles = Vec::new();
    provider_handle_strings(&proposed_value, &mut leaked_handles);
    assert!(
        leaked_handles.is_empty(),
        "successful proposed results must not retain provider handles: {leaked_handles:?}"
    );
    let proposed_stable_keys = proposed_value["records"]
        .as_array()
        .expect("canonical proposed records")
        .iter()
        .map(|record| {
            record["stable_key"]
                .as_str()
                .expect("canonical proposed stable key")
                .to_owned()
        })
        .collect::<HashSet<_>>();
    let published_stable_keys = records
        .iter()
        .map(|record| record.stable_key.clone())
        .collect::<HashSet<_>>();
    assert_eq!(proposed_stable_keys, published_stable_keys);
    let mut proposed_evidence = HashSet::new();
    canonical_evidence_references(&proposed_value, &mut proposed_evidence);
    let published_evidence = records
        .iter()
        .flat_map(|record| {
            record
                .fields
                .iter()
                .flat_map(|field| field.evidence.iter())
                .chain(
                    record
                        .contradictions
                        .iter()
                        .flat_map(|contradiction| contradiction.evidence.iter()),
                )
                .map(|evidence| {
                    (
                        evidence.reference.artifact_id.clone(),
                        evidence.reference.version,
                        evidence.reference.ordinal,
                    )
                })
        })
        .collect::<HashSet<_>>();
    assert!(!proposed_evidence.is_empty());
    assert_eq!(proposed_evidence, published_evidence);
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
async fn schema_output_invalid_without_rejected_proposal_returns_the_source_run() {
    let harness = RuntimeHarness::new("record-extraction-parity-evidence-metadata");
    let evidence = harness
        .parsed_pdf_evidence("schema-output-invalid", b"TENDER_RECORD_GOLDEN")
        .await;

    let extraction = harness
        .host
        .run_tender_record_extraction(RunTenderRecordExtractionCommand {
            tender_id: harness.tender_id.clone(),
            evidence: evidence.references,
            authorities: Vec::new(),
        })
        .await
        .expect("return the schema-invalid source run without creating a repair");

    assert_eq!(extraction.run.state, AgentRunState::Failed);
    assert_eq!(
        extraction.run.failure.map(|failure| failure.category),
        Some(ProviderFailureCategory::OutputInvalid)
    );
    assert_eq!(
        tender_record_extraction_runs(&harness.application_home, &harness.tender_id).len(),
        1
    );
    assert_eq!(
        fixture_record_extraction_turn_count(&harness.codex),
        1,
        "a schema-invalid outcome without a retained proposal must not dispatch a repair turn"
    );
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
        .expect("count retained semantic rejections");
    assert_eq!(rejected_count, 0);
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
    assert_eq!(
        tender_record_extraction_runs(&harness.application_home, &harness.tender_id).len(),
        1
    );
    assert_eq!(
        fixture_record_extraction_turn_count(&harness.codex),
        1,
        "a stale OutputInvalid outcome must not dispatch a repair turn"
    );
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
async fn semantic_repair_preserves_active_change_recovery_context() {
    let harness = RuntimeHarness::new("manager-intake");
    let prior = harness
        .parsed_pdf_evidence("repair-change-original", b"TENDER_RECORD_GOLDEN")
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
        .expect("complete initial Manager Intake");
    harness
        .host
        .create_bid_decision_package(CreateBidDecisionPackageCommand {
            tender_id: harness.tender_id.clone(),
            base_version: None,
            disposition_updates: Vec::new(),
            manager_capability_demands: Vec::new(),
        })
        .expect("move the verified Tender into Bid Decision before recovery");
    let records_before_change = inspect_all_records(&harness.host, &harness.tender_id);
    let addendum = harness
        .parsed_pdf_evidence("repair-change-addendum", b"TENDER_RECORD_GOLDEN")
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
        .expect("confirm addendum relationship");
    let assessment = harness
        .host
        .inspect_change_assessments(InspectChangeAssessmentsCommand {
            tender_id: harness.tender_id.clone(),
            before_sequence: None,
            limit: 4,
        })
        .expect("inspect active change assessment")
        .active
        .expect("active change assessment");
    let allowed_stable_keys = assessment
        .impacts
        .iter()
        .filter(|impact| impact.kind == ChangeAssessmentImpactKind::TenderRecord)
        .filter_map(|impact| {
            records_before_change
                .iter()
                .find(|record| record.record_id == impact.object_id)
                .map(|record| record.stable_key.clone())
        })
        .collect::<std::collections::HashSet<_>>();
    assert!(!allowed_stable_keys.is_empty());
    let decided = harness
        .host
        .decide_change_assessment(DecideChangeAssessmentCommand {
            tender_id: harness.tender_id.clone(),
            assessment_id: assessment.assessment_id.clone(),
            assessment_manifest_sha256: assessment.manifest_sha256,
            classification: ChangeAssessmentClassification::Material,
            rationale: "The replacement evidence requires bounded record successors.".into(),
        })
        .expect("classify material change");
    assert_eq!(decided.status, ChangeAssessmentStatus::ReworkRequired);
    let extraction_turns_before = fixture_record_extraction_turn_count(&harness.codex);
    harness.set_agent_scenario("record-extraction-change-recovery-invalid-then-valid");

    let extraction = harness
        .host
        .run_tender_record_extraction(RunTenderRecordExtractionCommand {
            tender_id: harness.tender_id.clone(),
            evidence: addendum.references,
            authorities: Vec::new(),
        })
        .await
        .expect("repair the bounded material-change extraction");
    assert_eq!(extraction.run.state, AgentRunState::Completed);
    assert_eq!(
        fixture_record_extraction_turn_count(&harness.codex),
        extraction_turns_before + 2
    );
    let all_extraction_runs =
        tender_record_extraction_runs(&harness.application_home, &harness.tender_id);
    let repair_runs = all_extraction_runs[all_extraction_runs.len() - 2..].to_vec();
    assert_eq!(repair_runs[0].2, "failed");
    assert_eq!(repair_runs[1].1.as_deref(), Some(repair_runs[0].0.as_str()));
    assert_eq!(repair_runs[1].2, "completed");
    let inspections = extraction_run_inspections(&harness.host, &harness.tender_id, &repair_runs);
    assert_independent_provider_turn_ownership(&inspections);
    assert_eq!(
        inspections[0].task.exact_inputs,
        inspections[1].task.exact_inputs
    );
    assert!(inspections[1]
        .task
        .exact_inputs
        .iter()
        .any(|input| input.kind == "change_assessment"
            && input.reference == assessment.assessment_id));

    let observed: serde_json::Value = serde_json::from_slice(
        &fs::read(harness.codex.with_extension("agent-workspace"))
            .expect("read repair provider workspace observation"),
    )
    .expect("parse repair provider workspace observation");
    let repair_view = observed
        .pointer("/provider_data_view/change_assessment")
        .expect("repair provider data preserves change recovery context");
    assert_eq!(
        repair_view["assessment_id"].as_str(),
        Some(assessment.assessment_id.as_str())
    );
    let repair_allowed_keys = repair_view["allowed_stable_keys"]
        .as_array()
        .expect("repair allowed stable keys")
        .iter()
        .filter_map(serde_json::Value::as_str)
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(
        repair_allowed_keys,
        allowed_stable_keys
            .iter()
            .map(String::as_str)
            .collect::<std::collections::HashSet<_>>()
    );
    assert!(
        repair_view["prior_records"]
            .as_array()
            .is_some_and(|records| !records.is_empty()),
        "repair Data View retains immutable prior-record context"
    );
    let published_keys = records_for_run(&harness.host, &harness.tender_id, &repair_runs[1].0)
        .into_iter()
        .map(|record| record.stable_key)
        .collect::<std::collections::HashSet<_>>();
    assert!(!published_keys.is_empty());
    assert!(
        published_keys
            .iter()
            .all(|stable_key| allowed_stable_keys.contains(stable_key)),
        "only exact impacted stable keys may publish as repaired successors"
    );
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
