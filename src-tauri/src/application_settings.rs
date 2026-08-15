use std::{path::Path, time::Duration};

use garde::Validate;
use rusqlite::{params, Connection, OpenFlags, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
    agent_runtime::{
        valid_login_url, AgentProvider, ProviderFailure, ProviderFailureCategory, CODEX_VERSION,
    },
    tender_store::{TenderCommandError, TenderErrorCode},
    QuantixHost,
};

pub(crate) const CODEX_CONNECTION_ID: &str = "codex_chatgpt";

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct ApplicationSettingsView {
    pub ai_execution_selection: Option<AiExecutionSelection>,
    pub provider_connections: Vec<ProviderConnectionView>,
    pub active_provider_login: Option<ProviderLoginView>,
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
    ai_execution_selection: Option<AiExecutionSelection>,
}

impl QuantixHost {
    pub async fn refresh_application_settings(
        &self,
    ) -> Result<ApplicationSettingsView, TenderCommandError> {
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
                if connection.status == ProviderConnectionStatus::Ready {
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
        self.set_runtime_verified(connection.status == ProviderConnectionStatus::Ready);
        let mut view = load_application_settings(self.application_home())?;
        view.active_provider_login = provider.login_snapshot();
        Ok(view)
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
        self.set_runtime_verified(false);
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
            self.set_runtime_verified(false);
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
        self.set_runtime_verified(false);
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
            let selection = default_selection(connection)
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::RuntimeRequired))?;
            store_selection(&transaction, &selection)?;
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
    let stored = StoredApplicationSettings {
        ai_execution_selection: Some(selection.clone()),
    };
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
    let provider_connections = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(settings_store_error)?
        .map(|raw| {
            raw.map_err(settings_store_error)
                .and_then(|raw| serde_json::from_str(&raw).map_err(settings_json_error))
        })
        .collect::<Result<Vec<ProviderConnectionView>, TenderCommandError>>()?;
    Ok(ApplicationSettingsView {
        ai_execution_selection: settings.ai_execution_selection,
        provider_connections,
        active_provider_login: None,
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

pub(crate) fn codex_connection_version() -> String {
    CODEX_VERSION.to_owned()
}
