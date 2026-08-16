use std::{
    collections::BTreeSet,
    ffi::OsString,
    fs,
    fs::OpenOptions,
    io::{self, Read},
    path::{Component, Path, PathBuf},
    process::Command,
    thread,
    time::Duration,
};

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use garde::Validate;
use minisign_verify::{Error as MinisignError, PublicKey, Signature};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension, TransactionBehavior};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ts_rs::TS;

use crate::{
    agent_runtime::CODEX_VERSION,
    runtime_readiness::{RuntimeReadinessState, DOCLING_VERSION, RUNTIME_PROVENANCE_SCHEMA},
    setup::{SetupState, INSTALLATION_SCHEMA_VERSION},
    tender_store::TENDER_SCHEMA_VERSION,
    QuantixHost,
};

const INSTALLATION_DATABASE: &str = "installation.sqlite";
const APPLICATION_RECOVERY_DIRECTORY: &str = "update-recovery";
const MAX_APPLICATION_ARTIFACT_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_APPLICATION_ARTIFACT_ENTRIES: usize = 16_384;
const ZERO_UPDATE_DECISION_HASH: &str =
    "0000000000000000000000000000000000000000000000000000000000000000";
const DEFAULT_UPDATE_ENDPOINT: &str =
    "https://github.com/kareem-sf/quantix/releases/latest/download/latest.json";
#[cfg(debug_assertions)]
const EMBEDDED_UPDATE_PUBLIC_KEY: Option<&str> = option_env!("QUANTIX_UPDATE_PUBLIC_KEY");
#[cfg(not(debug_assertions))]
const EMBEDDED_UPDATE_PUBLIC_KEY: Option<&str> = Some(env!(
    "QUANTIX_UPDATE_PUBLIC_KEY",
    "release builds require QUANTIX_UPDATE_PUBLIC_KEY from the secure release environment"
));

pub(crate) struct UpdateReleaseConfiguration {
    pub(crate) endpoint: tauri::Url,
    pub(crate) public_key: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstalledApplicationArtifactKind {
    WindowsBundle,
    MacOsBundle,
    LinuxAppImage,
}

impl InstalledApplicationArtifactKind {
    fn database_value(self) -> &'static str {
        match self {
            Self::WindowsBundle => "windows_bundle",
            Self::MacOsBundle => "mac_os_bundle",
            Self::LinuxAppImage => "linux_app_image",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledApplicationArtifactSet {
    pub kind: InstalledApplicationArtifactKind,
    pub root: PathBuf,
    pub launcher_relative: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplicationRollbackPlan {
    pub helper_path: PathBuf,
    pub arguments: Vec<OsString>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ApplicationRecoveryEntryKind {
    Directory,
    File,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ApplicationRecoveryEntry {
    relative_path: PathBuf,
    kind: ApplicationRecoveryEntryKind,
    size_bytes: Option<u64>,
    sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ApplicationRecoveryManifest {
    update_id: String,
    application_version: String,
    artifact_kind: InstalledApplicationArtifactKind,
    destination_root: PathBuf,
    launcher_relative: Option<PathBuf>,
    entries: Vec<ApplicationRecoveryEntry>,
}

pub(crate) fn update_release_configuration(
) -> Result<UpdateReleaseConfiguration, UpdateCommandError> {
    let endpoint = option_env!("QUANTIX_UPDATE_ENDPOINT").unwrap_or(DEFAULT_UPDATE_ENDPOINT);
    let endpoint = tauri::Url::parse(endpoint)
        .ok()
        .filter(|url| url.scheme() == "https")
        .ok_or_else(|| UpdateCommandError::new(UpdateDiagnostic::UpdaterConfigurationMissing))?;
    let public_key = EMBEDDED_UPDATE_PUBLIC_KEY
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| UpdateCommandError::new(UpdateDiagnostic::UpdaterConfigurationMissing))?;
    decode_tauri_public_key(public_key)
        .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::UpdaterConfigurationMissing))?;
    Ok(UpdateReleaseConfiguration {
        endpoint,
        public_key: public_key.to_owned(),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum UpdatePlatform {
    WindowsX86_64,
    MacOsAarch64,
    UbuntuX86_64,
}

impl UpdatePlatform {
    pub fn current() -> Option<Self> {
        if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
            Some(Self::WindowsX86_64)
        } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            Some(Self::MacOsAarch64)
        } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
            Some(Self::UbuntuX86_64)
        } else {
            None
        }
    }
}

pub fn current_update_platform() -> Option<UpdatePlatform> {
    UpdatePlatform::current()
}

pub fn current_application_artifact_is_restorable() -> bool {
    installed_application_artifact_for_recovery().is_ok()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct SignedArtifactIdentity {
    #[garde(custom(valid_sha256))]
    pub sha256: String,
    #[garde(custom(valid_sha256))]
    pub signature_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct UpdateCompatibilityManifest {
    #[garde(range(min = 1))]
    pub installation_schema_version: u32,
    #[garde(range(min = 1))]
    pub tender_schema_version: u32,
    #[garde(length(bytes, min = 1, max = 64))]
    pub codex_version: String,
    #[garde(length(bytes, min = 1, max = 64))]
    pub docling_version: String,
    #[garde(range(min = 1))]
    pub runtime_manifest_schema_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct UpdateReleaseInformation {
    #[garde(length(bytes, min = 1, max = 64))]
    pub published_at: String,
    #[garde(length(bytes, min = 1, max = 200))]
    pub title: String,
    #[garde(length(bytes, min = 1, max = 4_000))]
    pub notes: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct UpdateImpact {
    #[garde(length(bytes, min = 1, max = 1_000))]
    pub summary: String,
    #[garde(skip)]
    pub stored_data_may_change: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct UpdateCandidate {
    #[garde(length(bytes, min = 1, max = 64))]
    pub current_version: String,
    #[garde(length(bytes, min = 1, max = 64))]
    pub version: String,
    #[garde(skip)]
    pub platform: UpdatePlatform,
    #[garde(dive)]
    pub artifact: SignedArtifactIdentity,
    #[garde(dive)]
    pub compatibility: UpdateCompatibilityManifest,
    #[garde(dive)]
    pub release: UpdateReleaseInformation,
    #[garde(dive)]
    pub impact: UpdateImpact,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct UpdateOffer {
    pub update_id: String,
    pub current_version: String,
    pub version: String,
    pub platform: UpdatePlatform,
    pub artifact: SignedArtifactIdentity,
    pub compatibility: UpdateCompatibilityManifest,
    pub release: UpdateReleaseInformation,
    pub impact: UpdateImpact,
}

impl UpdateCandidate {
    pub fn canonical_manifest_bytes(&self) -> Result<Vec<u8>, UpdateCommandError> {
        self.validate()
            .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InvalidManifest))?;
        serde_json_canonicalizer::to_vec(self)
            .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InvalidManifest))
    }

    fn into_offer(self) -> Result<UpdateOffer, UpdateCommandError> {
        let bytes = self.canonical_manifest_bytes()?;
        let update_id = sha256_hex(&bytes);
        Ok(UpdateOffer {
            update_id,
            current_version: self.current_version,
            version: self.version,
            platform: self.platform,
            artifact: self.artifact,
            compatibility: self.compatibility,
            release: self.release,
            impact: self.impact,
        })
    }
}

fn validate_update_offer_identity(offer: &UpdateOffer) -> Result<(), UpdateCommandError> {
    let expected = UpdateCandidate {
        current_version: offer.current_version.clone(),
        version: offer.version.clone(),
        platform: offer.platform,
        artifact: offer.artifact.clone(),
        compatibility: offer.compatibility.clone(),
        release: offer.release.clone(),
        impact: offer.impact.clone(),
    }
    .into_offer()?;
    if expected != *offer {
        return Err(UpdateCommandError::new(UpdateDiagnostic::InvalidManifest));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum UpdateDecision {
    Approve,
    Deny,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct UpdateDecisionRecord {
    pub sequence: u32,
    pub update_id: String,
    pub offer_sha256: String,
    pub decision: UpdateDecision,
    pub rationale: String,
    pub decided_by: String,
    pub acting_role: String,
    pub decided_at: String,
    pub preceding_hash: String,
    pub current_hash: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum UpdateState {
    Idle,
    AwaitingApproval,
    Approved,
    Denied,
    Installing,
    RestartValidationRequired,
    Ready,
    Rejected,
    RepairRequired,
    RolledBack,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum UpdateDiagnostic {
    InvalidManifest,
    DowngradeRejected,
    UnsupportedPlatform,
    InstallationSchemaIncompatible,
    TenderStoreIncompatible,
    CodexIncompatible,
    DoclingIncompatible,
    RuntimeIncompatible,
    ApprovalRequired,
    ActiveWork,
    VerifiedBackupRequired,
    UnsignedArtifact,
    WrongSigningKey,
    ArtifactTampered,
    DownloadFailed,
    InstallationFailed,
    InstallationInterrupted,
    RestartValidationFailed,
    UpdaterConfigurationMissing,
    UpdaterUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct UpdateStatus {
    pub state: UpdateState,
    pub offer: Option<UpdateOffer>,
    pub decision_history: Vec<UpdateDecisionRecord>,
    pub diagnostic: Option<UpdateDiagnostic>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct UpdateCommandError {
    pub diagnostic: UpdateDiagnostic,
}

impl UpdateCommandError {
    pub(crate) fn new(diagnostic: UpdateDiagnostic) -> Self {
        Self { diagnostic }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Validate, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct DecideUpdateCommand {
    #[garde(custom(valid_sha256))]
    pub update_id: String,
    #[garde(skip)]
    pub decision: UpdateDecision,
    #[garde(length(bytes, min = 1, max = 4_000), custom(non_blank))]
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Validate, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct InstallUpdateCommand {
    #[garde(custom(valid_sha256))]
    pub update_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Validate, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct UpdateActionCommand {
    #[garde(custom(valid_sha256))]
    pub update_id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TauriQuantixManifest {
    artifact_sha256: String,
    manifest_signature: String,
    release_title: String,
    compatibility: UpdateCompatibilityManifest,
    impact: UpdateImpact,
}

pub(crate) fn candidate_from_tauri_update(
    update: &tauri_plugin_updater::Update,
) -> Result<(UpdateCandidate, String), UpdateCommandError> {
    if update.signature.trim().is_empty() {
        return Err(UpdateCommandError::new(UpdateDiagnostic::UnsignedArtifact));
    }
    let extension: TauriQuantixManifest = serde_json::from_value(
        update
            .raw_json
            .get("quantix")
            .cloned()
            .ok_or_else(|| UpdateCommandError::new(UpdateDiagnostic::InvalidManifest))?,
    )
    .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InvalidManifest))?;
    let platform = platform_from_tauri_target(&update.target)?;
    let candidate = UpdateCandidate {
        current_version: update.current_version.clone(),
        version: update.version.clone(),
        platform,
        artifact: SignedArtifactIdentity {
            sha256: extension.artifact_sha256,
            signature_sha256: sha256_hex(update.signature.as_bytes()),
        },
        compatibility: extension.compatibility,
        release: UpdateReleaseInformation {
            published_at: update
                .date
                .map(|date| date.to_string())
                .ok_or_else(|| UpdateCommandError::new(UpdateDiagnostic::InvalidManifest))?,
            title: extension.release_title,
            notes: update
                .body
                .clone()
                .filter(|notes| !notes.trim().is_empty())
                .ok_or_else(|| UpdateCommandError::new(UpdateDiagnostic::InvalidManifest))?,
        },
        impact: extension.impact,
    };
    Ok((candidate, extension.manifest_signature))
}

pub(crate) fn downloaded_artifact_matches(bytes: &[u8], expected_sha256: &str) -> bool {
    sha256_hex(bytes) == expected_sha256
}

pub fn verify_signed_update_candidate(
    candidate: &UpdateCandidate,
    signature: &str,
    public_key: &str,
) -> Result<(), UpdateCommandError> {
    verify_minisign(
        &candidate.canonical_manifest_bytes()?,
        signature,
        public_key,
    )
}

pub fn verify_signed_update_artifact(
    bytes: &[u8],
    signature: &str,
    public_key: &str,
    expected_sha256: &str,
) -> Result<(), UpdateCommandError> {
    if !downloaded_artifact_matches(bytes, expected_sha256) {
        return Err(UpdateCommandError::new(UpdateDiagnostic::ArtifactTampered));
    }
    verify_minisign(bytes, signature, public_key)
}

fn verify_minisign(
    bytes: &[u8],
    signature: &str,
    public_key: &str,
) -> Result<(), UpdateCommandError> {
    if signature.trim().is_empty() {
        return Err(UpdateCommandError::new(UpdateDiagnostic::UnsignedArtifact));
    }
    let public_key = decode_tauri_public_key(public_key)
        .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::UpdaterUnavailable))?;
    let signature = BASE64_STANDARD
        .decode(signature.trim())
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .and_then(|signature| Signature::decode(&signature).ok())
        .ok_or_else(|| UpdateCommandError::new(UpdateDiagnostic::UnsignedArtifact))?;
    public_key
        .verify(bytes, &signature, false)
        .map_err(|error| {
            UpdateCommandError::new(match error {
                MinisignError::UnexpectedKeyId => UpdateDiagnostic::WrongSigningKey,
                _ => UpdateDiagnostic::ArtifactTampered,
            })
        })
}

fn decode_tauri_public_key(public_key: &str) -> Result<PublicKey, MinisignError> {
    let decoded = BASE64_STANDARD
        .decode(public_key.trim())
        .map_err(|_| MinisignError::InvalidEncoding)?;
    let decoded = String::from_utf8(decoded).map_err(|_| MinisignError::InvalidEncoding)?;
    PublicKey::decode(&decoded)
}

pub(crate) fn updater_download_diagnostic(error: &tauri_plugin_updater::Error) -> UpdateDiagnostic {
    match error {
        tauri_plugin_updater::Error::Minisign(MinisignError::UnexpectedKeyId) => {
            UpdateDiagnostic::WrongSigningKey
        }
        tauri_plugin_updater::Error::Minisign(MinisignError::InvalidSignature) => {
            UpdateDiagnostic::ArtifactTampered
        }
        tauri_plugin_updater::Error::Minisign(_) => UpdateDiagnostic::UnsignedArtifact,
        tauri_plugin_updater::Error::Base64(_) | tauri_plugin_updater::Error::SignatureUtf8(_) => {
            UpdateDiagnostic::UnsignedArtifact
        }
        _ => UpdateDiagnostic::DownloadFailed,
    }
}

pub fn update_platform_from_target(target: &str) -> Option<UpdatePlatform> {
    let normalized = target.to_ascii_lowercase();
    match normalized.as_str() {
        "windows-x86_64" | "x86_64-pc-windows-msvc" => Some(UpdatePlatform::WindowsX86_64),
        "darwin-aarch64" | "macos-aarch64" | "aarch64-apple-darwin" => {
            Some(UpdatePlatform::MacOsAarch64)
        }
        "linux-x86_64" | "x86_64-unknown-linux-gnu" | "x86_64-unknown-linux-musl" => {
            Some(UpdatePlatform::UbuntuX86_64)
        }
        _ => None,
    }
}

fn platform_from_tauri_target(target: &str) -> Result<UpdatePlatform, UpdateCommandError> {
    match update_platform_from_target(target) {
        Some(platform) if Some(platform) == UpdatePlatform::current() => Ok(platform),
        _ => Err(UpdateCommandError::new(
            UpdateDiagnostic::UnsupportedPlatform,
        )),
    }
}

impl QuantixHost {
    pub(crate) fn require_update_ready_setup(&self) -> Result<(), UpdateCommandError> {
        let setup = crate::ensure_quantix_setup(self);
        if matches!(
            setup.state,
            crate::SetupState::Ready | crate::SetupState::Warning
        ) {
            return Ok(());
        }
        let diagnostic = if setup
            .issues
            .contains(&crate::SetupIssue::UpdateInstallationActive)
        {
            UpdateDiagnostic::ActiveWork
        } else {
            UpdateDiagnostic::UpdaterUnavailable
        };
        Err(UpdateCommandError::new(diagnostic))
    }

    pub fn stage_application_recovery_point(
        &self,
        update_id: &str,
        application_version: &str,
        artifact_set: &InstalledApplicationArtifactSet,
    ) -> Result<(), UpdateCommandError> {
        if !self.update_installation_lease_is_held()
            || !valid_sha256_value(update_id)
            || Version::parse(application_version).is_err()
            || !matches!(
                self.load_update_status(update_id),
                Ok(status) if status.state == UpdateState::Approved
            )
        {
            return Err(UpdateCommandError::new(
                UpdateDiagnostic::InstallationFailed,
            ));
        }
        let manifest =
            build_application_recovery_manifest(update_id, application_version, artifact_set)?;
        let application_home = self
            .application_home()
            .canonicalize()
            .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?;
        if application_home.starts_with(&manifest.destination_root)
            || manifest.destination_root.starts_with(&application_home)
        {
            return Err(UpdateCommandError::new(
                UpdateDiagnostic::InstallationFailed,
            ));
        }
        let manifest_json = serde_json_canonicalizer::to_string(&manifest)
            .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?;
        let manifest_sha256 = sha256_hex(manifest_json.as_bytes());
        let recovery_parent = application_recovery_parent(self.application_home());
        ensure_private_recovery_directory(&recovery_parent)?;
        let final_root = recovery_parent.join(update_id);
        let staging = self
            .application_home()
            .join("staging")
            .join(format!("update-recovery-{update_id}"));
        let recovery_fact = update_connection(self.application_home())?
            .query_row(
                "SELECT application_version, artifact_kind, destination_root,
                        manifest_sha256, manifest_json
                 FROM update_recovery_points WHERE update_id = ?1",
                [update_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                    ))
                },
            )
            .optional()
            .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?;
        if let Some(recovery_fact) = recovery_fact {
            let destination_root = manifest
                .destination_root
                .to_str()
                .ok_or_else(|| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?;
            if recovery_fact
                != (
                    manifest.application_version.clone(),
                    manifest.artifact_kind.database_value().to_owned(),
                    destination_root.to_owned(),
                    manifest_sha256.clone(),
                    manifest_json.clone(),
                )
            {
                return Err(UpdateCommandError::new(
                    UpdateDiagnostic::InstallationFailed,
                ));
            }
            if final_root.exists() {
                let existing =
                    load_application_recovery_manifest(self.application_home(), update_id)?;
                if existing == manifest && recovery_artifacts_verify(&final_root, &existing) {
                    if staging.exists() {
                        let metadata = fs::symlink_metadata(&staging).map_err(|_| {
                            UpdateCommandError::new(UpdateDiagnostic::InstallationFailed)
                        })?;
                        if metadata.file_type().is_symlink() || !metadata.is_dir() {
                            return Err(UpdateCommandError::new(
                                UpdateDiagnostic::InstallationFailed,
                            ));
                        }
                        fs::remove_dir_all(&staging).map_err(|_| {
                            UpdateCommandError::new(UpdateDiagnostic::InstallationFailed)
                        })?;
                    }
                    return Ok(());
                }
                return Err(UpdateCommandError::new(
                    UpdateDiagnostic::InstallationFailed,
                ));
            }
            if staging.exists() {
                let existing =
                    load_application_recovery_manifest(self.application_home(), update_id)?;
                if existing == manifest && recovery_artifacts_verify(&final_root, &existing) {
                    return Ok(());
                }
                return Err(UpdateCommandError::new(
                    UpdateDiagnostic::InstallationFailed,
                ));
            }
            fs::create_dir(&staging)
                .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?;
            let result = (|| {
                copy_application_recovery_artifacts(artifact_set, &manifest, &staging)?;
                write_durable_recovery_manifest(&staging, &manifest_json)?;
                if !recovery_artifacts_verify(&staging, &manifest) {
                    return Err(UpdateCommandError::new(UpdateDiagnostic::ArtifactTampered));
                }
                publish_recovery_directory(&staging, &final_root, &recovery_parent)
            })();
            if result.is_err() {
                let _ = fs::remove_dir_all(&staging);
            }
            return result;
        }
        if final_root.exists() {
            return Err(UpdateCommandError::new(
                UpdateDiagnostic::InstallationFailed,
            ));
        }
        if staging.exists() {
            let metadata = fs::symlink_metadata(&staging)
                .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(UpdateCommandError::new(
                    UpdateDiagnostic::InstallationFailed,
                ));
            }
            fs::remove_dir_all(&staging)
                .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?;
        }
        fs::create_dir(&staging)
            .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?;
        let mut recovery_fact_committed = false;
        let result = (|| {
            copy_application_recovery_artifacts(artifact_set, &manifest, &staging)?;
            write_durable_recovery_manifest(&staging, &manifest_json)?;
            if !recovery_artifacts_verify(&staging, &manifest) {
                return Err(UpdateCommandError::new(UpdateDiagnostic::ArtifactTampered));
            }
            let destination_root = manifest
                .destination_root
                .to_str()
                .ok_or_else(|| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?;
            update_connection(self.application_home())?
                .execute(
                    "INSERT INTO update_recovery_points (
                       update_id, application_version, artifact_kind, destination_root,
                       manifest_sha256, manifest_json
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        manifest.update_id,
                        manifest.application_version,
                        manifest.artifact_kind.database_value(),
                        destination_root,
                        manifest_sha256,
                        manifest_json,
                    ],
                )
                .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?;
            recovery_fact_committed = true;
            publish_recovery_directory(&staging, &final_root, &recovery_parent)?;
            Ok(())
        })();
        if result.is_err() && !recovery_fact_committed {
            let _ = fs::remove_dir_all(&staging);
            let _ = fs::remove_dir_all(&final_root);
        }
        result
    }

    pub fn restore_application_recovery_point(
        &self,
        update_id: &str,
    ) -> Result<(), UpdateCommandError> {
        restore_application_recovery_point(self.application_home(), update_id).map(|_| ())
    }

    pub fn prepare_application_rollback(
        &self,
        update_id: &str,
    ) -> Result<ApplicationRollbackPlan, UpdateCommandError> {
        require_repair_required(self.application_home(), update_id)?;
        let helper_root = application_recovery_parent(self.application_home()).join(update_id);
        let manifest = load_application_recovery_manifest(self.application_home(), update_id)?;
        if !recovery_artifacts_verify(&helper_root, &manifest) {
            return Err(UpdateCommandError::new(UpdateDiagnostic::ArtifactTampered));
        }
        let helper = match manifest.artifact_kind {
            InstalledApplicationArtifactKind::LinuxAppImage => {
                helper_root.join("artifacts/application")
            }
            InstalledApplicationArtifactKind::WindowsBundle
            | InstalledApplicationArtifactKind::MacOsBundle => helper_root.join("artifacts").join(
                manifest
                    .launcher_relative
                    .ok_or_else(|| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?,
            ),
        };
        if !helper.is_file() {
            return Err(UpdateCommandError::new(
                UpdateDiagnostic::InstallationFailed,
            ));
        }
        Ok(ApplicationRollbackPlan {
            helper_path: helper,
            arguments: vec![
                OsString::from("--quantix-update-rollback"),
                self.application_home().as_os_str().to_owned(),
                OsString::from(update_id),
            ],
        })
    }

    pub(crate) fn schedule_application_rollback(
        &self,
        update_id: &str,
    ) -> Result<(), UpdateCommandError> {
        let plan = self.prepare_application_rollback(update_id)?;
        Command::new(plan.helper_path)
            .args(plan.arguments)
            .spawn()
            .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?;
        Ok(())
    }

    pub fn authorize_update_restart(
        &self,
        update_id: &str,
    ) -> Result<UpdateStatus, UpdateCommandError> {
        let status = self.load_update_status(update_id)?;
        if status.state != UpdateState::RestartValidationRequired {
            return Err(UpdateCommandError::new(
                UpdateDiagnostic::RestartValidationFailed,
            ));
        }
        Ok(status)
    }

    pub fn present_update(
        &self,
        candidate: UpdateCandidate,
    ) -> Result<UpdateStatus, UpdateCommandError> {
        self.require_update_ready_setup()?;
        if update_work_is_blocked(self.application_home()) {
            return Err(UpdateCommandError::new(UpdateDiagnostic::ActiveWork));
        }
        let offer = candidate.into_offer()?;
        validate_compatibility(&offer)?;
        let connection = update_connection(self.application_home())?;
        connection
            .execute(
                "INSERT INTO update_operations (
                   update_id, state, offer_json, diagnostic_code, created_at, updated_at
                 ) VALUES (?1, 'awaiting_approval', ?2, NULL,
                           strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                           strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                 ON CONFLICT(update_id) DO NOTHING",
                params![
                    offer.update_id,
                    serde_json::to_string(&offer)
                        .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InvalidManifest))?
                ],
            )
            .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::UpdaterUnavailable))?;
        self.load_update_status(&offer.update_id)
    }

    pub fn decide_update(
        &self,
        update_id: String,
        decision: UpdateDecision,
        rationale: String,
    ) -> Result<UpdateStatus, UpdateCommandError> {
        self.require_update_ready_setup()?;
        if !valid_sha256_value(&update_id) || rationale.trim().is_empty() || rationale.len() > 4_000
        {
            return Err(UpdateCommandError::new(UpdateDiagnostic::InvalidManifest));
        }
        let acting_engineer = self.acting_engineer_user();
        let state = match decision {
            UpdateDecision::Approve => "approved",
            UpdateDecision::Deny => "denied",
        };
        let mut connection = update_connection(self.application_home())?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::UpdaterUnavailable))?;
        let offered: Option<(String, String)> = transaction
            .query_row(
                "SELECT state, offer_json FROM update_operations WHERE update_id = ?1",
                [&update_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::UpdaterUnavailable))?;
        let Some((current_state, offer_json)) = offered else {
            return Err(UpdateCommandError::new(UpdateDiagnostic::ApprovalRequired));
        };
        if current_state != "awaiting_approval" {
            return Err(UpdateCommandError::new(UpdateDiagnostic::ApprovalRequired));
        }
        let offer: UpdateOffer = serde_json::from_str(&offer_json)
            .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InvalidManifest))?;
        if offer.update_id != update_id || validate_update_offer_identity(&offer).is_err() {
            return Err(UpdateCommandError::new(UpdateDiagnostic::InvalidManifest));
        }
        append_update_decision(
            &transaction,
            &update_id,
            decision,
            &rationale,
            acting_engineer,
        )?;
        let changed = transaction
            .execute(
                "UPDATE update_operations
                 SET state = ?2, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                 WHERE update_id = ?1 AND state = 'awaiting_approval'",
                params![update_id, state],
            )
            .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::UpdaterUnavailable))?;
        if changed != 1 {
            return Err(UpdateCommandError::new(UpdateDiagnostic::ApprovalRequired));
        }
        transaction
            .commit()
            .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::UpdaterUnavailable))?;
        self.load_update_status(&update_id)
    }

    pub fn authorize_update_installation(
        &self,
        update_id: &str,
    ) -> Result<UpdateStatus, UpdateCommandError> {
        self.require_update_ready_setup()?;
        let current = self.load_update_status(update_id)?;
        if current.state != UpdateState::Approved
            || current.decision_history.last().is_none_or(|record| {
                record.decision != UpdateDecision::Approve
                    || record.update_id != update_id
                    || record.offer_sha256 != update_id
                    || record.decided_by != self.acting_engineer_user()
                    || record.acting_role != "tendering_manager"
            })
        {
            return Err(UpdateCommandError::new(UpdateDiagnostic::ApprovalRequired));
        }
        if !self.claim_update_installation() {
            return Err(UpdateCommandError::new(UpdateDiagnostic::ActiveWork));
        }
        let result = (|| {
            if !self.update_environment_is_quiescent() {
                return Err(UpdateCommandError::new(UpdateDiagnostic::ActiveWork));
            }
            let offer = current
                .offer
                .as_ref()
                .ok_or_else(|| UpdateCommandError::new(UpdateDiagnostic::InvalidManifest))?;
            if offer.impact.stored_data_may_change && !self.has_exact_verified_backups()? {
                return Err(UpdateCommandError::new(
                    UpdateDiagnostic::VerifiedBackupRequired,
                ));
            }
            self.load_update_status(update_id)
        })();
        if result.is_err() {
            self.release_update_installation();
        }
        result
    }

    pub(crate) fn cancel_update_installation_authorization(
        &self,
        update_id: &str,
    ) -> Result<UpdateStatus, UpdateCommandError> {
        self.release_update_installation();
        self.load_update_status(update_id)
    }

    pub fn begin_update_installation_after_recovery(
        &self,
        update_id: &str,
    ) -> Result<UpdateStatus, UpdateCommandError> {
        if !self.update_installation_lease_is_held()
            || load_application_recovery_manifest(self.application_home(), update_id).is_err()
        {
            return Err(UpdateCommandError::new(
                UpdateDiagnostic::InstallationFailed,
            ));
        }
        transition_update_to_installing(self.application_home(), update_id)?;
        self.load_update_status(update_id)
    }

    pub(crate) fn record_update_rejection_before_install(
        &self,
        update_id: &str,
        diagnostic: UpdateDiagnostic,
    ) -> Result<UpdateStatus, UpdateCommandError> {
        if !matches!(
            diagnostic,
            UpdateDiagnostic::UnsignedArtifact
                | UpdateDiagnostic::WrongSigningKey
                | UpdateDiagnostic::ArtifactTampered
        ) {
            return Err(UpdateCommandError::new(UpdateDiagnostic::InvalidManifest));
        }
        transition_update(
            self.application_home(),
            update_id,
            "approved",
            "rejected",
            Some(diagnostic),
        )?;
        self.release_update_installation();
        self.load_update_status(update_id)
    }

    pub fn record_update_failure(
        &self,
        update_id: &str,
        diagnostic: UpdateDiagnostic,
    ) -> Result<UpdateStatus, UpdateCommandError> {
        let (expected, state) = match diagnostic {
            UpdateDiagnostic::UnsignedArtifact
            | UpdateDiagnostic::WrongSigningKey
            | UpdateDiagnostic::ArtifactTampered
            | UpdateDiagnostic::DownloadFailed
            | UpdateDiagnostic::UpdaterConfigurationMissing
            | UpdateDiagnostic::UpdaterUnavailable => ("approved", "rejected"),
            _ => ("installing", "repair_required"),
        };
        transition_update(
            self.application_home(),
            update_id,
            expected,
            state,
            Some(diagnostic),
        )?;
        self.release_update_installation();
        self.load_update_status(update_id)
    }

    pub fn record_update_installed(
        &self,
        update_id: &str,
    ) -> Result<UpdateStatus, UpdateCommandError> {
        transition_update(
            self.application_home(),
            update_id,
            "installing",
            "restart_validation_required",
            None,
        )?;
        self.release_update_installation();
        self.load_update_status(update_id)
    }

    pub fn inspect_update_status(&self) -> Result<UpdateStatus, UpdateCommandError> {
        let connection = update_connection(self.application_home())?;
        let update_id = connection
            .query_row(
                "SELECT update_id FROM update_operations
                 ORDER BY state IN ('installing', 'restart_validation_required', 'repair_required') DESC,
                          rowid DESC
                 LIMIT 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::UpdaterUnavailable))?;
        match update_id {
            Some(update_id) => self.load_update_status(&update_id),
            None => Ok(UpdateStatus {
                state: UpdateState::Idle,
                offer: None,
                decision_history: Vec::new(),
                diagnostic: None,
            }),
        }
    }

    pub async fn validate_update_after_restart(
        &self,
        application_version: &str,
    ) -> Result<UpdateStatus, UpdateCommandError> {
        let mut status = self.inspect_update_status()?;
        let Some(offer) = status.offer.clone() else {
            return Ok(status);
        };
        if status.state == UpdateState::Installing {
            if application_version == offer.version {
                transition_update(
                    self.application_home(),
                    &offer.update_id,
                    "installing",
                    "restart_validation_required",
                    None,
                )?;
            } else {
                transition_update(
                    self.application_home(),
                    &offer.update_id,
                    "installing",
                    "repair_required",
                    Some(UpdateDiagnostic::InstallationInterrupted),
                )?;
                return self.load_update_status(&offer.update_id);
            }
            status = self.load_update_status(&offer.update_id)?;
        }
        if status.state == UpdateState::RepairRequired {
            if application_version == offer.current_version {
                let original_diagnostic = status
                    .diagnostic
                    .unwrap_or(UpdateDiagnostic::InstallationInterrupted);
                let (runtime_ready, tenders_ready) =
                    self.validate_restarted_update_environment(&offer).await;
                if runtime_ready
                    && tenders_ready
                    && application_recovery_is_restored(self.application_home(), &offer.update_id)
                {
                    transition_update(
                        self.application_home(),
                        &offer.update_id,
                        "repair_required",
                        "rolled_back",
                        Some(original_diagnostic),
                    )?;
                    cleanup_application_recovery(self.application_home(), &offer.update_id)?;
                } else {
                    transition_update(
                        self.application_home(),
                        &offer.update_id,
                        "repair_required",
                        "repair_required",
                        Some(UpdateDiagnostic::RestartValidationFailed),
                    )?;
                }
                return self.load_update_status(&offer.update_id);
            }
            if application_version == offer.version {
                transition_update(
                    self.application_home(),
                    &offer.update_id,
                    "repair_required",
                    "restart_validation_required",
                    None,
                )?;
                status = self.load_update_status(&offer.update_id)?;
            }
        }
        if status.state != UpdateState::RestartValidationRequired {
            return Ok(status);
        }
        if application_version != offer.version {
            transition_update(
                self.application_home(),
                &offer.update_id,
                "restart_validation_required",
                "repair_required",
                Some(UpdateDiagnostic::RestartValidationFailed),
            )?;
            return self.load_update_status(&offer.update_id);
        }

        let (runtime_ready, tenders_ready) =
            self.validate_restarted_update_environment(&offer).await;
        let (next, diagnostic) = if runtime_ready && tenders_ready {
            ("ready", None)
        } else {
            (
                "repair_required",
                Some(UpdateDiagnostic::RestartValidationFailed),
            )
        };
        transition_update(
            self.application_home(),
            &offer.update_id,
            "restart_validation_required",
            next,
            diagnostic,
        )?;
        if next == "ready" {
            cleanup_application_recovery(self.application_home(), &offer.update_id)?;
        }
        self.load_update_status(&offer.update_id)
    }

    async fn validate_restarted_update_environment(&self, offer: &UpdateOffer) -> (bool, bool) {
        let setup = self.validate_setup_for_update_restart();
        if !matches!(setup.state, SetupState::Ready | SetupState::Warning) {
            return (false, false);
        }
        let readiness = self.inspect_runtime_readiness_for_update().await;
        let runtime_ready = matches!(
            readiness.state,
            RuntimeReadinessState::Ready | RuntimeReadinessState::AuthenticationRequired
        ) && readiness.codex_version.as_deref()
            == Some(offer.compatibility.codex_version.as_str())
            && readiness.docling_version.as_deref()
                == Some(offer.compatibility.docling_version.as_str())
            && offer.compatibility.runtime_manifest_schema_version == RUNTIME_PROVENANCE_SCHEMA;
        let tenders_ready = self
            .all_tender_integrity_ready_for_update()
            .unwrap_or(false);
        (runtime_ready, tenders_ready)
    }

    fn load_update_status(&self, update_id: &str) -> Result<UpdateStatus, UpdateCommandError> {
        let connection = update_connection(self.application_home())?;
        let row = connection
            .query_row(
                "SELECT state, offer_json, diagnostic_code
                 FROM update_operations WHERE update_id = ?1",
                [update_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                },
            )
            .optional()
            .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::UpdaterUnavailable))?
            .ok_or_else(|| UpdateCommandError::new(UpdateDiagnostic::InvalidManifest))?;
        let offer: UpdateOffer = serde_json::from_str(&row.1)
            .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InvalidManifest))?;
        validate_update_offer_identity(&offer)?;
        Ok(UpdateStatus {
            state: parse_state(&row.0)?,
            offer: Some(offer),
            decision_history: load_update_decision_history(&connection, update_id)?,
            diagnostic: row.2.as_deref().map(parse_diagnostic).transpose()?,
        })
    }

    fn has_exact_verified_backups(&self) -> Result<bool, UpdateCommandError> {
        let tenders = self
            .tender_summaries_for_update()
            .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::VerifiedBackupRequired))?;
        for tender in tenders {
            let tender_id = crate::tender_store::TenderId::parse(&tender.tender_id)
                .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::VerifiedBackupRequired))?;
            if !self
                .has_exact_verified_backup_for_update(&tender_id, &tender)
                .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::VerifiedBackupRequired))?
            {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

fn validate_compatibility(offer: &UpdateOffer) -> Result<(), UpdateCommandError> {
    let current = Version::parse(&offer.current_version)
        .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InvalidManifest))?;
    let offered = Version::parse(&offer.version)
        .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InvalidManifest))?;
    if offered <= current {
        return Err(UpdateCommandError::new(UpdateDiagnostic::DowngradeRejected));
    }
    if Some(offer.platform) != UpdatePlatform::current() {
        return Err(UpdateCommandError::new(
            UpdateDiagnostic::UnsupportedPlatform,
        ));
    }
    let compatibility = &offer.compatibility;
    if compatibility.installation_schema_version != INSTALLATION_SCHEMA_VERSION as u32 {
        return Err(UpdateCommandError::new(
            UpdateDiagnostic::InstallationSchemaIncompatible,
        ));
    }
    if compatibility.tender_schema_version != TENDER_SCHEMA_VERSION as u32 {
        return Err(UpdateCommandError::new(
            UpdateDiagnostic::TenderStoreIncompatible,
        ));
    }
    if compatibility.codex_version != CODEX_VERSION {
        return Err(UpdateCommandError::new(UpdateDiagnostic::CodexIncompatible));
    }
    if compatibility.docling_version != DOCLING_VERSION {
        return Err(UpdateCommandError::new(
            UpdateDiagnostic::DoclingIncompatible,
        ));
    }
    if compatibility.runtime_manifest_schema_version != RUNTIME_PROVENANCE_SCHEMA {
        return Err(UpdateCommandError::new(
            UpdateDiagnostic::RuntimeIncompatible,
        ));
    }
    Ok(())
}

fn update_decision_code(decision: UpdateDecision) -> &'static str {
    match decision {
        UpdateDecision::Approve => "approve",
        UpdateDecision::Deny => "deny",
    }
}

fn parse_update_decision(value: &str) -> Result<UpdateDecision, UpdateCommandError> {
    match value {
        "approve" => Ok(UpdateDecision::Approve),
        "deny" => Ok(UpdateDecision::Deny),
        _ => Err(UpdateCommandError::new(UpdateDiagnostic::InvalidManifest)),
    }
}

fn update_decision_payload(record: &UpdateDecisionRecord) -> Result<String, UpdateCommandError> {
    serde_json_canonicalizer::to_string(&serde_json::json!({
        "acting_role": record.acting_role.as_str(),
        "decided_at": record.decided_at.as_str(),
        "decided_by": record.decided_by.as_str(),
        "decision": update_decision_code(record.decision),
        "offer_sha256": record.offer_sha256.as_str(),
        "preceding_hash": record.preceding_hash.as_str(),
        "rationale": record.rationale.as_str(),
        "schema_version": "1",
        "sequence": record.sequence.to_string(),
        "update_id": record.update_id.as_str(),
    }))
    .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::UpdaterUnavailable))
}

fn append_update_decision(
    transaction: &rusqlite::Transaction<'_>,
    update_id: &str,
    decision: UpdateDecision,
    rationale: &str,
    decided_by: &str,
) -> Result<(), UpdateCommandError> {
    let previous: Option<(i64, String)> = transaction
        .query_row(
            "SELECT sequence, current_hash FROM update_decisions ORDER BY sequence DESC LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::UpdaterUnavailable))?;
    let sequence = previous
        .as_ref()
        .map_or(1_i64, |(sequence, _)| sequence + 1);
    let sequence_u32 = u32::try_from(sequence)
        .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::UpdaterUnavailable))?;
    let decided_at: String = transaction
        .query_row("SELECT strftime('%Y-%m-%dT%H:%M:%fZ', 'now')", [], |row| {
            row.get(0)
        })
        .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::UpdaterUnavailable))?;
    let preceding_hash = previous
        .map(|(_, hash)| hash)
        .unwrap_or_else(|| ZERO_UPDATE_DECISION_HASH.to_owned());
    let mut record = UpdateDecisionRecord {
        sequence: sequence_u32,
        update_id: update_id.to_owned(),
        offer_sha256: update_id.to_owned(),
        decision,
        rationale: rationale.to_owned(),
        decided_by: decided_by.to_owned(),
        acting_role: "tendering_manager".to_owned(),
        decided_at,
        preceding_hash,
        current_hash: String::new(),
    };
    record.current_hash = sha256_hex(update_decision_payload(&record)?.as_bytes());
    transaction
        .execute(
            "INSERT INTO update_decisions (
               sequence, update_id, offer_sha256, decision, rationale, decided_by,
               acting_role, decided_at, preceding_hash, current_hash
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                sequence,
                record.update_id,
                record.offer_sha256,
                update_decision_code(record.decision),
                record.rationale,
                record.decided_by,
                record.acting_role,
                record.decided_at,
                record.preceding_hash,
                record.current_hash,
            ],
        )
        .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::UpdaterUnavailable))?;
    Ok(())
}

fn load_update_decision_history(
    connection: &Connection,
    requested_update_id: &str,
) -> Result<Vec<UpdateDecisionRecord>, UpdateCommandError> {
    let mut statement = connection
        .prepare(
            "SELECT sequence, update_id, offer_sha256, decision, rationale, decided_by,
                    acting_role, decided_at, preceding_hash, current_hash
             FROM update_decisions ORDER BY sequence",
        )
        .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::UpdaterUnavailable))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, String>(9)?,
            ))
        })
        .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::UpdaterUnavailable))?;
    let mut expected_preceding = ZERO_UPDATE_DECISION_HASH.to_owned();
    let mut requested = Vec::new();
    for (expected_sequence, row) in (1_i64..).zip(rows) {
        let row = row.map_err(|_| UpdateCommandError::new(UpdateDiagnostic::UpdaterUnavailable))?;
        let record = UpdateDecisionRecord {
            sequence: u32::try_from(row.0)
                .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InvalidManifest))?,
            update_id: row.1,
            offer_sha256: row.2,
            decision: parse_update_decision(&row.3)?,
            rationale: row.4,
            decided_by: row.5,
            acting_role: row.6,
            decided_at: row.7,
            preceding_hash: row.8,
            current_hash: row.9,
        };
        if row.0 != expected_sequence
            || record.update_id != record.offer_sha256
            || !valid_sha256_value(&record.update_id)
            || record.rationale.trim().is_empty()
            || record.rationale.len() > 4_000
            || record.decided_by != "engineer_user"
            || record.acting_role != "tendering_manager"
            || record.decided_at.is_empty()
            || record.preceding_hash != expected_preceding
            || sha256_hex(update_decision_payload(&record)?.as_bytes()) != record.current_hash
        {
            return Err(UpdateCommandError::new(UpdateDiagnostic::InvalidManifest));
        }
        expected_preceding = record.current_hash.clone();
        if record.update_id == requested_update_id {
            requested.push(record);
        }
    }
    Ok(requested)
}

fn update_connection(application_home: &Path) -> Result<Connection, UpdateCommandError> {
    let connection = Connection::open_with_flags(
        application_home.join(INSTALLATION_DATABASE),
        OpenFlags::SQLITE_OPEN_READ_WRITE,
    )
    .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::UpdaterUnavailable))?;
    connection
        .busy_timeout(std::time::Duration::from_secs(5))
        .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::UpdaterUnavailable))?;
    Ok(connection)
}

pub(crate) fn installed_application_artifact_for_recovery(
) -> Result<InstalledApplicationArtifactSet, UpdateCommandError> {
    #[cfg(target_os = "windows")]
    {
        let executable = std::env::current_exe()
            .and_then(|path| path.canonicalize())
            .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::UpdaterUnavailable));
        let executable = executable?;
        let root = executable
            .parent()
            .ok_or_else(|| UpdateCommandError::new(UpdateDiagnostic::UpdaterUnavailable))?
            .to_path_buf();
        if !root.join("runtime").is_dir() {
            return Err(UpdateCommandError::new(
                UpdateDiagnostic::UpdaterUnavailable,
            ));
        }
        Ok(InstalledApplicationArtifactSet {
            kind: InstalledApplicationArtifactKind::WindowsBundle,
            launcher_relative: Some(
                executable
                    .strip_prefix(&root)
                    .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::UpdaterUnavailable))?
                    .to_path_buf(),
            ),
            root,
        })
    }
    #[cfg(target_os = "macos")]
    {
        let executable = std::env::current_exe()
            .and_then(|path| path.canonicalize())
            .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::UpdaterUnavailable))?;
        let root = executable
            .ancestors()
            .find(|path| path.extension().is_some_and(|extension| extension == "app"))
            .ok_or_else(|| UpdateCommandError::new(UpdateDiagnostic::UpdaterUnavailable))?
            .to_path_buf();
        if !root.join("Contents/Resources/runtime").is_dir() {
            return Err(UpdateCommandError::new(
                UpdateDiagnostic::UpdaterUnavailable,
            ));
        }
        Ok(InstalledApplicationArtifactSet {
            kind: InstalledApplicationArtifactKind::MacOsBundle,
            launcher_relative: Some(
                executable
                    .strip_prefix(&root)
                    .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::UpdaterUnavailable))?
                    .to_path_buf(),
            ),
            root,
        })
    }
    #[cfg(target_os = "linux")]
    {
        let app_image = std::env::var_os("APPIMAGE")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .ok_or_else(|| UpdateCommandError::new(UpdateDiagnostic::UpdaterUnavailable))?;
        let metadata = fs::symlink_metadata(&app_image)
            .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::UpdaterUnavailable))?;
        if metadata.file_type().is_symlink()
            || !metadata.is_file()
            || app_image.canonicalize().ok().as_ref() != Some(&app_image)
        {
            return Err(UpdateCommandError::new(
                UpdateDiagnostic::UpdaterUnavailable,
            ));
        }
        Ok(InstalledApplicationArtifactSet {
            kind: InstalledApplicationArtifactKind::LinuxAppImage,
            root: app_image,
            launcher_relative: None,
        })
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        Err(UpdateCommandError::new(
            UpdateDiagnostic::UpdaterUnavailable,
        ))
    }
}

fn require_repair_required(
    application_home: &Path,
    update_id: &str,
) -> Result<(), UpdateCommandError> {
    if !valid_sha256_value(update_id) {
        return Err(UpdateCommandError::new(
            UpdateDiagnostic::InstallationFailed,
        ));
    }
    let state = update_connection(application_home)?
        .query_row(
            "SELECT state FROM update_operations WHERE update_id = ?1",
            [update_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?;
    if state.as_deref() != Some("repair_required") {
        return Err(UpdateCommandError::new(
            UpdateDiagnostic::InstallationFailed,
        ));
    }
    Ok(())
}

fn application_recovery_parent(application_home: &Path) -> PathBuf {
    application_home.join(APPLICATION_RECOVERY_DIRECTORY)
}

fn ensure_private_recovery_directory(path: &Path) -> Result<(), UpdateCommandError> {
    if path.exists() {
        let metadata = fs::symlink_metadata(path)
            .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(UpdateCommandError::new(
                UpdateDiagnostic::InstallationFailed,
            ));
        }
        return Ok(());
    }
    fs::create_dir(path).map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))
}

fn valid_recovery_relative_path(path: &Path) -> bool {
    let mut count = 0_usize;
    for component in path.components() {
        if !matches!(component, Component::Normal(_)) {
            return false;
        }
        count += 1;
        if count > 64 {
            return false;
        }
    }
    count > 0
}

fn build_application_recovery_manifest(
    update_id: &str,
    application_version: &str,
    artifact_set: &InstalledApplicationArtifactSet,
) -> Result<ApplicationRecoveryManifest, UpdateCommandError> {
    if !artifact_set.root.is_absolute() {
        return Err(UpdateCommandError::new(
            UpdateDiagnostic::InstallationFailed,
        ));
    }
    let original_metadata = fs::symlink_metadata(&artifact_set.root)
        .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?;
    if original_metadata.file_type().is_symlink() {
        return Err(UpdateCommandError::new(
            UpdateDiagnostic::InstallationFailed,
        ));
    }
    let destination_root = artifact_set
        .root
        .canonicalize()
        .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?;
    let mut entries = Vec::new();
    match artifact_set.kind {
        InstalledApplicationArtifactKind::LinuxAppImage => {
            if !original_metadata.is_file()
                || original_metadata.len() == 0
                || original_metadata.len() > MAX_APPLICATION_ARTIFACT_BYTES
                || artifact_set.launcher_relative.is_some()
            {
                return Err(UpdateCommandError::new(
                    UpdateDiagnostic::InstallationFailed,
                ));
            }
            entries.push(ApplicationRecoveryEntry {
                relative_path: PathBuf::from("application"),
                kind: ApplicationRecoveryEntryKind::File,
                size_bytes: Some(original_metadata.len()),
                sha256: Some(sha256_file(&destination_root)?),
            });
        }
        InstalledApplicationArtifactKind::WindowsBundle
        | InstalledApplicationArtifactKind::MacOsBundle => {
            let required_runtime = match artifact_set.kind {
                InstalledApplicationArtifactKind::WindowsBundle => destination_root.join("runtime"),
                InstalledApplicationArtifactKind::MacOsBundle => {
                    destination_root.join("Contents/Resources/runtime")
                }
                InstalledApplicationArtifactKind::LinuxAppImage => unreachable!(),
            };
            if !original_metadata.is_dir() || !required_runtime.is_dir() {
                return Err(UpdateCommandError::new(
                    UpdateDiagnostic::InstallationFailed,
                ));
            }
            let launcher = artifact_set
                .launcher_relative
                .as_ref()
                .filter(|path| valid_recovery_relative_path(path))
                .ok_or_else(|| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?;
            let launcher_path = destination_root.join(launcher);
            let launcher_metadata = fs::symlink_metadata(&launcher_path)
                .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?;
            if launcher_metadata.file_type().is_symlink()
                || !launcher_metadata.is_file()
                || launcher_metadata.len() == 0
                || launcher_path.canonicalize().ok().as_ref() != Some(&launcher_path)
            {
                return Err(UpdateCommandError::new(
                    UpdateDiagnostic::InstallationFailed,
                ));
            }
            let mut total_size = 0_u64;
            for walked in walkdir::WalkDir::new(&destination_root)
                .follow_links(false)
                .sort_by_file_name()
            {
                let walked = walked
                    .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?;
                if walked.path() == destination_root {
                    continue;
                }
                if entries.len() >= MAX_APPLICATION_ARTIFACT_ENTRIES {
                    return Err(UpdateCommandError::new(
                        UpdateDiagnostic::InstallationFailed,
                    ));
                }
                let relative_path = walked
                    .path()
                    .strip_prefix(&destination_root)
                    .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?
                    .to_path_buf();
                if !valid_recovery_relative_path(&relative_path) || walked.file_type().is_symlink()
                {
                    return Err(UpdateCommandError::new(
                        UpdateDiagnostic::InstallationFailed,
                    ));
                }
                if walked.file_type().is_dir() {
                    entries.push(ApplicationRecoveryEntry {
                        relative_path,
                        kind: ApplicationRecoveryEntryKind::Directory,
                        size_bytes: None,
                        sha256: None,
                    });
                } else if walked.file_type().is_file() {
                    let size_bytes = walked
                        .metadata()
                        .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?
                        .len();
                    total_size = total_size
                        .checked_add(size_bytes)
                        .filter(|size| *size <= MAX_APPLICATION_ARTIFACT_BYTES)
                        .ok_or_else(|| {
                            UpdateCommandError::new(UpdateDiagnostic::InstallationFailed)
                        })?;
                    entries.push(ApplicationRecoveryEntry {
                        relative_path,
                        kind: ApplicationRecoveryEntryKind::File,
                        size_bytes: Some(size_bytes),
                        sha256: Some(sha256_file(walked.path())?),
                    });
                } else {
                    return Err(UpdateCommandError::new(
                        UpdateDiagnostic::InstallationFailed,
                    ));
                }
            }
            if !entries.iter().any(|entry| {
                entry.kind == ApplicationRecoveryEntryKind::File && entry.relative_path == *launcher
            }) {
                return Err(UpdateCommandError::new(
                    UpdateDiagnostic::InstallationFailed,
                ));
            }
        }
    }
    Ok(ApplicationRecoveryManifest {
        update_id: update_id.to_owned(),
        application_version: application_version.to_owned(),
        artifact_kind: artifact_set.kind,
        destination_root,
        launcher_relative: artifact_set.launcher_relative.clone(),
        entries,
    })
}

fn recovery_entry_artifact_path(root: &Path, entry: &ApplicationRecoveryEntry) -> PathBuf {
    root.join("artifacts").join(&entry.relative_path)
}

fn copy_application_recovery_artifacts(
    artifact_set: &InstalledApplicationArtifactSet,
    manifest: &ApplicationRecoveryManifest,
    staging: &Path,
) -> Result<(), UpdateCommandError> {
    let artifacts = staging.join("artifacts");
    fs::create_dir(&artifacts)
        .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?;
    for entry in &manifest.entries {
        let destination = recovery_entry_artifact_path(staging, entry);
        match entry.kind {
            ApplicationRecoveryEntryKind::Directory => fs::create_dir_all(&destination)
                .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?,
            ApplicationRecoveryEntryKind::File => {
                let source = match manifest.artifact_kind {
                    InstalledApplicationArtifactKind::LinuxAppImage => artifact_set.root.clone(),
                    InstalledApplicationArtifactKind::WindowsBundle
                    | InstalledApplicationArtifactKind::MacOsBundle => {
                        manifest.destination_root.join(&entry.relative_path)
                    }
                };
                if let Some(parent) = destination.parent() {
                    fs::create_dir_all(parent).map_err(|_| {
                        UpdateCommandError::new(UpdateDiagnostic::InstallationFailed)
                    })?;
                }
                let source_metadata = fs::metadata(&source)
                    .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?;
                let mut source_file = fs::File::open(&source)
                    .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?;
                let mut destination_file = OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(&destination)
                    .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?;
                let copied = io::copy(&mut source_file, &mut destination_file)
                    .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?;
                destination_file
                    .sync_all()
                    .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?;
                drop(destination_file);
                if copied != entry.size_bytes.expect("validated recovery file size") {
                    return Err(UpdateCommandError::new(UpdateDiagnostic::ArtifactTampered));
                }
                fs::set_permissions(&destination, source_metadata.permissions())
                    .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?;
            }
        }
    }
    Ok(())
}

fn write_durable_recovery_manifest(
    staging: &Path,
    manifest_json: &str,
) -> Result<(), UpdateCommandError> {
    let path = staging.join("manifest.json");
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?;
    use std::io::Write;
    file.write_all(manifest_json.as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))
}

#[cfg(target_family = "unix")]
fn publish_recovery_directory(
    staging: &Path,
    final_root: &Path,
    recovery_parent: &Path,
) -> Result<(), UpdateCommandError> {
    let mut directories = walkdir::WalkDir::new(staging)
        .follow_links(false)
        .into_iter()
        .map(|entry| {
            entry.map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))
        })
        .collect::<Result<Vec<_>, _>>()?;
    directories.retain(|entry| entry.file_type().is_dir());
    directories.sort_by_key(|entry| std::cmp::Reverse(entry.depth()));
    for directory in directories {
        fs::File::open(directory.path())
            .and_then(|directory| directory.sync_all())
            .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?;
    }
    fs::rename(staging, final_root)
        .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?;
    fs::File::open(recovery_parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))
}

#[cfg(windows)]
fn publish_recovery_directory(
    staging: &Path,
    final_root: &Path,
    _recovery_parent: &Path,
) -> Result<(), UpdateCommandError> {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_WRITE_THROUGH};
    use windows_core::PCWSTR;

    let staging = staging
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let final_root = final_root
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    unsafe {
        MoveFileExW(
            PCWSTR(staging.as_ptr()),
            PCWSTR(final_root.as_ptr()),
            MOVEFILE_WRITE_THROUGH,
        )
    }
    .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))
}

#[cfg(not(any(target_family = "unix", windows)))]
fn publish_recovery_directory(
    staging: &Path,
    final_root: &Path,
    _recovery_parent: &Path,
) -> Result<(), UpdateCommandError> {
    fs::rename(staging, final_root)
        .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))
}

fn validate_application_recovery_manifest(
    manifest: &ApplicationRecoveryManifest,
) -> Result<(), UpdateCommandError> {
    if !valid_sha256_value(&manifest.update_id)
        || Version::parse(&manifest.application_version).is_err()
        || !manifest.destination_root.is_absolute()
        || manifest.entries.is_empty()
        || manifest.entries.len() > MAX_APPLICATION_ARTIFACT_ENTRIES
    {
        return Err(UpdateCommandError::new(
            UpdateDiagnostic::InstallationFailed,
        ));
    }
    let mut seen = BTreeSet::new();
    let mut total_size = 0_u64;
    for entry in &manifest.entries {
        if !valid_recovery_relative_path(&entry.relative_path)
            || !seen.insert(entry.relative_path.clone())
        {
            return Err(UpdateCommandError::new(
                UpdateDiagnostic::InstallationFailed,
            ));
        }
        match entry.kind {
            ApplicationRecoveryEntryKind::Directory
                if entry.size_bytes.is_none() && entry.sha256.is_none() => {}
            ApplicationRecoveryEntryKind::File => {
                let size = entry
                    .size_bytes
                    .ok_or_else(|| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?;
                let sha256 = entry
                    .sha256
                    .as_deref()
                    .filter(|hash| valid_sha256_value(hash))
                    .ok_or_else(|| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?;
                let _ = sha256;
                total_size = total_size
                    .checked_add(size)
                    .filter(|size| *size <= MAX_APPLICATION_ARTIFACT_BYTES)
                    .ok_or_else(|| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?;
            }
            _ => {
                return Err(UpdateCommandError::new(
                    UpdateDiagnostic::InstallationFailed,
                ))
            }
        }
    }
    match manifest.artifact_kind {
        InstalledApplicationArtifactKind::LinuxAppImage => {
            if manifest.launcher_relative.is_some()
                || manifest.entries.len() != 1
                || manifest.entries[0].kind != ApplicationRecoveryEntryKind::File
                || manifest.entries[0].relative_path != Path::new("application")
            {
                return Err(UpdateCommandError::new(
                    UpdateDiagnostic::InstallationFailed,
                ));
            }
        }
        InstalledApplicationArtifactKind::WindowsBundle
        | InstalledApplicationArtifactKind::MacOsBundle => {
            let launcher = manifest
                .launcher_relative
                .as_ref()
                .filter(|path| valid_recovery_relative_path(path))
                .ok_or_else(|| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?;
            if !manifest.entries.iter().any(|entry| {
                entry.kind == ApplicationRecoveryEntryKind::File
                    && entry.relative_path == *launcher
                    && entry.size_bytes.is_some_and(|size| size > 0)
            }) {
                return Err(UpdateCommandError::new(
                    UpdateDiagnostic::InstallationFailed,
                ));
            }
        }
    }
    Ok(())
}

fn recovery_artifacts_verify(root: &Path, manifest: &ApplicationRecoveryManifest) -> bool {
    if validate_application_recovery_manifest(manifest).is_err()
        || !manifest.entries.iter().all(|entry| {
            let path = recovery_entry_artifact_path(root, entry);
            match entry.kind {
                ApplicationRecoveryEntryKind::Directory => fs::symlink_metadata(path)
                    .is_ok_and(|metadata| !metadata.file_type().is_symlink() && metadata.is_dir()),
                ApplicationRecoveryEntryKind::File => verified_file_matches(
                    &path,
                    entry.size_bytes.expect("validated recovery file size"),
                    entry
                        .sha256
                        .as_deref()
                        .expect("validated recovery file hash"),
                ),
            }
        })
    {
        return false;
    }
    let expected = manifest
        .entries
        .iter()
        .map(|entry| entry.relative_path.clone())
        .collect::<BTreeSet<_>>();
    let artifacts = root.join("artifacts");
    let mut actual = BTreeSet::new();
    for walked in walkdir::WalkDir::new(&artifacts).follow_links(false) {
        let Ok(walked) = walked else {
            return false;
        };
        if walked.path() == artifacts {
            continue;
        }
        let Ok(relative) = walked.path().strip_prefix(&artifacts) else {
            return false;
        };
        if walked.file_type().is_symlink() || !actual.insert(relative.to_path_buf()) {
            return false;
        }
    }
    actual == expected
}

fn load_application_recovery_manifest(
    application_home: &Path,
    update_id: &str,
) -> Result<ApplicationRecoveryManifest, UpdateCommandError> {
    if !valid_sha256_value(update_id) {
        return Err(UpdateCommandError::new(
            UpdateDiagnostic::InstallationFailed,
        ));
    }
    let connection = update_connection(application_home)?;
    let recovery_fact = connection
        .query_row(
            "SELECT application_version, artifact_kind, destination_root,
                    manifest_sha256, manifest_json
             FROM update_recovery_points WHERE update_id = ?1",
            [update_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            },
        )
        .optional()
        .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?
        .ok_or_else(|| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?;
    let final_root = application_recovery_parent(application_home).join(update_id);
    let interrupted_staging = application_home
        .join("staging")
        .join(format!("update-recovery-{update_id}"));
    let root = if final_root.exists() {
        final_root.clone()
    } else {
        interrupted_staging.clone()
    };
    let metadata = fs::symlink_metadata(&root)
        .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(UpdateCommandError::new(
            UpdateDiagnostic::InstallationFailed,
        ));
    }
    let bytes = fs::read(root.join("manifest.json"))
        .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?;
    let canonical_manifest = String::from_utf8(bytes.clone())
        .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?;
    let manifest: ApplicationRecoveryManifest = serde_json::from_slice(&bytes)
        .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?;
    validate_application_recovery_manifest(&manifest)?;
    if manifest.update_id != update_id
        || manifest.application_version != recovery_fact.0
        || manifest.artifact_kind.database_value() != recovery_fact.1
        || manifest.destination_root.as_path() != Path::new(&recovery_fact.2)
        || sha256_hex(canonical_manifest.as_bytes()) != recovery_fact.3
        || canonical_manifest != recovery_fact.4
        || serde_json_canonicalizer::to_string(&manifest)
            .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?
            != canonical_manifest
    {
        return Err(UpdateCommandError::new(
            UpdateDiagnostic::InstallationFailed,
        ));
    }
    if root == interrupted_staging {
        publish_recovery_directory(
            &interrupted_staging,
            &final_root,
            &application_recovery_parent(application_home),
        )?;
    }
    let parent = manifest
        .destination_root
        .parent()
        .ok_or_else(|| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?;
    if parent.canonicalize().ok().as_deref() != Some(parent) {
        return Err(UpdateCommandError::new(
            UpdateDiagnostic::InstallationFailed,
        ));
    }
    match fs::symlink_metadata(&manifest.destination_root) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(UpdateCommandError::new(
                UpdateDiagnostic::InstallationFailed,
            ));
        }
        Ok(_) => {
            if manifest.destination_root.canonicalize().ok().as_ref()
                != Some(&manifest.destination_root)
            {
                return Err(UpdateCommandError::new(
                    UpdateDiagnostic::InstallationFailed,
                ));
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(_) => {
            return Err(UpdateCommandError::new(
                UpdateDiagnostic::InstallationFailed,
            ))
        }
    }
    Ok(manifest)
}

fn restore_application_recovery_point(
    application_home: &Path,
    update_id: &str,
) -> Result<PathBuf, UpdateCommandError> {
    let manifest = load_application_recovery_manifest(application_home, update_id)?;
    let root = application_recovery_parent(application_home).join(update_id);
    if !recovery_artifacts_verify(&root, &manifest) {
        return Err(UpdateCommandError::new(UpdateDiagnostic::ArtifactTampered));
    }
    match manifest.artifact_kind {
        InstalledApplicationArtifactKind::LinuxAppImage => {
            let entry = &manifest.entries[0];
            restore_recovery_file(
                &recovery_entry_artifact_path(&root, entry),
                &manifest.destination_root,
                entry,
                update_id,
            )?;
        }
        InstalledApplicationArtifactKind::WindowsBundle
        | InstalledApplicationArtifactKind::MacOsBundle => {
            ensure_restore_bundle_root(&manifest.destination_root)?;
            for entry in manifest
                .entries
                .iter()
                .filter(|entry| entry.kind == ApplicationRecoveryEntryKind::Directory)
            {
                ensure_restore_directory(&manifest.destination_root.join(&entry.relative_path))?;
            }
            for entry in manifest
                .entries
                .iter()
                .filter(|entry| entry.kind == ApplicationRecoveryEntryKind::File)
            {
                restore_recovery_file(
                    &recovery_entry_artifact_path(&root, entry),
                    &manifest.destination_root.join(&entry.relative_path),
                    entry,
                    update_id,
                )?;
            }
            remove_unexpected_application_artifacts(&manifest)?;
        }
    }
    if !application_destination_matches(&manifest) {
        return Err(UpdateCommandError::new(
            UpdateDiagnostic::InstallationFailed,
        ));
    }
    Ok(match manifest.artifact_kind {
        InstalledApplicationArtifactKind::LinuxAppImage => manifest.destination_root,
        InstalledApplicationArtifactKind::WindowsBundle
        | InstalledApplicationArtifactKind::MacOsBundle => manifest.destination_root.join(
            manifest
                .launcher_relative
                .expect("validated application bundle launcher"),
        ),
    })
}

fn ensure_restore_bundle_root(destination: &Path) -> Result<(), UpdateCommandError> {
    match fs::symlink_metadata(destination) {
        Ok(metadata) if !metadata.file_type().is_symlink() && metadata.is_dir() => {
            if destination.canonicalize().ok().as_ref() == Some(&destination.to_path_buf()) {
                Ok(())
            } else {
                Err(UpdateCommandError::new(
                    UpdateDiagnostic::InstallationFailed,
                ))
            }
        }
        Ok(metadata) if !metadata.file_type().is_symlink() && metadata.is_file() => {
            fs::remove_file(destination)
                .and_then(|()| fs::create_dir(destination))
                .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))
        }
        Ok(_) => Err(UpdateCommandError::new(
            UpdateDiagnostic::InstallationFailed,
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => fs::create_dir(destination)
            .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed)),
        Err(_) => Err(UpdateCommandError::new(
            UpdateDiagnostic::InstallationFailed,
        )),
    }
}

fn ensure_restore_directory(destination: &Path) -> Result<(), UpdateCommandError> {
    let parent = destination
        .parent()
        .ok_or_else(|| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?;
    if parent.canonicalize().ok().as_deref() != Some(parent) {
        return Err(UpdateCommandError::new(
            UpdateDiagnostic::InstallationFailed,
        ));
    }
    match fs::symlink_metadata(destination) {
        Ok(metadata) if !metadata.file_type().is_symlink() && metadata.is_dir() => Ok(()),
        Ok(metadata) if !metadata.file_type().is_symlink() && metadata.is_file() => {
            fs::remove_file(destination)
                .and_then(|()| fs::create_dir(destination))
                .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))
        }
        Ok(_) => Err(UpdateCommandError::new(
            UpdateDiagnostic::InstallationFailed,
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => fs::create_dir(destination)
            .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed)),
        Err(_) => Err(UpdateCommandError::new(
            UpdateDiagnostic::InstallationFailed,
        )),
    }
}

fn restore_recovery_file(
    artifact: &Path,
    destination: &Path,
    entry: &ApplicationRecoveryEntry,
    update_id: &str,
) -> Result<(), UpdateCommandError> {
    if let Ok(metadata) = fs::symlink_metadata(destination) {
        if metadata.file_type().is_symlink() || (!metadata.is_file() && !metadata.is_dir()) {
            return Err(UpdateCommandError::new(
                UpdateDiagnostic::InstallationFailed,
            ));
        }
    }
    let parent = destination
        .parent()
        .ok_or_else(|| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?;
    if parent.canonicalize().ok().as_deref() != Some(parent) {
        return Err(UpdateCommandError::new(
            UpdateDiagnostic::InstallationFailed,
        ));
    }
    let expected_size = entry
        .size_bytes
        .ok_or_else(|| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?;
    let expected_sha256 = entry
        .sha256
        .as_deref()
        .ok_or_else(|| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?;
    for attempt in 0..300 {
        match replace_application_artifact_atomically(
            artifact,
            destination,
            update_id,
            attempt,
            expected_size,
            expected_sha256,
        ) {
            Ok(()) if verified_file_matches(destination, expected_size, expected_sha256) => {
                return Ok(())
            }
            Ok(()) => return Err(UpdateCommandError::new(UpdateDiagnostic::ArtifactTampered)),
            Err(error) if error.diagnostic == UpdateDiagnostic::ArtifactTampered => {
                return Err(error)
            }
            Err(_) => {}
        }
        thread::sleep(Duration::from_millis(100));
    }
    Err(UpdateCommandError::new(
        UpdateDiagnostic::InstallationFailed,
    ))
}

fn remove_unexpected_application_artifacts(
    manifest: &ApplicationRecoveryManifest,
) -> Result<(), UpdateCommandError> {
    let expected = manifest
        .entries
        .iter()
        .map(|entry| entry.relative_path.clone())
        .collect::<BTreeSet<_>>();
    let mut unexpected = Vec::new();
    for walked in walkdir::WalkDir::new(&manifest.destination_root)
        .follow_links(false)
        .contents_first(true)
    {
        let walked =
            walked.map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?;
        if walked.path() == manifest.destination_root {
            continue;
        }
        if walked.file_type().is_symlink() {
            return Err(UpdateCommandError::new(
                UpdateDiagnostic::InstallationFailed,
            ));
        }
        let relative = walked
            .path()
            .strip_prefix(&manifest.destination_root)
            .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?
            .to_path_buf();
        if !valid_recovery_relative_path(&relative) {
            return Err(UpdateCommandError::new(
                UpdateDiagnostic::InstallationFailed,
            ));
        }
        if !expected.contains(&relative) {
            unexpected.push((walked.path().to_path_buf(), walked.file_type().is_dir()));
        }
    }
    for (path, directory) in unexpected {
        if directory {
            fs::remove_dir(&path)
                .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?;
        } else {
            fs::remove_file(&path)
                .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?;
        }
    }
    Ok(())
}

fn replace_application_artifact_atomically(
    artifact: &Path,
    destination: &Path,
    update_id: &str,
    attempt: u16,
    expected_size: u64,
    expected_sha256: &str,
) -> Result<(), UpdateCommandError> {
    let parent = destination
        .parent()
        .ok_or_else(|| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?;
    let staging = parent.join(format!(
        ".quantix-restore-{}-{}-{attempt}.tmp",
        &update_id[..12],
        std::process::id()
    ));
    let result = (|| {
        let mut source = fs::File::open(artifact)
            .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?;
        let mut staged = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&staging)
            .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?;
        let copied = io::copy(&mut source, &mut staged)
            .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?;
        if copied != expected_size || copied > MAX_APPLICATION_ARTIFACT_BYTES {
            return Err(UpdateCommandError::new(
                UpdateDiagnostic::InstallationFailed,
            ));
        }
        staged
            .sync_all()
            .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?;
        drop(staged);
        fs::set_permissions(
            &staging,
            fs::metadata(artifact)
                .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?
                .permissions(),
        )
        .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?;
        if !verified_file_matches(&staging, expected_size, expected_sha256) {
            return Err(UpdateCommandError::new(UpdateDiagnostic::ArtifactTampered));
        }
        if fs::symlink_metadata(destination).is_ok_and(|metadata| metadata.is_dir()) {
            fs::remove_dir_all(destination)
                .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?;
        }
        atomic_replace_file(destination, &staging)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&staging);
    }
    result
}

#[cfg(windows)]
fn atomic_replace_file(destination: &Path, replacement: &Path) -> Result<(), UpdateCommandError> {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Storage::FileSystem::{
        MoveFileExW, ReplaceFileW, MOVEFILE_WRITE_THROUGH, REPLACEFILE_WRITE_THROUGH,
    };
    use windows_core::PCWSTR;

    let destination_path = destination;
    let replacement_path = replacement;
    let destination: Vec<u16> = destination_path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let replacement: Vec<u16> = replacement_path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    if fs::symlink_metadata(destination_path)
        .is_err_and(|error| error.kind() == io::ErrorKind::NotFound)
    {
        return unsafe {
            MoveFileExW(
                PCWSTR(replacement.as_ptr()),
                PCWSTR(destination.as_ptr()),
                MOVEFILE_WRITE_THROUGH,
            )
        }
        .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed));
    }
    unsafe {
        ReplaceFileW(
            PCWSTR(destination.as_ptr()),
            PCWSTR(replacement.as_ptr()),
            PCWSTR::null(),
            REPLACEFILE_WRITE_THROUGH,
            None,
            None,
        )
    }
    .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))
}

#[cfg(not(windows))]
fn atomic_replace_file(destination: &Path, replacement: &Path) -> Result<(), UpdateCommandError> {
    fs::rename(replacement, destination)
        .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))
}

fn application_recovery_is_restored(application_home: &Path, update_id: &str) -> bool {
    load_application_recovery_manifest(application_home, update_id)
        .ok()
        .is_some_and(|manifest| application_destination_matches(&manifest))
}

fn application_destination_matches(manifest: &ApplicationRecoveryManifest) -> bool {
    match manifest.artifact_kind {
        InstalledApplicationArtifactKind::LinuxAppImage => {
            let entry = &manifest.entries[0];
            verified_file_matches(
                &manifest.destination_root,
                entry.size_bytes.expect("validated AppImage size"),
                entry.sha256.as_deref().expect("validated AppImage hash"),
            )
        }
        InstalledApplicationArtifactKind::WindowsBundle
        | InstalledApplicationArtifactKind::MacOsBundle => {
            let root_metadata = fs::symlink_metadata(&manifest.destination_root);
            if root_metadata.is_err()
                || root_metadata
                    .is_ok_and(|metadata| metadata.file_type().is_symlink() || !metadata.is_dir())
            {
                return false;
            }
            let expected = manifest
                .entries
                .iter()
                .map(|entry| entry.relative_path.clone())
                .collect::<BTreeSet<_>>();
            let mut actual = BTreeSet::new();
            for walked in walkdir::WalkDir::new(&manifest.destination_root).follow_links(false) {
                let Ok(walked) = walked else {
                    return false;
                };
                if walked.path() == manifest.destination_root {
                    continue;
                }
                let Ok(relative) = walked.path().strip_prefix(&manifest.destination_root) else {
                    return false;
                };
                if !actual.insert(relative.to_path_buf()) {
                    return false;
                }
                let Some(entry) = manifest
                    .entries
                    .iter()
                    .find(|entry| entry.relative_path == relative)
                else {
                    return false;
                };
                let matches = match entry.kind {
                    ApplicationRecoveryEntryKind::Directory => {
                        !walked.file_type().is_symlink() && walked.file_type().is_dir()
                    }
                    ApplicationRecoveryEntryKind::File => verified_file_matches(
                        walked.path(),
                        entry.size_bytes.expect("validated recovery file size"),
                        entry
                            .sha256
                            .as_deref()
                            .expect("validated recovery file hash"),
                    ),
                };
                if !matches {
                    return false;
                }
            }
            actual == expected
        }
    }
}

fn cleanup_application_recovery(
    application_home: &Path,
    update_id: &str,
) -> Result<(), UpdateCommandError> {
    if !valid_sha256_value(update_id) {
        return Err(UpdateCommandError::new(
            UpdateDiagnostic::InstallationFailed,
        ));
    }
    for root in [
        application_recovery_parent(application_home).join(update_id),
        application_home
            .join("staging")
            .join(format!("update-recovery-{update_id}")),
    ] {
        match fs::symlink_metadata(&root) {
            Ok(metadata) if !metadata.file_type().is_symlink() && metadata.is_dir() => {
                let mut removed = false;
                for _ in 0..300 {
                    match fs::remove_dir_all(&root) {
                        Ok(()) => {
                            removed = true;
                            break;
                        }
                        Err(error) if error.kind() == io::ErrorKind::NotFound => {
                            removed = true;
                            break;
                        }
                        Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
                            thread::sleep(Duration::from_millis(100));
                        }
                        Err(_) => break,
                    }
                }
                if !removed {
                    return Err(UpdateCommandError::new(
                        UpdateDiagnostic::InstallationFailed,
                    ));
                }
            }
            Ok(_) => {
                return Err(UpdateCommandError::new(
                    UpdateDiagnostic::InstallationFailed,
                ));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(_) => {
                return Err(UpdateCommandError::new(
                    UpdateDiagnostic::InstallationFailed,
                ));
            }
        }
    }
    Ok(())
}

fn verified_file_matches(path: &Path, expected_size: u64, expected_sha256: &str) -> bool {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return false;
    };
    !metadata.file_type().is_symlink()
        && metadata.is_file()
        && metadata.len() == expected_size
        && sha256_file(path).is_ok_and(|sha256| sha256 == expected_sha256)
}

fn sha256_file(path: &Path) -> Result<String, UpdateCommandError> {
    let mut file = fs::File::open(path)
        .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?;
    let mut hasher = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?;
        if read == 0 {
            break;
        }
        total = total
            .checked_add(read as u64)
            .filter(|total| *total <= MAX_APPLICATION_ARTIFACT_BYTES)
            .ok_or_else(|| UpdateCommandError::new(UpdateDiagnostic::InstallationFailed))?;
        hasher.update(&buffer[..read]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

pub fn run_update_rollback_helper_from_args() -> bool {
    run_update_rollback_helper(std::env::args_os())
}

pub fn run_update_rollback_helper(arguments: impl IntoIterator<Item = OsString>) -> bool {
    run_update_rollback_helper_with_launcher(arguments, |destination| {
        Command::new(destination).spawn().map(|_| ())
    })
}

pub fn run_update_rollback_helper_with_launcher(
    arguments: impl IntoIterator<Item = OsString>,
    launch: impl FnOnce(&Path) -> io::Result<()>,
) -> bool {
    let mut arguments = arguments.into_iter();
    let _executable = arguments.next();
    if arguments.next().as_deref() != Some(std::ffi::OsStr::new("--quantix-update-rollback")) {
        return false;
    }
    let Some(application_home) = arguments.next().map(PathBuf::from) else {
        return true;
    };
    let Some(update_id) = arguments.next().and_then(|value| value.into_string().ok()) else {
        return true;
    };
    if arguments.next().is_some() || !application_home.is_absolute() {
        return true;
    }
    if require_repair_required(&application_home, &update_id).is_err() {
        return true;
    }
    if let Ok(destination) = restore_application_recovery_point(&application_home, &update_id) {
        let _ = launch(&destination);
    }
    true
}

pub(crate) fn update_work_is_blocked(application_home: &Path) -> bool {
    if !application_home.join(INSTALLATION_DATABASE).exists() {
        return false;
    }
    let Ok(connection) = update_connection(application_home) else {
        return true;
    };
    connection
        .query_row(
            "SELECT EXISTS(
               SELECT 1 FROM update_operations
               WHERE state IN ('installing', 'restart_validation_required', 'repair_required')
             )",
            [],
            |row| row.get::<_, bool>(0),
        )
        .optional()
        .ok()
        .flatten()
        .unwrap_or(true)
}

fn transition_update_to_installing(
    application_home: &Path,
    update_id: &str,
) -> Result<(), UpdateCommandError> {
    let mut connection = update_connection(application_home)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::UpdaterUnavailable))?;
    let blocked = transaction
        .query_row(
            "SELECT EXISTS(
               SELECT 1 FROM update_operations
               WHERE state IN ('installing', 'restart_validation_required', 'repair_required')
             )",
            [],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::UpdaterUnavailable))?;
    if blocked {
        return Err(UpdateCommandError::new(UpdateDiagnostic::ActiveWork));
    }
    let changed = transaction
        .execute(
            "UPDATE update_operations
             SET state = 'installing', updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE update_id = ?1 AND state = 'approved'",
            [update_id],
        )
        .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::UpdaterUnavailable))?;
    if changed != 1 {
        return Err(UpdateCommandError::new(UpdateDiagnostic::ApprovalRequired));
    }
    transaction
        .commit()
        .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::UpdaterUnavailable))?;
    Ok(())
}

fn transition_update(
    application_home: &Path,
    update_id: &str,
    expected: &str,
    state: &str,
    diagnostic: Option<UpdateDiagnostic>,
) -> Result<(), UpdateCommandError> {
    let mut connection = update_connection(application_home)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::UpdaterUnavailable))?;
    let changed = transaction
        .execute(
            "UPDATE update_operations
             SET state = ?3, diagnostic_code = ?4,
                 updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE update_id = ?1 AND state = ?2",
            params![update_id, expected, state, diagnostic.map(diagnostic_code)],
        )
        .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::UpdaterUnavailable))?;
    if changed != 1 {
        return Err(UpdateCommandError::new(UpdateDiagnostic::ApprovalRequired));
    }
    transaction
        .commit()
        .map_err(|_| UpdateCommandError::new(UpdateDiagnostic::UpdaterUnavailable))?;
    Ok(())
}

fn parse_state(value: &str) -> Result<UpdateState, UpdateCommandError> {
    match value {
        "awaiting_approval" => Ok(UpdateState::AwaitingApproval),
        "approved" => Ok(UpdateState::Approved),
        "denied" => Ok(UpdateState::Denied),
        "installing" => Ok(UpdateState::Installing),
        "restart_validation_required" => Ok(UpdateState::RestartValidationRequired),
        "ready" => Ok(UpdateState::Ready),
        "rejected" => Ok(UpdateState::Rejected),
        "repair_required" => Ok(UpdateState::RepairRequired),
        "rolled_back" => Ok(UpdateState::RolledBack),
        _ => Err(UpdateCommandError::new(UpdateDiagnostic::InvalidManifest)),
    }
}

fn diagnostic_code(value: UpdateDiagnostic) -> &'static str {
    match value {
        UpdateDiagnostic::InvalidManifest => "invalid_manifest",
        UpdateDiagnostic::DowngradeRejected => "downgrade_rejected",
        UpdateDiagnostic::UnsupportedPlatform => "unsupported_platform",
        UpdateDiagnostic::InstallationSchemaIncompatible => "installation_schema_incompatible",
        UpdateDiagnostic::TenderStoreIncompatible => "tender_store_incompatible",
        UpdateDiagnostic::CodexIncompatible => "codex_incompatible",
        UpdateDiagnostic::DoclingIncompatible => "docling_incompatible",
        UpdateDiagnostic::RuntimeIncompatible => "runtime_incompatible",
        UpdateDiagnostic::ApprovalRequired => "approval_required",
        UpdateDiagnostic::ActiveWork => "active_work",
        UpdateDiagnostic::VerifiedBackupRequired => "verified_backup_required",
        UpdateDiagnostic::UnsignedArtifact => "unsigned_artifact",
        UpdateDiagnostic::WrongSigningKey => "wrong_signing_key",
        UpdateDiagnostic::ArtifactTampered => "artifact_tampered",
        UpdateDiagnostic::DownloadFailed => "download_failed",
        UpdateDiagnostic::InstallationFailed => "installation_failed",
        UpdateDiagnostic::InstallationInterrupted => "installation_interrupted",
        UpdateDiagnostic::RestartValidationFailed => "restart_validation_failed",
        UpdateDiagnostic::UpdaterConfigurationMissing => "updater_configuration_missing",
        UpdateDiagnostic::UpdaterUnavailable => "updater_unavailable",
    }
}

fn parse_diagnostic(value: &str) -> Result<UpdateDiagnostic, UpdateCommandError> {
    [
        UpdateDiagnostic::InvalidManifest,
        UpdateDiagnostic::DowngradeRejected,
        UpdateDiagnostic::UnsupportedPlatform,
        UpdateDiagnostic::InstallationSchemaIncompatible,
        UpdateDiagnostic::TenderStoreIncompatible,
        UpdateDiagnostic::CodexIncompatible,
        UpdateDiagnostic::DoclingIncompatible,
        UpdateDiagnostic::RuntimeIncompatible,
        UpdateDiagnostic::ApprovalRequired,
        UpdateDiagnostic::ActiveWork,
        UpdateDiagnostic::VerifiedBackupRequired,
        UpdateDiagnostic::UnsignedArtifact,
        UpdateDiagnostic::WrongSigningKey,
        UpdateDiagnostic::ArtifactTampered,
        UpdateDiagnostic::DownloadFailed,
        UpdateDiagnostic::InstallationFailed,
        UpdateDiagnostic::InstallationInterrupted,
        UpdateDiagnostic::RestartValidationFailed,
        UpdateDiagnostic::UpdaterConfigurationMissing,
        UpdateDiagnostic::UpdaterUnavailable,
    ]
    .into_iter()
    .find(|diagnostic| diagnostic_code(*diagnostic) == value)
    .ok_or_else(|| UpdateCommandError::new(UpdateDiagnostic::InvalidManifest))
}

fn valid_sha256(value: &str, _context: &()) -> garde::Result {
    if valid_sha256_value(value) {
        Ok(())
    } else {
        Err(garde::Error::new("must be a lowercase SHA-256 digest"))
    }
}

fn non_blank(value: &str, _context: &()) -> garde::Result {
    if value.trim().is_empty() {
        Err(garde::Error::new("must contain non-whitespace text"))
    } else {
        Ok(())
    }
}

fn valid_sha256_value(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
