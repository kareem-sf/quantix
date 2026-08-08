mod agent_runtime;
mod document_parsing;
mod host;
mod process_supervisor;
mod runtime_readiness;
mod setup;
mod tender_intake;
mod tender_store;

pub use agent_runtime::{
    approve_one_run_access, AccessApproval, AccessRequest, AgentAccessRequestStatus,
    AgentAccessRequestView, AgentAccessResolution, AgentProfileVersionView, AgentResourceBudget,
    AgentRunInspection, AgentRunPermissions, AgentRunState, AgentRunWorkspaceManifest,
    AgentTaskInputReference, ApproveAgentAccessCommand, DataClassification, DataViewManifest,
    InterruptAgentRunCommand, OneRunAccessGrant, PermissionCeiling, PermissionDenialReason,
    PermissionGrant, ProposedAgentResult, ProviderEvent, ProviderEventKind, ProviderFailure,
    ProviderFailureCategory, ProviderRateLimit, ProviderRateLimitState, ProviderRateLimitWindow,
    ProviderUsage, RequestAgentAccessCommand, ResolveAgentAccessCommand, RunBootstrapAgentCommand,
    TenderTaskView, ThreadExposureSet, ToolIdempotency, ToolSideEffectClass, TypedToolDefinition,
    TypedToolQuota, VerificationStatus,
};
pub use document_parsing::{
    DocumentParseResult, EvidenceBoundingBox, EvidenceDocument, EvidenceLanguage, EvidenceLocation,
    EvidenceLocationKind, EvidenceRegion, EvidenceSearchHit, EvidenceSearchResult,
    ParseExceptionCode, ParseSourceArtifactCommand, ParseState, SearchEvidenceCommand,
    TextDirection,
};
pub use host::QuantixHost;
pub use runtime_readiness::{
    RuntimeLayout, RuntimeReadiness, RuntimeReadinessIssue, RuntimeReadinessState,
};
pub use setup::{
    ensure_quantix_setup, DeviceProtection, SetupIssue, SetupOutcome, SetupPlatform, SetupState,
    StoragePermissions, MINIMUM_SETUP_FREE_SPACE_BYTES,
};
pub use tender_intake::{
    ChooseTenderPackageCommand, ConfirmSourceRelationshipCommand, DocumentRegister,
    DocumentRegisterEntry, ImportTenderPackageCommand, IntakeExceptionCode, RegistrationState,
    SourceRelationshipKind, SupersessionState, TenderPackageImportResult, TenderPackageSourceKind,
};
pub use tender_store::{
    ContentVersionSummary, CreateTenderCommand, OpenTenderCommand, RegisterTenderContentCommand,
    ReviseTenderCommand, TenderCommandError, TenderErrorCode, TenderInspection, TenderSummary,
};

use tauri::Manager;

mod tauri_commands {
    use super::{
        ensure_quantix_setup as ensure_setup, AgentAccessRequestView, AgentRunInspection,
        ApproveAgentAccessCommand, ChooseTenderPackageCommand, ConfirmSourceRelationshipCommand,
        CreateTenderCommand, DocumentParseResult, DocumentRegister, EvidenceDocument,
        EvidenceSearchResult, ImportTenderPackageCommand, InterruptAgentRunCommand,
        OpenTenderCommand, ParseSourceArtifactCommand, QuantixHost, RequestAgentAccessCommand,
        ResolveAgentAccessCommand, ReviseTenderCommand, RunBootstrapAgentCommand, RuntimeReadiness,
        SearchEvidenceCommand, SetupOutcome, TenderCommandError, TenderErrorCode,
        TenderPackageImportResult, TenderPackageSourceKind, TenderSummary,
    };
    use tauri_plugin_dialog::DialogExt;

    #[tauri::command]
    pub(super) async fn ensure_quantix_setup(
        host: tauri::State<'_, QuantixHost>,
    ) -> Result<SetupOutcome, &'static str> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || ensure_setup(&host))
            .await
            .map_err(|_| "Quantix Setup stopped unexpectedly")
    }

    #[tauri::command]
    pub(super) async fn create_tender(
        host: tauri::State<'_, QuantixHost>,
        command: CreateTenderCommand,
    ) -> Result<TenderSummary, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.create_tender(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn list_tenders(
        host: tauri::State<'_, QuantixHost>,
    ) -> Result<Vec<TenderSummary>, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.list_tenders())
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn open_tender(
        host: tauri::State<'_, QuantixHost>,
        command: OpenTenderCommand,
    ) -> Result<TenderSummary, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.open_tender(&command.tender_id))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn revise_tender(
        host: tauri::State<'_, QuantixHost>,
        command: ReviseTenderCommand,
    ) -> Result<TenderSummary, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.revise_tender(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn choose_and_import_tender_package<R: tauri::Runtime>(
        app: tauri::AppHandle<R>,
        host: tauri::State<'_, QuantixHost>,
        command: ChooseTenderPackageCommand,
    ) -> Result<Option<TenderPackageImportResult>, TenderCommandError> {
        let source_kind = command.source_kind;
        let selected = tauri::async_runtime::spawn_blocking(move || {
            let picker = app.dialog().file();
            match source_kind {
                TenderPackageSourceKind::Directory => picker.blocking_pick_folder(),
                TenderPackageSourceKind::ZipArchive => picker
                    .add_filter("ZIP archive", &["zip"])
                    .blocking_pick_file(),
            }
        })
        .await
        .map_err(|_| TenderCommandError {
            code: TenderErrorCode::StoreUnavailable,
        })?;
        let Some(selected) = selected else {
            return Ok(None);
        };
        let source_path = selected.into_path().map_err(|_| TenderCommandError {
            code: TenderErrorCode::InvalidCommand,
        })?;
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || {
            host.import_tender_package(ImportTenderPackageCommand {
                tender_id: command.tender_id,
                source_path: source_path.to_string_lossy().into_owned(),
            })
            .map(Some)
        })
        .await
        .map_err(|_| TenderCommandError {
            code: TenderErrorCode::StoreUnavailable,
        })?
    }

    #[tauri::command]
    pub(super) async fn inspect_document_register(
        host: tauri::State<'_, QuantixHost>,
        command: OpenTenderCommand,
    ) -> Result<DocumentRegister, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || {
            host.inspect_document_register(&command.tender_id)
        })
        .await
        .map_err(|_| TenderCommandError {
            code: TenderErrorCode::StoreUnavailable,
        })?
    }

    #[tauri::command]
    pub(super) async fn confirm_source_relationship(
        host: tauri::State<'_, QuantixHost>,
        command: ConfirmSourceRelationshipCommand,
    ) -> Result<DocumentRegister, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.confirm_source_relationship(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn parse_source_artifact(
        host: tauri::State<'_, QuantixHost>,
        command: ParseSourceArtifactCommand,
    ) -> Result<DocumentParseResult, TenderCommandError> {
        host.inner().parse_source_artifact(command).await
    }

    #[tauri::command]
    pub(super) fn cancel_source_artifact_parse(
        host: tauri::State<'_, QuantixHost>,
        command: ParseSourceArtifactCommand,
    ) -> Result<bool, TenderCommandError> {
        host.inner().cancel_source_artifact_parse(command)
    }

    #[tauri::command]
    pub(super) async fn inspect_evidence(
        host: tauri::State<'_, QuantixHost>,
        command: ParseSourceArtifactCommand,
    ) -> Result<EvidenceDocument, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.inspect_evidence(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn search_evidence(
        host: tauri::State<'_, QuantixHost>,
        command: SearchEvidenceCommand,
    ) -> Result<EvidenceSearchResult, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.search_evidence(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn inspect_runtime_readiness(
        host: tauri::State<'_, QuantixHost>,
    ) -> Result<RuntimeReadiness, &'static str> {
        Ok(host.inner().inspect_runtime_readiness().await)
    }

    #[tauri::command]
    pub(super) async fn repair_runtime_readiness(
        host: tauri::State<'_, QuantixHost>,
    ) -> Result<RuntimeReadiness, &'static str> {
        Ok(host.inner().repair_runtime_readiness().await)
    }

    #[tauri::command]
    pub(super) fn cancel_runtime_preparation(host: tauri::State<'_, QuantixHost>) -> bool {
        host.inner().cancel_runtime_preparation()
    }

    #[tauri::command]
    pub(super) async fn run_bootstrap_agent(
        host: tauri::State<'_, QuantixHost>,
        command: RunBootstrapAgentCommand,
    ) -> Result<AgentRunInspection, TenderCommandError> {
        host.inner().run_bootstrap_agent(command).await
    }

    #[tauri::command]
    pub(super) async fn inspect_agent_runs(
        host: tauri::State<'_, QuantixHost>,
        command: OpenTenderCommand,
    ) -> Result<Vec<AgentRunInspection>, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.inspect_agent_runs(&command.tender_id))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn request_agent_access(
        host: tauri::State<'_, QuantixHost>,
        command: RequestAgentAccessCommand,
    ) -> Result<AgentAccessRequestView, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.request_agent_access(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn approve_agent_access(
        host: tauri::State<'_, QuantixHost>,
        command: ApproveAgentAccessCommand,
    ) -> Result<AgentAccessRequestView, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.approve_agent_access(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn resolve_agent_access(
        host: tauri::State<'_, QuantixHost>,
        command: ResolveAgentAccessCommand,
    ) -> Result<AgentAccessRequestView, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.resolve_agent_access(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) fn interrupt_agent_run(
        host: tauri::State<'_, QuantixHost>,
        command: InterruptAgentRunCommand,
    ) -> Result<bool, TenderCommandError> {
        host.inner().interrupt_agent_run(command)
    }
}

pub fn configure_tauri_builder<R: tauri::Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder.invoke_handler(tauri::generate_handler![
        tauri_commands::ensure_quantix_setup,
        tauri_commands::create_tender,
        tauri_commands::list_tenders,
        tauri_commands::open_tender,
        tauri_commands::revise_tender,
        tauri_commands::choose_and_import_tender_package,
        tauri_commands::inspect_document_register,
        tauri_commands::confirm_source_relationship,
        tauri_commands::parse_source_artifact,
        tauri_commands::cancel_source_artifact_parse,
        tauri_commands::inspect_evidence,
        tauri_commands::search_evidence,
        tauri_commands::inspect_runtime_readiness,
        tauri_commands::repair_runtime_readiness,
        tauri_commands::cancel_runtime_preparation,
        tauri_commands::run_bootstrap_agent,
        tauri_commands::inspect_agent_runs,
        tauri_commands::request_agent_access,
        tauri_commands::approve_agent_access,
        tauri_commands::resolve_agent_access,
        tauri_commands::interrupt_agent_run
    ])
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    configure_tauri_builder(tauri::Builder::default())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .setup(|app| {
            let application_home = app.path().home_dir()?.join(".quantix");
            let resource_directory = app.path().resource_dir()?;
            app.manage(QuantixHost::new(application_home, resource_directory));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
