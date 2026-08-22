use std::time::Duration;

use jiff::Timestamp;
use reqwest::{Client, StatusCode};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use crate::application_settings::{
    AiProviderKind, ProviderConnectionStatus, ProviderConnectionView, ProviderModelOption,
    ProviderReasoningOption, ProviderReasoningSelection, GEMINI_ADAPTER_VERSION,
    GEMINI_CONNECTION_ID,
};

use super::{
    codex_protocol::{dynamic_tool_specs, provider_instruction_bundle, validate_candidate},
    AgentRunState, PendingProviderEvent, PreparedAgentRun, ProviderEventKind, ProviderExecution,
    ProviderFailure, ProviderFailureCategory, ProviderUsage, RunCallbacks, PROVIDER_OUTPUT_LIMIT,
};

const API_ORIGIN: &str = "https://generativelanguage.googleapis.com";
const MAX_CATALOGUE_PAGES: usize = 100;

pub(super) async fn fetch_connection(
    api_key: &str,
) -> Result<ProviderConnectionView, ProviderFailure> {
    let client = client(Duration::from_secs(45))?;
    let mut models = Vec::new();
    let mut page_token: Option<String> = None;
    for _ in 0..MAX_CATALOGUE_PAGES {
        let mut request = client
            .get(format!("{API_ORIGIN}/v1beta/models"))
            .header("x-goog-api-key", api_key)
            .query(&[("pageSize", "1000")]);
        if let Some(token) = &page_token {
            request = request.query(&[("pageToken", token)]);
        }
        let response = request.send().await.map_err(|_| transport_failure(false))?;
        if !response.status().is_success() {
            return Err(status_failure(response.status(), false));
        }
        let payload: Value = response.json().await.map_err(|_| protocol_failure(false))?;
        let page = payload
            .get("models")
            .and_then(Value::as_array)
            .ok_or_else(|| protocol_failure(false))?;
        for raw in page {
            if let Some(model) = parse_model(raw)? {
                if models
                    .iter()
                    .any(|current: &ProviderModelOption| current.model_id == model.model_id)
                {
                    return Err(protocol_failure(false));
                }
                models.push(model);
            }
        }
        page_token = payload
            .get("nextPageToken")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        if page_token.is_none() {
            break;
        }
    }
    if models.is_empty() || page_token.is_some() {
        return Err(protocol_failure(false));
    }
    Ok(ProviderConnectionView {
        connection_id: GEMINI_CONNECTION_ID.to_owned(),
        provider: AiProviderKind::Gemini,
        display_name: "Google Gemini API key".to_owned(),
        status: ProviderConnectionStatus::Ready,
        account_label: Some("API key stored in the system credential vault".to_owned()),
        account_plan: None,
        models,
        catalogue_fetched_at: Some(Timestamp::now().to_string()),
        adapter_version: GEMINI_ADAPTER_VERSION.to_owned(),
        status_summary: "Ready to run Tender work through Gemini.".to_owned(),
    })
}

fn parse_model(raw: &Value) -> Result<Option<ProviderModelOption>, ProviderFailure> {
    let methods = raw
        .get("supportedGenerationMethods")
        .and_then(Value::as_array)
        .ok_or_else(|| protocol_failure(false))?;
    if !methods
        .iter()
        .any(|method| method.as_str() == Some("generateContent"))
    {
        return Ok(None);
    }
    let model_id = bounded_text(raw.get("name"), 240)?;
    if !valid_model_resource(&model_id) {
        return Err(protocol_failure(false));
    }
    let display_name = bounded_text(raw.get("displayName"), 300)?;
    let provider_description = raw
        .get("description")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| value.len() <= 1_000)
        .unwrap_or("Live Gemini model");
    let input_limit = raw.get("inputTokenLimit").and_then(Value::as_u64);
    let output_limit = raw.get("outputTokenLimit").and_then(Value::as_u64);
    if input_limit == Some(0) || output_limit == Some(0) {
        return Ok(None);
    }
    let thinking = raw.get("thinking").and_then(Value::as_bool) == Some(true);
    Ok(Some(ProviderModelOption {
        model_id,
        display_name,
        description: format!(
            "{provider_description} Thinking: {}. Input limit: {}. Output limit: {}.",
            if thinking { "supported" } else { "not advertised" },
            input_limit.map_or_else(|| "not advertised".to_owned(), |value| value.to_string()),
            output_limit.map_or_else(|| "not advertised".to_owned(), |value| value.to_string()),
        ),
        is_default: false,
        input_modalities: vec!["text".to_owned()],
        reasoning_options: vec![ProviderReasoningOption {
            selection: ProviderReasoningSelection::ProviderDefault,
            label: if thinking { "Automatic" } else { "Provider default" }.to_owned(),
            description: "Gemini does not advertise exact reasoning levels in the live Models API; Quantix does not invent them.".to_owned(),
            is_default: true,
        }],
    }))
}

fn valid_model_resource(value: &str) -> bool {
    value.strip_prefix("models/").is_some_and(|id| {
        !id.is_empty()
            && id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    })
}

fn bounded_text(value: Option<&Value>, maximum: usize) -> Result<String, ProviderFailure> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty() && value.len() <= maximum)
        .map(str::to_owned)
        .ok_or_else(|| protocol_failure(false))
}

pub(super) async fn run_turn(
    api_key: String,
    prepared: PreparedAgentRun,
    operation_limit: Duration,
    cancellation: CancellationToken,
    callbacks: RunCallbacks,
) -> ProviderExecution {
    let failed = |failure: ProviderFailure| ProviderExecution {
        state: match failure.category {
            ProviderFailureCategory::OutcomeUnknown => AgentRunState::Indeterminate,
            ProviderFailureCategory::Interrupted => AgentRunState::Interrupted,
            _ => AgentRunState::Failed,
        },
        provider_thread_ref: None,
        provider_turn_ref: None,
        events: vec![PendingProviderEvent::new(
            ProviderEventKind::Terminal,
            "Gemini Provider Turn did not complete",
            None,
        )],
        usage: ProviderUsage::default(),
        failure: Some(failure),
        candidate_payload_json: None,
    };
    if prepared.provider_selection.provider != AiProviderKind::Gemini
        || prepared.provider_selection.reasoning != ProviderReasoningSelection::ProviderDefault
        || !valid_model_resource(&prepared.provider_selection.model_id)
    {
        return failed(protocol_failure(false));
    }
    let RunCallbacks {
        on_thread_archived,
        on_thread_established,
        on_requested,
        on_accepted,
        mut on_event,
        mut on_tool_call,
    } = callbacks;
    if let Some(archive) = prepared.provider_thread_to_archive.as_deref() {
        if let Err(failure) = on_thread_archived(archive) {
            return failed(failure);
        }
    }
    let resumed = prepared.provider_thread_ref.is_some();
    let thread_ref = prepared
        .provider_thread_ref
        .clone()
        .unwrap_or_else(|| format!("gemini:{}", prepared.run_id));
    if let Err(failure) = on_thread_established(&thread_ref, resumed) {
        return failed(failure);
    }
    if let Err(failure) = on_requested() {
        return failed(failure);
    }
    let instruction = match provider_instruction_bundle(&prepared) {
        Ok(value) => value,
        Err(failure) => return failed(failure),
    };
    let output_schema: Value = match serde_json::from_str(&prepared.task.output_contract_json) {
        Ok(value) => value,
        Err(_) => return failed(protocol_failure(false)),
    };
    let tools = match gemini_tools(&prepared) {
        Ok(value) => value,
        Err(failure) => return failed(failure),
    };
    let client = match client(operation_limit) {
        Ok(value) => value,
        Err(failure) => return failed(failure),
    };
    let mut contents = vec![json!({
        "role": "user",
        "parts": [{"text": "Perform the exact Quantix task supplied in the system instruction."}],
    })];
    let mut usage = ProviderUsage::default();
    let mut events = Vec::new();
    let mut accepted = Some(on_accepted);
    let mut turn_ref: Option<String> = None;
    for _ in 0..prepared.task.resource_budget.provider_turns.max(1) {
        if cancellation.is_cancelled() {
            return interrupted(thread_ref, turn_ref, events, usage);
        }
        let body = json!({
            "systemInstruction": {"parts": [{"text": instruction.clone()}]},
            "contents": contents.clone(),
            "tools": tools.clone(),
            "generationConfig": {
                "responseMimeType": "application/json",
                "responseJsonSchema": output_schema.clone(),
                "maxOutputTokens": maximum_output_tokens(&prepared),
                "candidateCount": 1,
            }
        });
        let url = format!(
            "{API_ORIGIN}/v1beta/{}:streamGenerateContent?alt=sse",
            prepared.provider_selection.model_id
        );
        let response = tokio::select! {
            _ = cancellation.cancelled() => return interrupted(thread_ref, turn_ref, events, usage),
            response = client.post(url).header("x-goog-api-key", &api_key).json(&body).send() => response,
        };
        let mut response = match response {
            Ok(value) => value,
            Err(_) => return failed(transport_failure(turn_ref.is_some())),
        };
        if !response.status().is_success() {
            return failed(status_failure(response.status(), turn_ref.is_some()));
        }
        let streamed = match collect_stream(&mut response, &cancellation, turn_ref.is_some()).await
        {
            Ok(value) => value,
            Err(failure) => return failed(failure),
        };
        merge_usage(&streamed.usage, &mut usage);
        if turn_ref.is_none() {
            let reference = match streamed.response_id.clone() {
                Some(value) => value,
                None => return failed(protocol_failure(false)),
            };
            let callback = match accepted.take() {
                Some(value) => value,
                None => return failed(protocol_failure(true)),
            };
            if let Err(failure) = callback(&reference) {
                return failed(failure);
            }
            turn_ref = Some(reference.clone());
            let event = PendingProviderEvent::new(
                ProviderEventKind::TurnStarted,
                "Gemini accepted the Provider Turn",
                Some(&reference),
            );
            if let Err(failure) = on_event(&event, &usage) {
                return failed(failure);
            }
            events.push(event);
        }
        let stream_event = PendingProviderEvent::new(
            ProviderEventKind::Warning,
            "Gemini streamed Provider activity",
            turn_ref.as_deref(),
        );
        if let Err(failure) = on_event(&stream_event, &usage) {
            return failed(failure);
        }
        events.push(stream_event);
        if !streamed.function_calls.is_empty() {
            contents.push(json!({"role": "model", "parts": streamed.parts.clone()}));
            let mut results = Vec::new();
            for call in streamed.function_calls {
                let call_id = call
                    .get("id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| protocol_failure(true));
                let call_id = match call_id {
                    Ok(value) => value,
                    Err(failure) => return failed(failure),
                };
                let name = match call.get("name").and_then(Value::as_str) {
                    Some(value) => value,
                    None => return failed(protocol_failure(true)),
                };
                let args = call.get("args").cloned().unwrap_or_else(|| json!({}));
                let result = match on_tool_call(call_id, name, &args) {
                    Ok(Some(value)) => json!({"result": value}),
                    Ok(None) => json!({"error": "Quantix denied this tool request."}),
                    Err(failure) => return failed(failure),
                };
                results.push(json!({
                    "functionResponse": {"name": name, "response": result, "id": call_id}
                }));
            }
            contents.push(json!({"role": "user", "parts": results}));
            continue;
        }
        if streamed.finish_reason.as_deref() != Some("STOP") {
            return failed(output_failure());
        }
        let candidate = match validate_candidate(
            Some(&streamed.text),
            &output_schema,
            prepared
                .task
                .resource_budget
                .output_bytes
                .min(PROVIDER_OUTPUT_LIMIT as u32),
        ) {
            Ok(value) => value,
            Err(failure) => return failed(failure),
        };
        let usage_event = PendingProviderEvent::new(
            ProviderEventKind::UsageObserved,
            "Gemini usage observed",
            None,
        );
        if let Err(failure) = on_event(&usage_event, &usage) {
            return failed(failure);
        }
        events.push(usage_event);
        events.push(PendingProviderEvent::new(
            ProviderEventKind::Terminal,
            "Gemini returned a schema-valid proposed result",
            turn_ref.as_deref(),
        ));
        return ProviderExecution {
            state: AgentRunState::Completed,
            provider_thread_ref: Some(thread_ref),
            provider_turn_ref: turn_ref,
            events,
            usage,
            failure: None,
            candidate_payload_json: Some(candidate),
        };
    }
    failed(ProviderFailure::new(
        ProviderFailureCategory::RateLimited,
        true,
        "Increase the approved Provider Turn budget or create a linked retry.",
        Some("Gemini exhausted the exact tool-turn budget."),
    ))
}

fn gemini_tools(prepared: &PreparedAgentRun) -> Result<Vec<Value>, ProviderFailure> {
    let declarations = dynamic_tool_specs(&prepared.permission_grant)?
        .into_iter()
        .map(|tool| {
            Ok(json!({
                "name": tool.get("name").cloned().ok_or_else(|| protocol_failure(false))?,
                "description": tool.get("description").cloned().ok_or_else(|| protocol_failure(false))?,
                "parametersJsonSchema": tool.get("inputSchema").cloned().ok_or_else(|| protocol_failure(false))?,
            }))
        })
        .collect::<Result<Vec<_>, ProviderFailure>>()?;
    if declarations.is_empty() {
        Ok(Vec::new())
    } else {
        Ok(vec![json!({"functionDeclarations": declarations})])
    }
}

struct StreamedResponse {
    response_id: Option<String>,
    text: String,
    parts: Vec<Value>,
    function_calls: Vec<Value>,
    finish_reason: Option<String>,
    usage: ProviderUsage,
}

async fn collect_stream(
    response: &mut reqwest::Response,
    cancellation: &CancellationToken,
    accepted: bool,
) -> Result<StreamedResponse, ProviderFailure> {
    let mut buffer = Vec::<u8>::new();
    let mut output = StreamedResponse {
        response_id: None,
        text: String::new(),
        parts: Vec::new(),
        function_calls: Vec::new(),
        finish_reason: None,
        usage: ProviderUsage::default(),
    };
    loop {
        let chunk = tokio::select! {
            _ = cancellation.cancelled() => return Err(interruption_failure(accepted || output.response_id.is_some())),
            chunk = response.chunk() => chunk.map_err(|_| transport_failure(accepted || output.response_id.is_some()))?,
        };
        let Some(chunk) = chunk else { break };
        buffer.extend_from_slice(&chunk);
        if buffer.len() > PROVIDER_OUTPUT_LIMIT {
            return Err(output_failure());
        }
        while let Some((boundary, delimiter)) = sse_boundary(&buffer) {
            let frame = std::str::from_utf8(&buffer[..boundary])
                .map_err(|_| protocol_failure(accepted || output.response_id.is_some()))?
                .to_owned();
            buffer.drain(..boundary + delimiter);
            let data = frame
                .lines()
                .filter_map(|line| line.strip_prefix("data: "))
                .collect::<String>();
            if data.is_empty() {
                continue;
            }
            let payload: Value = serde_json::from_str(&data)
                .map_err(|_| protocol_failure(accepted || output.response_id.is_some()))?;
            if let Some(id) = payload.get("responseId").and_then(Value::as_str) {
                if output
                    .response_id
                    .as_deref()
                    .is_some_and(|current| current != id)
                {
                    return Err(protocol_failure(true));
                }
                output.response_id = Some(id.to_owned());
            }
            if let Some(metadata) = payload.get("usageMetadata") {
                output.usage = parse_usage(metadata)?;
            }
            let Some(candidates) = payload.get("candidates").and_then(Value::as_array) else {
                if payload.get("usageMetadata").is_some() {
                    continue;
                }
                return Err(protocol_failure(output.response_id.is_some()));
            };
            if candidates.len() != 1 {
                return Err(protocol_failure(output.response_id.is_some()));
            }
            let candidate = &candidates[0];
            if let Some(reason) = candidate.get("finishReason").and_then(Value::as_str) {
                output.finish_reason = Some(reason.to_owned());
            }
            if let Some(parts) = candidate
                .pointer("/content/parts")
                .and_then(Value::as_array)
            {
                for part in parts {
                    if let Some(text) = part.get("text").and_then(Value::as_str) {
                        if output.text.len().saturating_add(text.len()) > PROVIDER_OUTPUT_LIMIT {
                            return Err(output_failure());
                        }
                        output.text.push_str(text);
                    }
                    if let Some(call) = part.get("functionCall") {
                        output.function_calls.push(call.clone());
                    }
                    output.parts.push(part.clone());
                }
            }
        }
    }
    if output.response_id.is_none() || (output.text.is_empty() && output.function_calls.is_empty())
    {
        return Err(protocol_failure(accepted));
    }
    Ok(output)
}

fn parse_usage(value: &Value) -> Result<ProviderUsage, ProviderFailure> {
    if !value.is_object() {
        return Err(protocol_failure(true));
    }
    Ok(ProviderUsage {
        input_tokens: value.get("promptTokenCount").and_then(Value::as_u64),
        cached_input_tokens: value.get("cachedContentTokenCount").and_then(Value::as_u64),
        output_tokens: value.get("candidatesTokenCount").and_then(Value::as_u64),
        reasoning_output_tokens: value.get("thoughtsTokenCount").and_then(Value::as_u64),
        total_tokens: value.get("totalTokenCount").and_then(Value::as_u64),
        context_window: None,
        elapsed_milliseconds: None,
        rate_limit: None,
    })
}

fn merge_usage(current: &ProviderUsage, total: &mut ProviderUsage) {
    let add =
        |left: Option<u64>, right: Option<u64>| left.unwrap_or(0).checked_add(right.unwrap_or(0));
    total.input_tokens = add(total.input_tokens, current.input_tokens);
    total.cached_input_tokens = add(total.cached_input_tokens, current.cached_input_tokens);
    total.output_tokens = add(total.output_tokens, current.output_tokens);
    total.reasoning_output_tokens = add(
        total.reasoning_output_tokens,
        current.reasoning_output_tokens,
    );
    total.total_tokens = add(total.total_tokens, current.total_tokens);
}

fn sse_boundary(buffer: &[u8]) -> Option<(usize, usize)> {
    let lf = buffer.windows(2).position(|value| value == b"\n\n");
    let crlf = buffer.windows(4).position(|value| value == b"\r\n\r\n");
    match (lf, crlf) {
        (Some(left), Some(right)) if left <= right => Some((left, 2)),
        (Some(_), Some(right)) => Some((right, 4)),
        (Some(left), None) => Some((left, 2)),
        (None, Some(right)) => Some((right, 4)),
        _ => None,
    }
}

fn maximum_output_tokens(prepared: &PreparedAgentRun) -> u64 {
    u64::from(prepared.task.resource_budget.output_bytes / 2).clamp(1_024, 65_536)
}

fn client(timeout: Duration) -> Result<Client, ProviderFailure> {
    Client::builder()
        .https_only(true)
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(90))
        .timeout(timeout)
        .user_agent("Quantix/0.1 GeminiAdapter")
        .build()
        .map_err(|_| transport_failure(false))
}

fn status_failure(status: StatusCode, accepted: bool) -> ProviderFailure {
    match status.as_u16() {
        400 | 404 => ProviderFailure::new(
            if accepted {
                ProviderFailureCategory::OutcomeUnknown
            } else {
                ProviderFailureCategory::ProtocolInvalid
            },
            !accepted,
            if accepted {
                "Resolve the quarantined Agent Run before retrying."
            } else {
                "Refresh Gemini models and choose a current capability."
            },
            Some("Gemini rejected the selected model, tool, or output capability."),
        ),
        401 | 403 => ProviderFailure::new(
            ProviderFailureCategory::AuthenticationRequired,
            true,
            "Replace or revoke the Gemini API key in Settings before retrying.",
            Some("Gemini rejected the credential."),
        ),
        429 => ProviderFailure::new(
            ProviderFailureCategory::RateLimited,
            true,
            "Wait for Gemini quota or capacity to recover before retrying.",
            Some("Gemini reported a quota or rate limit."),
        ),
        _ => transport_failure(accepted),
    }
}

fn protocol_failure(accepted: bool) -> ProviderFailure {
    ProviderFailure::new(
        if accepted {
            ProviderFailureCategory::OutcomeUnknown
        } else {
            ProviderFailureCategory::ProtocolInvalid
        },
        !accepted,
        if accepted {
            "Resolve the quarantined Agent Run before retrying."
        } else {
            "Refresh Gemini capabilities before retrying."
        },
        Some("Gemini returned an incompatible or malformed response."),
    )
}

fn transport_failure(accepted: bool) -> ProviderFailure {
    ProviderFailure::new(
        if accepted {
            ProviderFailureCategory::OutcomeUnknown
        } else {
            ProviderFailureCategory::ProcessFailed
        },
        !accepted,
        if accepted {
            "Resolve the quarantined Agent Run before retrying."
        } else {
            "Check the connection and retry Gemini."
        },
        Some("The Gemini request did not complete."),
    )
}

fn interruption_failure(accepted: bool) -> ProviderFailure {
    ProviderFailure::new(
        if accepted {
            ProviderFailureCategory::OutcomeUnknown
        } else {
            ProviderFailureCategory::Interrupted
        },
        !accepted,
        if accepted {
            "Resolve the quarantined Agent Run before retrying."
        } else {
            "Create a linked retry when ready."
        },
        Some("The Gemini request was interrupted."),
    )
}

fn output_failure() -> ProviderFailure {
    ProviderFailure::new(
        ProviderFailureCategory::OutputInvalid,
        true,
        "Create a linked retry after reviewing the output-contract failure.",
        Some("Gemini did not return a valid candidate output."),
    )
}

fn interrupted(
    thread_ref: String,
    turn_ref: Option<String>,
    mut events: Vec<PendingProviderEvent>,
    usage: ProviderUsage,
) -> ProviderExecution {
    let accepted = turn_ref.is_some();
    events.push(PendingProviderEvent::new(
        ProviderEventKind::Terminal,
        "Gemini Provider Turn was interrupted",
        turn_ref.as_deref(),
    ));
    ProviderExecution {
        state: if accepted {
            AgentRunState::Indeterminate
        } else {
            AgentRunState::Interrupted
        },
        provider_thread_ref: Some(thread_ref),
        provider_turn_ref: turn_ref,
        events,
        usage,
        failure: Some(interruption_failure(accepted)),
        candidate_payload_json: None,
    }
}
