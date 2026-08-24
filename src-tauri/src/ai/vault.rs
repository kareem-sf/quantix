use std::{
    collections::BTreeMap,
    error::Error,
    fmt,
    fs::{File, OpenOptions},
    io::{self, Read, Write},
    os::windows::{
        ffi::OsStrExt,
        fs::{MetadataExt, OpenOptionsExt},
        io::AsRawHandle,
    },
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

#[cfg(feature = "runtime-fixture")]
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

use fs4::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use windows::Win32::{
    Foundation::{GENERIC_READ, HANDLE},
    Storage::FileSystem::{
        FileDispositionInfo, FileIdInfo, FileStreamInfo, GetFileInformationByHandle,
        GetFileInformationByHandleEx, MoveFileExW, ReplaceFileW, SetFileInformationByHandle,
        BY_HANDLE_FILE_INFORMATION, DELETE, FILE_ATTRIBUTE_REPARSE_POINT, FILE_DISPOSITION_INFO,
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_ID_INFO, FILE_SHARE_READ,
        FILE_STREAM_INFO, MOVEFILE_WRITE_THROUGH, REPLACE_FILE_FLAGS,
    },
};
use windows_core::PCWSTR;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use super::contract::{
    normalize_label, AiConnectionConfiguration, AiConnectionRevision, AiProbeEvidence,
    AiProviderKind, CredentialGeneration,
};
use super::windows_dpapi::{protect_for_current_user, unprotect_for_current_user};
use crate::setup::validate_application_home_path;

const VAULT_FILE_NAME: &str = "ai-connections.vault";
const LOCK_FILE_NAME: &str = "ai-connections.vault.lock";
const STAGED_FILE_PREFIX: &str = ".ai-connections.vault.";
const STAGED_FILE_SUFFIX: &str = ".tmp";
const VAULT_SCHEMA_VERSION: u32 = 1;
const MAX_CLEAR_BYTES: usize = 4 * 1024 * 1024;
const MAX_CIPHERTEXT_BYTES: usize = 8 * 1024 * 1024;
const MAX_STREAM_QUERY_BYTES: usize = 64 * 1024;
const MAX_CONNECTIONS: usize = 32;
const MAX_ACCOUNT_ID_BYTES: usize = 4 * 1024;
const MAX_CREDENTIAL_BYTES: usize = 16 * 1024;
const MAX_CUSTOM_VALUE_BYTES: usize = 4 * 1024;
const MAX_EXPIRY_BYTES: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VaultError {
    Unavailable,
    Corrupt,
    Unsupported,
    Invalid,
    RevisionConflict,
    RevisionOverflow,
    NotFound,
}

impl fmt::Display for VaultError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "AI connection vault is unavailable",
            Self::Corrupt => "AI connection vault is corrupt",
            Self::Unsupported => "AI connection vault version is unsupported",
            Self::Invalid => "AI connection vault mutation is invalid",
            Self::RevisionConflict => "AI connection vault revision conflict",
            Self::RevisionOverflow => "AI connection counter overflow",
            Self::NotFound => "AI connection was not found",
        })
    }
}

impl Error for VaultError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultSnapshot {
    pub mutation_revision: u64,
    pub connection_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VaultLoadState {
    Missing,
    Ready(VaultSnapshot),
    Corrupt,
    Unsupported,
}

impl VaultLoadState {
    pub fn ready(&self) -> Option<&VaultSnapshot> {
        match self {
            Self::Ready(snapshot) => Some(snapshot),
            Self::Missing | Self::Corrupt | Self::Unsupported => None,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct VaultPayload {
    schema_version: u32,
    mutation_revision: u64,
    connections: BTreeMap<String, StoredAiConnection>,
}

#[derive(Deserialize)]
struct VaultVersionHeader {
    schema_version: u32,
}

impl Zeroize for VaultPayload {
    fn zeroize(&mut self) {
        self.schema_version.zeroize();
        self.mutation_revision.zeroize();
        self.connections.clear();
    }
}

impl ZeroizeOnDrop for VaultPayload {}

impl Drop for VaultPayload {
    fn drop(&mut self) {
        self.zeroize();
    }
}

impl fmt::Debug for VaultPayload {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("VaultPayload")
            .field("schema_version", &self.schema_version)
            .field("mutation_revision", &self.mutation_revision)
            .field("connection_count", &self.connections.len())
            .finish()
    }
}

impl VaultPayload {
    fn empty() -> Self {
        Self {
            schema_version: VAULT_SCHEMA_VERSION,
            mutation_revision: 0,
            connections: BTreeMap::new(),
        }
    }

    fn validate(&self) -> Result<(), VaultError> {
        if self.schema_version != VAULT_SCHEMA_VERSION {
            return Err(VaultError::Unsupported);
        }
        if self.connections.len() > MAX_CONNECTIONS
            || self.connections.iter().any(|(key, connection)| {
                key != &connection.connection_id || !is_connection_id(key) || !connection.is_valid()
            })
        {
            return Err(VaultError::Corrupt);
        }
        Ok(())
    }

    pub(crate) fn insert(&mut self, connection: StoredAiConnection) -> Result<(), VaultError> {
        if self.connections.len() >= MAX_CONNECTIONS
            || !connection.is_valid()
            || self.connections.contains_key(&connection.connection_id)
        {
            return Err(VaultError::Invalid);
        }
        self.connections
            .insert(connection.connection_id.clone(), connection);
        Ok(())
    }

    pub(crate) fn connections(&self) -> impl Iterator<Item = &StoredAiConnection> {
        self.connections.values()
    }

    pub(crate) fn connection(
        &self,
        connection_id: &str,
    ) -> Result<&StoredAiConnection, VaultError> {
        self.connections
            .get(connection_id)
            .ok_or(VaultError::NotFound)
    }

    pub(crate) fn record_probe(
        &mut self,
        evidence: AiProbeEvidence,
        expected_endpoint_fingerprint: &str,
        expected_adapter_version: &str,
    ) -> Result<(), VaultError> {
        let connection = self
            .connections
            .get_mut(evidence.connection_id.as_str())
            .ok_or(VaultError::NotFound)?;
        connection.record_probe(
            evidence,
            expected_endpoint_fingerprint,
            expected_adapter_version,
        )
    }

    pub(crate) fn replace_connection_configuration(
        &mut self,
        connection_id: &str,
        expected_execution_revision: u64,
        expected_credential_generation: u64,
        display_name: String,
        configuration: AiConnectionConfiguration,
        replacement_credential: Option<StoredCredential>,
    ) -> Result<(), VaultError> {
        let connection = self.require_exact_connection_mut(
            connection_id,
            expected_execution_revision,
            expected_credential_generation,
        )?;
        let material_change =
            connection.configuration != configuration || replacement_credential.is_some();
        if material_change {
            connection.execution_revision = connection
                .execution_revision
                .checked_add(1)
                .ok_or(VaultError::RevisionOverflow)?;
            connection.probe_evidence = None;
        }
        if let Some(credential) = replacement_credential {
            connection.credential_generation = connection
                .credential_generation
                .checked_add(1)
                .ok_or(VaultError::RevisionOverflow)?;
            connection.credential = Some(credential);
        }
        connection.display_name = display_name;
        connection.configuration = configuration;
        Ok(())
    }

    pub(crate) fn rename_connection(
        &mut self,
        connection_id: &str,
        expected_execution_revision: u64,
        expected_credential_generation: u64,
        display_name: String,
    ) -> Result<(), VaultError> {
        let connection = self.require_exact_connection_mut(
            connection_id,
            expected_execution_revision,
            expected_credential_generation,
        )?;
        connection.display_name = display_name;
        Ok(())
    }

    pub(crate) fn set_enabled(
        &mut self,
        connection_id: &str,
        expected_execution_revision: u64,
        expected_credential_generation: u64,
        enabled: bool,
    ) -> Result<(), VaultError> {
        let connection = self.require_exact_connection_mut(
            connection_id,
            expected_execution_revision,
            expected_credential_generation,
        )?;
        connection.enabled = enabled;
        Ok(())
    }

    pub(crate) fn disconnect(
        &mut self,
        connection_id: &str,
        expected_execution_revision: u64,
        expected_credential_generation: u64,
    ) -> Result<(), VaultError> {
        let connection = self.require_exact_connection_mut(
            connection_id,
            expected_execution_revision,
            expected_credential_generation,
        )?;
        connection.credential_generation = connection
            .credential_generation
            .checked_add(1)
            .ok_or(VaultError::RevisionOverflow)?;
        connection.credential = None;
        Ok(())
    }

    pub(crate) fn remove_connection(
        &mut self,
        connection_id: &str,
        expected_execution_revision: u64,
        expected_credential_generation: u64,
    ) -> Result<(), VaultError> {
        self.require_exact_connection_mut(
            connection_id,
            expected_execution_revision,
            expected_credential_generation,
        )?;
        self.connections
            .remove(connection_id)
            .ok_or(VaultError::Invalid)?;
        Ok(())
    }

    pub(crate) fn rotate_same_account_tokens(
        &mut self,
        connection_id: &str,
        expected_execution_revision: u64,
        expected_credential_generation: u64,
        verified_account_id: &str,
        replacement_credential: StoredCredential,
    ) -> Result<(), VaultError> {
        let connection = self.require_exact_connection_mut(
            connection_id,
            expected_execution_revision,
            expected_credential_generation,
        )?;
        let configured_account_id = match &connection.configuration {
            AiConnectionConfiguration::AccountLogin { account_id, .. } => account_id,
            _ => return Err(VaultError::Invalid),
        };
        if configured_account_id != verified_account_id
            || connection
                .credential
                .as_ref()
                .and_then(StoredCredential::verified_account_id)
                != Some(verified_account_id)
            || replacement_credential.verified_account_id() != Some(verified_account_id)
        {
            return Err(VaultError::Invalid);
        }
        connection.credential_generation = connection
            .credential_generation
            .checked_add(1)
            .ok_or(VaultError::RevisionOverflow)?;
        connection.credential = Some(replacement_credential);
        Ok(())
    }

    fn require_exact_connection_mut(
        &mut self,
        connection_id: &str,
        expected_execution_revision: u64,
        expected_credential_generation: u64,
    ) -> Result<&mut StoredAiConnection, VaultError> {
        let connection = self
            .connections
            .get_mut(connection_id)
            .ok_or(VaultError::NotFound)?;
        if connection.execution_revision != expected_execution_revision
            || connection.credential_generation != expected_credential_generation
        {
            return Err(VaultError::RevisionConflict);
        }
        Ok(connection)
    }

    pub(crate) fn require_connection_cas(
        &self,
        connection_id: &str,
        expected_execution_revision: u64,
        expected_credential_generation: u64,
    ) -> Result<(), VaultError> {
        let connection = self.connection(connection_id)?;
        if connection.execution_revision != expected_execution_revision
            || connection.credential_generation != expected_credential_generation
        {
            return Err(VaultError::RevisionConflict);
        }
        Ok(())
    }

    fn finish_mutation(&mut self, original_revision: u64) -> Result<(), VaultError> {
        if self.schema_version != VAULT_SCHEMA_VERSION
            || self.mutation_revision != original_revision
        {
            return Err(VaultError::Invalid);
        }
        self.validate().map_err(|_| VaultError::Invalid)?;
        self.mutation_revision = original_revision
            .checked_add(1)
            .ok_or(VaultError::RevisionOverflow)?;
        Ok(())
    }

    fn secret_free_snapshot(&self) -> VaultSnapshot {
        VaultSnapshot {
            mutation_revision: self.mutation_revision,
            connection_ids: self.connections.keys().cloned().collect(),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StoredAiConnection {
    pub(crate) connection_id: String,
    display_name: String,
    configuration: AiConnectionConfiguration,
    enabled: bool,
    execution_revision: u64,
    credential_generation: u64,
    credential: Option<StoredCredential>,
    probe_evidence: Option<AiProbeEvidence>,
}

impl Zeroize for StoredAiConnection {
    fn zeroize(&mut self) {
        self.credential.zeroize();
    }
}

impl ZeroizeOnDrop for StoredAiConnection {}

impl Drop for StoredAiConnection {
    fn drop(&mut self) {
        self.zeroize();
    }
}

impl fmt::Debug for StoredAiConnection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("StoredAiConnection([REDACTED])")
    }
}

impl StoredAiConnection {
    pub(crate) fn new(
        connection_id: String,
        display_name: String,
        configuration: AiConnectionConfiguration,
        credential: StoredCredential,
    ) -> Result<Self, VaultError> {
        let connection = Self {
            connection_id,
            display_name,
            configuration,
            enabled: true,
            execution_revision: 1,
            credential_generation: 1,
            credential: Some(credential),
            probe_evidence: None,
        };
        connection
            .is_valid()
            .then_some(connection)
            .ok_or(VaultError::Invalid)
    }

    fn is_valid(&self) -> bool {
        is_connection_id(&self.connection_id)
            && normalize_label(&self.display_name)
                .is_ok_and(|normalized| normalized == self.display_name)
            && self.configuration.validate().is_ok()
            && configuration_account_is_valid(&self.configuration)
            && AiConnectionRevision::new(self.execution_revision).is_ok()
            && CredentialGeneration::new(self.credential_generation).is_ok()
            && self
                .credential
                .as_ref()
                .is_none_or(|credential| credential.is_valid_for(&self.configuration))
            && self.probe_evidence.as_ref().is_none_or(|evidence| {
                evidence.connection_id.as_str() == self.connection_id
                    && evidence.execution_revision.get() == self.execution_revision
                    && evidence.provider == self.configuration.provider()
                    && self
                        .configuration
                        .endpoint_fingerprint()
                        .is_ok_and(|fingerprint| evidence.endpoint_fingerprint == fingerprint)
                    && evidence.validate().is_ok()
                    && self
                        .configuration
                        .accepts_destination_class(evidence.destination_class)
                    && tested_model_matches_configuration(&self.configuration, evidence)
            })
    }

    pub(crate) fn connection_id(&self) -> &str {
        &self.connection_id
    }

    pub(crate) fn display_name(&self) -> &str {
        &self.display_name
    }

    pub(crate) fn configuration(&self) -> &AiConnectionConfiguration {
        &self.configuration
    }

    pub(crate) fn enabled(&self) -> bool {
        self.enabled
    }

    pub(crate) fn execution_revision(&self) -> u64 {
        self.execution_revision
    }

    pub(crate) fn credential_generation(&self) -> u64 {
        self.credential_generation
    }

    pub(crate) fn has_credential(&self) -> bool {
        self.credential.is_some()
    }

    pub(crate) fn probe_evidence(&self) -> Option<&AiProbeEvidence> {
        self.probe_evidence.as_ref()
    }

    fn record_probe(
        &mut self,
        evidence: AiProbeEvidence,
        expected_endpoint_fingerprint: &str,
        expected_adapter_version: &str,
    ) -> Result<(), VaultError> {
        if evidence.execution_revision.get() != self.execution_revision {
            return Err(VaultError::RevisionConflict);
        }
        if evidence.connection_id.as_str() != self.connection_id
            || evidence.provider != self.configuration.provider()
            || evidence.endpoint_fingerprint != expected_endpoint_fingerprint
            || evidence.adapter_version != expected_adapter_version
            || evidence.validate().is_err()
            || !self
                .configuration
                .accepts_destination_class(evidence.destination_class)
            || !tested_model_matches_configuration(&self.configuration, &evidence)
        {
            return Err(VaultError::Invalid);
        }
        self.probe_evidence = Some(evidence);
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(deny_unknown_fields)]
pub(crate) struct StoredCredential {
    pub(crate) values: Vec<SecretNameValue>,
}

impl fmt::Debug for StoredCredential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("StoredCredential([REDACTED])")
    }
}

impl StoredCredential {
    pub(crate) fn from_api_key(api_key: SecretString) -> Result<Self, VaultError> {
        Self::from_values(vec![("api_key".to_owned(), api_key)])
    }

    pub(crate) fn from_account(
        access_token: SecretString,
        refresh_token: SecretString,
        expires_at: SecretString,
        verified_account_id: SecretString,
    ) -> Result<Self, VaultError> {
        Self::from_values(vec![
            ("access_token".to_owned(), access_token),
            ("refresh_token".to_owned(), refresh_token),
            ("expires_at".to_owned(), expires_at),
            ("verified_account_id".to_owned(), verified_account_id),
        ])
    }

    pub(crate) fn from_compatible(
        api_key: SecretString,
        header_values: Vec<(String, SecretString)>,
        query_values: Vec<(String, SecretString)>,
    ) -> Result<Self, VaultError> {
        let mut values = Vec::with_capacity(1 + header_values.len() + query_values.len());
        values.push(("api_key".to_owned(), api_key));
        values.extend(
            header_values
                .into_iter()
                .map(|(name, value)| (format!("header:{name}"), value)),
        );
        values.extend(
            query_values
                .into_iter()
                .map(|(name, value)| (format!("query:{name}"), value)),
        );
        Self::from_values(values)
    }

    fn from_values(values: Vec<(String, SecretString)>) -> Result<Self, VaultError> {
        let credential = Self {
            values: values
                .into_iter()
                .map(|(name, value)| SecretNameValue {
                    name: SecretName(name),
                    value: SecretValue(value),
                })
                .collect(),
        };
        credential
            .is_valid()
            .then_some(credential)
            .ok_or(VaultError::Invalid)
    }

    fn is_valid(&self) -> bool {
        !self.values.is_empty()
            && self.values.iter().all(SecretNameValue::is_valid)
            && self.values.iter().enumerate().all(|(index, value)| {
                !self.values[..index]
                    .iter()
                    .any(|prior| prior.name.0 == value.name.0)
            })
            && self
                .values
                .iter()
                .try_fold(0usize, |total, value| {
                    total.checked_add(value.value.0.len())
                })
                .is_some_and(|total| total <= MAX_CREDENTIAL_BYTES)
    }

    fn is_valid_for(&self, configuration: &AiConnectionConfiguration) -> bool {
        if !self.is_valid() {
            return false;
        }
        match configuration {
            AiConnectionConfiguration::AccountLogin { account_id, .. } => {
                self.values.iter().all(|value| {
                    matches!(
                        value.name.0.as_str(),
                        "access_token" | "refresh_token" | "expires_at" | "verified_account_id"
                    )
                }) && self.named_value("access_token").is_some()
                    && self.named_value("refresh_token").is_some()
                    && self
                        .named_value("expires_at")
                        .is_some_and(|expires_at| expires_at.len() <= MAX_EXPIRY_BYTES)
                    && self.named_value("verified_account_id") == Some(account_id.as_str())
            }
            AiConnectionConfiguration::DirectProviderKey { .. } => {
                self.values.len() == 1 && self.named_value("api_key").is_some()
            }
            AiConnectionConfiguration::OpenAiCompatible { endpoint, .. }
            | AiConnectionConfiguration::AnthropicCompatible { endpoint, .. } => {
                let expected_count = 1usize
                    .checked_add(endpoint.custom_header_names.len())
                    .and_then(|count| count.checked_add(endpoint.custom_query_names.len()));
                expected_count == Some(self.values.len())
                    && self.named_value("api_key").is_some()
                    && endpoint.custom_header_names.iter().all(|name| {
                        self.named_value(&format!("header:{name}"))
                            .is_some_and(|value| value.len() <= MAX_CUSTOM_VALUE_BYTES)
                    })
                    && endpoint.custom_query_names.iter().all(|name| {
                        self.named_value(&format!("query:{name}"))
                            .is_some_and(|value| value.len() <= MAX_CUSTOM_VALUE_BYTES)
                    })
            }
        }
    }

    fn named_value(&self, name: &str) -> Option<&str> {
        self.values
            .iter()
            .find(|value| value.name.0 == name)
            .map(|value| value.value.0.as_str())
    }

    fn verified_account_id(&self) -> Option<&str> {
        self.values
            .iter()
            .find(|value| value.name.0 == "verified_account_id")
            .map(|value| value.value.0.as_str())
    }
}

#[derive(Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(deny_unknown_fields)]
pub(crate) struct SecretNameValue {
    pub(crate) name: SecretName,
    pub(crate) value: SecretValue,
}

impl fmt::Debug for SecretNameValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretNameValue([REDACTED])")
    }
}

impl SecretNameValue {
    fn is_valid(&self) -> bool {
        !self.name.0.is_empty() && !self.value.0.is_empty()
    }
}

fn configuration_account_is_valid(configuration: &AiConnectionConfiguration) -> bool {
    match configuration {
        AiConnectionConfiguration::AccountLogin { account_id, .. } => {
            !account_id.is_empty() && account_id.len() <= MAX_ACCOUNT_ID_BYTES
        }
        AiConnectionConfiguration::DirectProviderKey { .. }
        | AiConnectionConfiguration::OpenAiCompatible { .. }
        | AiConnectionConfiguration::AnthropicCompatible { .. } => true,
    }
}

fn tested_model_matches_configuration(
    configuration: &AiConnectionConfiguration,
    evidence: &AiProbeEvidence,
) -> bool {
    match configuration {
        AiConnectionConfiguration::OpenAiCompatible { endpoint, .. }
        | AiConnectionConfiguration::AnthropicCompatible { endpoint, .. } => {
            evidence.tested_model_id == endpoint.model_id
        }
        AiConnectionConfiguration::AccountLogin { .. }
        | AiConnectionConfiguration::DirectProviderKey { .. } => true,
    }
}

#[derive(Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(transparent)]
pub(crate) struct SecretName(pub(crate) String);

impl fmt::Debug for SecretName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretName([REDACTED])")
    }
}

#[derive(Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(transparent)]
pub(crate) struct SecretValue(pub(crate) SecretString);

impl fmt::Debug for SecretValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretValue([REDACTED])")
    }
}

pub(crate) struct SecretString(Zeroizing<String>);

impl SecretString {
    pub(crate) fn new(value: String) -> Self {
        Self(Zeroizing::new(value))
    }

    pub(crate) fn as_str(&self) -> &str {
        self.0.as_str()
    }

    fn len(&self) -> usize {
        self.0.len()
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Serialize for SecretString {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for SecretString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        String::deserialize(deserializer).map(Self::new)
    }
}

impl Zeroize for SecretString {
    fn zeroize(&mut self) {
        self.0.zeroize();
    }
}

impl ZeroizeOnDrop for SecretString {}

impl Drop for SecretString {
    fn drop(&mut self) {
        self.zeroize();
        #[cfg(feature = "runtime-fixture")]
        SECRET_DROP_OBSERVATIONS.fetch_add(1, Ordering::AcqRel);
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretString([REDACTED])")
    }
}

#[cfg(feature = "runtime-fixture")]
static SECRET_DROP_OBSERVATIONS: AtomicUsize = AtomicUsize::new(0);

#[cfg(feature = "runtime-fixture")]
pub(crate) fn reset_secret_drop_observations() {
    SECRET_DROP_OBSERVATIONS.store(0, Ordering::Release);
}

#[cfg(feature = "runtime-fixture")]
pub(crate) fn secret_drop_observations() -> usize {
    SECRET_DROP_OBSERVATIONS.load(Ordering::Acquire)
}

pub struct AiConnectionVault {
    path: PathBuf,
    lock_path: PathBuf,
    #[cfg(feature = "runtime-fixture")]
    publication_fault: AtomicU8,
}

impl AiConnectionVault {
    pub fn new(application_home: &Path) -> Result<Self, VaultError> {
        let application_home = validate_application_home_path(application_home)
            .map_err(|_| VaultError::Unavailable)?;
        validate_application_home_handle(&application_home)?;
        Ok(Self {
            path: application_home.join(VAULT_FILE_NAME),
            lock_path: application_home.join(LOCK_FILE_NAME),
            #[cfg(feature = "runtime-fixture")]
            publication_fault: AtomicU8::new(PUBLISH_FAULT_NONE),
        })
    }

    pub fn load(&self) -> Result<VaultLoadState, VaultError> {
        let _process = vault_mutex().lock().map_err(|_| VaultError::Unavailable)?;
        let lock = self.open_lock()?;
        FileExt::lock(&lock).map_err(|_| VaultError::Unavailable)?;
        validate_file_handle(&lock)?;
        self.load_state_locked()
    }

    pub(crate) fn mutate<F>(
        &self,
        expected_revision: u64,
        mutation: F,
    ) -> Result<VaultSnapshot, VaultError>
    where
        F: FnOnce(&mut VaultPayload) -> Result<(), VaultError>,
    {
        let _process = vault_mutex().lock().map_err(|_| VaultError::Unavailable)?;
        let lock = self.open_lock()?;
        FileExt::lock(&lock).map_err(|_| VaultError::Unavailable)?;
        validate_file_handle(&lock)?;
        let mut payload = self.load_payload_locked()?;
        if payload.mutation_revision != expected_revision {
            return Err(VaultError::RevisionConflict);
        }
        let original_revision = payload.mutation_revision;
        mutation(&mut payload)?;
        payload.finish_mutation(original_revision)?;
        self.publish_locked(&payload)?;
        Ok(payload.secret_free_snapshot())
    }

    #[allow(dead_code)]
    pub(crate) fn mutate_current<F>(&self, mutation: F) -> Result<VaultSnapshot, VaultError>
    where
        F: FnOnce(&mut VaultPayload) -> Result<(), VaultError>,
    {
        let _process = vault_mutex().lock().map_err(|_| VaultError::Unavailable)?;
        let lock = self.open_lock()?;
        FileExt::lock(&lock).map_err(|_| VaultError::Unavailable)?;
        validate_file_handle(&lock)?;
        let mut payload = self.load_payload_locked()?;
        let original_revision = payload.mutation_revision;
        mutation(&mut payload)?;
        payload.finish_mutation(original_revision)?;
        self.publish_locked(&payload)?;
        Ok(payload.secret_free_snapshot())
    }

    pub(crate) fn mutate_current_project<F, T>(&self, mutation: F) -> Result<T, VaultError>
    where
        F: FnOnce(&mut VaultPayload) -> Result<T, VaultError>,
    {
        let _process = vault_mutex().lock().map_err(|_| VaultError::Unavailable)?;
        let lock = self.open_lock()?;
        FileExt::lock(&lock).map_err(|_| VaultError::Unavailable)?;
        validate_file_handle(&lock)?;
        let mut payload = self.load_payload_locked()?;
        let original_revision = payload.mutation_revision;
        let projection = mutation(&mut payload)?;
        payload.finish_mutation(original_revision)?;
        self.publish_locked(&payload)?;
        Ok(projection)
    }

    #[allow(dead_code)]
    pub(crate) fn with_locked_payload<F, T>(&self, operation: F) -> Result<T, VaultError>
    where
        F: FnOnce(&VaultPayload) -> Result<T, VaultError>,
    {
        let _process = vault_mutex().lock().map_err(|_| VaultError::Unavailable)?;
        let lock = self.open_lock()?;
        FileExt::lock(&lock).map_err(|_| VaultError::Unavailable)?;
        validate_file_handle(&lock)?;
        let payload = self.load_payload_locked()?;
        operation(&payload)
    }

    #[cfg(feature = "runtime-fixture")]
    pub fn fixture_insert(
        &self,
        expected_revision: u64,
        connection_id: &str,
        secret: &Zeroizing<String>,
    ) -> Result<VaultSnapshot, VaultError> {
        let connection = fixture_connection(connection_id, secret)?;
        self.mutate(expected_revision, |payload| payload.insert(connection))
    }

    #[cfg(feature = "runtime-fixture")]
    pub fn fixture_insert_current(
        &self,
        connection_id: &str,
        secret: &Zeroizing<String>,
    ) -> Result<VaultSnapshot, VaultError> {
        let connection = fixture_connection(connection_id, secret)?;
        self.mutate_current(|payload| payload.insert(connection))
    }

    #[cfg(feature = "runtime-fixture")]
    pub fn fixture_fail_before_publish_once(&self) {
        self.publication_fault
            .store(PUBLISH_FAULT_BEFORE, Ordering::Release);
    }

    #[cfg(feature = "runtime-fixture")]
    pub fn fixture_fail_after_publish_once(&self) {
        self.publication_fault
            .store(PUBLISH_FAULT_AFTER, Ordering::Release);
    }

    #[cfg(feature = "runtime-fixture")]
    pub fn fixture_add_staged_ads_before_publish_once(&self) {
        self.publication_fault
            .store(PUBLISH_FAULT_STAGE_ADS, Ordering::Release);
    }

    #[cfg(feature = "runtime-fixture")]
    pub fn fixture_swap_staged_path_before_failure_once(&self) {
        self.publication_fault
            .store(PUBLISH_FAULT_STAGE_SWAP, Ordering::Release);
    }

    #[cfg(feature = "runtime-fixture")]
    pub fn fixture_override_revision_current(&self) -> Result<VaultSnapshot, VaultError> {
        self.mutate_current(|payload| {
            payload.mutation_revision = 40;
            Ok(())
        })
    }

    #[cfg(feature = "runtime-fixture")]
    pub fn fixture_insert_key_mismatch_current(&self) -> Result<VaultSnapshot, VaultError> {
        let secret = Zeroizing::new("fixture-mismatch".to_owned());
        let connection = fixture_connection("00000000000000000000000000000092", &secret)?;
        self.mutate_current(|payload| {
            payload
                .connections
                .insert("00000000000000000000000000000091".to_owned(), connection);
            Ok(())
        })
    }

    #[cfg(feature = "runtime-fixture")]
    pub fn fixture_insert_invalid_record_current(&self) -> Result<VaultSnapshot, VaultError> {
        self.mutate_current(|payload| {
            let connection_id = "00000000000000000000000000000093".to_owned();
            payload.connections.insert(
                connection_id.clone(),
                StoredAiConnection {
                    connection_id,
                    display_name: "Invalid fixture".to_owned(),
                    configuration: AiConnectionConfiguration::DirectProviderKey {
                        provider: AiProviderKind::OpenAi,
                    },
                    enabled: true,
                    execution_revision: 1,
                    credential_generation: 1,
                    credential: Some(StoredCredential { values: Vec::new() }),
                    probe_evidence: None,
                },
            );
            Ok(())
        })
    }

    #[cfg(feature = "runtime-fixture")]
    pub fn fixture_override_schema_current(&self) -> Result<VaultSnapshot, VaultError> {
        self.mutate_current(|payload| {
            payload.schema_version = VAULT_SCHEMA_VERSION + 1;
            Ok(())
        })
    }

    #[cfg(feature = "runtime-fixture")]
    pub fn fixture_verify_cleartext_writer_backstop() -> Result<(), VaultError> {
        let mut writer = CappedCleartextWriter::new()?;
        let initial_capacity = writer.bytes.capacity();
        let initial_pointer = writer.bytes.as_ptr();
        let chunk = [b'x'; 4 * 1024];
        for _ in 0..(MAX_CLEAR_BYTES / chunk.len()) {
            writer.write_all(&chunk).map_err(|_| VaultError::Invalid)?;
            if writer.bytes.capacity() != initial_capacity
                || writer.bytes.as_ptr() != initial_pointer
            {
                return Err(VaultError::Invalid);
            }
        }
        if writer.bytes.len() != MAX_CLEAR_BYTES
            || writer.write_all(b"x").is_ok()
            || writer.bytes.len() != MAX_CLEAR_BYTES
            || writer.bytes.capacity() != initial_capacity
            || writer.bytes.as_ptr() != initial_pointer
        {
            return Err(VaultError::Invalid);
        }
        Ok(())
    }

    #[cfg(feature = "runtime-fixture")]
    pub fn fixture_set_connection_counters(
        &self,
        connection_id: &str,
        execution_revision: u64,
        credential_generation: u64,
    ) -> Result<VaultSnapshot, VaultError> {
        self.mutate_current(|payload| {
            let connection = payload
                .connections
                .get_mut(connection_id)
                .ok_or(VaultError::Invalid)?;
            connection.execution_revision = execution_revision;
            connection.credential_generation = credential_generation;
            connection.probe_evidence = None;
            Ok(())
        })
    }

    fn load_state_locked(&self) -> Result<VaultLoadState, VaultError> {
        let ciphertext = match self.read_ciphertext_locked() {
            Ok(ciphertext) => ciphertext,
            Err(VaultError::Corrupt | VaultError::Invalid) => return Ok(VaultLoadState::Corrupt),
            Err(error) => return Err(error),
        };
        let Some(ciphertext) = ciphertext else {
            return Ok(VaultLoadState::Missing);
        };
        match decode_payload(&ciphertext) {
            Ok(payload) => Ok(VaultLoadState::Ready(payload.secret_free_snapshot())),
            Err(VaultError::Corrupt | VaultError::Invalid) => Ok(VaultLoadState::Corrupt),
            Err(VaultError::Unsupported) => Ok(VaultLoadState::Unsupported),
            Err(error) => Err(error),
        }
    }

    fn load_payload_locked(&self) -> Result<VaultPayload, VaultError> {
        let Some(ciphertext) = self.read_ciphertext_locked()? else {
            return Ok(VaultPayload::empty());
        };
        decode_payload(&ciphertext)
    }

    fn read_ciphertext_locked(&self) -> Result<Option<Vec<u8>>, VaultError> {
        let Some(mut file) = open_existing_validated_file(&self.path, false)? else {
            return Ok(None);
        };
        let metadata = file.metadata().map_err(|_| VaultError::Unavailable)?;
        if metadata.len() > MAX_CIPHERTEXT_BYTES as u64 {
            return Err(VaultError::Corrupt);
        }
        let mut ciphertext = Vec::with_capacity(metadata.len() as usize);
        (&mut file)
            .take((MAX_CIPHERTEXT_BYTES + 1) as u64)
            .read_to_end(&mut ciphertext)
            .map_err(|_| VaultError::Unavailable)?;
        if ciphertext.is_empty() || ciphertext.len() > MAX_CIPHERTEXT_BYTES {
            return Err(VaultError::Corrupt);
        }
        Ok(Some(ciphertext))
    }

    fn open_lock(&self) -> Result<File, VaultError> {
        for _ in 0..32 {
            match open_create_new_file(&self.lock_path, true) {
                Ok(file) => return Ok(file),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    if let Some(file) = open_existing_validated_file(&self.lock_path, true)? {
                        return Ok(file);
                    }
                }
                Err(_) => return Err(VaultError::Unavailable),
            }
        }
        Err(VaultError::Unavailable)
    }

    fn publish_locked(&self, payload: &VaultPayload) -> Result<(), VaultError> {
        let mut clear = CappedCleartextWriter::new()?;
        serde_json::to_writer(&mut clear, payload).map_err(|_| VaultError::Invalid)?;
        let clear = clear.into_bytes();
        let ciphertext = protect_for_current_user(clear).map_err(|_| VaultError::Unavailable)?;
        if ciphertext.is_empty() || ciphertext.len() > MAX_CIPHERTEXT_BYTES {
            return Err(VaultError::Invalid);
        }
        let expected_ciphertext_sha256 = Sha256::digest(&ciphertext);

        let (mut staged_path, mut staged) = self.create_staged_file()?;
        staged
            .write_all(&ciphertext)
            .and_then(|()| staged.flush())
            .and_then(|()| staged.sync_all())
            .map_err(|_| VaultError::Unavailable)?;
        let fault = self.take_publication_fault();
        self.apply_staged_fixture_fault(fault, staged_path.path())?;
        validate_file_handle(&staged)?;
        drop(staged);

        if matches!(fault, PUBLISH_FAULT_STAGE_ADS | PUBLISH_FAULT_STAGE_SWAP) {
            return Err(VaultError::Unavailable);
        }

        let target_state = self.validated_target_state()?;
        let raw_publication = if fault == PUBLISH_FAULT_BEFORE {
            Err(())
        } else {
            match target_state {
                TargetState::Existing => replace_file(&self.path, staged_path.path()),
                TargetState::Missing => move_file_write_through(staged_path.path(), &self.path),
            }
            .map_err(|_| ())
        };
        if raw_publication.is_ok() {
            staged_path.disarm();
        }
        let publication = if fault == PUBLISH_FAULT_AFTER && raw_publication.is_ok() {
            Err(())
        } else {
            raw_publication
        };

        let verified = self.reopen_and_verify(
            expected_ciphertext_sha256.as_slice(),
            payload.mutation_revision,
        );
        match (publication, verified) {
            (_, Ok(true)) => {
                staged_path.disarm();
                Ok(())
            }
            (Ok(()), Ok(false)) | (Err(()), Ok(false)) => Err(VaultError::Unavailable),
            (Ok(()) | Err(()), Err(error)) => Err(error),
        }
    }

    fn create_staged_file(&self) -> Result<(OwnedStagedPath, File), VaultError> {
        for _ in 0..32 {
            let mut random = [0u8; 16];
            getrandom::fill(&mut random).map_err(|_| VaultError::Unavailable)?;
            let suffix: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
            let path = self
                .path
                .with_file_name(format!("{STAGED_FILE_PREFIX}{suffix}{STAGED_FILE_SUFFIX}"));
            match open_create_new_file(&path, false) {
                Ok(file) => {
                    let identity = file_identity(&file).map_err(|_| VaultError::Unavailable)?;
                    return Ok((OwnedStagedPath::new(path, identity), file));
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(_) => return Err(VaultError::Unavailable),
            }
        }
        Err(VaultError::Unavailable)
    }

    fn validated_target_state(&self) -> Result<TargetState, VaultError> {
        Ok(
            if open_existing_validated_file(&self.path, false)?.is_some() {
                TargetState::Existing
            } else {
                TargetState::Missing
            },
        )
    }

    fn reopen_and_verify(
        &self,
        expected_ciphertext_sha256: &[u8],
        expected_mutation_revision: u64,
    ) -> Result<bool, VaultError> {
        let Some(ciphertext) = self.read_ciphertext_locked()? else {
            return Ok(false);
        };
        if Sha256::digest(&ciphertext).as_slice() != expected_ciphertext_sha256 {
            return Ok(false);
        }
        match decode_payload(&ciphertext) {
            Ok(payload) => Ok(payload.mutation_revision == expected_mutation_revision),
            Err(VaultError::Corrupt | VaultError::Unsupported | VaultError::Invalid) => Ok(false),
            Err(error) => Err(error),
        }
    }

    #[cfg(feature = "runtime-fixture")]
    fn take_publication_fault(&self) -> u8 {
        self.publication_fault
            .swap(PUBLISH_FAULT_NONE, Ordering::AcqRel)
    }

    #[cfg(not(feature = "runtime-fixture"))]
    fn take_publication_fault(&self) -> u8 {
        PUBLISH_FAULT_NONE
    }

    #[cfg(feature = "runtime-fixture")]
    fn apply_staged_fixture_fault(&self, fault: u8, path: &Path) -> Result<(), VaultError> {
        if fault == PUBLISH_FAULT_STAGE_ADS {
            let stream = PathBuf::from(format!("{}:fixture", path.display()));
            std::fs::write(stream, b"fixture-stage-stream").map_err(|_| VaultError::Unavailable)?;
        } else if fault == PUBLISH_FAULT_STAGE_SWAP {
            let moved = path.with_extension("owned-stage");
            move_file_write_through(path, &moved).map_err(|_| VaultError::Unavailable)?;
            let mut replacement = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .map_err(|_| VaultError::Unavailable)?;
            replacement
                .write_all(b"replacement-path-object")
                .and_then(|()| replacement.flush())
                .and_then(|()| replacement.sync_all())
                .map_err(|_| VaultError::Unavailable)?;
        }
        Ok(())
    }

    #[cfg(not(feature = "runtime-fixture"))]
    fn apply_staged_fixture_fault(&self, _fault: u8, _path: &Path) -> Result<(), VaultError> {
        Ok(())
    }
}

const PUBLISH_FAULT_NONE: u8 = 0;
const PUBLISH_FAULT_BEFORE: u8 = 1;
const PUBLISH_FAULT_AFTER: u8 = 2;
const PUBLISH_FAULT_STAGE_ADS: u8 = 3;
const PUBLISH_FAULT_STAGE_SWAP: u8 = 4;

enum TargetState {
    Missing,
    Existing,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct FileIdentity {
    volume_serial_number: u64,
    identifier: [u8; 16],
}

impl FileIdentity {
    fn from_file_id_info(information: FILE_ID_INFO) -> Self {
        Self {
            volume_serial_number: information.VolumeSerialNumber,
            identifier: information.FileId.Identifier,
        }
    }
}

struct OwnedStagedPath {
    path: PathBuf,
    identity: FileIdentity,
    armed: bool,
}

struct CappedCleartextWriter {
    bytes: Zeroizing<Vec<u8>>,
}

impl CappedCleartextWriter {
    fn new() -> Result<Self, VaultError> {
        let mut bytes = Zeroizing::new(Vec::new());
        bytes
            .try_reserve_exact(MAX_CLEAR_BYTES)
            .map_err(|_| VaultError::Unavailable)?;
        Ok(Self { bytes })
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.bytes.len()
    }

    #[cfg(test)]
    fn capacity(&self) -> usize {
        self.bytes.capacity()
    }

    #[cfg(test)]
    fn base_pointer(&self) -> *const u8 {
        self.bytes.as_ptr()
    }

    fn into_bytes(self) -> Zeroizing<Vec<u8>> {
        self.bytes
    }
}

impl Write for CappedCleartextWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let next_len = self
            .bytes
            .len()
            .checked_add(bytes.len())
            .filter(|next_len| *next_len <= MAX_CLEAR_BYTES)
            .ok_or_else(|| io::Error::other("vault cleartext exceeds its fixed bound"))?;
        debug_assert!(next_len <= self.bytes.capacity());
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl OwnedStagedPath {
    fn new(path: PathBuf, identity: FileIdentity) -> Self {
        Self {
            path,
            identity,
            armed: true,
        }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for OwnedStagedPath {
    fn drop(&mut self) {
        if self.armed {
            delete_file_if_identity_matches(&self.path, self.identity);
        }
    }
}

fn decode_payload(ciphertext: &[u8]) -> Result<VaultPayload, VaultError> {
    if ciphertext.is_empty() || ciphertext.len() > MAX_CIPHERTEXT_BYTES {
        return Err(VaultError::Corrupt);
    }
    let clear = unprotect_for_current_user(ciphertext).map_err(|_| VaultError::Corrupt)?;
    if clear.len() > MAX_CLEAR_BYTES {
        return Err(VaultError::Corrupt);
    }
    let header: VaultVersionHeader =
        serde_json::from_slice(&clear).map_err(|_| VaultError::Corrupt)?;
    if header.schema_version != VAULT_SCHEMA_VERSION {
        return Err(VaultError::Unsupported);
    }
    let payload: VaultPayload = serde_json::from_slice(&clear).map_err(|_| VaultError::Corrupt)?;
    payload.validate()?;
    Ok(payload)
}

fn vault_mutex() -> &'static Mutex<()> {
    static MUTEX: OnceLock<Mutex<()>> = OnceLock::new();
    MUTEX.get_or_init(|| Mutex::new(()))
}

fn validate_application_home_handle(path: &Path) -> Result<(), VaultError> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .custom_flags((FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS).0);
    let directory = options.open(path).map_err(|_| VaultError::Unavailable)?;
    let metadata = directory.metadata().map_err(|_| VaultError::Unavailable)?;
    if !metadata.is_dir() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0 {
        return Err(VaultError::Unavailable);
    }
    Ok(())
}

fn open_create_new_file(path: &Path, readable: bool) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options
        .read(readable)
        .write(true)
        .create_new(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0);
    let file = options.open(path)?;
    validate_file_handle_io(&file)?;
    Ok(file)
}

fn open_existing_validated_file(path: &Path, writable: bool) -> Result<Option<File>, VaultError> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .write(writable)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0);
    let file = match options.open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(VaultError::Unavailable),
    };
    validate_file_handle(&file)?;
    Ok(Some(file))
}

fn validate_file_handle(file: &File) -> Result<(), VaultError> {
    validate_file_handle_io(file).map_err(|_| VaultError::Unavailable)
}

fn validate_file_handle_io(file: &File) -> io::Result<()> {
    let metadata = file.metadata()?;
    let handle_information = file_information(file)?;
    if !metadata.is_file()
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0
        || handle_information.nNumberOfLinks != 1
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid vault storage object",
        ));
    }
    validate_unnamed_data_stream_only(file)?;
    Ok(())
}

fn file_information(file: &File) -> io::Result<BY_HANDLE_FILE_INFORMATION> {
    let mut information = BY_HANDLE_FILE_INFORMATION::default();
    // SAFETY: the borrowed standard-library file handle remains valid for the call and
    // the output points to an initialized structure owned by this stack frame.
    unsafe { GetFileInformationByHandle(HANDLE(file.as_raw_handle()), &mut information) }
        .map_err(|_| io::Error::other("could not inspect vault storage object"))?;
    Ok(information)
}

fn file_identity(file: &File) -> io::Result<FileIdentity> {
    let mut information = FILE_ID_INFO::default();
    let information_size = u32::try_from(std::mem::size_of::<FILE_ID_INFO>())
        .map_err(|_| io::Error::other("invalid file identity structure size"))?;
    // SAFETY: FILE_ID_INFO is correctly aligned on this stack frame, the mutable pointer
    // remains valid for its exact checked size during the synchronous call, and the file
    // handle is borrowed from a live `File`. Failure preserves the staged object.
    unsafe {
        GetFileInformationByHandleEx(
            HANDLE(file.as_raw_handle()),
            FileIdInfo,
            std::ptr::from_mut(&mut information).cast(),
            information_size,
        )
    }
    .map_err(|_| io::Error::other("could not inspect vault storage identity"))?;
    Ok(FileIdentity::from_file_id_info(information))
}

fn validate_unnamed_data_stream_only(file: &File) -> io::Result<()> {
    const UNNAMED_DATA_STREAM: &[u16] = &[
        b':' as u16,
        b':' as u16,
        b'$' as u16,
        b'D' as u16,
        b'A' as u16,
        b'T' as u16,
        b'A' as u16,
    ];
    const STREAM_HEADER_BYTES: usize = std::mem::offset_of!(FILE_STREAM_INFO, StreamName);

    let aligned_words = MAX_STREAM_QUERY_BYTES.div_ceil(std::mem::size_of::<u64>());
    let mut storage = Zeroizing::new(vec![0u64; aligned_words]);
    let buffer_bytes = storage.len() * std::mem::size_of::<u64>();
    let query_size =
        u32::try_from(buffer_bytes).map_err(|_| io::Error::other("invalid stream query bound"))?;

    // SAFETY: `Vec<u64>` provides the documented 8-byte alignment for FILE_STREAM_INFO,
    // the pointer remains valid and writable for the fixed query size, and the handle is
    // borrowed from a live `File`. Any query failure is intentionally fail-closed.
    unsafe {
        GetFileInformationByHandleEx(
            HANDLE(file.as_raw_handle()),
            FileStreamInfo,
            storage.as_mut_ptr().cast(),
            query_size,
        )
    }
    .map_err(|_| io::Error::other("could not enumerate vault storage streams"))?;

    if std::mem::size_of::<FILE_STREAM_INFO>() > buffer_bytes
        || STREAM_HEADER_BYTES + std::mem::size_of::<u16>() > buffer_bytes
    {
        return Err(io::Error::other("invalid stream query layout"));
    }

    let base = storage.as_ptr().cast::<u8>();
    // SAFETY: the allocation is 8-byte aligned, the query populated at least the first
    // FILE_STREAM_INFO on success, and the fixed buffer was checked to contain its header.
    let stream = unsafe { &*base.cast::<FILE_STREAM_INFO>() };
    let name_bytes = usize::try_from(stream.StreamNameLength)
        .map_err(|_| io::Error::other("invalid stream name length"))?;
    if name_bytes == 0 || name_bytes % std::mem::size_of::<u16>() != 0 {
        return Err(io::Error::other("invalid stream name length"));
    }
    let name_end = STREAM_HEADER_BYTES
        .checked_add(name_bytes)
        .filter(|end| *end <= buffer_bytes)
        .ok_or_else(|| io::Error::other("stream name exceeds query buffer"))?;
    if stream.NextEntryOffset != 0 {
        let next = usize::try_from(stream.NextEntryOffset)
            .map_err(|_| io::Error::other("invalid stream entry offset"))?;
        let next_header_end = next
            .checked_add(STREAM_HEADER_BYTES)
            .ok_or_else(|| io::Error::other("invalid stream entry offset"))?;
        if next % std::mem::align_of::<FILE_STREAM_INFO>() != 0
            || next < name_end
            || next_header_end > buffer_bytes
        {
            return Err(io::Error::other("invalid stream entry offset"));
        }
        return Err(io::Error::other("named stream is not allowed"));
    }

    let name_units = name_bytes / std::mem::size_of::<u16>();
    let name_pointer = unsafe { base.add(STREAM_HEADER_BYTES).cast::<u16>() };
    // SAFETY: StreamName begins at an aligned WCHAR field and `name_end` was checked
    // against the bounded query allocation before constructing this exact-length slice.
    let name = unsafe { std::slice::from_raw_parts(name_pointer, name_units) };
    if name != UNNAMED_DATA_STREAM {
        return Err(io::Error::other("named stream is not allowed"));
    }
    Ok(())
}

fn delete_file_if_identity_matches(path: &Path, expected_identity: FileIdentity) {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .access_mode(GENERIC_READ.0 | DELETE.0)
        .share_mode(FILE_SHARE_READ.0)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0);
    let Ok(file) = options.open(path) else {
        return;
    };
    let Ok(metadata) = file.metadata() else {
        return;
    };
    let Ok(information) = file_information(&file) else {
        return;
    };
    let Ok(actual_identity) = file_identity(&file) else {
        return;
    };
    if !metadata.is_file()
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0
        || information.nNumberOfLinks != 1
        || actual_identity != expected_identity
    {
        return;
    }

    let disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
    let disposition_size = u32::try_from(std::mem::size_of::<FILE_DISPOSITION_INFO>())
        .expect("FILE_DISPOSITION_INFO size fits u32");
    // SAFETY: this exact still-live handle was opened with DELETE access, its stable
    // identity was compared while the handle prevented a pathname swap, and the input
    // points to a correctly sized FILE_DISPOSITION_INFO for the synchronous call.
    let _ = unsafe {
        SetFileInformationByHandle(
            HANDLE(file.as_raw_handle()),
            FileDispositionInfo,
            std::ptr::from_ref(&disposition).cast(),
            disposition_size,
        )
    };
}

fn is_connection_id(value: &str) -> bool {
    value.len() == 32
        && value
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

#[cfg(feature = "runtime-fixture")]
fn fixture_connection(
    connection_id: &str,
    secret: &Zeroizing<String>,
) -> Result<StoredAiConnection, VaultError> {
    if !is_connection_id(connection_id) || secret.is_empty() {
        return Err(VaultError::Invalid);
    }
    StoredAiConnection::new(
        connection_id.to_owned(),
        "Vault fixture".to_owned(),
        AiConnectionConfiguration::DirectProviderKey {
            provider: AiProviderKind::OpenAi,
        },
        StoredCredential::from_api_key(SecretString::new(secret.to_string()))?,
    )
}

fn replace_file(target: &Path, staged: &Path) -> windows_core::Result<()> {
    let target = wide_path(target);
    let staged = wide_path(staged);
    // SAFETY: both buffers are terminated and remain alive for the call; no backup is requested.
    unsafe {
        ReplaceFileW(
            PCWSTR(target.as_ptr()),
            PCWSTR(staged.as_ptr()),
            PCWSTR::null(),
            REPLACE_FILE_FLAGS(0),
            None,
            None,
        )
    }
}

fn move_file_write_through(staged: &Path, target: &Path) -> windows_core::Result<()> {
    let staged = wide_path(staged);
    let target = wide_path(target);
    // SAFETY: both buffers are terminated and remain alive for the call.
    unsafe {
        MoveFileExW(
            PCWSTR(staged.as_ptr()),
            PCWSTR(target.as_ptr()),
            MOVEFILE_WRITE_THROUGH,
        )
    }
}

fn wide_path(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use windows::Win32::Storage::FileSystem::{FILE_ID_128, FILE_ID_INFO};

    use super::{CappedCleartextWriter, FileIdentity, MAX_CLEAR_BYTES};

    #[test]
    fn cleartext_writer_never_accepts_or_retains_bytes_past_the_cap() {
        let mut writer = CappedCleartextWriter::new().unwrap();
        let initial_capacity = writer.capacity();
        let initial_pointer = writer.base_pointer();
        assert!(initial_capacity >= MAX_CLEAR_BYTES);

        let escaped_chunk = "\"\\".repeat(64);
        for _ in 0..1_024 {
            serde_json::to_writer(&mut writer, &escaped_chunk).unwrap();
            assert_eq!(writer.capacity(), initial_capacity);
            assert_eq!(writer.base_pointer(), initial_pointer);
        }
        let remaining = MAX_CLEAR_BYTES - writer.len();
        writer.write_all(&vec![b'x'; remaining]).unwrap();
        assert_eq!(writer.len(), MAX_CLEAR_BYTES);
        assert_eq!(writer.capacity(), initial_capacity);
        assert_eq!(writer.base_pointer(), initial_pointer);

        assert!(writer.write_all(b"x").is_err());
        assert_eq!(writer.len(), MAX_CLEAR_BYTES);
        assert_eq!(writer.capacity(), initial_capacity);
        assert_eq!(writer.base_pointer(), initial_pointer);
    }

    #[test]
    fn file_identity_retains_the_full_volume_and_128_bit_identifier() {
        let first = FileIdentity::from_file_id_info(FILE_ID_INFO {
            VolumeSerialNumber: 0x0102_0304_0506_0708,
            FileId: FILE_ID_128 {
                Identifier: [
                    0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc,
                    0xdd, 0xee, 0xff,
                ],
            },
        });
        let second = FileIdentity::from_file_id_info(FILE_ID_INFO {
            VolumeSerialNumber: 0x0102_0304_0506_0708,
            FileId: FILE_ID_128 {
                Identifier: [
                    0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc,
                    0xdd, 0xee, 0xfe,
                ],
            },
        });

        assert_eq!(first.volume_serial_number, 0x0102_0304_0506_0708);
        assert_eq!(first.identifier.len(), 16);
        assert!(first != second);
    }
}
