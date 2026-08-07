use serde_json::json;

use super::{
    AgentProfileVersionView, AgentResourceBudget, AgentRunPermissions, AgentTaskInputReference,
    TenderTaskView, PROVIDER_TIMEOUT,
};

pub(crate) fn bootstrap_profile(profile_id: String) -> AgentProfileVersionView {
    AgentProfileVersionView {
        profile_id,
        version: 1,
        identity: "Bootstrap Tender Analyst".into(),
        profession: "Tender Engineer".into(),
        capabilities: vec!["analyze_tender_intake_readiness".into()],
        instructions: "Assess only the supplied exact Tender revision and propose the next controlled intake action. State uncertainty explicitly and make no approval decision.".into(),
        output_contract_json: bootstrap_output_contract(),
        review_policy: "Independent review is required before this Proposed result can support Tender work.".into(),
        permissions: bootstrap_permissions(),
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
        permissions: profile.permissions.clone(),
        resource_budget: profile.resource_budget.clone(),
    }
}

fn bootstrap_permissions() -> AgentRunPermissions {
    AgentRunPermissions {
        data_scopes: vec!["tender_metadata".into()],
        allowed_actions: vec!["propose_intake_readiness".into()],
        allowed_tools: Vec::new(),
        network_allowed: false,
        workspace_write_allowed: false,
    }
}

fn bootstrap_resource_budget() -> AgentResourceBudget {
    AgentResourceBudget {
        provider_turns: 1,
        duration_seconds: PROVIDER_TIMEOUT.as_secs() as u32,
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
