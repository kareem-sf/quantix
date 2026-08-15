use std::{path::Path, time::Duration};

use garde::Validate;
use rusqlite::{params, Connection, OpenFlags, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
    agent_runtime::{
        inspect_anthropic_connection, inspect_gemini_connection, valid_login_url, AgentProvider,
        ProviderFailure, ProviderFailureCategory, CODEX_VERSION,
    },
    setup::INSTALLATION_SCHEMA_VERSION,
    tender_store::{TenderCommandError, TenderErrorCode, TENDER_SCHEMA_VERSION},
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
    pub high_contrast: bool,
    pub larger_text: bool,
    pub notify_when_attention_needed: bool,
}

impl Default for GeneralApplicationPreferences {
    fn default() -> Self {
        Self {
            appearance: AppearancePreference::System,
            reduced_motion: false,
            high_contrast: false,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredApplicationSettings {
    general_preferences: GeneralApplicationPreferences,
    ai_execution_selection: Option<AiExecutionSelection>,
}

impl QuantixHost {
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
            if self.runtime_is_verified() {
                self.invalidate_missing_provider()?;
            }
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
            self.set_runtime_verified(
                view.provider_connections
                    .iter()
                    .any(|connection| connection.status == ProviderConnectionStatus::Ready),
            );
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
        let ready = connection.status == ProviderConnectionStatus::Ready
            || load_application_settings(self.application_home())?
                .provider_connections
                .iter()
                .any(|candidate| {
                    candidate.connection_id != CODEX_CONNECTION_ID
                        && candidate.status == ProviderConnectionStatus::Ready
                });
        self.set_runtime_verified(ready);
        let mut view = load_application_settings(self.application_home())?;
        view.active_provider_login = provider.login_snapshot();
        Ok(view)
    }

    pub(crate) async fn refresh_exact_ai_execution_selection(
        &self,
        preferred: Option<&AiExecutionSelection>,
    ) -> Result<Option<AiExecutionSelection>, TenderCommandError> {
        #[cfg(any(test, feature = "runtime-fixture"))]
        if self.runtime_is_verified() {
            if let Some(preferred) = preferred {
                return Ok(Some(preferred.clone()));
            }
            return load_current_ai_execution_selection(self.application_home()).map(Some);
        }

        let view = self.refresh_application_settings().await?;
        let preferred = preferred
            .cloned()
            .or_else(|| view.ai_execution_selection.clone());
        let Some(preferred) = preferred else {
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
        Ok(Some(selection_with_current_provenance(
            connection, &preferred,
        )?))
    }

    pub(crate) async fn require_current_live_ai_selection(
        &self,
    ) -> Result<AiExecutionSelection, TenderCommandError> {
        self.refresh_exact_ai_execution_selection(None)
            .await?
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::RuntimeRequired))
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
                .map_err(|_| TenderCommandError::new(TenderErrorCode::RuntimeRequired))?;
            let connection = inspect_anthropic_connection(&api_key)
                .await
                .map_err(|_| TenderCommandError::new(TenderErrorCode::RuntimeRequired))?;
            let selection = selection_from_command(&connection, &command)?;
            save_connection_and_selection(self.application_home(), &connection, &selection)?;
            self.set_runtime_verified(true);
            return load_application_settings(self.application_home());
        }
        if command.connection_id == GEMINI_CONNECTION_ID {
            let api_key = load_gemini_api_key()
                .map_err(|_| TenderCommandError::new(TenderErrorCode::RuntimeRequired))?;
            let connection = inspect_gemini_connection(&api_key)
                .await
                .map_err(|_| TenderCommandError::new(TenderErrorCode::RuntimeRequired))?;
            let selection = selection_from_command(&connection, &command)?;
            save_connection_and_selection(self.application_home(), &connection, &selection)?;
            self.set_runtime_verified(true);
            return load_application_settings(self.application_home());
        }
        if command.connection_id != CODEX_CONNECTION_ID {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        self.require_runtime_verified()?;
        let provider = self.agent_provider().lock().await.as_ref().cloned();
        let Some(provider) = provider else {
            self.invalidate_missing_provider()?;
            return Err(TenderCommandError::new(TenderErrorCode::RuntimeRequired));
        };
        let refreshed = match provider.refresh_readiness().await {
            Ok(refreshed) => refreshed,
            Err(failure) => {
                self.retire_failed_provider(&provider, &failure).await?;
                return Err(TenderCommandError::new(TenderErrorCode::RuntimeRequired));
            }
        };
        if !refreshed {
            return Err(TenderCommandError::new(TenderErrorCode::RuntimeRequired));
        }
        let connection = provider.connection_snapshot();
        let selection = selection_from_command(&connection, &command)?;
        save_connection_and_selection(self.application_home(), &connection, &selection)?;
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
                _ => TenderCommandError::new(TenderErrorCode::RuntimeRequired),
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
        self.set_runtime_verified(true);
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
        let view = load_application_settings(self.application_home())?;
        self.set_runtime_verified(
            view.provider_connections
                .iter()
                .any(|connection| connection.status == ProviderConnectionStatus::Ready),
        );
        Ok(view)
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
                    _ => TenderCommandError::new(TenderErrorCode::RuntimeRequired),
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
        self.set_runtime_verified(true);
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
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::RuntimeRequired))?;
        let login = match provider.start_login(command.method).await {
            Ok(login) => login,
            Err(failure) if failure.category == ProviderFailureCategory::PermissionDenied => {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            Err(failure) => {
                self.retire_failed_provider(&provider, &failure).await?;
                return Err(TenderCommandError::new(TenderErrorCode::RuntimeRequired));
            }
        };
        self.set_runtime_verified(false);
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
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::RuntimeRequired))?;
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
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::RuntimeRequired))?;
        match provider.cancel_login(command.login_id).await {
            Ok(()) => {}
            Err(failure) if failure.category == ProviderFailureCategory::PermissionDenied => {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            Err(failure) => {
                self.retire_failed_provider(&provider, &failure).await?;
                return Err(TenderCommandError::new(TenderErrorCode::RuntimeRequired));
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
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::RuntimeRequired))?;
        let connection = match provider.logout().await {
            Ok(connection) => connection,
            Err(failure) if failure.category == ProviderFailureCategory::PermissionDenied => {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            Err(failure) => {
                self.retire_failed_provider(&provider, &failure).await?;
                return Err(TenderCommandError::new(TenderErrorCode::RuntimeRequired));
            }
        };
        save_live_connection(self.application_home(), &connection)?;
        let view = load_application_settings(self.application_home())?;
        self.set_runtime_verified(
            view.provider_connections
                .iter()
                .any(|candidate| candidate.status == ProviderConnectionStatus::Ready),
        );
        Ok(view)
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
            let view = load_application_settings(self.application_home())?;
            self.set_runtime_verified(
                view.provider_connections
                    .iter()
                    .any(|connection| connection.status == ProviderConnectionStatus::Ready),
            );
        }
        Ok(())
    }

    fn invalidate_missing_provider(&self) -> Result<(), TenderCommandError> {
        let (status, summary) =
            codex_failure_connection_status(ProviderFailureCategory::ProcessFailed);
        save_codex_connection_status(self.application_home(), status, summary)?;
        let view = load_application_settings(self.application_home())?;
        self.set_runtime_verified(
            view.provider_connections
                .iter()
                .any(|connection| connection.status == ProviderConnectionStatus::Ready),
        );
        Ok(())
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

pub(crate) fn load_current_ai_execution_selection(
    application_home: &Path,
) -> Result<AiExecutionSelection, TenderCommandError> {
    let view = load_application_settings(application_home)?;
    if let Some(selection) = view.ai_execution_selection {
        let current = view.provider_connections.iter().any(|connection| {
            selection_is_supported(connection, &selection)
                && connection.catalogue_fetched_at.as_deref()
                    == Some(selection.catalogue_fetched_at.as_str())
                && connection.adapter_version == selection.adapter_version
        });
        if current {
            return Ok(selection);
        }
    }
    #[cfg(any(test, feature = "runtime-fixture"))]
    {
        Ok(AiExecutionSelection {
            connection_id: CODEX_CONNECTION_ID.to_owned(),
            provider: AiProviderKind::Codex,
            model_id: "gpt-5.6-terra".to_owned(),
            reasoning: ProviderReasoningSelection::CodexEffort("medium".to_owned()),
            catalogue_fetched_at: "2026-01-01T00:00:00Z".to_owned(),
            adapter_version: format!("{}-fixture", CODEX_VERSION),
        })
    }
    #[cfg(not(any(test, feature = "runtime-fixture")))]
    Err(TenderCommandError::new(TenderErrorCode::RuntimeRequired))
}

pub(crate) fn load_preferred_ai_execution_selection(
    application_home: &Path,
) -> Result<Option<AiExecutionSelection>, TenderCommandError> {
    let selection = load_application_settings(application_home)?.ai_execution_selection;
    #[cfg(any(test, feature = "runtime-fixture"))]
    if selection.is_none() {
        return load_current_ai_execution_selection(application_home).map(Some);
    }
    Ok(selection)
}

fn selection_from_command(
    connection: &ProviderConnectionView,
    command: &UpdateAiExecutionSelectionCommand,
) -> Result<AiExecutionSelection, TenderCommandError> {
    if connection.status != ProviderConnectionStatus::Ready
        || connection.connection_id != command.connection_id
    {
        return Err(TenderCommandError::new(TenderErrorCode::RuntimeRequired));
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
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::RuntimeRequired))?,
        adapter_version: connection.adapter_version.clone(),
    })
}

fn selection_with_current_provenance(
    connection: &ProviderConnectionView,
    preferred: &AiExecutionSelection,
) -> Result<AiExecutionSelection, TenderCommandError> {
    if !selection_is_supported(connection, preferred) {
        return Err(TenderCommandError::new(TenderErrorCode::RuntimeRequired));
    }
    Ok(AiExecutionSelection {
        connection_id: connection.connection_id.clone(),
        provider: connection.provider,
        model_id: preferred.model_id.clone(),
        reasoning: preferred.reasoning.clone(),
        catalogue_fetched_at: connection
            .catalogue_fetched_at
            .clone()
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::RuntimeRequired))?,
        adapter_version: connection.adapter_version.clone(),
    })
}

fn default_selection(connection: &ProviderConnectionView) -> Option<AiExecutionSelection> {
    let model = connection.models.iter().find(|model| model.is_default)?;
    let reasoning = model
        .reasoning_options
        .iter()
        .find(|option| option.is_default)?
        .selection
        .clone();
    Some(AiExecutionSelection {
        connection_id: connection.connection_id.clone(),
        provider: connection.provider,
        model_id: model.model_id.clone(),
        reasoning,
        catalogue_fetched_at: connection.catalogue_fetched_at.clone()?,
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
    upsert_connection(&transaction, connection)?;
    let settings = load_stored_settings(&transaction)?;
    match settings.ai_execution_selection {
        Some(selection) if selection_is_supported(connection, &selection) => {
            let rebound = AiExecutionSelection {
                catalogue_fetched_at: connection
                    .catalogue_fetched_at
                    .clone()
                    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::RuntimeRequired))?,
                adapter_version: connection.adapter_version.clone(),
                ..selection
            };
            store_selection(&transaction, &rebound)?;
        }
        Some(_) => {}
        None if connection.status == ProviderConnectionStatus::Ready => {
            if let Some(selection) = default_selection(connection) {
                store_selection(&transaction, &selection)?;
            }
        }
        None => {}
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
    store_selection(&transaction, selection)?;
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
