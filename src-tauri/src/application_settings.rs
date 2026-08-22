use std::{fs, path::Path, time::Duration};

use garde::Validate;
use jiff::Timestamp;
use rusqlite::{params, Connection, OpenFlags, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ts_rs::TS;

use crate::{
    agent_runtime::{
        inspect_anthropic_connection, inspect_gemini_connection, valid_login_url, AgentProvider,
        ProviderFailure, ProviderFailureCategory, CODEX_VERSION,
    },
    setup::INSTALLATION_SCHEMA_VERSION,
    tender_store::{TenderCommandError, TenderErrorCode, TenderId, TENDER_SCHEMA_VERSION},
    QuantixHost,
};

pub(crate) const CODEX_CONNECTION_ID: &str = "codex_chatgpt";
pub(crate) const ANTHROPIC_CONNECTION_ID: &str = "anthropic_byok";
pub(crate) const ANTHROPIC_ADAPTER_VERSION: &str = "anthropic-messages-v1";
pub(crate) const GEMINI_CONNECTION_ID: &str = "gemini_byok";
pub(crate) const GEMINI_ADAPTER_VERSION: &str = "gemini-generate-content-v1beta";
const CREDENTIAL_SERVICE: &str = "com.quantix.ai-provider";
const ANTHROPIC_CREDENTIAL_ACCOUNT: &str = "anthropic_api_key";
const GEMINI_CREDENTIAL_ACCOUNT: &str = "gemini_api_key";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum AiProviderKind {
    Codex,
    Anthropic,
    Gemini,
}

impl AiProviderKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Anthropic => "anthropic",
            Self::Gemini => "gemini",
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
    AnthropicEffort(String),
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
    pub active_provider_login: Option<ProviderLoginView>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ProviderLoginMethod {
    Browser,
    DeviceCode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ProviderLoginStatus {
    AwaitingUser,
    Cancelling,
    Cancelled,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct ProviderLoginView {
    pub connection_id: String,
    pub login_id: String,
    pub method: ProviderLoginMethod,
    pub status: ProviderLoginStatus,
    #[serde(skip_serializing)]
    #[ts(skip)]
    pub authorization_url: String,
    pub user_code: Option<String>,
    pub status_summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct StartProviderLoginCommand {
    #[garde(skip)]
    pub method: ProviderLoginMethod,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct CancelProviderLoginCommand {
    #[garde(length(bytes, min = 1, max = 200))]
    pub login_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct OpenProviderLoginCommand {
    #[garde(length(bytes, min = 1, max = 200))]
    pub login_id: String,
}

#[derive(Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct ConnectAnthropicCommand {
    #[garde(length(bytes, min = 1, max = 500))]
    pub api_key: String,
}

#[derive(Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct ConnectGeminiCommand {
    #[garde(length(bytes, min = 1, max = 500))]
    pub api_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct DisconnectAiProviderCommand {
    #[garde(length(bytes, min = 1, max = 100))]
    pub connection_id: String,
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
        load_application_settings(self.application_home())
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
        let mut view = load_application_settings(self.application_home())?;
        view.active_provider_login = self
            .agent_provider()
            .lock()
            .await
            .as_ref()
            .and_then(AgentProvider::login_snapshot);
        Ok(view)
    }

    pub async fn refresh_application_settings(
        &self,
    ) -> Result<ApplicationSettingsView, TenderCommandError> {
        if let Ok(api_key) = load_anthropic_api_key() {
            match inspect_anthropic_connection(&api_key).await {
                Ok(connection) => save_live_connection(self.application_home(), &connection)?,
                Err(failure) => save_anthropic_connection_status(
                    self.application_home(),
                    provider_failure_status(failure.category),
                    "Anthropic is unavailable. The key remains only in the system credential vault.",
                )?,
            }
        }
        if let Ok(api_key) = load_gemini_api_key() {
            match inspect_gemini_connection(&api_key).await {
                Ok(connection) => save_live_connection(self.application_home(), &connection)?,
                Err(failure) => save_gemini_connection_status(
                    self.application_home(),
                    provider_failure_status(failure.category),
                    "Gemini is unavailable. The key remains only in the system credential vault.",
                )?,
            }
        }
        let provider_missing = self.agent_provider().lock().await.is_none();
        // Provider discovery is independent from local document-tool
        // readiness. The workspace must be able to discover (or explain the
        // loss of) an AI connection while a Tender remains readable and the
        // local tools are still waiting for an explicit preparation action.
        if provider_missing {
            let _ = self
                .inspect_codex_subscription(tokio_util::sync::CancellationToken::new())
                .await;
        }
        let mut current_provider = self.agent_provider().lock().await.as_ref().cloned();
        if current_provider
            .as_ref()
            .is_some_and(AgentProvider::is_closed)
        {
            if let Some(provider) = current_provider.take() {
                let failure = ProviderFailure::new(
                    ProviderFailureCategory::ProcessFailed,
                    true,
                    "Retry the provider connection.",
                    None,
                );
                self.retire_failed_provider(&provider, &failure).await?;
            }
        }
        let Some(provider) = current_provider else {
            let mut view = load_application_settings(self.application_home())?;
            for connection in &mut view.provider_connections {
                if connection.connection_id == CODEX_CONNECTION_ID
                    && connection.status == ProviderConnectionStatus::Ready
                {
                    connection.status = ProviderConnectionStatus::TemporarilyUnavailable;
                    connection.status_summary =
                        "The local AI runtime is unavailable. Tender records remain accessible."
                            .to_owned();
                }
            }
            return Ok(view);
        };
        let login_snapshot = provider.login_snapshot();
        if login_snapshot.as_ref().is_some_and(|login| {
            matches!(
                login.status,
                ProviderLoginStatus::AwaitingUser | ProviderLoginStatus::Cancelling
            )
        }) {
            let connection = provider.connection_snapshot();
            save_live_connection(self.application_home(), &connection)?;
            let mut view = load_application_settings(self.application_home())?;
            view.active_provider_login = login_snapshot;
            return Ok(view);
        }
        let refreshed = match provider.refresh_readiness().await {
            Ok(refreshed) => refreshed,
            Err(failure) => {
                self.retire_failed_provider(&provider, &failure).await?;
                return load_application_settings(self.application_home());
            }
        };
        if !refreshed {
            let mut view = load_application_settings(self.application_home())?;
            view.active_provider_login = provider.login_snapshot();
            if view.active_provider_login.is_some() {
                return Ok(view);
            }
            if let Some(connection) = view
                .provider_connections
                .iter_mut()
                .find(|connection| connection.connection_id == CODEX_CONNECTION_ID)
            {
                connection.status = ProviderConnectionStatus::TemporarilyUnavailable;
                connection.status_summary =
                    "A live catalog refresh will be available when current Agent Runs finish."
                        .to_owned();
            }
            return Ok(view);
        }
        let connection = provider.connection_snapshot();
        save_live_connection(self.application_home(), &connection)?;
        let mut view = load_application_settings(self.application_home())?;
        view.active_provider_login = provider.login_snapshot();
        Ok(view)
    }

    pub(crate) async fn refresh_exact_ai_execution_selection(
        &self,
        preferred: Option<&AiExecutionSelection>,
    ) -> Result<Option<AiExecutionSelection>, TenderCommandError> {
        let view = self.refresh_application_settings().await?;
        let preferred = preferred
            .cloned()
            .or_else(|| view.ai_execution_selection.clone());
        let Some(preferred) = preferred else {
            return Ok(None);
        };
        let Some(approval) = view.ai_execution_approval.as_ref() else {
            return Ok(None);
        };
        let Some(connection) = view
            .provider_connections
            .iter()
            .find(|connection| connection.connection_id == preferred.connection_id)
        else {
            return Ok(None);
        };
        if !selection_is_supported(connection, &preferred) {
            return Ok(None);
        }
        let selection = selection_with_current_provenance(connection, &preferred)?;
        if !approval_matches(connection, &selection, approval) {
            return Ok(None);
        }
        Ok(Some(selection))
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
        if binding.readiness != TenderAiSelectionReadiness::Ready {
            return Err(TenderCommandError::new(TenderErrorCode::AiProviderRequired));
        }
        Ok(selection)
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
        if command.connection_id == ANTHROPIC_CONNECTION_ID {
            let api_key = load_anthropic_api_key()
                .map_err(|_| TenderCommandError::new(TenderErrorCode::AiProviderRequired))?;
            let connection = inspect_anthropic_connection(&api_key)
                .await
                .map_err(|_| TenderCommandError::new(TenderErrorCode::AiProviderRequired))?;
            let selection = selection_from_command(&connection, &command)?;
            save_connection_and_selection(self.application_home(), &connection, &selection)?;
            return load_application_settings(self.application_home());
        }
        if command.connection_id == GEMINI_CONNECTION_ID {
            let api_key = load_gemini_api_key()
                .map_err(|_| TenderCommandError::new(TenderErrorCode::AiProviderRequired))?;
            let connection = inspect_gemini_connection(&api_key)
                .await
                .map_err(|_| TenderCommandError::new(TenderErrorCode::AiProviderRequired))?;
            let selection = selection_from_command(&connection, &command)?;
            save_connection_and_selection(self.application_home(), &connection, &selection)?;
            return load_application_settings(self.application_home());
        }
        if command.connection_id != CODEX_CONNECTION_ID {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let provider = self.agent_provider().lock().await.as_ref().cloned();
        let Some(provider) = provider else {
            self.invalidate_missing_provider()?;
            return Err(TenderCommandError::new(TenderErrorCode::AiProviderRequired));
        };
        let connection = provider.connection_snapshot();
        if connection.status != ProviderConnectionStatus::Ready {
            return Err(TenderCommandError::new(TenderErrorCode::AiProviderRequired));
        }
        let selection = selection_from_command(&connection, &command)?;
        save_connection_and_selection(self.application_home(), &connection, &selection)?;
        load_application_settings(self.application_home())
    }

    pub async fn confirm_ai_execution_selection(
        &self,
        command: ConfirmAiExecutionSelectionCommand,
    ) -> Result<ApplicationSettingsView, TenderCommandError> {
        let view = self.update_ai_execution_selection(command.into()).await?;
        let selection = view
            .ai_execution_selection
            .as_ref()
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::AiProviderRequired))?;
        let connection = view
            .provider_connections
            .iter()
            .find(|connection| connection.connection_id == selection.connection_id)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::AiProviderRequired))?;
        let approval = AiExecutionApproval {
            connection_id: selection.connection_id.clone(),
            provider: selection.provider,
            account_fingerprint: account_fingerprint(connection),
            model_id: selection.model_id.clone(),
            reasoning: selection.reasoning.clone(),
            data_destination: data_destination(connection.provider).to_owned(),
            approved_at: Timestamp::now().to_string(),
        };
        let mut database = settings_connection(self.application_home())?;
        let transaction = database
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(settings_store_error)?;
        let mut stored = load_stored_settings(&transaction)?;
        stored.ai_execution_approval = Some(approval);
        store_application_settings(&transaction, &stored)?;
        transaction.commit().map_err(settings_store_error)?;
        let view = load_application_settings(self.application_home())?;
        if let Some(selection) = view.ai_execution_selection.as_ref() {
            self.refresh_matching_tender_ai_bindings(&view, selection)?;
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

    pub async fn connect_anthropic(
        &self,
        command: ConnectAnthropicCommand,
    ) -> Result<ApplicationSettingsView, TenderCommandError> {
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        if !self.update_environment_is_quiescent() {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let api_key = command.api_key.trim().to_owned();
        if api_key.is_empty() {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let connection = inspect_anthropic_connection(&api_key)
            .await
            .map_err(|failure| match failure.category {
                ProviderFailureCategory::AuthenticationRequired => {
                    TenderCommandError::new(TenderErrorCode::InvalidCommand)
                }
                _ => TenderCommandError::new(TenderErrorCode::AiProviderRequired),
            })?;
        let previous = read_anthropic_api_key().ok();
        write_anthropic_api_key(&api_key)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        if let Err(error) = save_live_connection(self.application_home(), &connection) {
            match previous {
                Some(previous) => {
                    let _ = write_anthropic_api_key(&previous);
                }
                None => {
                    let _ = delete_anthropic_api_key();
                }
            }
            return Err(error);
        }
        load_application_settings(self.application_home())
    }

    pub async fn disconnect_ai_provider(
        &self,
        command: DisconnectAiProviderCommand,
    ) -> Result<ApplicationSettingsView, TenderCommandError> {
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        if !self.update_environment_is_quiescent() {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        match command.connection_id.as_str() {
            ANTHROPIC_CONNECTION_ID => {
                delete_anthropic_api_key()
                    .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
                save_anthropic_connection_status(
                    self.application_home(),
                    ProviderConnectionStatus::AuthenticationRequired,
                    "Add an Anthropic API key to connect. Revoke externally created keys in the Anthropic Console.",
                )?;
            }
            GEMINI_CONNECTION_ID => {
                delete_gemini_api_key()
                    .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
                save_gemini_connection_status(
                    self.application_home(),
                    ProviderConnectionStatus::AuthenticationRequired,
                    "Add a Gemini API key to connect. Revoke externally created keys in Google AI Studio or Google Cloud.",
                )?;
            }
            _ => return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand)),
        }
        clear_selection_for_connection(self.application_home(), &command.connection_id)?;
        load_application_settings(self.application_home())
    }

    pub async fn connect_gemini(
        &self,
        command: ConnectGeminiCommand,
    ) -> Result<ApplicationSettingsView, TenderCommandError> {
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        if !self.update_environment_is_quiescent() {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let api_key = command.api_key.trim().to_owned();
        if api_key.is_empty() {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let connection =
            inspect_gemini_connection(&api_key)
                .await
                .map_err(|failure| match failure.category {
                    ProviderFailureCategory::AuthenticationRequired => {
                        TenderCommandError::new(TenderErrorCode::InvalidCommand)
                    }
                    _ => TenderCommandError::new(TenderErrorCode::AiProviderRequired),
                })?;
        let previous = read_gemini_api_key().ok();
        write_gemini_api_key(&api_key)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        if let Err(error) = save_live_connection(self.application_home(), &connection) {
            match previous {
                Some(previous) => {
                    let _ = write_gemini_api_key(&previous);
                }
                None => {
                    let _ = delete_gemini_api_key();
                }
            }
            return Err(error);
        }
        load_application_settings(self.application_home())
    }

    pub async fn start_provider_login(
        &self,
        command: StartProviderLoginCommand,
    ) -> Result<ApplicationSettingsView, TenderCommandError> {
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        if !self.update_environment_is_quiescent() {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let provider = self
            .agent_provider()
            .lock()
            .await
            .as_ref()
            .filter(|provider| !provider.is_closed())
            .cloned()
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::AiProviderRequired))?;
        let login = match provider.start_login(command.method).await {
            Ok(login) => login,
            Err(failure) if failure.category == ProviderFailureCategory::PermissionDenied => {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            Err(failure) => {
                self.retire_failed_provider(&provider, &failure).await?;
                return Err(TenderCommandError::new(TenderErrorCode::AiProviderRequired));
            }
        };
        let mut view = load_application_settings(self.application_home())?;
        view.active_provider_login = Some(login);
        Ok(view)
    }

    pub async fn open_provider_login(
        &self,
        command: OpenProviderLoginCommand,
    ) -> Result<(), TenderCommandError> {
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let provider = self
            .agent_provider()
            .lock()
            .await
            .as_ref()
            .filter(|provider| !provider.is_closed())
            .cloned()
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::AiProviderRequired))?;
        let login = provider
            .login_snapshot()
            .filter(|login| {
                login.login_id == command.login_id
                    && login.status == ProviderLoginStatus::AwaitingUser
                    && valid_login_url(login.method, &login.authorization_url)
            })
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        tokio::task::spawn_blocking(move || webbrowser::open(&login.authorization_url))
            .await
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))
    }

    pub async fn cancel_provider_login(
        &self,
        command: CancelProviderLoginCommand,
    ) -> Result<ApplicationSettingsView, TenderCommandError> {
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let provider = self
            .agent_provider()
            .lock()
            .await
            .as_ref()
            .filter(|provider| !provider.is_closed())
            .cloned()
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::AiProviderRequired))?;
        match provider.cancel_login(command.login_id).await {
            Ok(()) => {}
            Err(failure) if failure.category == ProviderFailureCategory::PermissionDenied => {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            Err(failure) => {
                self.retire_failed_provider(&provider, &failure).await?;
                return Err(TenderCommandError::new(TenderErrorCode::AiProviderRequired));
            }
        }
        let mut view = load_application_settings(self.application_home())?;
        view.active_provider_login = provider.login_snapshot();
        Ok(view)
    }

    pub async fn logout_provider(&self) -> Result<ApplicationSettingsView, TenderCommandError> {
        if !self.update_environment_is_quiescent() {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let provider = self
            .agent_provider()
            .lock()
            .await
            .as_ref()
            .filter(|provider| !provider.is_closed())
            .cloned()
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::AiProviderRequired))?;
        let connection = match provider.logout().await {
            Ok(connection) => connection,
            Err(failure) if failure.category == ProviderFailureCategory::PermissionDenied => {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            Err(failure) => {
                self.retire_failed_provider(&provider, &failure).await?;
                return Err(TenderCommandError::new(TenderErrorCode::AiProviderRequired));
            }
        };
        save_live_connection(self.application_home(), &connection)?;
        load_application_settings(self.application_home())
    }

    async fn retire_failed_provider(
        &self,
        provider: &AgentProvider,
        failure: &ProviderFailure,
    ) -> Result<(), TenderCommandError> {
        let retired = {
            let mut provider_slot = self.agent_provider().lock().await;
            if provider_slot
                .as_ref()
                .is_some_and(|current| current.same_instance(provider))
            {
                *provider_slot = None;
                true
            } else {
                false
            }
        };
        if retired {
            let snapshot = provider.connection_snapshot();
            if matches!(
                snapshot.status,
                ProviderConnectionStatus::AuthenticationRequired
                    | ProviderConnectionStatus::SubscriptionRequired
                    | ProviderConnectionStatus::Incompatible
            ) {
                save_live_connection(self.application_home(), &snapshot)?;
            } else {
                let (status, summary) = codex_failure_connection_status(failure.category);
                save_codex_connection_status(self.application_home(), status, summary)?;
            }
        }
        Ok(())
    }

    fn invalidate_missing_provider(&self) -> Result<(), TenderCommandError> {
        let (status, summary) =
            codex_failure_connection_status(ProviderFailureCategory::ProcessFailed);
        save_codex_connection_status(self.application_home(), status, summary)
    }
}

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

fn selection_with_current_provenance(
    connection: &ProviderConnectionView,
    preferred: &AiExecutionSelection,
) -> Result<AiExecutionSelection, TenderCommandError> {
    if !selection_is_supported(connection, preferred) {
        return Err(TenderCommandError::new(TenderErrorCode::AiProviderRequired));
    }
    Ok(AiExecutionSelection {
        connection_id: connection.connection_id.clone(),
        provider: connection.provider,
        model_id: preferred.model_id.clone(),
        reasoning: preferred.reasoning.clone(),
        catalogue_fetched_at: connection
            .catalogue_fetched_at
            .clone()
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::AiProviderRequired))?,
        adapter_version: connection.adapter_version.clone(),
    })
}

pub(crate) fn save_live_connection(
    application_home: &Path,
    connection: &ProviderConnectionView,
) -> Result<(), TenderCommandError> {
    let mut database = settings_connection(application_home)?;
    let transaction = database
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(settings_store_error)?;
    let previous = transaction
        .query_row(
            "SELECT connection_json FROM provider_connections WHERE connection_id = ?1",
            [connection.connection_id.as_str()],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(settings_store_error)?
        .map(|raw| serde_json::from_str::<ProviderConnectionView>(&raw))
        .transpose()
        .map_err(settings_json_error)?;
    let mut persisted = connection.clone();
    // A provider outage often returns only a status and no live catalogue.
    // Retain the last verified catalogue and account identity so an explicit
    // approval can survive reconnection without silently rebinding elsewhere.
    if persisted.status != ProviderConnectionStatus::Ready && persisted.models.is_empty() {
        if let Some(previous) = previous {
            persisted.models = previous.models;
            persisted.catalogue_fetched_at = previous.catalogue_fetched_at;
            persisted.adapter_version = previous.adapter_version;
            persisted.account_label = previous.account_label;
            persisted.account_plan = previous.account_plan;
        }
    }
    upsert_connection(&transaction, &persisted)?;
    let settings = load_stored_settings(&transaction)?;
    match settings.ai_execution_selection {
        Some(selection) if selection_is_supported(&persisted, &selection) => {
            let rebound = AiExecutionSelection {
                catalogue_fetched_at: persisted
                    .catalogue_fetched_at
                    .clone()
                    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::AiProviderRequired))?,
                adapter_version: persisted.adapter_version.clone(),
                ..selection
            };
            store_selection(&transaction, &rebound)?;
        }
        Some(_) | None => {}
    }
    transaction.commit().map_err(settings_store_error)
}

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

fn save_connection_and_selection(
    application_home: &Path,
    connection: &ProviderConnectionView,
    selection: &AiExecutionSelection,
) -> Result<(), TenderCommandError> {
    let mut database = settings_connection(application_home)?;
    let transaction = database
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(settings_store_error)?;
    upsert_connection(&transaction, connection)?;
    let mut stored = load_stored_settings(&transaction)?;
    stored.ai_execution_selection = Some(selection.clone());
    stored.ai_execution_approval = None;
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
        AiProviderKind::Codex => "OpenAI Codex account",
        AiProviderKind::Anthropic => "Anthropic API account",
        AiProviderKind::Gemini => "Google Gemini API account",
    }
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

fn store_selection(
    transaction: &rusqlite::Transaction<'_>,
    selection: &AiExecutionSelection,
) -> Result<(), TenderCommandError> {
    let mut stored = load_stored_settings(transaction)?;
    stored.ai_execution_selection = Some(selection.clone());
    store_application_settings(transaction, &stored)
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
    let approval = AiExecutionApproval {
        connection_id: selection.connection_id.clone(),
        provider: selection.provider,
        account_fingerprint: account_fingerprint(&connection),
        model_id: selection.model_id.clone(),
        reasoning: selection.reasoning.clone(),
        data_destination: data_destination(connection.provider).to_owned(),
        approved_at: "2026-01-01T00:00:00Z".to_owned(),
    };
    let mut database = settings_connection(application_home)?;
    let transaction = database
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(settings_store_error)?;
    let stored = load_stored_settings(&transaction)?;
    if stored.ai_execution_selection.is_some() && stored.ai_execution_approval.is_some() {
        drop(transaction);
        return Ok(());
    }
    upsert_connection(&transaction, &connection)?;
    let mut stored = stored;
    stored.ai_execution_selection = Some(selection);
    stored.ai_execution_approval = Some(approval);
    store_application_settings(&transaction, &stored)?;
    transaction.commit().map_err(settings_store_error)
}

fn load_application_settings(
    application_home: &Path,
) -> Result<ApplicationSettingsView, TenderCommandError> {
    let database = settings_connection(application_home)?;
    let settings = load_stored_settings(&database)?;
    let mut statement = database
        .prepare(
            "SELECT connection_json FROM provider_connections
             ORDER BY provider_kind, connection_id",
        )
        .map_err(settings_store_error)?;
    let mut provider_connections = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(settings_store_error)?
        .map(|raw| {
            raw.map_err(settings_store_error)
                .and_then(|raw| serde_json::from_str(&raw).map_err(settings_json_error))
        })
        .collect::<Result<Vec<ProviderConnectionView>, TenderCommandError>>()?;
    if !provider_connections
        .iter()
        .any(|connection| connection.connection_id == ANTHROPIC_CONNECTION_ID)
    {
        provider_connections.push(anthropic_connection_without_catalog(
            ProviderConnectionStatus::AuthenticationRequired,
            "Add an Anthropic API key to connect.",
        ));
    }
    if !provider_connections
        .iter()
        .any(|connection| connection.connection_id == GEMINI_CONNECTION_ID)
    {
        provider_connections.push(gemini_connection_without_catalog(
            ProviderConnectionStatus::AuthenticationRequired,
            "Add a Gemini API key to connect.",
        ));
    }
    Ok(ApplicationSettingsView {
        general_preferences: settings.general_preferences,
        ai_execution_selection: settings.ai_execution_selection,
        ai_execution_approval: settings.ai_execution_approval,
        provider_connections,
        active_provider_login: None,
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

fn provider_failure_status(category: ProviderFailureCategory) -> ProviderConnectionStatus {
    match category {
        ProviderFailureCategory::AuthenticationRequired => {
            ProviderConnectionStatus::AuthenticationRequired
        }
        ProviderFailureCategory::ProtocolInvalid | ProviderFailureCategory::OutputInvalid => {
            ProviderConnectionStatus::Incompatible
        }
        _ => ProviderConnectionStatus::TemporarilyUnavailable,
    }
}

fn anthropic_connection_without_catalog(
    status: ProviderConnectionStatus,
    summary: &str,
) -> ProviderConnectionView {
    ProviderConnectionView {
        connection_id: ANTHROPIC_CONNECTION_ID.to_owned(),
        provider: AiProviderKind::Anthropic,
        display_name: "Anthropic API key".to_owned(),
        status,
        account_label: None,
        account_plan: None,
        models: Vec::new(),
        catalogue_fetched_at: None,
        adapter_version: ANTHROPIC_ADAPTER_VERSION.to_owned(),
        status_summary: summary.to_owned(),
    }
}

fn gemini_connection_without_catalog(
    status: ProviderConnectionStatus,
    summary: &str,
) -> ProviderConnectionView {
    ProviderConnectionView {
        connection_id: GEMINI_CONNECTION_ID.to_owned(),
        provider: AiProviderKind::Gemini,
        display_name: "Google Gemini API key".to_owned(),
        status,
        account_label: None,
        account_plan: None,
        models: Vec::new(),
        catalogue_fetched_at: None,
        adapter_version: GEMINI_ADAPTER_VERSION.to_owned(),
        status_summary: summary.to_owned(),
    }
}

fn save_anthropic_connection_status(
    application_home: &Path,
    status: ProviderConnectionStatus,
    summary: &str,
) -> Result<(), TenderCommandError> {
    let mut database = settings_connection(application_home)?;
    let transaction = database
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(settings_store_error)?;
    let connection = anthropic_connection_without_catalog(status, summary);
    upsert_connection(&transaction, &connection)?;
    transaction.commit().map_err(settings_store_error)
}

fn save_gemini_connection_status(
    application_home: &Path,
    status: ProviderConnectionStatus,
    summary: &str,
) -> Result<(), TenderCommandError> {
    let mut database = settings_connection(application_home)?;
    let transaction = database
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(settings_store_error)?;
    upsert_connection(
        &transaction,
        &gemini_connection_without_catalog(status, summary),
    )?;
    transaction.commit().map_err(settings_store_error)
}

fn clear_selection_for_connection(
    application_home: &Path,
    connection_id: &str,
) -> Result<(), TenderCommandError> {
    let mut database = settings_connection(application_home)?;
    let transaction = database
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(settings_store_error)?;
    let mut stored = load_stored_settings(&transaction)?;
    if stored
        .ai_execution_selection
        .as_ref()
        .is_some_and(|selection| selection.connection_id == connection_id)
    {
        stored.ai_execution_selection = None;
        store_application_settings(&transaction, &stored)?;
    }
    transaction.commit().map_err(settings_store_error)
}

fn anthropic_credential_entry() -> Result<keyring::v1::Entry, keyring::v1::Error> {
    keyring::v1::Entry::new(CREDENTIAL_SERVICE, ANTHROPIC_CREDENTIAL_ACCOUNT)
}

fn gemini_credential_entry() -> Result<keyring::v1::Entry, keyring::v1::Error> {
    keyring::v1::Entry::new(CREDENTIAL_SERVICE, GEMINI_CREDENTIAL_ACCOUNT)
}

fn read_anthropic_api_key() -> Result<String, keyring::v1::Error> {
    anthropic_credential_entry()?.get_password()
}

fn write_anthropic_api_key(api_key: &str) -> Result<(), keyring::v1::Error> {
    anthropic_credential_entry()?.set_password(api_key)
}

fn delete_anthropic_api_key() -> Result<(), keyring::v1::Error> {
    match anthropic_credential_entry()?.delete_credential() {
        Ok(()) | Err(keyring::v1::Error::NoEntry) => Ok(()),
        Err(error) => Err(error),
    }
}

fn read_gemini_api_key() -> Result<String, keyring::v1::Error> {
    gemini_credential_entry()?.get_password()
}

fn write_gemini_api_key(api_key: &str) -> Result<(), keyring::v1::Error> {
    gemini_credential_entry()?.set_password(api_key)
}

fn delete_gemini_api_key() -> Result<(), keyring::v1::Error> {
    match gemini_credential_entry()?.delete_credential() {
        Ok(()) | Err(keyring::v1::Error::NoEntry) => Ok(()),
        Err(error) => Err(error),
    }
}

pub(crate) fn load_anthropic_api_key() -> Result<String, ProviderFailure> {
    match read_anthropic_api_key() {
        Ok(api_key) if !api_key.trim().is_empty() => Ok(api_key),
        Ok(_) | Err(keyring::v1::Error::NoEntry) => Err(ProviderFailure::new(
            ProviderFailureCategory::AuthenticationRequired,
            true,
            "Add an Anthropic API key in Settings before retrying.",
            Some("No Anthropic credential is available in the system credential vault."),
        )),
        Err(_) => Err(ProviderFailure::new(
            ProviderFailureCategory::ProcessFailed,
            true,
            "Unlock or repair the operating-system credential vault before retrying.",
            Some("Quantix could not read the Anthropic credential from the system vault."),
        )),
    }
}

pub(crate) fn load_gemini_api_key() -> Result<String, ProviderFailure> {
    match read_gemini_api_key() {
        Ok(api_key) if !api_key.trim().is_empty() => Ok(api_key),
        Ok(_) | Err(keyring::v1::Error::NoEntry) => Err(ProviderFailure::new(
            ProviderFailureCategory::AuthenticationRequired,
            true,
            "Add a Gemini API key in Settings before retrying.",
            Some("No Gemini credential is available in the system credential vault."),
        )),
        Err(_) => Err(ProviderFailure::new(
            ProviderFailureCategory::ProcessFailed,
            true,
            "Unlock or repair the operating-system credential vault before retrying.",
            Some("Quantix could not read the Gemini credential from the system vault."),
        )),
    }
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

pub(crate) fn codex_connection_version() -> String {
    CODEX_VERSION.to_owned()
}
