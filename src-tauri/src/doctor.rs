//! Redacted, host-composed diagnostics for the About & Diagnostics surface.
//!
//! This module is intentionally orchestration-free: inspection composes facts
//! already produced by Setup, runtime, provider, update, and Tender integrity
//! services. The Host owns when inspection runs and dispatches only the typed
//! repair actions returned by this module after an Engineer gesture.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ts_rs::TS;

use crate::{
    application_settings::{
        tender_ai_execution_binding_from_view, ApplicationSettingsView, TenderAiExecutionBinding,
        TenderAiSelectionReadiness,
    },
    diagnostics::DiagnosticHealth,
    runtime_readiness::{RuntimeReadiness, RuntimeReadinessState},
    setup::{SetupOutcome, SetupState},
    tender_store::{TenderIntegrityReport, TenderIntegrityState},
    update::{UpdateDiagnostic, UpdateState, UpdateStatus},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum QuantixDoctorSeverity {
    Info,
    Warning,
    Blocker,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum QuantixDoctorArea {
    Setup,
    DocumentTools,
    DefaultAi,
    Update,
    TenderIntegrity,
    Diagnostics,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum QuantixDoctorRepairAction {
    PrepareDocumentTools,
    RetryDocumentTools,
    RefreshAiProvider,
    RebindTenderAiSelection,
    RetryUpdateInspection,
    InspectTenderIntegrity,
    RetryDiagnostics,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct QuantixDoctorFinding {
    pub code: String,
    pub area: QuantixDoctorArea,
    pub severity: QuantixDoctorSeverity,
    pub title: String,
    pub cause: String,
    pub affected_capability: String,
    pub impact: String,
    pub safe_remediation: String,
    pub repair_action: Option<QuantixDoctorRepairAction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct QuantixDoctorReport {
    pub revision: String,
    pub healthy: bool,
    pub findings: Vec<QuantixDoctorFinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct InspectQuantixDoctorCommand {
    pub tender_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum QuantixDoctorRepairTarget {
    Application,
    Tender,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct QuantixDoctorRepairCommand {
    pub report_revision: String,
    pub code: String,
    pub action: QuantixDoctorRepairAction,
    pub target: QuantixDoctorRepairTarget,
    pub tender_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorTenderInput {
    pub tender_id: String,
    pub integrity: TenderIntegrityReport,
    pub ai_execution: TenderAiExecutionBinding,
}

macro_rules! finding {
    ($code:expr, $area:expr, $severity:expr, $title:expr, $cause:expr,
     $capability:expr, $impact:expr, $remediation:expr, $action:expr $(,)?) => {
        QuantixDoctorFinding {
            code: $code.into(),
            area: $area,
            severity: $severity,
            title: $title.into(),
            cause: $cause.into(),
            affected_capability: $capability.into(),
            impact: $impact.into(),
            safe_remediation: $remediation.into(),
            repair_action: $action,
        }
    };
}

fn report_revision(findings: &[QuantixDoctorFinding]) -> String {
    let payload = serde_json_canonicalizer::to_vec(&findings).unwrap_or_default();
    Sha256::digest(payload)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Compose a redacted report from authoritative service snapshots. An update
/// source failure is represented by `Err` and deliberately receives no local
/// repair action: only the external update source can fix it.
pub fn compose_quantix_doctor_report(
    setup: &SetupOutcome,
    runtime: &RuntimeReadiness,
    settings: &ApplicationSettingsView,
    update: Result<&UpdateStatus, UpdateDiagnostic>,
    diagnostics: &DiagnosticHealth,
    tender: Option<&DoctorTenderInput>,
) -> QuantixDoctorReport {
    let mut findings = Vec::new();

    if !matches!(setup.state, SetupState::Ready | SetupState::Warning) {
        findings.push(finding!(
            "setup_not_ready",
            QuantixDoctorArea::Setup,
            QuantixDoctorSeverity::Blocker,
            "Quantix Setup needs attention",
            format!(
                "Setup is {:?}; review the listed Setup issues before Tender work.",
                setup.state
            ),
            "Application Home and storage",
            "Quantix cannot safely persist all Tender operations.",
            "Review the exact Setup finding and correct the storage or installation condition.",
            None,
        ));
    } else if !setup.issues.is_empty() {
        findings.push(finding!(
            "setup_warning",
            QuantixDoctorArea::Setup,
            QuantixDoctorSeverity::Warning,
            "Quantix Setup has a warning",
            "Storage or installation checks completed with a warning.",
            "Application Home and storage",
            "Tender data remains available, but a storage guarantee needs review.",
            "Review the listed Setup issue; Doctor will not mutate storage automatically.",
            None,
        ));
    }

    if runtime.state != RuntimeReadinessState::Ready {
        findings.push(finding!(
            "document_tools_not_ready",
            QuantixDoctorArea::DocumentTools,
            QuantixDoctorSeverity::Blocker,
            "Document tools are not ready",
            format!(
                "Document preparation is {:?}; inspect the exact runtime issue before retrying.",
                runtime.state
            ),
            "Document parsing and local models",
            "Source files remain registered, while document-assisted intake waits.",
            "Prepare or repair the managed runtime after reviewing its download and storage impact.",
            Some(if runtime.repair_available {
                QuantixDoctorRepairAction::RetryDocumentTools
            } else {
                QuantixDoctorRepairAction::PrepareDocumentTools
            }),
        ));
    }

    match settings.ai_execution_selection.as_ref() {
        None => findings.push(finding!(
            "ai_selection_required",
            QuantixDoctorArea::DefaultAi,
            QuantixDoctorSeverity::Warning,
            "Choose an AI default for new Tenders",
            "No approved application AI default is configured.",
            "AI-assisted work in newly created Tenders",
            "Local-only Tender work remains available; new AI Runs wait for an Engineer selection.",
            "Open AI & Models and select an exact provider, model, and reasoning option.",
            Some(QuantixDoctorRepairAction::RefreshAiProvider),
        )),
        Some(selection) => {
            let readiness =
                tender_ai_execution_binding_from_view(settings, Some(selection.clone())).readiness;
            if readiness != TenderAiSelectionReadiness::Ready {
                findings.push(finding!(
                    "ai_selection_not_ready",
                    QuantixDoctorArea::DefaultAi,
                    QuantixDoctorSeverity::Blocker,
                    "The application AI default is not ready",
                    match readiness {
                        TenderAiSelectionReadiness::ProviderUnavailable => String::from("The selected provider connection is unavailable."),
                        TenderAiSelectionReadiness::CatalogueStale => String::from("Refresh the provider catalogue before starting new AI work."),
                        TenderAiSelectionReadiness::ModelUnavailable => String::from("The selected model or reasoning capability is no longer available."),
                        TenderAiSelectionReadiness::ApprovalRequired => String::from("Confirm the selected provider destination and model before AI work starts."),
                        _ => String::from("The selected AI execution requires attention."),
                    },
                    "AI default for new Tenders",
                    "New Tenders may start local-only; existing Tender selections are unchanged.",
                    "Refresh provider status, then explicitly approve a supported selection.",
                    Some(QuantixDoctorRepairAction::RefreshAiProvider),
                ));
            }
        }
    }

    match update {
        Ok(status) if status.state == UpdateState::RepairRequired => {
            findings.push(finding!(
                "update_repair_required",
                QuantixDoctorArea::Update,
                QuantixDoctorSeverity::Warning,
                "The last application update needs repair",
                status.diagnostic.map_or_else(
                    || "Review Update Recovery before retrying.".into(),
                    |diagnostic| format!("Update recovery reports {:?}.", diagnostic),
                ),
                "Signed application updates",
                "The installed application remains usable; update recovery needs an Engineer decision.",
                "Retry signed update inspection. Installation or restart still requires separate approval.",
                Some(QuantixDoctorRepairAction::RetryUpdateInspection),
            ))
        }
        Ok(status) if status.diagnostic.is_some() => findings.push(finding!("update_warning", QuantixDoctorArea::Update, QuantixDoctorSeverity::Warning, "Application updates need attention", "The last signed-update inspection returned a diagnostic.", "Signed application updates", "Tender work is unaffected, but update availability cannot be confirmed.", "Retry signed update inspection; no application files change during inspection.", Some(QuantixDoctorRepairAction::RetryUpdateInspection))),
        Ok(_) => {}
        Err(_) => findings.push(finding!("update_source_unavailable", QuantixDoctorArea::Update, QuantixDoctorSeverity::Info, "The signed update source is unavailable", "The external signed update source could not be reached.", "Signed application updates", "Tender work is unaffected; update availability is temporarily unknown.", "No local repair is available. Try inspection again later.", None)),
    }

    if !diagnostics.enabled || diagnostics.degraded {
        findings.push(finding!(
            "diagnostics_degraded",
            QuantixDoctorArea::Diagnostics,
            QuantixDoctorSeverity::Warning,
            "Operational diagnostics need attention",
            if diagnostics.enabled {
                "The diagnostics writer reported a local storage error."
            } else {
                "The diagnostics writer is not active."
            },
            "Operational troubleshooting logs",
            "Tender work continues, but some troubleshooting events may be missing.",
            "Retry the diagnostics writer and reapply local retention checks.",
            Some(QuantixDoctorRepairAction::RetryDiagnostics),
        ));
    } else if diagnostics.dropped_critical > 0 {
        findings.push(finding!(
            "diagnostics_critical_events_dropped",
            QuantixDoctorArea::Diagnostics,
            QuantixDoctorSeverity::Warning,
            "Some critical diagnostics were dropped",
            "The protected diagnostics queue reached its bounded capacity.",
            "Operational troubleshooting logs",
            "Tender work was protected from backpressure; part of the diagnostic history is missing.",
            "Retry diagnostics after the current workload settles.",
            Some(QuantixDoctorRepairAction::RetryDiagnostics),
        ));
    }

    if let Some(tender) = tender {
        if !matches!(
            tender.ai_execution.readiness,
            TenderAiSelectionReadiness::Ready | TenderAiSelectionReadiness::LocalOnly
        ) {
            findings.push(finding!(
                "tender_ai_not_ready",
                QuantixDoctorArea::DefaultAi,
                QuantixDoctorSeverity::Blocker,
                "The selected Tender AI execution is not ready",
                tender.ai_execution.status_summary.clone(),
                "New Agent Runs in this Tender",
                "Messages and local work remain durable; AI-required work waits without fallback.",
                "Choose and approve an available selection for this Tender.",
                Some(QuantixDoctorRepairAction::RebindTenderAiSelection),
            ));
        }
        if tender.integrity.state != TenderIntegrityState::Ready {
            findings.push(finding!("tender_integrity_not_healthy", QuantixDoctorArea::TenderIntegrity, QuantixDoctorSeverity::Blocker, "A Tender needs integrity recovery", "The selected Tender store did not pass its integrity inspection.", "Selected Tender records and recovery", "The Tender remains isolated; mutation is blocked until the Engineer reviews recovery.", "Open Tender recovery to inspect the exact findings. Doctor does not modify Tender data.", Some(QuantixDoctorRepairAction::InspectTenderIntegrity)));
        }
    }

    let revision = report_revision(&findings);
    QuantixDoctorReport {
        revision,
        healthy: findings.is_empty(),
        findings,
    }
}

pub fn validate_quantix_doctor_repair(
    report: &QuantixDoctorReport,
    command: &QuantixDoctorRepairCommand,
) -> bool {
    let target_matches = command.report_revision == report.revision
        && match command.action {
            QuantixDoctorRepairAction::RebindTenderAiSelection
            | QuantixDoctorRepairAction::InspectTenderIntegrity => {
                command.target == QuantixDoctorRepairTarget::Tender && command.tender_id.is_some()
            }
            _ => {
                command.target == QuantixDoctorRepairTarget::Application
                    && command.tender_id.is_none()
            }
        };
    target_matches
        && report.findings.iter().any(|finding| {
            finding.code == command.code && finding.repair_action == Some(command.action)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        application_settings::{AppearancePreference, GeneralApplicationPreferences},
        runtime_readiness::RuntimeReadinessIssue,
        setup::SetupIssue,
    };

    fn settings() -> ApplicationSettingsView {
        ApplicationSettingsView {
            general_preferences: GeneralApplicationPreferences {
                appearance: AppearancePreference::System,
                reduced_motion: false,
                larger_text: false,
                notify_when_attention_needed: false,
            },
            ai_execution_selection: None,
            ai_execution_approval: None,
            provider_connections: Vec::new(),
            active_provider_login: None,
            storage: crate::application_settings::ApplicationStorageFacts {
                application_home: "C:/quantix".into(),
                tender_backups_are_preserved: true,
                trash_requires_explicit_purge: true,
            },
            diagnostics: crate::application_settings::ApplicationDiagnostics {
                quantix_version: "test".into(),
                installation_schema_version: 1,
                tender_schema_version: 1,
            },
        }
    }

    #[test]
    fn doctor_reports_actionable_runtime_and_local_only_ai_without_external_update_repair() {
        let report = compose_quantix_doctor_report(
            &SetupOutcome {
                state: SetupState::Warning,
                setup_performed: true,
                issues: vec![SetupIssue::StoragePermissionsUnverified],
            },
            &RuntimeReadiness {
                state: RuntimeReadinessState::MissingExecutable,
                issues: vec![RuntimeReadinessIssue::OcrExecutableMissing],
                uv_version: None,
                ocr_version: None,
                repair_available: true,
            },
            &settings(),
            Err(UpdateDiagnostic::DownloadFailed),
            &DiagnosticHealth {
                enabled: true,
                degraded: false,
                writer_error: None,
                dropped_normal: 0,
                dropped_critical: 0,
                retained_bytes: 0,
                retained_files: 0,
                deep_active_tender: None,
                deep_remaining_seconds: None,
                deep_bytes: 0,
            },
            None,
        );
        assert!(!report.healthy);
        assert_eq!(
            report
                .findings
                .iter()
                .find(|finding| finding.code == "document_tools_not_ready")
                .and_then(|finding| finding.repair_action),
            Some(QuantixDoctorRepairAction::RetryDocumentTools)
        );
        let update = report
            .findings
            .iter()
            .find(|finding| finding.code == "update_source_unavailable")
            .expect("update outage finding");
        assert_eq!(update.repair_action, None);
        assert!(report
            .findings
            .iter()
            .any(|finding| finding.code == "ai_selection_required"));
    }

    #[test]
    fn doctor_offers_only_the_safe_diagnostics_retry_for_writer_degradation() {
        let report = compose_quantix_doctor_report(
            &SetupOutcome {
                state: SetupState::Warning,
                setup_performed: true,
                issues: vec![SetupIssue::StoragePermissionsUnverified],
            },
            &RuntimeReadiness {
                state: RuntimeReadinessState::MissingExecutable,
                issues: vec![RuntimeReadinessIssue::OcrExecutableMissing],
                uv_version: None,
                ocr_version: None,
                repair_available: true,
            },
            &settings(),
            Err(UpdateDiagnostic::DownloadFailed),
            &DiagnosticHealth {
                enabled: true,
                degraded: true,
                writer_error: Some("diagnostics_writer_unavailable".into()),
                dropped_normal: 0,
                dropped_critical: 0,
                retained_bytes: 0,
                retained_files: 0,
                deep_active_tender: None,
                deep_remaining_seconds: None,
                deep_bytes: 0,
            },
            None,
        );
        let finding = report
            .findings
            .iter()
            .find(|finding| finding.code == "diagnostics_degraded")
            .expect("diagnostics finding");
        assert_eq!(finding.area, QuantixDoctorArea::Diagnostics);
        assert_eq!(
            finding.repair_action,
            Some(QuantixDoctorRepairAction::RetryDiagnostics)
        );
    }

    #[test]
    fn doctor_repair_validation_requires_the_reported_typed_action() {
        let findings = vec![finding!(
            "document_tools_not_ready",
            QuantixDoctorArea::DocumentTools,
            QuantixDoctorSeverity::Blocker,
            "Document tools are not ready",
            "repair",
            "Document tools",
            "Document work waits.",
            "Retry preparation.",
            Some(QuantixDoctorRepairAction::RetryDocumentTools),
        )];
        let report = QuantixDoctorReport {
            revision: report_revision(&findings),
            healthy: false,
            findings,
        };
        assert!(validate_quantix_doctor_repair(
            &report,
            &QuantixDoctorRepairCommand {
                report_revision: report.revision.clone(),
                code: "document_tools_not_ready".into(),
                action: QuantixDoctorRepairAction::RetryDocumentTools,
                target: QuantixDoctorRepairTarget::Application,
                tender_id: None,
            }
        ));
        assert!(!validate_quantix_doctor_repair(
            &report,
            &QuantixDoctorRepairCommand {
                report_revision: report.revision.clone(),
                code: "document_tools_not_ready".into(),
                action: QuantixDoctorRepairAction::RefreshAiProvider,
                target: QuantixDoctorRepairTarget::Application,
                tender_id: None,
            }
        ));
    }
}
