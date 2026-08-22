use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use garde::Validate;
use jiff::Timestamp;
use rusqlite::{params, Connection, OpenFlags, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ts_rs::TS;

use crate::{
    agent_runtime::{ProviderFailure, ProviderFailureCategory},
    chatgpt_oauth::{
        load, needs_refresh, refresh_connection_unlocked, with_connection_mutation, LoadState,
        StoredConnection, TokenClient,
    },
    setup::INSTALLATION_SCHEMA_VERSION,
    tender_store::{TenderCommandError, TenderErrorCode, TenderId, TENDER_SCHEMA_VERSION},
    QuantixHost,
};

pub(crate) const CODEX_CONNECTION_ID: &str = "codex_chatgpt";
const CHATGPT_DIRECT_ADAPTER_VERSION: &str = "chatgpt-direct-v1";
const CHATGPT_DIRECT_CATALOGUE_VERSION: &str = "chatgpt-direct-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum AiProviderKind {
    Codex,
}

impl AiProviderKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Codex => "codex",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ProviderConnectionStatus {
    Ready,
    AuthenticationRequired,
    SubscriptionRequired,
    TemporarilyUnavailable,
    Incompatible,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
#[ts(export)]
pub enum ProviderReasoningSelection {
    ProviderDefault,
    CodexEffort(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct ProviderReasoningOption {
    pub selection: ProviderReasoningSelection,
    pub label: String,
    pub description: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct ProviderModelOption {
    pub model_id: String,
    pub display_name: String,
    pub description: String,
    pub is_default: bool,
    pub input_modalities: Vec<String>,
    pub reasoning_options: Vec<ProviderReasoningOption>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct ProviderConnectionView {
    pub connection_id: String,
    pub provider: AiProviderKind,
    pub display_name: String,
    pub status: ProviderConnectionStatus,
    pub account_label: Option<String>,
    pub account_plan: Option<String>,
    pub models: Vec<ProviderModelOption>,
    pub catalogue_fetched_at: Option<String>,
    pub adapter_version: String,
    pub status_summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct AiExecutionSelection {
    pub connection_id: String,
    pub provider: AiProviderKind,
    pub model_id: String,
    pub reasoning: ProviderReasoningSelection,
    pub catalogue_fetched_at: String,
    pub adapter_version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum TenderAiSelectionReadiness {
    LocalOnly,
    Ready,
    SelectionRequired,
    ProviderUnavailable,
    CatalogueStale,
    ModelUnavailable,
    ApprovalRequired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct TenderAiExecutionBinding {
    pub revision: u64,
    pub selection: Option<AiExecutionSelection>,
    pub readiness: TenderAiSelectionReadiness,
    pub status_summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct AiExecutionApproval {
    pub connection_id: String,
    pub provider: AiProviderKind,
    pub account_fingerprint: String,
    pub model_id: String,
    pub reasoning: ProviderReasoningSelection,
    pub data_destination: String,
    pub approved_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum AppearancePreference {
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct GeneralApplicationPreferences {
    pub appearance: AppearancePreference,
    pub reduced_motion: bool,
    pub larger_text: bool,
    pub notify_when_attention_needed: bool,
}

impl Default for GeneralApplicationPreferences {
    fn default() -> Self {
        Self {
            appearance: AppearancePreference::System,
            reduced_motion: false,
            larger_text: false,
            notify_when_attention_needed: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct ApplicationStorageFacts {
    pub application_home: String,
    pub tender_backups_are_preserved: bool,
    pub trash_requires_explicit_purge: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct ApplicationDiagnostics {
    pub quantix_version: String,
    pub installation_schema_version: i64,
    pub tender_schema_version: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct ApplicationSettingsView {
    pub general_preferences: GeneralApplicationPreferences,
    pub ai_execution_selection: Option<AiExecutionSelection>,
    pub ai_execution_approval: Option<AiExecutionApproval>,
    pub provider_connections: Vec<ProviderConnectionView>,
    pub chatgpt: crate::chatgpt_login::ChatGptConnectionStatus,
    pub storage: ApplicationStorageFacts,
    pub diagnostics: ApplicationDiagnostics,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct UpdateGeneralApplicationPreferencesCommand {
    #[garde(skip)]
    pub preferences: GeneralApplicationPreferences,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct UpdateAiExecutionSelectionCommand {
    #[garde(length(bytes, min = 1, max = 100))]
    pub connection_id: String,
    #[garde(length(bytes, min = 1, max = 200))]
    pub model_id: String,
    #[garde(skip)]
    pub reasoning: ProviderReasoningSelection,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct ConfirmAiExecutionSelectionCommand {
    #[garde(length(bytes, min = 1, max = 100))]
    pub connection_id: String,
    #[garde(length(bytes, min = 1, max = 200))]
    pub model_id: String,
    #[garde(skip)]
    pub reasoning: ProviderReasoningSelection,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct InspectTenderAiExecutionCommand {
    #[garde(length(bytes, min = 1, max = 64))]
    pub tender_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct UpdateTenderAiExecutionSelectionCommand {
    #[garde(length(bytes, min = 1, max = 64))]
    pub tender_id: String,
    #[garde(range(min = 1))]
    pub expected_revision: u64,
    #[garde(skip)]
    pub selection: Option<AiExecutionSelection>,
}

impl From<ConfirmAiExecutionSelectionCommand> for UpdateAiExecutionSelectionCommand {
    fn from(command: ConfirmAiExecutionSelectionCommand) -> Self {
        Self {
            connection_id: command.connection_id,
            model_id: command.model_id,
            reasoning: command.reasoning,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredApplicationSettings {
    general_preferences: GeneralApplicationPreferences,
    ai_execution_selection: Option<AiExecutionSelection>,
    #[serde(default)]
    ai_execution_approval: Option<AiExecutionApproval>,
}

impl QuantixHost {
    pub fn inspect_application_settings(
        &self,
    ) -> Result<ApplicationSettingsView, TenderCommandError> {
        self.application_settings_with_chatgpt_phase()
    }

    pub async fn update_general_application_preferences(
        &self,
        command: UpdateGeneralApplicationPreferencesCommand,
    ) -> Result<ApplicationSettingsView, TenderCommandError> {
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        {
            let mut database = settings_connection(self.application_home())?;
            let transaction = database
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(settings_store_error)?;
            let mut stored = load_stored_settings(&transaction)?;
            stored.general_preferences = command.preferences;
            store_application_settings(&transaction, &stored)?;
            transaction.commit().map_err(settings_store_error)?;
        }
        self.application_settings_with_chatgpt_phase()
    }

    pub async fn refresh_application_settings(
        &self,
    ) -> Result<ApplicationSettingsView, TenderCommandError> {
        let application_home = self.application_home().to_path_buf();
        tokio::task::spawn_blocking(move || {
            refresh_application_settings_projection(
                &application_home,
                crate::chatgpt_login::PRODUCTION_ISSUER,
                current_time_ms(),
            )
        })
        .await
        .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))??;
        self.application_settings_with_chatgpt_phase()
    }

    pub(crate) async fn refresh_exact_ai_execution_selection(
        &self,
        preferred: Option<&AiExecutionSelection>,
    ) -> Result<Option<AiExecutionSelection>, TenderCommandError> {
        let application_home = self.application_home().to_path_buf();
        let preferred = preferred.cloned();
        tokio::task::spawn_blocking(move || {
            refresh_exact_ai_execution_selection_projection(&application_home, preferred.as_ref())
        })
        .await
        .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
    }

    pub(crate) async fn require_current_tender_ai_selection(
        &self,
        tender_id: &TenderId,
    ) -> Result<AiExecutionSelection, TenderCommandError> {
        let store = self.tender_store(tender_id)?;
        let binding = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .inspect_tender_ai_execution_binding()?;
        let selection = binding
            .selection
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::AiProviderRequired))?;
        self.refresh_exact_ai_execution_selection(Some(&selection))
            .await?
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::AiProviderRequired))
    }

    pub fn inspect_tender_ai_execution(
        &self,
        command: InspectTenderAiExecutionCommand,
    ) -> Result<TenderAiExecutionBinding, TenderCommandError> {
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let store = self.tender_store(&tender_id)?;
        let binding = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .inspect_tender_ai_execution_binding()?;
        Ok(binding)
    }

    pub fn update_tender_ai_execution(
        &self,
        command: UpdateTenderAiExecutionSelectionCommand,
    ) -> Result<TenderAiExecutionBinding, TenderCommandError> {
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let next_revision = command
            .expected_revision
            .checked_add(1)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let binding = assess_tender_ai_execution_binding(
            self.application_home(),
            command.selection.clone(),
            next_revision,
        )?;
        let store = self.tender_store(&tender_id)?;
        let updated = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .update_tender_ai_execution_binding(
                &tender_id,
                command.expected_revision,
                binding.selection,
                binding.readiness,
                &binding.status_summary,
            )?;
        Ok(updated)
    }

    pub async fn update_ai_execution_selection(
        &self,
        command: UpdateAiExecutionSelectionCommand,
    ) -> Result<ApplicationSettingsView, TenderCommandError> {
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        if command.connection_id != CODEX_CONNECTION_ID {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let application_home = self.application_home().to_path_buf();
        tokio::task::spawn_blocking(move || {
            update_ai_execution_selection_projection(&application_home, &command)
        })
        .await
        .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
    }

    pub async fn confirm_ai_execution_selection(
        &self,
        command: ConfirmAiExecutionSelectionCommand,
    ) -> Result<ApplicationSettingsView, TenderCommandError> {
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        if command.connection_id != CODEX_CONNECTION_ID {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let application_home = self.application_home().to_path_buf();
        let update_command = command.into();
        let view = tokio::task::spawn_blocking(move || {
            confirm_ai_execution_selection_projection(&application_home, &update_command)
        })
        .await
        .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))??;
        if let Some(selection) = view.ai_execution_selection.clone() {
            self.refresh_matching_tender_ai_bindings(&view, &selection)?;
        }
        load_application_settings(self.application_home())
    }

    fn refresh_matching_tender_ai_bindings(
        &self,
        view: &ApplicationSettingsView,
        selection: &AiExecutionSelection,
    ) -> Result<(), TenderCommandError> {
        let tenders_root = self.application_home().join("tenders");
        let Ok(entries) = fs::read_dir(tenders_root) else {
            return Ok(());
        };
        for entry in entries {
            let entry =
                entry.map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
            let Some(tender_id) = entry
                .file_name()
                .to_str()
                .and_then(|value| TenderId::parse(value).ok())
            else {
                continue;
            };
            let store = match self.tender_store(&tender_id) {
                Ok(store) => store,
                Err(_) => continue,
            };
            let mut store = store
                .lock()
                .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
            let current = store.inspect_tender_ai_execution_binding()?;
            if current.selection.as_ref() != Some(selection) {
                continue;
            }
            let refreshed = tender_ai_execution_binding_from_view(view, Some(selection.clone()));
            store.update_tender_ai_execution_binding(
                &tender_id,
                current.revision,
                refreshed.selection,
                refreshed.readiness,
                &refreshed.status_summary,
            )?;
        }
        Ok(())
    }

    pub fn clear_ai_execution_selection(
        &self,
    ) -> Result<ApplicationSettingsView, TenderCommandError> {
        let mut database = settings_connection(self.application_home())?;
        let transaction = database
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(settings_store_error)?;
        let mut stored = load_stored_settings(&transaction)?;
        stored.ai_execution_selection = None;
        stored.ai_execution_approval = None;
        store_application_settings(&transaction, &stored)?;
        transaction.commit().map_err(settings_store_error)?;
        load_application_settings(self.application_home())
    }

    fn application_settings_with_chatgpt_phase(
        &self,
    ) -> Result<ApplicationSettingsView, TenderCommandError> {
        let mut view = load_application_settings(self.application_home())?;
        view.chatgpt = crate::chatgpt_login::chatgpt_connection_status_with_phase(
            self.application_home(),
            self.chatgpt_login_phase(),
        );
        Ok(view)
    }
}

#[cfg(feature = "runtime-fixture")]
pub(crate) fn codex_failure_connection_status(
    category: ProviderFailureCategory,
) -> (ProviderConnectionStatus, &'static str) {
    match category {
        ProviderFailureCategory::AuthenticationRequired => (
            ProviderConnectionStatus::AuthenticationRequired,
            "Connect an OpenAI account to use Codex intelligence.",
        ),
        ProviderFailureCategory::SubscriptionRequired => (
            ProviderConnectionStatus::SubscriptionRequired,
            "The connected OpenAI account does not provide an eligible Codex subscription.",
        ),
        ProviderFailureCategory::ProtocolInvalid => (
            ProviderConnectionStatus::Incompatible,
            "The installed Codex runtime is incompatible with Quantix.",
        ),
        _ => (
            ProviderConnectionStatus::TemporarilyUnavailable,
            "Codex is temporarily unavailable. Tender records remain accessible.",
        ),
    }
}

pub(crate) fn chatgpt_authentication_failure() -> ProviderFailure {
    ProviderFailure::new(
        ProviderFailureCategory::AuthenticationRequired,
        true,
        "Connect your ChatGPT subscription in Settings before retrying.",
        Some("No usable ChatGPT connection is stored."),
    )
}

pub(crate) fn chatgpt_subscription_failure() -> ProviderFailure {
    ProviderFailure::new(
        ProviderFailureCategory::SubscriptionRequired,
        true,
        "Connect an eligible ChatGPT subscription in Settings before retrying.",
        Some("The connected ChatGPT account is not an eligible subscription."),
    )
}

/// Connection readiness for the Quantix-owned ChatGPT provider: a usable
/// stored token connection whose `plan_type` claim names an eligible ChatGPT
/// plan. An expired connection is refreshed through the issuer once; a failed
/// refresh surfaces as `AuthenticationRequired`.
pub(crate) fn chatgpt_connection_readiness(
    application_home: &Path,
    issuer: &str,
    now_ms: u64,
) -> Result<StoredConnection, ProviderFailure> {
    with_connection_mutation(|| {
        chatgpt_connection_readiness_unlocked(application_home, issuer, now_ms)
    })
}

fn chatgpt_connection_readiness_unlocked(
    application_home: &Path,
    issuer: &str,
    now_ms: u64,
) -> Result<StoredConnection, ProviderFailure> {
    let stored = match load(application_home) {
        LoadState::Connected(connection) => *connection,
        LoadState::Absent | LoadState::Unusable => return Err(chatgpt_authentication_failure()),
    };
    let connection = if needs_refresh(&stored, now_ms) {
        let client = TokenClient::new(issuer).map_err(|_| chatgpt_authentication_failure())?;
        refresh_connection_unlocked(application_home, &client, now_ms)
            .map_err(|_| chatgpt_authentication_failure())?
    } else {
        stored
    };
    if !chatgpt_subscription_is_supported(connection.plan_type.as_deref()) {
        return Err(chatgpt_subscription_failure());
    }
    Ok(connection)
}

fn refresh_application_settings_projection(
    application_home: &Path,
    issuer: &str,
    now_ms: u64,
) -> Result<(), TenderCommandError> {
    project_chatgpt_connection_readiness_with(application_home, || {
        chatgpt_connection_readiness_unlocked(application_home, issuer, now_ms)
    })
    .map(|_| ())
}

pub(crate) fn project_chatgpt_connection_readiness(
    application_home: &Path,
    issuer: &str,
    now_ms: u64,
) -> Result<StoredConnection, ProviderFailure> {
    project_chatgpt_connection_readiness_with(application_home, || {
        chatgpt_connection_readiness_unlocked(application_home, issuer, now_ms)
    })
    .map_err(|_| connection_task_error())?
}

fn project_chatgpt_connection_readiness_with(
    application_home: &Path,
    inspect: impl FnOnce() -> Result<StoredConnection, ProviderFailure>,
) -> Result<Result<StoredConnection, ProviderFailure>, TenderCommandError> {
    with_connection_mutation(|| {
        project_chatgpt_connection_readiness_unlocked_with(application_home, inspect)
    })
}

fn project_chatgpt_connection_readiness_unlocked_with(
    application_home: &Path,
    inspect: impl FnOnce() -> Result<StoredConnection, ProviderFailure>,
) -> Result<Result<StoredConnection, ProviderFailure>, TenderCommandError> {
    let readiness = inspect();
    let projection = match readiness.as_ref() {
        Ok(connection) => chatgpt_connection_view(connection),
        Err(failure) => chatgpt_connection_failure_view(failure.category),
    };
    save_live_connection_unlocked(application_home, &projection)?;
    Ok(readiness)
}

fn refresh_exact_ai_execution_selection_projection(
    application_home: &Path,
    preferred: Option<&AiExecutionSelection>,
) -> Result<Option<AiExecutionSelection>, TenderCommandError> {
    with_connection_mutation(|| {
        let readiness =
            project_chatgpt_connection_readiness_unlocked_with(application_home, || {
                chatgpt_connection_readiness_unlocked(
                    application_home,
                    crate::chatgpt_login::PRODUCTION_ISSUER,
                    current_time_ms(),
                )
            })?;
        let Ok(connection) = readiness else {
            return Ok(None);
        };
        exact_approved_selection_unlocked(
            application_home,
            &chatgpt_connection_view(&connection),
            preferred,
        )
    })
}

pub(crate) fn project_approved_chatgpt_connection(
    application_home: &Path,
    issuer: &str,
    now_ms: u64,
    selection: &AiExecutionSelection,
) -> Result<StoredConnection, ProviderFailure> {
    project_approved_chatgpt_connection_with(application_home, selection, || {
        chatgpt_connection_readiness_unlocked(application_home, issuer, now_ms)
    })
}

/// Refreshes a provider turn after one authentication rejection without
/// allowing a replacement account or revoked approval to inherit that retry.
/// The account observation, refresh, settings projection, and exact approval
/// check share the connection-mutation boundary.
pub(crate) fn refresh_approved_chatgpt_connection(
    application_home: &Path,
    token_client: &TokenClient,
    now_ms: u64,
    expected_account_id: &str,
    selection: &AiExecutionSelection,
) -> Result<StoredConnection, ProviderFailure> {
    with_connection_mutation(|| {
        let current = match load(application_home) {
            LoadState::Connected(connection) => *connection,
            LoadState::Absent | LoadState::Unusable => return Err(chatgpt_authentication_failure()),
        };
        if current.account_id != expected_account_id {
            return Err(chatgpt_authentication_failure());
        }

        let readiness =
            project_chatgpt_connection_readiness_unlocked_with(application_home, || {
                let connection =
                    refresh_connection_unlocked(application_home, token_client, now_ms)
                        .map_err(|_| chatgpt_authentication_failure())?;
                if !chatgpt_subscription_is_supported(connection.plan_type.as_deref()) {
                    return Err(chatgpt_subscription_failure());
                }
                Ok(connection)
            })
            .map_err(|_| connection_task_error())?;
        let connection = readiness?;
        if connection.account_id != expected_account_id {
            return Err(chatgpt_authentication_failure());
        }
        let approved = exact_approved_selection_unlocked(
            application_home,
            &chatgpt_connection_view(&connection),
            Some(selection),
        )
        .map_err(|_| connection_task_error())?;
        if approved.as_ref() != Some(selection) {
            return Err(chatgpt_authentication_failure());
        }
        Ok(connection)
    })
}

fn project_approved_chatgpt_connection_with(
    application_home: &Path,
    selection: &AiExecutionSelection,
    inspect: impl FnOnce() -> Result<StoredConnection, ProviderFailure>,
) -> Result<StoredConnection, ProviderFailure> {
    with_connection_mutation(|| {
        let readiness =
            project_chatgpt_connection_readiness_unlocked_with(application_home, inspect)
                .map_err(|_| connection_task_error())?;
        let connection = readiness?;
        let approved = exact_approved_selection_unlocked(
            application_home,
            &chatgpt_connection_view(&connection),
            Some(selection),
        )
        .map_err(|_| connection_task_error())?;
        if approved.as_ref() != Some(selection) {
            return Err(chatgpt_authentication_failure());
        }
        Ok(connection)
    })
}

fn exact_approved_selection_unlocked(
    application_home: &Path,
    connection: &ProviderConnectionView,
    preferred: Option<&AiExecutionSelection>,
) -> Result<Option<AiExecutionSelection>, TenderCommandError> {
    let database = settings_connection(application_home)?;
    let settings = load_stored_settings(&database)?;
    let Some(preferred) = preferred.or(settings.ai_execution_selection.as_ref()) else {
        return Ok(None);
    };
    if settings.ai_execution_selection.as_ref() != Some(preferred)
        || !selection_is_supported(connection, preferred)
        || !settings
            .ai_execution_approval
            .as_ref()
            .is_some_and(|approval| approval_matches(connection, preferred, approval))
    {
        return Ok(None);
    }
    Ok(Some(preferred.clone()))
}

fn connection_task_error() -> ProviderFailure {
    ProviderFailure::new(
        ProviderFailureCategory::ProcessFailed,
        true,
        "ChatGPT connection verification could not complete.",
        Some("The local connection task stopped unexpectedly."),
    )
}

fn chatgpt_subscription_is_supported(plan_type: Option<&str>) -> bool {
    matches!(
        plan_type,
        Some(
            "go" | "plus"
                | "pro"
                | "prolite"
                | "team"
                | "self_serve_business_prolite"
                | "self_serve_business_usage_based"
                | "business"
                | "ent26"
                | "enterprise_cbp_automation"
                | "enterprise_cbp_usage_based"
                | "enterprise"
                | "edu"
        )
    )
}

pub(crate) fn chatgpt_connection_view(connection: &StoredConnection) -> ProviderConnectionView {
    ProviderConnectionView {
        connection_id: CODEX_CONNECTION_ID.to_owned(),
        provider: AiProviderKind::Codex,
        display_name: "ChatGPT subscription".to_owned(),
        status: ProviderConnectionStatus::Ready,
        account_label: Some(connection.account_id.clone()),
        account_plan: connection.plan_type.clone(),
        models: chatgpt_direct_models(),
        catalogue_fetched_at: Some(CHATGPT_DIRECT_CATALOGUE_VERSION.to_owned()),
        adapter_version: CHATGPT_DIRECT_ADAPTER_VERSION.to_owned(),
        status_summary: "Ready through the Quantix-owned ChatGPT connection.".to_owned(),
    }
}

fn chatgpt_connection_failure_view(category: ProviderFailureCategory) -> ProviderConnectionView {
    let (status, status_summary) = match category {
        ProviderFailureCategory::SubscriptionRequired => (
            ProviderConnectionStatus::SubscriptionRequired,
            "The connected ChatGPT account does not provide an eligible subscription.",
        ),
        ProviderFailureCategory::AuthenticationRequired => (
            ProviderConnectionStatus::AuthenticationRequired,
            "Connect your ChatGPT subscription in Settings before retrying.",
        ),
        _ => (
            ProviderConnectionStatus::TemporarilyUnavailable,
            "ChatGPT is temporarily unavailable. Tender records remain accessible.",
        ),
    };
    ProviderConnectionView {
        connection_id: CODEX_CONNECTION_ID.to_owned(),
        provider: AiProviderKind::Codex,
        display_name: "ChatGPT subscription".to_owned(),
        status,
        account_label: None,
        account_plan: None,
        models: Vec::new(),
        catalogue_fetched_at: None,
        adapter_version: CHATGPT_DIRECT_ADAPTER_VERSION.to_owned(),
        status_summary: status_summary.to_owned(),
    }
}

pub(crate) fn save_chatgpt_disconnected_unlocked(
    application_home: &Path,
) -> Result<(), TenderCommandError> {
    let mut database = settings_connection(application_home)?;
    let transaction = database
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(settings_store_error)?;
    upsert_connection(
        &transaction,
        &chatgpt_connection_failure_view(ProviderFailureCategory::AuthenticationRequired),
    )?;
    let mut stored = load_stored_settings(&transaction)?;
    if stored
        .ai_execution_selection
        .as_ref()
        .is_some_and(|selection| selection.connection_id == CODEX_CONNECTION_ID)
    {
        stored.ai_execution_selection = None;
    }
    if stored
        .ai_execution_approval
        .as_ref()
        .is_some_and(|approval| approval.connection_id == CODEX_CONNECTION_ID)
    {
        stored.ai_execution_approval = None;
    }
    store_application_settings(&transaction, &stored)?;
    transaction.commit().map_err(settings_store_error)
}

fn chatgpt_direct_models() -> Vec<ProviderModelOption> {
    [
        ("gpt-5.5", "GPT-5.5", true),
        ("gpt-5.4", "GPT-5.4", false),
        ("gpt-5.4-mini", "GPT-5.4 mini", false),
        ("gpt-5.3-codex-spark", "GPT-5.3 Codex Spark", false),
    ]
    .into_iter()
    .map(|(model_id, display_name, is_default)| ProviderModelOption {
        model_id: model_id.to_owned(),
        display_name: display_name.to_owned(),
        description: "ChatGPT direct Responses API model.".to_owned(),
        is_default,
        input_modalities: vec!["text".to_owned()],
        reasoning_options: ["none", "low", "medium", "high", "xhigh"]
            .into_iter()
            .map(|effort| ProviderReasoningOption {
                selection: ProviderReasoningSelection::CodexEffort(effort.to_owned()),
                label: effort.to_owned(),
                description: format!("{effort} reasoning effort"),
                is_default: effort == "medium",
            })
            .collect(),
    })
    .collect()
}

fn current_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

/// Returns the application-wide selection that should seed a new Tender.
/// A missing selection is intentional and represents a local-only Tender
/// until the Engineer chooses and approves an AI destination.
pub(crate) fn default_tender_ai_execution_binding(
    application_home: &Path,
) -> Result<TenderAiExecutionBinding, TenderCommandError> {
    let view = load_application_settings(application_home)?;
    let selection = view.ai_execution_selection.clone();
    Ok(tender_ai_execution_binding_from_view(&view, selection))
}

/// Re-evaluates an exact Tender selection against the currently persisted
/// provider catalogue and Engineer approval. This is deliberately a local
/// fact check; live provider refresh remains the Host's explicit action.
pub(crate) fn assess_tender_ai_execution_binding(
    application_home: &Path,
    selection: Option<AiExecutionSelection>,
    revision: u64,
) -> Result<TenderAiExecutionBinding, TenderCommandError> {
    let view = load_application_settings(application_home)?;
    Ok(tender_ai_execution_binding_from_view(&view, selection).with_revision(revision))
}

impl TenderAiExecutionBinding {
    pub(crate) fn with_revision(mut self, revision: u64) -> Self {
        self.revision = revision;
        self
    }
}

pub(crate) fn tender_ai_execution_binding_from_view(
    view: &ApplicationSettingsView,
    selection: Option<AiExecutionSelection>,
) -> TenderAiExecutionBinding {
    let Some(selection) = selection else {
        return TenderAiExecutionBinding {
            revision: 1,
            selection: None,
            readiness: TenderAiSelectionReadiness::LocalOnly,
            status_summary: "No AI provider is selected; local-only Tender work remains available."
                .to_owned(),
        };
    };

    let Some(connection) = view
        .provider_connections
        .iter()
        .find(|connection| connection.connection_id == selection.connection_id)
    else {
        return TenderAiExecutionBinding {
            revision: 1,
            selection: Some(selection),
            readiness: TenderAiSelectionReadiness::ProviderUnavailable,
            status_summary: "The selected AI provider connection is unavailable.".to_owned(),
        };
    };
    if connection.status != ProviderConnectionStatus::Ready {
        return TenderAiExecutionBinding {
            revision: 1,
            selection: Some(selection),
            readiness: TenderAiSelectionReadiness::ProviderUnavailable,
            status_summary: connection.status_summary.clone(),
        };
    }
    if connection.catalogue_fetched_at.as_deref() != Some(selection.catalogue_fetched_at.as_str())
        || connection.adapter_version != selection.adapter_version
    {
        return TenderAiExecutionBinding {
            revision: 1,
            selection: Some(selection),
            readiness: TenderAiSelectionReadiness::CatalogueStale,
            status_summary: "The selected AI capability catalogue is stale; refresh it before running Tender work."
                .to_owned(),
        };
    }
    if !selection_is_supported(connection, &selection) {
        return TenderAiExecutionBinding {
            revision: 1,
            selection: Some(selection),
            readiness: TenderAiSelectionReadiness::ModelUnavailable,
            status_summary: "The selected model or reasoning capability is no longer available."
                .to_owned(),
        };
    }
    let approved = view
        .ai_execution_approval
        .as_ref()
        .is_some_and(|approval| approval_matches(connection, &selection, approval));
    if !approved {
        return TenderAiExecutionBinding {
            revision: 1,
            selection: Some(selection),
            readiness: TenderAiSelectionReadiness::ApprovalRequired,
            status_summary:
                "Confirm the selected provider, model, and reasoning before AI work starts."
                    .to_owned(),
        };
    }
    TenderAiExecutionBinding {
        revision: 1,
        selection: Some(selection),
        readiness: TenderAiSelectionReadiness::Ready,
        status_summary: "The selected AI provider, model, and reasoning capability are ready."
            .to_owned(),
    }
}

fn selection_from_command(
    connection: &ProviderConnectionView,
    command: &UpdateAiExecutionSelectionCommand,
) -> Result<AiExecutionSelection, TenderCommandError> {
    if connection.status != ProviderConnectionStatus::Ready
        || connection.connection_id != command.connection_id
    {
        return Err(TenderCommandError::new(TenderErrorCode::AiProviderRequired));
    }
    let model = connection
        .models
        .iter()
        .find(|model| model.model_id == command.model_id)
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
    if !model
        .reasoning_options
        .iter()
        .any(|option| option.selection == command.reasoning)
    {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    Ok(AiExecutionSelection {
        connection_id: connection.connection_id.clone(),
        provider: connection.provider,
        model_id: model.model_id.clone(),
        reasoning: command.reasoning.clone(),
        catalogue_fetched_at: connection
            .catalogue_fetched_at
            .clone()
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::AiProviderRequired))?,
        adapter_version: connection.adapter_version.clone(),
    })
}

fn update_ai_execution_selection_projection(
    application_home: &Path,
    command: &UpdateAiExecutionSelectionCommand,
) -> Result<ApplicationSettingsView, TenderCommandError> {
    update_ai_execution_selection_projection_with(application_home, command, || {
        chatgpt_connection_readiness_unlocked(
            application_home,
            crate::chatgpt_login::PRODUCTION_ISSUER,
            current_time_ms(),
        )
    })
}

fn update_ai_execution_selection_projection_with(
    application_home: &Path,
    command: &UpdateAiExecutionSelectionCommand,
    inspect: impl FnOnce() -> Result<StoredConnection, ProviderFailure>,
) -> Result<ApplicationSettingsView, TenderCommandError> {
    mutate_ai_execution_selection_projection_with(application_home, command, false, inspect)
}

fn confirm_ai_execution_selection_projection(
    application_home: &Path,
    command: &UpdateAiExecutionSelectionCommand,
) -> Result<ApplicationSettingsView, TenderCommandError> {
    confirm_ai_execution_selection_projection_with(application_home, command, || {
        chatgpt_connection_readiness_unlocked(
            application_home,
            crate::chatgpt_login::PRODUCTION_ISSUER,
            current_time_ms(),
        )
    })
}

fn confirm_ai_execution_selection_projection_with(
    application_home: &Path,
    command: &UpdateAiExecutionSelectionCommand,
    inspect: impl FnOnce() -> Result<StoredConnection, ProviderFailure>,
) -> Result<ApplicationSettingsView, TenderCommandError> {
    mutate_ai_execution_selection_projection_with(application_home, command, true, inspect)
}

fn mutate_ai_execution_selection_projection_with(
    application_home: &Path,
    command: &UpdateAiExecutionSelectionCommand,
    confirm: bool,
    inspect: impl FnOnce() -> Result<StoredConnection, ProviderFailure>,
) -> Result<ApplicationSettingsView, TenderCommandError> {
    with_connection_mutation(|| {
        let stored_connection = match inspect() {
            Ok(connection) => connection,
            Err(failure) => {
                save_live_connection_unlocked(
                    application_home,
                    &chatgpt_connection_failure_view(failure.category),
                )?;
                return Err(TenderCommandError::new(TenderErrorCode::AiProviderRequired));
            }
        };
        let connection = chatgpt_connection_view(&stored_connection);
        let selection = match selection_from_command(&connection, command) {
            Ok(selection) => selection,
            Err(error) => {
                save_live_connection_unlocked(application_home, &connection)?;
                return Err(error);
            }
        };
        let approval = confirm.then(|| AiExecutionApproval {
            connection_id: selection.connection_id.clone(),
            provider: selection.provider,
            account_fingerprint: account_fingerprint(&connection),
            model_id: selection.model_id.clone(),
            reasoning: selection.reasoning.clone(),
            data_destination: data_destination(connection.provider).to_owned(),
            approved_at: Timestamp::now().to_string(),
        });
        save_connection_and_selection_unlocked(
            application_home,
            &connection,
            &selection,
            approval,
        )?;
        load_application_settings(application_home)
    })
}

pub(crate) fn save_live_connection(
    application_home: &Path,
    connection: &ProviderConnectionView,
) -> Result<(), TenderCommandError> {
    with_connection_mutation(|| save_live_connection_unlocked(application_home, connection))
}

pub(crate) fn save_authorized_connection_projection_unlocked(
    application_home: &Path,
    connection: &StoredConnection,
) -> Result<(), TenderCommandError> {
    save_live_connection_unlocked(application_home, &chatgpt_connection_view(connection))
}

fn save_live_connection_unlocked(
    application_home: &Path,
    connection: &ProviderConnectionView,
) -> Result<(), TenderCommandError> {
    let mut database = settings_connection(application_home)?;
    let transaction = database
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(settings_store_error)?;
    let persisted = connection.clone();
    upsert_connection(&transaction, &persisted)?;
    let mut settings = load_stored_settings(&transaction)?;
    let had_selection = settings.ai_execution_selection.is_some();
    settings.ai_execution_selection = match settings.ai_execution_selection.clone() {
        Some(selection) if selection_is_supported(&persisted, &selection) => {
            Some(AiExecutionSelection {
                catalogue_fetched_at: persisted
                    .catalogue_fetched_at
                    .clone()
                    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::AiProviderRequired))?,
                adapter_version: persisted.adapter_version.clone(),
                ..selection
            })
        }
        Some(selection) => Some(selection),
        None if persisted.status == ProviderConnectionStatus::Ready => {
            default_selection(&persisted)?
        }
        None => None,
    };
    let approval_is_current = had_selection
        && settings
            .ai_execution_selection
            .as_ref()
            .zip(settings.ai_execution_approval.as_ref())
            .is_some_and(|(selection, approval)| {
                selection_is_supported(&persisted, selection)
                    && approval_matches(&persisted, selection, approval)
            });
    if settings.ai_execution_approval.is_some() && !approval_is_current {
        settings.ai_execution_approval = None;
    }
    store_application_settings(&transaction, &settings)?;
    transaction.commit().map_err(settings_store_error)
}

#[cfg(feature = "runtime-fixture")]
pub(crate) fn save_codex_connection_status(
    application_home: &Path,
    status: ProviderConnectionStatus,
    status_summary: &str,
) -> Result<(), TenderCommandError> {
    let mut database = settings_connection(application_home)?;
    let transaction = database
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(settings_store_error)?;
    let existing = transaction
        .query_row(
            "SELECT connection_json FROM provider_connections WHERE connection_id = ?1",
            [CODEX_CONNECTION_ID],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(settings_store_error)?
        .map(|raw| serde_json::from_str::<ProviderConnectionView>(&raw))
        .transpose()
        .map_err(settings_json_error)?;
    let mut connection = existing.unwrap_or_else(|| ProviderConnectionView {
        connection_id: CODEX_CONNECTION_ID.to_owned(),
        provider: AiProviderKind::Codex,
        display_name: "OpenAI account via Codex".to_owned(),
        status,
        account_label: None,
        account_plan: None,
        models: Vec::new(),
        catalogue_fetched_at: None,
        adapter_version: codex_connection_version(),
        status_summary: status_summary.to_owned(),
    });
    connection.status = status;
    connection.status_summary = status_summary.to_owned();
    upsert_connection(&transaction, &connection)?;
    transaction.commit().map_err(settings_store_error)
}

fn save_connection_and_selection_unlocked(
    application_home: &Path,
    connection: &ProviderConnectionView,
    selection: &AiExecutionSelection,
    approval: Option<AiExecutionApproval>,
) -> Result<(), TenderCommandError> {
    let mut database = settings_connection(application_home)?;
    let transaction = database
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(settings_store_error)?;
    upsert_connection(&transaction, connection)?;
    let mut stored = load_stored_settings(&transaction)?;
    stored.ai_execution_selection = Some(selection.clone());
    stored.ai_execution_approval = approval;
    store_application_settings(&transaction, &stored)?;
    transaction.commit().map_err(settings_store_error)
}

fn selection_is_supported(
    connection: &ProviderConnectionView,
    selection: &AiExecutionSelection,
) -> bool {
    connection.status == ProviderConnectionStatus::Ready
        && connection.connection_id == selection.connection_id
        && connection.provider == selection.provider
        && connection.models.iter().any(|model| {
            model.model_id == selection.model_id
                && model
                    .reasoning_options
                    .iter()
                    .any(|option| option.selection == selection.reasoning)
        })
}

fn data_destination(provider: AiProviderKind) -> &'static str {
    match provider {
        AiProviderKind::Codex => "ChatGPT subscription",
    }
}

fn default_selection(
    connection: &ProviderConnectionView,
) -> Result<Option<AiExecutionSelection>, TenderCommandError> {
    let Some(model) = connection.models.iter().find(|model| model.is_default) else {
        return Ok(None);
    };
    let Some(reasoning) = model
        .reasoning_options
        .iter()
        .find(|reasoning| reasoning.is_default)
    else {
        return Ok(None);
    };
    let Some(catalogue_fetched_at) = connection.catalogue_fetched_at.clone() else {
        return Ok(None);
    };
    Ok(Some(AiExecutionSelection {
        connection_id: connection.connection_id.clone(),
        provider: connection.provider,
        model_id: model.model_id.clone(),
        reasoning: reasoning.selection.clone(),
        catalogue_fetched_at,
        adapter_version: connection.adapter_version.clone(),
    }))
}

fn account_fingerprint(connection: &ProviderConnectionView) -> String {
    let mut hasher = Sha256::new();
    hasher.update(connection.connection_id.as_bytes());
    hasher.update([0]);
    hasher.update(connection.provider.as_str().as_bytes());
    hasher.update([0]);
    hasher.update(
        connection
            .account_label
            .as_deref()
            .unwrap_or("unknown")
            .as_bytes(),
    );
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn approval_matches(
    connection: &ProviderConnectionView,
    selection: &AiExecutionSelection,
    approval: &AiExecutionApproval,
) -> bool {
    approval.connection_id == selection.connection_id
        && approval.provider == selection.provider
        && approval.model_id == selection.model_id
        && approval.reasoning == selection.reasoning
        && approval.data_destination == data_destination(connection.provider)
        && approval.account_fingerprint == account_fingerprint(connection)
}

fn upsert_connection(
    transaction: &rusqlite::Transaction<'_>,
    connection: &ProviderConnectionView,
) -> Result<(), TenderCommandError> {
    let connection_json = serde_json::to_string(connection).map_err(settings_json_error)?;
    transaction
        .execute(
            "INSERT INTO provider_connections (
               connection_id, provider_kind, connection_json, updated_at
             ) VALUES (?1, ?2, ?3, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
             ON CONFLICT(connection_id) DO UPDATE SET
               provider_kind = excluded.provider_kind,
               connection_json = excluded.connection_json,
               updated_at = excluded.updated_at",
            params![
                connection.connection_id,
                connection.provider.as_str(),
                connection_json
            ],
        )
        .map_err(settings_store_error)?;
    Ok(())
}

fn store_application_settings(
    transaction: &rusqlite::Transaction<'_>,
    stored: &StoredApplicationSettings,
) -> Result<(), TenderCommandError> {
    let settings_json = serde_json::to_string(&stored).map_err(settings_json_error)?;
    transaction
        .execute(
            "UPDATE application_settings
             SET settings_json = ?1, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE singleton = 1",
            [settings_json],
        )
        .map_err(settings_store_error)?;
    Ok(())
}

#[cfg(any(test, feature = "runtime-fixture"))]
pub(crate) fn seed_runtime_fixture_ai_selection(
    application_home: &Path,
) -> Result<(), TenderCommandError> {
    let connection = ProviderConnectionView {
        connection_id: CODEX_CONNECTION_ID.to_owned(),
        provider: AiProviderKind::Codex,
        display_name: "OpenAI account via Codex".to_owned(),
        status: ProviderConnectionStatus::Ready,
        account_label: None,
        account_plan: Some("plus".to_owned()),
        models: vec![ProviderModelOption {
            model_id: "gpt-5.6-terra".to_owned(),
            display_name: "GPT-5.6 Terra".to_owned(),
            description: "Fixture Codex model".to_owned(),
            is_default: true,
            input_modalities: vec!["text".to_owned()],
            reasoning_options: vec![ProviderReasoningOption {
                selection: ProviderReasoningSelection::CodexEffort("medium".to_owned()),
                label: "medium".to_owned(),
                description: "Fixture reasoning effort".to_owned(),
                is_default: true,
            }],
        }],
        catalogue_fetched_at: Some("fixture-catalogue".to_owned()),
        adapter_version: codex_connection_version(),
        status_summary: "Fixture provider ready.".to_owned(),
    };
    let selection = AiExecutionSelection {
        connection_id: connection.connection_id.clone(),
        provider: connection.provider,
        model_id: "gpt-5.6-terra".to_owned(),
        reasoning: ProviderReasoningSelection::CodexEffort("medium".to_owned()),
        catalogue_fetched_at: "fixture-catalogue".to_owned(),
        adapter_version: connection.adapter_version.clone(),
    };
    let mut database = settings_connection(application_home)?;
    let transaction = database
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(settings_store_error)?;
    let stored = load_stored_settings(&transaction)?;
    if stored.ai_execution_selection.is_some() {
        drop(transaction);
        return Ok(());
    }
    upsert_connection(&transaction, &connection)?;
    let mut stored = stored;
    stored.ai_execution_selection = Some(selection);
    store_application_settings(&transaction, &stored)?;
    transaction.commit().map_err(settings_store_error)
}

pub(crate) fn load_application_settings(
    application_home: &Path,
) -> Result<ApplicationSettingsView, TenderCommandError> {
    let database = settings_connection(application_home)?;
    let settings = load_stored_settings(&database)?;
    let mut statement = database
        .prepare(
            "SELECT connection_json FROM provider_connections
             WHERE connection_id = ?1",
        )
        .map_err(settings_store_error)?;
    let mut provider_connections = statement
        .query_map([CODEX_CONNECTION_ID], |row| row.get::<_, String>(0))
        .map_err(settings_store_error)?
        .map(|raw| {
            raw.map_err(settings_store_error)
                .and_then(|raw| serde_json::from_str(&raw).map_err(settings_json_error))
        })
        .collect::<Result<Vec<ProviderConnectionView>, TenderCommandError>>()?;
    if provider_connections.is_empty() {
        provider_connections.push(chatgpt_connection_failure_view(
            ProviderFailureCategory::AuthenticationRequired,
        ));
    }
    Ok(ApplicationSettingsView {
        general_preferences: settings.general_preferences,
        ai_execution_selection: settings.ai_execution_selection,
        ai_execution_approval: settings.ai_execution_approval,
        provider_connections,
        chatgpt: crate::chatgpt_login::chatgpt_connection_status(application_home),
        storage: ApplicationStorageFacts {
            application_home: application_home.to_string_lossy().into_owned(),
            tender_backups_are_preserved: true,
            trash_requires_explicit_purge: true,
        },
        diagnostics: ApplicationDiagnostics {
            quantix_version: env!("CARGO_PKG_VERSION").to_owned(),
            installation_schema_version: INSTALLATION_SCHEMA_VERSION,
            tender_schema_version: TENDER_SCHEMA_VERSION,
        },
    })
}

fn load_stored_settings(
    connection: &Connection,
) -> Result<StoredApplicationSettings, TenderCommandError> {
    let raw = connection
        .query_row(
            "SELECT settings_json FROM application_settings WHERE singleton = 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(settings_store_error)?
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
    serde_json::from_str(&raw).map_err(settings_json_error)
}

fn settings_connection(application_home: &Path) -> Result<Connection, TenderCommandError> {
    let connection = Connection::open_with_flags(
        application_home.join("installation.sqlite"),
        OpenFlags::SQLITE_OPEN_READ_WRITE,
    )
    .map_err(settings_store_error)?;
    connection
        .busy_timeout(Duration::from_secs(5))
        .map_err(settings_store_error)?;
    connection
        .pragma_update(None, "foreign_keys", true)
        .map_err(settings_store_error)?;
    Ok(connection)
}

fn settings_store_error(_: rusqlite::Error) -> TenderCommandError {
    TenderCommandError::new(TenderErrorCode::StoreUnavailable)
}

fn settings_json_error(_: serde_json::Error) -> TenderCommandError {
    TenderCommandError::new(TenderErrorCode::IntegrityFailed)
}

#[cfg(any(test, feature = "runtime-fixture"))]
pub(crate) fn codex_connection_version() -> String {
    crate::agent_runtime::CODEX_VERSION.to_owned()
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::path::PathBuf;
    use std::sync::mpsc;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use super::*;
    use crate::chatgpt_login::PRODUCTION_ISSUER;
    use crate::chatgpt_oauth::save;

    fn temp_home(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("quantix-settings-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn stored_connection(expires_at_ms: u64, plan_type: Option<&str>) -> StoredConnection {
        StoredConnection {
            access_token: "at-current".to_owned(),
            refresh_token: "rt-current".to_owned(),
            id_token: "idt-current".to_owned(),
            expires_at_ms,
            account_id: "acc-77".to_owned(),
            plan_type: plan_type.map(str::to_owned),
            compute_residency: None,
        }
    }

    fn initialized_home(name: &str) -> PathBuf {
        let home = temp_home(name);
        let host = crate::QuantixHost::new(&home, &home);
        let outcome = host.ensure_setup();
        assert!(
            matches!(
                outcome.state,
                crate::SetupState::Ready | crate::SetupState::Warning
            ),
            "settings database ready: {outcome:?}"
        );
        home
    }

    fn ready_connection(account_id: &str) -> ProviderConnectionView {
        chatgpt_connection_view(&StoredConnection {
            access_token: format!("at-{account_id}"),
            refresh_token: format!("rt-{account_id}"),
            id_token: format!("idt-{account_id}"),
            expires_at_ms: 9_999_999,
            account_id: account_id.to_owned(),
            plan_type: Some("plus".to_owned()),
            compute_residency: None,
        })
    }

    fn store_test_approval(
        home: &Path,
        connection: &ProviderConnectionView,
        data_destination: &str,
    ) -> AiExecutionSelection {
        save_live_connection(home, connection).unwrap();
        let mut database = settings_connection(home).unwrap();
        let transaction = database
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let mut stored = load_stored_settings(&transaction).unwrap();
        let selection = stored
            .ai_execution_selection
            .clone()
            .expect("ready connection prepares a selection");
        stored.ai_execution_approval = Some(AiExecutionApproval {
            connection_id: selection.connection_id.clone(),
            provider: selection.provider,
            account_fingerprint: account_fingerprint(connection),
            model_id: selection.model_id.clone(),
            reasoning: selection.reasoning.clone(),
            data_destination: data_destination.to_owned(),
            approved_at: "2026-08-22T00:00:00Z".to_owned(),
        });
        store_application_settings(&transaction, &stored).unwrap();
        transaction.commit().unwrap();
        selection
    }

    #[test]
    fn ready_refresh_projection_cannot_finish_after_disconnect_projection() {
        let home = initialized_home("refresh-disconnect-serialization");
        let stored = stored_connection(9_999_999, Some("plus"));
        save(&home, &stored).unwrap();
        let (observed_tx, observed_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let refresh_home = home.clone();
        let refresh = std::thread::spawn(move || {
            project_chatgpt_connection_readiness_with(&refresh_home, || {
                observed_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                Ok(stored)
            })
        });
        observed_rx.recv_timeout(Duration::from_secs(5)).unwrap();

        let (attempting_tx, attempting_rx) = mpsc::channel();
        let (entered_tx, entered_rx) = mpsc::channel();
        let disconnect_home = home.clone();
        let disconnect = std::thread::spawn(move || {
            attempting_tx.send(()).unwrap();
            crate::chatgpt_oauth::with_connection_mutation(|| {
                entered_tx.send(()).unwrap();
                crate::chatgpt_oauth::clear_unlocked(&disconnect_home).unwrap();
                save_chatgpt_disconnected_unlocked(&disconnect_home).unwrap();
            });
        });
        attempting_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(entered_rx.recv_timeout(Duration::from_millis(150)).is_err());

        release_tx.send(()).unwrap();
        assert_eq!(
            refresh.join().unwrap().unwrap().unwrap().account_id,
            "acc-77"
        );
        entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        disconnect.join().unwrap();

        assert!(matches!(load(&home), LoadState::Absent));
        let view = load_application_settings(&home).unwrap();
        assert_eq!(
            view.provider_connections[0].status,
            ProviderConnectionStatus::AuthenticationRequired
        );
        assert!(view.ai_execution_selection.is_none());
        assert!(view.ai_execution_approval.is_none());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn runtime_failure_projection_cannot_finish_after_login_ready_projection() {
        let home = initialized_home("refresh-login-serialization");
        let connected = stored_connection(9_999_999, Some("plus"));
        let (observed_tx, observed_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let refresh_home = home.clone();
        let refresh = std::thread::spawn(move || {
            project_chatgpt_connection_readiness_with(&refresh_home, || {
                observed_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                Err(chatgpt_authentication_failure())
            })
        });
        observed_rx.recv_timeout(Duration::from_secs(5)).unwrap();

        let (attempting_tx, attempting_rx) = mpsc::channel();
        let (entered_tx, entered_rx) = mpsc::channel();
        let login_home = home.clone();
        let login = std::thread::spawn(move || {
            attempting_tx.send(()).unwrap();
            crate::chatgpt_oauth::with_connection_mutation(|| {
                entered_tx.send(()).unwrap();
                crate::chatgpt_oauth::save_unlocked(&login_home, &connected).unwrap();
                save_authorized_connection_projection_unlocked(&login_home, &connected).unwrap();
            });
        });
        attempting_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(entered_rx.recv_timeout(Duration::from_millis(150)).is_err());

        release_tx.send(()).unwrap();
        assert_eq!(
            refresh.join().unwrap().unwrap().unwrap_err().category,
            ProviderFailureCategory::AuthenticationRequired
        );
        entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        login.join().unwrap();

        assert!(matches!(load(&home), LoadState::Connected(_)));
        let view = load_application_settings(&home).unwrap();
        assert_eq!(
            view.chatgpt.state,
            crate::chatgpt_login::ChatGptConnectionState::Connected
        );
        assert_eq!(
            view.provider_connections[0].status,
            ProviderConnectionStatus::Ready
        );
        assert_eq!(
            view.provider_connections[0].account_label.as_deref(),
            Some("acc-77")
        );
        assert!(view.ai_execution_selection.is_some());
        assert!(view.ai_execution_approval.is_none());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn selection_update_cannot_publish_after_a_later_disconnect() {
        let home = initialized_home("selection-update-disconnect-serialization");
        let stored = stored_connection(9_999_999, Some("plus"));
        save(&home, &stored).unwrap();
        save_live_connection(&home, &chatgpt_connection_view(&stored)).unwrap();
        let command = UpdateAiExecutionSelectionCommand {
            connection_id: CODEX_CONNECTION_ID.to_owned(),
            model_id: "gpt-5.4".to_owned(),
            reasoning: ProviderReasoningSelection::CodexEffort("high".to_owned()),
        };
        let (observed_tx, observed_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let update_home = home.clone();
        let update = std::thread::spawn(move || {
            update_ai_execution_selection_projection_with(&update_home, &command, || {
                observed_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                Ok(stored)
            })
        });
        observed_rx.recv_timeout(Duration::from_secs(5)).unwrap();

        let (attempting_tx, attempting_rx) = mpsc::channel();
        let (entered_tx, entered_rx) = mpsc::channel();
        let disconnect_home = home.clone();
        let disconnect = std::thread::spawn(move || {
            attempting_tx.send(()).unwrap();
            crate::chatgpt_oauth::with_connection_mutation(|| {
                entered_tx.send(()).unwrap();
                crate::chatgpt_oauth::clear_unlocked(&disconnect_home).unwrap();
                save_chatgpt_disconnected_unlocked(&disconnect_home).unwrap();
            });
        });
        attempting_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(entered_rx.recv_timeout(Duration::from_millis(150)).is_err());

        release_tx.send(()).unwrap();
        let selected = update.join().unwrap().unwrap();
        assert_eq!(selected.ai_execution_selection.unwrap().model_id, "gpt-5.4");
        entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        disconnect.join().unwrap();

        assert!(matches!(load(&home), LoadState::Absent));
        let view = load_application_settings(&home).unwrap();
        assert_eq!(
            view.provider_connections[0].status,
            ProviderConnectionStatus::AuthenticationRequired
        );
        assert!(view.ai_execution_selection.is_none());
        assert!(view.ai_execution_approval.is_none());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn confirmation_cannot_publish_an_old_account_approval_after_account_replacement() {
        let home = initialized_home("confirmation-account-serialization");
        let original = StoredConnection {
            account_id: "account-a".to_owned(),
            ..stored_connection(9_999_999, Some("plus"))
        };
        save(&home, &original).unwrap();
        save_live_connection(&home, &chatgpt_connection_view(&original)).unwrap();
        let command = UpdateAiExecutionSelectionCommand {
            connection_id: CODEX_CONNECTION_ID.to_owned(),
            model_id: "gpt-5.5".to_owned(),
            reasoning: ProviderReasoningSelection::CodexEffort("medium".to_owned()),
        };
        let (observed_tx, observed_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let confirmation_home = home.clone();
        let confirmation = std::thread::spawn(move || {
            confirm_ai_execution_selection_projection_with(&confirmation_home, &command, || {
                observed_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                Ok(original)
            })
        });
        observed_rx.recv_timeout(Duration::from_secs(5)).unwrap();

        let replacement = StoredConnection {
            account_id: "account-b".to_owned(),
            ..stored_connection(9_999_999, Some("plus"))
        };
        let (attempting_tx, attempting_rx) = mpsc::channel();
        let (entered_tx, entered_rx) = mpsc::channel();
        let replacement_home = home.clone();
        let replacement_thread = std::thread::spawn(move || {
            attempting_tx.send(()).unwrap();
            crate::chatgpt_oauth::with_connection_mutation(|| {
                entered_tx.send(()).unwrap();
                crate::chatgpt_oauth::save_unlocked(&replacement_home, &replacement).unwrap();
                save_authorized_connection_projection_unlocked(&replacement_home, &replacement)
                    .unwrap();
            });
        });
        attempting_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(entered_rx.recv_timeout(Duration::from_millis(150)).is_err());

        release_tx.send(()).unwrap();
        let approved = confirmation.join().unwrap().unwrap();
        assert!(approved.ai_execution_approval.is_some());
        entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        replacement_thread.join().unwrap();

        match load(&home) {
            LoadState::Connected(connection) => assert_eq!(connection.account_id, "account-b"),
            other => panic!("expected replacement connection, got {other:?}"),
        }
        let view = load_application_settings(&home).unwrap();
        assert_eq!(
            view.provider_connections[0].account_label.as_deref(),
            Some("account-b")
        );
        assert!(view.ai_execution_selection.is_some());
        assert!(view.ai_execution_approval.is_none());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[tokio::test]
    async fn stale_cached_tender_readiness_cannot_authorize_a_replacement_account() {
        let home = initialized_home("stale-tender-ready-account-replacement");
        let host = crate::QuantixHost::new(&home, &home);
        let original = StoredConnection {
            account_id: "account-a".to_owned(),
            ..stored_connection(9_999_999_999, Some("plus"))
        };
        save(&home, &original).unwrap();
        store_test_approval(
            &home,
            &chatgpt_connection_view(&original),
            "ChatGPT subscription",
        );
        let tender = host
            .create_tender(crate::tender_store::CreateTenderCommand {
                name: "Cached ready tender".to_owned(),
            })
            .unwrap();
        let tender_id = TenderId::parse(&tender.tender_id).unwrap();
        assert_eq!(
            host.inspect_tender_ai_execution(InspectTenderAiExecutionCommand {
                tender_id: tender.tender_id.clone(),
            })
            .unwrap()
            .readiness,
            TenderAiSelectionReadiness::Ready
        );

        let replacement = StoredConnection {
            account_id: "account-b".to_owned(),
            ..stored_connection(9_999_999_999, Some("plus"))
        };
        save(&home, &replacement).unwrap();
        save_live_connection(&home, &chatgpt_connection_view(&replacement)).unwrap();

        assert_eq!(
            host.require_current_tender_ai_selection(&tender_id)
                .await
                .unwrap_err()
                .code,
            TenderErrorCode::AiProviderRequired
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn provider_turn_rejects_a_selection_approved_for_another_account() {
        let home = initialized_home("provider-turn-account-selection");
        let original = StoredConnection {
            account_id: "account-a".to_owned(),
            ..stored_connection(9_999_999_999, Some("plus"))
        };
        save(&home, &original).unwrap();
        let selection = store_test_approval(
            &home,
            &chatgpt_connection_view(&original),
            "ChatGPT subscription",
        );
        let replacement = StoredConnection {
            account_id: "account-b".to_owned(),
            ..stored_connection(9_999_999_999, Some("plus"))
        };
        save(&home, &replacement).unwrap();

        let failure =
            project_approved_chatgpt_connection_with(&home, &selection, || Ok(replacement))
                .unwrap_err();

        assert_eq!(
            failure.category,
            ProviderFailureCategory::AuthenticationRequired
        );
        let view = load_application_settings(&home).unwrap();
        assert_eq!(
            view.provider_connections[0].account_label.as_deref(),
            Some("account-b")
        );
        assert!(view.ai_execution_approval.is_none());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn unauthorized_retry_rejects_replacement_account_before_refresh() {
        let home = initialized_home("provider-retry-account-replacement");
        let original = StoredConnection {
            account_id: "account-a".to_owned(),
            ..stored_connection(9_999_999_999, Some("plus"))
        };
        save(&home, &original).unwrap();
        let selection = store_test_approval(
            &home,
            &chatgpt_connection_view(&original),
            "ChatGPT subscription",
        );
        let replacement = StoredConnection {
            account_id: "account-b".to_owned(),
            ..stored_connection(9_999_999_999, Some("plus"))
        };
        save(&home, &replacement).unwrap();
        let (issuer, requests) = mock_issuer(refresh_body("account-b", Some("plus"), None));
        let token_client = TokenClient::new(&issuer).unwrap();

        let failure = refresh_approved_chatgpt_connection(
            &home,
            &token_client,
            9_999_999_999,
            "account-a",
            &selection,
        )
        .unwrap_err();

        assert_eq!(
            failure.category,
            ProviderFailureCategory::AuthenticationRequired
        );
        assert!(
            requests.lock().unwrap().is_empty(),
            "a replacement account must be rejected before spending its refresh credential"
        );
        match load(&home) {
            LoadState::Connected(current) => assert_eq!(current.account_id, "account-b"),
            other => panic!("replacement account remains stored, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn projection_failure_never_restores_a_consumed_refresh_token() {
        let home = initialized_home("projection-failure-forward-only-refresh");
        let previous = stored_connection(1_000, Some("plus"));
        save(&home, &previous).unwrap();
        let database = rusqlite::Connection::open(home.join("installation.sqlite")).unwrap();
        database
            .execute("DROP TABLE provider_connections", [])
            .unwrap();
        drop(database);
        let rotated = StoredConnection {
            access_token: "at-rotated".to_owned(),
            refresh_token: "rt-rotated".to_owned(),
            expires_at_ms: 9_999_999_999,
            ..previous
        };

        assert!(project_chatgpt_connection_readiness_with(&home, || {
            crate::chatgpt_oauth::save_unlocked(&home, &rotated).unwrap();
            Ok(rotated.clone())
        })
        .is_err());
        match load(&home) {
            LoadState::Connected(connection) => {
                assert_eq!(connection.access_token, "at-rotated");
                assert_eq!(connection.refresh_token, "rt-rotated");
            }
            other => panic!("rotated connection must remain authoritative, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn live_connection_account_change_invalidates_execution_approval() {
        let home = initialized_home("approval-account-change");
        let original = ready_connection("account-a");
        let selection = store_test_approval(&home, &original, "ChatGPT subscription");
        assert!(load_application_settings(&home)
            .unwrap()
            .ai_execution_approval
            .is_some());

        save_live_connection(&home, &ready_connection("account-b")).unwrap();

        let view = load_application_settings(&home).unwrap();
        assert_eq!(view.ai_execution_selection.as_ref(), Some(&selection));
        assert!(view.ai_execution_approval.is_none());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn live_connection_data_destination_mismatch_invalidates_execution_approval() {
        let home = initialized_home("approval-destination-change");
        let connection = ready_connection("account-a");
        store_test_approval(&home, &connection, "A different destination");

        save_live_connection(&home, &connection).unwrap();

        assert!(load_application_settings(&home)
            .unwrap()
            .ai_execution_approval
            .is_none());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn direct_chatgpt_catalogue_uses_the_stored_account_and_stable_version() {
        let connection = chatgpt_connection_view(&stored_connection(9_999_999, Some("plus")));

        assert_eq!(connection.status, ProviderConnectionStatus::Ready);
        assert_eq!(connection.account_label.as_deref(), Some("acc-77"));
        assert_eq!(connection.account_plan.as_deref(), Some("plus"));
        assert_eq!(connection.adapter_version, "chatgpt-direct-v1");
        assert_eq!(
            connection
                .models
                .iter()
                .map(|model| model.model_id.as_str())
                .collect::<Vec<_>>(),
            vec!["gpt-5.5", "gpt-5.4", "gpt-5.4-mini", "gpt-5.3-codex-spark"]
        );
    }

    fn jwt_id_token(
        account_id: &str,
        plan_type: Option<&str>,
        compute_residency: Option<&str>,
    ) -> String {
        let header =
            crate::chatgpt_oauth::crypto::base64url_encode(br#"{"alg":"RS256","typ":"JWT"}"#);
        let mut claims = serde_json::Map::new();
        claims.insert(
            "chatgpt_account_id".to_owned(),
            serde_json::Value::String(account_id.to_owned()),
        );
        if let Some(plan_type) = plan_type {
            claims.insert(
                "https://api.openai.com/auth".to_owned(),
                serde_json::json!({
                    "chatgpt_plan_type": plan_type,
                    "chatgpt_compute_residency": compute_residency,
                }),
            );
        }
        let payload = crate::chatgpt_oauth::crypto::base64url_encode(
            serde_json::Value::Object(claims).to_string().as_bytes(),
        );
        format!("{header}.{payload}.signature")
    }

    fn refresh_body(
        account_id: &str,
        plan_type: Option<&str>,
        compute_residency: Option<&str>,
    ) -> String {
        serde_json::json!({
            "access_token": "at-new",
            "refresh_token": "rt-new",
            "id_token": jwt_id_token(account_id, plan_type, compute_residency),
            "expires_in": 3600,
        })
        .to_string()
    }

    fn mock_issuer(body: impl Into<String>) -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let bodies: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let shared = Arc::clone(&bodies);
        let body = body.into();
        std::thread::spawn(move || {
            for mut stream in listener.incoming().flatten() {
                if let Some(form) = read_form(&mut stream) {
                    shared.lock().unwrap().push(form);
                }
                write_body(&mut stream, &body);
            }
        });
        (format!("http://127.0.0.1:{port}"), bodies)
    }

    fn read_form(stream: &mut TcpStream) -> Option<String> {
        let mut raw = Vec::new();
        let mut chunk = [0u8; 512];
        loop {
            if raw.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
            let read = stream.read(&mut chunk).ok()?;
            if read == 0 {
                break;
            }
            raw.extend_from_slice(&chunk[..read]);
        }
        let split = raw.windows(4).position(|window| window == b"\r\n\r\n")?;
        Some(String::from_utf8_lossy(&raw[split + 4..]).to_string())
    }

    fn write_body(stream: &mut TcpStream, body: &str) {
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.flush();
    }

    #[test]
    fn a_fresh_eligible_connection_is_ready_without_contacting_the_issuer() {
        let home = temp_home("ready");
        save(&home, &stored_connection(9_999_999, Some("plus"))).unwrap();

        let connection = chatgpt_connection_readiness(&home, PRODUCTION_ISSUER, 1_000_000)
            .expect("fresh eligible connection is ready");

        assert_eq!(connection.access_token, "at-current");
        assert_eq!(connection.plan_type.as_deref(), Some("plus"));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn an_expired_connection_refreshes_through_the_issuer_and_persists() {
        let home = temp_home("refresh-required");
        save(&home, &stored_connection(1_000_000, Some("team"))).unwrap();
        let (issuer, bodies) = mock_issuer(refresh_body("acc-refreshed", Some("plus"), Some("eu")));

        let connection = chatgpt_connection_readiness(&home, &issuer, 2_000_000)
            .expect("expired connection must auto-refresh");

        assert_eq!(connection.access_token, "at-new");
        assert_eq!(connection.refresh_token, "rt-new");
        assert_eq!(connection.account_id, "acc-refreshed");
        assert_eq!(connection.plan_type.as_deref(), Some("plus"));
        assert_eq!(connection.compute_residency.as_deref(), Some("eu"));
        assert_eq!(connection.expires_at_ms, 2_000_000 + 3_600_000);
        let forms = bodies.lock().unwrap();
        assert_eq!(forms.len(), 1, "exactly one refresh call");
        assert!(forms[0].contains("grant_type=refresh_token"));
        drop(forms);
        match load(&home) {
            LoadState::Connected(persisted) => {
                assert_eq!(persisted.refresh_token, "rt-new");
                assert_eq!(persisted.account_id, "acc-refreshed");
                assert_eq!(persisted.plan_type.as_deref(), Some("plus"));
                assert_eq!(persisted.compute_residency.as_deref(), Some("eu"));
            }
            other => panic!("expected persisted refreshed store, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn an_expired_stale_plan_refreshes_before_subscription_validation() {
        let home = temp_home("stale-plan-refresh");
        save(&home, &stored_connection(1_000, Some("free"))).unwrap();
        let (issuer, bodies) = mock_issuer(refresh_body(
            "acc-current",
            Some("team"),
            Some("no_constraint"),
        ));

        let connection = chatgpt_connection_readiness(&home, &issuer, 2_000_000)
            .expect("the refreshed eligible identity is authoritative");

        assert_eq!(connection.account_id, "acc-current");
        assert_eq!(connection.plan_type.as_deref(), Some("team"));
        assert_eq!(connection.compute_residency, None);
        assert_eq!(bodies.lock().unwrap().len(), 1, "refresh is attempted");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn an_absent_store_requires_authentication() {
        let home = temp_home("absent");

        let failure = chatgpt_connection_readiness(&home, PRODUCTION_ISSUER, 1_000_000)
            .expect_err("no store must block readiness");

        assert_eq!(
            failure.category,
            ProviderFailureCategory::AuthenticationRequired
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn an_unusable_store_requires_authentication() {
        let home = temp_home("unusable");
        std::fs::write(home.join("auth.json"), "<html>garbage</html>").unwrap();

        let failure = chatgpt_connection_readiness(&home, PRODUCTION_ISSUER, 1_000_000)
            .expect_err("corrupt store must block readiness");

        assert_eq!(
            failure.category,
            ProviderFailureCategory::AuthenticationRequired
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn a_fresh_ineligible_plan_requires_a_subscription_without_refresh() {
        let home = temp_home("blocked-plan");
        save(&home, &stored_connection(9_999_999, Some("free"))).unwrap();
        let (issuer, bodies) = mock_issuer(refresh_body("acc-current", Some("team"), None));

        let failure = chatgpt_connection_readiness(&home, &issuer, 2_000_000)
            .expect_err("ineligible plans must not run");

        assert_eq!(
            failure.category,
            ProviderFailureCategory::SubscriptionRequired
        );
        assert!(
            bodies.lock().unwrap().is_empty(),
            "an ineligible plan must not spend a refresh call"
        );
        match load(&home) {
            LoadState::Connected(persisted) => {
                assert_eq!(persisted.refresh_token, "rt-current", "store untouched");
            }
            other => panic!("expected untouched store, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn a_missing_plan_claim_requires_a_subscription() {
        let home = temp_home("missing-plan");
        save(&home, &stored_connection(9_999_999, None)).unwrap();

        let failure = chatgpt_connection_readiness(&home, PRODUCTION_ISSUER, 1_000_000)
            .expect_err("a missing plan claim must block readiness");

        assert_eq!(
            failure.category,
            ProviderFailureCategory::SubscriptionRequired
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn a_rejected_refresh_requires_authentication_without_clobbering_the_store() {
        let home = temp_home("rejected-refresh");
        save(&home, &stored_connection(1_000, Some("plus"))).unwrap();
        let (issuer, _bodies) = mock_issuer(r#"{"error":"invalid_grant"}"#);

        // The issuer answers HTTP 400 for invalid grants; the mock above always
        // answers 200 with an unparseable success payload instead, which the
        // token client also rejects as malformed.
        let failure = chatgpt_connection_readiness(&home, &issuer, 2_000_000)
            .expect_err("a failed refresh must block readiness");

        assert_eq!(
            failure.category,
            ProviderFailureCategory::AuthenticationRequired
        );
        match load(&home) {
            LoadState::Connected(persisted) => {
                assert_eq!(persisted.refresh_token, "rt-current", "store preserved");
            }
            other => panic!("expected preserved store, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn refreshed_tokens_without_identity_require_authentication_without_clobbering_the_store() {
        let home = temp_home("refresh-missing-identity");
        save(&home, &stored_connection(1_000, Some("plus"))).unwrap();
        let (issuer, _bodies) = mock_issuer(
            serde_json::json!({
                "access_token": "at-new",
                "refresh_token": "rt-new",
                "id_token": "not-a-jwt",
                "expires_in": 3600,
            })
            .to_string(),
        );

        let failure = chatgpt_connection_readiness(&home, &issuer, 2_000_000)
            .expect_err("a refresh without authoritative identity must fail closed");

        assert_eq!(
            failure.category,
            ProviderFailureCategory::AuthenticationRequired
        );
        match load(&home) {
            LoadState::Connected(persisted) => {
                assert_eq!(persisted.access_token, "at-current");
                assert_eq!(persisted.refresh_token, "rt-current");
                assert_eq!(persisted.account_id, "acc-77");
                assert_eq!(persisted.plan_type.as_deref(), Some("plus"));
            }
            other => panic!("expected preserved store, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&home);
    }
}
