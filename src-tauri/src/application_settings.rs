use std::{fs, path::Path, time::Duration};

use garde::Validate;
use jiff::Timestamp;
use rusqlite::{params, Connection, OpenFlags, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ts_rs::TS;

use crate::{
    agent_runtime::ProviderFailureCategory,
    setup::INSTALLATION_SCHEMA_VERSION,
    tender_store::{TenderCommandError, TenderErrorCode, TenderId, TENDER_SCHEMA_VERSION},
    QuantixHost,
};

pub(crate) const CODEX_CONNECTION_ID: &str = "codex_chatgpt";
/// A model provider has no catalogue Quantix fetched and no adapter it downloaded, so
/// both fields record the lane rather than a version that would imply otherwise.
const MODEL_PROVIDER_CATALOGUE: &str = "engineer-configured-v1";
const MODEL_PROVIDER_ADAPTER: &str = "quantix-ai-worker-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum AiProviderKind {
    Codex,
    /// A connection the Engineer configured themselves, executed through the AI
    /// worker rather than the Codex app server. Its endpoint, model and key live in
    /// the Application Home's `.env`, keyed by the selection's connection id.
    ModelProvider,
}

impl AiProviderKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::ModelProvider => "model_provider",
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
    /// Persisted as `codex_effort` before the provider-neutral rename in 162dfac. The
    /// tag is part of the on-disk settings format, so the old spelling has to keep
    /// parsing or every installation written before that commit fails to load its
    /// settings — which fails every connection save and leaves the UI with nothing to
    /// show. Serialization always uses the current name, so stored rows heal on the
    /// next write.
    #[serde(alias = "codex_effort")]
    Effort(String),
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
        if cfg!(not(test)) {
            self.inspect_codex_subscription(tokio_util::sync::CancellationToken::new())
                .await;
        }
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
            mutate_ai_execution_selection_projection(&application_home, &command, false)
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
            mutate_ai_execution_selection_projection(&application_home, &update_command, true)
        })
        .await
        .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))??;
        if let Some(selection) = view.ai_execution_selection.clone() {
            self.refresh_matching_tender_ai_bindings(&view, &selection)?;
        }
        load_application_settings(self.application_home())
    }

    #[cfg(any(test, feature = "runtime-fixture"))]
    pub fn approve_runtime_fixture_ai_selection(&self) -> Result<(), TenderCommandError> {
        approve_runtime_fixture_ai_selection_projection(self.application_home())
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

fn codex_connection_failure_view(category: ProviderFailureCategory) -> ProviderConnectionView {
    let (status, status_summary) = match category {
        ProviderFailureCategory::SubscriptionRequired => (
            ProviderConnectionStatus::SubscriptionRequired,
            "The connected OpenAI account does not provide an eligible Codex subscription.",
        ),
        ProviderFailureCategory::AuthenticationRequired => (
            ProviderConnectionStatus::AuthenticationRequired,
            "Connect an OpenAI account to use Codex intelligence.",
        ),
        ProviderFailureCategory::ProtocolInvalid => (
            ProviderConnectionStatus::Incompatible,
            "The installed Codex runtime is incompatible with Quantix.",
        ),
        _ => (
            ProviderConnectionStatus::TemporarilyUnavailable,
            "Codex is temporarily unavailable. Tender records remain accessible.",
        ),
    };
    ProviderConnectionView {
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
    }
}

pub(crate) fn save_codex_disconnected(application_home: &Path) -> Result<(), TenderCommandError> {
    let mut database = settings_connection(application_home)?;
    let transaction = database
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(settings_store_error)?;
    upsert_connection(
        &transaction,
        &codex_connection_failure_view(ProviderFailureCategory::AuthenticationRequired),
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

fn refresh_exact_ai_execution_selection_projection(
    application_home: &Path,
    preferred: Option<&AiExecutionSelection>,
) -> Result<Option<AiExecutionSelection>, TenderCommandError> {
    let view = load_application_settings(application_home)?;
    let Some(selection) = preferred.or(view.ai_execution_selection.as_ref()) else {
        return Ok(None);
    };
    let Some(connection) = view
        .provider_connections
        .iter()
        .find(|connection| connection.connection_id == selection.connection_id)
    else {
        return Ok(None);
    };
    exact_approved_selection_unlocked(application_home, connection, Some(selection))
}

fn mutate_ai_execution_selection_projection(
    application_home: &Path,
    command: &UpdateAiExecutionSelectionCommand,
    confirm: bool,
) -> Result<ApplicationSettingsView, TenderCommandError> {
    let view = load_application_settings(application_home)?;
    let connection = view
        .provider_connections
        .iter()
        .find(|connection| {
            connection.connection_id == command.connection_id
                && connection.status == ProviderConnectionStatus::Ready
        })
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::AiProviderRequired))?;
    let selection = selection_from_command(connection, command)?;
    let approval = confirm.then(|| AiExecutionApproval {
        connection_id: selection.connection_id.clone(),
        provider: selection.provider,
        account_fingerprint: account_fingerprint(connection),
        model_id: selection.model_id.clone(),
        reasoning: selection.reasoning.clone(),
        data_destination: data_destination(connection.provider).to_owned(),
        approved_at: Timestamp::now().to_string(),
    });
    save_connection_and_selection_unlocked(application_home, connection, &selection, approval)?;
    load_application_settings(application_home)
}

/// Point newly created Tenders at a connection the Engineer configured themselves.
///
/// This deliberately does not go through the Codex selection path: that one validates
/// against a provider catalogue Quantix fetches, and a model provider has no catalogue
/// beyond the model id the Engineer typed. The approval is recorded here for the same
/// reason it is recorded there — so a later change of endpoint or model stops runs
/// until the Engineer agrees to the new destination.
pub(crate) fn select_model_provider(
    application_home: &Path,
    connection_id: &str,
    model_id: &str,
    data_destination: &str,
    account_fingerprint: &str,
) -> Result<(), TenderCommandError> {
    let mut database = settings_connection(application_home)?;
    let transaction = database
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(settings_store_error)?;
    let mut settings = load_stored_settings(&transaction)?;
    let selection = AiExecutionSelection {
        connection_id: connection_id.to_owned(),
        provider: AiProviderKind::ModelProvider,
        model_id: model_id.to_owned(),
        // The endpoint decides its own effort; Quantix does not send one, because
        // several routes reject the setting outright.
        reasoning: ProviderReasoningSelection::ProviderDefault,
        catalogue_fetched_at: MODEL_PROVIDER_CATALOGUE.to_owned(),
        adapter_version: MODEL_PROVIDER_ADAPTER.to_owned(),
    };
    settings.ai_execution_approval = Some(AiExecutionApproval {
        connection_id: selection.connection_id.clone(),
        provider: selection.provider,
        account_fingerprint: account_fingerprint.to_owned(),
        model_id: selection.model_id.clone(),
        reasoning: selection.reasoning.clone(),
        data_destination: data_destination.to_owned(),
        approved_at: Timestamp::now().to_string(),
    });
    settings.ai_execution_selection = Some(selection);
    store_application_settings(&transaction, &settings)?;
    transaction.commit().map_err(settings_store_error)
}

pub(crate) fn save_live_connection(
    application_home: &Path,
    connection: &ProviderConnectionView,
) -> Result<(), TenderCommandError> {
    save_live_connection_unlocked(application_home, connection)
}

pub(crate) fn load_codex_connection_view(
    application_home: &Path,
) -> Option<ProviderConnectionView> {
    let database = settings_connection(application_home).ok()?;
    let raw = database
        .query_row(
            "SELECT connection_json FROM provider_connections WHERE connection_id = ?1",
            [CODEX_CONNECTION_ID],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .ok()?
        .or(None)?;
    serde_json::from_str(&raw).ok()
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
    // This saves one connection's live view. A selection pointing at a different
    // connection cannot be judged against it, so leave that approval alone rather than
    // revoking it every time the Codex connection refreshes.
    let selection_targets_this_connection = settings
        .ai_execution_selection
        .as_ref()
        .is_some_and(|selection| selection.connection_id == persisted.connection_id);
    let approval_is_current = !selection_targets_this_connection
        || (had_selection
            && settings
                .ai_execution_selection
                .as_ref()
                .zip(settings.ai_execution_approval.as_ref())
                .is_some_and(|(selection, approval)| {
                    selection_is_supported(&persisted, selection)
                        && approval_matches(&persisted, selection, approval)
                }));
    if settings.ai_execution_approval.is_some() && !approval_is_current {
        settings.ai_execution_approval = None;
    }
    store_application_settings(&transaction, &settings)?;
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
        // The exact endpoint is part of the connection, not the kind, so the approval
        // records the class of destination the Engineer agreed to.
        AiProviderKind::ModelProvider => "Configured model provider",
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
    let (connection, selection) = runtime_fixture_connection_and_selection();
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

#[cfg(any(test, feature = "runtime-fixture"))]
fn runtime_fixture_connection_and_selection() -> (ProviderConnectionView, AiExecutionSelection) {
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
                selection: ProviderReasoningSelection::Effort("medium".to_owned()),
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
        reasoning: ProviderReasoningSelection::Effort("medium".to_owned()),
        catalogue_fetched_at: "fixture-catalogue".to_owned(),
        adapter_version: connection.adapter_version.clone(),
    };
    (connection, selection)
}

#[cfg(any(test, feature = "runtime-fixture"))]
fn approve_runtime_fixture_ai_selection_projection(
    application_home: &Path,
) -> Result<(), TenderCommandError> {
    let (connection, selection) = runtime_fixture_connection_and_selection();
    let approval = AiExecutionApproval {
        connection_id: selection.connection_id.clone(),
        provider: selection.provider,
        account_fingerprint: account_fingerprint(&connection),
        model_id: selection.model_id.clone(),
        reasoning: selection.reasoning.clone(),
        data_destination: data_destination(connection.provider).to_owned(),
        approved_at: Timestamp::now().to_string(),
    };
    save_connection_and_selection_unlocked(
        application_home,
        &connection,
        &selection,
        Some(approval),
    )
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
        provider_connections.push(codex_connection_failure_view(
            ProviderFailureCategory::AuthenticationRequired,
        ));
    }
    Ok(ApplicationSettingsView {
        general_preferences: settings.general_preferences,
        ai_execution_selection: settings.ai_execution_selection,
        ai_execution_approval: settings.ai_execution_approval,
        chatgpt: crate::chatgpt_login::chatgpt_connection_status_from_view(
            &provider_connections.first().cloned(),
            crate::chatgpt_login::ChatGptLoginPhase::Idle,
        ),
        provider_connections,
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

pub(crate) fn codex_connection_version() -> String {
    crate::agent_runtime::CODEX_VERSION.to_owned()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn temp_home(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("quantix-settings-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn fixture_connection(account_id: &str) -> ProviderConnectionView {
        ProviderConnectionView {
            connection_id: CODEX_CONNECTION_ID.to_owned(),
            provider: AiProviderKind::Codex,
            display_name: "OpenAI account via Codex".to_owned(),
            status: ProviderConnectionStatus::Ready,
            account_label: Some(account_id.to_owned()),
            account_plan: Some("plus".to_owned()),
            models: vec![ProviderModelOption {
                model_id: "gpt-5.6-terra".to_owned(),
                display_name: "GPT-5.6 Terra".to_owned(),
                description: "Fixture Codex model".to_owned(),
                is_default: true,
                input_modalities: vec!["text".to_owned()],
                reasoning_options: vec![ProviderReasoningOption {
                    selection: ProviderReasoningSelection::Effort("medium".to_owned()),
                    label: "medium".to_owned(),
                    description: "Fixture reasoning effort".to_owned(),
                    is_default: true,
                }],
            }],
            catalogue_fetched_at: Some("2026-08-30T00:00:00Z".to_owned()),
            adapter_version: codex_connection_version(),
            status_summary: "Ready to run Tender work.".to_owned(),
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
    fn codex_connection_status_follows_the_persisted_connection_view() {
        let home = initialized_home("status-derivation");
        let view = load_application_settings(&home).unwrap();
        assert_eq!(
            view.chatgpt.state,
            crate::chatgpt_login::ChatGptConnectionState::Absent
        );
        assert!(view.ai_execution_selection.is_none());

        save_live_connection(&home, &fixture_connection("account-77")).unwrap();
        let view = load_application_settings(&home).unwrap();
        assert_eq!(
            view.chatgpt.state,
            crate::chatgpt_login::ChatGptConnectionState::Connected
        );
        assert_eq!(view.chatgpt.account_id.as_deref(), Some("account-77"));
        assert_eq!(view.chatgpt.plan_type.as_deref(), Some("plus"));
        assert!(view.ai_execution_selection.is_some());
        assert_eq!(
            view.chatgpt.login_phase,
            crate::chatgpt_login::ChatGptLoginPhase::Idle
        );

        save_codex_disconnected(&home).unwrap();
        let view = load_application_settings(&home).unwrap();
        assert_eq!(
            view.chatgpt.state,
            crate::chatgpt_login::ChatGptConnectionState::Absent
        );
        assert!(view.ai_execution_selection.is_none());
        assert!(view.ai_execution_approval.is_none());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn live_connection_account_change_invalidates_execution_approval() {
        let home = initialized_home("approval-account-change");
        let original = fixture_connection("account-a");
        let selection = store_test_approval(&home, &original, "ChatGPT subscription");
        assert!(load_application_settings(&home)
            .unwrap()
            .ai_execution_approval
            .is_some());

        save_live_connection(&home, &fixture_connection("account-b")).unwrap();

        let view = load_application_settings(&home).unwrap();
        assert_eq!(view.ai_execution_selection.as_ref(), Some(&selection));
        assert!(view.ai_execution_approval.is_none());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn live_connection_data_destination_mismatch_invalidates_execution_approval() {
        let home = initialized_home("approval-destination-change");
        let connection = fixture_connection("account-a");
        store_test_approval(&home, &connection, "A different destination");

        save_live_connection(&home, &connection).unwrap();

        assert!(load_application_settings(&home)
            .unwrap()
            .ai_execution_approval
            .is_none());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn settings_written_before_the_reasoning_rename_still_load() {
        // Verbatim shape of an installation written before 162dfac renamed
        // CodexEffort to Effort. Losing this compatibility silently breaks every
        // pre-existing installation: the settings row stops parsing, so saving a
        // provider connection fails and Settings never receives one to display.
        let stored = r#"{
            "general_preferences": {
                "appearance": "system",
                "reduced_motion": false,
                "larger_text": false,
                "notify_when_attention_needed": true
            },
            "ai_execution_selection": {
                "connection_id": "codex_chatgpt",
                "provider": "codex",
                "model_id": "gpt-5.3-codex-spark",
                "reasoning": { "kind": "codex_effort", "value": "low" },
                "catalogue_fetched_at": "chatgpt-direct-v1",
                "adapter_version": "chatgpt-direct-v1"
            },
            "ai_execution_approval": {
                "connection_id": "codex_chatgpt",
                "provider": "codex",
                "account_fingerprint": "967e4e50",
                "model_id": "gpt-5.3-codex-spark",
                "reasoning": { "kind": "codex_effort", "value": "low" },
                "data_destination": "ChatGPT subscription",
                "approved_at": "2026-08-23T03:39:43.1440419Z"
            }
        }"#;

        let settings: StoredApplicationSettings =
            serde_json::from_str(stored).expect("settings predating the rename still parse");
        assert_eq!(
            settings.ai_execution_selection.unwrap().reasoning,
            ProviderReasoningSelection::Effort("low".to_owned())
        );
        assert_eq!(
            settings.ai_execution_approval.unwrap().reasoning,
            ProviderReasoningSelection::Effort("low".to_owned())
        );
    }

    #[test]
    fn reasoning_selection_serializes_under_the_current_tag() {
        // The alias is read-only compatibility; new writes must use the current name.
        let json = serde_json::to_string(&ProviderReasoningSelection::Effort("high".to_owned()))
            .expect("reasoning selection serializes");
        assert_eq!(json, r#"{"kind":"effort","value":"high"}"#);
    }

    #[test]
    fn codex_catalogue_is_versioned_by_the_pinned_codex_release() {
        let connection = fixture_connection("account-77");
        assert_eq!(
            connection.adapter_version,
            crate::agent_runtime::CODEX_VERSION
        );
        assert_eq!(connection.models.len(), 1);
        assert!(connection.models[0].is_default);
    }
}
