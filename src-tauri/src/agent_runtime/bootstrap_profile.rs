use serde_json::json;

use super::{
    permissions::TENDER_METADATA_TOOL_NAME, AgentProfileVersionView, AgentResourceBudget,
    AgentRunPermissions, AgentTaskInputReference, BootstrapRole, DataClassification,
    TenderTaskView,
};

pub(crate) fn bootstrap_profile(
    role: BootstrapRole,
    profile_id: String,
) -> AgentProfileVersionView {
    let (identity, profession, capabilities, instructions, review_policy) = match role {
        BootstrapRole::TenderOfficeCoordinator => (
            "Tender Office Coordinator",
            "Tender Coordination Engineer",
            vec!["coordinate_pre_bid_analysis".into()],
            "Coordinate only restricted pre-bid analysis, dependencies, deadlines, and escalations. Make no Tendering Manager decision and activate no production work.",
            "Coordinator output remains Proposed and cannot replace independent review or an Engineer User decision.",
        ),
        BootstrapRole::DocumentController => (
            "Document Controller",
            "Tender Document Controller",
            vec!["control_tender_sources".into()],
            "Control exact Source Artifact Versions, relationships, the Document Register, and evidence locations without deciding technical or commercial meaning.",
            "Document-control output remains Proposed where it interprets content and requires independent review before reliance.",
        ),
        BootstrapRole::TenderAnalyst => (
            "Bootstrap Tender Analyst",
            "Tender Engineer",
            vec!["analyze_tender_intake_readiness".into()],
            "Assess only the supplied exact Tender revision and propose the next controlled intake action. State uncertainty explicitly and make no approval decision.",
            "Independent review is required before this Proposed result can support Tender work.",
        ),
        BootstrapRole::IndependentReviewer => (
            "Independent Reviewer",
            "Independent Tender Reviewer",
            vec!["independently_review_pre_bid_analysis".into()],
            "Review exact proposed pre-bid records produced by another Agent Profile. Do not edit the target, close findings, or approve.",
            "Review outcomes bind exact target versions and remain non-approving recommendations to the Engineer User.",
        ),
    };
    AgentProfileVersionView {
        profile_id,
        version: 1,
        identity: identity.into(),
        profession: profession.into(),
        seniority: "Senior".into(),
        capabilities,
        objective: instructions.into(),
        behavior: "Work only from exact registered inputs, preserve uncertainty, and escalate blocked decisions.".into(),
        skepticism: "Challenge unsupported claims and require attributable Evidence before reliance.".into(),
        risk_tolerance: "Low tolerance for unverified or irreversible Tender commitments.".into(),
        instructions: instructions.into(),
        output_contract_json: bootstrap_output_contract(),
        review_policy: review_policy.into(),
        permissions: bootstrap_permissions(role),
        prohibited_actions: vec![
            "approve_tender_decision".into(),
            "mutate_tender_store_directly".into(),
            "perform_external_action".into(),
            "access_secret_data".into(),
        ],
        resource_budget: bootstrap_resource_budget(),
    }
}

pub(crate) fn bootstrap_task(
    task_id: String,
    tender_id: &str,
    tender_revision: u32,
    deadline: String,
    profile: &AgentProfileVersionView,
) -> TenderTaskView {
    TenderTaskView {
        task_id,
        profile_id: profile.profile_id.clone(),
        profile_version: profile.version,
        objective: "Assess whether the current Tender is ready for controlled intake analysis and propose exactly one next action.".into(),
        exact_inputs: vec![AgentTaskInputReference {
            kind: "tender_revision".into(),
            reference: tender_id.to_owned(),
            version: tender_revision,
        }],
        output_contract_json: profile.output_contract_json.clone(),
        review_policy: profile.review_policy.clone(),
        deadline,
        permissions: AgentRunPermissions {
            allowed_tools: Vec::new(),
            ..profile.permissions.clone()
        },
        resource_budget: profile.resource_budget.clone(),
    }
}

fn bootstrap_permissions(role: BootstrapRole) -> AgentRunPermissions {
    let (data_scopes, allowed_actions, allowed_tools) = match role {
        BootstrapRole::TenderOfficeCoordinator => (
            vec!["tender_analysis".into()],
            vec!["coordinate_pre_bid_analysis".into()],
            Vec::new(),
        ),
        BootstrapRole::DocumentController => (
            vec!["tender_sources".into()],
            vec!["control_tender_sources".into()],
            Vec::new(),
        ),
        BootstrapRole::TenderAnalyst => (
            vec!["tender_metadata".into()],
            vec!["propose_intake_readiness".into()],
            vec![TENDER_METADATA_TOOL_NAME.into()],
        ),
        BootstrapRole::IndependentReviewer => (
            vec!["tender_analysis".into()],
            vec!["review_pre_bid_analysis".into()],
            Vec::new(),
        ),
    };
    AgentRunPermissions {
        data_scopes,
        data_classifications: vec![DataClassification::TenderInternal],
        allowed_actions,
        allowed_tools,
        network_allowed: false,
        workspace_write_allowed: true,
    }
}

fn bootstrap_resource_budget() -> AgentResourceBudget {
    #[cfg(feature = "runtime-fixture")]
    let duration_seconds = 8;
    #[cfg(not(feature = "runtime-fixture"))]
    let duration_seconds = super::PROVIDER_TIMEOUT.as_secs() as u32;
    AgentResourceBudget {
        provider_turns: 1,
        duration_seconds,
        output_bytes: 16 * 1024,
    }
}

fn bootstrap_output_contract() -> String {
    serde_json_canonicalizer::to_string(&json!({
        "additionalProperties": false,
        "properties": {
            "recommended_next_action": {
                "maxLength": 500,
                "minLength": 1,
                "type": "string"
            },
            "summary": {
                "maxLength": 2000,
                "minLength": 1,
                "type": "string"
            }
        },
        "required": ["summary", "recommended_next_action"],
        "type": "object"
    }))
    .expect("static Bootstrap output contract is canonical JSON")
}
