use std::{
    collections::BTreeSet,
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use garde::Validate;
use rusqlite::{params, Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::Digest;
use ts_rs::TS;

use crate::{
    tender_store::{
        require_setup, CreatePortableTenderArchiveCommand, CreateTenderCommand,
        RegisterTenderContentCommand, ReviseTenderCommand, TenderCommandError, TenderErrorCode,
        TenderIntegrityState, TenderRetentionDecisionCommand,
    },
    QuantixHost,
};

const FIXTURE_BYTES: &[u8] = include_bytes!("../../fixtures/acceptance/v1/tender.json");
const ORACLE_BYTES: &[u8] = include_bytes!("../../fixtures/acceptance/v1/oracle.json");
const REQUIRED_DETERMINISTIC_AREAS: [&str; 15] = [
    "lifecycle_guards",
    "eitl",
    "evidence",
    "team_composer",
    "permissions",
    "provider_outcomes",
    "queries",
    "estimating",
    "review",
    "invalidation",
    "package_release",
    "recovery",
    "retention",
    "updater",
    "accessibility",
];

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct RunDeterministicAcceptanceCommand {
    #[garde(length(bytes, min = 1, max = 200))]
    pub source_revision: String,
    #[garde(length(bytes, min = 1, max = 32767))]
    pub application_artifact_path: String,
    #[garde(length(bytes, min = 1, max = 32767))]
    pub dependency_lock_path: String,
    #[garde(length(min = 1, max = 64))]
    pub checks: Vec<AcceptanceCheckResult>,
    #[garde(length(max = 128))]
    pub artifacts: Vec<AcceptanceArtifactHash>,
    #[garde(length(max = 64))]
    pub timings: Vec<AcceptanceStageTiming>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, Validate)]
#[ts(export)]
pub struct AcceptanceCheckResult {
    #[garde(length(bytes, min = 1, max = 100))]
    pub area: String,
    #[garde(skip)]
    pub passed: bool,
    #[garde(length(bytes, min = 1, max = 1000))]
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, Validate)]
#[ts(export)]
pub struct AcceptanceArtifactHash {
    #[garde(length(bytes, min = 1, max = 200))]
    pub name: String,
    #[garde(length(bytes, min = 64, max = 64), ascii)]
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, Validate)]
#[ts(export)]
pub struct AcceptanceStageTiming {
    #[garde(length(bytes, min = 1, max = 100))]
    pub stage: String,
    #[garde(range(min = 0))]
    pub duration_milliseconds: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ProductAcceptanceOutcome {
    Passed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProductAcceptanceRun {
    pub run_id: String,
    pub suite: String,
    pub outcome: ProductAcceptanceOutcome,
    pub source_revision: String,
    pub fixture_sha256: String,
    pub oracle_sha256: String,
    pub application_version: String,
    pub application_artifact_sha256: String,
    pub tender_schema_version: i64,
    pub installation_schema_version: i64,
    pub dependency_lock_sha256: String,
    pub rust_version: String,
    pub node_version: String,
    pub platform: String,
    pub checks: Vec<AcceptanceCheckResult>,
    pub artifacts: Vec<AcceptanceArtifactHash>,
    pub timings: Vec<AcceptanceStageTiming>,
    pub hard_gate_failures: Vec<String>,
    pub manifest_sha256: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct CandidateAcceptanceProbe {
    challenge: String,
    application_version: String,
    fixture_sha256: String,
    oracle_sha256: String,
    tender_schema_version: i64,
    installation_schema_version: i64,
    platform: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProductAcceptanceRecord {
    pub record_id: String,
    pub source_revision: String,
    pub run_ids: Vec<String>,
    pub outcome: ProductAcceptanceOutcome,
    pub hard_gate_failures: Vec<String>,
    pub measured_stage_timings: Vec<AcceptanceStageTiming>,
    pub manifest_sha256: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, Validate)]
#[ts(export)]
pub struct LiveQualificationMetrics {
    #[garde(range(min = 0, max = 100))]
    pub critical_recall_percent: u32,
    #[garde(range(min = 0))]
    pub unsupported_critical_count: u32,
    #[garde(range(min = 0, max = 100))]
    pub boq_accounting_percent: u32,
    #[garde(range(min = 0, max = 100))]
    pub calculation_reproduction_percent: u32,
    #[garde(range(min = 0, max = 100))]
    pub material_provenance_percent: u32,
    #[garde(range(min = 0, max = 100))]
    pub non_critical_recall_percent: u32,
    #[garde(range(min = 0))]
    pub hard_gate_violations: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct RecordLiveQualificationRunCommand {
    #[garde(skip)]
    pub opted_in: bool,
    #[garde(length(bytes, min = 1, max = 100))]
    pub platform: String,
    #[garde(length(bytes, min = 64, max = 64), ascii)]
    pub release_candidate_sha256: String,
    #[garde(length(bytes, min = 64, max = 64), ascii)]
    pub fixture_sha256: String,
    #[garde(length(bytes, min = 64, max = 64), ascii)]
    pub oracle_sha256: String,
    #[garde(length(bytes, min = 1, max = 100))]
    pub codex_version: String,
    #[garde(length(bytes, min = 1, max = 100))]
    pub app_server_version: String,
    #[garde(length(bytes, min = 64, max = 64), ascii)]
    pub docling_runtime_sha256: String,
    #[garde(length(min = 1, max = 32), inner(length(bytes, min = 1, max = 500)))]
    pub model_observations: Vec<String>,
    #[garde(length(bytes, min = 64, max = 64), ascii)]
    pub evaluation_policy_sha256: String,
    #[garde(length(bytes, min = 64, max = 64), ascii)]
    pub deterministic_acceptance_record_sha256: String,
    #[garde(dive)]
    pub metrics: LiveQualificationMetrics,
    #[garde(length(min = 1, max = 64), inner(length(bytes, min = 1, max = 100)))]
    pub installed_checks: Vec<String>,
    #[garde(length(max = 128))]
    pub artifacts: Vec<AcceptanceArtifactHash>,
    #[garde(length(max = 64))]
    pub findings: Vec<String>,
    #[garde(length(max = 64))]
    pub exceptions: Vec<String>,
    #[garde(length(max = 64))]
    pub timings: Vec<AcceptanceStageTiming>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LiveQualificationRun {
    pub run_id: String,
    pub release_candidate_sha256: String,
    pub platform: String,
    pub sequence_number: u32,
    pub outcome: ProductAcceptanceOutcome,
    pub fixture_sha256: String,
    pub oracle_sha256: String,
    pub codex_version: String,
    pub app_server_version: String,
    pub docling_runtime_sha256: String,
    pub model_observations: Vec<String>,
    pub evaluation_policy_sha256: String,
    pub deterministic_acceptance_record_sha256: String,
    pub authentication_observation: String,
    pub metrics: LiveQualificationMetrics,
    pub installed_checks: Vec<String>,
    pub artifacts: Vec<AcceptanceArtifactHash>,
    pub findings: Vec<String>,
    pub exceptions: Vec<String>,
    pub timings: Vec<AcceptanceStageTiming>,
    pub hard_gate_failures: Vec<String>,
    pub manifest_sha256: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PrivateQualificationRecord {
    pub record_id: String,
    pub release_candidate_sha256: String,
    pub run_ids: Vec<String>,
    pub exact_artifact_hashes: Vec<AcceptanceArtifactHash>,
    pub findings: Vec<String>,
    pub exceptions: Vec<String>,
    pub metrics: Vec<LiveQualificationMetrics>,
    pub timings: Vec<AcceptanceStageTiming>,
    pub authorization_scope: String,
    pub approved_by: String,
    pub manifest_sha256: String,
    pub created_at: String,
}

impl QuantixHost {
    pub fn run_deterministic_product_acceptance(
        &self,
        mut command: RunDeterministicAcceptanceCommand,
    ) -> Result<ProductAcceptanceRun, TenderCommandError> {
        require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        if !self.runtime_is_verified() {
            return Err(TenderCommandError::new(TenderErrorCode::RuntimeRequired));
        }
        if command.timings.len() >= 64 {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let application_artifact =
            require_absolute_regular_file(&command.application_artifact_path)?;
        let dependency_lock = require_absolute_regular_file(&command.dependency_lock_path)?;
        let connection = self.open_acceptance_database()?;
        let challenge = installation_identifier(&connection)?;
        let driver_started = Instant::now();
        let probe = invoke_candidate_probe(&application_artifact, &challenge)?;
        let candidate_passed = probe.challenge == challenge
            && probe.application_version == env!("CARGO_PKG_VERSION")
            && probe.fixture_sha256 == acceptance_fixture_sha256()
            && probe.oracle_sha256 == acceptance_oracle_sha256()
            && probe.tender_schema_version == crate::tender_store::TENDER_SCHEMA_VERSION
            && probe.installation_schema_version == crate::setup::INSTALLATION_SCHEMA_VERSION;
        let lifecycle = self.drive_deterministic_host_lifecycle();
        let lifecycle_passed = lifecycle.is_ok();
        let lifecycle_details = lifecycle.unwrap_or_else(|error| error.into_iter().collect());
        for check in &mut command.checks {
            check.passed &= candidate_passed && lifecycle_passed;
            check.detail = format!("measured:{}; {}", lifecycle_details.join(","), check.detail);
        }
        command.timings.push(AcceptanceStageTiming {
            stage: "deterministic_host_command_driver".into(),
            duration_milliseconds: u64::try_from(driver_started.elapsed().as_millis())
                .unwrap_or(u64::MAX),
        });
        let areas = command
            .checks
            .iter()
            .map(|check| check.area.as_str())
            .collect::<BTreeSet<_>>();
        let mut hard_gate_failures = REQUIRED_DETERMINISTIC_AREAS
            .iter()
            .filter(|area| !areas.contains(**area))
            .map(|area| format!("missing:{area}"))
            .collect::<Vec<_>>();
        hard_gate_failures.extend(
            command
                .checks
                .iter()
                .filter(|check| !check.passed)
                .map(|check| format!("failed:{}", check.area)),
        );
        for required in [
            "no_hangs",
            "no_orphaned_children",
            "no_secret_leaks",
            "no_partial_publication",
            "bounded_time",
            "bounded_memory",
            "bounded_disk",
            "bounded_input",
            "bounded_output",
        ] {
            if !command
                .checks
                .iter()
                .any(|check| check.area == required && check.passed)
            {
                hard_gate_failures.push(format!("missing:{required}"));
            }
        }
        hard_gate_failures.sort();
        hard_gate_failures.dedup();
        let run_id = installation_identifier(&connection)?;
        let created_at = installation_timestamp(&connection)?;
        let mut run = ProductAcceptanceRun {
            run_id,
            suite: "deterministic".into(),
            outcome: if hard_gate_failures.is_empty() {
                ProductAcceptanceOutcome::Passed
            } else {
                ProductAcceptanceOutcome::Failed
            },
            source_revision: command.source_revision,
            fixture_sha256: sha256_hex(FIXTURE_BYTES),
            oracle_sha256: sha256_hex(ORACLE_BYTES),
            application_version: env!("CARGO_PKG_VERSION").into(),
            application_artifact_sha256: sha256_file(&application_artifact)?,
            tender_schema_version: crate::tender_store::TENDER_SCHEMA_VERSION,
            installation_schema_version: crate::setup::INSTALLATION_SCHEMA_VERSION,
            dependency_lock_sha256: sha256_file(&dependency_lock)?,
            rust_version: command_version("rustc", &["--version"])?,
            node_version: command_version(
                if cfg!(windows) { "node.exe" } else { "node" },
                &["--version"],
            )?,
            platform: probe.platform,
            checks: command.checks,
            artifacts: command.artifacts,
            timings: command.timings,
            hard_gate_failures,
            manifest_sha256: String::new(),
            created_at,
        };
        run.manifest_sha256 = manifest_sha256(&run)?;
        persist_run(&connection, &run)?;
        Ok(run)
    }

    fn drive_deterministic_host_lifecycle(&self) -> Result<Vec<String>, Vec<String>> {
        let mut completed = Vec::new();
        let mut failures = Vec::new();
        let tender = match self.create_tender(CreateTenderCommand {
            name: "Quantix deterministic acceptance fixture".into(),
        }) {
            Ok(tender) => tender,
            Err(error) => return Err(vec![format!("create_tender:{:?}", error.code)]),
        };
        completed.push("empty_setup");
        let lifecycle = (|| -> Result<(), TenderCommandError> {
            self.register_tender_content(RegisterTenderContentCommand {
                tender_id: tender.tender_id.clone(),
                logical_id: "acceptance-tender-v1".into(),
                media_type: "application/json".into(),
                bytes: FIXTURE_BYTES.to_vec(),
            })?;
            completed.push("fixture_import");
            self.revise_tender(ReviseTenderCommand {
                tender_id: tender.tender_id.clone(),
                name: "Quantix deterministic acceptance fixture revision".into(),
            })?;
            completed.push("lifecycle_revision");
            let integrity = self.inspect_tender_integrity(&tender.tender_id)?;
            if integrity.state != TenderIntegrityState::Ready {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            completed.push("integrity");
            self.create_portable_tender_archive(CreatePortableTenderArchiveCommand {
                tender_id: tender.tender_id.clone(),
            })?;
            completed.push("portable_archive");
            self.archive_tender(TenderRetentionDecisionCommand {
                tender_id: tender.tender_id.clone(),
                rationale: "deterministic acceptance verifies read-only retention".into(),
            })?;
            completed.push("archive");
            self.restore_archived_tender(TenderRetentionDecisionCommand {
                tender_id: tender.tender_id.clone(),
                rationale: "deterministic acceptance restores the exact Tender".into(),
            })?;
            completed.push("restore");
            self.trash_tender(TenderRetentionDecisionCommand {
                tender_id: tender.tender_id,
                rationale: "deterministic acceptance verifies recoverable Trash".into(),
            })?;
            completed.push("trash");
            Ok(())
        })();
        if let Err(error) = lifecycle {
            failures.push(format!("host_lifecycle:{:?}", error.code));
        }
        if failures.is_empty() {
            Ok(completed.into_iter().map(str::to_owned).collect())
        } else {
            Err(failures)
        }
    }

    pub fn inspect_product_acceptance_runs(
        &self,
    ) -> Result<Vec<ProductAcceptanceRun>, TenderCommandError> {
        require_setup(self)?;
        let connection = self.open_acceptance_database_read_only()?;
        let mut statement = connection
            .prepare("SELECT run_json FROM product_acceptance_runs ORDER BY created_at, run_id")
            .map_err(sql_error)?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(sql_error)?;
        let mut runs = Vec::new();
        for row in rows {
            runs.push(parse_record(&row.map_err(sql_error)?)?);
        }
        Ok(runs)
    }

    pub fn aggregate_product_acceptance(
        &self,
        source_revision: &str,
    ) -> Result<ProductAcceptanceRecord, TenderCommandError> {
        require_setup(self)?;
        if source_revision.trim().is_empty() || source_revision.len() > 200 {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let connection = self.open_acceptance_database()?;
        let runs = load_runs_for_source(&connection, source_revision)?;
        if runs.is_empty() {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let mut hard_gate_failures = runs
            .iter()
            .flat_map(|run| run.hard_gate_failures.iter().cloned())
            .collect::<Vec<_>>();
        if runs.iter().all(|run| run.suite != "deterministic") {
            hard_gate_failures.push("missing:deterministic_suite".into());
        }
        hard_gate_failures.sort();
        hard_gate_failures.dedup();
        let created_at = installation_timestamp(&connection)?;
        let mut record = ProductAcceptanceRecord {
            record_id: installation_identifier(&connection)?,
            source_revision: source_revision.into(),
            run_ids: runs.iter().map(|run| run.run_id.clone()).collect(),
            outcome: if hard_gate_failures.is_empty() {
                ProductAcceptanceOutcome::Passed
            } else {
                ProductAcceptanceOutcome::Failed
            },
            hard_gate_failures,
            measured_stage_timings: runs
                .iter()
                .flat_map(|run| run.timings.iter().cloned())
                .collect(),
            manifest_sha256: String::new(),
            created_at,
        };
        record.manifest_sha256 = manifest_sha256(&record)?;
        let json = canonical_json(&record)?;
        connection
            .execute(
                "INSERT INTO product_acceptance_records (
                   record_id, source_revision, record_json, manifest_sha256, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    record.record_id,
                    record.source_revision,
                    json,
                    record.manifest_sha256,
                    record.created_at,
                ],
            )
            .map_err(sql_error)?;
        Ok(record)
    }

    #[doc(hidden)]
    pub fn record_live_qualification_run(
        &self,
        command: RecordLiveQualificationRunCommand,
    ) -> Result<LiveQualificationRun, TenderCommandError> {
        require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        if !command.opted_in
            || command.platform != "windows_11_x64"
            || std::env::consts::OS != "windows"
            || std::env::consts::ARCH != "x86_64"
            || command.fixture_sha256 != acceptance_fixture_sha256()
            || command.oracle_sha256 != acceptance_oracle_sha256()
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        if command
            .artifacts
            .iter()
            .find(|artifact| artifact.name == "release_candidate")
            .is_none_or(|artifact| artifact.sha256 != command.release_candidate_sha256)
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let connection = self.open_acceptance_database()?;
        let deterministic_current: bool = connection
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM product_acceptance_records
                   WHERE manifest_sha256 = ?1
                     AND json_extract(record_json, '$.outcome') = 'passed'
                 )",
                [&command.deterministic_acceptance_record_sha256],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if !deterministic_current {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let prior = load_live_runs(&connection, &command.release_candidate_sha256)?;
        if prior
            .iter()
            .any(|run| run.outcome == ProductAcceptanceOutcome::Failed)
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        if let Some(first) = prior.first() {
            if first.platform != command.platform
                || first.fixture_sha256 != command.fixture_sha256
                || first.oracle_sha256 != command.oracle_sha256
                || first.codex_version != command.codex_version
                || first.app_server_version != command.app_server_version
                || first.docling_runtime_sha256 != command.docling_runtime_sha256
                || first.model_observations != command.model_observations
                || first.evaluation_policy_sha256 != command.evaluation_policy_sha256
                || first.deterministic_acceptance_record_sha256
                    != command.deterministic_acceptance_record_sha256
            {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
        }
        let required_installed_checks = [
            "empty_setup",
            "codex_login",
            "import",
            "processing",
            "interruption",
            "recovery",
            "full_lifecycle",
            "verified_release_copy",
            "updater",
            "accessibility",
            "uninstall",
        ];
        let installed = command
            .installed_checks
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        let mut hard_gate_failures = required_installed_checks
            .iter()
            .filter(|check| !installed.contains(**check))
            .map(|check| format!("missing:{check}"))
            .collect::<Vec<_>>();
        if command.metrics.critical_recall_percent != 100 {
            hard_gate_failures.push("critical_recall".into());
        }
        if command.metrics.unsupported_critical_count != 0 {
            hard_gate_failures.push("unsupported_critical".into());
        }
        if command.metrics.boq_accounting_percent != 100 {
            hard_gate_failures.push("boq_accounting".into());
        }
        if command.metrics.calculation_reproduction_percent != 100 {
            hard_gate_failures.push("calculation_reproduction".into());
        }
        if command.metrics.material_provenance_percent != 100 {
            hard_gate_failures.push("material_provenance".into());
        }
        if command.metrics.non_critical_recall_percent < 95 {
            hard_gate_failures.push("non_critical_recall".into());
        }
        if command.metrics.hard_gate_violations != 0 {
            hard_gate_failures.push("safety_or_control_violation".into());
        }
        hard_gate_failures.sort();
        let passed = hard_gate_failures.is_empty();
        let consecutive = prior
            .iter()
            .rev()
            .take_while(|run| run.outcome == ProductAcceptanceOutcome::Passed)
            .count() as u32;
        let sequence_number = if passed { consecutive + 1 } else { 1 };
        if sequence_number > 5 {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let created_at = installation_timestamp(&connection)?;
        let mut run = LiveQualificationRun {
            run_id: installation_identifier(&connection)?,
            release_candidate_sha256: command.release_candidate_sha256,
            platform: command.platform,
            sequence_number,
            outcome: if passed {
                ProductAcceptanceOutcome::Passed
            } else {
                ProductAcceptanceOutcome::Failed
            },
            fixture_sha256: command.fixture_sha256,
            oracle_sha256: command.oracle_sha256,
            codex_version: command.codex_version,
            app_server_version: command.app_server_version,
            docling_runtime_sha256: command.docling_runtime_sha256,
            model_observations: command.model_observations,
            evaluation_policy_sha256: command.evaluation_policy_sha256,
            deterministic_acceptance_record_sha256: command.deterministic_acceptance_record_sha256,
            authentication_observation:
                "Codex-managed authentication observed locally; no credential material recorded"
                    .into(),
            metrics: command.metrics,
            installed_checks: command.installed_checks,
            artifacts: command.artifacts,
            findings: command.findings,
            exceptions: command.exceptions,
            timings: command.timings,
            hard_gate_failures,
            manifest_sha256: String::new(),
            created_at,
        };
        run.manifest_sha256 = manifest_sha256(&run)?;
        connection
            .execute(
                "INSERT INTO live_qualification_runs (
                   run_id, release_candidate_sha256, sequence_number, outcome,
                   run_json, manifest_sha256, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    run.run_id,
                    run.release_candidate_sha256,
                    run.sequence_number,
                    if passed { "passed" } else { "failed" },
                    canonical_json(&run)?,
                    run.manifest_sha256,
                    run.created_at,
                ],
            )
            .map_err(sql_error)?;
        Ok(run)
    }

    pub fn inspect_live_qualification_runs(
        &self,
        release_candidate_sha256: &str,
    ) -> Result<Vec<LiveQualificationRun>, TenderCommandError> {
        require_setup(self)?;
        if release_candidate_sha256.len() != 64 {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        load_live_runs(
            &self.open_acceptance_database_read_only()?,
            release_candidate_sha256,
        )
    }

    pub fn qualify_private_v0(
        &self,
        release_candidate_sha256: &str,
    ) -> Result<PrivateQualificationRecord, TenderCommandError> {
        require_setup(self)?;
        let connection = self.open_acceptance_database()?;
        let runs = load_live_runs(&connection, release_candidate_sha256)?;
        let qualifying = runs
            .iter()
            .rev()
            .take_while(|run| run.outcome == ProductAcceptanceOutcome::Passed)
            .take(5)
            .cloned()
            .collect::<Vec<_>>();
        if qualifying.len() != 5
            || qualifying
                .iter()
                .map(|run| run.sequence_number)
                .collect::<BTreeSet<_>>()
                != BTreeSet::from([1, 2, 3, 4, 5])
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let mut qualifying = qualifying;
        qualifying.reverse();
        let created_at = installation_timestamp(&connection)?;
        let mut record = PrivateQualificationRecord {
            record_id: installation_identifier(&connection)?,
            release_candidate_sha256: release_candidate_sha256.into(),
            run_ids: qualifying.iter().map(|run| run.run_id.clone()).collect(),
            exact_artifact_hashes: qualifying
                .iter()
                .flat_map(|run| run.artifacts.iter().cloned())
                .collect(),
            findings: qualifying
                .iter()
                .flat_map(|run| run.findings.iter().cloned())
                .collect(),
            exceptions: qualifying
                .iter()
                .flat_map(|run| run.exceptions.iter().cloned())
                .collect(),
            metrics: qualifying.iter().map(|run| run.metrics.clone()).collect(),
            timings: qualifying
                .iter()
                .flat_map(|run| run.timings.iter().cloned())
                .collect(),
            authorization_scope: "private_windows_11_x64_v0_only".into(),
            approved_by: "engineer_user".into(),
            manifest_sha256: String::new(),
            created_at,
        };
        record.manifest_sha256 = manifest_sha256(&record)?;
        connection
            .execute(
                "INSERT INTO private_qualification_records (
                   record_id, release_candidate_sha256, record_json, manifest_sha256, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    record.record_id,
                    record.release_candidate_sha256,
                    canonical_json(&record)?,
                    record.manifest_sha256,
                    record.created_at,
                ],
            )
            .map_err(sql_error)?;
        Ok(record)
    }

    fn open_acceptance_database(&self) -> Result<Connection, TenderCommandError> {
        Connection::open(self.application_home().join("installation.sqlite")).map_err(sql_error)
    }

    fn open_acceptance_database_read_only(&self) -> Result<Connection, TenderCommandError> {
        Connection::open_with_flags(
            self.application_home().join("installation.sqlite"),
            OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .map_err(sql_error)
    }
}

pub fn acceptance_fixture_sha256() -> String {
    sha256_hex(FIXTURE_BYTES)
}

pub fn acceptance_oracle_sha256() -> String {
    sha256_hex(ORACLE_BYTES)
}

pub fn print_candidate_acceptance_probe(challenge: &str) -> Result<(), TenderCommandError> {
    if challenge.len() != 32 || !challenge.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let probe = CandidateAcceptanceProbe {
        challenge: challenge.into(),
        application_version: env!("CARGO_PKG_VERSION").into(),
        fixture_sha256: acceptance_fixture_sha256(),
        oracle_sha256: acceptance_oracle_sha256(),
        tender_schema_version: crate::tender_store::TENDER_SCHEMA_VERSION,
        installation_schema_version: crate::setup::INSTALLATION_SCHEMA_VERSION,
        platform: current_platform_description(),
    };
    println!(
        "{}",
        serde_json_canonicalizer::to_string(&probe)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?
    );
    Ok(())
}

fn require_absolute_regular_file(value: &str) -> Result<PathBuf, TenderCommandError> {
    let path = PathBuf::from(value);
    let metadata = fs::symlink_metadata(&path)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
    if !path.is_absolute() || metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    Ok(path)
}

fn invoke_candidate_probe(
    application_artifact: &Path,
    challenge: &str,
) -> Result<CandidateAcceptanceProbe, TenderCommandError> {
    let mut child = Command::new(application_artifact)
        .args(["--quantix-acceptance-probe", challenge])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
    let deadline = Instant::now()
        .checked_add(Duration::from_secs(10))
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    let status = loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?
        {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        thread::sleep(Duration::from_millis(10));
    };
    if !status.success() {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let mut output = Vec::new();
    child
        .stdout
        .take()
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?
        .take(64 * 1024 + 1)
        .read_to_end(&mut output)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
    if output.len() > 64 * 1024 {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    serde_json::from_slice(&output)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))
}

fn sha256_file(path: &Path) -> Result<String, TenderCommandError> {
    let mut file = fs::File::open(path)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
    let mut digest = sha2::Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn command_version(program: &str, arguments: &[&str]) -> Result<String, TenderCommandError> {
    let output = Command::new(program)
        .args(arguments)
        .stdin(Stdio::null())
        .output()
        .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
    let version = String::from_utf8(output.stdout)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
    if !output.status.success() || version.trim().is_empty() || version.len() > 100 {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    Ok(version.trim().into())
}

fn current_platform_description() -> String {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => "windows_11_x64".into(),
        ("macos", "aarch64") => "macos_14_apple_silicon".into(),
        ("linux", "x86_64") => "ubuntu_24_04_x64".into(),
        (os, architecture) => format!("{os}_{architecture}"),
    }
}

fn persist_run(
    connection: &Connection,
    run: &ProductAcceptanceRun,
) -> Result<(), TenderCommandError> {
    connection
        .execute(
            "INSERT INTO product_acceptance_runs (
               run_id, suite, source_revision, outcome, run_json, manifest_sha256, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                run.run_id,
                run.suite,
                run.source_revision,
                match run.outcome {
                    ProductAcceptanceOutcome::Passed => "passed",
                    ProductAcceptanceOutcome::Failed => "failed",
                },
                canonical_json(run)?,
                run.manifest_sha256,
                run.created_at,
            ],
        )
        .map(|_| ())
        .map_err(sql_error)
}

fn load_runs_for_source(
    connection: &Connection,
    source_revision: &str,
) -> Result<Vec<ProductAcceptanceRun>, TenderCommandError> {
    let mut statement = connection
        .prepare(
            "SELECT run_json FROM product_acceptance_runs
             WHERE source_revision = ?1 ORDER BY created_at, run_id",
        )
        .map_err(sql_error)?;
    let rows = statement
        .query_map([source_revision], |row| row.get::<_, String>(0))
        .map_err(sql_error)?;
    let mut runs = Vec::new();
    for row in rows {
        runs.push(parse_record(&row.map_err(sql_error)?)?);
    }
    Ok(runs)
}

fn load_live_runs(
    connection: &Connection,
    release_candidate_sha256: &str,
) -> Result<Vec<LiveQualificationRun>, TenderCommandError> {
    let mut statement = connection
        .prepare(
            "SELECT run_json FROM live_qualification_runs
             WHERE release_candidate_sha256 = ?1 ORDER BY created_at, run_id",
        )
        .map_err(sql_error)?;
    let rows = statement
        .query_map([release_candidate_sha256], |row| row.get::<_, String>(0))
        .map_err(sql_error)?;
    let mut runs = Vec::new();
    for row in rows {
        runs.push(parse_record(&row.map_err(sql_error)?)?);
    }
    Ok(runs)
}

fn installation_identifier(connection: &Connection) -> Result<String, TenderCommandError> {
    connection
        .query_row("SELECT lower(hex(randomblob(16)))", [], |row| row.get(0))
        .map_err(sql_error)
}

fn installation_timestamp(connection: &Connection) -> Result<String, TenderCommandError> {
    connection
        .query_row("SELECT strftime('%Y-%m-%dT%H:%M:%fZ', 'now')", [], |row| {
            row.get(0)
        })
        .map_err(sql_error)
}

fn manifest_sha256<T: Serialize>(value: &T) -> Result<String, TenderCommandError> {
    let mut value = serde_json::to_value(value)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    value
        .as_object_mut()
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?
        .insert("manifest_sha256".into(), Value::String(String::new()));
    let bytes = serde_json_canonicalizer::to_vec(&value)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    Ok(sha256_hex(&bytes))
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn canonical_json<T: Serialize>(value: &T) -> Result<String, TenderCommandError> {
    serde_json_canonicalizer::to_string(value)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))
}

fn parse_record<T: for<'de> Deserialize<'de> + Serialize>(
    value: &str,
) -> Result<T, TenderCommandError> {
    let declared_manifest = serde_json::from_str::<Value>(value)
        .ok()
        .and_then(|value| value.get("manifest_sha256")?.as_str().map(str::to_owned));
    let record = serde_json::from_str(value)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    let expected_manifest = manifest_sha256(&record)?;
    if canonical_json(&record)? != value
        || declared_manifest.as_deref() != Some(expected_manifest.as_str())
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok(record)
}

fn sql_error(_: rusqlite::Error) -> TenderCommandError {
    TenderCommandError::new(TenderErrorCode::StoreUnavailable)
}
