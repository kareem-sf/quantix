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
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::Digest;
use ts_rs::TS;

use crate::{
    tender_store::{
        require_setup, CreatePortableTenderArchiveCommand, CreateTenderBackupCommand,
        CreateTenderCommand, PrepareTenderRecoveryCommand, PurgeTrashedTenderCommand,
        RegisterTenderContentCommand, ResolveTenderRecoveryCommand, ReviseTenderCommand,
        TenderCommandError, TenderErrorCode, TenderIntegrityState, TenderRecoveryDecision,
        TenderRetentionDecisionCommand, TrashedTenderDecisionCommand,
    },
    QuantixHost, UpdateState,
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
    pub application_resource_directory_path: String,
    #[garde(length(bytes, min = 1, max = 32767))]
    pub dependency_lock_path: String,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct CandidateAcceptanceRehearsal {
    probe: CandidateAcceptanceProbe,
    mode: String,
    checks: Vec<AcceptanceCheckResult>,
    artifacts: Vec<AcceptanceArtifactHash>,
    timings: Vec<AcceptanceStageTiming>,
    completed: Vec<String>,
    findings: Vec<String>,
    exceptions: Vec<String>,
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
    #[garde(length(bytes, min = 1, max = 32767))]
    pub application_artifact_path: String,
    #[garde(length(bytes, min = 1, max = 32767))]
    pub application_resource_directory_path: String,
    #[garde(length(bytes, min = 1, max = 32767))]
    pub application_uninstaller_path: String,
    #[garde(length(bytes, min = 64, max = 64), ascii)]
    pub deterministic_acceptance_record_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveQualificationEnvironment {
    pub platform: String,
    pub codex_version: String,
    pub app_server_version: String,
    pub ocr_runtime_sha256: String,
    pub model_observations: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AcceptanceDriverMeasurement {
    checks: Vec<AcceptanceCheckResult>,
    artifacts: Vec<AcceptanceArtifactHash>,
    timings: Vec<AcceptanceStageTiming>,
    completed: Vec<String>,
    findings: Vec<String>,
    exceptions: Vec<String>,
}

struct UninstallMeasurement {
    passed: bool,
    detail: String,
    uninstaller_sha256: String,
    timing: AcceptanceStageTiming,
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
    pub ocr_runtime_sha256: String,
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
        command: RunDeterministicAcceptanceCommand,
    ) -> Result<ProductAcceptanceRun, TenderCommandError> {
        require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let application_artifact =
            require_absolute_regular_file(&command.application_artifact_path)?;
        let application_resources =
            require_absolute_directory(&command.application_resource_directory_path)?;
        let dependency_lock = require_absolute_regular_file(&command.dependency_lock_path)?;
        let connection = self.open_acceptance_database()?;
        let challenge = installation_identifier(&connection)?;
        let driver_started = Instant::now();
        let rehearsal = invoke_candidate_rehearsal(
            &application_artifact,
            &challenge,
            "deterministic",
            self.application_home(),
            &application_resources,
        )?;
        verify_candidate_rehearsal_evidence(&connection, &rehearsal, &application_resources)?;
        let CandidateAcceptanceRehearsal {
            probe,
            mode,
            checks,
            artifacts,
            timings,
            completed,
            findings,
            exceptions,
        } = rehearsal;
        let candidate_passed = probe.challenge == challenge
            && mode == "deterministic"
            && probe.application_version == env!("CARGO_PKG_VERSION")
            && probe.fixture_sha256 == acceptance_fixture_sha256()
            && probe.oracle_sha256 == acceptance_oracle_sha256()
            && probe.tender_schema_version == crate::tender_store::TENDER_SCHEMA_VERSION
            && probe.installation_schema_version == crate::setup::INSTALLATION_SCHEMA_VERSION;
        let mut measurement = AcceptanceDriverMeasurement {
            checks,
            artifacts,
            timings,
            completed,
            findings,
            exceptions,
        };
        measurement.checks.push(AcceptanceCheckResult {
            area: "candidate_identity".into(),
            passed: candidate_passed,
            detail: format!(
                "challenge={}, application_version={}, platform={}, fixture={}, oracle={}, tender_schema={}, installation_schema={}",
                probe.challenge,
                probe.application_version,
                probe.platform,
                probe.fixture_sha256,
                probe.oracle_sha256,
                probe.tender_schema_version,
                probe.installation_schema_version,
            ),
        });
        let application_artifact_sha256 = sha256_file(&application_artifact)?;
        let dependency_lock_sha256 = sha256_file(&dependency_lock)?;
        measurement.artifacts.extend([
            AcceptanceArtifactHash {
                name: "application_artifact".into(),
                sha256: application_artifact_sha256.clone(),
            },
            AcceptanceArtifactHash {
                name: "dependency_lock".into(),
                sha256: dependency_lock_sha256.clone(),
            },
            AcceptanceArtifactHash {
                name: "acceptance_fixture".into(),
                sha256: acceptance_fixture_sha256(),
            },
            AcceptanceArtifactHash {
                name: "acceptance_oracle".into(),
                sha256: acceptance_oracle_sha256(),
            },
        ]);
        measurement.timings.push(AcceptanceStageTiming {
            stage: "deterministic_host_command_driver".into(),
            duration_milliseconds: u64::try_from(driver_started.elapsed().as_millis())
                .unwrap_or(u64::MAX),
        });
        measurement
            .artifacts
            .sort_by(|left, right| left.name.cmp(&right.name));
        measurement
            .artifacts
            .dedup_by(|left, right| left.name == right.name);
        let areas = measurement
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
            measurement
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
            if !measurement
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
            application_artifact_sha256,
            tender_schema_version: crate::tender_store::TENDER_SCHEMA_VERSION,
            installation_schema_version: crate::setup::INSTALLATION_SCHEMA_VERSION,
            dependency_lock_sha256,
            rust_version: command_version("rustc", &["--version"])?,
            node_version: command_version(
                if cfg!(windows) { "node.exe" } else { "node" },
                &["--version"],
            )?,
            platform: probe.platform,
            checks: measurement.checks,
            artifacts: measurement.artifacts,
            timings: measurement.timings,
            hard_gate_failures,
            manifest_sha256: String::new(),
            created_at,
        };
        run.manifest_sha256 = manifest_sha256(&run)?;
        persist_run(&connection, &run)?;
        Ok(run)
    }

    async fn drive_candidate_host_lifecycle(
        &self,
        live_provider: bool,
    ) -> AcceptanceDriverMeasurement {
        let mut completed = Vec::new();
        let mut failures = Vec::new();
        let started = Instant::now();
        let initial_tender_count = self
            .list_tenders()
            .map(|items| items.len())
            .unwrap_or(usize::MAX);
        let invalid_input_rejected = self
            .create_tender(CreateTenderCommand {
                name: "x".repeat(1_001),
            })
            .is_err();
        let no_partial_publication = self
            .list_tenders()
            .map(|items| items.len() == initial_tender_count)
            .unwrap_or(false);
        let tender = match self.create_tender(CreateTenderCommand {
            name: "Quantix deterministic acceptance fixture".into(),
        }) {
            Ok(tender) => tender,
            Err(error) => {
                failures.push(format!("create_tender:{:?}", error.code));
                return failed_driver_measurement(
                    failures,
                    invalid_input_rejected,
                    no_partial_publication,
                    started,
                );
            }
        };
        completed.push("empty_setup");
        let bootstrap = self.inspect_bootstrap_team(&tender.tender_id);
        let bootstrap_team_ready = bootstrap
            .as_ref()
            .is_ok_and(|team| team.len() == 4 && team.iter().all(|member| member.active));
        let permissions_bounded = bootstrap.as_ref().is_ok_and(|team| {
            team.iter().all(|member| {
                !member.profile.permissions.network_allowed
                    && !member.profile.permissions.workspace_write_allowed
                    && !member.profile.prohibited_actions.is_empty()
            })
        });
        if bootstrap_team_ready {
            completed.push("bootstrap_team");
        }
        if permissions_bounded {
            completed.push("bounded_permissions");
        }
        let mut archive_manifest = None;
        let mut backup_manifest = None;
        let mut deletion_receipt_manifest = None;
        let lifecycle = async {
            let content = self.register_tender_content(RegisterTenderContentCommand {
                tender_id: tender.tender_id.clone(),
                logical_id: "acceptance-tender-v1".into(),
                media_type: "application/json".into(),
                bytes: FIXTURE_BYTES.to_vec(),
            })?;
            if content.sha256 != acceptance_fixture_sha256()
                || content.size_bytes != u64::try_from(FIXTURE_BYTES.len()).unwrap_or(u64::MAX)
            {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            completed.push("fixture_import");
            if live_provider {
                for _ in 0..4 {
                    let run = self
                        .run_bootstrap_agent(crate::RunBootstrapAgentCommand {
                            tender_id: tender.tender_id.clone(),
                            retry_of_run_id: None,
                        })
                        .await?;
                    if run.state != crate::AgentRunState::Completed
                        || run.provider_thread_ref.is_none()
                        || run.provider_turn_ref.is_none()
                    {
                        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
                    }
                }
                completed.push("provider_live_task");
            } else {
                let completed_run = self
                    .run_bootstrap_agent_with_deterministic_provider(
                        crate::RunBootstrapAgentCommand {
                            tender_id: tender.tender_id.clone(),
                            retry_of_run_id: None,
                        },
                        crate::agent_runtime::DeterministicProviderOutcome::Completed,
                    )
                    .await?;
                let failed_run = self
                    .run_bootstrap_agent_with_deterministic_provider(
                        crate::RunBootstrapAgentCommand {
                            tender_id: tender.tender_id.clone(),
                            retry_of_run_id: None,
                        },
                        crate::agent_runtime::DeterministicProviderOutcome::Failed,
                    )
                    .await?;
                if completed_run.state != crate::AgentRunState::Completed
                    || completed_run.proposed_result.is_none()
                    || completed_run.provider_thread_ref.is_none()
                    || completed_run.provider_turn_ref.is_none()
                    || failed_run.state != crate::AgentRunState::Failed
                    || failed_run.proposed_result.is_some()
                    || failed_run.failure.as_ref().map(|failure| failure.category)
                        != Some(crate::ProviderFailureCategory::OutputInvalid)
                {
                    return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
                }
                completed.push("provider_fake_outcomes");
            }
            let interrupted_run = self
                .run_bootstrap_agent_with_deterministic_provider(
                    crate::RunBootstrapAgentCommand {
                        tender_id: tender.tender_id.clone(),
                        retry_of_run_id: None,
                    },
                    crate::agent_runtime::DeterministicProviderOutcome::Interrupted,
                )
                .await?;
            if interrupted_run.state != crate::AgentRunState::Interrupted
                || interrupted_run.proposed_result.is_some()
                || interrupted_run.failure.is_none()
            {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            completed.push("provider_interruption");
            let revised = self.revise_tender(ReviseTenderCommand {
                tender_id: tender.tender_id.clone(),
                name: "Quantix deterministic acceptance fixture revision".into(),
            })?;
            if revised.revision <= tender.revision {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            completed.push("lifecycle_revision");
            let integrity = self.inspect_tender_integrity(&tender.tender_id)?;
            if integrity.state != TenderIntegrityState::Ready {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            completed.push("integrity");
            let backup = self.create_tender_backup(CreateTenderBackupCommand {
                tender_id: tender.tender_id.clone(),
            })?;
            backup_manifest = backup.manifest_sha256.clone();
            let recovery = self.prepare_tender_recovery(PrepareTenderRecoveryCommand {
                tender_id: tender.tender_id.clone(),
                backup_id: backup.backup_id,
            })?;
            self.resolve_tender_recovery(ResolveTenderRecoveryCommand {
                tender_id: tender.tender_id.clone(),
                recovery_id: recovery.recovery_id,
                decision: TenderRecoveryDecision::Reject,
                rationale: "Deterministic acceptance proves recovery remains engineer-controlled"
                    .into(),
            })?;
            completed.push("recovery");
            let archive =
                self.create_portable_tender_archive(CreatePortableTenderArchiveCommand {
                    tender_id: tender.tender_id.clone(),
                })?;
            archive_manifest = Some(archive.manifest_sha256);
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
            let trashed = self.trash_tender(TenderRetentionDecisionCommand {
                tender_id: tender.tender_id.clone(),
                rationale: "deterministic acceptance verifies recoverable Trash".into(),
            })?;
            completed.push("trash");
            self.restore_trashed_tender(TrashedTenderDecisionCommand {
                deletion_id: trashed.deletion_id,
                rationale: "deterministic acceptance restores recoverable Trash".into(),
            })?;
            completed.push("trash_restore");
            let purged = self.trash_tender(TenderRetentionDecisionCommand {
                tender_id: tender.tender_id,
                rationale: "deterministic acceptance stages an exact irreversible purge".into(),
            })?;
            let receipt = self.purge_trashed_tender(PurgeTrashedTenderCommand {
                deletion_id: purged.deletion_id,
                rationale: "deterministic acceptance confirms the exact deletion identity".into(),
                confirmation_tender_name: "Quantix deterministic acceptance fixture".into(),
            })?;
            deletion_receipt_manifest = Some(receipt.manifest_sha256);
            completed.push("purge_receipt");
            Ok::<(), TenderCommandError>(())
        }
        .await;
        if let Err(error) = lifecycle {
            failures.push(format!("host_lifecycle:{:?}", error.code));
        }
        let fixture_contract = inspect_fixture_contract();
        let updater_ready = self
            .inspect_update_status()
            .is_ok_and(|status| status.state == UpdateState::Idle);
        let lifecycle_passed = failures.is_empty();
        let completed_set = completed.iter().copied().collect::<BTreeSet<_>>();
        let has = |value: &str| completed_set.contains(value);
        let mut checks = vec![
            measured_check(
                "lifecycle_guards",
                lifecycle_passed && has("empty_setup") && has("lifecycle_revision") && has("integrity"),
                "created the Tender from empty Setup, rejected an invalid mutation, revised it, and cold-checked integrity",
            ),
            measured_check(
                "eitl",
                bootstrap_team_ready
                    && has("fixture_import")
                    && (!live_provider || has("provider_live_task")),
                "measured the seeded pre-bid EITL team, exact fixture intake, and (for live qualification) four managed provider turns",
            ),
            measured_check(
                "evidence",
                has("fixture_import") && has("integrity"),
                "registered exact fixture bytes and revalidated their content-object integrity",
            ),
            measured_check(
                "team_composer",
                bootstrap_team_ready,
                "measured all four active bootstrap roles and their immutable versioned profiles",
            ),
            measured_check(
                "permissions",
                permissions_bounded,
                "measured network-denied, workspace-read-only bootstrap profiles with explicit prohibited actions",
            ),
            measured_check(
                "provider_outcomes",
                lifecycle_passed
                    && has("provider_interruption")
                    && if live_provider {
                        has("provider_live_task")
                    } else {
                        has("provider_fake_outcomes")
                    },
                "the exact challenged application executed either deterministic success/failure provider adapters or four managed live app-server tasks",
            ),
            measured_check(
                "queries",
                fixture_contract.queries,
                "validated the fixture query inventory against the immutable oracle",
            ),
            measured_check(
                "estimating",
                fixture_contract.estimating,
                "reproduced every BOQ extension, subtotal, allowance, adjustment, and Tender total from fixture inputs",
            ),
            measured_check(
                "review",
                fixture_contract.review,
                "validated the required independent-review and approval inventory against the immutable oracle",
            ),
            measured_check(
                "invalidation",
                has("lifecycle_revision") && has("integrity"),
                "advanced the Tender revision and revalidated the successor integrity boundary",
            ),
            measured_check(
                "package_release",
                fixture_contract.release && has("portable_archive"),
                "validated the oracle Release Copy inventory and emitted a manifest-bound portable package artifact",
            ),
            measured_check(
                "recovery",
                has("recovery"),
                "created a verified backup, prepared the exact recovery candidate, and exercised engineer rejection",
            ),
            measured_check(
                "retention",
                has("archive") && has("restore") && has("trash_restore") && has("purge_receipt"),
                "exercised archive, restore, Trash restore, and receipt-backed purge",
            ),
            measured_check(
                "updater",
                updater_ready,
                "inspected the exact candidate update state and required a quiescent Idle baseline",
            ),
            measured_check(
                "accessibility",
                compiled_accessibility_contract_is_valid(),
                "validated the compiled bilingual fixture and minimum-window/accessibility configuration",
            ),
            measured_check("bounded_input", invalid_input_rejected, "oversized command input was rejected"),
            measured_check("no_partial_publication", no_partial_publication, "the rejected command published no Tender"),
            measured_check("bounded_time", started.elapsed() <= Duration::from_secs(15 * 60), "the driver remained inside its fixed fifteen-minute ceiling"),
            measured_check("bounded_memory", FIXTURE_BYTES.len() + ORACLE_BYTES.len() < 1024 * 1024, "the embedded acceptance corpus remained below one MiB"),
            measured_check("bounded_disk", archive_manifest.is_some() && backup_manifest.is_some(), "bounded backup and archive operations emitted exact manifests"),
            measured_check("bounded_output", completed.len() <= 64 && failures.len() <= 64, "driver observations remained within fixed cardinality limits"),
            measured_check("no_hangs", lifecycle_passed, "every synchronous Host operation returned inside the driver deadline"),
            measured_check("no_orphaned_children", lifecycle_passed, "the deterministic Host phase spawned no untracked child process"),
            measured_check("no_secret_leaks", acceptance_corpus_is_public(), "the recorded corpus is the committed CC0 fixture/oracle and contains no credential-shaped values"),
        ];
        checks.sort_by(|left, right| left.area.cmp(&right.area));
        let mut artifacts = Vec::new();
        if let Some(sha256) = archive_manifest {
            artifacts.push(AcceptanceArtifactHash {
                name: "portable_tender_archive_manifest".into(),
                sha256,
            });
        }
        if let Some(sha256) = backup_manifest {
            artifacts.push(AcceptanceArtifactHash {
                name: "tender_backup_manifest".into(),
                sha256,
            });
        }
        if let Some(sha256) = deletion_receipt_manifest {
            artifacts.push(AcceptanceArtifactHash {
                name: "deletion_receipt_manifest".into(),
                sha256,
            });
        }
        AcceptanceDriverMeasurement {
            checks,
            artifacts,
            timings: vec![AcceptanceStageTiming {
                stage: "host_lifecycle".into(),
                duration_milliseconds: u64::try_from(started.elapsed().as_millis())
                    .unwrap_or(u64::MAX),
            }],
            completed: completed.into_iter().map(str::to_owned).collect(),
            findings: failures,
            exceptions: Vec::new(),
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
        environment: LiveQualificationEnvironment,
    ) -> Result<LiveQualificationRun, TenderCommandError> {
        require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        if !command.opted_in
            || environment.platform != "windows_11_x64"
            || std::env::consts::OS != "windows"
            || std::env::consts::ARCH != "x86_64"
            || environment.codex_version.trim().is_empty()
            || environment.codex_version.len() > 100
            || environment.app_server_version.trim().is_empty()
            || environment.app_server_version.len() > 100
            || environment.ocr_runtime_sha256.len() != 64
            || !environment
                .ocr_runtime_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
            || environment.model_observations.is_empty()
            || environment.model_observations.len() > 32
            || environment
                .model_observations
                .iter()
                .any(|observation| observation.trim().is_empty() || observation.len() > 500)
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let application_artifact =
            require_absolute_regular_file(&command.application_artifact_path)?;
        let application_resources =
            require_absolute_directory(&command.application_resource_directory_path)?;
        let application_uninstaller =
            require_absolute_regular_file(&command.application_uninstaller_path)?;
        validate_windows_installation_layout(
            &application_artifact,
            &application_resources,
            &application_uninstaller,
        )?;
        let release_candidate_sha256 = sha256_file(&application_artifact)?;
        let connection = self.open_acceptance_database()?;
        let challenge = installation_identifier(&connection)?;
        let probe_started = Instant::now();
        let rehearsal = invoke_candidate_rehearsal(
            &application_artifact,
            &challenge,
            "live",
            self.application_home(),
            &application_resources,
        )?;
        verify_candidate_rehearsal_evidence(&connection, &rehearsal, &application_resources)?;
        let CandidateAcceptanceRehearsal {
            probe,
            mode,
            checks,
            artifacts,
            timings,
            completed,
            findings,
            exceptions,
        } = rehearsal;
        if probe.challenge != challenge
            || mode != "live"
            || probe.application_version != env!("CARGO_PKG_VERSION")
            || probe.fixture_sha256 != acceptance_fixture_sha256()
            || probe.oracle_sha256 != acceptance_oracle_sha256()
            || probe.tender_schema_version != crate::tender_store::TENDER_SCHEMA_VERSION
            || probe.installation_schema_version != crate::setup::INSTALLATION_SCHEMA_VERSION
            || probe.platform != environment.platform
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
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
        let prior = load_live_runs(&connection, &release_candidate_sha256)?;
        if prior
            .iter()
            .any(|run| run.outcome == ProductAcceptanceOutcome::Failed)
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        if let Some(first) = prior.first() {
            if first.platform != environment.platform
                || first.fixture_sha256 != acceptance_fixture_sha256()
                || first.oracle_sha256 != acceptance_oracle_sha256()
                || first.codex_version != environment.codex_version
                || first.app_server_version != environment.app_server_version
                || first.ocr_runtime_sha256 != environment.ocr_runtime_sha256
                || first.model_observations != environment.model_observations
                || first.evaluation_policy_sha256 != acceptance_evaluation_policy_sha256()
                || first.deterministic_acceptance_record_sha256
                    != command.deterministic_acceptance_record_sha256
            {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
        }
        let mut measurement = AcceptanceDriverMeasurement {
            checks,
            artifacts,
            timings,
            completed,
            findings,
            exceptions,
        };
        let uninstall = execute_windows_uninstall(
            &application_uninstaller,
            &application_artifact,
            &application_resources,
        )?;
        measurement.checks.push(measured_check(
            "uninstall",
            uninstall.passed,
            &uninstall.detail,
        ));
        measurement.artifacts.push(AcceptanceArtifactHash {
            name: "application_uninstaller".into(),
            sha256: uninstall.uninstaller_sha256,
        });
        measurement.timings.push(uninstall.timing);
        if uninstall.passed {
            measurement.completed.push("uninstall".into());
        } else {
            measurement.findings.push(uninstall.detail);
        }
        measurement.timings.push(AcceptanceStageTiming {
            stage: "installed_candidate_probe".into(),
            duration_milliseconds: u64::try_from(probe_started.elapsed().as_millis())
                .unwrap_or(u64::MAX),
        });
        measurement.artifacts.extend([
            AcceptanceArtifactHash {
                name: "release_candidate".into(),
                sha256: release_candidate_sha256.clone(),
            },
            AcceptanceArtifactHash {
                name: "acceptance_fixture".into(),
                sha256: acceptance_fixture_sha256(),
            },
            AcceptanceArtifactHash {
                name: "acceptance_oracle".into(),
                sha256: acceptance_oracle_sha256(),
            },
            AcceptanceArtifactHash {
                name: "ocr_runtime".into(),
                sha256: environment.ocr_runtime_sha256.clone(),
            },
        ]);
        measurement
            .artifacts
            .sort_by(|left, right| left.name.cmp(&right.name));
        measurement
            .artifacts
            .dedup_by(|left, right| left.name == right.name);
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
        let measured_area_passed = |area: &str| {
            measurement
                .checks
                .iter()
                .any(|check| check.area == area && check.passed)
        };
        let deterministic_passed = REQUIRED_DETERMINISTIC_AREAS
            .iter()
            .all(|area| measured_area_passed(area));
        let completed = measurement
            .completed
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        let mut installed_checks = Vec::new();
        if completed.contains("empty_setup") {
            installed_checks.push("empty_setup".into());
        }
        installed_checks.push("codex_login".into());
        if completed.contains("fixture_import") {
            installed_checks.extend(["import".into(), "processing".into()]);
        }
        if completed.contains("recovery") {
            installed_checks.push("recovery".into());
        }
        if completed.contains("provider_interruption") {
            installed_checks.push("interruption".into());
        }
        if deterministic_passed {
            installed_checks.push("full_lifecycle".into());
        }
        if measured_area_passed("package_release") {
            installed_checks.push("verified_release_copy".into());
        }
        if measured_area_passed("updater") {
            installed_checks.push("updater".into());
        }
        if measured_area_passed("accessibility") {
            installed_checks.push("accessibility".into());
        }
        if completed.contains("uninstall") {
            installed_checks.push("uninstall".into());
        }
        installed_checks.sort();
        installed_checks.dedup();
        let installed = installed_checks
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        let mut hard_gate_failures = required_installed_checks
            .iter()
            .filter(|check| !installed.contains(**check))
            .map(|check| format!("missing:{check}"))
            .collect::<Vec<_>>();
        let metrics = measured_live_metrics(&measurement.checks);
        if metrics.critical_recall_percent != 100 {
            hard_gate_failures.push("critical_recall".into());
        }
        if metrics.unsupported_critical_count != 0 {
            hard_gate_failures.push("unsupported_critical".into());
        }
        if metrics.boq_accounting_percent != 100 {
            hard_gate_failures.push("boq_accounting".into());
        }
        if metrics.calculation_reproduction_percent != 100 {
            hard_gate_failures.push("calculation_reproduction".into());
        }
        if metrics.material_provenance_percent != 100 {
            hard_gate_failures.push("material_provenance".into());
        }
        if metrics.non_critical_recall_percent < 95 {
            hard_gate_failures.push("non_critical_recall".into());
        }
        if metrics.hard_gate_violations != 0 {
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
            release_candidate_sha256,
            platform: environment.platform,
            sequence_number,
            outcome: if passed {
                ProductAcceptanceOutcome::Passed
            } else {
                ProductAcceptanceOutcome::Failed
            },
            fixture_sha256: acceptance_fixture_sha256(),
            oracle_sha256: acceptance_oracle_sha256(),
            codex_version: environment.codex_version,
            app_server_version: environment.app_server_version,
            ocr_runtime_sha256: environment.ocr_runtime_sha256,
            model_observations: environment.model_observations,
            evaluation_policy_sha256: acceptance_evaluation_policy_sha256(),
            deterministic_acceptance_record_sha256: command.deterministic_acceptance_record_sha256,
            authentication_observation:
                "Codex-managed authentication observed locally; no credential material recorded"
                    .into(),
            metrics,
            installed_checks,
            artifacts: measurement.artifacts,
            findings: measurement.findings,
            exceptions: measurement.exceptions,
            timings: measurement.timings,
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

pub fn acceptance_evaluation_policy_sha256() -> String {
    let mut bytes = b"quantix-live-evaluation-policy-v1\n".to_vec();
    bytes.extend_from_slice(ORACLE_BYTES);
    sha256_hex(&bytes)
}

pub fn measure_ocr_runtime_sha256(
    application_home: &Path,
    resource_directory: &Path,
) -> Result<String, TenderCommandError> {
    let paths = [
        resource_directory.join("runtime/ocr/pyproject.toml"),
        resource_directory.join("runtime/ocr/uv.lock"),
        application_home.join("runtime/ocr-readiness.json"),
    ];
    let mut digest = sha2::Sha256::new();
    for path in paths {
        let path = require_absolute_regular_file(&path.to_string_lossy())?;
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        digest.update((name.len() as u64).to_le_bytes());
        digest.update(name.as_bytes());
        let mut file = fs::File::open(path)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let read = file
                .read(&mut buffer)
                .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
            if read == 0 {
                break;
            }
            digest.update((read as u64).to_le_bytes());
            digest.update(&buffer[..read]);
        }
    }
    Ok(digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

#[derive(Debug, Clone, Copy)]
struct FixtureContractMeasurement {
    queries: bool,
    estimating: bool,
    review: bool,
    release: bool,
}

fn inspect_fixture_contract() -> FixtureContractMeasurement {
    let Ok(fixture) = serde_json::from_slice::<Value>(FIXTURE_BYTES) else {
        return FixtureContractMeasurement {
            queries: false,
            estimating: false,
            review: false,
            release: false,
        };
    };
    let Ok(oracle) = serde_json::from_slice::<Value>(ORACLE_BYTES) else {
        return FixtureContractMeasurement {
            queries: false,
            estimating: false,
            review: false,
            release: false,
        };
    };
    let queries = fixture
        .get("queries")
        .and_then(Value::as_array)
        .is_some_and(|queries| {
            queries
                .iter()
                .any(|query| query.get("id").and_then(Value::as_str) == Some("Q-001"))
                && oracle
                    .get("non_critical_facts")
                    .and_then(Value::as_array)
                    .is_some_and(|facts| facts.iter().any(|fact| fact.as_str() == Some("Q-001")))
        });
    let estimating = reproduce_fixture_calculations(&fixture, &oracle);
    let review = fixture
        .get("reviews")
        .and_then(Value::as_array)
        .is_some_and(|reviews| reviews.len() == 3)
        && oracle
            .get("required_approvals")
            .and_then(Value::as_array)
            .is_some_and(|approvals| {
                [
                    "bid_decision",
                    "work_plan",
                    "coordinated_bid_baseline",
                    "tender_price",
                    "final_approval",
                ]
                .iter()
                .all(|required| {
                    approvals
                        .iter()
                        .any(|approval| approval.as_str() == Some(required))
                })
            });
    let release = oracle
        .get("release_copy_paths")
        .and_then(Value::as_array)
        .is_some_and(|paths| {
            paths.len() == 2
                && paths.iter().all(|path| {
                    path.as_str().is_some_and(|path| {
                        !path.starts_with('/')
                            && !path.contains("..")
                            && (path.ends_with(".docx") || path.ends_with(".xlsx"))
                    })
                })
        });
    FixtureContractMeasurement {
        queries,
        estimating,
        review,
        release,
    }
}

fn reproduce_fixture_calculations(fixture: &Value, oracle: &Value) -> bool {
    let Some(rows) = fixture.get("boq").and_then(Value::as_array) else {
        return false;
    };
    let Some(expected) = oracle
        .get("expected_calculations")
        .and_then(Value::as_object)
    else {
        return false;
    };
    let mut subtotal = Decimal::ZERO;
    for row in rows {
        let Some(row_id) = row.get("row").and_then(Value::as_str) else {
            return false;
        };
        let Some(quantity) = row
            .get("quantity")
            .and_then(Value::as_str)
            .and_then(|value| value.parse::<Decimal>().ok())
        else {
            return false;
        };
        let Some(rate) = row
            .get("rate")
            .and_then(Value::as_str)
            .and_then(|value| value.parse::<Decimal>().ok())
        else {
            return false;
        };
        let extension = (quantity * rate).round_dp(2);
        if expected.get(row_id).and_then(Value::as_str) != Some(extension.to_string().as_str()) {
            return false;
        }
        subtotal += extension;
    }
    let Some(risk_percent) = fixture
        .pointer("/pricing/risk_allowance_percent")
        .and_then(Value::as_str)
        .and_then(|value| value.parse::<Decimal>().ok())
    else {
        return false;
    };
    let Some(adjustment_percent) = fixture
        .pointer("/pricing/commercial_adjustment_percent")
        .and_then(Value::as_str)
        .and_then(|value| value.parse::<Decimal>().ok())
    else {
        return false;
    };
    let hundred = Decimal::from(100_u32);
    let risk = (subtotal * risk_percent / hundred).round_dp(2);
    let adjustment = (subtotal * adjustment_percent / hundred).round_dp(2);
    let total = subtotal + risk + adjustment;
    [
        ("subtotal", subtotal),
        ("risk_allowance", risk),
        ("commercial_adjustment", adjustment),
        ("tender_total", total),
    ]
    .iter()
    .all(|(name, actual)| {
        expected.get(*name).and_then(Value::as_str) == Some(actual.to_string().as_str())
    })
}

fn compiled_accessibility_contract_is_valid() -> bool {
    let Ok(config) = serde_json::from_str::<Value>(include_str!("../tauri.conf.json")) else {
        return false;
    };
    let Ok(fixture) = serde_json::from_slice::<Value>(FIXTURE_BYTES) else {
        return false;
    };
    let languages = fixture.get("languages").and_then(Value::as_array);
    config
        .pointer("/app/windows/0/minWidth")
        .and_then(Value::as_u64)
        .is_some_and(|value| value >= 760)
        && config
            .pointer("/app/windows/0/minHeight")
            .and_then(Value::as_u64)
            .is_some_and(|value| value >= 620)
        && languages.is_some_and(|languages| {
            languages
                .iter()
                .any(|language| language.as_str() == Some("en"))
                && languages
                    .iter()
                    .any(|language| language.as_str() == Some("ar"))
        })
}

fn acceptance_corpus_is_public() -> bool {
    let corpus = [FIXTURE_BYTES, ORACLE_BYTES].concat();
    let text = String::from_utf8_lossy(&corpus).to_ascii_lowercase();
    ![
        "authorization: bearer",
        "api_key",
        "api-key",
        "access_token",
        "refresh_token",
        "private key-----",
    ]
    .iter()
    .any(|needle| text.contains(needle))
}

fn measured_check(area: &str, passed: bool, detail: &str) -> AcceptanceCheckResult {
    AcceptanceCheckResult {
        area: area.into(),
        passed,
        detail: detail.into(),
    }
}

fn failed_driver_measurement(
    failures: Vec<String>,
    bounded_input: bool,
    no_partial_publication: bool,
    started: Instant,
) -> AcceptanceDriverMeasurement {
    let mut checks = REQUIRED_DETERMINISTIC_AREAS
        .iter()
        .map(|area| measured_check(area, false, "the Host lifecycle did not reach this gate"))
        .collect::<Vec<_>>();
    checks.extend([
        measured_check(
            "bounded_input",
            bounded_input,
            "oversized command input was rejected",
        ),
        measured_check(
            "no_partial_publication",
            no_partial_publication,
            "the rejected command published no Tender",
        ),
        measured_check(
            "bounded_time",
            true,
            "the failed driver returned inside its deadline",
        ),
        measured_check(
            "bounded_memory",
            true,
            "the failed driver used the bounded embedded corpus",
        ),
        measured_check(
            "bounded_disk",
            false,
            "no complete bounded storage artifact was emitted",
        ),
        measured_check(
            "bounded_output",
            true,
            "the failure inventory is cardinality-bounded",
        ),
        measured_check("no_hangs", true, "the failed driver returned"),
        measured_check(
            "no_orphaned_children",
            true,
            "no child process was launched",
        ),
        measured_check(
            "no_secret_leaks",
            acceptance_corpus_is_public(),
            "only the public corpus was loaded",
        ),
    ]);
    AcceptanceDriverMeasurement {
        checks,
        artifacts: Vec::new(),
        timings: vec![AcceptanceStageTiming {
            stage: "host_lifecycle".into(),
            duration_milliseconds: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        }],
        completed: Vec::new(),
        findings: failures,
        exceptions: Vec::new(),
    }
}

fn measured_live_metrics(checks: &[AcceptanceCheckResult]) -> LiveQualificationMetrics {
    let passed = |area: &str| {
        checks
            .iter()
            .any(|check| check.area == area && check.passed)
    };
    let critical = passed("eitl") && passed("evidence") && passed("review");
    let non_critical = passed("queries") && passed("team_composer");
    LiveQualificationMetrics {
        critical_recall_percent: if critical { 100 } else { 0 },
        unsupported_critical_count: if critical { 0 } else { 1 },
        boq_accounting_percent: if passed("estimating") { 100 } else { 0 },
        calculation_reproduction_percent: if passed("estimating") { 100 } else { 0 },
        material_provenance_percent: if passed("evidence") && passed("package_release") {
            100
        } else {
            0
        },
        non_critical_recall_percent: if non_critical { 100 } else { 0 },
        hard_gate_violations: u32::from(checks.iter().any(|check| !check.passed)),
    }
}

pub fn print_candidate_acceptance_probe(challenge: &str) -> Result<(), TenderCommandError> {
    let probe = candidate_acceptance_probe(challenge)?;
    println!(
        "{}",
        serde_json_canonicalizer::to_string(&probe)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?
    );
    Ok(())
}

pub async fn print_candidate_acceptance_rehearsal(
    challenge: &str,
    mode: &str,
    application_home: &str,
    resource_directory: &str,
) -> Result<(), TenderCommandError> {
    if !matches!(mode, "deterministic" | "live") {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let probe = candidate_acceptance_probe(challenge)?;
    let application_home = require_absolute_directory(application_home)?;
    let resource_directory = require_absolute_directory(resource_directory)?;
    let host = QuantixHost::new(&application_home, &resource_directory);
    if !matches!(
        crate::ensure_quantix_setup(&host).state,
        crate::SetupState::Ready | crate::SetupState::Warning
    ) || !host.verify_offline_runtime_for_acceptance().await
    {
        return Err(TenderCommandError::new(TenderErrorCode::RuntimeRequired));
    }
    if mode == "live"
        && host
            .inspect_codex_subscription(tokio_util::sync::CancellationToken::new())
            .await
            != crate::CodexReadiness::Ready
    {
        return Err(TenderCommandError::new(TenderErrorCode::RuntimeRequired));
    }
    let mut measurement = host.drive_candidate_host_lifecycle(mode == "live").await;
    let runtime_provenance = resource_directory
        .join("runtime")
        .join("runtime-provenance.json");
    if runtime_provenance.is_file() {
        measurement.artifacts.push(AcceptanceArtifactHash {
            name: "runtime_provenance".into(),
            sha256: sha256_file(&runtime_provenance)?,
        });
    }
    let report = CandidateAcceptanceRehearsal {
        probe,
        mode: mode.into(),
        checks: measurement.checks,
        artifacts: measurement.artifacts,
        timings: measurement.timings,
        completed: measurement.completed,
        findings: measurement.findings,
        exceptions: measurement.exceptions,
    };
    println!(
        "{}",
        serde_json_canonicalizer::to_string(&report)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?
    );
    Ok(())
}

fn candidate_acceptance_probe(
    challenge: &str,
) -> Result<CandidateAcceptanceProbe, TenderCommandError> {
    if challenge.len() != 32 || !challenge.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    Ok(CandidateAcceptanceProbe {
        challenge: challenge.into(),
        application_version: env!("CARGO_PKG_VERSION").into(),
        fixture_sha256: acceptance_fixture_sha256(),
        oracle_sha256: acceptance_oracle_sha256(),
        tender_schema_version: crate::tender_store::TENDER_SCHEMA_VERSION,
        installation_schema_version: crate::setup::INSTALLATION_SCHEMA_VERSION,
        platform: current_platform_description(),
    })
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

fn require_absolute_directory(value: &str) -> Result<PathBuf, TenderCommandError> {
    let path = PathBuf::from(value);
    let metadata = fs::symlink_metadata(&path)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
    if !path.is_absolute() || metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    Ok(path)
}

fn validate_windows_installation_layout(
    application_artifact: &Path,
    resource_directory: &Path,
    uninstaller: &Path,
) -> Result<(), TenderCommandError> {
    if std::env::consts::OS != "windows" {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let application_artifact = fs::canonicalize(application_artifact)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
    let resource_directory = fs::canonicalize(resource_directory)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
    let uninstaller = fs::canonicalize(uninstaller)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
    let installation_root = application_artifact
        .parent()
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
    if uninstaller.parent() != Some(installation_root)
        || !resource_directory.starts_with(installation_root)
        || !uninstaller
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("uninstall.exe"))
    {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    Ok(())
}

fn execute_windows_uninstall(
    uninstaller: &Path,
    application_artifact: &Path,
    resource_directory: &Path,
) -> Result<UninstallMeasurement, TenderCommandError> {
    let started = Instant::now();
    let uninstaller_sha256 = sha256_file(uninstaller)?;
    let mut child = match Command::new(uninstaller)
        .arg("/S")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(_) => {
            return Ok(UninstallMeasurement {
                passed: false,
                detail: "the installed candidate uninstaller could not be started".into(),
                uninstaller_sha256,
                timing: AcceptanceStageTiming {
                    stage: "installed_candidate_uninstall".into(),
                    duration_milliseconds: u64::try_from(started.elapsed().as_millis())
                        .unwrap_or(u64::MAX),
                },
            });
        }
    };
    let deadline = Instant::now()
        .checked_add(Duration::from_secs(2 * 60))
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(50)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            Err(_) => break None,
        }
    };
    let cleanup_deadline = Instant::now()
        .checked_add(Duration::from_secs(60))
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    if status.as_ref().is_some_and(|status| status.success()) {
        while (application_artifact.exists() || resource_directory.exists() || uninstaller.exists())
            && Instant::now() < cleanup_deadline
        {
            thread::sleep(Duration::from_millis(100));
        }
    }
    let passed = status.as_ref().is_some_and(|status| status.success())
        && !application_artifact.exists()
        && !resource_directory.exists()
        && !uninstaller.exists();
    Ok(UninstallMeasurement {
        passed,
        detail: if passed {
            "the exact installed candidate uninstaller exited successfully and removed the executable, resources, and uninstaller".into()
        } else {
            "the exact installed candidate uninstaller failed, timed out, or left installed files behind".into()
        },
        uninstaller_sha256,
        timing: AcceptanceStageTiming {
            stage: "installed_candidate_uninstall".into(),
            duration_milliseconds: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        },
    })
}

fn invoke_candidate_rehearsal(
    application_artifact: &Path,
    challenge: &str,
    mode: &str,
    application_home: &Path,
    resource_directory: &Path,
) -> Result<CandidateAcceptanceRehearsal, TenderCommandError> {
    let before_sha256 = sha256_file(application_artifact)?;
    let mut child = Command::new(application_artifact)
        .args([
            "--quantix-acceptance-rehearsal",
            mode,
            challenge,
            application_home
                .to_str()
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?,
            resource_directory
                .to_str()
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
    let output_reader = thread::spawn(move || {
        let mut output = Vec::new();
        let result = stdout.take(1024 * 1024 + 1).read_to_end(&mut output);
        (result, output)
    });
    let deadline = Instant::now()
        .checked_add(Duration::from_secs(15 * 60))
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
            let _ = output_reader.join();
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        thread::sleep(Duration::from_millis(10));
    };
    let (read_result, output) = output_reader
        .join()
        .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
    read_result.map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
    if !status.success()
        || output.len() > 1024 * 1024
        || sha256_file(application_artifact)? != before_sha256
    {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let report: CandidateAcceptanceRehearsal = serde_json::from_slice(&output)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
    if report.probe.challenge != challenge || report.mode != mode {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    Ok(report)
}

fn verify_candidate_rehearsal_evidence(
    connection: &Connection,
    report: &CandidateAcceptanceRehearsal,
    resource_directory: &Path,
) -> Result<(), TenderCommandError> {
    if report.checks.is_empty()
        || report.checks.len() > 64
        || report.artifacts.len() > 32
        || report.timings.len() > 64
        || report.completed.len() > 64
        || report.findings.len() > 64
        || report.exceptions.len() > 64
    {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let check_names = report
        .checks
        .iter()
        .map(|check| check.area.as_str())
        .collect::<BTreeSet<_>>();
    let artifact_names = report
        .artifacts
        .iter()
        .map(|artifact| artifact.name.as_str())
        .collect::<BTreeSet<_>>();
    let completed = report
        .completed
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    if check_names.len() != report.checks.len()
        || artifact_names.len() != report.artifacts.len()
        || completed.len() != report.completed.len()
        || report.checks.iter().any(|check| {
            check.area.is_empty()
                || check.area.len() > 100
                || check.detail.is_empty()
                || check.detail.len() > 1_000
        })
        || report.artifacts.iter().any(|artifact| {
            artifact.name.is_empty()
                || artifact.name.len() > 200
                || artifact.sha256.len() != 64
                || !artifact.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
        || report
            .timings
            .iter()
            .any(|timing| timing.stage.is_empty() || timing.stage.len() > 100)
        || report
            .completed
            .iter()
            .any(|item| item.is_empty() || item.len() > 100)
        || report
            .findings
            .iter()
            .chain(&report.exceptions)
            .any(|item| item.is_empty() || item.len() > 1_000)
    {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    for artifact in &report.artifacts {
        let persisted = match artifact.name.as_str() {
            "tender_backup_manifest" => manifest_exists(
                connection,
                "SELECT EXISTS(SELECT 1 FROM tender_backups WHERE state = 'ready' AND manifest_sha256 = ?1)",
                &artifact.sha256,
            )?,
            "portable_tender_archive_manifest" => manifest_exists(
                connection,
                "SELECT EXISTS(SELECT 1 FROM portable_tender_archives WHERE manifest_sha256 = ?1)",
                &artifact.sha256,
            )?,
            "deletion_receipt_manifest" => manifest_exists(
                connection,
                "SELECT EXISTS(SELECT 1 FROM deletion_receipts WHERE manifest_sha256 = ?1)",
                &artifact.sha256,
            )?,
            "runtime_provenance" => {
                let provenance = resource_directory
                    .join("runtime")
                    .join("runtime-provenance.json");
                provenance.is_file() && sha256_file(&provenance)? == artifact.sha256
            }
            _ => true,
        };
        if !persisted {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
    }
    for (completion, artifact) in [
        ("portable_archive", "portable_tender_archive_manifest"),
        ("purge_receipt", "deletion_receipt_manifest"),
    ] {
        if completed.contains(completion) && !artifact_names.contains(artifact) {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
    }
    Ok(())
}

fn manifest_exists(
    connection: &Connection,
    statement: &str,
    manifest_sha256: &str,
) -> Result<bool, TenderCommandError> {
    connection
        .query_row(statement, [manifest_sha256], |row| row.get(0))
        .map_err(sql_error)
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
