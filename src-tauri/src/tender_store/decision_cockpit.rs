use std::collections::HashSet;

use garde::Validate;
use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
    agent_runtime::{
        AgentAccessRequestStatus, AgentRunState, AgentTaskInputReference, VerificationStatus,
    },
    QuantixHost,
};

use super::{
    bid_decisions::{
        bid_decision_has_active_execution, bid_decision_lifecycle_is_open,
        BidDecisionPackageChangeSummary, BidDecisionPackageRecordCategory,
        BidRecommendationOutcome,
    },
    calculations::{CalculationRuleReviewOutcome, ControlledBoqCalculationStatus},
    change_assessments::{
        change_assessment_has_active_affected_execution, ChangeAssessmentImpactConsequence,
        ChangeAssessmentImpactKind, ChangeAssessmentStatus,
    },
    coordinated_baselines::CoordinatedBidBaselineBindingKind,
    external_rfis::ExternalRfiReviewOutcome,
    lock_mutex_with_check,
    production_scheduler::{ProductionFindingSeverity, ProductionReviewResult},
    require_setup,
    team_composer::{work_plan_decision_lifecycle_is_open, work_plan_has_active_execution},
    tender_queries::{InspectTenderQueriesCommand, TenderQuery},
    tender_records::{TenderRecordInspection, TenderRecordKind, TenderRecordTrustClass},
    BidPackageOperationBudget, InspectExternalRfisCommand, TenderBackupState, TenderCommandError,
    TenderErrorCode, TenderId, TenderLifecyclePhase, TenderRecoveryState, TenderStore,
};

const MAX_COCKPIT_DECISIONS: usize = 1_024;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct InspectDecisionCockpitCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum DecisionKind {
    BidDecision,
    WorkPlanApproval,
    TenderRecord,
    QueryTreatment,
    ExternalRfiIssue,
    ExternalRfiResponseInterpretation,
    CalculationRuleApproval,
    CalculationRunApproval,
    BasisOfEstimateApproval,
    PricedCostBaselineApproval,
    PricingAdjustmentApproval,
    CommercialStrategyApproval,
    PricingScenarioSelection,
    TenderPriceApproval,
    ProductionFindingException,
    CoordinatedBidBaselineApproval,
    ChangeAssessment,
    AgentAccessRequest,
    AgentRunRecovery,
    TenderRecovery,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum DecisionTargetKind {
    BidDecisionPackage,
    WorkPlan,
    WorkPlanWorkstream,
    WorkPlanTask,
    AgentProfile,
    TenderRecord,
    TenderQuery,
    ExternalRfi,
    ExternalRfiResponse,
    CalculationRule,
    CalculationRun,
    BasisOfEstimate,
    PricedCostBaseline,
    PricingAdjustment,
    CommercialStrategy,
    PricingScenario,
    ApprovedTenderPrice,
    CalculationManifest,
    ProductionReviewFinding,
    ProductionTask,
    ProductionArtifact,
    ProductionReview,
    CoordinatedBidBaseline,
    ChangeAssessment,
    AgentAccessRequest,
    AgentRun,
    TenderPackage,
    TenderBackup,
    TenderRecovery,
    Approval,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DecisionTarget {
    pub kind: DecisionTargetKind,
    pub object_id: String,
    pub version: u32,
    pub manifest_sha256: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum DecisionResponsibleKind {
    TenderingManager,
    AgentProfile,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DecisionResponsible {
    pub kind: DecisionResponsibleKind,
    pub label: String,
    pub profile_id: Option<String>,
    pub profile_version: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum DecisionLifecycleGate {
    EvidenceGovernance,
    BidDecision,
    WorkPlanApproval,
    QueryControl,
    ExternalIssue,
    CalculationGovernance,
    EstimateApproval,
    CommercialApproval,
    ProductionAssurance,
    IntegratedReview,
    ChangeAssessment,
    AccessControl,
    Recovery,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum DecisionUrgency {
    Immediate,
    Approaching,
    Routine,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum DecisionStatus {
    Ready,
    Blocked,
    AwaitingReview,
    Stale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum DecisionAction {
    Accept,
    Approve,
    Return,
    Reject,
    Verify,
    ApproveAssumption,
    ApplyTreatment,
    ClassifyIrrelevant,
    ClassifyMaterial,
    Select,
    ApproveException,
    RetryTask,
    CloseTask,
    ApproveReplacement,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum DecisionFactKind {
    AgentRecommendation,
    VerifiedFact,
    ApprovedAssumption,
    DeterministicResult,
    UnresolvedGap,
    PriorEngineerDecision,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DecisionEvidence {
    pub artifact_id: String,
    pub version: u32,
    pub location_ordinal: Option<u32>,
    pub label: String,
    pub original_text: Option<String>,
    pub translated_text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DecisionFact {
    pub kind: DecisionFactKind,
    pub label: String,
    pub value: String,
    pub evidence: Vec<DecisionEvidence>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum DecisionDependencyStatus {
    Current,
    Approved,
    Unresolved,
    Stale,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DecisionDependency {
    pub target: DecisionTarget,
    pub label: String,
    pub status: DecisionDependencyStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DecisionGroupMember {
    pub target: DecisionTarget,
    pub condition: String,
    pub status: DecisionStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PendingDecision {
    pub decision_id: String,
    pub kind: DecisionKind,
    pub title: String,
    pub summary: String,
    pub target: DecisionTarget,
    pub responsible: DecisionResponsible,
    pub lifecycle_gate: DecisionLifecycleGate,
    pub urgency: DecisionUrgency,
    pub urgency_reason: String,
    pub deadline: Option<String>,
    pub status: DecisionStatus,
    pub ready: bool,
    pub blocking_consequences: Vec<String>,
    pub allowed_actions: Vec<DecisionAction>,
    pub facts: Vec<DecisionFact>,
    pub evidence: Vec<DecisionEvidence>,
    pub changes_since_prior_review: Vec<String>,
    pub dependencies: Vec<DecisionDependency>,
    pub unresolved_queries: Vec<DecisionDependency>,
    pub assumptions: Vec<DecisionFact>,
    pub calculations: Vec<DecisionFact>,
    pub findings: Vec<DecisionFact>,
    pub exceptions: Vec<DecisionFact>,
    pub independent_review: Option<DecisionFact>,
    pub group_members: Vec<DecisionGroupMember>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DecisionCockpit {
    pub tender_id: String,
    pub tender_revision: u32,
    pub lifecycle_phase: TenderLifecyclePhase,
    pub pending_decisions: Vec<PendingDecision>,
}

impl QuantixHost {
    pub fn inspect_decision_cockpit(
        &self,
        command: InspectDecisionCockpitCommand,
    ) -> Result<DecisionCockpit, TenderCommandError> {
        require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let mut cockpit = lock_mutex_with_check(&store, &mut || budget.check())?
            .inspect_decision_cockpit(&tender_id, budget)?;
        self.collect_tender_recovery_decisions(&tender_id, &mut cockpit.pending_decisions)?;
        validate_and_sort_decisions(&mut cockpit.pending_decisions)?;
        Ok(cockpit)
    }

    fn collect_tender_recovery_decisions(
        &self,
        tender_id: &TenderId,
        decisions: &mut Vec<PendingDecision>,
    ) -> Result<(), TenderCommandError> {
        let backups = self.inspect_tender_backups(tender_id.as_str())?;
        for recovery in self
            .inspect_tender_recoveries(tender_id.as_str())?
            .into_iter()
            .filter(|recovery| recovery.state == TenderRecoveryState::AwaitingApproval)
        {
            let backup = backups
                .iter()
                .find(|backup| {
                    backup.backup_id == recovery.backup_id
                        && backup.state == TenderBackupState::Ready
                })
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            let backup_source = recovery
                .backup_source
                .as_ref()
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            let backup_manifest = backup
                .manifest_sha256
                .as_ref()
                .filter(|manifest| manifest.len() == 64)
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            if backup.source.as_ref() != Some(backup_source) {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }

            let mut facts = vec![DecisionFact {
                kind: DecisionFactKind::VerifiedFact,
                label: "Verified replacement candidate".into(),
                value: format!(
                    "Backup {} preserves {} revision {} with {} audit events at {}.",
                    backup.backup_id,
                    backup_source.name,
                    backup_source.revision,
                    backup_source.audit_event_count,
                    backup_source.audit_chain_head,
                ),
                evidence: Vec::new(),
            }];
            let mut dependencies = vec![DecisionDependency {
                target: target(
                    DecisionTargetKind::TenderBackup,
                    backup.backup_id.clone(),
                    backup_source.revision,
                    Some(backup_manifest.as_str().to_owned()),
                ),
                label: "Verified backup proposed for replacement".into(),
                status: DecisionDependencyStatus::Approved,
            }];
            if let Some(current_source) = &recovery.current_source {
                facts.push(DecisionFact {
                    kind: DecisionFactKind::VerifiedFact,
                    label: "Current Tender before replacement".into(),
                    value: format!(
                        "{} revision {} with {} audit events at {}.",
                        current_source.name,
                        current_source.revision,
                        current_source.audit_event_count,
                        current_source.audit_chain_head,
                    ),
                    evidence: Vec::new(),
                });
                dependencies.push(DecisionDependency {
                    target: target(
                        DecisionTargetKind::TenderPackage,
                        current_source.tender_id.clone(),
                        current_source.revision,
                        None,
                    ),
                    label: "Current Tender that would be replaced".into(),
                    status: DecisionDependencyStatus::Current,
                });
            } else {
                facts.push(DecisionFact {
                    kind: DecisionFactKind::UnresolvedGap,
                    label: "Current Tender before replacement".into(),
                    value: "No current Tender summary is available for comparison.".into(),
                    evidence: Vec::new(),
                });
            }

            let mut decision = pending(
                DecisionKind::TenderRecovery,
                "Approve verified Tender replacement".into(),
                format!(
                    "Decide whether recovery {} may replace the current Tender with verified backup {}.",
                    recovery.recovery_id, recovery.backup_id
                ),
                target(
                    DecisionTargetKind::TenderRecovery,
                    recovery.recovery_id,
                    backup_source.revision,
                    Some(backup_manifest.as_str().to_owned()),
                ),
                DecisionLifecycleGate::Recovery,
                None,
                true,
                vec![
                    "The verified replacement proposal remains staged until the Tendering Manager decides it."
                        .into(),
                ],
                vec![DecisionAction::ApproveReplacement, DecisionAction::Reject],
                facts,
                Vec::new(),
                None,
            );
            decision.dependencies = dependencies;
            decisions.push(decision);
        }
        Ok(())
    }
}

impl TenderStore {
    pub(crate) fn inspect_decision_cockpit(
        &self,
        tender_id: &TenderId,
        budget: BidPackageOperationBudget,
    ) -> Result<DecisionCockpit, TenderCommandError> {
        budget.check()?;
        let (tender_revision, lifecycle_phase): (u32, String) = self
            .connection
            .query_row(
                "SELECT current_revision, lifecycle_phase FROM tender
                 WHERE singleton = 1 AND tender_id = ?1",
                [tender_id.as_str()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(super::sql_error)?;
        let lifecycle_phase = TenderLifecyclePhase::parse(&lifecycle_phase)?;
        let mut decisions = Vec::new();
        let bid_execution_active = bid_decision_has_active_execution(&self.connection)?;
        let work_plan_execution_active = work_plan_has_active_execution(&self.connection)?;

        self.collect_bid_decision(bid_execution_active, &mut decisions)?;
        self.collect_work_plan_decision(
            budget,
            lifecycle_phase,
            work_plan_execution_active,
            &mut decisions,
        )?;
        self.collect_record_decisions(&mut decisions)?;
        self.collect_query_decisions(tender_id, budget, &mut decisions)?;
        self.collect_external_rfi_decisions(tender_id, budget, &mut decisions)?;
        self.collect_calculation_decisions(tender_revision, budget, &mut decisions)?;
        self.collect_estimate_decisions(budget, &mut decisions)?;
        self.collect_pricing_decisions(budget, &mut decisions)?;
        self.collect_production_finding_decisions(budget, &mut decisions)?;
        self.collect_baseline_decision(budget, &mut decisions)?;
        self.collect_change_decision(budget, &mut decisions)?;
        self.collect_runtime_decisions(&mut decisions)?;
        self.apply_change_intake_action_eligibility(&mut decisions)?;

        validate_and_sort_decisions(&mut decisions)?;
        Ok(DecisionCockpit {
            tender_id: tender_id.as_str().to_owned(),
            tender_revision,
            lifecycle_phase,
            pending_decisions: decisions,
        })
    }

    fn apply_change_intake_action_eligibility(
        &self,
        decisions: &mut [PendingDecision],
    ) -> Result<(), TenderCommandError> {
        if self.change_intake_is_writable()? {
            return Ok(());
        }
        for decision in decisions {
            if decision.allowed_actions.is_empty() {
                continue;
            }
            let allowed = match decision.kind {
                DecisionKind::TenderRecord => {
                    self.active_change_allows_record_governance(&decision.target.object_id)?
                }
                DecisionKind::QueryTreatment => self.active_change_allows_object(
                    ChangeAssessmentImpactKind::TenderQuery,
                    &decision.target.object_id,
                )?,
                DecisionKind::CalculationRunApproval => {
                    self.active_change_allows_calculation_run(&decision.target.object_id)?
                }
                DecisionKind::BasisOfEstimateApproval => self.active_change_allows_estimate(
                    &decision.target.object_id,
                    decision.target.version,
                )?,
                DecisionKind::PricedCostBaselineApproval
                | DecisionKind::PricingAdjustmentApproval
                | DecisionKind::CommercialStrategyApproval
                | DecisionKind::PricingScenarioSelection
                | DecisionKind::TenderPriceApproval => self.active_change_allows_pricing_object(
                    &decision.target.object_id,
                    decision.target.version,
                )?,
                DecisionKind::AgentAccessRequest => {
                    if let Some(dependency) = decision
                        .dependencies
                        .iter()
                        .find(|dependency| dependency.target.kind == DecisionTargetKind::AgentRun)
                    {
                        self.active_change_allows_run(&dependency.target.object_id)?
                            || self.unresolved_change_allows_unaffected_run(
                                &dependency.target.object_id,
                            )?
                    } else {
                        false
                    }
                }
                DecisionKind::ExternalRfiIssue
                | DecisionKind::ExternalRfiResponseInterpretation
                | DecisionKind::CalculationRuleApproval => false,
                DecisionKind::BidDecision | DecisionKind::WorkPlanApproval => false,
                DecisionKind::ProductionFindingException
                | DecisionKind::CoordinatedBidBaselineApproval
                | DecisionKind::ChangeAssessment
                | DecisionKind::AgentRunRecovery => true,
                DecisionKind::TenderRecovery => true,
            };
            if !allowed {
                decision.ready = false;
                decision.status = DecisionStatus::Blocked;
                decision.allowed_actions.clear();
                decision.blocking_consequences.push(
                    "The unresolved material-change gate does not authorize this exact target."
                        .into(),
                );
            }
        }
        Ok(())
    }

    fn collect_bid_decision(
        &self,
        active_execution: bool,
        decisions: &mut Vec<PendingDecision>,
    ) -> Result<(), TenderCommandError> {
        let Some(package) = self.inspect_current_bid_decision_package()? else {
            return Ok(());
        };
        if package.approval.is_some() {
            return Ok(());
        }
        let lifecycle_open = bid_decision_lifecycle_is_open(package.lifecycle_phase);
        let ready =
            package.current && lifecycle_open && package.decision_gate_ready && !active_execution;
        let status = decision_status_from_gate(ready, package.current, package.review.is_none());
        let recommendation = match package.recommendation.outcome {
            BidRecommendationOutcome::Proceed => "Proceed",
            BidRecommendationOutcome::Hold => "Hold",
            BidRecommendationOutcome::Decline => "Decline",
        };
        let mut facts = vec![DecisionFact {
            kind: DecisionFactKind::AgentRecommendation,
            label: "Bid recommendation".into(),
            value: format!("{recommendation}: {}", package.recommendation.rationale),
            evidence: Vec::new(),
        }];
        if let Some(prior) = &package.return_rework_basis {
            facts.push(DecisionFact {
                kind: DecisionFactKind::PriorEngineerDecision,
                label: "Prior return decision".into(),
                value: prior
                    .items
                    .iter()
                    .map(|item| format!("{}: {}", item.required_rework, item.resolution))
                    .collect::<Vec<_>>()
                    .join("; "),
                evidence: Vec::new(),
            });
        }
        if let Some(prior) = &package.material_change_basis {
            facts.push(DecisionFact {
                kind: DecisionFactKind::PriorEngineerDecision,
                label: "Prior approval invalidation".into(),
                value: prior.material_change_summary.clone(),
                evidence: Vec::new(),
            });
        }
        let mut package_records = Vec::new();
        for category in [
            BidDecisionPackageRecordCategory::ProjectFingerprint,
            BidDecisionPackageRecordCategory::Risk,
            BidDecisionPackageRecordCategory::Opportunity,
            BidDecisionPackageRecordCategory::Assumption,
            BidDecisionPackageRecordCategory::UnresolvedQuery,
        ] {
            let mut after_ordinal = None;
            let mut seen_ordinals = HashSet::new();
            loop {
                let page = self.inspect_bid_decision_package_record_page(
                    &package.package_id,
                    package.version,
                    category,
                    after_ordinal,
                    4,
                )?;
                package_records.extend(page.records);
                let Some(next) = page.next_ordinal else {
                    break;
                };
                if !seen_ordinals.insert(next) {
                    return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
                }
                after_ordinal = Some(next);
            }
        }
        let mut evidence = Vec::new();
        let mut assumptions = Vec::new();
        let mut findings = Vec::new();
        let mut unresolved_queries = Vec::new();
        let mut group_members = Vec::new();
        for binding in package_records {
            let record_evidence = record_evidence(&binding.record);
            for reference in &record_evidence {
                if !evidence.contains(reference) {
                    evidence.push(reference.clone());
                }
            }
            let fact = trust_fact(&binding.record, record_evidence);
            facts.push(fact.clone());
            match binding.category {
                BidDecisionPackageRecordCategory::Assumption => assumptions.push(fact),
                BidDecisionPackageRecordCategory::Risk => findings.push(fact),
                BidDecisionPackageRecordCategory::UnresolvedQuery => {
                    unresolved_queries.push(DecisionDependency {
                        target: target(
                            DecisionTargetKind::TenderRecord,
                            binding.record.record_id.clone(),
                            binding.record.version,
                            None,
                        ),
                        label: binding.record.title.clone(),
                        status: DecisionDependencyStatus::Unresolved,
                    });
                }
                BidDecisionPackageRecordCategory::ProjectFingerprint
                | BidDecisionPackageRecordCategory::Opportunity => {}
            }
            group_members.push(DecisionGroupMember {
                target: target(
                    DecisionTargetKind::TenderRecord,
                    binding.record.record_id,
                    binding.record.version,
                    None,
                ),
                condition: format!(
                    "{}; {:?}; {:?}",
                    binding.record.title, binding.category, binding.record.verification_status
                ),
                status: decision_status_for_record(
                    package.current,
                    binding.record.verification_status,
                ),
            });
        }
        let mut blockers = package
            .blockers
            .iter()
            .map(|blocker| blocker.summary.clone())
            .collect::<Vec<_>>();
        if active_execution {
            blockers.push(
                "An active Agent Run or parse attempt must terminalize before the decision.".into(),
            );
        }
        if !lifecycle_open {
            blockers.push("The Bid Decision gate is not open in this lifecycle phase.".into());
        }
        let review = package.review.as_ref().map(|review| DecisionFact {
            kind: DecisionFactKind::AgentRecommendation,
            label: "Independent review".into(),
            value: format!(
                "{:?}; {} disclosed findings",
                review.outcome,
                review.findings.len()
            ),
            evidence: Vec::new(),
        });
        let mut decision = pending(
            DecisionKind::BidDecision,
            "Proceed, hold, or decline".into(),
            "Govern the exact independently reviewed Bid Decision Package.".into(),
            target(
                DecisionTargetKind::BidDecisionPackage,
                package.package_id.clone(),
                package.version,
                Some(package.manifest_sha256.clone()),
            ),
            DecisionLifecycleGate::BidDecision,
            None,
            ready,
            blockers,
            if ready {
                vec![
                    DecisionAction::Accept,
                    DecisionAction::Return,
                    DecisionAction::Reject,
                ]
            } else if package.current && lifecycle_open && !active_execution {
                vec![DecisionAction::Return]
            } else {
                Vec::new()
            },
            facts,
            evidence,
            review,
        );
        decision.status = status;
        decision.assumptions = assumptions;
        decision.findings.extend(findings);
        if let Some(review) = &package.review {
            decision
                .findings
                .extend(review.findings.iter().map(|finding| DecisionFact {
                    kind: DecisionFactKind::UnresolvedGap,
                    label: format!("{:?} review finding: {}", finding.severity, finding.code),
                    value: finding.summary.clone(),
                    evidence: Vec::new(),
                }));
        }
        decision.unresolved_queries = unresolved_queries;
        decision.changes_since_prior_review = bid_change_summary(&package.change_summary);
        decision.group_members = group_members;
        decisions.push(decision);
        Ok(())
    }

    fn collect_work_plan_decision(
        &self,
        budget: BidPackageOperationBudget,
        lifecycle_phase: TenderLifecyclePhase,
        active_execution: bool,
        decisions: &mut Vec<PendingDecision>,
    ) -> Result<(), TenderCommandError> {
        let Some(plan) = self.inspect_current_work_plan(budget)? else {
            return Ok(());
        };
        if plan.approval.is_some() {
            return Ok(());
        }
        let member_status = if plan.current {
            DecisionStatus::Ready
        } else {
            DecisionStatus::Stale
        };
        let mut group_members = plan
            .profiles
            .iter()
            .map(|binding| DecisionGroupMember {
                target: target(
                    DecisionTargetKind::AgentProfile,
                    binding.profile.profile_id.clone(),
                    binding.profile.version,
                    None,
                ),
                condition: format!(
                    "{}; capabilities: {}; profile status: {:?}",
                    binding.profile.objective,
                    binding.profile.capabilities.join(", "),
                    binding.status,
                ),
                status: member_status,
            })
            .collect::<Vec<_>>();
        group_members.extend(
            plan.workstreams
                .iter()
                .map(|workstream| DecisionGroupMember {
                    target: target(
                        DecisionTargetKind::WorkPlanWorkstream,
                        workstream.workstream_key.clone(),
                        plan.version,
                        Some(plan.manifest_sha256.clone()),
                    ),
                    condition: format!(
                "{}; capability: {}; accountable profile: {}; dependencies: {}; deadlines: {}",
                workstream.name,
                workstream.capability,
                workstream.accountable_profile_id.as_deref().unwrap_or("unassigned"),
                workstream.dependencies.join(", "),
                workstream.deadlines.join(", "),
            ),
                    status: member_status,
                }),
        );
        group_members.extend(plan.tasks.iter().map(|task| DecisionGroupMember {
            target: target(
                DecisionTargetKind::WorkPlanTask,
                task.task_key.clone(),
                plan.version,
                Some(plan.manifest_sha256.clone()),
            ),
            condition: format!(
                "{}; profile {} v{}; workstream {}; dependencies: {}; deadline: {}; independent reviewer: {}",
                task.objective,
                task.profile_id,
                task.profile_version,
                task.workstream_key,
                task.dependencies.join(", "),
                task.deadline,
                task.review_profile_id.as_deref().unwrap_or("not required"),
            ),
            status: member_status,
        }));
        group_members.extend(plan.query_bindings.iter().map(|query| DecisionGroupMember {
            target: target(
                DecisionTargetKind::TenderRecord,
                query.record_id.clone(),
                query.version,
                None,
            ),
            condition: "Exact Query record bound to the Work Plan.".into(),
            status: member_status,
        }));
        let gaps = plan
            .capability_gaps
            .iter()
            .map(|gap| DecisionFact {
                kind: DecisionFactKind::UnresolvedGap,
                label: gap.capability.clone(),
                value: gap.reason.clone(),
                evidence: Vec::new(),
            })
            .collect::<Vec<_>>();
        let mut blockers = plan.blocker_codes.clone();
        if active_execution {
            blockers.push("An active Agent Run or parse attempt must terminalize before the Work Plan decision.".into());
        }
        let lifecycle_open = work_plan_decision_lifecycle_is_open(lifecycle_phase);
        if !lifecycle_open {
            blockers
                .push("The Work Plan decision gate is not open in this lifecycle phase.".into());
        }
        let ready = plan.current && lifecycle_open && blockers.is_empty();
        let status = decision_status_from_gate(ready, plan.current, false);
        let mut decision = pending(
            DecisionKind::WorkPlanApproval,
            "Approve the Tender Office Work Plan".into(),
            "Review every profile, task, dependency, permission ceiling, deadline, and capability gap.".into(),
            target(
                DecisionTargetKind::WorkPlan,
                plan.plan_id,
                plan.version,
                Some(plan.manifest_sha256),
            ),
            DecisionLifecycleGate::WorkPlanApproval,
            earliest(plan.tasks.iter().map(|task| task.deadline.as_str())),
            ready,
            blockers,
            if ready {
                vec![DecisionAction::Accept, DecisionAction::Return, DecisionAction::Reject]
            } else if plan.current && lifecycle_open && !active_execution {
                vec![DecisionAction::Return, DecisionAction::Reject]
            } else {
                Vec::new()
            },
            Vec::new(),
            Vec::new(),
            None,
        );
        decision.status = status;
        decision.findings = gaps;
        decision.group_members = group_members;
        decisions.push(decision);
        Ok(())
    }

    fn collect_record_decisions(
        &self,
        decisions: &mut Vec<PendingDecision>,
    ) -> Result<(), TenderCommandError> {
        let mut cursor = None;
        let mut seen_cursors = HashSet::new();
        loop {
            let page = self.inspect_tender_record_page(cursor.as_deref(), 4)?;
            for record in page.records {
                if record.verification_status != VerificationStatus::Proposed {
                    continue;
                }
                let mut actions = vec![DecisionAction::Reject];
                if record.kind == TenderRecordKind::Assumption {
                    actions.insert(0, DecisionAction::ApproveAssumption);
                } else {
                    actions.insert(0, DecisionAction::Verify);
                }
                let evidence = record_evidence(&record);
                let fact = trust_fact(&record, evidence.clone());
                let review = record
                    .reviews
                    .iter()
                    .find(|review| review.reviewer_kind != "engineer_user")
                    .map(|review| DecisionFact {
                        kind: DecisionFactKind::AgentRecommendation,
                        label: "Independent record review".into(),
                        value: format!("{:?}: {}", review.outcome, review.rationale),
                        evidence: evidence.clone(),
                    });
                let ready = record.current_for_decision();
                decisions.push(pending(
                    DecisionKind::TenderRecord,
                    format!("Govern {}", record.title),
                    "Decide whether this exact proposed record may support controlled Tender work."
                        .into(),
                    target(
                        DecisionTargetKind::TenderRecord,
                        record.record_id,
                        record.version,
                        None,
                    ),
                    DecisionLifecycleGate::EvidenceGovernance,
                    None,
                    ready,
                    if ready {
                        Vec::new()
                    } else {
                        vec!["The exact record is stale or already governed.".into()]
                    },
                    actions,
                    vec![fact],
                    evidence,
                    review,
                ));
            }
            cursor = page.next_cursor;
            if cursor.is_none() {
                break;
            }
            if cursor
                .as_ref()
                .is_some_and(|next| !seen_cursors.insert(next.clone()))
            {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
        }
        Ok(())
    }

    fn collect_query_decisions(
        &self,
        tender_id: &TenderId,
        budget: BidPackageOperationBudget,
        decisions: &mut Vec<PendingDecision>,
    ) -> Result<(), TenderCommandError> {
        let mut cursor = None;
        let mut seen_cursors = HashSet::new();
        loop {
            let command = InspectTenderQueriesCommand {
                tender_id: tender_id.as_str().to_owned(),
                cursor: cursor.clone(),
                limit: 8,
            };
            let page = self.inspect_tender_queries(&command, budget)?;
            for query in page.items {
                if query.approved_treatment.is_some() || !query.current {
                    continue;
                }
                decisions.push(query_decision(query));
            }
            cursor = page.next_cursor;
            if cursor.is_none() {
                break;
            }
            if cursor
                .as_ref()
                .is_some_and(|next| !seen_cursors.insert(next.clone()))
            {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
        }
        Ok(())
    }

    fn collect_external_rfi_decisions(
        &self,
        tender_id: &TenderId,
        budget: BidPackageOperationBudget,
        decisions: &mut Vec<PendingDecision>,
    ) -> Result<(), TenderCommandError> {
        let mut cursor = None;
        let mut seen_cursors = HashSet::new();
        loop {
            let command = InspectExternalRfisCommand {
                tender_id: tender_id.as_str().to_owned(),
                cursor: cursor.clone(),
                limit: 8,
            };
            let page = self.inspect_external_rfis(&command, budget)?;
            for rfi in page.items {
                if rfi.current && rfi.approval.is_none() {
                    let review_passed = rfi
                        .review
                        .as_ref()
                        .is_some_and(|review| review.outcome == ExternalRfiReviewOutcome::Passed);
                    let evidence = input_evidence(&rfi.source_evidence);
                    let review = rfi.review.as_ref().map(|review| DecisionFact {
                        kind: DecisionFactKind::AgentRecommendation,
                        label: "Independent RFI review".into(),
                        value: format!("{:?}; {} findings", review.outcome, review.findings.len()),
                        evidence: evidence.clone(),
                    });
                    let ready = review_passed && rfi.evidence_current;
                    let status = decision_status_from_gate(
                        ready,
                        rfi.evidence_current,
                        rfi.review.is_none(),
                    );
                    let blockers = if ready {
                        Vec::new()
                    } else {
                        vec!["A passing independent review of current Evidence is required before issue approval.".into()]
                    };
                    let mut decision = pending(
                        DecisionKind::ExternalRfiIssue,
                        "Approve External RFI for issue".into(),
                        rfi.response_need.clone(),
                        target(
                            DecisionTargetKind::ExternalRfi,
                            rfi.rfi_id.clone(),
                            rfi.version,
                            Some(rfi.manifest_sha256.clone()),
                        ),
                        DecisionLifecycleGate::ExternalIssue,
                        Some(rfi.due_at.clone()),
                        ready,
                        blockers,
                        if ready {
                            vec![DecisionAction::Approve]
                        } else {
                            Vec::new()
                        },
                        vec![DecisionFact {
                            kind: DecisionFactKind::VerifiedFact,
                            label: "Recipient".into(),
                            value: format!(
                                "{} — {}",
                                rfi.recipient.organization, rfi.recipient.attention
                            ),
                            evidence: Vec::new(),
                        }],
                        evidence,
                        review,
                    );
                    decision.status = status;
                    decisions.push(decision);
                }
                if rfi.approval.is_some() {
                    for response in &rfi.responses {
                        for query in &rfi.query_refs {
                            if rfi.interpretations.iter().any(|interpretation| {
                                interpretation.response_link_id == response.response_link_id
                                    && interpretation.query_id == query.query_id
                            }) {
                                continue;
                            }
                            let evidence = vec![DecisionEvidence {
                                artifact_id: response.source_artifact_id.clone(),
                                version: response.source_artifact_version,
                                location_ordinal: None,
                                label: "Registered External RFI response".into(),
                                original_text: None,
                                translated_text: None,
                            }];
                            let current_query = rfi
                                .current_query_refs
                                .iter()
                                .find(|current| current.query_id == query.query_id);
                            let ready = rfi.current && current_query.is_some();
                            let decision_query_version = current_query
                                .map(|current| current.version)
                                .unwrap_or(query.version);
                            let mut decision = pending(
                                DecisionKind::ExternalRfiResponseInterpretation,
                                "Interpret External RFI response".into(),
                                "Bind the exact response to an explicit material or non-material Query treatment.".into(),
                                target(DecisionTargetKind::ExternalRfiResponse, format!("{}:{}", response.response_link_id, query.query_id), decision_query_version, Some(response.manifest_sha256.clone())),
                                DecisionLifecycleGate::QueryControl,
                                Some(rfi.due_at.clone()),
                                ready,
                                if ready { Vec::new() } else { vec!["The issued RFI response no longer has a current unresolved Query basis.".into()] },
                                if ready { vec![DecisionAction::ApplyTreatment] } else { Vec::new() },
                                Vec::new(),
                                evidence,
                                None,
                            );
                            decision.status = decision_status_from_gate(ready, rfi.current, false);
                            decision.dependencies.push(DecisionDependency {
                                target: target(
                                    DecisionTargetKind::TenderQuery,
                                    query.query_id.clone(),
                                    query.version,
                                    Some(query.manifest_sha256.clone()),
                                ),
                                label: "Query version issued in the approved RFI".into(),
                                status: if decision_query_version == query.version {
                                    DecisionDependencyStatus::Current
                                } else {
                                    DecisionDependencyStatus::Stale
                                },
                            });
                            if let Some(current) = current_query {
                                decision.dependencies.push(DecisionDependency {
                                    target: target(
                                        DecisionTargetKind::TenderQuery,
                                        current.query_id.clone(),
                                        current.version,
                                        Some(current.manifest_sha256.clone()),
                                    ),
                                    label: "Current unresolved Query decision basis".into(),
                                    status: DecisionDependencyStatus::Unresolved,
                                });
                            }
                            decisions.push(decision);
                        }
                    }
                }
            }
            cursor = page.next_cursor;
            if cursor.is_none() {
                break;
            }
            if cursor
                .as_ref()
                .is_some_and(|next| !seen_cursors.insert(next.clone()))
            {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
        }
        Ok(())
    }

    fn collect_calculation_decisions(
        &self,
        tender_revision: u32,
        budget: BidPackageOperationBudget,
        decisions: &mut Vec<PendingDecision>,
    ) -> Result<(), TenderCommandError> {
        let workspace = self.inspect_calculation_workspace(0, 0, budget)?;
        if let Some(rule) = workspace
            .rule
            .as_ref()
            .filter(|rule| rule.approval.is_none())
        {
            let review_passed = rule
                .review
                .as_ref()
                .is_some_and(|review| review.outcome == CalculationRuleReviewOutcome::Passed);
            let tests_passed = rule.deterministic_tests.iter().all(|test| test.passed);
            let ready = rule.current && review_passed && tests_passed;
            let status = decision_status_from_gate(ready, rule.current, rule.review.is_none());
            let review = rule.review.as_ref().map(|review| DecisionFact {
                kind: DecisionFactKind::AgentRecommendation,
                label: "Independent rule review".into(),
                value: format!("{:?}; {} findings", review.outcome, review.findings.len()),
                evidence: Vec::new(),
            });
            let mut decision = pending(
                DecisionKind::CalculationRuleApproval,
                format!("Approve Calculation Rule: {}", rule.name),
                rule.formula.clone(),
                target(
                    DecisionTargetKind::CalculationRule,
                    rule.rule_id.clone(),
                    rule.version,
                    Some(rule.manifest_sha256.clone()),
                ),
                DecisionLifecycleGate::CalculationGovernance,
                None,
                ready,
                if ready {
                    Vec::new()
                } else {
                    vec![
                        "Deterministic tests and an independent passing review are required."
                            .into(),
                    ]
                },
                if ready {
                    vec![DecisionAction::Approve]
                } else {
                    Vec::new()
                },
                vec![DecisionFact {
                    kind: DecisionFactKind::DeterministicResult,
                    label: "Deterministic tests".into(),
                    value: format!(
                        "{} of {} passed",
                        rule.deterministic_tests
                            .iter()
                            .filter(|test| test.passed)
                            .count(),
                        rule.deterministic_tests.len()
                    ),
                    evidence: Vec::new(),
                }],
                Vec::new(),
                review,
            );
            decision.status = status;
            decisions.push(decision);
        }
        let total_run_count = workspace.total_run_count;
        let mut run_offset: u32 = 0;
        let mut page = workspace;
        loop {
            for run in page.recent_runs.into_iter().filter(|run| {
                run.approval.is_none() && run.status == ControlledBoqCalculationStatus::Completed
            }) {
                let ready = run.tender_revision == tender_revision;
                let mut decision = pending(
                    DecisionKind::CalculationRunApproval,
                    "Approve controlled calculation result".into(),
                    run.description.clone(),
                    target(
                        DecisionTargetKind::CalculationRun,
                        run.calculation_run_id,
                        1,
                        Some(run.manifest_sha256),
                    ),
                    DecisionLifecycleGate::CalculationGovernance,
                    None,
                    ready,
                    if ready {
                        Vec::new()
                    } else {
                        vec![format!(
                            "The Calculation Run belongs to stale Tender revision {} rather than current revision {tender_revision}.",
                            run.tender_revision
                        )]
                    },
                    if ready {
                        vec![DecisionAction::Approve]
                    } else {
                        Vec::new()
                    },
                    vec![DecisionFact {
                        kind: DecisionFactKind::DeterministicResult,
                        label: "Canonical result".into(),
                        value: run
                            .final_amount
                            .map(|amount| format!("{amount} {}", run.output_currency))
                            .unwrap_or_else(|| "No result".into()),
                        evidence: Vec::new(),
                    }],
                    Vec::new(),
                    None,
                );
                if !ready {
                    decision.status = DecisionStatus::Stale;
                }
                decisions.push(decision);
            }
            if !page.has_older_runs {
                break;
            }
            run_offset = run_offset
                .checked_add(8)
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            if run_offset >= total_run_count {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            page = self.inspect_calculation_workspace(0, run_offset, budget)?;
            if page.total_run_count != total_run_count || page.recent_runs.is_empty() {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
        }
        Ok(())
    }

    fn collect_estimate_decisions(
        &self,
        budget: BidPackageOperationBudget,
        decisions: &mut Vec<PendingDecision>,
    ) -> Result<(), TenderCommandError> {
        let workspace = self.inspect_estimate_workspace(0, None, budget)?;
        let Some(basis) = workspace.basis.filter(|basis| basis.approval.is_none()) else {
            return Ok(());
        };
        let ready = basis.current
            && basis.complete
            && basis.reconciled
            && basis.review.as_ref().is_some_and(|review| {
                matches!(
                    review.outcome,
                    super::estimates::BasisOfEstimateReviewOutcome::Passed
                )
            });
        let status = decision_status_from_gate(ready, basis.current, basis.review.is_none());
        let facts = vec![DecisionFact {
            kind: DecisionFactKind::DeterministicResult,
            label: "Estimate total".into(),
            value: format!("{} {}", basis.total_amount, basis.total_currency),
            evidence: Vec::new(),
        }];
        let assumptions = basis
            .material_assumptions
            .iter()
            .map(|assumption| DecisionFact {
                kind: DecisionFactKind::ApprovedAssumption,
                label: format!(
                    "Query {} v{}",
                    assumption.query_id, assumption.query_version
                ),
                value: assumption.treatment_details.clone(),
                evidence: Vec::new(),
            })
            .collect::<Vec<_>>();
        let review = basis.review.as_ref().map(|review| DecisionFact {
            kind: DecisionFactKind::AgentRecommendation,
            label: "Independent Basis review".into(),
            value: format!("{:?}; {} findings", review.outcome, review.findings.len()),
            evidence: Vec::new(),
        });
        let mut decision = pending(
            DecisionKind::BasisOfEstimateApproval,
            "Approve Basis of Estimate".into(),
            basis.scope.clone(),
            target(
                DecisionTargetKind::BasisOfEstimate,
                basis.basis_id,
                basis.version,
                Some(basis.manifest_sha256),
            ),
            DecisionLifecycleGate::EstimateApproval,
            None,
            ready,
            basis.blockers.clone(),
            if ready {
                vec![DecisionAction::Approve]
            } else {
                Vec::new()
            },
            facts,
            basis
                .quotations
                .iter()
                .map(|quotation| DecisionEvidence {
                    artifact_id: quotation.evidence.artifact_id.clone(),
                    version: quotation.evidence.version,
                    location_ordinal: Some(quotation.evidence.ordinal),
                    label: quotation.counterparty.clone(),
                    original_text: None,
                    translated_text: None,
                })
                .collect(),
            review,
        );
        decision.status = status;
        decision.assumptions = assumptions;
        decisions.push(decision);
        Ok(())
    }

    fn collect_pricing_decisions(
        &self,
        budget: BidPackageOperationBudget,
        decisions: &mut Vec<PendingDecision>,
    ) -> Result<(), TenderCommandError> {
        let workspace = self.inspect_pricing_workspace(budget)?;
        let strategy_gate_ready = workspace
            .baseline
            .as_ref()
            .is_some_and(|baseline| baseline.approval.is_some() && baseline.current);
        let approved_strategy_ids = workspace
            .strategies
            .iter()
            .filter(|strategy| strategy.current && strategy.approval.is_some())
            .map(|strategy| strategy.strategy_id.clone())
            .collect::<HashSet<_>>();
        if let Some(baseline) = workspace
            .baseline
            .filter(|baseline| baseline.approval.is_none())
        {
            let ready = baseline.current
                && baseline.review.as_ref().is_some_and(|review| {
                    matches!(
                        review.outcome,
                        super::pricing::PricedCostBaselineReviewOutcome::Passed
                    )
                });
            let status =
                decision_status_from_gate(ready, baseline.current, baseline.review.is_none());
            let mut decision = pending(
                DecisionKind::PricedCostBaselineApproval,
                "Approve Priced Cost Baseline".into(),
                baseline.rationale,
                target(
                    DecisionTargetKind::PricedCostBaseline,
                    baseline.baseline_id,
                    baseline.version,
                    Some(baseline.manifest_sha256),
                ),
                DecisionLifecycleGate::CommercialApproval,
                None,
                ready,
                if ready {
                    Vec::new()
                } else {
                    vec!["A current independently reviewed baseline is required.".into()]
                },
                if ready {
                    vec![DecisionAction::Approve]
                } else {
                    Vec::new()
                },
                vec![DecisionFact {
                    kind: DecisionFactKind::DeterministicResult,
                    label: "Expected cost".into(),
                    value: format!("{} {}", baseline.amount, baseline.currency),
                    evidence: Vec::new(),
                }],
                Vec::new(),
                baseline.review.map(|review| DecisionFact {
                    kind: DecisionFactKind::AgentRecommendation,
                    label: "Independent baseline review".into(),
                    value: format!("{:?}; {} findings", review.outcome, review.findings.len()),
                    evidence: Vec::new(),
                }),
            );
            decision.status = status;
            decisions.push(decision);
        }
        for adjustment in workspace
            .adjustments
            .into_iter()
            .filter(|adjustment| adjustment.approval.is_none() && adjustment.current)
        {
            let ready = adjustment.review.as_ref().is_some_and(|review| {
                matches!(
                    review.outcome,
                    super::pricing::PricedCostBaselineReviewOutcome::Passed
                )
            });
            let status = decision_status_from_gate(ready, true, adjustment.review.is_none());
            let mut decision = pending(
                DecisionKind::PricingAdjustmentApproval,
                "Approve pricing adjustment".into(),
                adjustment.scope,
                target(
                    DecisionTargetKind::PricingAdjustment,
                    adjustment.adjustment_id,
                    adjustment.version,
                    Some(adjustment.manifest_sha256),
                ),
                DecisionLifecycleGate::CommercialApproval,
                None,
                ready,
                if ready {
                    Vec::new()
                } else {
                    vec!["A passing independent review is required.".into()]
                },
                if ready {
                    vec![DecisionAction::Approve]
                } else {
                    Vec::new()
                },
                vec![DecisionFact {
                    kind: DecisionFactKind::DeterministicResult,
                    label: "Adjustment".into(),
                    value: format!(
                        "{:?} {} {}",
                        adjustment.direction, adjustment.amount, adjustment.currency
                    ),
                    evidence: Vec::new(),
                }],
                Vec::new(),
                adjustment.review.map(|review| DecisionFact {
                    kind: DecisionFactKind::AgentRecommendation,
                    label: "Independent adjustment review".into(),
                    value: format!("{:?}; {} findings", review.outcome, review.findings.len()),
                    evidence: Vec::new(),
                }),
            );
            decision.status = status;
            decisions.push(decision);
        }
        for strategy in workspace
            .strategies
            .into_iter()
            .filter(|strategy| strategy.approval.is_none() && strategy.current)
        {
            decisions.push(pending(
                DecisionKind::CommercialStrategyApproval,
                "Approve commercial strategy".into(),
                strategy.commercial_appetite,
                target(
                    DecisionTargetKind::CommercialStrategy,
                    strategy.strategy_id,
                    1,
                    Some(strategy.manifest_sha256),
                ),
                DecisionLifecycleGate::CommercialApproval,
                None,
                strategy_gate_ready,
                if strategy_gate_ready {
                    Vec::new()
                } else {
                    vec!["A current Approved Priced Cost Baseline is required.".into()]
                },
                if strategy_gate_ready {
                    vec![DecisionAction::Approve]
                } else {
                    Vec::new()
                },
                Vec::new(),
                Vec::new(),
                None,
            ));
        }
        for scenario in workspace
            .scenarios
            .into_iter()
            .filter(|scenario| scenario.current)
        {
            let exact_strategy_approved =
                approved_strategy_ids.contains(scenario.strategy_id.as_str());
            let dependencies = pricing_scenario_dependencies(&scenario);
            if scenario.selection.is_none() {
                let mut decision = pending(
                    DecisionKind::PricingScenarioSelection,
                    format!("Select pricing scenario: {}", scenario.name),
                    "Promote this exact immutable alternative for Tender Price consideration."
                        .into(),
                    target(
                        DecisionTargetKind::PricingScenario,
                        scenario.pricing_scenario_id.clone(),
                        scenario.version,
                        Some(scenario.manifest_sha256.clone()),
                    ),
                    DecisionLifecycleGate::CommercialApproval,
                    None,
                    exact_strategy_approved,
                    if exact_strategy_approved {
                        Vec::new()
                    } else {
                        vec!["The scenario's exact commercial strategy must be current and Approved before selection.".into()]
                    },
                    if exact_strategy_approved {
                        vec![DecisionAction::Select]
                    } else {
                        Vec::new()
                    },
                    vec![DecisionFact {
                        kind: DecisionFactKind::DeterministicResult,
                        label: "Scenario total".into(),
                        value: format!(
                            "{} {}",
                            scenario.calculation.final_amount, scenario.calculation.currency
                        ),
                        evidence: Vec::new(),
                    }],
                    Vec::new(),
                    None,
                );
                decision.dependencies = dependencies;
                decisions.push(decision);
            } else if scenario.approved_tender_price.is_none() {
                let ready = exact_strategy_approved;
                let mut decision = pending(
                    DecisionKind::TenderPriceApproval,
                    "Approve Tender Price".into(),
                    "Approve the exact selected scenario and canonical Calculation Manifest."
                        .into(),
                    target(
                        DecisionTargetKind::PricingScenario,
                        scenario.pricing_scenario_id,
                        scenario.version,
                        Some(scenario.manifest_sha256),
                    ),
                    DecisionLifecycleGate::CommercialApproval,
                    None,
                    ready,
                    if ready {
                        Vec::new()
                    } else {
                        vec![
                            "The selected scenario requires an Approved commercial strategy."
                                .into(),
                        ]
                    },
                    if ready {
                        vec![DecisionAction::Approve]
                    } else {
                        Vec::new()
                    },
                    vec![DecisionFact {
                        kind: DecisionFactKind::DeterministicResult,
                        label: "Tender Price".into(),
                        value: format!(
                            "{} {}",
                            scenario.calculation.final_amount, scenario.calculation.currency
                        ),
                        evidence: Vec::new(),
                    }],
                    Vec::new(),
                    None,
                );
                decision.dependencies = dependencies;
                decisions.push(decision);
            }
        }
        Ok(())
    }

    fn collect_production_finding_decisions(
        &self,
        budget: BidPackageOperationBudget,
        decisions: &mut Vec<PendingDecision>,
    ) -> Result<(), TenderCommandError> {
        let Some(production) = self.inspect_tender_production(budget)? else {
            return Ok(());
        };
        for task in production.tasks {
            if task.review_count == 0 || task.open_blocking_finding_count == 0 {
                continue;
            }
            let review = self.inspect_production_task_review(&task.production_task_id, budget)?;
            for production_review in review.reviews {
                for finding in production_review.findings.into_iter().filter(|finding| {
                    finding.disposition.is_none()
                        && finding.severity == ProductionFindingSeverity::Major
                }) {
                    let exception_allowed = matches!(
                        task.task.major_finding_policy,
                        super::team_composer::MajorFindingPolicy::EngineerExceptionAllowed
                    ) && production_review.result
                        == ProductionReviewResult::RequiresRemediation;
                    let artifact_id = production_review.target_artifact_id.clone();
                    let review_id = production_review.review_id.clone();
                    let finding_id = finding.finding_id.clone();
                    if !exception_allowed {
                        continue;
                    }
                    let mut decision = pending(
                        DecisionKind::ProductionFindingException,
                        "Decide Major Review Finding exception".into(),
                        finding.summary.clone(),
                        target(
                            DecisionTargetKind::ProductionReviewFinding,
                            finding_id,
                            production_review.target_version,
                            Some(production_review.target_payload_sha256.clone()),
                        ),
                        DecisionLifecycleGate::ProductionAssurance,
                        Some(task.task.deadline.clone()),
                        true,
                        Vec::new(),
                        vec![DecisionAction::ApproveException],
                        Vec::new(),
                        Vec::new(),
                        Some(DecisionFact {
                            kind: DecisionFactKind::AgentRecommendation,
                            label: "Independent Review Finding".into(),
                            value: finding.summary,
                            evidence: Vec::new(),
                        }),
                    );
                    decision.findings.push(DecisionFact {
                        kind: DecisionFactKind::UnresolvedGap,
                        label: "Major finding".into(),
                        value: production_review.capability.clone(),
                        evidence: Vec::new(),
                    });
                    decision.dependencies = vec![
                        DecisionDependency {
                            target: target(
                                DecisionTargetKind::ProductionTask,
                                task.production_task_id.clone(),
                                1,
                                None,
                            ),
                            label: "Exact Production Task".into(),
                            status: DecisionDependencyStatus::Current,
                        },
                        DecisionDependency {
                            target: target(
                                DecisionTargetKind::ProductionReview,
                                review_id,
                                production_review.target_version,
                                None,
                            ),
                            label: "Independent Review".into(),
                            status: DecisionDependencyStatus::Current,
                        },
                        DecisionDependency {
                            target: target(
                                DecisionTargetKind::ProductionArtifact,
                                artifact_id,
                                production_review.target_version,
                                Some(production_review.target_payload_sha256.clone()),
                            ),
                            label: "Exact reviewed Artifact".into(),
                            status: DecisionDependencyStatus::Current,
                        },
                    ];
                    decisions.push(decision);
                }
            }
        }
        Ok(())
    }

    fn collect_baseline_decision(
        &self,
        budget: BidPackageOperationBudget,
        decisions: &mut Vec<PendingDecision>,
    ) -> Result<(), TenderCommandError> {
        let page = self.inspect_coordinated_bid_baselines(None, 1, budget)?;
        let Some(baseline) = page
            .items
            .into_iter()
            .next()
            .filter(|baseline| baseline.current && baseline.approval.is_none())
        else {
            return Ok(());
        };
        let ready = baseline.blockers.is_empty()
            && baseline.contradictions.is_empty()
            && page.lifecycle_phase == TenderLifecyclePhase::IntegratedReview;
        let blockers = baseline
            .blockers
            .iter()
            .map(|blocker| blocker.summary.clone())
            .chain(
                baseline
                    .contradictions
                    .iter()
                    .map(|contradiction| contradiction.summary.clone()),
            )
            .collect::<Vec<_>>();
        let group_members = baseline
            .bindings
            .iter()
            .map(|binding| DecisionGroupMember {
                target: target(
                    baseline_binding_target_kind(binding.kind),
                    binding.reference_id.clone(),
                    binding.version,
                    Some(binding.manifest_sha256.clone()),
                ),
                condition: format!("{}; source: {}", binding.summary, binding.source),
                status: DecisionStatus::Ready,
            })
            .collect();
        let mut decision = pending(
            DecisionKind::CoordinatedBidBaselineApproval,
            "Approve Coordinated Bid Baseline".into(),
            baseline.explanation,
            target(
                DecisionTargetKind::CoordinatedBidBaseline,
                baseline.baseline_id,
                baseline.version,
                Some(baseline.manifest_sha256),
            ),
            DecisionLifecycleGate::IntegratedReview,
            None,
            ready,
            blockers,
            if ready {
                vec![
                    DecisionAction::Accept,
                    DecisionAction::Return,
                    DecisionAction::Reject,
                ]
            } else if page.lifecycle_phase == TenderLifecyclePhase::IntegratedReview {
                vec![DecisionAction::Return, DecisionAction::Reject]
            } else {
                Vec::new()
            },
            Vec::new(),
            Vec::new(),
            None,
        );
        decision.group_members = group_members;
        decisions.push(decision);
        Ok(())
    }

    fn collect_change_decision(
        &self,
        budget: BidPackageOperationBudget,
        decisions: &mut Vec<PendingDecision>,
    ) -> Result<(), TenderCommandError> {
        let page = self.inspect_change_assessments(None, 1, budget)?;
        let Some(assessment) = page.active.filter(|assessment| {
            assessment.status == ChangeAssessmentStatus::Pending && assessment.decision.is_none()
        }) else {
            return Ok(());
        };
        let affected_execution_active = change_assessment_has_active_affected_execution(
            &self.connection,
            &assessment.assessment_id,
        )?;
        let assessment_sequence = assessment.assessment_sequence;
        let evidence = assessment
            .replacement_source
            .evidence_preview
            .iter()
            .map(|excerpt| DecisionEvidence {
                artifact_id: assessment.replacement_source.artifact_id.clone(),
                version: assessment.replacement_source.version,
                location_ordinal: Some(excerpt.ordinal),
                label: excerpt.structural_path.clone(),
                original_text: Some(excerpt.original_text.clone()),
                translated_text: excerpt.translated_text.clone(),
            })
            .collect::<Vec<_>>();
        let dependencies = assessment
            .impacts
            .iter()
            .map(|impact| DecisionDependency {
                target: target(
                    change_target_kind(impact.kind),
                    impact.object_id.clone(),
                    impact.object_version,
                    None,
                ),
                label: impact.summary.clone(),
                status: match impact.consequence {
                    ChangeAssessmentImpactConsequence::Stale => DecisionDependencyStatus::Stale,
                    ChangeAssessmentImpactConsequence::Reopen
                    | ChangeAssessmentImpactConsequence::Revoke => {
                        DecisionDependencyStatus::Unresolved
                    }
                },
            })
            .collect::<Vec<_>>();
        let group_members = dependencies
            .iter()
            .map(|dependency| DecisionGroupMember {
                target: dependency.target.clone(),
                condition: dependency.label.clone(),
                status: if dependency.status == DecisionDependencyStatus::Stale {
                    DecisionStatus::Stale
                } else {
                    DecisionStatus::Blocked
                },
            })
            .collect();
        let mut consequences = assessment
            .approval_consequences
            .iter()
            .map(|consequence| consequence.consequence.clone())
            .collect::<Vec<_>>();
        if affected_execution_active {
            consequences.push(
                "Material classification must wait for every affected execution to terminalize; irrelevant classification remains available."
                    .into(),
            );
        }
        let mut actions = vec![DecisionAction::ClassifyIrrelevant];
        if !affected_execution_active {
            actions.push(DecisionAction::ClassifyMaterial);
        }
        let mut decision = pending(
            DecisionKind::ChangeAssessment,
            "Classify confirmed source change".into(),
            assessment.deadline_effect.clone(),
            target(
                DecisionTargetKind::ChangeAssessment,
                assessment.assessment_id,
                assessment_sequence,
                Some(assessment.manifest_sha256),
            ),
            DecisionLifecycleGate::ChangeAssessment,
            None,
            true,
            consequences,
            actions,
            vec![DecisionFact {
                kind: DecisionFactKind::VerifiedFact,
                label: "Confirmed replacement Evidence".into(),
                value: assessment.replacement_source.package_path,
                evidence: evidence.clone(),
            }],
            evidence,
            None,
        );
        decision.changes_since_prior_review = assessment.affected_commitments;
        decision.dependencies = dependencies;
        decision.group_members = group_members;
        decisions.push(decision);
        Ok(())
    }

    fn collect_runtime_decisions(
        &self,
        decisions: &mut Vec<PendingDecision>,
    ) -> Result<(), TenderCommandError> {
        for run in self.inspect_agent_runs()? {
            for request in run
                .access_requests
                .iter()
                .filter(|request| request.status == AgentAccessRequestStatus::Blocked)
            {
                let mut decision = pending(
                    DecisionKind::AgentAccessRequest,
                    "Decide one-run access request".into(),
                    request.request.purpose.clone(),
                    target(
                        DecisionTargetKind::AgentAccessRequest,
                        request.request.request_id.clone(),
                        1,
                        None,
                    ),
                    DecisionLifecycleGate::AccessControl,
                    None,
                    false,
                    vec![if request.request.recurring {
                        "Recurring access requires a Work Plan Amendment; use the exact Agent Office gate.".into()
                    } else {
                        "Access decisions require live Agent Run revalidation in the exact Agent Office gate.".into()
                    }],
                    Vec::new(),
                    Vec::new(),
                    input_evidence(&request.request.exact_inputs),
                    None,
                );
                decision.dependencies.push(DecisionDependency {
                    target: target(DecisionTargetKind::AgentRun, run.run_id.clone(), 1, None),
                    label: "Requesting Agent Run".into(),
                    status: DecisionDependencyStatus::Current,
                });
                decisions.push(decision);
            }
            if run.state == AgentRunState::Indeterminate && run.recovery_decision.is_none() {
                let retry_eligible = run.linked_retry_supported
                    && self.indeterminate_run_retry_is_eligible(&run.run_id)?;
                decisions.push(pending(
                    DecisionKind::AgentRunRecovery,
                    "Resolve indeterminate Agent Run".into(),
                    run.failure.as_ref().map(|failure| failure.required_user_action.clone()).unwrap_or_else(|| "Choose whether to start a separate retry or close the uncertain task.".into()),
                    target(DecisionTargetKind::AgentRun, run.run_id, 1, None),
                    DecisionLifecycleGate::Recovery,
                    None,
                    true,
                    if retry_eligible {
                        vec!["The exact Tender Task remains blocked while the Provider Turn outcome is unknown.".into()]
                    } else {
                        vec!["The exact Tender Task remains blocked, and stale Work Plan dependencies prohibit a retry.".into()]
                    },
                    if retry_eligible {
                        vec![DecisionAction::RetryTask, DecisionAction::CloseTask]
                    } else {
                        vec![DecisionAction::CloseTask]
                    },
                    Vec::new(),
                    Vec::new(),
                    None,
                ));
            }
        }
        Ok(())
    }
}

trait RecordDecisionCurrent {
    fn current_for_decision(&self) -> bool;
}

impl RecordDecisionCurrent for TenderRecordInspection {
    fn current_for_decision(&self) -> bool {
        self.verification_status == VerificationStatus::Proposed
    }
}

fn validate_and_sort_decisions(
    decisions: &mut Vec<PendingDecision>,
) -> Result<(), TenderCommandError> {
    if decisions.len() > MAX_COCKPIT_DECISIONS {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let mut seen_decision_ids = HashSet::with_capacity(decisions.len());
    if !decisions
        .iter()
        .all(|decision| seen_decision_ids.insert(decision.decision_id.clone()))
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    decisions.sort_by(|left, right| {
        left.urgency
            .cmp(&right.urgency)
            .then_with(|| left.deadline.cmp(&right.deadline))
            .then_with(|| left.kind.cmp(&right.kind))
            .then_with(|| left.decision_id.cmp(&right.decision_id))
    });
    Ok(())
}

fn decision_status_from_gate(ready: bool, current: bool, awaiting_review: bool) -> DecisionStatus {
    if ready {
        DecisionStatus::Ready
    } else if !current {
        DecisionStatus::Stale
    } else if awaiting_review {
        DecisionStatus::AwaitingReview
    } else {
        DecisionStatus::Blocked
    }
}

fn pending(
    kind: DecisionKind,
    title: String,
    summary: String,
    target: DecisionTarget,
    lifecycle_gate: DecisionLifecycleGate,
    deadline: Option<String>,
    ready: bool,
    blocking_consequences: Vec<String>,
    allowed_actions: Vec<DecisionAction>,
    facts: Vec<DecisionFact>,
    evidence: Vec<DecisionEvidence>,
    independent_review: Option<DecisionFact>,
) -> PendingDecision {
    let (urgency, urgency_reason) = urgency(deadline.as_deref(), !blocking_consequences.is_empty());
    let status = if ready {
        DecisionStatus::Ready
    } else {
        DecisionStatus::Blocked
    };
    let decision_id = format!("{:?}:{}:{}", kind, target.object_id, target.version);
    let assumptions = facts
        .iter()
        .filter(|fact| fact.kind == DecisionFactKind::ApprovedAssumption)
        .cloned()
        .collect();
    let calculations = facts
        .iter()
        .filter(|fact| fact.kind == DecisionFactKind::DeterministicResult)
        .cloned()
        .collect();
    let findings = facts
        .iter()
        .filter(|fact| fact.kind == DecisionFactKind::UnresolvedGap)
        .cloned()
        .collect();
    PendingDecision {
        decision_id,
        kind,
        title,
        summary,
        target,
        responsible: DecisionResponsible {
            kind: DecisionResponsibleKind::TenderingManager,
            label: "Tendering Manager".into(),
            profile_id: None,
            profile_version: None,
        },
        lifecycle_gate,
        urgency,
        urgency_reason,
        deadline,
        status,
        ready,
        blocking_consequences,
        allowed_actions,
        facts,
        evidence,
        changes_since_prior_review: Vec::new(),
        dependencies: Vec::new(),
        unresolved_queries: Vec::new(),
        assumptions,
        calculations,
        findings,
        exceptions: Vec::new(),
        independent_review,
        group_members: Vec::new(),
    }
}

fn target(
    kind: DecisionTargetKind,
    object_id: String,
    version: u32,
    manifest_sha256: Option<String>,
) -> DecisionTarget {
    DecisionTarget {
        kind,
        object_id,
        version,
        manifest_sha256,
    }
}

fn urgency(deadline: Option<&str>, blocking: bool) -> (DecisionUrgency, String) {
    if blocking {
        return (
            DecisionUrgency::Immediate,
            "This decision is blocking controlled work.".into(),
        );
    }
    if let Some(deadline) = deadline {
        let now = Timestamp::now();
        if let Ok(exact_deadline) = deadline.parse::<Timestamp>() {
            if exact_deadline <= now {
                return (
                    DecisionUrgency::Immediate,
                    format!("The canonical deadline has passed: {deadline}."),
                );
            }
            if std::time::Duration::try_from(now.duration_until(exact_deadline))
                .is_ok_and(|remaining| remaining <= std::time::Duration::from_secs(72 * 60 * 60))
            {
                return (
                    DecisionUrgency::Approaching,
                    format!("The canonical deadline is within 72 hours: {deadline}."),
                );
            }
        }
        return (
            DecisionUrgency::Routine,
            format!("The canonical deadline is recorded: {deadline}."),
        );
    }
    (
        DecisionUrgency::Routine,
        "No earlier canonical deadline is recorded for this gate.".into(),
    )
}

fn decision_status_for_record(
    package_current: bool,
    verification_status: VerificationStatus,
) -> DecisionStatus {
    if !package_current {
        return DecisionStatus::Stale;
    }
    match verification_status {
        VerificationStatus::Verified => DecisionStatus::Ready,
        VerificationStatus::Proposed => DecisionStatus::AwaitingReview,
        VerificationStatus::Stale | VerificationStatus::Superseded => DecisionStatus::Stale,
        VerificationStatus::Rejected => DecisionStatus::Blocked,
    }
}

fn bid_change_summary(summary: &BidDecisionPackageChangeSummary) -> Vec<String> {
    let mut changes = Vec::new();
    if let Some(prior_version) = summary.prior_version {
        changes.push(format!(
            "Compared with Bid Decision Package v{prior_version}."
        ));
    }
    if summary.added_record_count > 0 {
        changes.push(format!(
            "{} exact records were added.",
            summary.added_record_count
        ));
    }
    if summary.removed_record_count > 0 {
        changes.push(format!(
            "{} exact records were removed.",
            summary.removed_record_count
        ));
    }
    if summary.changed_compliance_row_count > 0 {
        changes.push(format!(
            "{} Compliance Matrix rows changed.",
            summary.changed_compliance_row_count
        ));
    }
    if summary.capability_demands_changed {
        changes.push("Capability Demands changed.".into());
    }
    if summary.resource_implications_changed {
        changes.push("Resource implications changed.".into());
    }
    changes
}

fn earliest<'a>(values: impl Iterator<Item = &'a str>) -> Option<String> {
    values
        .filter(|value| !value.is_empty())
        .min()
        .map(str::to_owned)
}

fn record_evidence(record: &TenderRecordInspection) -> Vec<DecisionEvidence> {
    record
        .fields
        .iter()
        .flat_map(|field| field.evidence.iter())
        .map(|evidence| DecisionEvidence {
            artifact_id: evidence.reference.artifact_id.clone(),
            version: evidence.reference.version,
            location_ordinal: Some(evidence.reference.ordinal),
            label: format!(
                "{} · {}",
                evidence.package_path, evidence.location.structural_path
            ),
            original_text: Some(evidence.location.original_text.clone()),
            translated_text: evidence.location.translated_text.clone(),
        })
        .collect()
}

fn input_evidence(inputs: &[AgentTaskInputReference]) -> Vec<DecisionEvidence> {
    inputs
        .iter()
        .filter_map(|input| match input.kind.as_str() {
            "source_evidence"
            | "calculation_quantity_evidence"
            | "calculation_unit_rate_evidence" => {
                let (artifact_id, ordinal) = input.reference.rsplit_once('#')?;
                Some(DecisionEvidence {
                    artifact_id: artifact_id.into(),
                    version: input.version,
                    location_ordinal: ordinal.parse().ok(),
                    label: input.kind.clone(),
                    original_text: None,
                    translated_text: None,
                })
            }
            "source_artifact" => Some(DecisionEvidence {
                artifact_id: input.reference.clone(),
                version: input.version,
                location_ordinal: None,
                label: input.kind.clone(),
                original_text: None,
                translated_text: None,
            }),
            _ => None,
        })
        .collect()
}

fn trust_fact(record: &TenderRecordInspection, evidence: Vec<DecisionEvidence>) -> DecisionFact {
    let kind = match record.trust_class {
        TenderRecordTrustClass::AiProposal => DecisionFactKind::AgentRecommendation,
        TenderRecordTrustClass::DeterministicFact => DecisionFactKind::DeterministicResult,
        TenderRecordTrustClass::Verified | TenderRecordTrustClass::EngineerVerified => {
            DecisionFactKind::VerifiedFact
        }
        TenderRecordTrustClass::ApprovedAssumption => DecisionFactKind::ApprovedAssumption,
        TenderRecordTrustClass::UnresolvedGap => DecisionFactKind::UnresolvedGap,
        TenderRecordTrustClass::PriorDecision => DecisionFactKind::PriorEngineerDecision,
    };
    let value = record
        .fields
        .iter()
        .map(|field| {
            let exact_value = field
                .normalized_value
                .as_deref()
                .or(field.value.as_deref())
                .or(field.original_expression.as_deref())
                .or(field.basis_description.as_deref())
                .unwrap_or("No canonical value recorded");
            format!("{}: {exact_value}", field.name)
        })
        .collect::<Vec<_>>()
        .join("; ");
    DecisionFact {
        kind,
        label: record.title.clone(),
        value,
        evidence,
    }
}

fn query_decision(query: TenderQuery) -> PendingDecision {
    let evidence = input_evidence(&query.evidence);
    let facts = query
        .proposed_treatments
        .iter()
        .map(|proposal| DecisionFact {
            kind: DecisionFactKind::AgentRecommendation,
            label: format!("Proposed {:?} treatment", proposal.treatment),
            value: proposal.rationale.clone(),
            evidence: evidence.clone(),
        })
        .collect();
    let ready = !query.proposed_treatments.is_empty();
    let mut decision = pending(
        DecisionKind::QueryTreatment,
        format!("Decide Query treatment: {}", query.question),
        query.ambiguity_or_gap,
        target(
            DecisionTargetKind::TenderQuery,
            query.query_id,
            query.version,
            Some(query.manifest_sha256),
        ),
        DecisionLifecycleGate::QueryControl,
        Some(query.due_at),
        ready,
        if ready {
            Vec::new()
        } else {
            vec!["No bounded treatment has been proposed for Engineer decision.".into()]
        },
        if ready {
            vec![DecisionAction::ApplyTreatment]
        } else {
            Vec::new()
        },
        facts,
        evidence,
        None,
    );
    decision.unresolved_queries.push(DecisionDependency {
        target: decision.target.clone(),
        label: "Unresolved Tender Query".into(),
        status: DecisionDependencyStatus::Unresolved,
    });
    decision
}

fn change_target_kind(kind: super::ChangeAssessmentImpactKind) -> DecisionTargetKind {
    use super::ChangeAssessmentImpactKind as Kind;
    match kind {
        Kind::TenderRecord => DecisionTargetKind::TenderRecord,
        Kind::WorkPlan => DecisionTargetKind::WorkPlan,
        Kind::ProductionTask => DecisionTargetKind::ProductionTask,
        Kind::ProductionArtifact => DecisionTargetKind::ProductionArtifact,
        Kind::Review => DecisionTargetKind::ProductionReview,
        Kind::AgentRun => DecisionTargetKind::AgentRun,
        Kind::TenderQuery => DecisionTargetKind::TenderQuery,
        Kind::CalculationRun => DecisionTargetKind::CalculationRun,
        Kind::Estimate => DecisionTargetKind::BasisOfEstimate,
        Kind::PricingDecision => DecisionTargetKind::PricingScenario,
        Kind::CoordinatedBaseline => DecisionTargetKind::CoordinatedBidBaseline,
        Kind::Package => DecisionTargetKind::TenderPackage,
        Kind::Approval => DecisionTargetKind::Approval,
    }
}

fn baseline_binding_target_kind(kind: CoordinatedBidBaselineBindingKind) -> DecisionTargetKind {
    match kind {
        CoordinatedBidBaselineBindingKind::ProductionArtifactVersion => {
            DecisionTargetKind::ProductionArtifact
        }
        CoordinatedBidBaselineBindingKind::TenderRecordVersion => DecisionTargetKind::TenderRecord,
        CoordinatedBidBaselineBindingKind::TenderQueryVersion => DecisionTargetKind::TenderQuery,
        CoordinatedBidBaselineBindingKind::ExternalRfiVersion => DecisionTargetKind::ExternalRfi,
        CoordinatedBidBaselineBindingKind::PricedCostBaseline => {
            DecisionTargetKind::PricedCostBaseline
        }
        CoordinatedBidBaselineBindingKind::ApprovedTenderPrice => {
            DecisionTargetKind::ApprovedTenderPrice
        }
        CoordinatedBidBaselineBindingKind::CalculationManifest => {
            DecisionTargetKind::CalculationManifest
        }
        CoordinatedBidBaselineBindingKind::CommercialStrategy => {
            DecisionTargetKind::CommercialStrategy
        }
    }
}

fn pricing_scenario_dependencies(
    scenario: &super::pricing::PricingScenarioVersion,
) -> Vec<DecisionDependency> {
    vec![
        DecisionDependency {
            target: target(
                DecisionTargetKind::PricedCostBaseline,
                scenario.baseline_id.clone(),
                scenario.baseline_version,
                Some(scenario.baseline_manifest_sha256.clone()),
            ),
            label: "Exact Approved Priced Cost Baseline".into(),
            status: DecisionDependencyStatus::Approved,
        },
        DecisionDependency {
            target: target(
                DecisionTargetKind::CommercialStrategy,
                scenario.strategy_id.clone(),
                1,
                Some(scenario.strategy_manifest_sha256.clone()),
            ),
            label: "Scenario-bound commercial strategy".into(),
            status: DecisionDependencyStatus::Approved,
        },
        DecisionDependency {
            target: target(
                DecisionTargetKind::CalculationManifest,
                scenario.calculation.pricing_calculation_run_id.clone(),
                1,
                Some(scenario.calculation.manifest_sha256.clone()),
            ),
            label: "Canonical pricing Calculation Manifest".into(),
            status: DecisionDependencyStatus::Current,
        },
    ]
}
