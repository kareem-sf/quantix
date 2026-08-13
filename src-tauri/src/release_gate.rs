use std::collections::BTreeSet;

use garde::Validate;
use jiff::Timestamp;
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use crate::{
    acceptance::{AcceptanceArtifactHash, ProductAcceptanceOutcome, ProductAcceptanceRun},
    tender_store::{require_setup, TenderCommandError, TenderErrorCode},
    QuantixHost,
};

const REQUIRED_PLATFORMS: [&str; 3] = [
    "windows_11_x64",
    "macos_14_apple_silicon",
    "ubuntu_24_04_x64",
];
const REQUIRED_NATIVE_CHECKS: [&str; 8] = [
    "setup",
    "accessibility",
    "updater",
    "import",
    "processing",
    "interruption_recovery",
    "export",
    "uninstall",
];
const CODEX_APP_SERVER_PRODUCTION_SUPPORTED: bool = false;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, Validate)]
#[ts(export)]
pub struct NativePlatformQualificationEvidence {
    #[garde(length(bytes, min = 1, max = 100))]
    pub platform: String,
    #[garde(length(bytes, min = 64, max = 64), ascii)]
    pub release_candidate_manifest_sha256: String,
    #[garde(length(bytes, min = 64, max = 64), ascii)]
    pub signed_binary_sha256: String,
    #[garde(length(bytes, min = 64, max = 64), ascii)]
    pub dependency_lock_sha256: String,
    #[garde(length(bytes, min = 64, max = 64), ascii)]
    pub codex_binary_sha256: String,
    #[garde(length(bytes, min = 64, max = 64), ascii)]
    pub uv_binary_sha256: String,
    #[garde(length(bytes, min = 64, max = 64), ascii)]
    pub docling_runtime_sha256: String,
    #[garde(length(bytes, min = 64, max = 64), ascii)]
    pub model_assets_sha256: String,
    #[garde(length(bytes, min = 64, max = 64), ascii)]
    pub fixture_sha256: String,
    #[garde(length(min = 1, max = 32), inner(length(bytes, min = 1, max = 100)))]
    pub completed_checks: Vec<String>,
    #[garde(length(bytes, min = 64, max = 64), ascii)]
    pub product_acceptance_run_sha256: String,
    #[garde(length(max = 64), inner(length(bytes, min = 1, max = 1000)))]
    pub findings: Vec<String>,
    #[garde(length(bytes, min = 1, max = 100))]
    pub qualified_by: String,
    #[garde(length(bytes, min = 1, max = 100))]
    pub qualified_at: String,
    #[garde(length(bytes, min = 1, max = 100))]
    pub expires_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct RecordNativePlatformQualificationCommand {
    #[garde(dive)]
    pub evidence: NativePlatformQualificationEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct NativePlatformQualificationRecord {
    pub record_id: String,
    pub evidence: NativePlatformQualificationEvidence,
    pub blockers: Vec<String>,
    pub passed: bool,
    pub manifest_sha256: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, Validate)]
#[ts(export)]
pub struct LicenseDistributionReview {
    #[garde(length(bytes, min = 64, max = 64), ascii)]
    pub inventory_sha256: String,
    #[garde(length(min = 6, max = 16), inner(length(bytes, min = 1, max = 100)))]
    pub reviewed_categories: Vec<String>,
    #[garde(skip)]
    pub passed: bool,
    #[garde(length(max = 64), inner(length(bytes, min = 1, max = 1000)))]
    pub findings: Vec<String>,
    #[garde(length(bytes, min = 1, max = 100))]
    pub reviewed_by: String,
    #[garde(length(bytes, min = 1, max = 100))]
    pub reviewed_at: String,
    #[garde(length(bytes, min = 1, max = 100))]
    pub expires_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, Validate)]
#[ts(export)]
pub struct CodexProductionAssuranceEvidence {
    #[garde(skip)]
    pub production_supported: bool,
    #[garde(length(bytes, min = 1, max = 500))]
    pub evidence_reference: String,
    #[garde(length(bytes, min = 64, max = 64), ascii)]
    pub evidence_sha256: String,
    #[garde(length(bytes, min = 1, max = 100))]
    pub verified_by: String,
    #[garde(length(bytes, min = 1, max = 100))]
    pub verified_at: String,
    #[garde(length(bytes, min = 1, max = 100))]
    pub expires_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, Validate)]
#[ts(export)]
pub struct IntegrationTermsDecision {
    #[garde(skip)]
    pub third_party_subscription_integration_authorized: bool,
    #[garde(length(bytes, min = 1, max = 500))]
    pub terms_reference: String,
    #[garde(length(bytes, min = 64, max = 64), ascii)]
    pub terms_sha256: String,
    #[garde(length(bytes, min = 1, max = 100))]
    pub decided_by: String,
    #[garde(length(bytes, min = 1, max = 100))]
    pub decided_at: String,
    #[garde(length(bytes, min = 1, max = 100))]
    pub expires_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, Validate)]
#[ts(export)]
pub struct TechnicalRiskAcceptance {
    #[garde(length(bytes, min = 1, max = 100))]
    pub risk_kind: String,
    #[garde(length(bytes, min = 1, max = 4000))]
    pub rationale: String,
    #[garde(length(bytes, min = 1, max = 100))]
    pub accepted_by: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct EvaluatePublicReleaseGateCommand {
    #[garde(length(bytes, min = 64, max = 64), ascii)]
    pub release_candidate_sha256: String,
    #[garde(length(bytes, min = 64, max = 64), ascii)]
    pub private_qualification_sha256: String,
    #[garde(skip)]
    pub native_platforms: Vec<NativePlatformQualificationRecord>,
    #[garde(dive)]
    pub license_review: LicenseDistributionReview,
    #[garde(dive)]
    pub codex_production_assurance: CodexProductionAssuranceEvidence,
    #[garde(dive)]
    pub integration_terms: IntegrationTermsDecision,
    #[garde(length(max = 16), dive)]
    pub technical_risks: Vec<TechnicalRiskAcceptance>,
    #[garde(length(min = 1, max = 128), dive)]
    pub release_artifacts: Vec<AcceptanceArtifactHash>,
    #[garde(length(bytes, min = 1, max = 100))]
    pub approver: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum PublicReleaseGateOutcome {
    Blocked,
    Authorized,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PublicReleaseGateRecord {
    pub gate_id: String,
    pub release_candidate_sha256: String,
    pub private_qualification_sha256: String,
    pub native_platforms: Vec<NativePlatformQualificationRecord>,
    pub license_review: LicenseDistributionReview,
    pub codex_production_assurance: CodexProductionAssuranceEvidence,
    pub integration_terms: IntegrationTermsDecision,
    pub technical_risks: Vec<TechnicalRiskAcceptance>,
    pub release_artifacts: Vec<AcceptanceArtifactHash>,
    pub blockers: Vec<String>,
    pub outcome: PublicReleaseGateOutcome,
    pub supported_platform_claims: Vec<String>,
    pub public_production_ready: bool,
    pub approver: String,
    pub manifest_sha256: String,
    pub created_at: String,
}

impl QuantixHost {
    pub fn record_native_platform_qualification(
        &self,
        command: RecordNativePlatformQualificationCommand,
    ) -> Result<NativePlatformQualificationRecord, TenderCommandError> {
        require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let evidence = command.evidence;
        validate_timestamp_order(&evidence.qualified_at, &evidence.expires_at)?;
        if evidence.qualified_by != "engineer_user" {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let connection = self.open_release_gate_database()?;
        let mut blockers = Vec::new();
        let current_platform = match (std::env::consts::OS, std::env::consts::ARCH) {
            ("windows", "x86_64") => Some("windows_11_x64"),
            ("macos", "aarch64") => Some("macos_14_apple_silicon"),
            ("linux", "x86_64") => Some("ubuntu_24_04_x64"),
            _ => None,
        };
        if current_platform != Some(evidence.platform.as_str()) {
            blockers.push("native_platform_does_not_match_current_host".into());
        }
        let completed = evidence
            .completed_checks
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        for required in REQUIRED_NATIVE_CHECKS {
            if !completed.contains(required) {
                blockers.push(format!("native_check_missing:{required}"));
            }
        }
        if evidence.fixture_sha256 != crate::acceptance::acceptance_fixture_sha256() {
            blockers.push("fixture_changed".into());
        }
        if evidence_expired(&evidence.expires_at) {
            blockers.push("native_evidence_expired".into());
        }
        let acceptance_json = connection
            .query_row(
                "SELECT run_json FROM product_acceptance_runs WHERE manifest_sha256 = ?1",
                [&evidence.product_acceptance_run_sha256],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(sql_error)?;
        let acceptance_run = acceptance_json
            .as_deref()
            .map(parse_record::<ProductAcceptanceRun>)
            .transpose()?;
        if acceptance_run.as_ref().is_none_or(|run| {
            run.outcome != ProductAcceptanceOutcome::Passed
                || run.application_artifact_sha256 != evidence.signed_binary_sha256
                || run.platform != evidence.platform
        }) {
            blockers.push("native_product_acceptance_missing_or_changed".into());
        }
        blockers.sort();
        blockers.dedup();
        let created_at = installation_timestamp(&connection)?;
        let mut record = NativePlatformQualificationRecord {
            record_id: installation_identifier(&connection)?,
            evidence,
            passed: blockers.is_empty(),
            blockers,
            manifest_sha256: String::new(),
            created_at,
        };
        record.manifest_sha256 = manifest_sha256(&record)?;
        connection
            .execute(
                "INSERT INTO native_platform_qualification_records (
                   record_id, platform, outcome, record_json, manifest_sha256, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    record.record_id,
                    record.evidence.platform,
                    if record.passed { "passed" } else { "failed" },
                    canonical_json(&record)?,
                    record.manifest_sha256,
                    record.created_at,
                ],
            )
            .map_err(sql_error)?;
        Ok(record)
    }

    pub fn evaluate_public_release_gate(
        &self,
        command: EvaluatePublicReleaseGateCommand,
    ) -> Result<PublicReleaseGateRecord, TenderCommandError> {
        require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        if command.native_platforms.len() != 3
            || command
                .native_platforms
                .iter()
                .any(|record| record.evidence.validate().is_err())
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        if command.license_review.reviewed_by != "engineer_user"
            || command.codex_production_assurance.verified_by != "engineer_user"
            || command.integration_terms.decided_by != "engineer_user"
            || command.approver != "engineer_user"
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        validate_timestamp_order(
            &command.license_review.reviewed_at,
            &command.license_review.expires_at,
        )?;
        validate_timestamp_order(
            &command.codex_production_assurance.verified_at,
            &command.codex_production_assurance.expires_at,
        )?;
        validate_timestamp_order(
            &command.integration_terms.decided_at,
            &command.integration_terms.expires_at,
        )?;
        for record in &command.native_platforms {
            validate_timestamp_order(&record.evidence.qualified_at, &record.evidence.expires_at)?;
        }
        let connection = self.open_release_gate_database()?;
        let mut blockers = Vec::new();
        let private_current: bool = connection
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM private_qualification_records
                   WHERE release_candidate_sha256 = ?1 AND manifest_sha256 = ?2
                 )",
                params![
                    command.release_candidate_sha256,
                    command.private_qualification_sha256
                ],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if !private_current {
            blockers.push("private_qualification_missing_or_changed".into());
        }
        let platforms = command
            .native_platforms
            .iter()
            .map(|record| record.evidence.platform.as_str())
            .collect::<BTreeSet<_>>();
        if platforms != BTreeSet::from(REQUIRED_PLATFORMS) {
            blockers.push("native_platform_set_incomplete".into());
        }
        for native_record in &command.native_platforms {
            let evidence = &native_record.evidence;
            let persisted_json = connection
                .query_row(
                    "SELECT record_json FROM native_platform_qualification_records
                     WHERE platform = ?1 AND manifest_sha256 = ?2 AND outcome = 'passed'",
                    params![evidence.platform, native_record.manifest_sha256],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(sql_error)?;
            let persisted = persisted_json
                .as_deref()
                .map(parse_record::<NativePlatformQualificationRecord>)
                .transpose()?;
            if persisted.as_ref() != Some(native_record)
                || !native_record.passed
                || !native_record.blockers.is_empty()
            {
                blockers.push(format!(
                    "native_record_missing_or_failed:{}",
                    evidence.platform
                ));
            }
            let completed = evidence
                .completed_checks
                .iter()
                .map(String::as_str)
                .collect::<BTreeSet<_>>();
            if REQUIRED_NATIVE_CHECKS
                .iter()
                .any(|required| !completed.contains(*required))
            {
                blockers.push(format!("native_checks_incomplete:{}", evidence.platform));
            }
            if evidence.fixture_sha256 != crate::acceptance::acceptance_fixture_sha256() {
                blockers.push(format!("fixture_changed:{}", evidence.platform));
            }
            if evidence.release_candidate_manifest_sha256 != command.release_candidate_sha256 {
                blockers.push(format!("release_candidate_changed:{}", evidence.platform));
            }
            if evidence_expired(&evidence.expires_at) {
                blockers.push(format!("native_evidence_expired:{}", evidence.platform));
            }
        }
        let categories = command
            .license_review
            .reviewed_categories
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        for category in [
            "redistributed_binaries",
            "rust_dependencies",
            "typescript_dependencies",
            "python_dependencies",
            "model_assets",
            "templates",
            "fixture_content",
        ] {
            if !categories.contains(category) {
                blockers.push(format!("license_category_missing:{category}"));
            }
        }
        if !command.license_review.passed || evidence_expired(&command.license_review.expires_at) {
            blockers.push("license_distribution_review_failed_or_expired".into());
        }
        if !CODEX_APP_SERVER_PRODUCTION_SUPPORTED
            || !command.codex_production_assurance.production_supported
            || evidence_expired(&command.codex_production_assurance.expires_at)
        {
            blockers.push("codex_production_assurance_absent_or_expired".into());
        }
        if !command
            .integration_terms
            .third_party_subscription_integration_authorized
            || evidence_expired(&command.integration_terms.expires_at)
        {
            blockers.push("integration_terms_not_authorized_or_expired".into());
        }
        if command
            .technical_risks
            .iter()
            .any(|risk| risk.risk_kind != "protocol_instability")
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        blockers.sort();
        blockers.dedup();
        let authorized = blockers.is_empty();
        let created_at = installation_timestamp(&connection)?;
        let mut record = PublicReleaseGateRecord {
            gate_id: installation_identifier(&connection)?,
            release_candidate_sha256: command.release_candidate_sha256,
            private_qualification_sha256: command.private_qualification_sha256,
            native_platforms: command.native_platforms,
            license_review: command.license_review,
            codex_production_assurance: command.codex_production_assurance,
            integration_terms: command.integration_terms,
            technical_risks: command.technical_risks,
            release_artifacts: command.release_artifacts,
            blockers,
            outcome: if authorized {
                PublicReleaseGateOutcome::Authorized
            } else {
                PublicReleaseGateOutcome::Blocked
            },
            supported_platform_claims: if authorized {
                REQUIRED_PLATFORMS
                    .iter()
                    .map(|value| (*value).into())
                    .collect()
            } else {
                Vec::new()
            },
            public_production_ready: authorized,
            approver: command.approver,
            manifest_sha256: String::new(),
            created_at,
        };
        record.manifest_sha256 = manifest_sha256(&record)?;
        connection
            .execute(
                "INSERT INTO public_release_gate_records (
                   gate_id, release_candidate_sha256, outcome, record_json,
                   manifest_sha256, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    record.gate_id,
                    record.release_candidate_sha256,
                    if authorized { "authorized" } else { "blocked" },
                    canonical_json(&record)?,
                    record.manifest_sha256,
                    record.created_at,
                ],
            )
            .map_err(sql_error)?;
        Ok(record)
    }

    pub fn inspect_current_public_release_gate(
        &self,
        release_candidate_sha256: &str,
    ) -> Result<Option<PublicReleaseGateRecord>, TenderCommandError> {
        require_setup(self)?;
        if release_candidate_sha256.len() != 64 {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let connection = Connection::open_with_flags(
            self.application_home().join("installation.sqlite"),
            OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .map_err(sql_error)?;
        let json = connection
            .query_row(
                "SELECT record_json FROM public_release_gate_records
                 WHERE release_candidate_sha256 = ?1
                 ORDER BY created_at DESC, gate_id DESC LIMIT 1",
                [release_candidate_sha256],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(sql_error)?;
        let Some(json) = json else {
            return Ok(None);
        };
        let record: PublicReleaseGateRecord = parse_record(&json)?;
        let expired = record
            .native_platforms
            .iter()
            .any(|record| evidence_expired(&record.evidence.expires_at))
            || evidence_expired(&record.license_review.expires_at)
            || evidence_expired(&record.codex_production_assurance.expires_at)
            || evidence_expired(&record.integration_terms.expires_at);
        if expired {
            Ok(None)
        } else {
            Ok(Some(record))
        }
    }

    fn open_release_gate_database(&self) -> Result<Connection, TenderCommandError> {
        Connection::open(self.application_home().join("installation.sqlite")).map_err(sql_error)
    }
}

fn validate_timestamp_order(created_at: &str, expires_at: &str) -> Result<(), TenderCommandError> {
    let created = created_at
        .parse::<Timestamp>()
        .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
    let expires = expires_at
        .parse::<Timestamp>()
        .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
    let now = Timestamp::now()
        .round(jiff::Unit::Millisecond)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
    if created > now || expires <= created {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    Ok(())
}

fn evidence_expired(expires_at: &str) -> bool {
    expires_at
        .parse::<Timestamp>()
        .map(|expires| expires <= Timestamp::now())
        .unwrap_or(true)
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
    let record: T = serde_json::from_str(value)
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
