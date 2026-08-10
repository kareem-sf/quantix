use std::collections::{HashMap, HashSet};

use garde::Validate;
use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::json;
use ts_rs::TS;

use crate::{
    agent_runtime::{AgentProfileVersionView, AgentTaskInputReference},
    tender_store::{BidPackageOperationBudget, TenderRecordVersionReference},
};

use super::{
    agent_records::load_profile, append_audit_event, append_audit_event_with_sequence,
    lock_mutex_with_check, random_identifier, sha256_hex, sql_error, sqlite_timestamp,
    valid_identifier, QuantixHost, TenderCommandError, TenderErrorCode, TenderId, TenderStore,
};

const MAX_QUERY_VERSIONS: u32 = 32;
const MAX_TENDER_QUERIES: usize = 256;
const MAX_QUERY_VERSIONS_TOTAL: u32 = 4_096;
const MAX_QUERY_INVALIDATIONS_TOTAL: u32 = 65_536;
const MAX_QUERY_PAGE_ITEMS: u32 = 8;
const MAX_QUERY_EVIDENCE: usize = 64;
const MAX_QUERY_AFFECTED_RECORDS: usize = 64;
const MAX_QUERY_AFFECTED_TASKS: usize = 64;
const MAX_QUERY_PROPOSED_TREATMENTS: usize = 16;
const MAX_QUERY_RESPONSES: usize = 32;
const MAX_QUERY_PRODUCTION_TARGETS: usize = 256;
const MAX_PRODUCTION_QUERY_CONTEXT_BYTES: usize = 2 * 1024 * 1024;
const QUERY_CONTEXT_DECISION_AND_METADATA_RESERVE: usize = 16 * 1024;
const ALL_PRODUCTION_TASKS_SCOPE: &str = "*";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum TenderQueryType {
    MissingInformation,
    Ambiguity,
    Contradiction,
    ResponsibilitySensitive,
}

impl TenderQueryType {
    fn as_str(self) -> &'static str {
        match self {
            Self::MissingInformation => "missing_information",
            Self::Ambiguity => "ambiguity",
            Self::Contradiction => "contradiction",
            Self::ResponsibilitySensitive => "responsibility_sensitive",
        }
    }

    fn parse(value: &str) -> Result<Self, TenderCommandError> {
        match value {
            "missing_information" => Ok(Self::MissingInformation),
            "ambiguity" => Ok(Self::Ambiguity),
            "contradiction" => Ok(Self::Contradiction),
            "responsibility_sensitive" => Ok(Self::ResponsibilitySensitive),
            _ => Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum TenderQueryTreatment {
    InternalResolution,
    ExternalRfiDrafting,
    ApprovedAssumption,
    Qualification,
    Exclusion,
    Allowance,
    Blocked,
}

impl TenderQueryTreatment {
    fn as_str(self) -> &'static str {
        match self {
            Self::InternalResolution => "internal_resolution",
            Self::ExternalRfiDrafting => "external_rfi_drafting",
            Self::ApprovedAssumption => "approved_assumption",
            Self::Qualification => "qualification",
            Self::Exclusion => "exclusion",
            Self::Allowance => "allowance",
            Self::Blocked => "blocked",
        }
    }

    fn parse(value: &str) -> Result<Self, TenderCommandError> {
        match value {
            "internal_resolution" => Ok(Self::InternalResolution),
            "external_rfi_drafting" => Ok(Self::ExternalRfiDrafting),
            "approved_assumption" => Ok(Self::ApprovedAssumption),
            "qualification" => Ok(Self::Qualification),
            "exclusion" => Ok(Self::Exclusion),
            "allowance" => Ok(Self::Allowance),
            "blocked" => Ok(Self::Blocked),
            _ => Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
        }
    }

    pub(crate) fn permits_dependent_work(self) -> bool {
        !matches!(self, Self::ExternalRfiDrafting | Self::Blocked)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum TenderQueryStatus {
    Open,
    TreatmentProposed,
    Responded,
    TreatmentApproved,
    Closed,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TenderQueryTreatmentProposal {
    pub treatment: TenderQueryTreatment,
    pub rationale: String,
    pub proposed_by: String,
    pub proposed_by_run_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TenderQueryResponse {
    pub response_id: String,
    pub response: String,
    pub evidence: Vec<AgentTaskInputReference>,
    pub registered_by: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ApprovedQueryTreatment {
    pub decision_id: String,
    pub query_id: String,
    pub query_version: u32,
    pub treatment: TenderQueryTreatment,
    pub rationale: String,
    pub treatment_details: String,
    pub closes_query: bool,
    pub decided_by: String,
    pub acting_role: String,
    pub manifest_sha256: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TenderQueryInvalidation {
    pub invalidation_id: String,
    pub target_kind: String,
    pub target_id: String,
    pub target_version: Option<u32>,
    pub reason: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TenderQuery {
    pub query_id: String,
    pub version: u32,
    pub query_type: TenderQueryType,
    pub question: String,
    pub ambiguity_or_gap: String,
    pub owner_profile_id: String,
    pub owner_profile_version: u32,
    pub evidence: Vec<AgentTaskInputReference>,
    pub affected_records: Vec<TenderRecordVersionReference>,
    pub affected_task_keys: Vec<String>,
    pub due_at: String,
    pub material: bool,
    pub release_blocking: bool,
    pub proposed_treatments: Vec<TenderQueryTreatmentProposal>,
    pub responses: Vec<TenderQueryResponse>,
    pub approved_treatment: Option<ApprovedQueryTreatment>,
    pub invalidations: Vec<TenderQueryInvalidation>,
    pub status: TenderQueryStatus,
    pub overdue: bool,
    pub current: bool,
    pub source_run_id: Option<String>,
    pub created_by: String,
    pub manifest_sha256: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ProductionQueryContext {
    pub query_id: String,
    pub query_version: u32,
    pub owner_profile_id: String,
    pub owner_profile_version: u32,
    pub question: String,
    pub ambiguity_or_gap: String,
    pub evidence: Vec<AgentTaskInputReference>,
    pub affected_records: Vec<TenderRecordVersionReference>,
    pub affected_task_keys: Vec<String>,
    pub material: bool,
    pub release_blocking: bool,
    pub responses: Vec<TenderQueryResponse>,
    pub approved_treatment: Option<ApprovedQueryTreatment>,
    pub manifest_sha256: String,
    pub source_run_id: Option<String>,
}

impl ProductionQueryContext {
    pub(crate) fn blocks_dependent_work(&self) -> bool {
        self.approved_treatment
            .as_ref()
            .map_or(self.material || self.release_blocking, |decision| {
                !decision.treatment.permits_dependent_work()
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TenderQueryPage {
    pub query_register_open: bool,
    pub owner_profiles: Vec<AgentProfileVersionView>,
    pub production_task_keys: Vec<String>,
    pub items: Vec<TenderQuery>,
    pub next_cursor: Option<String>,
    pub total_current_count: u32,
    pub overdue_count: u32,
    pub release_blocking_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct TenderQueryTreatmentProposalInput {
    pub treatment: TenderQueryTreatment,
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct AgentTenderQueryProposal {
    pub query_type: TenderQueryType,
    pub question: String,
    pub ambiguity_or_gap: String,
    pub evidence: Vec<AgentTaskInputReference>,
    pub affected_records: Vec<TenderRecordVersionReference>,
    pub affected_task_keys: Vec<String>,
    pub due_at: String,
    pub material: bool,
    pub release_blocking: bool,
    pub proposed_treatments: Vec<TenderQueryTreatmentProposalInput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct AgentTenderQueryUpdate {
    pub query_id: String,
    pub base_version: u32,
    pub added_evidence: Vec<AgentTaskInputReference>,
    pub proposed_treatments: Vec<TenderQueryTreatmentProposalInput>,
    pub response: Option<String>,
    pub response_evidence: Vec<AgentTaskInputReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct CreateTenderQueryCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(skip)]
    pub query_type: TenderQueryType,
    #[garde(length(bytes, min = 1, max = 4000))]
    pub question: String,
    #[garde(length(bytes, min = 1, max = 4000))]
    pub ambiguity_or_gap: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub owner_profile_id: String,
    #[garde(range(min = 1))]
    pub owner_profile_version: u32,
    #[garde(skip)]
    pub evidence: Vec<AgentTaskInputReference>,
    #[garde(skip)]
    pub affected_records: Vec<TenderRecordVersionReference>,
    #[garde(skip)]
    pub affected_task_keys: Vec<String>,
    #[garde(length(bytes, min = 20, max = 32))]
    pub due_at: String,
    #[garde(skip)]
    pub material: bool,
    #[garde(skip)]
    pub release_blocking: bool,
    #[garde(skip)]
    pub proposed_treatments: Vec<TenderQueryTreatmentProposalInput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct ReviseTenderQueryCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub query_id: String,
    #[garde(range(min = 1, max = 32))]
    pub base_version: u32,
    #[garde(skip)]
    pub query_type: TenderQueryType,
    #[garde(length(bytes, min = 1, max = 4000))]
    pub question: String,
    #[garde(length(bytes, min = 1, max = 4000))]
    pub ambiguity_or_gap: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub owner_profile_id: String,
    #[garde(range(min = 1))]
    pub owner_profile_version: u32,
    #[garde(skip)]
    pub evidence: Vec<AgentTaskInputReference>,
    #[garde(skip)]
    pub affected_records: Vec<TenderRecordVersionReference>,
    #[garde(skip)]
    pub affected_task_keys: Vec<String>,
    #[garde(length(bytes, min = 20, max = 32))]
    pub due_at: String,
    #[garde(skip)]
    pub material: bool,
    #[garde(skip)]
    pub release_blocking: bool,
    #[garde(skip)]
    pub proposed_treatments: Vec<TenderQueryTreatmentProposalInput>,
    #[garde(length(bytes, max = 4000))]
    pub response: Option<String>,
    #[garde(skip)]
    pub response_evidence: Vec<AgentTaskInputReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct DecideTenderQueryTreatmentCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub query_id: String,
    #[garde(range(min = 1, max = 32))]
    pub query_version: u32,
    #[garde(skip)]
    pub treatment: TenderQueryTreatment,
    #[garde(length(bytes, min = 1, max = 4000))]
    pub rationale: String,
    #[garde(length(bytes, min = 1, max = 4000))]
    pub treatment_details: String,
    #[garde(skip)]
    pub closes_query: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct InspectTenderQueriesCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, max = 32))]
    pub cursor: Option<String>,
    #[garde(range(min = 1, max = 8))]
    pub limit: u32,
}

#[derive(Serialize)]
struct TenderQueryManifest<'a> {
    schema_version: u32,
    query_id: &'a str,
    version: u32,
    query_type: TenderQueryType,
    question: &'a str,
    ambiguity_or_gap: &'a str,
    owner_profile_id: &'a str,
    owner_profile_version: u32,
    evidence: &'a [AgentTaskInputReference],
    affected_records: &'a [TenderRecordVersionReference],
    affected_task_keys: &'a [String],
    invalidation_targets: &'a [QueryInvalidationTarget],
    due_at: &'a str,
    material: bool,
    release_blocking: bool,
    proposed_treatments: &'a [TenderQueryTreatmentProposal],
    responses: &'a [TenderQueryResponse],
    source_run_id: Option<&'a str>,
    created_by: &'a str,
    created_at: &'a str,
}

#[derive(Serialize)]
struct QueryTreatmentManifest<'a> {
    schema_version: u32,
    decision_id: &'a str,
    query_id: &'a str,
    query_version: u32,
    query_manifest_sha256: &'a str,
    treatment: TenderQueryTreatment,
    rationale: &'a str,
    treatment_details: &'a str,
    closes_query: bool,
    decided_by: &'a str,
    acting_role: &'a str,
    invalidation_targets: &'a [QueryInvalidationTarget],
    created_at: &'a str,
}

impl QuantixHost {
    pub fn create_tender_query(
        &self,
        command: CreateTenderQueryCommand,
    ) -> Result<TenderQuery, TenderCommandError> {
        super::require_setup(self)?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let mut store = lock_mutex_with_check(&store, &mut || budget.check())?;
        if command.validate().is_err() {
            store.record_query_denial(&tender_id, "create_tender_query", None, "command_shape")?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        store.create_tender_query(&tender_id, &command, budget)
    }

    pub fn revise_tender_query(
        &self,
        command: ReviseTenderQueryCommand,
    ) -> Result<TenderQuery, TenderCommandError> {
        super::require_setup(self)?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let mut store = lock_mutex_with_check(&store, &mut || budget.check())?;
        if command.validate().is_err() {
            store.record_query_denial(
                &tender_id,
                "revise_tender_query",
                Some(&command.query_id),
                "command_shape",
            )?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        store.revise_tender_query(&tender_id, &command, budget)
    }

    pub fn decide_tender_query_treatment(
        &self,
        command: DecideTenderQueryTreatmentCommand,
    ) -> Result<TenderQuery, TenderCommandError> {
        super::require_setup(self)?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let mut store = lock_mutex_with_check(&store, &mut || budget.check())?;
        if command.validate().is_err() {
            store.record_query_denial(
                &tender_id,
                "decide_tender_query_treatment",
                Some(&command.query_id),
                "command_shape",
            )?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        store.decide_tender_query_treatment(&tender_id, &command, budget)
    }

    pub fn inspect_tender_queries(
        &self,
        command: InspectTenderQueriesCommand,
    ) -> Result<TenderQueryPage, TenderCommandError> {
        super::require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let result = lock_mutex_with_check(&store, &mut || budget.check())?
            .inspect_tender_queries(&command, budget);
        result
    }
}

impl TenderStore {
    fn record_query_denial(
        &mut self,
        tender_id: &TenderId,
        command: &str,
        query_id: Option<&str>,
        reason: &str,
    ) -> Result<(), TenderCommandError> {
        self.require_storage_writable()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        append_query_denial(&transaction, tender_id, command, query_id, reason)?;
        transaction.commit().map_err(sql_error)
    }

    fn create_tender_query(
        &mut self,
        tender_id: &TenderId,
        command: &CreateTenderQueryCommand,
        budget: BidPackageOperationBudget,
    ) -> Result<TenderQuery, TenderCommandError> {
        self.require_change_intake_writable()?;
        budget.check()?;
        let query_id = random_identifier(&self.connection)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        if require_query_register_open(&transaction).is_err() {
            append_query_denial(
                &transaction,
                tender_id,
                "create_tender_query",
                None,
                "query_register_not_open",
            )?;
            transaction.commit().map_err(sql_error)?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let query_count: u32 = transaction
            .query_row("SELECT COUNT(*) FROM tender_queries", [], |row| row.get(0))
            .map_err(sql_error)?;
        if usize::try_from(query_count)
            .ok()
            .is_none_or(|count| count >= MAX_TENDER_QUERIES)
        {
            append_query_denial(
                &transaction,
                tender_id,
                "create_tender_query",
                None,
                "query_limit",
            )?;
            transaction.commit().map_err(sql_error)?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let created_at = sqlite_timestamp(&transaction)?;
        let proposed_treatments = command
            .proposed_treatments
            .iter()
            .map(|proposal| TenderQueryTreatmentProposal {
                treatment: proposal.treatment,
                rationale: proposal.rationale.trim().to_owned(),
                proposed_by: "engineer_user".into(),
                proposed_by_run_id: None,
            })
            .collect::<Vec<_>>();
        let candidate = QueryCandidateRef {
            question: &command.question,
            ambiguity_or_gap: &command.ambiguity_or_gap,
            owner_profile_id: &command.owner_profile_id,
            owner_profile_version: command.owner_profile_version,
            evidence: &command.evidence,
            affected_records: &command.affected_records,
            affected_task_keys: &command.affected_task_keys,
            due_at: &command.due_at,
            proposed_treatments: &proposed_treatments,
            responses: &[],
        };
        let candidate_validation =
            validate_query_candidate_with_check(&transaction, candidate, &mut || budget.check())
                .and_then(|_| {
                    validate_query_publication_targets_with_check(
                        &transaction,
                        &command.affected_task_keys,
                        &mut || budget.check(),
                    )
                });
        if let Err(error) = candidate_validation {
            if error.code == TenderErrorCode::OperationTimedOut {
                return Err(error);
            }
            append_query_denial(
                &transaction,
                tender_id,
                "create_tender_query",
                None,
                "query_candidate_invalid",
            )?;
            transaction.commit().map_err(sql_error)?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        if !query_context_candidates_fit(
            &transaction,
            &[(None, query_candidate_context_bound(candidate)?)],
        )? {
            append_query_denial(
                &transaction,
                tender_id,
                "create_tender_query",
                None,
                "query_context_capacity_reached",
            )?;
            transaction.commit().map_err(sql_error)?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        if !query_publication_has_capacity(
            &transaction,
            &command.affected_records,
            &command.affected_task_keys,
        )? {
            append_query_denial(
                &transaction,
                tender_id,
                "create_tender_query",
                None,
                "query_capacity_reached",
            )?;
            transaction.commit().map_err(sql_error)?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        transaction
            .execute(
                "INSERT INTO tender_queries (query_id, created_at) VALUES (?1, ?2)",
                params![query_id, created_at],
            )
            .map_err(sql_error)?;
        let invalidation_targets = collect_query_invalidation_targets_with_check(
            &transaction,
            &command.affected_records,
            &command.affected_task_keys,
            &mut || budget.check(),
        )?;
        let manifest_sha256 = insert_query_version(
            &transaction,
            QueryVersionInsert {
                query_id: &query_id,
                version: 1,
                query_type: command.query_type,
                question: command.question.trim(),
                ambiguity_or_gap: command.ambiguity_or_gap.trim(),
                owner_profile_id: &command.owner_profile_id,
                owner_profile_version: command.owner_profile_version,
                evidence: &command.evidence,
                affected_records: &command.affected_records,
                affected_task_keys: &command.affected_task_keys,
                invalidation_targets: &invalidation_targets,
                due_at: &command.due_at,
                material: command.material,
                release_blocking: command.release_blocking,
                proposed_treatments: &proposed_treatments,
                responses: &[],
                source_run_id: None,
                created_by: "engineer_user",
                created_at: &created_at,
            },
        )?;
        transaction
            .execute(
                "INSERT INTO tender_query_heads (query_id, current_version) VALUES (?1, 1)",
                [&query_id],
            )
            .map_err(sql_error)?;
        invalidate_query_targets(
            &transaction,
            QueryTargetInvalidation {
                query_id: &query_id,
                query_version: 1,
                targets: &invalidation_targets,
                reason: "query_opened",
                blocks_dependent_work: command.material || command.release_blocking,
                created_at: &created_at,
            },
        )?;
        append_query_event(
            &transaction,
            tender_id,
            "tender_query_created",
            &query_id,
            1,
            &manifest_sha256,
            &created_at,
        )?;
        budget.check()?;
        transaction.commit().map_err(sql_error)?;
        self.load_tender_query(&query_id, 1, true, budget)
    }

    fn revise_tender_query(
        &mut self,
        tender_id: &TenderId,
        command: &ReviseTenderQueryCommand,
        budget: BidPackageOperationBudget,
    ) -> Result<TenderQuery, TenderCommandError> {
        self.require_change_intake_writable()?;
        budget.check()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        if require_query_register_open(&transaction).is_err() {
            append_query_denial(
                &transaction,
                tender_id,
                "revise_tender_query",
                Some(&command.query_id),
                "query_register_not_open",
            )?;
            transaction.commit().map_err(sql_error)?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let Some(current) =
            load_query_version_row(&transaction, &command.query_id, command.base_version)?
        else {
            append_query_denial(
                &transaction,
                tender_id,
                "revise_tender_query",
                Some(&command.query_id),
                "query_version_not_found",
            )?;
            transaction.commit().map_err(sql_error)?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        };
        let mut released_task_keys = current
            .affected_task_keys
            .iter()
            .filter(|task_key| !command.affected_task_keys.contains(task_key))
            .cloned()
            .collect::<Vec<_>>();
        if !command.material && !command.release_blocking {
            released_task_keys.extend(command.affected_task_keys.iter().cloned());
        }
        released_task_keys.sort();
        released_task_keys.dedup();
        let head: Option<u32> = transaction
            .query_row(
                "SELECT current_version FROM tender_query_heads WHERE query_id = ?1",
                [&command.query_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_error)?;
        if head != Some(command.base_version) || command.base_version >= MAX_QUERY_VERSIONS {
            append_query_denial(
                &transaction,
                tender_id,
                "revise_tender_query",
                Some(&command.query_id),
                "query_version_not_current",
            )?;
            transaction.commit().map_err(sql_error)?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let created_at = sqlite_timestamp(&transaction)?;
        let mut responses = current.responses;
        if let Some(response) = command.response.as_deref() {
            responses.push(TenderQueryResponse {
                response_id: random_identifier(&transaction)?,
                response: response.trim().to_owned(),
                evidence: command.response_evidence.clone(),
                registered_by: "engineer_user".into(),
                created_at: created_at.clone(),
            });
        } else if !command.response_evidence.is_empty() {
            append_query_denial(
                &transaction,
                tender_id,
                "revise_tender_query",
                Some(&command.query_id),
                "response_evidence_without_response",
            )?;
            transaction.commit().map_err(sql_error)?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let mut inherited_proposals = current.proposed_treatments.clone();
        let proposed_treatments = command
            .proposed_treatments
            .iter()
            .map(|candidate| {
                inherited_proposals
                    .iter()
                    .position(|existing| {
                        candidate.treatment == existing.treatment
                            && candidate.rationale.trim() == existing.rationale
                    })
                    .map(|index| inherited_proposals.remove(index))
                    .unwrap_or_else(|| TenderQueryTreatmentProposal {
                        treatment: candidate.treatment,
                        rationale: candidate.rationale.trim().to_owned(),
                        proposed_by: "engineer_user".into(),
                        proposed_by_run_id: None,
                    })
            })
            .collect::<Vec<_>>();
        let candidate = QueryCandidateRef {
            question: &command.question,
            ambiguity_or_gap: &command.ambiguity_or_gap,
            owner_profile_id: &command.owner_profile_id,
            owner_profile_version: command.owner_profile_version,
            evidence: &command.evidence,
            affected_records: &command.affected_records,
            affected_task_keys: &command.affected_task_keys,
            due_at: &command.due_at,
            proposed_treatments: &proposed_treatments,
            responses: &responses,
        };
        let candidate_validation =
            validate_query_candidate_with_check(&transaction, candidate, &mut || budget.check())
                .and_then(|_| {
                    validate_query_publication_targets_with_check(
                        &transaction,
                        &command.affected_task_keys,
                        &mut || budget.check(),
                    )
                });
        if let Err(error) = candidate_validation {
            if error.code == TenderErrorCode::OperationTimedOut {
                return Err(error);
            }
            append_query_denial(
                &transaction,
                tender_id,
                "revise_tender_query",
                Some(&command.query_id),
                "query_candidate_invalid",
            )?;
            transaction.commit().map_err(sql_error)?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        if !query_context_candidates_fit(
            &transaction,
            &[(
                Some(command.query_id.as_str()),
                query_candidate_context_bound(candidate)?,
            )],
        )? {
            append_query_denial(
                &transaction,
                tender_id,
                "revise_tender_query",
                Some(&command.query_id),
                "query_context_capacity_reached",
            )?;
            transaction.commit().map_err(sql_error)?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        if !query_publication_has_capacity(
            &transaction,
            &command.affected_records,
            &command.affected_task_keys,
        )? {
            append_query_denial(
                &transaction,
                tender_id,
                "revise_tender_query",
                Some(&command.query_id),
                "query_capacity_reached",
            )?;
            transaction.commit().map_err(sql_error)?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let version = command.base_version + 1;
        let invalidation_targets = collect_query_invalidation_targets_with_check(
            &transaction,
            &command.affected_records,
            &command.affected_task_keys,
            &mut || budget.check(),
        )?;
        let manifest_sha256 = insert_query_version(
            &transaction,
            QueryVersionInsert {
                query_id: &command.query_id,
                version,
                query_type: command.query_type,
                question: command.question.trim(),
                ambiguity_or_gap: command.ambiguity_or_gap.trim(),
                owner_profile_id: &command.owner_profile_id,
                owner_profile_version: command.owner_profile_version,
                evidence: &command.evidence,
                affected_records: &command.affected_records,
                affected_task_keys: &command.affected_task_keys,
                invalidation_targets: &invalidation_targets,
                due_at: &command.due_at,
                material: command.material,
                release_blocking: command.release_blocking,
                proposed_treatments: &proposed_treatments,
                responses: &responses,
                source_run_id: None,
                created_by: "engineer_user",
                created_at: &created_at,
            },
        )?;
        if transaction
            .execute(
                "UPDATE tender_query_heads SET current_version = ?2
                 WHERE query_id = ?1 AND current_version = ?3",
                params![command.query_id, version, command.base_version],
            )
            .map_err(sql_error)?
            != 1
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let reason = if command.response.is_some() {
            "response_added"
        } else if current.evidence != command.evidence {
            "evidence_changed"
        } else {
            "treatment_changed"
        };
        invalidate_query_targets(
            &transaction,
            QueryTargetInvalidation {
                query_id: &command.query_id,
                query_version: version,
                targets: &invalidation_targets,
                reason,
                blocks_dependent_work: command.material || command.release_blocking,
                created_at: &created_at,
            },
        )?;
        release_query_blocked_tasks(&transaction, &released_task_keys, &created_at)?;
        append_query_event(
            &transaction,
            tender_id,
            "tender_query_revised",
            &command.query_id,
            version,
            &manifest_sha256,
            &created_at,
        )?;
        budget.check()?;
        transaction.commit().map_err(sql_error)?;
        self.load_tender_query(&command.query_id, version, true, budget)
    }

    fn decide_tender_query_treatment(
        &mut self,
        tender_id: &TenderId,
        command: &DecideTenderQueryTreatmentCommand,
        budget: BidPackageOperationBudget,
    ) -> Result<TenderQuery, TenderCommandError> {
        self.require_change_intake_writable()?;
        budget.check()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let Some(current) =
            load_query_version_row(&transaction, &command.query_id, command.query_version)?
        else {
            append_query_denial(
                &transaction,
                tender_id,
                "decide_tender_query_treatment",
                Some(&command.query_id),
                "query_version_not_found",
            )?;
            transaction.commit().map_err(sql_error)?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        };
        let head: Option<u32> = transaction
            .query_row(
                "SELECT current_version FROM tender_query_heads WHERE query_id = ?1",
                [&command.query_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_error)?;
        let already_decided: bool = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM tender_query_treatment_decisions
                 WHERE query_id = ?1 AND query_version = ?2)",
                params![command.query_id, command.query_version],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        let decision_valid = head == Some(command.query_version)
            && !already_decided
            && (!command.closes_query
                || (command.treatment.permits_dependent_work()
                    && (command.treatment != TenderQueryTreatment::InternalResolution
                        || !current.responses.is_empty())));
        if !decision_valid {
            append_query_denial(
                &transaction,
                tender_id,
                "decide_tender_query_treatment",
                Some(&command.query_id),
                "query_treatment_guard_failed",
            )?;
            transaction.commit().map_err(sql_error)?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let decision_id = random_identifier(&transaction)?;
        let created_at = sqlite_timestamp(&transaction)?;
        let current_targets = collect_query_invalidation_targets_with_check(
            &transaction,
            &current.affected_records,
            &current.affected_task_keys,
            &mut || budget.check(),
        )?;
        let decision_invalidation_targets = current_targets
            .into_iter()
            .filter(|target| !current.invalidation_targets.contains(target))
            .collect::<Vec<_>>();
        let invalidation_count: u32 = transaction
            .query_row(
                "SELECT COUNT(*) FROM tender_query_target_invalidations",
                [],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if invalidation_count
            .saturating_add(u32::try_from(decision_invalidation_targets.len()).unwrap_or(u32::MAX))
            > MAX_QUERY_INVALIDATIONS_TOTAL
        {
            append_query_denial(
                &transaction,
                tender_id,
                "decide_tender_query_treatment",
                Some(&command.query_id),
                "query_capacity_reached",
            )?;
            transaction.commit().map_err(sql_error)?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let manifest = QueryTreatmentManifest {
            schema_version: 1,
            decision_id: &decision_id,
            query_id: &command.query_id,
            query_version: command.query_version,
            query_manifest_sha256: &current.manifest_sha256,
            treatment: command.treatment,
            rationale: command.rationale.trim(),
            treatment_details: command.treatment_details.trim(),
            closes_query: command.closes_query,
            decided_by: "engineer_user",
            acting_role: "tendering_manager",
            invalidation_targets: &decision_invalidation_targets,
            created_at: &created_at,
        };
        let manifest_json = canonical_json(&manifest)?;
        let manifest_sha256 = sha256_hex(manifest_json.as_bytes());
        let tender_revision: u32 = transaction
            .query_row(
                "SELECT current_revision FROM tender WHERE singleton = 1",
                [],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        let audit_sequence = append_audit_event_with_sequence(
            &transaction,
            tender_id.as_str(),
            "tender_query_treatment_decided",
            tender_revision,
            json!({
                "acting_role": "tendering_manager",
                "closes_query": command.closes_query,
                "decided_by": "engineer_user",
                "decision_id": decision_id,
                "manifest_sha256": manifest_sha256,
                "query_id": command.query_id,
                "query_version": command.query_version.to_string(),
                "treatment": command.treatment.as_str(),
            }),
            &created_at,
        )?;
        transaction
            .execute(
                "INSERT INTO tender_query_treatment_decisions (
                   decision_id, query_id, query_version, treatment, rationale,
                   treatment_details, closes_query, decided_by, acting_role,
                   audit_sequence, invalidation_targets_json, manifest_json,
                   manifest_sha256, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7,
                           'engineer_user', 'tendering_manager', ?8, ?9, ?10, ?11, ?12)",
                params![
                    decision_id,
                    command.query_id,
                    command.query_version,
                    command.treatment.as_str(),
                    command.rationale.trim(),
                    command.treatment_details.trim(),
                    command.closes_query,
                    audit_sequence,
                    canonical_json(&decision_invalidation_targets)?,
                    manifest_json,
                    manifest_sha256,
                    created_at,
                ],
            )
            .map_err(sql_error)?;
        invalidate_query_targets(
            &transaction,
            QueryTargetInvalidation {
                query_id: &command.query_id,
                query_version: command.query_version,
                targets: &decision_invalidation_targets,
                reason: "treatment_changed",
                blocks_dependent_work: !command.treatment.permits_dependent_work(),
                created_at: &created_at,
            },
        )?;
        if command.treatment.permits_dependent_work() {
            release_query_blocked_tasks(&transaction, &current.affected_task_keys, &created_at)?;
        }
        budget.check()?;
        transaction.commit().map_err(sql_error)?;
        self.load_tender_query(&command.query_id, command.query_version, true, budget)
    }

    fn inspect_tender_queries(
        &self,
        command: &InspectTenderQueriesCommand,
        budget: BidPackageOperationBudget,
    ) -> Result<TenderQueryPage, TenderCommandError> {
        budget.check()?;
        if command.limit == 0 || command.limit > MAX_QUERY_PAGE_ITEMS {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let open: bool = self
            .connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM query_register WHERE singleton = 1)",
                [],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if !open {
            return Ok(TenderQueryPage {
                query_register_open: false,
                owner_profiles: Vec::new(),
                production_task_keys: Vec::new(),
                items: Vec::new(),
                next_cursor: None,
                total_current_count: 0,
                overdue_count: 0,
                release_blocking_count: 0,
            });
        }
        let cursor_rowid = command
            .cursor
            .as_deref()
            .map(|cursor| {
                if !valid_identifier(cursor) {
                    return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
                }
                self.connection
                    .query_row(
                        "SELECT rowid FROM tender_queries WHERE query_id = ?1",
                        [cursor],
                        |row| row.get::<_, i64>(0),
                    )
                    .optional()
                    .map_err(sql_error)?
                    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))
            })
            .transpose()?;
        let mut statement = self
            .connection
            .prepare(
                "SELECT queries.query_id, heads.current_version
                 FROM tender_queries AS queries
                 JOIN tender_query_heads AS heads ON heads.query_id = queries.query_id
                 WHERE (?1 IS NULL OR queries.rowid < ?1)
                 ORDER BY queries.rowid DESC LIMIT ?2",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map(params![cursor_rowid, command.limit + 1], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?))
            })
            .map_err(sql_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql_error)?;
        let has_more = rows.len() > command.limit as usize;
        let selected = rows
            .into_iter()
            .take(command.limit as usize)
            .collect::<Vec<_>>();
        let mut items = Vec::with_capacity(selected.len());
        for (query_id, version) in &selected {
            budget.check()?;
            items.push(self.load_tender_query(query_id, *version, true, budget)?);
        }
        let (total_current_count, overdue_count, release_blocking_count): (u32, u32, u32) = self
            .connection
            .query_row(
                "SELECT COUNT(*),
                        COALESCE(SUM(CASE WHEN julianday(versions.due_at) < julianday('now')
                                  AND (decisions.decision_id IS NULL OR decisions.closes_query = 0)
                                 THEN 1 ELSE 0 END), 0),
                        COALESCE(SUM(CASE WHEN versions.release_blocking = 1
                                  AND (decisions.decision_id IS NULL
                                       OR decisions.treatment IN ('external_rfi_drafting', 'blocked'))
                                 THEN 1 ELSE 0 END), 0)
                 FROM tender_query_heads AS heads
                 JOIN tender_query_versions AS versions
                   ON versions.query_id = heads.query_id AND versions.version = heads.current_version
                 LEFT JOIN tender_query_treatment_decisions AS decisions
                   ON decisions.query_id = versions.query_id
                  AND decisions.query_version = versions.version",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .map_err(sql_error)?;
        let mut profile_statement = self
            .connection
            .prepare(
                "SELECT profile_id, current_version FROM agent_profile_heads
                 WHERE status IN ('proposed', 'active', 'suspended')
                 ORDER BY profile_id LIMIT 32",
            )
            .map_err(sql_error)?;
        let profile_refs = profile_statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?))
            })
            .map_err(sql_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql_error)?;
        let owner_profiles = profile_refs
            .into_iter()
            .map(|profile| load_profile(&self.connection, profile))
            .collect::<Result<Vec<_>, _>>()?;
        let mut task_statement = self
            .connection
            .prepare(
                "SELECT tasks.task_key FROM production_tasks AS tasks
                 JOIN production_activations AS activations
                   ON activations.activation_id = tasks.activation_id
                 WHERE activations.status = 'active' ORDER BY tasks.task_key LIMIT 257",
            )
            .map_err(sql_error)?;
        let mut production_task_keys = vec![ALL_PRODUCTION_TASKS_SCOPE.to_owned()];
        production_task_keys.extend(
            task_statement
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(sql_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(sql_error)?,
        );
        if production_task_keys.len() > MAX_QUERY_PRODUCTION_TARGETS + 1 {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        Ok(TenderQueryPage {
            query_register_open: true,
            owner_profiles,
            production_task_keys,
            next_cursor: has_more
                .then(|| selected.last().map(|row| row.0.clone()))
                .flatten(),
            items,
            total_current_count,
            overdue_count,
            release_blocking_count,
        })
    }

    fn load_tender_query(
        &self,
        query_id: &str,
        version: u32,
        current: bool,
        budget: BidPackageOperationBudget,
    ) -> Result<TenderQuery, TenderCommandError> {
        budget.check()?;
        let row = load_query_version_row(&self.connection, query_id, version)?
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let approved_treatment = load_query_decision(&self.connection, query_id, version)?;
        let mut statement = self
            .connection
            .prepare(
                "SELECT invalidation_id, target_kind, target_id, target_version, reason, created_at
                 FROM tender_query_target_invalidations
                 WHERE query_id = ?1 AND query_version = ?2 ORDER BY rowid",
            )
            .map_err(sql_error)?;
        let invalidations = statement
            .query_map(params![query_id, version], |row| {
                Ok(TenderQueryInvalidation {
                    invalidation_id: row.get(0)?,
                    target_kind: row.get(1)?,
                    target_id: row.get(2)?,
                    target_version: match row.get::<_, u32>(3)? {
                        0 => None,
                        version => Some(version),
                    },
                    reason: row.get(4)?,
                    created_at: row.get(5)?,
                })
            })
            .map_err(sql_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql_error)?;
        let overdue: bool = self
            .connection
            .query_row(
                "SELECT julianday(?1) < julianday('now')",
                [&row.due_at],
                |result| result.get(0),
            )
            .map_err(sql_error)?;
        let status = query_status(&row, approved_treatment.as_ref());
        Ok(TenderQuery {
            query_id: query_id.to_owned(),
            version,
            query_type: row.query_type,
            question: row.question,
            ambiguity_or_gap: row.ambiguity_or_gap,
            owner_profile_id: row.owner_profile_id,
            owner_profile_version: row.owner_profile_version,
            evidence: row.evidence,
            affected_records: row.affected_records,
            affected_task_keys: row.affected_task_keys,
            due_at: row.due_at,
            material: row.material,
            release_blocking: row.release_blocking,
            proposed_treatments: row.proposed_treatments,
            responses: row.responses,
            approved_treatment,
            invalidations,
            status,
            overdue: overdue && status != TenderQueryStatus::Closed,
            current,
            source_run_id: row.source_run_id,
            created_by: row.created_by,
            manifest_sha256: row.manifest_sha256,
            created_at: row.created_at,
        })
    }

    pub(crate) fn tender_query_manifests_are_valid_with_check(
        &self,
        check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
    ) -> Result<bool, TenderCommandError> {
        check()?;
        let (intake_count, register_count, register_valid): (u32, u32, bool) = self
            .connection
            .query_row(
                "SELECT
                   (SELECT COUNT(*) FROM intake_runs),
                   (SELECT COUNT(*) FROM query_register),
                   EXISTS(
                     SELECT 1 FROM query_register AS register
                     JOIN intake_runs AS intake
                       ON intake.intake_id = register.opened_by_intake_id
                      AND intake.created_at = register.opened_at
                     WHERE register.singleton = 1
                       AND intake.rowid = (SELECT MIN(rowid) FROM intake_runs)
                   )",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .map_err(sql_error)?;
        if (intake_count == 0 && (register_count != 0 || register_valid))
            || (intake_count > 0 && (register_count != 1 || !register_valid))
        {
            return Ok(false);
        }
        let counts: (u32, u32, u32, u32) = self
            .connection
            .query_row(
                "SELECT (SELECT COUNT(*) FROM tender_queries),
                        (SELECT COUNT(*) FROM tender_query_heads),
                        (SELECT COUNT(*) FROM tender_query_versions),
                        (SELECT COUNT(*) FROM tender_query_target_invalidations)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .map_err(sql_error)?;
        if counts.0 != counts.1
            || counts.2 > MAX_QUERY_VERSIONS_TOTAL
            || counts.3 > MAX_QUERY_INVALIDATIONS_TOTAL
            || usize::try_from(counts.0)
                .ok()
                .is_none_or(|count| count > MAX_TENDER_QUERIES)
        {
            return Ok(false);
        }
        if stored_query_context_bounds(&self.connection)?.0 > MAX_PRODUCTION_QUERY_CONTEXT_BYTES {
            return Ok(false);
        }
        let mut statement = self
            .connection
            .prepare(
                "SELECT queries.query_id, heads.current_version
                 FROM tender_queries AS queries
                 JOIN tender_query_heads AS heads ON heads.query_id = queries.query_id
                 ORDER BY queries.rowid",
            )
            .map_err(sql_error)?;
        let heads = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?))
            })
            .map_err(sql_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql_error)?;
        for (query_id, head) in heads {
            check()?;
            if head == 0 || head > MAX_QUERY_VERSIONS {
                return Ok(false);
            }
            let version_count: u32 = self
                .connection
                .query_row(
                    "SELECT COUNT(*) FROM tender_query_versions WHERE query_id = ?1",
                    [&query_id],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            if version_count != head {
                return Ok(false);
            }
            for version in 1..=head {
                check()?;
                let Some(row) = load_query_version_row(&self.connection, &query_id, version)?
                else {
                    return Ok(false);
                };
                if validate_query_candidate_with_check(
                    &self.connection,
                    QueryCandidateRef {
                        question: &row.question,
                        ambiguity_or_gap: &row.ambiguity_or_gap,
                        owner_profile_id: &row.owner_profile_id,
                        owner_profile_version: row.owner_profile_version,
                        evidence: &row.evidence,
                        affected_records: &row.affected_records,
                        affected_task_keys: &row.affected_task_keys,
                        due_at: &row.due_at,
                        proposed_treatments: &row.proposed_treatments,
                        responses: &row.responses,
                    },
                    check,
                )
                .is_err()
                {
                    return Ok(false);
                }
                let expected_manifest = canonical_json(&TenderQueryManifest {
                    schema_version: 1,
                    query_id: &query_id,
                    version,
                    query_type: row.query_type,
                    question: &row.question,
                    ambiguity_or_gap: &row.ambiguity_or_gap,
                    owner_profile_id: &row.owner_profile_id,
                    owner_profile_version: row.owner_profile_version,
                    evidence: &row.evidence,
                    affected_records: &row.affected_records,
                    affected_task_keys: &row.affected_task_keys,
                    invalidation_targets: &row.invalidation_targets,
                    due_at: &row.due_at,
                    material: row.material,
                    release_blocking: row.release_blocking,
                    proposed_treatments: &row.proposed_treatments,
                    responses: &row.responses,
                    source_run_id: row.source_run_id.as_deref(),
                    created_by: &row.created_by,
                    created_at: &row.created_at,
                })?;
                if row.manifest_json != expected_manifest
                    || row.manifest_sha256 != sha256_hex(expected_manifest.as_bytes())
                    || !query_source_is_attributable(&self.connection, &query_id, version, &row)?
                    || !query_version_audit_is_valid(&self.connection, &query_id, version, &row)?
                    || !query_decision_is_valid(&self.connection, &query_id, version, &row)?
                    || !query_invalidations_are_valid(
                        &self.connection,
                        &query_id,
                        version,
                        &row,
                        check,
                    )?
                {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }
}

#[derive(Clone, Copy)]
struct QueryCandidateRef<'a> {
    question: &'a str,
    ambiguity_or_gap: &'a str,
    owner_profile_id: &'a str,
    owner_profile_version: u32,
    evidence: &'a [AgentTaskInputReference],
    affected_records: &'a [TenderRecordVersionReference],
    affected_task_keys: &'a [String],
    due_at: &'a str,
    proposed_treatments: &'a [TenderQueryTreatmentProposal],
    responses: &'a [TenderQueryResponse],
}

struct QueryVersionInsert<'a> {
    query_id: &'a str,
    version: u32,
    query_type: TenderQueryType,
    question: &'a str,
    ambiguity_or_gap: &'a str,
    owner_profile_id: &'a str,
    owner_profile_version: u32,
    evidence: &'a [AgentTaskInputReference],
    affected_records: &'a [TenderRecordVersionReference],
    affected_task_keys: &'a [String],
    invalidation_targets: &'a [QueryInvalidationTarget],
    due_at: &'a str,
    material: bool,
    release_blocking: bool,
    proposed_treatments: &'a [TenderQueryTreatmentProposal],
    responses: &'a [TenderQueryResponse],
    source_run_id: Option<&'a str>,
    created_by: &'a str,
    created_at: &'a str,
}

struct QueryVersionRow {
    query_type: TenderQueryType,
    question: String,
    ambiguity_or_gap: String,
    owner_profile_id: String,
    owner_profile_version: u32,
    evidence: Vec<AgentTaskInputReference>,
    affected_records: Vec<TenderRecordVersionReference>,
    affected_task_keys: Vec<String>,
    invalidation_targets: Vec<QueryInvalidationTarget>,
    due_at: String,
    material: bool,
    release_blocking: bool,
    proposed_treatments: Vec<TenderQueryTreatmentProposal>,
    responses: Vec<TenderQueryResponse>,
    source_run_id: Option<String>,
    created_by: String,
    manifest_json: String,
    manifest_sha256: String,
    created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
struct QueryInvalidationTarget {
    target_kind: String,
    target_id: String,
    target_version: Option<u32>,
}

fn validate_query_candidate(
    transaction: &rusqlite::Connection,
    candidate: QueryCandidateRef<'_>,
) -> Result<(), TenderCommandError> {
    validate_query_candidate_with_check(transaction, candidate, &mut || Ok(()))
}

fn validate_query_candidate_with_check(
    transaction: &rusqlite::Connection,
    candidate: QueryCandidateRef<'_>,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<(), TenderCommandError> {
    check()?;
    let profile = load_profile(
        transaction,
        (
            candidate.owner_profile_id.to_owned(),
            candidate.owner_profile_version,
        ),
    )?;
    let due_valid: bool = transaction
        .query_row(
            "SELECT julianday(?1) IS NOT NULL",
            [candidate.due_at],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    if profile.profile_id != candidate.owner_profile_id
        || candidate.question.trim().is_empty()
        || candidate.question.len() > 4_000
        || candidate.ambiguity_or_gap.trim().is_empty()
        || candidate.ambiguity_or_gap.len() > 4_000
        || !due_valid
        || candidate.evidence.is_empty()
        || candidate.evidence.len() > MAX_QUERY_EVIDENCE
        || candidate.affected_records.len() > MAX_QUERY_AFFECTED_RECORDS
        || candidate.affected_task_keys.is_empty()
        || candidate.affected_task_keys.len() > MAX_QUERY_AFFECTED_TASKS
        || candidate.proposed_treatments.len() > MAX_QUERY_PROPOSED_TREATMENTS
        || candidate.responses.len() > MAX_QUERY_RESPONSES
        || candidate
            .affected_task_keys
            .iter()
            .any(|task| task.trim().is_empty() || task.len() > 200)
        || candidate.proposed_treatments.iter().any(|proposal| {
            proposal.rationale.trim().is_empty()
                || proposal.rationale.len() > 4_000
                || proposal.proposed_by.trim().is_empty()
                || proposal.proposed_by.len() > 200
                || proposal
                    .proposed_by_run_id
                    .as_deref()
                    .is_some_and(|run_id| !valid_identifier(run_id))
        })
        || candidate.responses.iter().any(|response| {
            !valid_identifier(&response.response_id)
                || response.response.trim().is_empty()
                || response.response.len() > 4_000
                || response.registered_by.trim().is_empty()
                || response.registered_by.len() > 200
                || response.evidence.is_empty()
                || response.evidence.len() > MAX_QUERY_EVIDENCE
                || !references_are_unique(&response.evidence)
        })
        || !references_are_unique(candidate.evidence)
        || !records_are_unique(candidate.affected_records)
        || !strings_are_unique(candidate.affected_task_keys)
        || candidate
            .responses
            .iter()
            .map(|response| &response.response_id)
            .collect::<HashSet<_>>()
            .len()
            != candidate.responses.len()
    {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    for reference in candidate.evidence.iter().chain(
        candidate
            .responses
            .iter()
            .flat_map(|response| response.evidence.iter()),
    ) {
        check()?;
        if !query_evidence_reference_exists(transaction, reference)? {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
    }
    for record in candidate.affected_records {
        check()?;
        let exists: bool = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM tender_record_versions
                 WHERE record_id = ?1 AND version = ?2)",
                params![record.record_id, record.version],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if !exists {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
    }
    Ok(())
}

fn query_candidate_context_bound(
    candidate: QueryCandidateRef<'_>,
) -> Result<usize, TenderCommandError> {
    Ok(candidate
        .question
        .len()
        .saturating_add(candidate.ambiguity_or_gap.len())
        .saturating_add(canonical_json(candidate.evidence)?.len())
        .saturating_add(canonical_json(candidate.affected_records)?.len())
        .saturating_add(canonical_json(candidate.affected_task_keys)?.len())
        .saturating_add(canonical_json(candidate.responses)?.len())
        .saturating_add(QUERY_CONTEXT_DECISION_AND_METADATA_RESERVE))
}

fn stored_query_context_bounds(
    connection: &rusqlite::Connection,
) -> Result<(usize, HashMap<String, usize>), TenderCommandError> {
    let mut statement = connection
        .prepare(
            "SELECT query_id,
                    MAX(length(CAST(question AS BLOB))
                      + length(CAST(ambiguity_or_gap AS BLOB))
                      + length(CAST(evidence_json AS BLOB))
                      + length(CAST(affected_records_json AS BLOB))
                      + length(CAST(affected_task_keys_json AS BLOB))
                      + length(CAST(responses_json AS BLOB))
                      + ?1)
             FROM tender_query_versions GROUP BY query_id ORDER BY query_id",
        )
        .map_err(sql_error)?;
    let rows = statement
        .query_map(
            [u32::try_from(QUERY_CONTEXT_DECISION_AND_METADATA_RESERVE)
                .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )
        .map_err(sql_error)?;
    let mut total = 0usize;
    let mut maxima = HashMap::new();
    for row in rows {
        let (query_id, bound) = row.map_err(sql_error)?;
        let bound = usize::try_from(bound)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        total = total.saturating_add(bound);
        maxima.insert(query_id, bound);
    }
    Ok((total, maxima))
}

fn query_context_candidates_fit(
    connection: &rusqlite::Connection,
    candidates: &[(Option<&str>, usize)],
) -> Result<bool, TenderCommandError> {
    let (mut total, mut maxima) = stored_query_context_bounds(connection)?;
    for (query_id, bound) in candidates {
        if let Some(query_id) = query_id {
            let prior = maxima.get(*query_id).copied().unwrap_or_default();
            let next = prior.max(*bound);
            total = total.saturating_sub(prior).saturating_add(next);
            maxima.insert((*query_id).to_owned(), next);
        } else {
            total = total.saturating_add(*bound);
        }
        if total > MAX_PRODUCTION_QUERY_CONTEXT_BYTES {
            return Ok(false);
        }
    }
    Ok(true)
}

fn validate_query_publication_targets(
    connection: &rusqlite::Connection,
    affected_task_keys: &[String],
) -> Result<(), TenderCommandError> {
    validate_query_publication_targets_with_check(connection, affected_task_keys, &mut || Ok(()))
}

fn validate_query_publication_targets_with_check(
    connection: &rusqlite::Connection,
    affected_task_keys: &[String],
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<(), TenderCommandError> {
    for task_key in affected_task_keys {
        check()?;
        if task_key == ALL_PRODUCTION_TASKS_SCOPE {
            continue;
        }
        let exists: bool = connection
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM production_tasks AS tasks
                   JOIN production_activations AS activations
                     ON activations.activation_id = tasks.activation_id
                   WHERE activations.status = 'active' AND tasks.task_key = ?1
                   UNION ALL
                   SELECT 1 FROM work_plan_heads AS heads
                   JOIN work_plan_versions AS versions
                     ON versions.plan_id = heads.plan_id
                    AND versions.version = heads.current_version
                   JOIN work_plan_approvals AS approvals
                     ON approvals.plan_id = versions.plan_id
                    AND approvals.plan_version = versions.version
                    AND approvals.decision = 'approve'
                   JOIN json_each(versions.tasks_json) AS task
                   WHERE json_extract(task.value, '$.task_key') = ?1
                 )",
                [task_key],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if !exists {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
    }
    Ok(())
}

fn query_evidence_reference_exists(
    connection: &rusqlite::Connection,
    reference: &AgentTaskInputReference,
) -> Result<bool, TenderCommandError> {
    if reference.version == 0 || reference.reference.trim().is_empty() {
        return Ok(false);
    }
    match reference.kind.as_str() {
        "tender_revision" => connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM tender_revisions
                 WHERE tender_id = ?1 AND revision = ?2)",
                params![reference.reference, reference.version],
                |row| row.get(0),
            )
            .map_err(sql_error),
        "bid_decision_package" => connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM bid_decision_package_versions
                 WHERE package_id = ?1 AND version = ?2)",
                params![reference.reference, reference.version],
                |row| row.get(0),
            )
            .map_err(sql_error),
        "tender_record_version" => connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM tender_record_versions
                 WHERE record_id = ?1 AND version = ?2)",
                params![reference.reference, reference.version],
                |row| row.get(0),
            )
            .map_err(sql_error),
        "production_artifact_version" => connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM production_artifact_versions
                 WHERE artifact_id = ?1 AND version = ?2)",
                params![reference.reference, reference.version],
                |row| row.get(0),
            )
            .map_err(sql_error),
        "approved_query_treatment" => connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM tender_query_treatment_decisions
                 WHERE decision_id = ?1 AND query_version = ?2)",
                params![reference.reference, reference.version],
                |row| row.get(0),
            )
            .map_err(sql_error),
        "tender_query_version" => connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM tender_query_versions
                 WHERE query_id = ?1 AND version = ?2)",
                params![reference.reference, reference.version],
                |row| row.get(0),
            )
            .map_err(sql_error),
        "production_review_finding" => connection
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM production_review_findings AS findings
                   JOIN production_reviews AS reviews ON reviews.review_id = findings.review_id
                   WHERE findings.finding_id = ?1 AND reviews.target_version = ?2
                 )",
                params![reference.reference, reference.version],
                |row| row.get(0),
            )
            .map_err(sql_error),
        "engineer_entry" | "approved_calculation_run" => connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM tender_record_authorities
                 WHERE authority_id = ?1 AND tender_revision = ?2)",
                params![reference.reference, reference.version],
                |row| row.get(0),
            )
            .map_err(sql_error),
        "source_evidence" => {
            let Some((artifact_id, ordinal)) = reference.reference.rsplit_once('#') else {
                return Ok(false);
            };
            let Ok(ordinal) = ordinal.parse::<u32>() else {
                return Ok(false);
            };
            connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM evidence_locations
                     WHERE artifact_id = ?1 AND version = ?2 AND ordinal = ?3)",
                    params![artifact_id, reference.version, ordinal],
                    |row| row.get(0),
                )
                .map_err(sql_error)
        }
        _ => Ok(false),
    }
}

fn query_publication_has_capacity(
    connection: &rusqlite::Connection,
    affected_records: &[TenderRecordVersionReference],
    affected_task_keys: &[String],
) -> Result<bool, TenderCommandError> {
    let (version_count, invalidation_count): (u32, u32) = connection
        .query_row(
            "SELECT (SELECT COUNT(*) FROM tender_query_versions),
                    (SELECT COUNT(*) FROM tender_query_target_invalidations)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(sql_error)?;
    let additional_invalidations = u32::try_from(query_target_invalidation_bound(
        affected_records,
        affected_task_keys,
    ))
    .unwrap_or(u32::MAX);
    Ok(version_count < MAX_QUERY_VERSIONS_TOTAL
        && invalidation_count.saturating_add(additional_invalidations)
            <= MAX_QUERY_INVALIDATIONS_TOTAL)
}

fn query_target_invalidation_bound(
    affected_records: &[TenderRecordVersionReference],
    affected_task_keys: &[String],
) -> usize {
    let task_targets = if affected_task_keys.is_empty() {
        0
    } else {
        MAX_QUERY_PRODUCTION_TARGETS
    };
    affected_records
        .len()
        .saturating_mul(super::tender_records::MAX_RECORD_FIELDS)
        .saturating_add(task_targets.saturating_mul(4))
}

fn insert_query_version(
    transaction: &Transaction<'_>,
    input: QueryVersionInsert<'_>,
) -> Result<String, TenderCommandError> {
    let manifest = TenderQueryManifest {
        schema_version: 1,
        query_id: input.query_id,
        version: input.version,
        query_type: input.query_type,
        question: input.question,
        ambiguity_or_gap: input.ambiguity_or_gap,
        owner_profile_id: input.owner_profile_id,
        owner_profile_version: input.owner_profile_version,
        evidence: input.evidence,
        affected_records: input.affected_records,
        affected_task_keys: input.affected_task_keys,
        invalidation_targets: input.invalidation_targets,
        due_at: input.due_at,
        material: input.material,
        release_blocking: input.release_blocking,
        proposed_treatments: input.proposed_treatments,
        responses: input.responses,
        source_run_id: input.source_run_id,
        created_by: input.created_by,
        created_at: input.created_at,
    };
    let manifest_json = canonical_json(&manifest)?;
    let manifest_sha256 = sha256_hex(manifest_json.as_bytes());
    transaction
        .execute(
            "INSERT INTO tender_query_versions (
               query_id, version, query_type, question, ambiguity_or_gap,
               owner_profile_id, owner_profile_version, evidence_json,
               affected_records_json, affected_task_keys_json, invalidation_targets_json,
               due_at, material,
               release_blocking, proposed_treatments_json, responses_json,
               source_run_id, created_by, manifest_json, manifest_sha256, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
                       ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)",
            params![
                input.query_id,
                input.version,
                input.query_type.as_str(),
                input.question,
                input.ambiguity_or_gap,
                input.owner_profile_id,
                input.owner_profile_version,
                canonical_json(input.evidence)?,
                canonical_json(input.affected_records)?,
                canonical_json(input.affected_task_keys)?,
                canonical_json(input.invalidation_targets)?,
                input.due_at,
                input.material,
                input.release_blocking,
                canonical_json(input.proposed_treatments)?,
                canonical_json(input.responses)?,
                input.source_run_id,
                input.created_by,
                manifest_json,
                &manifest_sha256,
                input.created_at,
            ],
        )
        .map_err(sql_error)?;
    Ok(manifest_sha256)
}

fn load_query_version_row(
    connection: &rusqlite::Connection,
    query_id: &str,
    version: u32,
) -> Result<Option<QueryVersionRow>, TenderCommandError> {
    connection
        .query_row(
            "SELECT query_type, question, ambiguity_or_gap, owner_profile_id,
                    owner_profile_version, evidence_json, affected_records_json,
                    affected_task_keys_json, invalidation_targets_json,
                    due_at, material, release_blocking,
                    proposed_treatments_json, responses_json, source_run_id,
                    created_by, manifest_json, manifest_sha256, created_at
             FROM tender_query_versions WHERE query_id = ?1 AND version = ?2",
            params![query_id, version],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, u32>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, String>(9)?,
                    row.get::<_, bool>(10)?,
                    row.get::<_, bool>(11)?,
                    row.get::<_, String>(12)?,
                    row.get::<_, String>(13)?,
                    row.get::<_, Option<String>>(14)?,
                    row.get::<_, String>(15)?,
                    row.get::<_, String>(16)?,
                    row.get::<_, String>(17)?,
                    row.get::<_, String>(18)?,
                ))
            },
        )
        .optional()
        .map_err(sql_error)?
        .map(|row| {
            Ok(QueryVersionRow {
                query_type: TenderQueryType::parse(&row.0)?,
                question: row.1,
                ambiguity_or_gap: row.2,
                owner_profile_id: row.3,
                owner_profile_version: row.4,
                evidence: serde_json::from_str(&row.5)
                    .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
                affected_records: serde_json::from_str(&row.6)
                    .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
                affected_task_keys: serde_json::from_str(&row.7)
                    .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
                invalidation_targets: serde_json::from_str(&row.8)
                    .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
                due_at: row.9,
                material: row.10,
                release_blocking: row.11,
                proposed_treatments: serde_json::from_str(&row.12)
                    .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
                responses: serde_json::from_str(&row.13)
                    .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
                source_run_id: row.14,
                created_by: row.15,
                manifest_json: row.16,
                manifest_sha256: row.17,
                created_at: row.18,
            })
        })
        .transpose()
}

pub(crate) fn load_query_decision(
    connection: &rusqlite::Connection,
    query_id: &str,
    version: u32,
) -> Result<Option<ApprovedQueryTreatment>, TenderCommandError> {
    connection
        .query_row(
            "SELECT decision_id, treatment, rationale, treatment_details, closes_query,
                    decided_by, acting_role, manifest_sha256, created_at
             FROM tender_query_treatment_decisions
             WHERE query_id = ?1 AND query_version = ?2",
            params![query_id, version],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, bool>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                ))
            },
        )
        .optional()
        .map_err(sql_error)?
        .map(|row| {
            Ok(ApprovedQueryTreatment {
                decision_id: row.0,
                query_id: query_id.to_owned(),
                query_version: version,
                treatment: TenderQueryTreatment::parse(&row.1)?,
                rationale: row.2,
                treatment_details: row.3,
                closes_query: row.4,
                decided_by: row.5,
                acting_role: row.6,
                manifest_sha256: row.7,
                created_at: row.8,
            })
        })
        .transpose()
}

fn query_source_is_attributable(
    connection: &rusqlite::Connection,
    query_id: &str,
    query_version: u32,
    row: &QueryVersionRow,
) -> Result<bool, TenderCommandError> {
    let prior_proposals = if query_version > 1 {
        let Some(prior) = load_query_version_row(connection, query_id, query_version - 1)? else {
            return Ok(false);
        };
        prior.proposed_treatments
    } else {
        Vec::new()
    };
    let prior_responses = if query_version > 1 {
        load_query_version_row(connection, query_id, query_version - 1)?
            .map(|prior| prior.responses)
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    match (row.created_by.as_str(), row.source_run_id.as_deref()) {
        ("engineer_user", None) => {
            for proposal in &row.proposed_treatments {
                if proposal.proposed_by_run_id.is_none() {
                    if proposal.proposed_by != "engineer_user" {
                        return Ok(false);
                    }
                } else if !prior_proposals.contains(proposal)
                    || !query_proposal_is_attributable(connection, proposal)?
                {
                    return Ok(false);
                }
            }
            if row.responses.iter().any(|response| {
                !prior_responses.contains(response)
                    && (response.registered_by != "engineer_user"
                        || response.created_at != row.created_at)
            }) {
                return Ok(false);
            }
            Ok(true)
        }
        ("agent_run", Some(run_id)) => {
            let source: Option<(String, u32, String, Option<String>)> = connection
                .query_row(
                    "SELECT profile_id, profile_version, status, completed_at
                     FROM agent_runs WHERE run_id = ?1",
                    [run_id],
                    |record| {
                        Ok((
                            record.get(0)?,
                            record.get(1)?,
                            record.get(2)?,
                            record.get(3)?,
                        ))
                    },
                )
                .optional()
                .map_err(sql_error)?;
            let Some((profile_id, profile_version, status, completed_at)) = source else {
                return Ok(false);
            };
            if status != "completed"
                || completed_at.as_deref() != Some(row.created_at.as_str())
                || (query_version == 1
                    && (row.owner_profile_id != profile_id
                        || row.owner_profile_version != profile_version))
            {
                return Ok(false);
            }
            for proposal in &row.proposed_treatments {
                let newly_attributed_to_source = proposal.proposed_by_run_id.as_deref()
                    == Some(run_id)
                    && proposal.proposed_by == profile_id;
                if !newly_attributed_to_source
                    && (!prior_proposals.contains(proposal)
                        || !query_proposal_is_attributable(connection, proposal)?)
                {
                    return Ok(false);
                }
            }
            if row.responses.iter().any(|response| {
                !prior_responses.contains(response)
                    && (response.registered_by != profile_id
                        || response.created_at != row.created_at)
            }) {
                return Ok(false);
            }
            Ok(true)
        }
        _ => Ok(false),
    }
}

fn query_proposal_is_attributable(
    connection: &rusqlite::Connection,
    proposal: &TenderQueryTreatmentProposal,
) -> Result<bool, TenderCommandError> {
    match proposal.proposed_by_run_id.as_deref() {
        None => Ok(proposal.proposed_by == "engineer_user"),
        Some(run_id) => connection
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM agent_runs
                   WHERE run_id = ?1 AND status = 'completed' AND profile_id = ?2
                 )",
                params![run_id, proposal.proposed_by],
                |record| record.get(0),
            )
            .map_err(sql_error),
    }
}

fn query_version_audit_is_valid(
    connection: &rusqlite::Connection,
    query_id: &str,
    query_version: u32,
    row: &QueryVersionRow,
) -> Result<bool, TenderCommandError> {
    let event_type = match (query_version, row.source_run_id.is_some()) {
        (1, true) => "tender_query_proposed_by_agent",
        (1, false) => "tender_query_created",
        (_, true) => "tender_query_updated_by_agent",
        (_, false) => "tender_query_revised",
    };
    let payload: Option<String> = connection
        .query_row(
            "SELECT payload_json FROM audit_events
             WHERE event_type = ?1 AND created_at = ?2
               AND json_extract(payload_json, '$.change.query_id') = ?3
               AND json_extract(payload_json, '$.change.query_version') = ?4
             LIMIT 1",
            params![
                event_type,
                row.created_at,
                query_id,
                query_version.to_string()
            ],
            |audit| audit.get(0),
        )
        .optional()
        .map_err(sql_error)?;
    let expected = json!({
        "manifest_sha256": row.manifest_sha256,
        "query_id": query_id,
        "query_version": query_version.to_string(),
    });
    Ok(payload.is_some_and(|payload| {
        serde_json::from_str::<serde_json::Value>(&payload)
            .is_ok_and(|payload| payload.get("change") == Some(&expected))
    }))
}

fn query_decision_is_valid(
    connection: &rusqlite::Connection,
    query_id: &str,
    query_version: u32,
    query: &QueryVersionRow,
) -> Result<bool, TenderCommandError> {
    type StoredDecision = (
        String,
        String,
        String,
        String,
        bool,
        String,
        String,
        i64,
        String,
        String,
        String,
        String,
    );
    let decision: Option<StoredDecision> = connection
        .query_row(
            "SELECT decision_id, treatment, rationale, treatment_details, closes_query,
                    decided_by, acting_role, audit_sequence, invalidation_targets_json,
                    manifest_json, manifest_sha256, created_at
             FROM tender_query_treatment_decisions
             WHERE query_id = ?1 AND query_version = ?2",
            params![query_id, query_version],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                    row.get(9)?,
                    row.get(10)?,
                    row.get(11)?,
                ))
            },
        )
        .optional()
        .map_err(sql_error)?;
    let Some(decision) = decision else {
        return Ok(true);
    };
    let treatment = TenderQueryTreatment::parse(&decision.1)?;
    let decision_invalidation_targets: Vec<QueryInvalidationTarget> =
        serde_json::from_str(&decision.8)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    if decision.5 != "engineer_user"
        || decision.6 != "tendering_manager"
        || (decision.4
            && (!treatment.permits_dependent_work()
                || (treatment == TenderQueryTreatment::InternalResolution
                    && query.responses.is_empty())))
    {
        return Ok(false);
    }
    let expected_manifest = canonical_json(&QueryTreatmentManifest {
        schema_version: 1,
        decision_id: &decision.0,
        query_id,
        query_version,
        query_manifest_sha256: &query.manifest_sha256,
        treatment,
        rationale: &decision.2,
        treatment_details: &decision.3,
        closes_query: decision.4,
        decided_by: &decision.5,
        acting_role: &decision.6,
        invalidation_targets: &decision_invalidation_targets,
        created_at: &decision.11,
    })?;
    if decision.9 != expected_manifest || decision.10 != sha256_hex(expected_manifest.as_bytes()) {
        return Ok(false);
    }
    let audit: Option<(String, String, String)> = connection
        .query_row(
            "SELECT event_type, payload_json, created_at FROM audit_events WHERE sequence = ?1",
            [decision.7],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(sql_error)?;
    let expected_payload = json!({
        "acting_role": "tendering_manager",
        "closes_query": decision.4,
        "decided_by": "engineer_user",
        "decision_id": decision.0,
        "manifest_sha256": decision.10,
        "query_id": query_id,
        "query_version": query_version.to_string(),
        "treatment": treatment.as_str(),
    });
    Ok(audit.is_some_and(|(event_type, payload_json, created_at)| {
        event_type == "tender_query_treatment_decided"
            && created_at == decision.11
            && serde_json::from_str::<serde_json::Value>(&payload_json)
                .is_ok_and(|payload| payload.get("change") == Some(&expected_payload))
    }))
}

fn query_invalidations_are_valid(
    connection: &rusqlite::Connection,
    query_id: &str,
    query_version: u32,
    query: &QueryVersionRow,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<bool, TenderCommandError> {
    let mut statement = connection
        .prepare(
            "SELECT invalidation_id, target_kind, target_id, target_version, reason, created_at
             FROM tender_query_target_invalidations
             WHERE query_id = ?1 AND query_version = ?2 ORDER BY rowid",
        )
        .map_err(sql_error)?;
    let invalidations = statement
        .query_map(params![query_id, query_version], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, u32>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        })
        .map_err(sql_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql_error)?;
    if invalidations.len()
        > query_target_invalidation_bound(&query.affected_records, &query.affected_task_keys)
    {
        return Ok(false);
    }
    let mut stored_targets = invalidations
        .iter()
        .map(
            |(_, kind, target_id, target_version, _, _)| QueryInvalidationTarget {
                target_kind: kind.clone(),
                target_id: target_id.clone(),
                target_version: (*target_version != 0).then_some(*target_version),
            },
        )
        .collect::<Vec<_>>();
    stored_targets.sort();
    let decision: Option<(String, String)> = connection
        .query_row(
            "SELECT invalidation_targets_json, created_at
             FROM tender_query_treatment_decisions
             WHERE query_id = ?1 AND query_version = ?2",
            params![query_id, query_version],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(sql_error)?;
    let mut expected_targets = query.invalidation_targets.clone();
    let (decision_targets, decision_created_at) = match decision {
        Some((targets_json, created_at)) => (
            serde_json::from_str::<Vec<QueryInvalidationTarget>>(&targets_json)
                .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
            Some(created_at),
        ),
        None => (Vec::new(), None),
    };
    expected_targets.extend(decision_targets.iter().cloned());
    expected_targets.sort();
    if stored_targets.windows(2).any(|pair| pair[0] == pair[1])
        || expected_targets.windows(2).any(|pair| pair[0] == pair[1])
        || expected_targets != stored_targets
    {
        return Ok(false);
    }
    let expected_reason = if query_version == 1 {
        "query_opened"
    } else {
        let Some(prior) = load_query_version_row(connection, query_id, query_version - 1)? else {
            return Ok(false);
        };
        if query.responses.len() > prior.responses.len() {
            "response_added"
        } else if query.evidence != prior.evidence {
            "evidence_changed"
        } else {
            "treatment_changed"
        }
    };
    let affected_task_keys_json = canonical_json(&query.affected_task_keys)?;
    for (invalidation_id, kind, target_id, target_version, reason, created_at) in invalidations {
        check()?;
        let target = QueryInvalidationTarget {
            target_kind: kind.clone(),
            target_id: target_id.clone(),
            target_version: (target_version != 0).then_some(target_version),
        };
        let (target_reason, target_created_at) = if decision_targets.contains(&target) {
            ("treatment_changed", decision_created_at.as_deref())
        } else {
            (expected_reason, Some(query.created_at.as_str()))
        };
        if !valid_identifier(&invalidation_id)
            || reason != target_reason
            || Some(created_at.as_str()) != target_created_at
        {
            return Ok(false);
        }
        let valid = match kind.as_str() {
            "production_task" => {
                query_affects_production_task(connection, &target_id, &affected_task_keys_json)?
            }
            "artifact" => {
                let task_id = connection
                    .query_row(
                        "SELECT production_task_id FROM production_artifact_versions
                     WHERE artifact_id = ?1 AND version = ?2",
                        params![target_id, target_version],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()
                    .map_err(sql_error)?;
                match task_id {
                    Some(task_id) => query_affects_production_task(
                        connection,
                        &task_id,
                        &affected_task_keys_json,
                    )?,
                    None => false,
                }
            }
            "review" => {
                let task_id = connection
                    .query_row(
                        "SELECT production_task_id FROM production_reviews
                     WHERE review_id = ?1 AND target_version = ?2",
                        params![target_id, target_version],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()
                    .map_err(sql_error)?;
                match task_id {
                    Some(task_id) => query_affects_production_task(
                        connection,
                        &task_id,
                        &affected_task_keys_json,
                    )?,
                    None => false,
                }
            }
            "approval" => {
                let task_id = connection
                    .query_row(
                        "SELECT production_task_id FROM production_integration_readiness
                     WHERE readiness_id = ?1 AND artifact_version = ?2",
                        params![target_id, target_version],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()
                    .map_err(sql_error)?;
                match task_id {
                    Some(task_id) => query_affects_production_task(
                        connection,
                        &task_id,
                        &affected_task_keys_json,
                    )?,
                    None => false,
                }
            }
            "calculation" => connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM tender_record_authorities
                     WHERE authority_id = ?1 AND tender_revision = ?2
                       AND kind = 'calculation_run')",
                    params![target_id, target_version],
                    |row| row.get(0),
                )
                .map_err(sql_error)?,
            _ => false,
        };
        if !valid {
            return Ok(false);
        }
    }
    Ok(true)
}

fn query_affects_production_task(
    connection: &rusqlite::Connection,
    production_task_id: &str,
    affected_task_keys_json: &str,
) -> Result<bool, TenderCommandError> {
    connection
        .query_row(
            "WITH RECURSIVE affected_tasks(production_task_id, activation_id, task_key) AS (
               SELECT tasks.production_task_id, tasks.activation_id, tasks.task_key
               FROM production_tasks AS tasks
               WHERE tasks.task_key IN (SELECT value FROM json_each(?2))
                  OR '*' IN (SELECT value FROM json_each(?2))
               UNION
               SELECT dependent.production_task_id, dependent.activation_id, dependent.task_key
               FROM production_tasks AS dependent
               JOIN affected_tasks AS prerequisite
                 ON prerequisite.activation_id = dependent.activation_id
               JOIN json_each(dependent.task_definition_json, '$.dependencies') AS dependency
                 ON dependency.value = prerequisite.task_key
             )
             SELECT EXISTS(
               SELECT 1 FROM affected_tasks WHERE production_task_id = ?1
             )",
            params![production_task_id, affected_task_keys_json],
            |row| row.get(0),
        )
        .map_err(sql_error)
}

fn query_status(
    row: &QueryVersionRow,
    decision: Option<&ApprovedQueryTreatment>,
) -> TenderQueryStatus {
    if let Some(decision) = decision {
        if decision.closes_query {
            TenderQueryStatus::Closed
        } else if !decision.treatment.permits_dependent_work() {
            TenderQueryStatus::Blocked
        } else {
            TenderQueryStatus::TreatmentApproved
        }
    } else if !row.responses.is_empty() {
        TenderQueryStatus::Responded
    } else if !row.proposed_treatments.is_empty() {
        TenderQueryStatus::TreatmentProposed
    } else {
        TenderQueryStatus::Open
    }
}

fn require_query_register_open(
    connection: &rusqlite::Connection,
) -> Result<(), TenderCommandError> {
    let open: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM query_register WHERE singleton = 1)",
            [],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    if !open {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    Ok(())
}

struct QueryTargetInvalidation<'a> {
    query_id: &'a str,
    query_version: u32,
    targets: &'a [QueryInvalidationTarget],
    reason: &'a str,
    blocks_dependent_work: bool,
    created_at: &'a str,
}

fn collect_query_invalidation_targets(
    transaction: &Transaction<'_>,
    affected_records: &[TenderRecordVersionReference],
    affected_task_keys: &[String],
) -> Result<Vec<QueryInvalidationTarget>, TenderCommandError> {
    collect_query_invalidation_targets_with_check(
        transaction,
        affected_records,
        affected_task_keys,
        &mut || Ok(()),
    )
}

fn collect_query_invalidation_targets_with_check(
    transaction: &Transaction<'_>,
    affected_records: &[TenderRecordVersionReference],
    affected_task_keys: &[String],
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<Vec<QueryInvalidationTarget>, TenderCommandError> {
    check()?;
    let mut invalidation_targets = Vec::new();
    for record in affected_records {
        check()?;
        let mut statement = transaction
            .prepare(
                "SELECT authorities.authority_id, authorities.tender_revision
                 FROM tender_record_versions AS versions
                 JOIN json_each(versions.fields_json) AS field
                 JOIN tender_record_authorities AS authorities
                   ON authorities.authority_id = json_extract(field.value, '$.basis_reference')
                  AND authorities.kind = 'calculation_run'
                 WHERE versions.record_id = ?1 AND versions.version = ?2
                   AND json_extract(field.value, '$.basis_kind') = 'calculation_run'
                 ORDER BY authorities.rowid",
            )
            .map_err(sql_error)?;
        let calculations = statement
            .query_map(params![record.record_id, record.version], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?))
            })
            .map_err(sql_error)?;
        for calculation in calculations {
            check()?;
            let (authority_id, tender_revision) = calculation.map_err(sql_error)?;
            invalidation_targets.push(QueryInvalidationTarget {
                target_kind: "calculation".into(),
                target_id: authority_id,
                target_version: Some(tender_revision),
            });
        }
    }
    for task_key in affected_task_keys {
        check()?;
        let mut statement = transaction
            .prepare(
                "WITH RECURSIVE selected_tasks(
                   production_task_id, activation_id, task_key, status, task_definition_json
                 ) AS (
                   SELECT tasks.production_task_id, tasks.activation_id, tasks.task_key,
                          tasks.status, tasks.task_definition_json
                   FROM production_tasks AS tasks
                   JOIN production_activations AS activations
                     ON activations.activation_id = tasks.activation_id
                   WHERE activations.status = 'active'
                     AND (?1 = '*' OR tasks.task_key = ?1)
                   UNION
                   SELECT dependent.production_task_id, dependent.activation_id,
                          dependent.task_key, dependent.status,
                          dependent.task_definition_json
                   FROM production_tasks AS dependent
                   JOIN selected_tasks AS prerequisite
                     ON prerequisite.activation_id = dependent.activation_id
                   JOIN json_each(dependent.task_definition_json, '$.dependencies') AS dependency
                     ON dependency.value = prerequisite.task_key
                 )
                 SELECT tasks.production_task_id, tasks.status,
                        artifacts.artifact_id, artifacts.version,
                        reviews.review_id, reviews.target_version,
                        readiness.readiness_id, readiness.artifact_version
                 FROM selected_tasks AS tasks
                 LEFT JOIN production_artifact_versions AS artifacts
                   ON artifacts.production_task_id = tasks.production_task_id
                  AND artifacts.version = (
                    SELECT MAX(candidate.version) FROM production_artifact_versions AS candidate
                    WHERE candidate.production_task_id = tasks.production_task_id
                  )
                 LEFT JOIN production_reviews AS reviews
                   ON reviews.production_task_id = tasks.production_task_id
                  AND reviews.rowid = (
                    SELECT MAX(candidate.rowid) FROM production_reviews AS candidate
                    WHERE candidate.production_task_id = tasks.production_task_id
                  )
                 LEFT JOIN production_integration_readiness AS readiness
                   ON readiness.production_task_id = tasks.production_task_id
                  AND readiness.rowid = (
                    SELECT MAX(candidate.rowid) FROM production_integration_readiness AS candidate
                    WHERE candidate.production_task_id = tasks.production_task_id
                  )
                 ORDER BY tasks.production_task_id",
            )
            .map_err(sql_error)?;
        let targets = statement
            .query_map([task_key], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<u32>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<u32>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<u32>>(7)?,
                ))
            })
            .map_err(sql_error)?;
        for target in targets {
            check()?;
            let target = target.map_err(sql_error)?;
            invalidation_targets.push(QueryInvalidationTarget {
                target_kind: "production_task".into(),
                target_id: target.0,
                target_version: None,
            });
            if let (Some(artifact_id), Some(version)) = (target.2, target.3) {
                invalidation_targets.push(QueryInvalidationTarget {
                    target_kind: "artifact".into(),
                    target_id: artifact_id,
                    target_version: Some(version),
                });
            }
            if let (Some(review_id), Some(version)) = (target.4, target.5) {
                invalidation_targets.push(QueryInvalidationTarget {
                    target_kind: "review".into(),
                    target_id: review_id,
                    target_version: Some(version),
                });
            }
            if let (Some(readiness_id), Some(version)) = (target.6, target.7) {
                invalidation_targets.push(QueryInvalidationTarget {
                    target_kind: "approval".into(),
                    target_id: readiness_id,
                    target_version: Some(version),
                });
            }
        }
    }
    invalidation_targets.sort();
    invalidation_targets.dedup();
    Ok(invalidation_targets)
}

fn invalidate_query_targets(
    transaction: &Transaction<'_>,
    input: QueryTargetInvalidation<'_>,
) -> Result<(), TenderCommandError> {
    let QueryTargetInvalidation {
        query_id,
        query_version,
        targets,
        reason,
        blocks_dependent_work,
        created_at,
    } = input;
    for target in targets {
        insert_query_invalidation(
            transaction,
            QueryInvalidationInsert {
                query_id,
                query_version,
                target_kind: &target.target_kind,
                target_id: &target.target_id,
                target_version: target.target_version,
                reason,
                created_at,
            },
        )?;
    }
    for target in targets
        .iter()
        .filter(|target| target.target_kind == "production_task")
    {
        let mut has_invalidated_artifact = false;
        for candidate in targets
            .iter()
            .filter(|candidate| candidate.target_kind == "artifact")
        {
            has_invalidated_artifact = transaction
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM production_artifact_versions
                     WHERE production_task_id = ?1 AND artifact_id = ?2 AND version = ?3)",
                    params![
                        target.target_id,
                        candidate.target_id,
                        candidate.target_version
                    ],
                    |row| row.get::<_, bool>(0),
                )
                .map_err(sql_error)?;
            if has_invalidated_artifact {
                break;
            }
        }
        if blocks_dependent_work {
            transaction
                .execute(
                    "UPDATE production_tasks SET status = 'query_blocked', updated_at = ?2
                     WHERE production_task_id = ?1
                       AND status IN ('blocked', 'ready', 'review_ready',
                                      'remediation_ready', 'ready_for_integration')",
                    params![target.target_id, created_at],
                )
                .map_err(sql_error)?;
        } else if has_invalidated_artifact {
            transaction
                .execute(
                    "UPDATE production_tasks SET status = 'remediation_ready', updated_at = ?2
                     WHERE production_task_id = ?1
                       AND status IN ('review_ready', 'remediation_ready', 'ready_for_integration')",
                    params![target.target_id, created_at],
                )
                .map_err(sql_error)?;
        }
    }
    transaction
        .execute(
            "UPDATE production_tasks AS tasks
             SET status = 'blocked', updated_at = ?1
             WHERE status = 'ready'
               AND EXISTS(
                 SELECT 1 FROM json_each(tasks.task_definition_json, '$.dependencies') AS dependency
                 LEFT JOIN production_tasks AS prerequisite
                   ON prerequisite.activation_id = tasks.activation_id
                  AND prerequisite.task_key = dependency.value
                 WHERE prerequisite.production_task_id IS NULL
                    OR prerequisite.status != 'ready_for_integration'
               )",
            [created_at],
        )
        .map_err(sql_error)?;
    Ok(())
}

struct QueryInvalidationInsert<'a> {
    query_id: &'a str,
    query_version: u32,
    target_kind: &'a str,
    target_id: &'a str,
    target_version: Option<u32>,
    reason: &'a str,
    created_at: &'a str,
}

fn insert_query_invalidation(
    transaction: &Transaction<'_>,
    input: QueryInvalidationInsert<'_>,
) -> Result<(), TenderCommandError> {
    transaction
        .execute(
            "INSERT OR IGNORE INTO tender_query_target_invalidations (
               invalidation_id, query_id, query_version, target_kind, target_id,
               target_version, reason, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                random_identifier(transaction)?,
                input.query_id,
                input.query_version,
                input.target_kind,
                input.target_id,
                input.target_version.unwrap_or(0),
                input.reason,
                input.created_at,
            ],
        )
        .map_err(sql_error)?;
    Ok(())
}

pub(crate) fn production_task_state_after_query_release(
    connection: &rusqlite::Connection,
    production_task_id: &str,
) -> Result<&'static str, TenderCommandError> {
    let (has_artifact, dependencies_ready): (bool, bool) = connection
        .query_row(
            "SELECT
               EXISTS(SELECT 1 FROM production_artifact_versions AS artifacts
                      WHERE artifacts.production_task_id = tasks.production_task_id),
               NOT EXISTS(
                 SELECT 1 FROM json_each(tasks.task_definition_json, '$.dependencies') AS dep
                 LEFT JOIN production_tasks AS prerequisite
                   ON prerequisite.activation_id = tasks.activation_id
                  AND prerequisite.task_key = dep.value
                 WHERE prerequisite.status != 'ready_for_integration'
                    OR prerequisite.production_task_id IS NULL
               )
             FROM production_tasks AS tasks WHERE tasks.production_task_id = ?1",
            [production_task_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(sql_error)?;
    if has_artifact && task_has_current_query_artifact_invalidation(connection, production_task_id)?
    {
        Ok("remediation_ready")
    } else if has_artifact
        && connection
            .query_row(
                "SELECT EXISTS(
                           SELECT 1 FROM production_integration_readiness AS readiness
                           JOIN production_artifact_versions AS artifacts
                             ON artifacts.artifact_id = readiness.artifact_id
                            AND artifacts.version = readiness.artifact_version
                           WHERE readiness.production_task_id = ?1
                             AND artifacts.version = (
                               SELECT MAX(candidate.version)
                               FROM production_artifact_versions AS candidate
                               WHERE candidate.production_task_id = ?1
                             )
                             AND NOT EXISTS(
                               SELECT 1 FROM tender_query_target_invalidations AS invalidations
                               JOIN tender_query_heads AS heads
                                 ON heads.query_id = invalidations.query_id
                                AND heads.current_version = invalidations.query_version
                               WHERE invalidations.target_kind = 'approval'
                                 AND invalidations.target_id = readiness.readiness_id
                             )
                         )",
                [production_task_id],
                |row| row.get::<_, bool>(0),
            )
            .map_err(sql_error)?
    {
        Ok("ready_for_integration")
    } else if has_artifact
        && connection
            .query_row(
                "SELECT EXISTS(
                           SELECT 1 FROM production_reviews
                           WHERE production_task_id = ?1
                             AND result = 'requires_remediation'
                           ORDER BY rowid DESC LIMIT 1
                         )",
                [production_task_id],
                |row| row.get::<_, bool>(0),
            )
            .map_err(sql_error)?
    {
        Ok("remediation_ready")
    } else if has_artifact {
        Ok("review_ready")
    } else if dependencies_ready {
        Ok("ready")
    } else {
        Ok("blocked")
    }
}

fn release_query_blocked_tasks(
    transaction: &Transaction<'_>,
    _affected_task_keys: &[String],
    updated_at: &str,
) -> Result<(), TenderCommandError> {
    let mut statement = transaction
        .prepare(
            "SELECT tasks.production_task_id, tasks.task_key
             FROM production_tasks AS tasks
             JOIN production_activations AS activations
               ON activations.activation_id = tasks.activation_id
             WHERE activations.status = 'active' AND tasks.status = 'query_blocked'",
        )
        .map_err(sql_error)?;
    let tasks = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(sql_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql_error)?;
    for (production_task_id, actual_task_key) in tasks {
        if task_has_blocking_query(transaction, &actual_task_key)? {
            continue;
        }
        let state = production_task_state_after_query_release(transaction, &production_task_id)?;
        transaction
            .execute(
                "UPDATE production_tasks SET status = ?2, updated_at = ?3
                     WHERE production_task_id = ?1 AND status = 'query_blocked'",
                params![production_task_id, state, updated_at],
            )
            .map_err(sql_error)?;
    }
    Ok(())
}

pub(crate) fn task_has_blocking_query(
    connection: &rusqlite::Connection,
    task_key: &str,
) -> Result<bool, TenderCommandError> {
    connection
        .query_row(
            "WITH RECURSIVE blocking_tasks(query_id, task_key) AS (
               SELECT heads.query_id, affected.value
               FROM tender_query_heads AS heads
               JOIN tender_query_versions AS versions
                 ON versions.query_id = heads.query_id
                AND versions.version = heads.current_version
               JOIN json_each(versions.affected_task_keys_json) AS affected
               LEFT JOIN tender_query_treatment_decisions AS decisions
                 ON decisions.query_id = versions.query_id
                AND decisions.query_version = versions.version
               WHERE (decisions.decision_id IS NULL
                      AND (versions.material = 1 OR versions.release_blocking = 1))
                  OR decisions.treatment IN ('external_rfi_drafting', 'blocked')
               UNION
               SELECT blocked.query_id, dependent.task_key
               FROM blocking_tasks AS blocked
               JOIN production_tasks AS dependent
               JOIN production_activations AS activations
                 ON activations.activation_id = dependent.activation_id
                AND activations.status = 'active'
               JOIN json_each(dependent.task_definition_json, '$.dependencies') AS dependency
                 ON dependency.value = blocked.task_key
             )
             SELECT EXISTS(
               SELECT 1 FROM blocking_tasks WHERE task_key = ?1 OR task_key = '*'
             )",
            [task_key],
            |row| row.get(0),
        )
        .map_err(sql_error)
}

pub(crate) fn production_query_contexts_for_task(
    connection: &rusqlite::Connection,
    task_key: &str,
) -> Result<Vec<ProductionQueryContext>, TenderCommandError> {
    let mut statement = connection
        .prepare(
            "SELECT query_id, query_version FROM (
               SELECT heads.query_id, heads.current_version AS query_version, versions.rowid
               FROM tender_query_heads AS heads
               JOIN tender_query_versions AS versions
                 ON versions.query_id = heads.query_id
                AND versions.version = heads.current_version
               JOIN json_each(versions.affected_task_keys_json) AS affected
               WHERE affected.value = ?1 OR affected.value = '*'
               UNION
               SELECT heads.query_id, heads.current_version, versions.rowid
               FROM tender_query_heads AS heads
               JOIN tender_query_versions AS versions
                 ON versions.query_id = heads.query_id
                AND versions.version = heads.current_version
               JOIN tender_query_target_invalidations AS invalidations
                 ON invalidations.query_id = heads.query_id
                AND invalidations.query_version = heads.current_version
                AND invalidations.target_kind = 'production_task'
               JOIN production_tasks AS tasks
                 ON tasks.production_task_id = invalidations.target_id
               JOIN production_activations AS activations
                 ON activations.activation_id = tasks.activation_id
                AND activations.status = 'active'
               WHERE tasks.task_key = ?1
               UNION
               SELECT invalidations.query_id, invalidations.query_version, versions.rowid
               FROM production_tasks AS tasks
               JOIN production_artifact_versions AS artifacts
                 ON artifacts.production_task_id = tasks.production_task_id
                AND artifacts.version = (
                  SELECT MAX(candidate.version) FROM production_artifact_versions AS candidate
                  WHERE candidate.production_task_id = tasks.production_task_id
                )
               JOIN tender_query_target_invalidations AS invalidations
                 ON invalidations.target_kind = 'artifact'
                AND invalidations.target_id = artifacts.artifact_id
                AND invalidations.target_version = artifacts.version
               JOIN tender_query_versions AS versions
                 ON versions.query_id = invalidations.query_id
                AND versions.version = invalidations.query_version
               WHERE tasks.task_key = ?1
                 AND invalidations.query_version = (
                   SELECT MAX(candidate.query_version)
                   FROM tender_query_target_invalidations AS candidate
                   WHERE candidate.query_id = invalidations.query_id
                     AND candidate.target_kind = 'artifact'
                     AND candidate.target_id = artifacts.artifact_id
                     AND candidate.target_version = artifacts.version
                 )
                 AND NOT EXISTS(
                   SELECT 1 FROM tender_query_heads AS current_head
                   JOIN tender_query_versions AS current_version
                     ON current_version.query_id = current_head.query_id
                    AND current_version.version = current_head.current_version
                   JOIN json_each(current_version.affected_task_keys_json) AS current_affected
                   WHERE current_head.query_id = invalidations.query_id
                     AND (current_affected.value = ?1 OR current_affected.value = '*')
                 )
             ) ORDER BY rowid LIMIT 257",
        )
        .map_err(sql_error)?;
    let rows = statement
        .query_map([task_key], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?))
        })
        .map_err(sql_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql_error)?;
    if rows.len() > MAX_TENDER_QUERIES {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    rows.into_iter()
        .map(|(query_id, query_version)| {
            let row = load_query_version_row(connection, &query_id, query_version)?
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            Ok(ProductionQueryContext {
                query_id: query_id.clone(),
                query_version,
                owner_profile_id: row.owner_profile_id,
                owner_profile_version: row.owner_profile_version,
                question: row.question,
                ambiguity_or_gap: row.ambiguity_or_gap,
                evidence: row.evidence,
                affected_records: row.affected_records,
                affected_task_keys: row.affected_task_keys,
                material: row.material,
                release_blocking: row.release_blocking,
                responses: row.responses,
                approved_treatment: load_query_decision(connection, &query_id, query_version)?,
                manifest_sha256: row.manifest_sha256,
                source_run_id: row.source_run_id,
            })
        })
        .collect()
}

pub(crate) fn task_has_current_query_artifact_invalidation(
    connection: &rusqlite::Connection,
    production_task_id: &str,
) -> Result<bool, TenderCommandError> {
    connection
        .query_row(
            "SELECT EXISTS(
               SELECT 1 FROM tender_query_target_invalidations AS invalidations
               JOIN production_artifact_versions AS artifacts
                 ON artifacts.artifact_id = invalidations.target_id
                AND artifacts.version = invalidations.target_version
               WHERE invalidations.target_kind = 'artifact'
                 AND artifacts.production_task_id = ?1
                 AND artifacts.version = (
                   SELECT MAX(candidate.version) FROM production_artifact_versions AS candidate
                   WHERE candidate.production_task_id = ?1
                 )
             )",
            [production_task_id],
            |row| row.get(0),
        )
        .map_err(sql_error)
}

pub(crate) struct AgentQueryPublication<'a> {
    pub tender_id: &'a TenderId,
    pub run_id: &'a str,
    pub task_id: &'a str,
    pub current_task_key: &'a str,
    pub proposals: &'a [AgentTenderQueryProposal],
    pub updates: &'a [AgentTenderQueryUpdate],
    pub query_control: bool,
    pub created_at: &'a str,
}

pub(crate) fn agent_query_publication_is_valid(
    connection: &rusqlite::Connection,
    task_id: &str,
    proposals: &[AgentTenderQueryProposal],
    updates: &[AgentTenderQueryUpdate],
    query_control: bool,
) -> Result<bool, TenderCommandError> {
    match preflight_agent_query_publication(connection, task_id, proposals, updates, query_control)
    {
        Ok(()) => Ok(true),
        Err(error)
            if matches!(
                error.code,
                TenderErrorCode::InvalidCommand | TenderErrorCode::IntegrityFailed
            ) =>
        {
            Ok(false)
        }
        Err(error) => Err(error),
    }
}

fn preflight_agent_query_publication(
    connection: &rusqlite::Connection,
    task_id: &str,
    proposals: &[AgentTenderQueryProposal],
    updates: &[AgentTenderQueryUpdate],
    query_control: bool,
) -> Result<(), TenderCommandError> {
    if proposals.len() > MAX_QUERY_PROPOSED_TREATMENTS
        || updates.len() > MAX_QUERY_PROPOSED_TREATMENTS
        || (query_control && (!proposals.is_empty() || updates.is_empty()))
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let (run_id, profile_id, profile_version, exact_inputs_json, current_task_key): (
        String,
        String,
        u32,
        String,
        String,
    ) = connection
        .query_row(
            "SELECT runs.run_id, runs.profile_id, runs.profile_version,
                    tasks.exact_inputs_json, production.task_key
             FROM agent_runs AS runs
             JOIN tender_tasks AS tasks ON tasks.task_id = runs.task_id
             JOIN production_task_attempts AS attempts ON attempts.task_id = tasks.task_id
             JOIN production_tasks AS production
               ON production.production_task_id = attempts.production_task_id
             WHERE tasks.task_id = ?1",
            [task_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .map_err(sql_error)?;
    let exact_inputs: Vec<AgentTaskInputReference> = serde_json::from_str(&exact_inputs_json)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    let (query_count, version_count, invalidation_count): (u32, u32, u32) = connection
        .query_row(
            "SELECT (SELECT COUNT(*) FROM tender_queries),
                    (SELECT COUNT(*) FROM tender_query_versions),
                    (SELECT COUNT(*) FROM tender_query_target_invalidations)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(sql_error)?;
    let publication_count = proposals.len().saturating_add(updates.len());
    if usize::try_from(query_count)
        .ok()
        .is_none_or(|count| count.saturating_add(proposals.len()) > MAX_TENDER_QUERIES)
        || usize::try_from(version_count).ok().is_none_or(|count| {
            count.saturating_add(publication_count) > MAX_QUERY_VERSIONS_TOTAL as usize
        })
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let mut additional_invalidations = 0usize;
    let mut context_candidates = Vec::new();
    for proposal in proposals {
        if proposal.affected_task_keys != [current_task_key.clone()]
            || proposal.affected_records.iter().any(|record| {
                !exact_inputs.iter().any(|input| {
                    input.kind == "tender_record_version"
                        && input.reference == record.record_id
                        && input.version == record.version
                })
            })
            || proposal
                .evidence
                .iter()
                .any(|reference| !exact_inputs.contains(reference))
        {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let proposed_treatments = proposal
            .proposed_treatments
            .iter()
            .map(|candidate| TenderQueryTreatmentProposal {
                treatment: candidate.treatment,
                rationale: candidate.rationale.trim().to_owned(),
                proposed_by: profile_id.clone(),
                proposed_by_run_id: Some(run_id.clone()),
            })
            .collect::<Vec<_>>();
        let candidate = QueryCandidateRef {
            question: &proposal.question,
            ambiguity_or_gap: &proposal.ambiguity_or_gap,
            owner_profile_id: &profile_id,
            owner_profile_version: profile_version,
            evidence: &proposal.evidence,
            affected_records: &proposal.affected_records,
            affected_task_keys: &proposal.affected_task_keys,
            due_at: &proposal.due_at,
            proposed_treatments: &proposed_treatments,
            responses: &[],
        };
        validate_query_candidate(connection, candidate)?;
        validate_query_publication_targets(connection, &proposal.affected_task_keys)?;
        context_candidates.push((None, query_candidate_context_bound(candidate)?));
        additional_invalidations =
            additional_invalidations.saturating_add(query_target_invalidation_bound(
                &proposal.affected_records,
                &proposal.affected_task_keys,
            ));
    }
    let unique_updates = updates
        .iter()
        .map(|update| update.query_id.as_str())
        .collect::<HashSet<_>>();
    if unique_updates.len() != updates.len() {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    for update in updates {
        if update.query_id.len() != 32
            || update.base_version == 0
            || update.base_version >= MAX_QUERY_VERSIONS
            || (update.added_evidence.is_empty()
                && update.proposed_treatments.is_empty()
                && update.response.is_none())
            || update
                .added_evidence
                .iter()
                .chain(update.response_evidence.iter())
                .any(|reference| !exact_inputs.contains(reference))
        {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let head: Option<u32> = connection
            .query_row(
                "SELECT current_version FROM tender_query_heads WHERE query_id = ?1",
                [&update.query_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_error)?;
        if head != Some(update.base_version) {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let prior = load_query_version_row(connection, &update.query_id, update.base_version)?
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        if (!query_control
            && (prior.affected_task_keys.len() != 1
                || prior.affected_task_keys[0] != current_task_key))
            || (query_control
                && !prior
                    .affected_task_keys
                    .iter()
                    .any(|key| key == &current_task_key || key == ALL_PRODUCTION_TASKS_SCOPE))
        {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let mut evidence = prior.evidence.clone();
        evidence.extend(update.added_evidence.iter().cloned());
        evidence.sort_by(|left, right| {
            (&left.kind, &left.reference, left.version).cmp(&(
                &right.kind,
                &right.reference,
                right.version,
            ))
        });
        if evidence.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let mut proposed_treatments = prior.proposed_treatments.clone();
        proposed_treatments.extend(update.proposed_treatments.iter().map(|candidate| {
            TenderQueryTreatmentProposal {
                treatment: candidate.treatment,
                rationale: candidate.rationale.trim().to_owned(),
                proposed_by: profile_id.clone(),
                proposed_by_run_id: Some(run_id.clone()),
            }
        }));
        let mut responses = prior.responses.clone();
        if let Some(response) = update.response.as_deref() {
            let mut sequence = responses.len().saturating_add(1);
            let response_id = loop {
                let candidate = format!("{sequence:032x}");
                if responses
                    .iter()
                    .all(|existing| existing.response_id != candidate)
                {
                    break candidate;
                }
                sequence = sequence.saturating_add(1);
            };
            responses.push(TenderQueryResponse {
                response_id,
                response: response.trim().to_owned(),
                evidence: update.response_evidence.clone(),
                registered_by: profile_id.clone(),
                created_at: String::new(),
            });
        } else if !update.response_evidence.is_empty() {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let candidate = QueryCandidateRef {
            question: &prior.question,
            ambiguity_or_gap: &prior.ambiguity_or_gap,
            owner_profile_id: &prior.owner_profile_id,
            owner_profile_version: prior.owner_profile_version,
            evidence: &evidence,
            affected_records: &prior.affected_records,
            affected_task_keys: &prior.affected_task_keys,
            due_at: &prior.due_at,
            proposed_treatments: &proposed_treatments,
            responses: &responses,
        };
        validate_query_candidate(connection, candidate)?;
        validate_query_publication_targets(connection, &prior.affected_task_keys)?;
        context_candidates.push((
            Some(update.query_id.clone()),
            query_candidate_context_bound(candidate)?,
        ));
        additional_invalidations = additional_invalidations.saturating_add(
            query_target_invalidation_bound(&prior.affected_records, &prior.affected_task_keys),
        );
    }
    if usize::try_from(invalidation_count)
        .ok()
        .is_none_or(|count| {
            count.saturating_add(additional_invalidations) > MAX_QUERY_INVALIDATIONS_TOTAL as usize
        })
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let context_candidate_refs = context_candidates
        .iter()
        .map(|(query_id, bound)| (query_id.as_deref(), *bound))
        .collect::<Vec<_>>();
    if !query_context_candidates_fit(connection, &context_candidate_refs)? {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok(())
}

pub(crate) fn publish_agent_query_proposals(
    transaction: &Transaction<'_>,
    publication: AgentQueryPublication<'_>,
) -> Result<(), TenderCommandError> {
    let AgentQueryPublication {
        tender_id,
        run_id,
        task_id,
        current_task_key,
        proposals,
        updates,
        query_control,
        created_at,
    } = publication;
    if !agent_query_publication_is_valid(transaction, task_id, proposals, updates, query_control)? {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    if proposals.is_empty() && updates.is_empty() {
        return Ok(());
    }
    if proposals.len() > MAX_QUERY_PROPOSED_TREATMENTS
        || updates.len() > MAX_QUERY_PROPOSED_TREATMENTS
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let (profile_id, profile_version, exact_inputs_json): (String, u32, String) = transaction
        .query_row(
            "SELECT runs.profile_id, runs.profile_version, tasks.exact_inputs_json
             FROM agent_runs AS runs
             JOIN tender_tasks AS tasks ON tasks.task_id = runs.task_id
             WHERE runs.run_id = ?1 AND runs.task_id = ?2",
            params![run_id, task_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(sql_error)?;
    let exact_inputs: Vec<AgentTaskInputReference> = serde_json::from_str(&exact_inputs_json)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    let current_count: u32 = transaction
        .query_row("SELECT COUNT(*) FROM tender_queries", [], |row| row.get(0))
        .map_err(sql_error)?;
    if usize::try_from(current_count)
        .ok()
        .is_none_or(|count| count.saturating_add(proposals.len()) > MAX_TENDER_QUERIES)
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    for proposal in proposals {
        if proposal.affected_task_keys.len() != 1
            || proposal.affected_task_keys[0] != current_task_key
            || proposal.affected_records.iter().any(|record| {
                !exact_inputs.iter().any(|input| {
                    input.kind == "tender_record_version"
                        && input.reference == record.record_id
                        && input.version == record.version
                })
            })
            || proposal
                .evidence
                .iter()
                .any(|reference| !exact_inputs.contains(reference))
        {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let proposed_treatments = proposal
            .proposed_treatments
            .iter()
            .map(|candidate| TenderQueryTreatmentProposal {
                treatment: candidate.treatment,
                rationale: candidate.rationale.trim().to_owned(),
                proposed_by: profile_id.clone(),
                proposed_by_run_id: Some(run_id.to_owned()),
            })
            .collect::<Vec<_>>();
        validate_query_candidate(
            transaction,
            QueryCandidateRef {
                question: &proposal.question,
                ambiguity_or_gap: &proposal.ambiguity_or_gap,
                owner_profile_id: &profile_id,
                owner_profile_version: profile_version,
                evidence: &proposal.evidence,
                affected_records: &proposal.affected_records,
                affected_task_keys: &proposal.affected_task_keys,
                due_at: &proposal.due_at,
                proposed_treatments: &proposed_treatments,
                responses: &[],
            },
        )?;
        validate_query_publication_targets(transaction, &proposal.affected_task_keys)?;
        if !query_publication_has_capacity(
            transaction,
            &proposal.affected_records,
            &proposal.affected_task_keys,
        )? {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let query_id = random_identifier(transaction)?;
        transaction
            .execute(
                "INSERT INTO tender_queries (query_id, created_at) VALUES (?1, ?2)",
                params![query_id, created_at],
            )
            .map_err(sql_error)?;
        let invalidation_targets = collect_query_invalidation_targets(
            transaction,
            &proposal.affected_records,
            &proposal.affected_task_keys,
        )?;
        let manifest_sha256 = insert_query_version(
            transaction,
            QueryVersionInsert {
                query_id: &query_id,
                version: 1,
                query_type: proposal.query_type,
                question: proposal.question.trim(),
                ambiguity_or_gap: proposal.ambiguity_or_gap.trim(),
                owner_profile_id: &profile_id,
                owner_profile_version: profile_version,
                evidence: &proposal.evidence,
                affected_records: &proposal.affected_records,
                affected_task_keys: &proposal.affected_task_keys,
                invalidation_targets: &invalidation_targets,
                due_at: &proposal.due_at,
                material: proposal.material,
                release_blocking: proposal.release_blocking,
                proposed_treatments: &proposed_treatments,
                responses: &[],
                source_run_id: Some(run_id),
                created_by: "agent_run",
                created_at,
            },
        )?;
        transaction
            .execute(
                "INSERT INTO tender_query_heads (query_id, current_version) VALUES (?1, 1)",
                [&query_id],
            )
            .map_err(sql_error)?;
        invalidate_query_targets(
            transaction,
            QueryTargetInvalidation {
                query_id: &query_id,
                query_version: 1,
                targets: &invalidation_targets,
                reason: "query_opened",
                blocks_dependent_work: proposal.material || proposal.release_blocking,
                created_at,
            },
        )?;
        append_query_event(
            transaction,
            tender_id,
            "tender_query_proposed_by_agent",
            &query_id,
            1,
            &manifest_sha256,
            created_at,
        )?;
    }
    for update in updates {
        if update.query_id.len() != 32
            || update.base_version == 0
            || update.base_version >= MAX_QUERY_VERSIONS
            || (update.added_evidence.is_empty()
                && update.proposed_treatments.is_empty()
                && update.response.is_none())
            || update
                .added_evidence
                .iter()
                .chain(update.response_evidence.iter())
                .any(|reference| !exact_inputs.contains(reference))
        {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let head: Option<u32> = transaction
            .query_row(
                "SELECT current_version FROM tender_query_heads WHERE query_id = ?1",
                [&update.query_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_error)?;
        if head != Some(update.base_version) {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let prior = load_query_version_row(transaction, &update.query_id, update.base_version)?
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        if (!query_control
            && (prior.affected_task_keys.len() != 1
                || prior.affected_task_keys[0] != current_task_key))
            || (query_control
                && !prior
                    .affected_task_keys
                    .iter()
                    .any(|key| key == current_task_key || key == ALL_PRODUCTION_TASKS_SCOPE))
        {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let mut evidence = prior.evidence.clone();
        evidence.extend(update.added_evidence.iter().cloned());
        evidence.sort_by(|left, right| {
            (&left.kind, &left.reference, left.version).cmp(&(
                &right.kind,
                &right.reference,
                right.version,
            ))
        });
        if evidence.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let mut proposed_treatments = prior.proposed_treatments.clone();
        proposed_treatments.extend(update.proposed_treatments.iter().map(|candidate| {
            TenderQueryTreatmentProposal {
                treatment: candidate.treatment,
                rationale: candidate.rationale.trim().to_owned(),
                proposed_by: profile_id.clone(),
                proposed_by_run_id: Some(run_id.to_owned()),
            }
        }));
        let mut responses = prior.responses.clone();
        if let Some(response) = update.response.as_deref() {
            responses.push(TenderQueryResponse {
                response_id: random_identifier(transaction)?,
                response: response.trim().to_owned(),
                evidence: update.response_evidence.clone(),
                registered_by: profile_id.clone(),
                created_at: created_at.to_owned(),
            });
        } else if !update.response_evidence.is_empty() {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        validate_query_candidate(
            transaction,
            QueryCandidateRef {
                question: &prior.question,
                ambiguity_or_gap: &prior.ambiguity_or_gap,
                owner_profile_id: &prior.owner_profile_id,
                owner_profile_version: prior.owner_profile_version,
                evidence: &evidence,
                affected_records: &prior.affected_records,
                affected_task_keys: &prior.affected_task_keys,
                due_at: &prior.due_at,
                proposed_treatments: &proposed_treatments,
                responses: &responses,
            },
        )?;
        validate_query_publication_targets(transaction, &prior.affected_task_keys)?;
        if !query_publication_has_capacity(
            transaction,
            &prior.affected_records,
            &prior.affected_task_keys,
        )? {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let version = update.base_version + 1;
        let remains_release_blocking = prior.release_blocking
            || load_query_decision(transaction, &update.query_id, update.base_version)?
                .is_some_and(|decision| !decision.treatment.permits_dependent_work());
        let invalidation_targets = collect_query_invalidation_targets(
            transaction,
            &prior.affected_records,
            &prior.affected_task_keys,
        )?;
        let manifest_sha256 = insert_query_version(
            transaction,
            QueryVersionInsert {
                query_id: &update.query_id,
                version,
                query_type: prior.query_type,
                question: &prior.question,
                ambiguity_or_gap: &prior.ambiguity_or_gap,
                owner_profile_id: &prior.owner_profile_id,
                owner_profile_version: prior.owner_profile_version,
                evidence: &evidence,
                affected_records: &prior.affected_records,
                affected_task_keys: &prior.affected_task_keys,
                invalidation_targets: &invalidation_targets,
                due_at: &prior.due_at,
                material: prior.material,
                release_blocking: remains_release_blocking,
                proposed_treatments: &proposed_treatments,
                responses: &responses,
                source_run_id: Some(run_id),
                created_by: "agent_run",
                created_at,
            },
        )?;
        if transaction
            .execute(
                "UPDATE tender_query_heads SET current_version = ?2
                 WHERE query_id = ?1 AND current_version = ?3",
                params![update.query_id, version, update.base_version],
            )
            .map_err(sql_error)?
            != 1
        {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let reason = if update.response.is_some() {
            "response_added"
        } else if !update.added_evidence.is_empty() {
            "evidence_changed"
        } else {
            "treatment_changed"
        };
        invalidate_query_targets(
            transaction,
            QueryTargetInvalidation {
                query_id: &update.query_id,
                query_version: version,
                targets: &invalidation_targets,
                reason,
                blocks_dependent_work: prior.material || remains_release_blocking,
                created_at,
            },
        )?;
        append_query_event(
            transaction,
            tender_id,
            "tender_query_updated_by_agent",
            &update.query_id,
            version,
            &manifest_sha256,
            created_at,
        )?;
    }
    Ok(())
}

pub(crate) fn approved_query_treatments_for_task(
    connection: &rusqlite::Connection,
    task_key: &str,
) -> Result<Vec<ApprovedQueryTreatment>, TenderCommandError> {
    let mut statement = connection
        .prepare(
            "SELECT decisions.query_id, decisions.query_version
             FROM tender_query_heads AS heads
             JOIN tender_query_versions AS versions
               ON versions.query_id = heads.query_id
              AND versions.version = heads.current_version
             JOIN json_each(versions.affected_task_keys_json) AS affected
             JOIN tender_query_treatment_decisions AS decisions
               ON decisions.query_id = versions.query_id
              AND decisions.query_version = versions.version
             WHERE (affected.value = ?1 OR affected.value = '*')
               AND decisions.treatment NOT IN ('external_rfi_drafting', 'blocked')
             ORDER BY versions.rowid",
        )
        .map_err(sql_error)?;
    let rows = statement
        .query_map([task_key], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?))
        })
        .map_err(sql_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql_error)?;
    rows.into_iter()
        .map(|(query_id, version)| {
            load_query_decision(connection, &query_id, version)?
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))
        })
        .collect()
}

pub(crate) fn approved_query_treatments_for_inputs(
    connection: &rusqlite::Connection,
    inputs: &[AgentTaskInputReference],
) -> Result<Vec<ApprovedQueryTreatment>, TenderCommandError> {
    inputs
        .iter()
        .filter(|input| input.kind == "approved_query_treatment")
        .map(|input| {
            let decision: Option<(String, u32)> = connection
                .query_row(
                    "SELECT query_id, query_version FROM tender_query_treatment_decisions
                     WHERE decision_id = ?1 AND query_version = ?2",
                    params![input.reference, input.version],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()
                .map_err(sql_error)?;
            let (query_id, version) = decision
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            load_query_decision(connection, &query_id, version)?
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))
        })
        .collect()
}

fn append_query_event(
    transaction: &Transaction<'_>,
    tender_id: &TenderId,
    event_type: &str,
    query_id: &str,
    query_version: u32,
    manifest_sha256: &str,
    created_at: &str,
) -> Result<(), TenderCommandError> {
    let tender_revision: u32 = transaction
        .query_row(
            "SELECT current_revision FROM tender WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    append_audit_event(
        transaction,
        tender_id.as_str(),
        event_type,
        tender_revision,
        json!({
            "manifest_sha256": manifest_sha256,
            "query_id": query_id,
            "query_version": query_version.to_string(),
        }),
        created_at,
    )
}

fn append_query_denial(
    transaction: &Transaction<'_>,
    tender_id: &TenderId,
    command: &str,
    query_id: Option<&str>,
    reason: &str,
) -> Result<(), TenderCommandError> {
    let tender_revision: u32 = transaction
        .query_row(
            "SELECT current_revision FROM tender WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    let created_at = sqlite_timestamp(transaction)?;
    append_audit_event(
        transaction,
        tender_id.as_str(),
        "tender_query_command_denied",
        tender_revision,
        json!({ "command": command, "query_id": query_id, "reason": reason }),
        &created_at,
    )
}

fn references_are_unique(references: &[AgentTaskInputReference]) -> bool {
    references
        .iter()
        .map(|reference| (&reference.kind, &reference.reference, reference.version))
        .collect::<HashSet<_>>()
        .len()
        == references.len()
}

fn records_are_unique(records: &[TenderRecordVersionReference]) -> bool {
    records
        .iter()
        .map(|record| (&record.record_id, record.version))
        .collect::<HashSet<_>>()
        .len()
        == records.len()
}

fn strings_are_unique(values: &[String]) -> bool {
    values.iter().collect::<HashSet<_>>().len() == values.len()
}

fn canonical_json<T: Serialize + ?Sized>(value: &T) -> Result<String, TenderCommandError> {
    serde_json_canonicalizer::to_string(&value)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))
}
