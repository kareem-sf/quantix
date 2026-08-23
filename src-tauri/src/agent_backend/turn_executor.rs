use serde_json::Value;

use crate::agent_runtime::{
    request_budget_failure, ProviderFailure, ProviderFailureCategory, ProviderUsage,
};
use crate::chatgpt_oauth::StoredConnection;

use super::client::{
    BackendError, BackendRequest, ChatGptBackend, StreamEvent, TurnDisposition, UsageSnapshot,
};

pub(crate) type BackendEvent = StreamEvent;
type ToolDeniedCallback<'a> =
    dyn FnMut(&str, &str, &str, &str) -> Result<(), ProviderFailure> + Send + 'a;
type RefreshAuthCallback<'a> =
    dyn Fn(&str) -> Result<StoredConnection, ProviderFailure> + Sync + 'a;

pub(crate) struct TurnContext<'a> {
    pub backend: &'a dyn ChatGptBackend,
    pub auth: &'a StoredConnection,
    pub refresh_auth: &'a RefreshAuthCallback<'a>,
    pub request: BackendRequest,
    pub session_id: String,
    pub authorize_tool: &'a (dyn Fn(&str, &str) -> Result<Value, ToolRejection> + Sync),
    pub is_cancelled: &'a (dyn Fn() -> bool + Sync),
    pub on_event: &'a mut (dyn FnMut(BackendEvent) -> Result<(), ProviderFailure> + Send),
    pub on_tool_denied: &'a mut ToolDeniedCallback<'a>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ToolRejection {
    NotPermitted(&'static str),
    Failed(&'static str),
}

#[derive(Debug)]
pub(crate) struct ProviderTurnResult {
    pub usage: ProviderUsage,
}
const QUARANTINE_ACTION: &str = "Resolve the quarantined Agent Run before retrying.";

/// Generous bound on consecutive tool rounds inside one Provider Turn. A hostile
/// or broken backend can otherwise sustain endless follow-ups (denied calls
/// consume no reservations), so exceeding this budget quarantines the run.
const MAX_TOOL_ROUNDS: u32 = 32;

/// Drives one Provider Turn against the ChatGPT backend until it completes
/// without outstanding tool calls, fails, or is interrupted.
///
/// # Panics
///
/// Panics when polled inside a current-thread tokio runtime: OAuth token refresh
/// is a blocking call parked via [`tokio::task::block_in_place`], which requires
/// a multi-thread runtime (the Tauri default). Debug builds assert the runtime
/// flavor.
pub(crate) async fn execute_provider_turn(
    ctx: TurnContext<'_>,
) -> Result<ProviderTurnResult, ProviderFailure> {
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        debug_assert_ne!(
            handle.runtime_flavor(),
            tokio::runtime::RuntimeFlavor::CurrentThread,
            "execute_provider_turn requires a multi-thread tokio runtime"
        );
    }
    let mut request = ctx.request;
    request.session_id = ctx.session_id.clone();
    let mut items = request.input_items.clone();
    let mut auth = cloned_connection(ctx.auth);
    let mut usage_snapshots = Vec::new();
    let mut tool_rounds = 0u32;
    let mut turn_advanced = false;
    let mut refreshed = false;
    let mut cancelled = false;
    loop {
        if cancelled || (ctx.is_cancelled)() {
            return Err(interruption_failure(turn_advanced));
        }
        let mut response = ResponseCollector::default();
        let mut delivery_failure = None;
        let mut accepted = false;
        let mut outcome = create_response_once(
            ctx.backend,
            &auth,
            &request,
            &mut response,
            &mut usage_snapshots,
            &mut cancelled,
            &mut accepted,
            &mut delivery_failure,
            ctx.is_cancelled,
            &mut *ctx.on_event,
        )
        .await;
        turn_advanced |= accepted;
        if let Some(failure) = delivery_failure.take() {
            return Err(callback_failure(failure, turn_advanced));
        }
        if matches!(outcome, Err(BackendError::AuthenticationRequired)) && !refreshed {
            if cancelled || (ctx.is_cancelled)() {
                return Err(interruption_failure(turn_advanced));
            }
            refreshed = true;
            let refreshed_auth =
                tokio::task::block_in_place(|| (ctx.refresh_auth)(ctx.auth.account_id.as_str()))?;
            if refreshed_auth.account_id != ctx.auth.account_id {
                return Err(authentication_failure());
            }
            if cancelled || (ctx.is_cancelled)() {
                return Err(interruption_failure(turn_advanced));
            }
            auth = refreshed_auth;
            outcome = create_response_once(
                ctx.backend,
                &auth,
                &request,
                &mut response,
                &mut usage_snapshots,
                &mut cancelled,
                &mut accepted,
                &mut delivery_failure,
                ctx.is_cancelled,
                &mut *ctx.on_event,
            )
            .await;
            turn_advanced |= accepted;
            if let Some(failure) = delivery_failure.take() {
                return Err(callback_failure(failure, turn_advanced));
            }
        }
        match outcome {
            Err(error) => return Err(backend_failure(error, turn_advanced)),
            Ok(TurnDisposition::Failed) => return Err(protocol_failure(turn_advanced)),
            Ok(TurnDisposition::Completed) => {}
        }
        turn_advanced = true;
        if cancelled || (ctx.is_cancelled)() {
            return Err(interruption_failure(true));
        }
        if response.completed_id.is_none() {
            return Err(protocol_failure(turn_advanced));
        }
        if response.function_calls.is_empty() {
            return Ok(ProviderTurnResult {
                usage: accumulated_usage(&usage_snapshots),
            });
        }
        tool_rounds += 1;
        if tool_rounds > MAX_TOOL_ROUNDS {
            return Err(tool_round_budget_exhausted());
        }
        let mut outputs = Vec::with_capacity(response.function_calls.len());
        for call in &response.function_calls {
            let output = match (ctx.authorize_tool)(&call.name, &call.arguments) {
                Ok(value) => value,
                Err(ToolRejection::NotPermitted(reason)) => {
                    if let Err(failure) =
                        (ctx.on_tool_denied)(&call.call_id, &call.name, &call.arguments, reason)
                    {
                        return Err(callback_failure(failure, true));
                    }
                    serde_json::json!({ "error": "not_permitted", "reason": reason })
                }
                Err(ToolRejection::Failed(reason)) => {
                    serde_json::json!({ "error": "tool_failed", "reason": reason })
                }
            };
            outputs.push(serde_json::json!({
                "type": "function_call_output",
                "call_id": call.call_id,
                "output": output.to_string(),
            }));
        }
        items.extend(response.output_items.iter().cloned());
        items.extend(outputs);
        request.input_items = items.clone();
    }
}

struct RecordedFunctionCall {
    call_id: String,
    name: String,
    arguments: String,
}

#[derive(Default)]
struct ResponseCollector {
    output_items: Vec<Value>,
    function_calls: Vec<RecordedFunctionCall>,
    completed_id: Option<String>,
}

fn strip_item_id(mut item: Value) -> Value {
    if let Some(object) = item.as_object_mut() {
        object.remove("id");
    }
    item
}

#[allow(clippy::too_many_arguments)]
async fn create_response_once(
    backend: &dyn ChatGptBackend,
    auth: &StoredConnection,
    request: &BackendRequest,
    response: &mut ResponseCollector,
    usage_snapshots: &mut Vec<UsageSnapshot>,
    cancelled: &mut bool,
    accepted: &mut bool,
    delivery_failure: &mut Option<ProviderFailure>,
    is_cancelled: &(dyn Fn() -> bool + Sync),
    on_event: &mut (dyn FnMut(BackendEvent) -> Result<(), ProviderFailure> + Send),
) -> Result<TurnDisposition, BackendError> {
    let mut emit = |event: BackendEvent| -> Result<(), BackendError> {
        if is_cancelled() {
            *cancelled = true;
        }
        match &event {
            StreamEvent::Created { .. } => *accepted = true,
            StreamEvent::ItemDone(frame) => {
                if let Some(item) = frame.get("item") {
                    response.output_items.push(strip_item_id(item.clone()));
                }
            }
            StreamEvent::FunctionCallDone {
                call_id,
                name,
                arguments,
            } => response.function_calls.push(RecordedFunctionCall {
                call_id: call_id.clone(),
                name: name.clone(),
                arguments: arguments.clone(),
            }),
            StreamEvent::Completed {
                response_id,
                usage: snapshot,
            } => {
                response.completed_id = Some(response_id.clone());
                usage_snapshots.push(snapshot.clone());
            }
            StreamEvent::ItemAdded(_)
            | StreamEvent::TextDelta(_)
            | StreamEvent::FunctionCallDelta { .. }
            | StreamEvent::Errored(_) => {}
        }
        on_event(event).map_err(|failure| {
            *delivery_failure = Some(failure);
            BackendError::EventDelivery
        })
    };
    backend
        .create_response(auth, request, is_cancelled, &mut emit)
        .await
}

fn cloned_connection(conn: &StoredConnection) -> StoredConnection {
    StoredConnection {
        access_token: conn.access_token.clone(),
        refresh_token: conn.refresh_token.clone(),
        id_token: conn.id_token.clone(),
        expires_at_ms: conn.expires_at_ms,
        account_id: conn.account_id.clone(),
        plan_type: conn.plan_type.clone(),
        compute_residency: conn.compute_residency.clone(),
    }
}

fn merged_tokens(left: Option<u64>, right: Option<u64>) -> Option<u64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left + right),
        (left, None) => left,
        (None, right) => right,
    }
}

fn accumulated_usage(snapshots: &[UsageSnapshot]) -> ProviderUsage {
    let mut usage = ProviderUsage::default();
    for snapshot in snapshots {
        usage.input_tokens = Some(usage.input_tokens.unwrap_or(0) + snapshot.input_tokens);
        usage.output_tokens = Some(usage.output_tokens.unwrap_or(0) + snapshot.output_tokens);
        usage.total_tokens = Some(usage.total_tokens.unwrap_or(0) + snapshot.total_tokens);
        usage.cached_input_tokens =
            merged_tokens(usage.cached_input_tokens, snapshot.cached_input_tokens);
        usage.reasoning_output_tokens = merged_tokens(
            usage.reasoning_output_tokens,
            snapshot.reasoning_output_tokens,
        );
    }
    usage
}

fn authentication_failure() -> ProviderFailure {
    ProviderFailure::new(
        ProviderFailureCategory::AuthenticationRequired,
        true,
        "Reconnect the ChatGPT subscription in Settings before retrying.",
        Some("The ChatGPT connection needs reauthentication."),
    )
}

fn rate_limit_failure(retry_after_ms: Option<u64>) -> ProviderFailure {
    let detail = match retry_after_ms {
        Some(ms) => format!("ChatGPT rate limited the request (retry after {ms} ms)."),
        None => "ChatGPT rate limited the request.".to_owned(),
    };
    ProviderFailure::new(
        ProviderFailureCategory::RateLimited,
        true,
        "Wait for ChatGPT capacity to recover before retrying.",
        Some(&detail),
    )
    .with_retry_after_milliseconds(retry_after_ms)
}

fn protocol_failure(turn_advanced: bool) -> ProviderFailure {
    ProviderFailure::new(
        if turn_advanced {
            ProviderFailureCategory::OutcomeUnknown
        } else {
            ProviderFailureCategory::ProtocolInvalid
        },
        !turn_advanced,
        if turn_advanced {
            QUARANTINE_ACTION
        } else {
            "Verify the ChatGPT connection and model selection before retrying."
        },
        Some("The ChatGPT backend returned an incompatible or malformed response."),
    )
}

fn transport_failure(turn_advanced: bool) -> ProviderFailure {
    ProviderFailure::new(
        if turn_advanced {
            ProviderFailureCategory::OutcomeUnknown
        } else {
            ProviderFailureCategory::ProcessFailed
        },
        !turn_advanced,
        if turn_advanced {
            QUARANTINE_ACTION
        } else {
            "Check the connection and retry ChatGPT."
        },
        Some("The ChatGPT request did not complete."),
    )
}

fn interruption_failure(turn_advanced: bool) -> ProviderFailure {
    ProviderFailure::new(
        if turn_advanced {
            ProviderFailureCategory::OutcomeUnknown
        } else {
            ProviderFailureCategory::Interrupted
        },
        !turn_advanced,
        if turn_advanced {
            QUARANTINE_ACTION
        } else {
            "Create a linked retry when ready."
        },
        Some("The ChatGPT request was interrupted."),
    )
}

fn tool_round_budget_exhausted() -> ProviderFailure {
    ProviderFailure::new(
        ProviderFailureCategory::OutcomeUnknown,
        false,
        QUARANTINE_ACTION,
        Some("The ChatGPT Provider Turn exceeded its consecutive tool-round budget."),
    )
}

fn callback_failure(failure: ProviderFailure, turn_advanced: bool) -> ProviderFailure {
    if turn_advanced {
        ProviderFailure::new(
            ProviderFailureCategory::OutcomeUnknown,
            false,
            QUARANTINE_ACTION,
            Some(
                failure
                    .redacted_detail
                    .as_deref()
                    .unwrap_or("Authoritative Provider Turn persistence failed."),
            ),
        )
    } else {
        failure
    }
}

fn backend_failure(error: BackendError, turn_advanced: bool) -> ProviderFailure {
    match error {
        BackendError::AuthenticationRequired => authentication_failure(),
        BackendError::RateLimited { retry_after_ms } => rate_limit_failure(retry_after_ms),
        BackendError::RequestBudgetExceeded { request_bytes } => {
            request_budget_failure(request_bytes)
        }
        BackendError::Protocol(_) => protocol_failure(turn_advanced),
        BackendError::Transport(_) => transport_failure(turn_advanced),
        BackendError::Interrupted => interruption_failure(turn_advanced),
        BackendError::EventDelivery => protocol_failure(turn_advanced),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::future::Future;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    use serde_json::json;

    use super::super::client::{build_request_body, parse_stream_bytes};
    use super::*;
    use crate::chatgpt_oauth::{
        force_refresh_connection_unlocked, load, needs_refresh, save, with_connection_mutation,
        LoadState, TokenClient,
    };

    fn sse(frames: &[&str]) -> Vec<u8> {
        let mut bytes = frames.join("\n\n").into_bytes();
        bytes.extend_from_slice(b"\n\n");
        bytes
    }

    const TEXT_COMPLETION: &[&str] = &[
        r#"data: {"type":"response.created","response":{"id":"resp_text_1"}}"#,
        r#"data: {"type":"response.output_item.added","output_index":0,"item":{"id":"msg_1","type":"message","role":"assistant","content":[]}}"#,
        r#"data: {"type":"response.output_text.delta","item_id":"msg_1","delta":"Hello"}"#,
        r#"data: {"type":"response.output_text.delta","item_id":"msg_1","delta":" from the fixture."}"#,
        r#"data: {"type":"response.output_item.done","output_index":0,"item":{"id":"msg_1","type":"message","role":"assistant","content":[{"type":"output_text","text":"Hello from the fixture."}]}}"#,
        r#"data: {"type":"response.completed","response":{"id":"resp_text_1","usage":{"input_tokens":120,"input_tokens_details":{"cached_tokens":32},"output_tokens":45,"output_tokens_details":{"reasoning_tokens":16},"total_tokens":165}}}"#,
        "data: [DONE]",
    ];

    const TOOL_RESPONSE_ONE: &[&str] = &[
        r#"data: {"type":"response.created","response":{"id":"resp_tool_1"}}"#,
        r#"data: {"type":"response.output_item.added","output_index":0,"item":{"id":"rs_1","type":"reasoning","summary":[]}}"#,
        r#"data: {"type":"response.output_item.added","output_index":1,"item":{"id":"fc_1","type":"function_call","name":"read_file","call_id":"call_abc"}}"#,
        r#"data: {"type":"response.function_call_arguments.delta","item_id":"fc_1","delta":"{\"path\":"}"#,
        r#"data: {"type":"response.function_call_arguments.delta","item_id":"fc_1","delta":"\"spec.md\"}"}"#,
        r#"data: {"type":"response.function_call_arguments.done","item_id":"fc_1","arguments":"{\"path\":\"spec.md\"}"}"#,
        r#"data: {"type":"response.output_item.done","output_index":0,"item":{"id":"rs_1","type":"reasoning","summary":[],"encrypted_content":"enc-state-1"}}"#,
        r#"data: {"type":"response.output_item.done","output_index":1,"item":{"id":"fc_1","type":"function_call","name":"read_file","call_id":"call_abc","arguments":"{\"path\":\"spec.md\"}"}}"#,
        r#"data: {"type":"response.completed","response":{"id":"resp_tool_1","usage":{"input_tokens":200,"input_tokens_details":{"cached_tokens":0},"output_tokens":30,"output_tokens_details":{"reasoning_tokens":8},"total_tokens":230}}}"#,
        "data: [DONE]",
    ];

    const TOOL_RESPONSE_TWO: &[&str] = &[
        r#"data: {"type":"response.created","response":{"id":"resp_tool_2"}}"#,
        r#"data: {"type":"response.output_item.added","output_index":0,"item":{"id":"msg_2","type":"message","role":"assistant","content":[]}}"#,
        r#"data: {"type":"response.output_text.delta","item_id":"msg_2","delta":"All done."}"#,
        r#"data: {"type":"response.output_item.done","output_index":0,"item":{"id":"msg_2","type":"message","role":"assistant","content":[{"type":"output_text","text":"All done."}]}}"#,
        r#"data: {"type":"response.completed","response":{"id":"resp_tool_2","usage":{"input_tokens":50,"input_tokens_details":{"cached_tokens":5},"output_tokens":10,"output_tokens_details":{"reasoning_tokens":2},"total_tokens":60}}}"#,
        "data: [DONE]",
    ];

    const PARTIAL_STREAM: &[&str] = &[
        r#"data: {"type":"response.created","response":{"id":"resp_abort_1"}}"#,
        r#"data: {"type":"response.output_text.delta","item_id":"m","delta":"partial"}"#,
    ];

    struct ScriptedBackend {
        responses: Mutex<VecDeque<Result<Vec<u8>, BackendError>>>,
        requests: Mutex<Vec<serde_json::Value>>,
        access_tokens: Mutex<Vec<String>>,
    }

    impl ScriptedBackend {
        fn new(responses: Vec<Result<Vec<u8>, BackendError>>) -> Self {
            Self {
                responses: Mutex::new(responses.into()),
                requests: Mutex::new(Vec::new()),
                access_tokens: Mutex::new(Vec::new()),
            }
        }

        fn request_bodies(&self) -> Vec<serde_json::Value> {
            self.requests.lock().unwrap().clone()
        }
    }

    impl ChatGptBackend for ScriptedBackend {
        fn create_response<'a>(
            &'a self,
            auth: &'a StoredConnection,
            req: &'a BackendRequest,
            is_cancelled: &'a (dyn Fn() -> bool + Sync),
            on_event: &'a mut (dyn FnMut(StreamEvent) -> Result<(), BackendError> + Send),
        ) -> std::pin::Pin<
            Box<dyn Future<Output = Result<TurnDisposition, BackendError>> + Send + 'a>,
        > {
            Box::pin(async move {
                if is_cancelled() {
                    return Err(BackendError::Interrupted);
                }
                self.requests.lock().unwrap().push(build_request_body(req));
                self.access_tokens
                    .lock()
                    .unwrap()
                    .push(auth.access_token.clone());
                match self.responses.lock().unwrap().pop_front() {
                    Some(Ok(bytes)) => parse_stream_bytes(&bytes, on_event),
                    Some(Err(error)) => Err(error),
                    None => Err(BackendError::Protocol(
                        "script exhausted its scripted responses".to_owned(),
                    )),
                }
            })
        }
    }

    fn stored_connection(access_token: &str) -> StoredConnection {
        StoredConnection {
            access_token: access_token.to_owned(),
            refresh_token: "refresh-old".to_owned(),
            id_token: "id-old".to_owned(),
            expires_at_ms: 5_000,
            account_id: "acc-77".to_owned(),
            plan_type: Some("plus".to_owned()),
            compute_residency: Some("us".to_owned()),
        }
    }

    fn sample_request() -> BackendRequest {
        BackendRequest {
            model: "gpt-5.5".to_owned(),
            instructions: "system prompt".to_owned(),
            input_items: vec![serde_json::json!({
                "type": "message",
                "role": "user",
                "content": [{"type": "input_text", "text": "summarize the tender"}]
            })],
            tools: vec![serde_json::json!({
                "type": "function",
                "name": "read_file",
                "description": "reads a file",
                "parameters": {"type": "object", "properties": {}},
                "strict": true
            })],
            output_schema: serde_json::json!({
                "additionalProperties": false,
                "properties": {"answer": {"type": "string"}},
                "required": ["answer"],
                "type": "object"
            }),
            store: false,
            include_reasoning: true,
            reasoning_effort: None,
            session_id: "ses-stale".to_owned(),
        }
    }

    fn temp_home(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "quantix-turn-executor-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    const ISSUER_BODY: &str = r#"{"access_token":"at-new","refresh_token":"refresh-new","id_token":"eyJhbGciOiJub25lIn0.eyJjaGF0Z3B0X2FjY291bnRfaWQiOiJhY2MtNzciLCJodHRwczovL2FwaS5vcGVuYWkuY29tL2F1dGgiOnsiY2hhdGdwdF9wbGFuX3R5cGUiOiJwbHVzIiwiY2hhdGdwdF9jb21wdXRlX3Jlc2lkZW5jeSI6InVzIn19.c2ln","expires_in":3600}"#;

    fn mock_issuer(body: &'static str) -> (TokenClient, Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let bodies: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let shared = Arc::clone(&bodies);
        std::thread::spawn(move || {
            for mut stream in listener.incoming().flatten() {
                if let Some(form) = read_form(&mut stream) {
                    shared.lock().unwrap().push(form);
                }
                write_json(&mut stream, body);
            }
        });
        (
            TokenClient::new(&format!("http://127.0.0.1:{port}")).unwrap(),
            bodies,
        )
    }

    fn read_form(stream: &mut TcpStream) -> Option<String> {
        let mut raw = Vec::new();
        let mut chunk = [0u8; 512];
        loop {
            if raw.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
            let read = stream.read(&mut chunk).ok()?;
            if read == 0 {
                break;
            }
            raw.extend_from_slice(&chunk[..read]);
        }
        let split = raw.windows(4).position(|window| window == b"\r\n\r\n")?;
        let head = String::from_utf8_lossy(&raw[..split]).to_ascii_lowercase();
        let length: usize = head
            .lines()
            .find_map(|line| line.strip_prefix("content-length:")?.trim().parse().ok())?;
        let mut body = raw[split + 4..].to_vec();
        while body.len() < length {
            let read = stream.read(&mut chunk).ok()?;
            if read == 0 {
                break;
            }
            body.extend_from_slice(&chunk[..read]);
        }
        Some(String::from_utf8_lossy(&body).to_string())
    }

    fn write_json(stream: &mut TcpStream, body: &str) {
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.flush();
    }

    fn block_on_future<F: Future>(future: F) -> F::Output {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("test tokio runtime")
            .block_on(future)
    }

    struct AlwaysFunctionCallBackend {
        requests: Mutex<Vec<serde_json::Value>>,
    }

    impl ChatGptBackend for AlwaysFunctionCallBackend {
        fn create_response<'a>(
            &'a self,
            _auth: &StoredConnection,
            req: &'a BackendRequest,
            is_cancelled: &'a (dyn Fn() -> bool + Sync),
            on_event: &'a mut (dyn FnMut(StreamEvent) -> Result<(), BackendError> + Send),
        ) -> std::pin::Pin<
            Box<dyn Future<Output = Result<TurnDisposition, BackendError>> + Send + 'a>,
        > {
            Box::pin(async move {
                if is_cancelled() {
                    return Err(BackendError::Interrupted);
                }
                self.requests.lock().unwrap().push(build_request_body(req));
                parse_stream_bytes(&sse(TOOL_RESPONSE_ONE), on_event)
            })
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn run_turn<'a>(
        backend: &'a dyn ChatGptBackend,
        auth: &'a StoredConnection,
        token_client: &'a TokenClient,
        home: &'a Path,
        authorize_tool: &'a (dyn Fn(&str, &str) -> Result<Value, ToolRejection> + Sync),
        is_cancelled: &'a (dyn Fn() -> bool + Sync),
        on_event: &'a mut (dyn FnMut(BackendEvent) + Send),
    ) -> Result<ProviderTurnResult, ProviderFailure> {
        let refresh_auth = |expected_account_id: &str| {
            with_connection_mutation(|| {
                force_refresh_connection_unlocked(
                    home,
                    token_client,
                    test_now_ms(),
                    expected_account_id,
                )
            })
            .map_err(|()| authentication_failure())
        };
        run_turn_with_refresh(
            backend,
            auth,
            &refresh_auth,
            authorize_tool,
            is_cancelled,
            on_event,
        )
    }

    fn run_turn_with_refresh<'a>(
        backend: &'a dyn ChatGptBackend,
        auth: &'a StoredConnection,
        refresh_auth: &'a RefreshAuthCallback<'a>,
        authorize_tool: &'a (dyn Fn(&str, &str) -> Result<Value, ToolRejection> + Sync),
        is_cancelled: &'a (dyn Fn() -> bool + Sync),
        on_event: &'a mut (dyn FnMut(BackendEvent) + Send),
    ) -> Result<ProviderTurnResult, ProviderFailure> {
        let mut fallible_event = |event| {
            on_event(event);
            Ok(())
        };
        let mut denied = |_: &str, _: &str, _: &str, _: &str| Ok(());
        block_on_future(execute_provider_turn(TurnContext {
            backend,
            auth,
            refresh_auth,
            request: sample_request(),
            session_id: "ses-turn-1".to_owned(),
            authorize_tool,
            is_cancelled,
            on_event: &mut fallible_event,
            on_tool_denied: &mut denied,
        }))
    }

    #[allow(clippy::too_many_arguments)]
    fn run_turn_with_callbacks<'a>(
        backend: &'a dyn ChatGptBackend,
        auth: &'a StoredConnection,
        token_client: &'a TokenClient,
        home: &'a Path,
        authorize_tool: &'a (dyn Fn(&str, &str) -> Result<Value, ToolRejection> + Sync),
        is_cancelled: &'a (dyn Fn() -> bool + Sync),
        on_event: &'a mut (dyn FnMut(BackendEvent) -> Result<(), ProviderFailure> + Send),
        on_tool_denied: &'a mut ToolDeniedCallback<'a>,
    ) -> Result<ProviderTurnResult, ProviderFailure> {
        let refresh_auth = |expected_account_id: &str| {
            with_connection_mutation(|| {
                force_refresh_connection_unlocked(
                    home,
                    token_client,
                    test_now_ms(),
                    expected_account_id,
                )
            })
            .map_err(|()| authentication_failure())
        };
        block_on_future(execute_provider_turn(TurnContext {
            backend,
            auth,
            refresh_auth: &refresh_auth,
            request: sample_request(),
            session_id: "ses-turn-1".to_owned(),
            authorize_tool,
            is_cancelled,
            on_event,
            on_tool_denied,
        }))
    }

    fn test_now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_millis() as u64)
    }

    #[test]
    fn straight_text_answer_completes_with_accumulated_usage() {
        let backend = ScriptedBackend::new(vec![Ok(sse(TEXT_COMPLETION))]);
        let auth = stored_connection("at-1");
        let (token_client, issuer_bodies) = mock_issuer(ISSUER_BODY);
        assert!(issuer_bodies.lock().unwrap().is_empty());
        let home = temp_home("straight-text");
        let forwarded = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&forwarded);
        let mut sink = move |_: BackendEvent| {
            counter.fetch_add(1, Ordering::SeqCst);
        };
        let no_tools_expected =
            |_: &str, _: &str| -> Result<Value, ToolRejection> { panic!("no tool call expected") };

        let result = run_turn(
            &backend,
            &auth,
            &token_client,
            &home,
            &no_tools_expected,
            &|| false,
            &mut sink,
        )
        .expect("text-only turn must complete");

        assert_eq!(result.usage.input_tokens, Some(120));
        assert_eq!(result.usage.cached_input_tokens, Some(32));
        assert_eq!(result.usage.output_tokens, Some(45));
        assert_eq!(result.usage.reasoning_output_tokens, Some(16));
        assert_eq!(result.usage.total_tokens, Some(165));
        let requests = backend.request_bodies();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0]["input"][0]["role"], "user");
        assert!(requests[0].get("previous_response_id").is_none());
        assert!(
            forwarded.load(Ordering::SeqCst) >= 5,
            "events must reach the caller sink"
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn approved_tool_roundtrip_replays_outputs_to_the_model() {
        let backend =
            ScriptedBackend::new(vec![Ok(sse(TOOL_RESPONSE_ONE)), Ok(sse(TOOL_RESPONSE_TWO))]);
        let auth = stored_connection("at-1");
        let (token_client, _bodies) = mock_issuer(ISSUER_BODY);
        let home = temp_home("tool-roundtrip");
        let authorized: Arc<Mutex<Vec<(String, String)>>> = Arc::new(Mutex::new(Vec::new()));
        let tracker = Arc::clone(&authorized);
        let executed = Arc::new(AtomicUsize::new(0));
        let closure_executions = Arc::clone(&executed);
        let approve_read_file =
            move |name: &str, arguments: &str| -> Result<Value, ToolRejection> {
                tracker
                    .lock()
                    .unwrap()
                    .push((name.to_owned(), arguments.to_owned()));
                closure_executions.fetch_add(1, Ordering::SeqCst);
                Ok(json!({"content": "file body"}))
            };
        let mut sink = |_: BackendEvent| {};

        let result = run_turn(
            &backend,
            &auth,
            &token_client,
            &home,
            &approve_read_file,
            &|| false,
            &mut sink,
        )
        .expect("tool roundtrip must complete");

        assert_eq!(result.usage.input_tokens, Some(250));
        assert_eq!(result.usage.output_tokens, Some(40));
        assert_eq!(result.usage.total_tokens, Some(290));
        assert_eq!(result.usage.cached_input_tokens, Some(5));
        assert_eq!(result.usage.reasoning_output_tokens, Some(10));

        let requests = backend.request_bodies();
        assert_eq!(requests.len(), 2, "one follow-up request expected");
        assert!(requests[0].get("previous_response_id").is_none());
        assert!(
            requests[1].get("previous_response_id").is_none(),
            "REST tool follow-ups must replay the full stateless input"
        );

        let input = requests[1]["input"].as_array().expect("input array");
        assert_eq!(input[0]["role"], "user", "prior context must be replayed");
        assert!(
            input.iter().all(|item| item.get("id").is_none()),
            "replayed items must be stripped of ids: {input:?}"
        );
        let reasoning = input
            .iter()
            .find(|item| item["type"] == "reasoning")
            .expect("reasoning item replayed");
        assert_eq!(reasoning["encrypted_content"], "enc-state-1");
        let function_call = input
            .iter()
            .find(|item| item["type"] == "function_call")
            .expect("function_call item replayed");
        assert_eq!(function_call["call_id"], "call_abc");
        assert_eq!(function_call["name"], "read_file");
        let output = input
            .iter()
            .find(|item| item["type"] == "function_call_output")
            .expect("function_call_output appended");
        assert_eq!(output["call_id"], "call_abc");
        assert_eq!(output["output"], r#"{"content":"file body"}"#);

        assert_eq!(
            *authorized.lock().unwrap(),
            vec![("read_file".to_owned(), r#"{"path":"spec.md"}"#.to_owned())]
        );
        assert_eq!(executed.load(Ordering::SeqCst), 1);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn out_of_grant_call_returns_denial_and_counts_it_without_executing() {
        let backend =
            ScriptedBackend::new(vec![Ok(sse(TOOL_RESPONSE_ONE)), Ok(sse(TOOL_RESPONSE_TWO))]);
        let auth = stored_connection("at-1");
        let (token_client, _bodies) = mock_issuer(ISSUER_BODY);
        let home = temp_home("denied-tool");
        let executed = Arc::new(AtomicUsize::new(0));
        let attempts = Arc::new(AtomicUsize::new(0));
        let denial_attempts = Arc::clone(&attempts);
        let deny_everything =
            move |_name: &str, _arguments: &str| -> Result<Value, ToolRejection> {
                denial_attempts.fetch_add(1, Ordering::SeqCst);
                Err(ToolRejection::NotPermitted("tool_not_granted"))
            };
        let mut sink = |_: BackendEvent| {};

        run_turn(
            &backend,
            &auth,
            &token_client,
            &home,
            &deny_everything,
            &|| false,
            &mut sink,
        )
        .expect("denied tool must still complete via denial output");

        assert_eq!(attempts.load(Ordering::SeqCst), 1);
        assert_eq!(
            executed.load(Ordering::SeqCst),
            0,
            "a denied tool must not be executed"
        );

        let requests = backend.request_bodies();
        let input = requests[1]["input"].as_array().unwrap();
        let output = input
            .iter()
            .find(|item| item["type"] == "function_call_output")
            .expect("denial output returned to the model");
        assert_eq!(
            output["output"],
            json!({"error": "not_permitted", "reason": "tool_not_granted"}).to_string()
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn failed_tool_execution_is_reported_but_not_counted_as_denial() {
        let backend =
            ScriptedBackend::new(vec![Ok(sse(TOOL_RESPONSE_ONE)), Ok(sse(TOOL_RESPONSE_TWO))]);
        let auth = stored_connection("at-1");
        let (token_client, _bodies) = mock_issuer(ISSUER_BODY);
        let home = temp_home("failed-tool");
        let fail_everything = |_name: &str, _arguments: &str| -> Result<Value, ToolRejection> {
            Err(ToolRejection::Failed("quota_exhausted"))
        };
        let mut sink = |_: BackendEvent| {};

        run_turn(
            &backend,
            &auth,
            &token_client,
            &home,
            &fail_everything,
            &|| false,
            &mut sink,
        )
        .expect("failed tool must still complete via failure output");

        let requests = backend.request_bodies();
        let input = requests[1]["input"].as_array().unwrap();
        let output = input
            .iter()
            .find(|item| item["type"] == "function_call_output")
            .unwrap();
        assert_eq!(
            output["output"],
            json!({"error": "tool_failed", "reason": "quota_exhausted"}).to_string()
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn consecutive_tool_rounds_terminate_at_the_cap_with_quarantine() {
        let backend = AlwaysFunctionCallBackend {
            requests: Mutex::new(Vec::new()),
        };
        let auth = stored_connection("at-1");
        let (token_client, _bodies) = mock_issuer(ISSUER_BODY);
        let home = temp_home("tool-cap");
        let attempts = Arc::new(AtomicUsize::new(0));
        let closure_attempts = Arc::clone(&attempts);
        let approve_all = move |_name: &str, _arguments: &str| -> Result<Value, ToolRejection> {
            closure_attempts.fetch_add(1, Ordering::SeqCst);
            Ok(json!({"ok": true}))
        };
        let mut sink = |_: BackendEvent| {};

        let failure = run_turn(
            &backend,
            &auth,
            &token_client,
            &home,
            &approve_all,
            &|| false,
            &mut sink,
        )
        .expect_err("an endless tool loop must hit the cap");

        assert_eq!(failure.category, ProviderFailureCategory::OutcomeUnknown);
        assert!(!failure.retry_safe);
        assert_eq!(failure.required_user_action, QUARANTINE_ACTION);
        assert_eq!(
            failure.redacted_detail.as_deref(),
            Some("The ChatGPT Provider Turn exceeded its consecutive tool-round budget.")
        );
        assert_eq!(
            attempts.load(Ordering::SeqCst),
            MAX_TOOL_ROUNDS as usize,
            "the round past the cap must not authorize any tool"
        );
        assert_eq!(
            backend.requests.lock().unwrap().len(),
            MAX_TOOL_ROUNDS as usize + 1,
            "initial request plus one follow-up per capped round"
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn midstream_abort_after_created_is_outcome_unknown() {
        let backend = ScriptedBackend::new(vec![Ok(sse(PARTIAL_STREAM))]);
        let auth = stored_connection("at-1");
        let (token_client, _bodies) = mock_issuer(ISSUER_BODY);
        let home = temp_home("abort");
        let mut sink = |_: BackendEvent| {};
        let no_tools = |_name: &str, _arguments: &str| -> Result<Value, ToolRejection> {
            panic!("no tool call expected")
        };

        let failure = run_turn(
            &backend,
            &auth,
            &token_client,
            &home,
            &no_tools,
            &|| false,
            &mut sink,
        )
        .expect_err("aborted stream must fail the turn");

        assert_eq!(failure.category, ProviderFailureCategory::OutcomeUnknown);
        assert!(!failure.retry_safe);
        assert_eq!(backend.request_bodies().len(), 1);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn persistence_failure_on_created_stops_delivery_and_quarantines() {
        let backend = ScriptedBackend::new(vec![Ok(sse(TOOL_RESPONSE_ONE))]);
        let auth = stored_connection("at-1");
        let (token_client, _bodies) = mock_issuer(ISSUER_BODY);
        let home = temp_home("event-persistence-failure");
        let tool_executions = Arc::new(AtomicUsize::new(0));
        let executions = Arc::clone(&tool_executions);
        let authorize = move |_: &str, _: &str| -> Result<Value, ToolRejection> {
            executions.fetch_add(1, Ordering::SeqCst);
            Ok(json!({"unexpected": true}))
        };
        let delivered = Arc::new(AtomicUsize::new(0));
        let delivery_count = Arc::clone(&delivered);
        let mut on_event = move |_: BackendEvent| {
            delivery_count.fetch_add(1, Ordering::SeqCst);
            Err(ProviderFailure::new(
                ProviderFailureCategory::ProcessFailed,
                true,
                "Repair persistence before retrying.",
                Some("Authoritative event persistence failed."),
            ))
        };
        let mut on_denied = |_: &str, _: &str, _: &str, _: &str| Ok(());

        let failure = run_turn_with_callbacks(
            &backend,
            &auth,
            &token_client,
            &home,
            &authorize,
            &|| false,
            &mut on_event,
            &mut on_denied,
        )
        .unwrap_err();

        assert_eq!(failure.category, ProviderFailureCategory::OutcomeUnknown);
        assert!(!failure.retry_safe);
        assert_eq!(delivered.load(Ordering::SeqCst), 1);
        assert_eq!(tool_executions.load(Ordering::SeqCst), 0);
        assert_eq!(backend.request_bodies().len(), 1);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn denial_persistence_failure_stops_before_follow_up_request() {
        let backend = ScriptedBackend::new(vec![Ok(sse(TOOL_RESPONSE_ONE))]);
        let auth = stored_connection("at-1");
        let (token_client, _bodies) = mock_issuer(ISSUER_BODY);
        let home = temp_home("denial-persistence-failure");
        let authorize = |_: &str, _: &str| -> Result<Value, ToolRejection> {
            Err(ToolRejection::NotPermitted("tool_not_granted"))
        };
        let mut on_event = |_: BackendEvent| Ok(());
        let captured = Arc::new(Mutex::new(Vec::new()));
        let captured_for_callback = Arc::clone(&captured);
        let mut on_denied = move |call_id: &str, name: &str, arguments: &str, reason: &str| {
            captured_for_callback.lock().unwrap().push((
                call_id.to_owned(),
                name.to_owned(),
                arguments.to_owned(),
                reason.to_owned(),
            ));
            Err(ProviderFailure::new(
                ProviderFailureCategory::ProcessFailed,
                true,
                "Repair persistence before retrying.",
                Some("Authoritative denial persistence failed."),
            ))
        };

        let failure = run_turn_with_callbacks(
            &backend,
            &auth,
            &token_client,
            &home,
            &authorize,
            &|| false,
            &mut on_event,
            &mut on_denied,
        )
        .unwrap_err();

        assert_eq!(failure.category, ProviderFailureCategory::OutcomeUnknown);
        assert_eq!(backend.request_bodies().len(), 1);
        assert_eq!(
            *captured.lock().unwrap(),
            vec![(
                "call_abc".to_owned(),
                "read_file".to_owned(),
                r#"{"path":"spec.md"}"#.to_owned(),
                "tool_not_granted".to_owned(),
            )]
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn cancellation_before_the_first_request_never_calls_the_backend() {
        let backend = ScriptedBackend::new(vec![Ok(sse(TEXT_COMPLETION))]);
        let auth = stored_connection("at-1");
        let (token_client, _bodies) = mock_issuer(ISSUER_BODY);
        let home = temp_home("cancel-early");
        let mut sink = |_: BackendEvent| {};
        let no_tools = |_name: &str, _arguments: &str| -> Result<Value, ToolRejection> {
            panic!("no tool call expected")
        };

        let failure = run_turn(
            &backend,
            &auth,
            &token_client,
            &home,
            &no_tools,
            &|| true,
            &mut sink,
        )
        .expect_err("cancellation must interrupt the turn");

        assert_eq!(failure.category, ProviderFailureCategory::Interrupted);
        assert!(backend.request_bodies().is_empty());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn cancellation_between_responses_quarantines_an_advanced_turn() {
        let backend =
            ScriptedBackend::new(vec![Ok(sse(TOOL_RESPONSE_ONE)), Ok(sse(TOOL_RESPONSE_TWO))]);
        let auth = stored_connection("at-1");
        let (token_client, _bodies) = mock_issuer(ISSUER_BODY);
        let home = temp_home("cancel-late");
        let cancelled = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let sink_flag = Arc::clone(&cancelled);
        let mut sink = move |event: BackendEvent| {
            if matches!(event, StreamEvent::Completed { .. }) {
                sink_flag.store(true, Ordering::SeqCst);
            }
        };
        let cancel_flag = Arc::clone(&cancelled);
        let is_cancelled = move || cancel_flag.load(Ordering::SeqCst);
        let executed = Arc::new(AtomicUsize::new(0));
        let executions = Arc::clone(&executed);
        let no_tools = move |_name: &str, _arguments: &str| -> Result<Value, ToolRejection> {
            executions.fetch_add(1, Ordering::SeqCst);
            panic!("tools must not run once cancellation was observed")
        };

        let failure = run_turn(
            &backend,
            &auth,
            &token_client,
            &home,
            &no_tools,
            &is_cancelled,
            &mut sink,
        )
        .expect_err("cancellation must interrupt the turn chain");

        assert_eq!(failure.category, ProviderFailureCategory::OutcomeUnknown);
        assert_eq!(
            backend.request_bodies().len(),
            1,
            "no follow-up request may be issued after cancellation"
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn http_401_refreshes_once_and_retries_with_fresh_auth() {
        let backend = ScriptedBackend::new(vec![
            Err(BackendError::AuthenticationRequired),
            Ok(sse(TEXT_COMPLETION)),
        ]);
        let auth = StoredConnection {
            expires_at_ms: u64::MAX,
            ..stored_connection("at-stale")
        };
        assert!(!needs_refresh(&auth, test_now_ms()));
        let (token_client, issuer_bodies) = mock_issuer(ISSUER_BODY);
        let home = temp_home("refresh-once");
        save(&home, &auth).unwrap();
        let mut sink = |_: BackendEvent| {};
        let no_tools = |_name: &str, _arguments: &str| -> Result<Value, ToolRejection> {
            panic!("no tool call expected")
        };

        run_turn(
            &backend,
            &auth,
            &token_client,
            &home,
            &no_tools,
            &|| false,
            &mut sink,
        )
        .expect("refresh must recover the turn");

        let tokens = backend.access_tokens.lock().unwrap();
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0], "at-stale");
        assert_eq!(tokens[1], "at-new");
        drop(tokens);
        let issued = issuer_bodies.lock().unwrap();
        assert_eq!(issued.len(), 1, "exactly one refresh call");
        assert!(issued[0].contains("grant_type=refresh_token"));
        assert!(issued[0].contains("refresh_token=refresh-old"));

        match load(&home) {
            LoadState::Connected(loaded) => {
                assert_eq!(loaded.access_token, "at-new");
                assert_eq!(loaded.refresh_token, "refresh-new");
                assert!(loaded.id_token.contains("eyJjaGF0Z3B0X2FjY291bnRfaWQi"));
            }
            other => panic!("expected a persisted refreshed connection, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn account_replacement_between_401_and_retry_never_receives_tender_content() {
        let backend = ScriptedBackend::new(vec![
            Err(BackendError::AuthenticationRequired),
            Ok(sse(TEXT_COMPLETION)),
        ]);
        let auth = stored_connection("at-account-a");
        let mut replacement = stored_connection("at-account-b");
        replacement.account_id = "acc-replacement".to_owned();
        let refresh_auth = |expected_account_id: &str| {
            assert_eq!(expected_account_id, "acc-77");
            Ok(cloned_connection(&replacement))
        };
        let mut sink = |_: BackendEvent| {};
        let no_tools = |_name: &str, _arguments: &str| -> Result<Value, ToolRejection> {
            panic!("no tool call expected")
        };

        let failure = run_turn_with_refresh(
            &backend,
            &auth,
            &refresh_auth,
            &no_tools,
            &|| false,
            &mut sink,
        )
        .expect_err("a replacement account must not inherit the approved retry");

        assert_eq!(
            failure.category,
            ProviderFailureCategory::AuthenticationRequired
        );
        assert_eq!(
            backend.access_tokens.lock().unwrap().clone(),
            vec!["at-account-a".to_owned()],
            "the replacement account must not receive the Tender request"
        );
        assert_eq!(backend.request_bodies().len(), 1, "no retry may be sent");
    }

    #[test]
    fn second_unauthorized_response_surfaces_authentication_required() {
        let backend = ScriptedBackend::new(vec![
            Err(BackendError::AuthenticationRequired),
            Err(BackendError::AuthenticationRequired),
        ]);
        let auth = stored_connection("at-stale");
        let (token_client, issuer_bodies) = mock_issuer(ISSUER_BODY);
        let home = temp_home("refresh-exhausted");
        save(&home, &auth).unwrap();
        let mut sink = |_: BackendEvent| {};
        let no_tools = |_name: &str, _arguments: &str| -> Result<Value, ToolRejection> {
            panic!("no tool call expected")
        };

        let failure = run_turn(
            &backend,
            &auth,
            &token_client,
            &home,
            &no_tools,
            &|| false,
            &mut sink,
        )
        .expect_err("a persistent 401 must surface as authentication failure");

        assert_eq!(
            failure.category,
            ProviderFailureCategory::AuthenticationRequired
        );
        assert_eq!(issuer_bodies.lock().unwrap().len(), 1, "refresh only once");
        assert_eq!(backend.access_tokens.lock().unwrap().len(), 2);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn rate_limit_failure_preserves_retry_after_through_provider_failure() {
        for (source, retry_after_ms, expected) in [
            ("retry-after-ms", Some(1500), Some(1500)),
            ("retry-after seconds", Some(2000), Some(2000)),
            ("missing retry-after", None, None),
        ] {
            let backend =
                ScriptedBackend::new(vec![Err(BackendError::RateLimited { retry_after_ms })]);
            let auth = stored_connection("at-rate-limited");
            let refresh_auth =
                |_expected_account_id: &str| -> Result<StoredConnection, ProviderFailure> {
                    panic!("a rate-limited request must not refresh authentication")
                };
            let no_tools = |_name: &str, _arguments: &str| -> Result<Value, ToolRejection> {
                panic!("no tool call expected")
            };
            let mut sink = |_: BackendEvent| {};

            let failure = run_turn_with_refresh(
                &backend,
                &auth,
                &refresh_auth,
                &no_tools,
                &|| false,
                &mut sink,
            )
            .expect_err("{source} must surface as a provider failure");

            assert_eq!(failure.category, ProviderFailureCategory::RateLimited);
            assert_eq!(failure.retry_after_milliseconds, expected, "{source}");
        }
    }

    #[test]
    fn request_budget_failure_is_distinct_nonretryable_and_redacted() {
        let request_bytes = super::super::client::DIRECT_PROVIDER_REQUEST_HARD_CAP_BYTES + 1;

        let failure = backend_failure(BackendError::RequestBudgetExceeded { request_bytes }, false);

        assert_eq!(
            failure.category,
            ProviderFailureCategory::RequestBudgetExceeded
        );
        assert!(!failure.retry_safe);
        assert_eq!(failure.request_body_bytes, Some(request_bytes));
        assert!(!failure
            .required_user_action
            .contains(&request_bytes.to_string()));
        assert!(!failure
            .redacted_detail
            .as_deref()
            .unwrap_or_default()
            .contains(&request_bytes.to_string()));
        assert!(serde_json::to_value(&failure)
            .expect("serialize provider failure")
            .get("request_body_bytes")
            .is_none());
    }
}
