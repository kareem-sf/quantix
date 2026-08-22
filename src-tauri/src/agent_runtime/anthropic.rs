use std::{collections::BTreeMap, time::Duration};

use jiff::Timestamp;
use reqwest::{Client, StatusCode};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use crate::application_settings::{
    AiProviderKind, ProviderConnectionStatus, ProviderConnectionView, ProviderModelOption,
    ProviderReasoningOption, ProviderReasoningSelection, ANTHROPIC_ADAPTER_VERSION,
    ANTHROPIC_CONNECTION_ID,
};

use super::{
    codex_protocol::{dynamic_tool_specs, provider_instruction_bundle, validate_candidate},
    AgentRunState, PendingProviderEvent, PreparedAgentRun, ProviderEventKind, ProviderExecution,
    ProviderFailure, ProviderFailureCategory, ProviderUsage, RunCallbacks, PROVIDER_OUTPUT_LIMIT,
};

const API_ORIGIN: &str = "https://api.anthropic.com";
const API_VERSION: &str = "2023-06-01";
const MAX_CATALOGUE_PAGES: usize = 100;

pub(super) async fn fetch_connection(
    api_key: &str,
) -> Result<ProviderConnectionView, ProviderFailure> {
    let client = client(Duration::from_secs(45))?;
    let mut models = Vec::new();
    let mut after_id: Option<String> = None;
    for _ in 0..MAX_CATALOGUE_PAGES {
        let mut request = client
            .get(format!("{API_ORIGIN}/v1/models"))
            .header("x-api-key", api_key)
            .header("anthropic-version", API_VERSION)
            .query(&[("limit", "100")]);
        if let Some(cursor) = &after_id {
            request = request.query(&[("after_id", cursor)]);
        }
        let response = request.send().await.map_err(|_| transport_failure(false))?;
        let status = response.status();
        if !status.is_success() {
            return Err(status_failure(status, false));
        }
        let payload: Value = response.json().await.map_err(|_| protocol_failure(false))?;
        let page = payload
            .get("data")
            .and_then(Value::as_array)
            .ok_or_else(|| protocol_failure(false))?;
        for raw in page {
            let model = parse_model(raw)?;
            if models
                .iter()
                .any(|existing: &ProviderModelOption| existing.model_id == model.model_id)
            {
                return Err(protocol_failure(false));
            }
            models.push(model);
        }
        if payload.get("has_more").and_then(Value::as_bool) != Some(true) {
            after_id = None;
            break;
        }
        after_id = payload
            .get("last_id")
            .and_then(Value::as_str)
            .map(str::to_owned);
        if after_id.is_none() {
            return Err(protocol_failure(false));
        }
    }
    if models.is_empty() || after_id.is_some() {
        return Err(protocol_failure(false));
    }
    Ok(ProviderConnectionView {
        connection_id: ANTHROPIC_CONNECTION_ID.to_owned(),
        provider: AiProviderKind::Anthropic,
        display_name: "Anthropic API key".to_owned(),
        status: ProviderConnectionStatus::Ready,
        account_label: Some("API key stored in the system credential vault".to_owned()),
        account_plan: None,
        models,
        catalogue_fetched_at: Some(Timestamp::now().to_string()),
        adapter_version: ANTHROPIC_ADAPTER_VERSION.to_owned(),
        status_summary: "Ready to run Tender work through Anthropic.".to_owned(),
    })
}

fn parse_model(raw: &Value) -> Result<ProviderModelOption, ProviderFailure> {
    let model_id = bounded_text(raw.get("id"), 200)?;
    let display_name = bounded_text(raw.get("display_name"), 300)?;
    let capabilities = raw
        .get("capabilities")
        .and_then(Value::as_object)
        .ok_or_else(|| protocol_failure(false))?;
    let supported = |path: &str| {
        capabilities
            .get(path)
            .and_then(|value| value.get("supported"))
            .and_then(Value::as_bool)
            == Some(true)
    };
    let structured = supported("structured_outputs");
    let max_input = raw.get("max_input_tokens").and_then(Value::as_u64);
    let max_output = raw.get("max_tokens").and_then(Value::as_u64);
    let effort = capabilities.get("effort").and_then(Value::as_object);
    let mut reasoning_options = vec![ProviderReasoningOption {
        selection: ProviderReasoningSelection::ProviderDefault,
        label: "Provider default".to_owned(),
        description: "Let Anthropic apply the model's current default effort.".to_owned(),
        is_default: true,
    }];
    for level in ["low", "medium", "high", "xhigh", "max"] {
        if effort
            .and_then(|value| value.get(level))
            .and_then(|value| value.get("supported"))
            .and_then(Value::as_bool)
            == Some(true)
        {
            reasoning_options.push(ProviderReasoningOption {
                selection: ProviderReasoningSelection::AnthropicEffort(level.to_owned()),
                label: level.to_owned(),
                description: format!("Anthropic advertises {level} effort for this model."),
                is_default: false,
            });
        }
    }
    let mut input_modalities = vec!["text".to_owned()];
    if supported("image_input") {
        input_modalities.push("image".to_owned());
    }
    let description = format!(
        "Live Anthropic model. Structured output: {}. Input limit: {}. Output limit: {}.",
        if structured {
            "supported"
        } else {
            "not advertised"
        },
        max_input.map_or_else(|| "not advertised".to_owned(), |value| value.to_string()),
        max_output.map_or_else(|| "not advertised".to_owned(), |value| value.to_string()),
    );
    Ok(ProviderModelOption {
        model_id,
        display_name,
        description,
        is_default: false,
        input_modalities,
        reasoning_options,
    })
}

fn bounded_text(value: Option<&Value>, maximum: usize) -> Result<String, ProviderFailure> {
    let value = value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty() && value.len() <= maximum)
        .ok_or_else(|| protocol_failure(false))?;
    Ok(value.to_owned())
}

pub(super) async fn run_turn(
    api_key: String,
    prepared: PreparedAgentRun,
    operation_limit: Duration,
    cancellation: CancellationToken,
    callbacks: RunCallbacks,
) -> ProviderExecution {
    let failure_execution = |failure: ProviderFailure| ProviderExecution {
        state: match failure.category {
            ProviderFailureCategory::OutcomeUnknown => AgentRunState::Indeterminate,
            ProviderFailureCategory::Interrupted => AgentRunState::Interrupted,
            _ => AgentRunState::Failed,
        },
        provider_thread_ref: None,
        provider_turn_ref: None,
        events: vec![PendingProviderEvent::new(
            ProviderEventKind::Terminal,
            "Anthropic Provider Turn did not complete",
            None,
        )],
        usage: ProviderUsage::default(),
        failure: Some(failure),
        candidate_payload_json: None,
    };
    let RunCallbacks {
        on_thread_archived,
        on_thread_established,
        on_requested,
        on_accepted,
        mut on_event,
        mut on_tool_call,
    } = callbacks;
    let mut on_accepted = Some(on_accepted);
    if prepared.provider_selection.provider != AiProviderKind::Anthropic {
        return failure_execution(protocol_failure(false));
    }
    if let Some(archived) = prepared.provider_thread_to_archive.as_deref() {
        if let Err(failure) = on_thread_archived(archived) {
            return failure_execution(failure);
        }
    }
    let resumed = prepared.provider_thread_ref.is_some();
    let thread_ref = prepared
        .provider_thread_ref
        .clone()
        .unwrap_or_else(|| format!("anthropic:{}", prepared.run_id));
    if let Err(failure) = on_thread_established(&thread_ref, resumed) {
        return failure_execution(failure);
    }
    if let Err(failure) = on_requested() {
        return failure_execution(failure);
    }
    let instruction = match provider_instruction_bundle(&prepared) {
        Ok(instruction) => instruction,
        Err(failure) => return failure_execution(failure),
    };
    let output_schema: Value = match serde_json::from_str(&prepared.task.output_contract_json) {
        Ok(schema) => schema,
        Err(_) => return failure_execution(protocol_failure(false)),
    };
    let tools = match anthropic_tools(&prepared) {
        Ok(tools) => tools,
        Err(failure) => return failure_execution(failure),
    };
    let client = match client(operation_limit) {
        Ok(client) => client,
        Err(failure) => return failure_execution(failure),
    };
    let mut messages = vec![json!({
        "role": "user",
        "content": [{"type": "text", "text": instruction}],
    })];
    let mut usage = ProviderUsage::default();
    let mut events = vec![PendingProviderEvent::new(
        if resumed {
            ProviderEventKind::ThreadResumed
        } else {
            ProviderEventKind::ThreadEstablished
        },
        if resumed {
            "Anthropic context resumed"
        } else {
            "Anthropic context established"
        },
        Some(&thread_ref),
    )];
    let mut accepted_ref: Option<String> = None;
    let maximum_turns = prepared.task.resource_budget.provider_turns.max(1);
    for _ in 0..maximum_turns {
        if cancellation.is_cancelled() {
            return interrupted_execution(thread_ref, accepted_ref, events, usage);
        }
        let mut body = json!({
            "model": prepared.provider_selection.model_id.clone(),
            "max_tokens": maximum_output_tokens(&prepared),
            "messages": messages.clone(),
            "tools": tools.clone(),
            "stream": true,
            "output_config": {
                "format": {"type": "json_schema", "schema": output_schema.clone()}
            }
        });
        match &prepared.provider_selection.reasoning {
            ProviderReasoningSelection::ProviderDefault => {}
            ProviderReasoningSelection::AnthropicEffort(level) => {
                body["output_config"]["effort"] = Value::String(level.clone());
            }
            ProviderReasoningSelection::CodexEffort(_) => {
                return failure_execution(protocol_failure(false));
            }
        }
        let response = tokio::select! {
            _ = cancellation.cancelled() => {
                return interrupted_execution(thread_ref, accepted_ref, events, usage);
            }
            response = client.post(format!("{API_ORIGIN}/v1/messages"))
                .header("x-api-key", &api_key)
                .header("anthropic-version", API_VERSION)
                .json(&body)
                .send() => response,
        };
        let mut response = match response {
            Ok(response) => response,
            Err(_) => {
                return failure_execution(transport_failure(accepted_ref.is_some()));
            }
        };
        if !response.status().is_success() {
            return failure_execution(status_failure(response.status(), accepted_ref.is_some()));
        }
        let streamed = match collect_stream(
            &mut response,
            &cancellation,
            accepted_ref.is_none(),
            &mut on_event,
            &mut events,
            &mut usage,
        )
        .await
        {
            Ok(streamed) => streamed,
            Err(failure) => return failure_execution(failure),
        };
        if accepted_ref.is_none() {
            let callback = match on_accepted.take() {
                Some(callback) => callback,
                None => return failure_execution(protocol_failure(true)),
            };
            if let Err(failure) = callback(&streamed.message_id) {
                return failure_execution(failure);
            }
            accepted_ref = Some(streamed.message_id.clone());
            let event = PendingProviderEvent::new(
                ProviderEventKind::TurnStarted,
                "Anthropic accepted the Provider Turn",
                Some(&streamed.message_id),
            );
            if let Err(failure) = on_event(&event, &usage) {
                return failure_execution(failure);
            }
            events.push(event);
        }
        let usage_event = PendingProviderEvent::new(
            ProviderEventKind::UsageObserved,
            "Anthropic usage observed",
            None,
        );
        if let Err(failure) = on_event(&usage_event, &usage) {
            return failure_execution(failure);
        }
        events.push(usage_event);
        if streamed.stop_reason == "tool_use" {
            messages.push(json!({"role": "assistant", "content": streamed.content.clone()}));
            let mut tool_results = Vec::new();
            for block in &streamed.content {
                if block.get("type").and_then(Value::as_str) != Some("tool_use") {
                    continue;
                }
                let id = match block.get("id").and_then(Value::as_str) {
                    Some(id) => id,
                    None => return failure_execution(protocol_failure(true)),
                };
                let name = match block.get("name").and_then(Value::as_str) {
                    Some(name) => name,
                    None => return failure_execution(protocol_failure(true)),
                };
                let input = block.get("input").cloned().unwrap_or(Value::Null);
                let output = match on_tool_call(id, name, &input) {
                    Ok(Some(output)) => output,
                    Ok(None) => "Quantix denied this tool request.".to_owned(),
                    Err(failure) => return failure_execution(failure),
                };
                let denied = output == "Quantix denied this tool request.";
                tool_results.push(json!({
                    "type": "tool_result",
                    "tool_use_id": id,
                    "content": output,
                    "is_error": denied,
                }));
            }
            if tool_results.is_empty() {
                return failure_execution(protocol_failure(true));
            }
            messages.push(json!({"role": "user", "content": tool_results}));
            continue;
        }
        if streamed.stop_reason != "end_turn" {
            return failure_execution(output_failure());
        }
        let text = streamed
            .content
            .iter()
            .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|block| block.get("text").and_then(Value::as_str))
            .collect::<String>();
        let candidate = match validate_candidate(
            Some(&text),
            &output_schema,
            prepared
                .task
                .resource_budget
                .output_bytes
                .min(PROVIDER_OUTPUT_LIMIT as u32),
        ) {
            Ok(candidate) => candidate,
            Err(failure) => return failure_execution(failure),
        };
        events.push(PendingProviderEvent::new(
            ProviderEventKind::Terminal,
            "Anthropic returned a schema-valid proposed result",
            accepted_ref.as_deref(),
        ));
        return ProviderExecution {
            state: AgentRunState::Completed,
            provider_thread_ref: Some(thread_ref),
            provider_turn_ref: accepted_ref,
            events,
            usage,
            failure: None,
            candidate_payload_json: Some(candidate),
        };
    }
    failure_execution(ProviderFailure::new(
        ProviderFailureCategory::RateLimited,
        true,
        "Increase the approved Provider Turn budget or create a linked retry.",
        Some("Anthropic exhausted the exact tool-turn budget."),
    ))
}

fn anthropic_tools(prepared: &PreparedAgentRun) -> Result<Vec<Value>, ProviderFailure> {
    dynamic_tool_specs(&prepared.permission_grant)?
        .into_iter()
        .map(|tool| {
            Ok(json!({
                "name": tool.get("name").cloned().ok_or_else(|| protocol_failure(false))?,
                "description": tool.get("description").cloned().ok_or_else(|| protocol_failure(false))?,
                "input_schema": tool.get("inputSchema").cloned().ok_or_else(|| protocol_failure(false))?,
                "strict": true,
            }))
        })
        .collect()
}

struct StreamedMessage {
    message_id: String,
    content: Vec<Value>,
    stop_reason: String,
}

async fn collect_stream(
    response: &mut reqwest::Response,
    cancellation: &CancellationToken,
    require_message_id: bool,
    on_event: &mut Box<super::TurnEventCallback>,
    events: &mut Vec<PendingProviderEvent>,
    usage: &mut ProviderUsage,
) -> Result<StreamedMessage, ProviderFailure> {
    let mut buffer = Vec::<u8>::new();
    let mut blocks = BTreeMap::<u64, Value>::new();
    let mut message_id: Option<String> = None;
    let mut stop_reason: Option<String> = None;
    loop {
        let chunk = tokio::select! {
            _ = cancellation.cancelled() => return Err(interruption_failure(message_id.is_some() || !require_message_id)),
            chunk = response.chunk() => chunk.map_err(|_| transport_failure(message_id.is_some() || !require_message_id))?,
        };
        let Some(chunk) = chunk else { break };
        buffer.extend_from_slice(&chunk);
        if buffer.len() > PROVIDER_OUTPUT_LIMIT {
            return Err(output_failure());
        }
        while let Some((boundary, delimiter_length)) = sse_boundary(&buffer) {
            let frame = std::str::from_utf8(&buffer[..boundary])
                .map_err(|_| protocol_failure(true))?
                .to_owned();
            buffer.drain(..boundary + delimiter_length);
            let data = frame
                .lines()
                .filter_map(|line| line.strip_prefix("data: "))
                .collect::<String>();
            if data.is_empty() {
                continue;
            }
            let payload: Value = serde_json::from_str(&data).map_err(|_| protocol_failure(true))?;
            match payload.get("type").and_then(Value::as_str) {
                Some("message_start") => {
                    let message = payload
                        .get("message")
                        .ok_or_else(|| protocol_failure(true))?;
                    message_id = Some(bounded_text(message.get("id"), 300)?);
                    merge_usage(message.get("usage"), usage)?;
                }
                Some("content_block_start") => {
                    let index = payload
                        .get("index")
                        .and_then(Value::as_u64)
                        .ok_or_else(|| protocol_failure(true))?;
                    let block = payload
                        .get("content_block")
                        .cloned()
                        .ok_or_else(|| protocol_failure(true))?;
                    blocks.insert(index, block);
                }
                Some("content_block_delta") => {
                    let index = payload
                        .get("index")
                        .and_then(Value::as_u64)
                        .ok_or_else(|| protocol_failure(true))?;
                    let delta = payload.get("delta").ok_or_else(|| protocol_failure(true))?;
                    let block = blocks
                        .get_mut(&index)
                        .ok_or_else(|| protocol_failure(true))?;
                    match delta.get("type").and_then(Value::as_str) {
                        Some("text_delta") => append_string(block, "text", delta.get("text"))?,
                        Some("input_json_delta") => {
                            append_string(block, "partial_json", delta.get("partial_json"))?
                        }
                        Some("thinking_delta" | "signature_delta" | "citations_delta") => {}
                        _ => return Err(protocol_failure(true)),
                    }
                }
                Some("content_block_stop") => {
                    let index = payload
                        .get("index")
                        .and_then(Value::as_u64)
                        .ok_or_else(|| protocol_failure(true))?;
                    if let Some(block) = blocks.get_mut(&index) {
                        if block.get("type").and_then(Value::as_str) == Some("tool_use") {
                            let raw = block
                                .get("partial_json")
                                .and_then(Value::as_str)
                                .unwrap_or("{}");
                            block["input"] =
                                serde_json::from_str(raw).map_err(|_| protocol_failure(true))?;
                            block
                                .as_object_mut()
                                .map(|value| value.remove("partial_json"));
                        }
                    }
                }
                Some("message_delta") => {
                    stop_reason = payload
                        .pointer("/delta/stop_reason")
                        .and_then(Value::as_str)
                        .map(str::to_owned);
                    merge_usage(payload.get("usage"), usage)?;
                }
                Some("message_stop" | "ping") => {}
                Some("error") => {
                    return Err(protocol_failure(
                        message_id.is_some() || !require_message_id,
                    ));
                }
                _ => {
                    return Err(protocol_failure(
                        message_id.is_some() || !require_message_id,
                    ));
                }
            }
        }
    }
    let message_id = message_id.ok_or_else(|| protocol_failure(!require_message_id))?;
    let content = blocks.into_values().collect::<Vec<_>>();
    let stop_reason = stop_reason.ok_or_else(|| protocol_failure(true))?;
    let event = PendingProviderEvent::new(
        ProviderEventKind::Warning,
        "Anthropic streamed Provider activity",
        Some(&message_id),
    );
    on_event(&event, usage)?;
    events.push(event);
    Ok(StreamedMessage {
        message_id,
        content,
        stop_reason,
    })
}

fn sse_boundary(buffer: &[u8]) -> Option<(usize, usize)> {
    let lf = buffer.windows(2).position(|window| window == b"\n\n");
    let crlf = buffer.windows(4).position(|window| window == b"\r\n\r\n");
    match (lf, crlf) {
        (Some(left), Some(right)) if left <= right => Some((left, 2)),
        (Some(_), Some(right)) => Some((right, 4)),
        (Some(left), None) => Some((left, 2)),
        (None, Some(right)) => Some((right, 4)),
        (None, None) => None,
    }
}

fn append_string(
    block: &mut Value,
    field: &str,
    value: Option<&Value>,
) -> Result<(), ProviderFailure> {
    let delta = value
        .and_then(Value::as_str)
        .ok_or_else(|| protocol_failure(true))?;
    let object = block
        .as_object_mut()
        .ok_or_else(|| protocol_failure(true))?;
    let current = object
        .entry(field)
        .or_insert_with(|| Value::String(String::new()));
    let current = current
        .as_str()
        .ok_or_else(|| protocol_failure(true))?
        .to_owned();
    if current.len().saturating_add(delta.len()) > PROVIDER_OUTPUT_LIMIT {
        return Err(output_failure());
    }
    *object.get_mut(field).expect("field inserted above") = Value::String(current + delta);
    Ok(())
}

fn merge_usage(raw: Option<&Value>, usage: &mut ProviderUsage) -> Result<(), ProviderFailure> {
    let Some(raw) = raw.and_then(Value::as_object) else {
        return Ok(());
    };
    let input = raw.get("input_tokens").and_then(Value::as_u64);
    let output = raw.get("output_tokens").and_then(Value::as_u64);
    if let Some(value) = input {
        usage.input_tokens = Some(usage.input_tokens.unwrap_or(0).saturating_add(value));
    }
    if let Some(value) = output {
        usage.output_tokens = Some(usage.output_tokens.unwrap_or(0).saturating_add(value));
    }
    usage.total_tokens = Some(
        usage
            .input_tokens
            .unwrap_or(0)
            .saturating_add(usage.output_tokens.unwrap_or(0)),
    );
    Ok(())
}

fn maximum_output_tokens(prepared: &PreparedAgentRun) -> u64 {
    u64::from(prepared.task.resource_budget.output_bytes / 2).clamp(1_024, 128_000)
}

fn client(timeout: Duration) -> Result<Client, ProviderFailure> {
    Client::builder()
        .https_only(true)
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(90))
        .timeout(timeout)
        .user_agent("Quantix/0.1 AnthropicAdapter")
        .build()
        .map_err(|_| transport_failure(false))
}

fn status_failure(status: StatusCode, accepted: bool) -> ProviderFailure {
    match status.as_u16() {
        401 | 403 => ProviderFailure::new(
            ProviderFailureCategory::AuthenticationRequired,
            true,
            "Replace or revoke the Anthropic API key in Settings before retrying.",
            Some("Anthropic rejected the credential."),
        ),
        429 => ProviderFailure::new(
            ProviderFailureCategory::RateLimited,
            true,
            "Wait for Anthropic quota or capacity to recover before retrying.",
            Some("Anthropic reported a quota or rate limit."),
        ),
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
                "Refresh Anthropic models and choose a current capability."
            },
            Some("Anthropic rejected the selected model, effort, tool, or output capability."),
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
            "Refresh Anthropic capabilities before retrying."
        },
        Some("Anthropic returned an incompatible or malformed response."),
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
            "Check the connection and retry Anthropic."
        },
        Some("The Anthropic request did not complete."),
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
        Some("The Anthropic request was interrupted."),
    )
}

fn output_failure() -> ProviderFailure {
    ProviderFailure::new(
        ProviderFailureCategory::OutputInvalid,
        true,
        "Create a linked retry after reviewing the output-contract failure.",
        Some("Anthropic did not return a valid candidate output."),
    )
}

fn interrupted_execution(
    thread_ref: String,
    turn_ref: Option<String>,
    mut events: Vec<PendingProviderEvent>,
    usage: ProviderUsage,
) -> ProviderExecution {
    let accepted = turn_ref.is_some();
    let failure = interruption_failure(accepted);
    events.push(PendingProviderEvent::new(
        ProviderEventKind::Terminal,
        "Anthropic Provider Turn was interrupted",
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
        failure: Some(failure),
        candidate_payload_json: None,
    }
}
