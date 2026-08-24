use std::{
    fmt::{self, Write},
    sync::{
        atomic::{AtomicBool, AtomicU8, Ordering},
        Arc, Mutex, OnceLock,
    },
};

use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior};
use serde::{Deserialize, Deserializer, Serialize};
use ts_rs::TS;
use zeroize::{Zeroize, ZeroizeOnDrop};

use super::{
    contract::{
        catalogue_sha256, normalize_label, AccountLoginProgress, ActiveAiConfiguration,
        ActiveAiConfigurationView, ActiveAiReadiness, AiConnectionConfiguration, AiConnectionId,
        AiConnectionRevision, AiConnectionStatus, AiConnectionView, AiModelView,
        AiNetworkDestinationClass, AiProbeEvidence, AiProviderKind, AiReasoningSelection,
        CapabilitySupport, CredentialGeneration,
    },
    vault::{
        AiConnectionVault, SecretString, StoredAiConnection, StoredCredential, VaultError,
        VaultPayload,
    },
};
use crate::application_settings::GeneralApplicationPreferences;

const MAX_ADAPTER_VERSION_BYTES: usize = 128;
const MAX_CREDENTIAL_BYTES: usize = 16 * 1024;
const MAX_CUSTOM_VALUE_BYTES: usize = 4 * 1024;
const MAX_ACCOUNT_ID_BYTES: usize = 4 * 1024;
const MAX_PROBE_MODELS: usize = 500;
const MAX_REASONING_OPTIONS: usize = 32;
const MAX_MODEL_ID_BYTES: usize = 256;
const MAX_PROBE_LABEL_BYTES: usize = 120;
const MAX_PROBE_DESCRIPTION_BYTES: usize = 4 * 1024;
const MAX_PROBE_METADATA_BYTES: usize = 128;
const SHA256_HEX_BYTES: usize = 64;
const MAX_DATA_DESTINATION_BYTES: usize = 2_048;

pub struct SecretInput(Option<SecretString>);

impl SecretInput {
    pub fn new(value: impl Into<String>) -> Self {
        Self(Some(SecretString::new(value.into())))
    }

    fn as_str(&self) -> &str {
        self.0.as_ref().map_or("", SecretString::as_str)
    }

    fn len(&self) -> usize {
        self.as_str().len()
    }

    fn is_empty(&self) -> bool {
        self.as_str().is_empty()
    }

    fn take(&mut self) -> Result<SecretString, AiConnectionError> {
        self.0.take().ok_or(AiConnectionError::InvalidCommand)
    }
}

impl<'de> Deserialize<'de> for SecretInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer).map(Self::new)
    }
}

impl Zeroize for SecretInput {
    fn zeroize(&mut self) {
        self.0.zeroize();
    }
}

impl ZeroizeOnDrop for SecretInput {}

impl Drop for SecretInput {
    fn drop(&mut self) {
        self.zeroize();
    }
}

impl fmt::Debug for SecretInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretInput([REDACTED])")
    }
}

type Clock = dyn Fn() -> String + Send + Sync;
type NonterminalReferenceCheck = dyn Fn(&str) -> Result<bool, AiConnectionError> + Send + Sync;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiAdapterVersions {
    codex: String,
    general: String,
}

impl AiAdapterVersions {
    pub fn new(
        codex: impl Into<String>,
        general: impl Into<String>,
    ) -> Result<Self, AiConnectionError> {
        let versions = Self {
            codex: codex.into(),
            general: general.into(),
        };
        versions.validate()?;
        Ok(versions)
    }

    fn validate(&self) -> Result<(), AiConnectionError> {
        if [self.codex.as_str(), self.general.as_str()]
            .into_iter()
            .any(|version| version.is_empty() || version.len() > MAX_ADAPTER_VERSION_BYTES)
        {
            return Err(AiConnectionError::InvalidCommand);
        }
        Ok(())
    }

    fn for_provider(&self, provider: AiProviderKind) -> &str {
        if provider == AiProviderKind::Codex {
            &self.codex
        } else {
            &self.general
        }
    }
}

#[derive(Deserialize, TS, Zeroize, ZeroizeOnDrop)]
#[serde(deny_unknown_fields)]
pub struct SecretNameValueInput {
    pub name: String,
    #[ts(type = "string")]
    pub value: SecretInput,
}

impl fmt::Debug for SecretNameValueInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretNameValueInput([REDACTED])")
    }
}

#[derive(Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AiCredentialInput {
    Account {
        #[ts(type = "string")]
        access_token: SecretInput,
        #[ts(type = "string | null")]
        refresh_token: Option<SecretInput>,
        expires_at: String,
        verified_account_id: String,
    },
    ApiKey {
        #[ts(type = "string")]
        api_key: SecretInput,
        custom_header_values: Vec<SecretNameValueInput>,
        custom_query_values: Vec<SecretNameValueInput>,
    },
}

impl Zeroize for AiCredentialInput {
    fn zeroize(&mut self) {
        match self {
            Self::Account {
                access_token,
                refresh_token,
                expires_at,
                verified_account_id,
            } => {
                access_token.zeroize();
                refresh_token.zeroize();
                expires_at.zeroize();
                verified_account_id.zeroize();
            }
            Self::ApiKey {
                api_key,
                custom_header_values,
                custom_query_values,
            } => {
                api_key.zeroize();
                custom_header_values.zeroize();
                custom_query_values.zeroize();
            }
        }
    }
}

impl ZeroizeOnDrop for AiCredentialInput {}

impl Drop for AiCredentialInput {
    fn drop(&mut self) {
        self.zeroize();
    }
}

impl fmt::Debug for AiCredentialInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AiCredentialInput([REDACTED])")
    }
}

#[derive(Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct CreateAiConnectionCommand {
    pub display_name: String,
    pub configuration: AiConnectionConfiguration,
    pub credential: AiCredentialInput,
}

impl Zeroize for CreateAiConnectionCommand {
    fn zeroize(&mut self) {
        self.display_name.zeroize();
        self.credential.zeroize();
    }
}

impl ZeroizeOnDrop for CreateAiConnectionCommand {}

impl Drop for CreateAiConnectionCommand {
    fn drop(&mut self) {
        self.zeroize();
    }
}

impl fmt::Debug for CreateAiConnectionCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("CreateAiConnectionCommand([REDACTED])")
    }
}

#[derive(Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct UpdateAiConnectionCommand {
    pub connection_id: String,
    pub expected_execution_revision: u64,
    pub expected_credential_generation: u64,
    pub display_name: String,
    pub configuration: Option<AiConnectionConfiguration>,
    pub replacement_credential: Option<AiCredentialInput>,
}

impl Zeroize for UpdateAiConnectionCommand {
    fn zeroize(&mut self) {
        self.display_name.zeroize();
        self.replacement_credential.zeroize();
    }
}

impl ZeroizeOnDrop for UpdateAiConnectionCommand {}

impl Drop for UpdateAiConnectionCommand {
    fn drop(&mut self) {
        self.zeroize();
    }
}

impl fmt::Debug for UpdateAiConnectionCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("UpdateAiConnectionCommand([REDACTED])")
    }
}

pub struct SameAccountTokenRefreshCommand {
    pub connection_id: String,
    pub expected_execution_revision: u64,
    pub expected_credential_generation: u64,
    pub verified_account_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(try_from = "SetActiveAiConfigurationCommandInput")]
pub struct SetActiveAiConfigurationCommand {
    pub connection_id: String,
    pub expected_execution_revision: u64,
    pub provider: AiProviderKind,
    pub endpoint_fingerprint: String,
    pub model_id: String,
    pub reasoning: AiReasoningSelection,
    pub adapter_version: String,
    pub catalogue_sha256: String,
    pub destination_class: AiNetworkDestinationClass,
    pub confirmed_data_destination: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SetActiveAiConfigurationCommandInput {
    connection_id: String,
    expected_execution_revision: u64,
    provider: AiProviderKind,
    endpoint_fingerprint: String,
    model_id: String,
    reasoning: AiReasoningSelection,
    adapter_version: String,
    catalogue_sha256: String,
    destination_class: AiNetworkDestinationClass,
    confirmed_data_destination: String,
}

impl TryFrom<SetActiveAiConfigurationCommandInput> for SetActiveAiConfigurationCommand {
    type Error = AiConnectionError;

    fn try_from(value: SetActiveAiConfigurationCommandInput) -> Result<Self, Self::Error> {
        let command = Self {
            connection_id: value.connection_id,
            expected_execution_revision: value.expected_execution_revision,
            provider: value.provider,
            endpoint_fingerprint: value.endpoint_fingerprint,
            model_id: value.model_id,
            reasoning: value.reasoning,
            adapter_version: value.adapter_version,
            catalogue_sha256: value.catalogue_sha256,
            destination_class: value.destination_class,
            confirmed_data_destination: value.confirmed_data_destination,
        };
        command.validate()?;
        Ok(command)
    }
}

impl SetActiveAiConfigurationCommand {
    fn validate(&self) -> Result<(), AiConnectionError> {
        AiConnectionId::parse(self.connection_id.clone())
            .map_err(|_| AiConnectionError::InvalidCommand)?;
        AiConnectionRevision::new(self.expected_execution_revision)
            .map_err(|_| AiConnectionError::InvalidCommand)?;
        if !is_lower_sha256(&self.endpoint_fingerprint)
            || self.model_id.is_empty()
            || self.model_id.len() > MAX_MODEL_ID_BYTES
            || !reasoning_selection_is_bounded(&self.reasoning)
            || self.adapter_version.is_empty()
            || self.adapter_version.len() > MAX_ADAPTER_VERSION_BYTES
            || !is_lower_sha256(&self.catalogue_sha256)
            || self.confirmed_data_destination.is_empty()
            || self.confirmed_data_destination.len() > MAX_DATA_DESTINATION_BYTES
        {
            return Err(AiConnectionError::InvalidCommand);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SetAiConnectionEnabledCommand {
    pub connection_id: String,
    pub expected_execution_revision: u64,
    pub expected_credential_generation: u64,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct DisconnectAiConnectionCommand {
    pub connection_id: String,
    pub expected_execution_revision: u64,
    pub expected_credential_generation: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct DeleteAiConnectionCommand {
    pub connection_id: String,
    pub expected_execution_revision: u64,
    pub expected_credential_generation: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ClearActiveAiConfigurationCommand {}

impl Zeroize for SameAccountTokenRefreshCommand {
    fn zeroize(&mut self) {
        self.verified_account_id.zeroize();
    }
}

impl ZeroizeOnDrop for SameAccountTokenRefreshCommand {}

impl Drop for SameAccountTokenRefreshCommand {
    fn drop(&mut self) {
        self.zeroize();
    }
}

impl fmt::Debug for SameAccountTokenRefreshCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SameAccountTokenRefreshCommand([REDACTED])")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS, thiserror::Error)]
#[serde(rename_all = "snake_case")]
pub enum AiConnectionError {
    #[error("the AI connection command is invalid")]
    InvalidCommand,
    #[error("the AI connection was not found")]
    NotFound,
    #[error("the AI connection changed before the operation completed")]
    Conflict,
    #[error("the AI connection counter cannot advance")]
    RevisionOverflow,
    #[error("the AI connection vault is unavailable")]
    VaultUnavailable,
    #[error("the AI application settings store is unavailable")]
    StoreUnavailable,
    #[error("the AI application settings commit outcome is indeterminate")]
    StoreIndeterminate,
    #[error("the AI connection is disabled")]
    Disabled,
    #[error("the AI connection requires authentication")]
    AuthenticationRequired,
    #[error("the AI connection must be tested")]
    ProbeRequired,
    #[error("the AI adapter is unavailable")]
    WorkerUnavailable,
    #[error("the tested AI capability changed")]
    CapabilityChanged,
    #[error("the active AI connection cannot be deleted")]
    ActiveConnection,
    #[error("an in-progress run references the AI connection")]
    ReferencedByNonterminalRun,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ApplicationAiSettingsView {
    pub connections: Vec<AiConnectionView>,
    pub active_configuration: Option<ActiveAiConfigurationView>,
    pub readiness: ActiveAiReadiness,
    pub login: Option<AccountLoginProgress>,
}

#[cfg(feature = "runtime-fixture")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AiConnectionRecordFixtureView {
    pub execution_revision: u64,
    pub credential_generation: u64,
    pub has_probe_evidence: bool,
}

#[cfg(feature = "runtime-fixture")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FixtureSettingsCommitOutcome {
    ErrorAfterCommit = 1,
    ErrorBeforeCommit = 2,
    IndeterminateReread = 3,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredFinalApplicationSettings {
    general_preferences: GeneralApplicationPreferences,
    #[serde(deserialize_with = "deserialize_required_active_configuration")]
    active_ai_configuration: Option<ActiveAiConfiguration>,
}

#[derive(Clone, PartialEq, Eq)]
struct StoredFinalApplicationSettingsRow {
    settings: StoredFinalApplicationSettings,
    settings_json: String,
    updated_at: String,
}

enum SettingsCommitDisposition {
    Success,
    Reconcile,
    Indeterminate,
}

fn deserialize_required_active_configuration<'de, D>(
    deserializer: D,
) -> Result<Option<ActiveAiConfiguration>, D::Error>
where
    D: Deserializer<'de>,
{
    Option::<ActiveAiConfiguration>::deserialize(deserializer)
}

pub struct AiConnectionRepository {
    vault: AiConnectionVault,
    installation: Arc<Mutex<Connection>>,
    adapter_versions: AiAdapterVersions,
    clock: Arc<Clock>,
    nonterminal_reference_check: Arc<NonterminalReferenceCheck>,
    store_indeterminate: AtomicBool,
    #[cfg(feature = "runtime-fixture")]
    next_settings_commit_outcome: AtomicU8,
}

impl AiConnectionRepository {
    pub fn new(
        vault: AiConnectionVault,
        installation: Arc<Mutex<Connection>>,
        adapter_versions: AiAdapterVersions,
        clock: Arc<Clock>,
        nonterminal_reference_check: Arc<NonterminalReferenceCheck>,
    ) -> Self {
        Self {
            vault,
            installation,
            adapter_versions,
            clock,
            nonterminal_reference_check,
            store_indeterminate: AtomicBool::new(false),
            #[cfg(feature = "runtime-fixture")]
            next_settings_commit_outcome: AtomicU8::new(0),
        }
    }

    pub fn create_connection(
        &self,
        mut command: CreateAiConnectionCommand,
    ) -> Result<AiConnectionView, AiConnectionError> {
        self.adapter_versions.validate()?;
        let display_name = normalize_label(&command.display_name)
            .map_err(|_| AiConnectionError::InvalidCommand)?;
        command
            .configuration
            .validate()
            .map_err(|_| AiConnectionError::InvalidCommand)?;
        let configuration = command.configuration.clone();
        let credential = take_credential(&configuration, &mut command.credential)?;
        let connection_id = new_connection_id()?;
        let stored = StoredAiConnection::new(
            connection_id.clone(),
            display_name,
            configuration,
            credential,
        )
        .map_err(map_vault_error)?;

        let _gate = connection_gate()
            .lock()
            .map_err(|_| AiConnectionError::VaultUnavailable)?;
        self.vault
            .mutate_current_project(|payload| {
                payload.insert(stored)?;
                self.connection_view_vault(payload, &connection_id)
            })
            .map_err(map_vault_error)
    }

    pub fn replace_connection_configuration(
        &self,
        mut command: UpdateAiConnectionCommand,
    ) -> Result<AiConnectionView, AiConnectionError> {
        validate_connection_cas(
            &command.connection_id,
            command.expected_execution_revision,
            command.expected_credential_generation,
        )?;
        let display_name = normalize_label(&command.display_name)
            .map_err(|_| AiConnectionError::InvalidCommand)?;
        let configuration = command
            .configuration
            .clone()
            .ok_or(AiConnectionError::InvalidCommand)?;
        configuration
            .validate()
            .map_err(|_| AiConnectionError::InvalidCommand)?;
        let replacement = command
            .replacement_credential
            .as_mut()
            .map(|credential| take_credential(&configuration, credential))
            .transpose()?;
        let connection_id = command.connection_id.clone();
        let _gate = connection_gate()
            .lock()
            .map_err(|_| AiConnectionError::VaultUnavailable)?;
        self.vault
            .mutate_current_project(|payload| {
                payload.replace_connection_configuration(
                    &connection_id,
                    command.expected_execution_revision,
                    command.expected_credential_generation,
                    display_name,
                    configuration,
                    replacement,
                )?;
                self.connection_view_vault(payload, &connection_id)
            })
            .map_err(map_vault_error)
    }

    pub fn rename_connection(
        &self,
        mut command: UpdateAiConnectionCommand,
    ) -> Result<AiConnectionView, AiConnectionError> {
        validate_connection_cas(
            &command.connection_id,
            command.expected_execution_revision,
            command.expected_credential_generation,
        )?;
        if command.configuration.is_some() || command.replacement_credential.is_some() {
            return Err(AiConnectionError::InvalidCommand);
        }
        let display_name = normalize_label(&command.display_name)
            .map_err(|_| AiConnectionError::InvalidCommand)?;
        let connection_id = command.connection_id.clone();
        command.display_name.zeroize();
        let _gate = connection_gate()
            .lock()
            .map_err(|_| AiConnectionError::VaultUnavailable)?;
        self.vault
            .mutate_current_project(|payload| {
                payload.rename_connection(
                    &connection_id,
                    command.expected_execution_revision,
                    command.expected_credential_generation,
                    display_name,
                )?;
                self.connection_view_vault(payload, &connection_id)
            })
            .map_err(map_vault_error)
    }

    pub fn rotate_same_account_tokens(
        &self,
        command: SameAccountTokenRefreshCommand,
        mut credential: AiCredentialInput,
    ) -> Result<AiConnectionView, AiConnectionError> {
        validate_connection_cas(
            &command.connection_id,
            command.expected_execution_revision,
            command.expected_credential_generation,
        )?;
        let replacement = match &mut credential {
            AiCredentialInput::Account {
                access_token,
                refresh_token,
                expires_at,
                verified_account_id,
            } if verified_account_id == &command.verified_account_id => take_account_credential(
                verified_account_id,
                access_token,
                refresh_token,
                expires_at,
            )?,
            AiCredentialInput::Account { .. } | AiCredentialInput::ApiKey { .. } => {
                return Err(AiConnectionError::InvalidCommand)
            }
        };
        let connection_id = command.connection_id.clone();
        let verified_account_id = command.verified_account_id.clone();
        let _gate = connection_gate()
            .lock()
            .map_err(|_| AiConnectionError::VaultUnavailable)?;
        self.vault
            .mutate_current_project(|payload| {
                payload.rotate_same_account_tokens(
                    &connection_id,
                    command.expected_execution_revision,
                    command.expected_credential_generation,
                    &verified_account_id,
                    replacement,
                )?;
                self.connection_view_vault(payload, &connection_id)
            })
            .map_err(map_vault_error)
    }

    pub fn set_enabled(
        &self,
        command: SetAiConnectionEnabledCommand,
    ) -> Result<AiConnectionView, AiConnectionError> {
        validate_connection_cas(
            &command.connection_id,
            command.expected_execution_revision,
            command.expected_credential_generation,
        )?;
        let connection_id = command.connection_id.clone();
        let _gate = connection_gate()
            .lock()
            .map_err(|_| AiConnectionError::VaultUnavailable)?;
        self.vault
            .mutate_current_project(|payload| {
                payload.set_enabled(
                    &connection_id,
                    command.expected_execution_revision,
                    command.expected_credential_generation,
                    command.enabled,
                )?;
                self.connection_view_vault(payload, &connection_id)
            })
            .map_err(map_vault_error)
    }

    pub fn disconnect(
        &self,
        command: DisconnectAiConnectionCommand,
    ) -> Result<AiConnectionView, AiConnectionError> {
        validate_connection_cas(
            &command.connection_id,
            command.expected_execution_revision,
            command.expected_credential_generation,
        )?;
        let connection_id = command.connection_id.clone();
        let _gate = connection_gate()
            .lock()
            .map_err(|_| AiConnectionError::VaultUnavailable)?;
        self.vault
            .mutate_current_project(|payload| {
                payload.disconnect(
                    &connection_id,
                    command.expected_execution_revision,
                    command.expected_credential_generation,
                )?;
                self.connection_view_vault(payload, &connection_id)
            })
            .map_err(map_vault_error)
    }

    pub fn delete_connection(
        &self,
        command: DeleteAiConnectionCommand,
    ) -> Result<(), AiConnectionError> {
        validate_connection_cas(
            &command.connection_id,
            command.expected_execution_revision,
            command.expected_credential_generation,
        )?;
        let _gate = connection_gate()
            .lock()
            .map_err(|_| AiConnectionError::VaultUnavailable)?;
        self.require_store_determinate()?;
        let mut operation_error = None;
        let result =
            self.vault
                .mutate_current(|payload| match self.delete_payload(payload, &command) {
                    Ok(()) => Ok(()),
                    Err(error) => {
                        operation_error = Some(error);
                        Err(VaultError::Invalid)
                    }
                });
        match result {
            Ok(_) => Ok(()),
            Err(VaultError::Invalid) if operation_error.is_some() => {
                Err(operation_error.unwrap_or(AiConnectionError::InvalidCommand))
            }
            Err(error) => Err(map_vault_error(error)),
        }
    }

    pub fn activate(
        &self,
        command: SetActiveAiConfigurationCommand,
    ) -> Result<ApplicationAiSettingsView, AiConnectionError> {
        command.validate()?;
        self.adapter_versions.validate()?;
        let _gate = connection_gate()
            .lock()
            .map_err(|_| AiConnectionError::VaultUnavailable)?;
        self.require_store_determinate()?;
        self.vault
            .with_locked_payload(|payload| Ok(self.activate_payload(payload, &command)))
            .map_err(map_vault_error)?
    }

    pub fn clear_active(
        &self,
        _command: ClearActiveAiConfigurationCommand,
    ) -> Result<ApplicationAiSettingsView, AiConnectionError> {
        let _gate = connection_gate()
            .lock()
            .map_err(|_| AiConnectionError::VaultUnavailable)?;
        self.require_store_determinate()?;
        self.vault
            .with_locked_payload(|payload| Ok(self.clear_active_under_vault_lock(payload)))
            .map_err(map_vault_error)?
    }

    pub fn record_probe(
        &self,
        evidence: AiProbeEvidence,
    ) -> Result<AiConnectionView, AiConnectionError> {
        self.adapter_versions.validate()?;
        let evidence = validate_probe_evidence(evidence)?;
        let connection_id = evidence.connection_id.as_str().to_owned();
        let _gate = connection_gate()
            .lock()
            .map_err(|_| AiConnectionError::VaultUnavailable)?;
        self.vault
            .mutate_current_project(|payload| {
                let connection = payload.connection(&connection_id)?;
                let configuration = connection.configuration();
                let (endpoint_fingerprint, adapter_version) = (
                    configuration
                        .endpoint_fingerprint()
                        .map_err(|_| VaultError::Invalid)?,
                    self.adapter_versions
                        .for_provider(configuration.provider())
                        .to_owned(),
                );
                payload.record_probe(evidence, &endpoint_fingerprint, &adapter_version)?;
                self.connection_view_vault(payload, &connection_id)
            })
            .map_err(map_vault_error)
    }

    pub fn inspect(&self) -> Result<ApplicationAiSettingsView, AiConnectionError> {
        let _gate = connection_gate()
            .lock()
            .map_err(|_| AiConnectionError::VaultUnavailable)?;
        self.require_store_determinate()?;
        self.inspect_under_gate()
    }

    #[cfg(feature = "runtime-fixture")]
    pub fn fixture_connection_state(
        &self,
        connection_id: &str,
    ) -> Result<AiConnectionRecordFixtureView, AiConnectionError> {
        let _gate = connection_gate()
            .lock()
            .map_err(|_| AiConnectionError::VaultUnavailable)?;
        self.vault
            .with_locked_payload(|payload| {
                payload
                    .connections()
                    .find(|connection| connection.connection_id() == connection_id)
                    .map(|connection| AiConnectionRecordFixtureView {
                        execution_revision: connection.execution_revision(),
                        credential_generation: connection.credential_generation(),
                        has_probe_evidence: connection.probe_evidence().is_some(),
                    })
                    .ok_or(VaultError::Invalid)
            })
            .map_err(map_vault_error)
    }

    #[cfg(feature = "runtime-fixture")]
    pub fn fixture_reject_after_secret_transfer(
        &self,
        mut command: CreateAiConnectionCommand,
    ) -> Result<(), AiConnectionError> {
        command
            .configuration
            .validate()
            .map_err(|_| AiConnectionError::InvalidCommand)?;
        let credential = take_credential(&command.configuration, &mut command.credential)?;
        drop(credential);
        Err(AiConnectionError::InvalidCommand)
    }

    #[cfg(feature = "runtime-fixture")]
    pub fn fixture_create_future_run<F>(
        &self,
        connection_id: &str,
        insert_tender_binding: F,
    ) -> Result<(), AiConnectionError>
    where
        F: FnOnce() -> Result<(), AiConnectionError>,
    {
        AiConnectionId::parse(connection_id.to_owned())
            .map_err(|_| AiConnectionError::InvalidCommand)?;
        let _gate = connection_gate()
            .lock()
            .map_err(|_| AiConnectionError::VaultUnavailable)?;
        self.require_store_determinate()?;
        self.vault
            .with_locked_payload(|payload| {
                Ok((|| {
                    payload.connection(connection_id).map_err(map_vault_error)?;
                    let mut installation = self
                        .installation
                        .lock()
                        .map_err(|_| AiConnectionError::StoreUnavailable)?;
                    let transaction = installation
                        .transaction_with_behavior(TransactionBehavior::Immediate)
                        .map_err(|_| AiConnectionError::StoreUnavailable)?;
                    insert_tender_binding()?;
                    transaction
                        .commit()
                        .map_err(|_| AiConnectionError::StoreUnavailable)
                })())
            })
            .map_err(map_vault_error)?
    }

    #[cfg(feature = "runtime-fixture")]
    pub fn fixture_set_next_settings_commit_outcome(&self, outcome: FixtureSettingsCommitOutcome) {
        self.next_settings_commit_outcome
            .store(outcome as u8, Ordering::Release);
    }

    fn inspect_under_gate(&self) -> Result<ApplicationAiSettingsView, AiConnectionError> {
        match self
            .vault
            .with_locked_payload(|payload| Ok(self.inspect_payload(payload)))
        {
            Ok(view) => view,
            Err(_) => self.inspect_vault_unavailable(),
        }
    }

    fn inspect_vault_unavailable(&self) -> Result<ApplicationAiSettingsView, AiConnectionError> {
        let installation = self
            .installation
            .lock()
            .map_err(|_| AiConnectionError::StoreUnavailable)?;
        let settings = load_final_settings(&installation)?;
        Ok(ApplicationAiSettingsView {
            connections: Vec::new(),
            active_configuration: settings
                .active_ai_configuration
                .as_ref()
                .map(active_configuration_view),
            readiness: ActiveAiReadiness::VaultUnavailable,
            login: None,
        })
    }

    fn inspect_payload(
        &self,
        payload: &VaultPayload,
    ) -> Result<ApplicationAiSettingsView, AiConnectionError> {
        let installation = self
            .installation
            .lock()
            .map_err(|_| AiConnectionError::StoreUnavailable)?;
        let settings = load_final_settings(&installation)?;
        self.application_view(payload, &settings)
    }

    fn application_view(
        &self,
        payload: &VaultPayload,
        settings: &StoredFinalApplicationSettings,
    ) -> Result<ApplicationAiSettingsView, AiConnectionError> {
        let mut connections = payload
            .connections()
            .map(|connection| self.connection_view(connection))
            .collect::<Result<Vec<_>, _>>()?;
        connections.sort_by(|left, right| {
            left.display_name
                .cmp(&right.display_name)
                .then_with(|| left.connection_id.cmp(&right.connection_id))
        });
        let readiness = self.active_readiness(payload, settings.active_ai_configuration.as_ref());
        Ok(ApplicationAiSettingsView {
            connections,
            active_configuration: settings
                .active_ai_configuration
                .as_ref()
                .map(active_configuration_view),
            readiness,
            login: None,
        })
    }

    fn activate_payload(
        &self,
        payload: &VaultPayload,
        command: &SetActiveAiConfigurationCommand,
    ) -> Result<ApplicationAiSettingsView, AiConnectionError> {
        let connection = payload
            .connections()
            .find(|connection| connection.connection_id() == command.connection_id)
            .ok_or(AiConnectionError::NotFound)?;
        if connection.execution_revision() != command.expected_execution_revision {
            return Err(AiConnectionError::Conflict);
        }
        if !connection.enabled() {
            return Err(AiConnectionError::Disabled);
        }
        if !connection.has_credential() {
            return Err(AiConnectionError::AuthenticationRequired);
        }
        let configuration = connection.configuration();
        let provider = configuration.provider();
        let endpoint_fingerprint = configuration
            .endpoint_fingerprint()
            .map_err(|_| AiConnectionError::InvalidCommand)?;
        let data_destination = configuration
            .data_destination()
            .map_err(|_| AiConnectionError::InvalidCommand)?;
        let adapter_version = self.adapter_versions.for_provider(provider);
        if command.adapter_version != adapter_version {
            return Err(AiConnectionError::WorkerUnavailable);
        }
        if command.provider != provider
            || command.endpoint_fingerprint != endpoint_fingerprint
            || !configuration.accepts_destination_class(command.destination_class)
            || command.confirmed_data_destination != data_destination
        {
            return Err(AiConnectionError::InvalidCommand);
        }
        let evidence = connection
            .probe_evidence()
            .ok_or(AiConnectionError::ProbeRequired)?;
        if evidence.adapter_version != adapter_version {
            return Err(AiConnectionError::WorkerUnavailable);
        }
        let catalogue =
            catalogue_sha256(evidence).map_err(|_| AiConnectionError::CapabilityChanged)?;
        if command.catalogue_sha256 != catalogue {
            return Err(AiConnectionError::CapabilityChanged);
        }
        if command.model_id != evidence.tested_model_id
            || command.reasoning != evidence.tested_reasoning
            || command.destination_class != evidence.destination_class
        {
            return Err(AiConnectionError::CapabilityChanged);
        }
        let model = evidence
            .models
            .iter()
            .find(|model| model.model_id == command.model_id)
            .ok_or(AiConnectionError::CapabilityChanged)?;
        if !reasoning_is_activatable(model, &command.reasoning) {
            return Err(AiConnectionError::CapabilityChanged);
        }
        let activated_at = self.current_timestamp()?;
        let active = ActiveAiConfiguration {
            connection_id: super::contract::AiConnectionId::parse(command.connection_id.clone())
                .map_err(|_| AiConnectionError::InvalidCommand)?,
            execution_revision: super::contract::AiConnectionRevision::new(
                command.expected_execution_revision,
            )
            .map_err(|_| AiConnectionError::InvalidCommand)?,
            provider,
            endpoint_fingerprint,
            model_id: command.model_id.clone(),
            reasoning: command.reasoning.clone(),
            adapter_version: adapter_version.to_owned(),
            catalogue_sha256: catalogue,
            destination_class: evidence.destination_class,
            capabilities: model.capabilities.clone(),
            data_destination: data_destination.to_owned(),
            activated_at: activated_at.clone(),
        };
        let active: ActiveAiConfiguration = serde_json::from_value(
            serde_json::to_value(active).map_err(|_| AiConnectionError::InvalidCommand)?,
        )
        .map_err(|_| AiConnectionError::InvalidCommand)?;

        let mut installation = self
            .installation
            .lock()
            .map_err(|_| AiConnectionError::StoreUnavailable)?;
        let transaction = installation
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| AiConnectionError::StoreUnavailable)?;
        let prior = load_final_settings_row(&transaction)?;
        let mut settings = prior.settings.clone();
        settings.active_ai_configuration = Some(active);
        let intended = final_settings_row(settings, activated_at)?;
        let view = self.application_view(payload, &intended.settings)?;
        store_final_settings_row(&transaction, &intended)?;
        let disposition = self.finish_settings_transaction(transaction);
        self.reconcile_settings_commit(disposition, &installation, &prior, &intended, view)
    }

    fn clear_active_under_vault_lock(
        &self,
        payload: &VaultPayload,
    ) -> Result<ApplicationAiSettingsView, AiConnectionError> {
        let updated_at = self.current_timestamp()?;
        let mut installation = self
            .installation
            .lock()
            .map_err(|_| AiConnectionError::StoreUnavailable)?;
        let transaction = installation
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| AiConnectionError::StoreUnavailable)?;
        let prior = load_final_settings_row(&transaction)?;
        let mut settings = prior.settings.clone();
        settings.active_ai_configuration = None;
        let intended = final_settings_row(settings, updated_at)?;
        let view = self.application_view(payload, &intended.settings)?;
        store_final_settings_row(&transaction, &intended)?;
        let disposition = self.finish_settings_transaction(transaction);
        self.reconcile_settings_commit(disposition, &installation, &prior, &intended, view)
    }

    fn current_timestamp(&self) -> Result<String, AiConnectionError> {
        let timestamp = (self.clock)();
        if timestamp.is_empty() || timestamp.len() > MAX_PROBE_METADATA_BYTES {
            return Err(AiConnectionError::InvalidCommand);
        }
        Ok(timestamp)
    }

    fn finish_settings_transaction(
        &self,
        transaction: Transaction<'_>,
    ) -> SettingsCommitDisposition {
        let outcome = self.take_settings_commit_outcome();
        if outcome == 0 {
            return if transaction.commit().is_ok() {
                SettingsCommitDisposition::Success
            } else {
                SettingsCommitDisposition::Reconcile
            };
        }
        match outcome {
            1 | 3 => {
                let _ = transaction.commit();
                if outcome == 3 {
                    SettingsCommitDisposition::Indeterminate
                } else {
                    SettingsCommitDisposition::Reconcile
                }
            }
            2 => {
                let _ = transaction.rollback();
                SettingsCommitDisposition::Reconcile
            }
            _ => {
                let _ = transaction.rollback();
                SettingsCommitDisposition::Indeterminate
            }
        }
    }

    fn reconcile_settings_commit(
        &self,
        disposition: SettingsCommitDisposition,
        installation: &Connection,
        prior: &StoredFinalApplicationSettingsRow,
        intended: &StoredFinalApplicationSettingsRow,
        intended_view: ApplicationAiSettingsView,
    ) -> Result<ApplicationAiSettingsView, AiConnectionError> {
        if matches!(disposition, SettingsCommitDisposition::Success) {
            return Ok(intended_view);
        }
        if matches!(disposition, SettingsCommitDisposition::Indeterminate) {
            self.latch_store_indeterminate();
            return Err(AiConnectionError::StoreIndeterminate);
        }
        if !installation.is_autocommit()
            && (installation.execute_batch("ROLLBACK").is_err() || !installation.is_autocommit())
        {
            self.latch_store_indeterminate();
            return Err(AiConnectionError::StoreIndeterminate);
        }
        match load_final_settings_row(installation) {
            Ok(actual) if actual == *intended => Ok(intended_view),
            Ok(actual) if actual == *prior => Err(AiConnectionError::StoreUnavailable),
            Ok(_) | Err(_) => {
                self.latch_store_indeterminate();
                Err(AiConnectionError::StoreIndeterminate)
            }
        }
    }

    fn require_store_determinate(&self) -> Result<(), AiConnectionError> {
        if self.store_indeterminate.load(Ordering::Acquire) {
            return Err(AiConnectionError::StoreIndeterminate);
        }
        Ok(())
    }

    fn latch_store_indeterminate(&self) {
        self.store_indeterminate.store(true, Ordering::Release);
    }

    fn take_settings_commit_outcome(&self) -> u8 {
        #[cfg(feature = "runtime-fixture")]
        {
            self.next_settings_commit_outcome.swap(0, Ordering::AcqRel)
        }
        #[cfg(not(feature = "runtime-fixture"))]
        {
            0
        }
    }

    fn delete_payload(
        &self,
        payload: &mut VaultPayload,
        command: &DeleteAiConnectionCommand,
    ) -> Result<(), AiConnectionError> {
        payload
            .require_connection_cas(
                &command.connection_id,
                command.expected_execution_revision,
                command.expected_credential_generation,
            )
            .map_err(map_vault_error)?;
        let mut installation = self
            .installation
            .lock()
            .map_err(|_| AiConnectionError::StoreUnavailable)?;
        let transaction = installation
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| AiConnectionError::StoreUnavailable)?;
        let settings = load_final_settings(&transaction)?;
        if settings
            .active_ai_configuration
            .as_ref()
            .is_some_and(|active| active.connection_id.as_str() == command.connection_id)
        {
            return Err(AiConnectionError::ActiveConnection);
        }
        if (self.nonterminal_reference_check)(&command.connection_id)? {
            return Err(AiConnectionError::ReferencedByNonterminalRun);
        }
        payload
            .remove_connection(
                &command.connection_id,
                command.expected_execution_revision,
                command.expected_credential_generation,
            )
            .map_err(map_vault_error)?;
        transaction
            .commit()
            .map_err(|_| AiConnectionError::StoreUnavailable)
    }

    fn active_readiness(
        &self,
        payload: &VaultPayload,
        active: Option<&ActiveAiConfiguration>,
    ) -> ActiveAiReadiness {
        let Some(active) = active else {
            return ActiveAiReadiness::NotConfigured;
        };
        let Some(connection) = payload
            .connections()
            .find(|connection| connection.connection_id() == active.connection_id.as_str())
        else {
            return ActiveAiReadiness::StaleRevision;
        };
        let configuration = connection.configuration();
        let Ok(endpoint_fingerprint) = configuration.endpoint_fingerprint() else {
            return ActiveAiReadiness::StaleRevision;
        };
        let Ok(data_destination) = configuration.data_destination() else {
            return ActiveAiReadiness::StaleRevision;
        };
        if connection.execution_revision() != active.execution_revision.get()
            || configuration.provider() != active.provider
            || endpoint_fingerprint != active.endpoint_fingerprint
            || data_destination != active.data_destination
        {
            return ActiveAiReadiness::StaleRevision;
        }
        if !connection.enabled() {
            return ActiveAiReadiness::Disabled;
        }
        if !connection.has_credential() {
            return ActiveAiReadiness::AuthenticationRequired;
        }
        let current_adapter = self.adapter_versions.for_provider(active.provider);
        if active.adapter_version != current_adapter {
            return ActiveAiReadiness::WorkerUnavailable;
        }
        let Some(evidence) = connection.probe_evidence() else {
            return ActiveAiReadiness::CapabilityChanged;
        };
        if evidence.adapter_version != current_adapter {
            return ActiveAiReadiness::WorkerUnavailable;
        }
        let Ok(catalogue) = catalogue_sha256(evidence) else {
            return ActiveAiReadiness::CapabilityChanged;
        };
        let Some(model) = evidence
            .models
            .iter()
            .find(|model| model.model_id == active.model_id)
        else {
            return ActiveAiReadiness::CapabilityChanged;
        };
        if catalogue != active.catalogue_sha256
            || evidence.tested_model_id != active.model_id
            || evidence.tested_reasoning != active.reasoning
            || evidence.destination_class != active.destination_class
            || model.capabilities != active.capabilities
            || !reasoning_is_activatable(model, &active.reasoning)
        {
            return ActiveAiReadiness::CapabilityChanged;
        }
        ActiveAiReadiness::Ready
    }

    fn connection_view(
        &self,
        connection: &StoredAiConnection,
    ) -> Result<AiConnectionView, AiConnectionError> {
        let configuration = connection.configuration();
        let provider = configuration.provider();
        let data_destination = configuration
            .data_destination()
            .map_err(|_| AiConnectionError::VaultUnavailable)?
            .to_owned();
        let endpoint_fingerprint = configuration
            .endpoint_fingerprint()
            .map_err(|_| AiConnectionError::VaultUnavailable)?;
        let evidence = connection.probe_evidence();
        let catalogue_sha256 = evidence
            .map(catalogue_sha256)
            .transpose()
            .map_err(|_| AiConnectionError::VaultUnavailable)?;
        let status = if !connection.enabled() {
            AiConnectionStatus::Disabled
        } else if !connection.has_credential() {
            AiConnectionStatus::AuthenticationRequired
        } else if connection.probe_evidence().is_some_and(|evidence| {
            evidence.adapter_version == self.adapter_versions.for_provider(provider)
        }) {
            AiConnectionStatus::Ready
        } else {
            AiConnectionStatus::Untested
        };
        Ok(AiConnectionView {
            connection_id: connection.connection_id().to_owned(),
            execution_revision: connection.execution_revision(),
            credential_generation: connection.credential_generation(),
            method: configuration.method(),
            provider,
            display_name: connection.display_name().to_owned(),
            configuration: configuration.clone(),
            data_destination,
            endpoint_fingerprint,
            enabled: connection.enabled(),
            status,
            secret_configured: connection.has_credential(),
            models: evidence.map_or_else(Vec::new, |evidence| evidence.models.clone()),
            adapter_version: evidence.map(|evidence| evidence.adapter_version.clone()),
            catalogue_sha256,
            destination_class: evidence.map(|evidence| evidence.destination_class),
            tested_model_id: evidence.map(|evidence| evidence.tested_model_id.clone()),
            tested_reasoning: evidence.map(|evidence| evidence.tested_reasoning.clone()),
            status_summary: status_summary(status).to_owned(),
        })
    }

    fn connection_view_vault(
        &self,
        payload: &VaultPayload,
        connection_id: &str,
    ) -> Result<AiConnectionView, VaultError> {
        self.connection_view(payload.connection(connection_id)?)
            .map_err(|_| VaultError::Invalid)
    }
}

fn take_credential(
    configuration: &AiConnectionConfiguration,
    credential: &mut AiCredentialInput,
) -> Result<StoredCredential, AiConnectionError> {
    match (configuration, credential) {
        (
            AiConnectionConfiguration::AccountLogin { account_id, .. },
            AiCredentialInput::Account {
                access_token,
                refresh_token,
                expires_at,
                verified_account_id,
            },
        ) if account_id == verified_account_id => {
            take_account_credential(verified_account_id, access_token, refresh_token, expires_at)
        }
        (
            AiConnectionConfiguration::DirectProviderKey { .. },
            AiCredentialInput::ApiKey {
                api_key,
                custom_header_values,
                custom_query_values,
            },
        ) => {
            if api_key.is_empty()
                || api_key.len() > MAX_CREDENTIAL_BYTES
                || !custom_header_values.is_empty()
                || !custom_query_values.is_empty()
            {
                return Err(AiConnectionError::InvalidCommand);
            }
            StoredCredential::from_api_key(api_key.take()?).map_err(map_vault_error)
        }
        (
            AiConnectionConfiguration::OpenAiCompatible { endpoint, .. }
            | AiConnectionConfiguration::AnthropicCompatible { endpoint, .. },
            AiCredentialInput::ApiKey {
                api_key,
                custom_header_values,
                custom_query_values,
            },
        ) => take_compatible_credential(
            api_key,
            custom_header_values,
            custom_query_values,
            &endpoint.custom_header_names,
            &endpoint.custom_query_names,
        ),
        _ => Err(AiConnectionError::InvalidCommand),
    }
}

fn take_compatible_credential(
    api_key: &mut SecretInput,
    custom_header_values: &mut [SecretNameValueInput],
    custom_query_values: &mut [SecretNameValueInput],
    expected_header_names: &[String],
    expected_query_names: &[String],
) -> Result<StoredCredential, AiConnectionError> {
    if api_key.is_empty() {
        return Err(AiConnectionError::InvalidCommand);
    }
    validate_secret_fields(expected_header_names, custom_header_values, true)?;
    validate_secret_fields(expected_query_names, custom_query_values, false)?;
    let total_bytes = custom_header_values
        .iter()
        .chain(custom_query_values.iter())
        .try_fold(api_key.len(), |total, field| {
            total.checked_add(field.value.len())
        })
        .ok_or(AiConnectionError::InvalidCommand)?;
    if total_bytes > MAX_CREDENTIAL_BYTES {
        return Err(AiConnectionError::InvalidCommand);
    }

    let api_key = api_key.take()?;
    let mut header_values: Vec<(String, SecretString)> =
        Vec::with_capacity(expected_header_names.len());
    let mut query_values: Vec<(String, SecretString)> =
        Vec::with_capacity(expected_query_names.len());
    move_secret_fields(
        expected_header_names,
        custom_header_values,
        true,
        &mut header_values,
    )?;
    move_secret_fields(
        expected_query_names,
        custom_query_values,
        false,
        &mut query_values,
    )?;
    StoredCredential::from_compatible(api_key, header_values, query_values).map_err(map_vault_error)
}

fn validate_secret_fields(
    expected_names: &[String],
    fields: &[SecretNameValueInput],
    case_insensitive: bool,
) -> Result<(), AiConnectionError> {
    if expected_names.len() != fields.len()
        || fields.iter().any(|field| {
            field.value.is_empty()
                || field.value.len() > MAX_CUSTOM_VALUE_BYTES
                || !expected_names.iter().any(|expected| {
                    if case_insensitive {
                        expected.eq_ignore_ascii_case(&field.name)
                    } else {
                        expected == &field.name
                    }
                })
        })
    {
        return Err(AiConnectionError::InvalidCommand);
    }
    for (index, field) in fields.iter().enumerate() {
        if fields[..index].iter().any(|prior| {
            if case_insensitive {
                prior.name.eq_ignore_ascii_case(&field.name)
            } else {
                prior.name == field.name
            }
        }) {
            return Err(AiConnectionError::InvalidCommand);
        }
    }
    Ok(())
}

fn move_secret_fields(
    expected_names: &[String],
    fields: &mut [SecretNameValueInput],
    case_insensitive: bool,
    output: &mut Vec<(String, SecretString)>,
) -> Result<(), AiConnectionError> {
    for expected in expected_names {
        let field = fields
            .iter_mut()
            .find(|field| {
                if case_insensitive {
                    expected.eq_ignore_ascii_case(&field.name)
                } else {
                    expected == &field.name
                }
            })
            .ok_or(AiConnectionError::InvalidCommand)?;
        output.push((expected.clone(), field.value.take()?));
    }
    Ok(())
}

fn take_account_credential(
    verified_account_id: &str,
    access_token: &mut SecretInput,
    refresh_token: &mut Option<SecretInput>,
    expires_at: &mut String,
) -> Result<StoredCredential, AiConnectionError> {
    let total_bytes = verified_account_id
        .len()
        .checked_add(access_token.len())
        .and_then(|length| length.checked_add(refresh_token.as_ref().map_or(0, SecretInput::len)))
        .and_then(|length| length.checked_add(expires_at.len()))
        .ok_or(AiConnectionError::InvalidCommand)?;
    if verified_account_id.is_empty()
        || verified_account_id.len() > MAX_ACCOUNT_ID_BYTES
        || access_token.is_empty()
        || expires_at.is_empty()
        || expires_at.len() > MAX_PROBE_METADATA_BYTES
        || total_bytes > MAX_CREDENTIAL_BYTES
        || refresh_token.as_ref().is_none_or(SecretInput::is_empty)
    {
        return Err(AiConnectionError::InvalidCommand);
    }
    let refresh_token = refresh_token
        .as_mut()
        .ok_or(AiConnectionError::InvalidCommand)?
        .take()?;
    StoredCredential::from_account(
        access_token.take()?,
        refresh_token,
        SecretString::new(std::mem::take(expires_at)),
        SecretString::new(verified_account_id.to_owned()),
    )
    .map_err(map_vault_error)
}

fn validate_probe_evidence(
    evidence: AiProbeEvidence,
) -> Result<AiProbeEvidence, AiConnectionError> {
    if evidence.models.is_empty()
        || evidence.models.len() > MAX_PROBE_MODELS
        || evidence.endpoint_fingerprint.is_empty()
        || evidence.endpoint_fingerprint.len() > MAX_PROBE_METADATA_BYTES
        || evidence.adapter_version.is_empty()
        || evidence.adapter_version.len() > MAX_PROBE_METADATA_BYTES
        || evidence.observed_at.is_empty()
        || evidence.observed_at.len() > MAX_PROBE_METADATA_BYTES
        || evidence.models.iter().any(|model| {
            model.model_id.is_empty()
                || model.model_id.len() > MAX_MODEL_ID_BYTES
                || model.reported_model_id.as_ref().is_some_and(|reported| {
                    reported.is_empty() || reported.len() > MAX_MODEL_ID_BYTES
                })
                || model.display_name.is_empty()
                || model.display_name.len() > MAX_PROBE_LABEL_BYTES
                || model.reasoning_options.len() > MAX_REASONING_OPTIONS
                || model.reasoning_options.iter().any(|option| {
                    option.label.is_empty()
                        || option.label.len() > MAX_PROBE_LABEL_BYTES
                        || option.description.is_empty()
                        || option.description.len() > MAX_PROBE_DESCRIPTION_BYTES
                        || matches!(
                            &option.selection,
                            AiReasoningSelection::Effort { id }
                                if id.is_empty() || id.len() > MAX_MODEL_ID_BYTES
                        )
                })
        })
    {
        return Err(AiConnectionError::InvalidCommand);
    }
    let bytes = serde_json::to_vec(&evidence).map_err(|_| AiConnectionError::InvalidCommand)?;
    let evidence: AiProbeEvidence =
        serde_json::from_slice(&bytes).map_err(|_| AiConnectionError::InvalidCommand)?;
    if catalogue_sha256(&evidence).is_err() {
        return Err(AiConnectionError::InvalidCommand);
    }
    Ok(evidence)
}

fn load_final_settings(
    connection: &Connection,
) -> Result<StoredFinalApplicationSettings, AiConnectionError> {
    Ok(load_final_settings_row(connection)?.settings)
}

fn load_final_settings_row(
    connection: &Connection,
) -> Result<StoredFinalApplicationSettingsRow, AiConnectionError> {
    let (settings_json, updated_at) = connection
        .query_row(
            "SELECT settings_json, updated_at
             FROM application_settings WHERE singleton = 1",
            [],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(|_| AiConnectionError::StoreUnavailable)?
        .ok_or(AiConnectionError::StoreUnavailable)?;
    let settings =
        serde_json::from_str(&settings_json).map_err(|_| AiConnectionError::StoreUnavailable)?;
    Ok(StoredFinalApplicationSettingsRow {
        settings,
        settings_json,
        updated_at,
    })
}

fn final_settings_row(
    settings: StoredFinalApplicationSettings,
    updated_at: String,
) -> Result<StoredFinalApplicationSettingsRow, AiConnectionError> {
    let settings_json =
        serde_json::to_string(&settings).map_err(|_| AiConnectionError::StoreUnavailable)?;
    Ok(StoredFinalApplicationSettingsRow {
        settings,
        settings_json,
        updated_at,
    })
}

fn store_final_settings_row(
    transaction: &Transaction<'_>,
    row: &StoredFinalApplicationSettingsRow,
) -> Result<(), AiConnectionError> {
    let changed = transaction
        .execute(
            "UPDATE application_settings
             SET settings_json = ?1, updated_at = ?2
             WHERE singleton = 1",
            (&row.settings_json, &row.updated_at),
        )
        .map_err(|_| AiConnectionError::StoreUnavailable)?;
    if changed != 1 {
        return Err(AiConnectionError::StoreUnavailable);
    }
    Ok(())
}

fn reasoning_is_activatable(model: &AiModelView, selection: &AiReasoningSelection) -> bool {
    if model
        .reported_model_id
        .as_ref()
        .is_some_and(|reported| reported != &model.model_id)
    {
        return false;
    }
    match model.capabilities.reasoning {
        CapabilitySupport::Unsupported => {
            matches!(selection, AiReasoningSelection::Unsupported)
                && model.reasoning_options.is_empty()
        }
        CapabilitySupport::Supported => model
            .reasoning_options
            .iter()
            .any(|option| &option.selection == selection),
        CapabilitySupport::Unknown => false,
    }
}

fn active_configuration_view(active: &ActiveAiConfiguration) -> ActiveAiConfigurationView {
    ActiveAiConfigurationView {
        connection_id: active.connection_id.as_str().to_owned(),
        execution_revision: active.execution_revision.get(),
        provider: active.provider,
        endpoint_fingerprint: active.endpoint_fingerprint.clone(),
        model_id: active.model_id.clone(),
        reasoning: active.reasoning.clone(),
        adapter_version: active.adapter_version.clone(),
        catalogue_sha256: active.catalogue_sha256.clone(),
        destination_class: active.destination_class,
        capabilities: active.capabilities.clone(),
        data_destination: active.data_destination.clone(),
        activated_at: active.activated_at.clone(),
    }
}

fn status_summary(status: AiConnectionStatus) -> &'static str {
    match status {
        AiConnectionStatus::Untested => "Not tested.",
        AiConnectionStatus::Testing => "Testing.",
        AiConnectionStatus::Ready => "Tested and ready.",
        AiConnectionStatus::Disabled => "Disabled.",
        AiConnectionStatus::AuthenticationRequired => "Authentication required.",
        AiConnectionStatus::TemporarilyUnavailable => "Temporarily unavailable.",
        AiConnectionStatus::Incompatible => "Adapter incompatible.",
    }
}

fn new_connection_id() -> Result<String, AiConnectionError> {
    let mut random = [0u8; 16];
    getrandom::fill(&mut random).map_err(|_| AiConnectionError::VaultUnavailable)?;
    Ok(lower_hex_id(&random))
}

fn lower_hex_id(bytes: &[u8; 16]) -> String {
    let mut hex = String::with_capacity(32);
    for byte in bytes {
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

fn map_vault_error(error: VaultError) -> AiConnectionError {
    match error {
        VaultError::Invalid => AiConnectionError::InvalidCommand,
        VaultError::RevisionConflict => AiConnectionError::Conflict,
        VaultError::RevisionOverflow => AiConnectionError::RevisionOverflow,
        VaultError::NotFound => AiConnectionError::NotFound,
        VaultError::Unavailable | VaultError::Corrupt | VaultError::Unsupported => {
            AiConnectionError::VaultUnavailable
        }
    }
}

fn validate_connection_cas(
    connection_id: &str,
    execution_revision: u64,
    credential_generation: u64,
) -> Result<(), AiConnectionError> {
    AiConnectionId::parse(connection_id.to_owned())
        .map_err(|_| AiConnectionError::InvalidCommand)?;
    AiConnectionRevision::new(execution_revision).map_err(|_| AiConnectionError::InvalidCommand)?;
    CredentialGeneration::new(credential_generation)
        .map_err(|_| AiConnectionError::InvalidCommand)?;
    Ok(())
}

fn is_lower_sha256(value: &str) -> bool {
    value.len() == SHA256_HEX_BYTES
        && value
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

fn reasoning_selection_is_bounded(selection: &AiReasoningSelection) -> bool {
    match selection {
        AiReasoningSelection::Unsupported => true,
        AiReasoningSelection::Effort { id } => !id.is_empty() && id.len() <= MAX_MODEL_ID_BYTES,
    }
}

fn connection_gate() -> &'static Mutex<()> {
    static CONNECTION_GATE: OnceLock<Mutex<()>> = OnceLock::new();
    CONNECTION_GATE.get_or_init(|| Mutex::new(()))
}

#[cfg(feature = "runtime-fixture")]
pub fn fixture_reset_secret_drop_observations() {
    super::vault::reset_secret_drop_observations();
}

#[cfg(feature = "runtime-fixture")]
pub fn fixture_secret_drop_observations() -> usize {
    super::vault::secret_drop_observations()
}
