use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use crate::chatgpt_oauth::StoredConnection;

#[cfg_attr(feature = "runtime-fixture", allow(dead_code))]
pub(crate) const BACKEND_URL: &str = "https://chatgpt.com/backend-api/codex/responses";
const ORIGINATOR: &str = "quantix";
const COMPUTE_RESIDENCY_HEADER: &str = "x-openai-internal-codex-residency";

#[derive(Debug)]
pub(crate) struct BackendRequest {
    pub model: String,
    pub instructions: String,
    pub input_items: Vec<serde_json::Value>,
    pub tools: Vec<serde_json::Value>,
    pub store: bool,
    pub include_reasoning: bool,
    pub reasoning_effort: Option<ReasoningEffort>,
    pub session_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReasoningEffort {
    None,
    Low,
    Medium,
    High,
    XHigh,
}

impl ReasoningEffort {
    fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::XHigh => "xhigh",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UsageSnapshot {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub cached_input_tokens: Option<u64>,
    pub reasoning_output_tokens: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FailureCode {
    ResponseFailed,
    ResponseIncomplete,
    InBandError,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RedactedFailure {
    pub code: FailureCode,
    pub detail: String,
}

impl std::fmt::Display for RedactedFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "chatgpt stream failure ({:?}): {}",
            self.code, self.detail
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum StreamEvent {
    Created {
        response_id: String,
    },
    ItemAdded(serde_json::Value),
    ItemDone(serde_json::Value),
    TextDelta(String),
    FunctionCallDelta {
        call_id: String,
        name: String,
        args_delta: String,
    },
    FunctionCallDone {
        call_id: String,
        name: String,
        arguments: String,
    },
    Completed {
        response_id: String,
        usage: UsageSnapshot,
    },
    Errored(RedactedFailure),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TurnDisposition {
    Completed,
    Failed,
}

#[derive(Debug)]
pub(crate) enum BackendError {
    AuthenticationRequired,
    RateLimited { retry_after_ms: Option<u64> },
    Protocol(String),
    Transport(String),
    Interrupted,
    EventDelivery,
}

impl std::fmt::Display for BackendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackendError::AuthenticationRequired => write!(f, "chatgpt authentication required"),
            BackendError::RateLimited { retry_after_ms } => match retry_after_ms {
                Some(ms) => write!(f, "chatgpt rate limited (retry after {ms} ms)"),
                None => write!(f, "chatgpt rate limited"),
            },
            BackendError::Protocol(detail) => write!(f, "chatgpt protocol error: {detail}"),
            BackendError::Transport(detail) => write!(f, "chatgpt transport error: {detail}"),
            BackendError::Interrupted => write!(f, "chatgpt request interrupted"),
            BackendError::EventDelivery => write!(f, "chatgpt event delivery failed"),
        }
    }
}

impl std::error::Error for BackendError {}

pub(crate) trait ChatGptBackend: Send + Sync {
    fn create_response<'a>(
        &'a self,
        auth: &'a StoredConnection,
        req: &'a BackendRequest,
        is_cancelled: &'a (dyn Fn() -> bool + Sync),
        on_event: &'a mut (dyn FnMut(StreamEvent) -> Result<(), BackendError> + Send),
    ) -> Pin<Box<dyn Future<Output = Result<TurnDisposition, BackendError>> + Send + 'a>>;
}

fn user_agent() -> &'static str {
    concat!("quantix/", env!("CARGO_PKG_VERSION"))
}

pub(crate) fn build_request_body(req: &BackendRequest) -> serde_json::Value {
    let mut body = serde_json::Map::new();
    body.insert("model".into(), req.model.clone().into());
    body.insert("instructions".into(), req.instructions.clone().into());
    body.insert("input".into(), req.input_items.clone().into());
    body.insert("tools".into(), req.tools.clone().into());
    body.insert("store".into(), req.store.into());
    body.insert("stream".into(), true.into());
    if req.include_reasoning {
        body.insert(
            "include".into(),
            serde_json::json!(["reasoning.encrypted_content"]),
        );
    }
    if let Some(effort) = req.reasoning_effort {
        body.insert(
            "reasoning".into(),
            serde_json::json!({"effort": effort.as_str(), "summary": "auto"}),
        );
    }
    serde_json::Value::Object(body)
}

enum Terminal {
    Completed,
    Failed,
}

fn opt_str_field(value: &serde_json::Value, field: &str) -> Option<String> {
    value
        .get(field)
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

fn parse_usage(response: &serde_json::Value) -> UsageSnapshot {
    let usage = response.get("usage");
    let detail = |pointer: &str| {
        usage
            .and_then(|usage| usage.pointer(pointer))
            .and_then(serde_json::Value::as_u64)
    };
    UsageSnapshot {
        input_tokens: detail("/input_tokens").unwrap_or(0),
        cached_input_tokens: detail("/input_tokens_details/cached_tokens"),
        output_tokens: detail("/output_tokens").unwrap_or(0),
        reasoning_output_tokens: detail("/output_tokens_details/reasoning_tokens"),
        total_tokens: detail("/total_tokens").unwrap_or(0),
    }
}

struct SseParser<'a> {
    on_event: &'a mut (dyn FnMut(StreamEvent) -> Result<(), BackendError> + Send),
    buffer: Vec<u8>,
    data_lines: Vec<String>,
    pending_calls: HashMap<String, (String, String)>,
    terminal: Option<Terminal>,
    done_seen: bool,
    response_id: Option<String>,
}

impl<'a> SseParser<'a> {
    fn new(on_event: &'a mut (dyn FnMut(StreamEvent) -> Result<(), BackendError> + Send)) -> Self {
        Self {
            on_event,
            buffer: Vec::new(),
            data_lines: Vec::new(),
            pending_calls: HashMap::new(),
            terminal: None,
            done_seen: false,
            response_id: None,
        }
    }

    fn feed(&mut self, chunk: &[u8]) -> Result<(), BackendError> {
        if self.terminal.is_some() || self.done_seen {
            return Ok(());
        }
        self.buffer.extend_from_slice(chunk);
        while let Some(newline) = self.buffer.iter().position(|byte| *byte == b'\n') {
            let mut line: Vec<u8> = self.buffer.drain(..=newline).collect();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            self.process_line(&line)?;
            if self.terminal.is_some() || self.done_seen {
                self.buffer.clear();
                return Ok(());
            }
        }
        Ok(())
    }

    fn finish(&mut self) -> Result<(), BackendError> {
        if self.terminal.is_none() && !self.done_seen && !self.buffer.is_empty() {
            let line = std::mem::take(&mut self.buffer);
            self.process_line(&line)?;
        } else {
            self.buffer.clear();
        }
        self.dispatch_frame()
    }

    fn process_line(&mut self, line: &[u8]) -> Result<(), BackendError> {
        if line.is_empty() {
            return self.dispatch_frame();
        }
        if line[0] == b':' {
            return Ok(());
        }
        let text = String::from_utf8_lossy(line);
        if let Some(rest) = text.strip_prefix("data:") {
            let value = rest.strip_prefix(' ').unwrap_or(rest);
            self.data_lines.push(value.to_string());
        }
        Ok(())
    }

    fn dispatch_frame(&mut self) -> Result<(), BackendError> {
        if self.data_lines.is_empty() || self.terminal.is_some() || self.done_seen {
            self.data_lines.clear();
            return Ok(());
        }
        let data = self.data_lines.join("\n");
        self.data_lines.clear();
        if data == "[DONE]" {
            self.done_seen = true;
            return Ok(());
        }
        let value = serde_json::from_str::<serde_json::Value>(&data)
            .map_err(|_| BackendError::Protocol("malformed SSE JSON frame".to_string()))?;
        let event_type = required_str(&value, "type", "SSE event")?;
        match event_type {
            "response.created" => {
                let response_id = value
                    .get("response")
                    .and_then(|response| response.get("id"))
                    .and_then(serde_json::Value::as_str)
                    .filter(|id| !id.is_empty())
                    .ok_or_else(|| {
                        BackendError::Protocol("malformed response.created event".to_string())
                    })?
                    .to_string();
                if self
                    .response_id
                    .as_deref()
                    .is_some_and(|seen| seen != response_id)
                {
                    return Err(BackendError::Protocol(
                        "inconsistent response id in stream".to_string(),
                    ));
                }
                self.response_id = Some(response_id.clone());
                (self.on_event)(StreamEvent::Created { response_id })?;
            }
            "response.output_item.added" => {
                let item = value.get("item").ok_or_else(|| {
                    BackendError::Protocol("malformed response.output_item.added event".to_string())
                })?;
                if item.get("type").and_then(serde_json::Value::as_str) == Some("function_call") {
                    self.pending_calls.insert(
                        required_str(item, "id", "function call item")?.to_string(),
                        (
                            required_str(item, "call_id", "function call item")?.to_string(),
                            required_str(item, "name", "function call item")?.to_string(),
                        ),
                    );
                }
                (self.on_event)(StreamEvent::ItemAdded(value))?;
            }
            "response.output_text.delta" => {
                (self.on_event)(StreamEvent::TextDelta(
                    required_str(&value, "delta", "response.output_text.delta event")?.to_string(),
                ))?;
            }
            "response.function_call_arguments.delta" => {
                let item_id = required_str(
                    &value,
                    "item_id",
                    "response.function_call_arguments.delta event",
                )?
                .to_string();
                let (call_id, name) = self
                    .pending_calls
                    .get(&item_id)
                    .cloned()
                    .unwrap_or_else(|| (item_id, String::new()));
                (self.on_event)(StreamEvent::FunctionCallDelta {
                    call_id,
                    name,
                    args_delta: required_str(
                        &value,
                        "delta",
                        "response.function_call_arguments.delta event",
                    )?
                    .to_string(),
                })?;
            }
            "response.output_item.done" => {
                let item = value.get("item").ok_or_else(|| {
                    BackendError::Protocol("malformed response.output_item.done event".to_string())
                })?;
                (self.on_event)(StreamEvent::ItemDone(value.clone()))?;
                if item.get("type").and_then(serde_json::Value::as_str) != Some("function_call") {
                    return Ok(());
                }
                let fallback =
                    self.pending_calls
                        .remove(required_str(item, "id", "function call item")?);
                let (fallback_call_id, fallback_name) = fallback.unwrap_or_default();
                let call_id = opt_str_field(item, "call_id").unwrap_or(fallback_call_id);
                let name = opt_str_field(item, "name").unwrap_or(fallback_name);
                let arguments = required_str(item, "arguments", "function call item")?.to_string();
                if call_id.is_empty() || name.is_empty() {
                    return Err(BackendError::Protocol(
                        "malformed function call item".to_string(),
                    ));
                }
                (self.on_event)(StreamEvent::FunctionCallDone {
                    call_id,
                    name,
                    arguments,
                })?;
            }
            "response.completed" => {
                let response = value.get("response").ok_or_else(|| {
                    BackendError::Protocol("malformed response.completed event".to_string())
                })?;
                let response_id = required_str(response, "id", "response.completed event")?;
                if self
                    .response_id
                    .as_deref()
                    .is_some_and(|seen| seen != response_id)
                {
                    return Err(BackendError::Protocol(
                        "inconsistent response id in stream".to_string(),
                    ));
                }
                (self.on_event)(StreamEvent::Completed {
                    response_id: response_id.to_string(),
                    usage: parse_usage(response),
                })?;
                self.terminal = Some(Terminal::Completed);
            }
            "response.failed" => {
                (self.on_event)(StreamEvent::Errored(RedactedFailure {
                    code: FailureCode::ResponseFailed,
                    detail: "provider reported response.failed".to_string(),
                }))?;
                self.terminal = Some(Terminal::Failed);
            }
            "response.incomplete" => {
                let reason = value
                    .pointer("/response/incomplete_details/reason")
                    .and_then(serde_json::Value::as_str);
                let detail = match reason {
                    Some("max_output_tokens") => {
                        "stream ended early (output token cap reached)".to_string()
                    }
                    Some("content_filter") => "stream ended early (content filtered)".to_string(),
                    _ => "stream ended early (unclassified reason)".to_string(),
                };
                (self.on_event)(StreamEvent::Errored(RedactedFailure {
                    code: FailureCode::ResponseIncomplete,
                    detail,
                }))?;
                self.terminal = Some(Terminal::Failed);
            }
            "error" => {
                (self.on_event)(StreamEvent::Errored(RedactedFailure {
                    code: FailureCode::InBandError,
                    detail: "provider sent an in-band error event".to_string(),
                }))?;
                self.terminal = Some(Terminal::Failed);
            }
            _ => {}
        }
        Ok(())
    }
}

fn required_str<'a>(
    value: &'a serde_json::Value,
    field: &str,
    event: &str,
) -> Result<&'a str, BackendError> {
    value
        .get(field)
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| BackendError::Protocol(format!("malformed {event}")))
}

#[cfg(test)]
pub(crate) fn parse_stream_bytes(
    bytes: &[u8],
    on_event: &mut (dyn FnMut(StreamEvent) -> Result<(), BackendError> + Send),
) -> Result<TurnDisposition, BackendError> {
    let mut parser = SseParser::new(on_event);
    parser.feed(bytes)?;
    parser.finish()?;
    finish_disposition(parser.terminal)
}

fn finish_disposition(terminal: Option<Terminal>) -> Result<TurnDisposition, BackendError> {
    match terminal {
        Some(Terminal::Completed) => Ok(TurnDisposition::Completed),
        Some(Terminal::Failed) => Ok(TurnDisposition::Failed),
        None => Err(BackendError::Transport(
            "connection closed before terminal event".to_string(),
        )),
    }
}

async fn pump_response(
    mut response: reqwest::Response,
    is_cancelled: &(dyn Fn() -> bool + Sync),
    on_event: &mut (dyn FnMut(StreamEvent) -> Result<(), BackendError> + Send),
) -> Result<TurnDisposition, BackendError> {
    let mut parser = SseParser::new(on_event);
    loop {
        if is_cancelled() {
            return Err(BackendError::Interrupted);
        }
        tokio::select! {
            chunk = response.chunk() => match chunk {
                Ok(Some(bytes)) => parser.feed(&bytes)?,
                Ok(None) => break,
                Err(_) => {
                    return Err(BackendError::Transport(
                        "stream read failed".to_string(),
                    ));
                }
            },
            () = tokio::time::sleep(Duration::from_millis(25)) => {
                continue;
            }
        }
        if parser.terminal.is_some() || parser.done_seen {
            break;
        }
    }
    parser.finish()?;
    finish_disposition(parser.terminal)
}

pub(super) fn status_error(status: u16, retry_after_ms: Option<u64>) -> BackendError {
    match status {
        401 | 403 => BackendError::AuthenticationRequired,
        429 => BackendError::RateLimited { retry_after_ms },
        _ => BackendError::Protocol(format!("unexpected HTTP status {status}")),
    }
}

fn retry_after_ms(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    if let Some(value) = header_value(headers, "retry-after-ms") {
        if let Ok(ms) = value.parse::<u64>() {
            return Some(ms);
        }
    }
    header_value(headers, "retry-after")?
        .trim()
        .parse::<u64>()
        .ok()
        .map(|secs| secs.saturating_mul(1000))
}

fn header_value(headers: &reqwest::header::HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
}

pub(crate) struct ReqwestBackend {
    endpoint: String,
    http: reqwest::Client,
}

impl ReqwestBackend {
    pub(crate) fn new(endpoint: &str) -> Result<Self, reqwest::Error> {
        Ok(Self {
            endpoint: endpoint.to_string(),
            http: reqwest::Client::builder()
                .connect_timeout(std::time::Duration::from_secs(30))
                .build()?,
        })
    }
}

impl ChatGptBackend for ReqwestBackend {
    fn create_response<'a>(
        &'a self,
        auth: &'a StoredConnection,
        req: &'a BackendRequest,
        is_cancelled: &'a (dyn Fn() -> bool + Sync),
        on_event: &'a mut (dyn FnMut(StreamEvent) -> Result<(), BackendError> + Send),
    ) -> Pin<Box<dyn Future<Output = Result<TurnDisposition, BackendError>> + Send + 'a>> {
        Box::pin(async move {
            if is_cancelled() {
                return Err(BackendError::Interrupted);
            }
            let mut request = self
                .http
                .post(&self.endpoint)
                .bearer_auth(&auth.access_token)
                .header("ChatGPT-Account-Id", &auth.account_id)
                .header("originator", ORIGINATOR)
                .header("session-id", &req.session_id)
                .header("user-agent", user_agent());
            if let Some(compute_residency) = auth
                .compute_residency
                .as_deref()
                .filter(|value| *value != "no_constraint")
            {
                request = request.header(COMPUTE_RESIDENCY_HEADER, compute_residency);
            }
            let send = request.json(&build_request_body(req)).send();
            tokio::pin!(send);
            let response = loop {
                tokio::select! {
                    response = &mut send => {
                        break response.map_err(|_| {
                            BackendError::Transport("request failed".to_string())
                        })?;
                    }
                    () = tokio::time::sleep(Duration::from_millis(25)) => {
                        if is_cancelled() {
                            return Err(BackendError::Interrupted);
                        }
                    }
                }
            };
            let status = response.status();
            if !status.is_success() {
                return Err(status_error(
                    status.as_u16(),
                    retry_after_ms(response.headers()),
                ));
            }
            pump_response(response, is_cancelled, on_event).await
        })
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::{Arc, Mutex};

    use super::*;

    const SCRIPT_DIR: &str = "tests/support/backend_scripts";
    const SECRET_TOKEN: &str = "super-secret-access-token-value";

    fn script_bytes(name: &str) -> Vec<u8> {
        std::fs::read(format!("{SCRIPT_DIR}/{name}.sse")).unwrap()
    }

    fn sample_request() -> BackendRequest {
        BackendRequest {
            model: "gpt-5.5".to_string(),
            instructions: "system prompt".to_string(),
            input_items: vec![serde_json::json!({
                "type": "message",
                "role": "user",
                "content": [{"type": "input_text", "text": "hi"}]
            })],
            tools: vec![serde_json::json!({
                "type": "function",
                "name": "read_file",
                "description": "reads a file",
                "parameters": {"type": "object", "properties": {}},
                "strict": true
            })],
            store: false,
            include_reasoning: true,
            reasoning_effort: Some(ReasoningEffort::High),
            session_id: "ses-1234".to_string(),
        }
    }

    fn sample_auth() -> StoredConnection {
        StoredConnection {
            access_token: SECRET_TOKEN.to_string(),
            refresh_token: "refresh-1".to_string(),
            id_token: "id-1".to_string(),
            expires_at_ms: 5_000,
            account_id: "acc-77".to_string(),
            plan_type: Some("plus".to_string()),
            compute_residency: Some("us".to_string()),
        }
    }

    #[test]
    fn happy_text_script_parses_incrementally_to_expected_events() {
        let bytes = script_bytes("happy-text");
        let collected: Arc<Mutex<Vec<StreamEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&collected);
        let mut sink_fn = move |event| {
            sink.lock().unwrap().push(event);
            Ok(())
        };
        let mut parser = SseParser::new(&mut sink_fn);
        for chunk in bytes.chunks(7) {
            parser.feed(chunk).unwrap();
        }
        parser.finish().unwrap();

        let events = collected.lock().unwrap();
        assert_eq!(events.len(), 6, "unexpected event count: {events:?}");
        assert!(matches!(
            &events[0],
            StreamEvent::Created { response_id } if response_id == "resp_text_1"
        ));
        assert!(matches!(
            &events[1],
            StreamEvent::ItemAdded(value)
                if value["item"]["type"] == "message" && value["output_index"] == 0
        ));
        match (&events[2], &events[3]) {
            (StreamEvent::TextDelta(a), StreamEvent::TextDelta(b)) => {
                assert_eq!(format!("{a}{b}"), "Hello from the fixture.");
            }
            other => panic!("expected two text deltas, got {other:?}"),
        }
        assert!(matches!(
            &events[4],
            StreamEvent::ItemDone(value)
                if value["item"]["type"] == "message" && value["output_index"] == 0
        ));
        match &events[5] {
            StreamEvent::Completed { response_id, usage } => {
                assert_eq!(response_id, "resp_text_1");
                assert_eq!(
                    usage,
                    &UsageSnapshot {
                        input_tokens: 120,
                        cached_input_tokens: Some(32),
                        output_tokens: 45,
                        reasoning_output_tokens: Some(16),
                        total_tokens: 165,
                    }
                );
            }
            other => panic!("expected completed terminal, got {other:?}"),
        }
    }

    #[test]
    fn tool_roundtrip_script_yields_exactly_one_function_call_done() {
        let bytes = script_bytes("tool-roundtrip");
        let collected: Arc<Mutex<Vec<StreamEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&collected);
        let disposition = parse_stream_bytes(&bytes, &mut move |event| {
            sink.lock().unwrap().push(event);
            Ok(())
        })
        .unwrap();

        assert_eq!(disposition, TurnDisposition::Completed);
        let events = collected.lock().unwrap();
        let deltas: Vec<&StreamEvent> = events
            .iter()
            .filter(|event| matches!(event, StreamEvent::FunctionCallDelta { .. }))
            .collect();
        assert_eq!(deltas.len(), 2, "expected streamed argument deltas");
        for delta in deltas {
            match delta {
                StreamEvent::FunctionCallDelta {
                    call_id,
                    name,
                    args_delta: _,
                } => {
                    assert_eq!(call_id, "call_abc");
                    assert_eq!(name, "read_file");
                }
                other => panic!("unexpected delta shape {other:?}"),
            }
        }
        let accumulated: String = events
            .iter()
            .filter_map(|event| match event {
                StreamEvent::FunctionCallDelta { args_delta, .. } => Some(args_delta.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(accumulated, r#"{"path":"spec.md"}"#);
        let dones: Vec<&StreamEvent> = events
            .iter()
            .filter(|event| matches!(event, StreamEvent::FunctionCallDone { .. }))
            .collect();
        assert_eq!(dones.len(), 1, "arguments.done must not double-emit");
        match dones[0] {
            StreamEvent::FunctionCallDone {
                call_id,
                name,
                arguments,
            } => {
                assert_eq!(call_id, "call_abc");
                assert_eq!(name, "read_file");
                assert_eq!(arguments, r#"{"path":"spec.md"}"#);
            }
            other => panic!("unexpected done shape {other:?}"),
        }
    }

    #[test]
    fn unknown_event_types_and_comments_are_tolerated() {
        let mut bytes = b": keep-alive comment\r\n\r\nevent: ping\r\ndata: {\"type\":\"response.custom_future_thing\",\"payload\":{\"x\":1}}\r\n\r\ndata: {\"type\":\"response.output_text.delta\",\"item_id\":\"m\",\"delta\":\"ok\"}\r\n\r\n".to_vec();
        bytes.extend_from_slice(
            br#"data: {"type":"response.completed","response":{"id":"r","usage":{"input_tokens":1,"output_tokens":2,"total_tokens":3}}}"#,
        );
        bytes.extend_from_slice(b"\r\n\r\n");
        let collected: Arc<Mutex<Vec<StreamEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&collected);
        let disposition = parse_stream_bytes(&bytes, &mut move |event| {
            sink.lock().unwrap().push(event);
            Ok(())
        })
        .unwrap();
        assert_eq!(disposition, TurnDisposition::Completed);
        let events = collected.lock().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(
            events[0],
            StreamEvent::TextDelta("ok".to_string()),
            "unknown types must be skipped without breaking framing"
        );
        assert!(
            matches!(&events[1], StreamEvent::Completed { response_id, .. } if response_id == "r")
        );
    }

    #[test]
    fn failure_terminals_map_to_errored_events_and_failed_disposition() {
        let cases: [(&str, &str, FailureCode); 3] = [
            (
                "response.failed",
                r#"data: {"type":"response.failed","response":{"id":"r","error":{"code":"server_error"}}}"#,
                FailureCode::ResponseFailed,
            ),
            (
                "response.incomplete",
                r#"data: {"type":"response.incomplete","response":{"id":"r","incomplete_details":{"reason":"max_output_tokens"}}}"#,
                FailureCode::ResponseIncomplete,
            ),
            (
                "error",
                r#"data: {"type":"error","code":"rate_limit_exceeded","message":"slow down"}"#,
                FailureCode::InBandError,
            ),
        ];
        for (label, frame, code) in cases {
            let mut bytes = frame.as_bytes().to_vec();
            bytes.extend_from_slice(b"\n\n");
            let collected: Arc<Mutex<Vec<StreamEvent>>> = Arc::new(Mutex::new(Vec::new()));
            let sink = Arc::clone(&collected);
            let disposition = parse_stream_bytes(&bytes, &mut move |event| {
                sink.lock().unwrap().push(event);
                Ok(())
            })
            .unwrap();
            assert_eq!(disposition, TurnDisposition::Failed, "case {label}");
            let events = collected.lock().unwrap();
            assert_eq!(events.len(), 1, "case {label}");
            match &events[0] {
                StreamEvent::Errored(failure) => assert_eq!(failure.code, code, "case {label}"),
                other => panic!("case {label}: expected errored, got {other:?}"),
            }
        }
    }

    #[test]
    fn incomplete_reason_is_redacted_to_static_detail() {
        let frame = r#"data: {"type":"response.incomplete","response":{"id":"r","incomplete_details":{"reason":"canary_reason_x"}}}"#;
        let mut bytes = frame.as_bytes().to_vec();
        bytes.extend_from_slice(b"\n\n");
        let collected: Arc<Mutex<Vec<StreamEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&collected);
        let disposition = parse_stream_bytes(&bytes, &mut move |event| {
            sink.lock().unwrap().push(event);
            Ok(())
        })
        .unwrap();

        assert_eq!(disposition, TurnDisposition::Failed);
        let events = collected.lock().unwrap();
        match &events[0] {
            StreamEvent::Errored(failure) => {
                assert_eq!(failure.code, FailureCode::ResponseIncomplete);
                let rendered = format!("{failure:?} / {failure}");
                assert!(
                    !rendered.contains("canary_reason_x"),
                    "server-provided reason must not reach the rendered detail: {rendered}"
                );
            }
            other => panic!("expected errored event, got {other:?}"),
        }
    }

    #[test]
    fn stream_ending_without_terminal_is_a_transport_error() {
        let bytes = script_bytes("midstream-abort");
        let collected: Arc<Mutex<Vec<StreamEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&collected);
        let error = parse_stream_bytes(&bytes, &mut move |event| {
            sink.lock().unwrap().push(event);
            Ok(())
        })
        .unwrap_err();
        assert!(matches!(error, BackendError::Transport(_)));
        let events = collected.lock().unwrap();
        assert!(
            !events.is_empty(),
            "events before the abort must still reach the caller"
        );
    }

    #[test]
    fn bytes_after_terminal_are_not_parsed() {
        let mut bytes = script_bytes("happy-text");
        bytes.extend_from_slice(b"data: {not json}\n\n");
        let collected: Arc<Mutex<Vec<StreamEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&collected);
        let disposition = parse_stream_bytes(&bytes, &mut move |event| {
            sink.lock().unwrap().push(event);
            Ok(())
        })
        .unwrap();
        assert_eq!(disposition, TurnDisposition::Completed);
        let events = collected.lock().unwrap();
        assert!(matches!(
            events.first(),
            Some(StreamEvent::Created { response_id }) if response_id == "resp_text_1"
        ));
        assert!(matches!(events.last(), Some(StreamEvent::Completed { .. })));
    }

    #[test]
    fn request_body_matches_contract_shape() {
        let mut request = sample_request();
        let body = build_request_body(&request);
        assert_eq!(body["model"], "gpt-5.5");
        assert_eq!(body["instructions"], "system prompt");
        assert_eq!(body["input"][0]["role"], "user");
        assert_eq!(body["tools"][0]["name"], "read_file");
        assert_eq!(body["store"], false);
        assert_eq!(body["stream"], true);
        assert!(
            body.get("previous_response_id").is_none(),
            "the REST backend must receive stateless full-replay input"
        );
        assert_eq!(
            body["include"],
            serde_json::json!(["reasoning.encrypted_content"])
        );
        assert_eq!(
            body["reasoning"],
            serde_json::json!({"effort": "high", "summary": "auto"})
        );
        for forbidden in ["max_output_tokens", "max_tokens", "metadata", "session_id"] {
            assert!(
                body.get(forbidden).is_none(),
                "{forbidden} must not be sent"
            );
        }

        request.include_reasoning = false;
        let body = build_request_body(&request);
        assert!(body.get("previous_response_id").is_none());
        assert!(body.get("include").is_none());

        request.include_reasoning = true;
        let body = build_request_body(&request);
        assert_eq!(
            body["include"],
            serde_json::json!(["reasoning.encrypted_content"])
        );
    }

    #[test]
    fn all_supported_reasoning_efforts_map_exactly_to_the_responses_contract() {
        for (effort, expected) in [
            (ReasoningEffort::None, "none"),
            (ReasoningEffort::Low, "low"),
            (ReasoningEffort::Medium, "medium"),
            (ReasoningEffort::High, "high"),
            (ReasoningEffort::XHigh, "xhigh"),
        ] {
            let mut request = sample_request();
            request.reasoning_effort = Some(effort);
            assert_eq!(
                build_request_body(&request)["reasoning"],
                serde_json::json!({"effort": expected, "summary": "auto"})
            );
        }
        let mut request = sample_request();
        request.reasoning_effort = None;
        assert!(build_request_body(&request).get("reasoning").is_none());
    }

    #[test]
    fn malformed_json_and_required_fields_are_protocol_errors() {
        for frame in [
            "data: {not-json}\n\n",
            "data: {\"type\":\"response.created\",\"response\":{}}\n\n",
            "data: {\"type\":\"response.output_text.delta\"}\n\n",
            "data: {\"response\":{\"id\":\"r\"}}\n\n",
        ] {
            let error = parse_stream_bytes(frame.as_bytes(), &mut |_| Ok(())).unwrap_err();
            let rendered = error.to_string();
            assert!(matches!(error, BackendError::Protocol(_)));
            assert!(!rendered.contains("not-json"));
        }
    }

    #[test]
    fn inconsistent_created_and_completed_ids_are_rejected() {
        let bytes = b"data: {\"type\":\"response.created\",\"response\":{\"id\":\"r1\"}}\n\ndata: {\"type\":\"response.completed\",\"response\":{\"id\":\"r2\"}}\n\n";
        let error = parse_stream_bytes(bytes, &mut |_| Ok(())).unwrap_err();
        assert!(matches!(error, BackendError::Protocol(_)));
    }

    #[test]
    fn event_delivery_failure_stops_parsing_immediately() {
        let bytes = script_bytes("happy-text");
        let delivered = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let count = Arc::clone(&delivered);
        let error = parse_stream_bytes(&bytes, &mut move |_| {
            count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Err(BackendError::EventDelivery)
        })
        .unwrap_err();
        assert!(matches!(error, BackendError::EventDelivery));
        assert_eq!(delivered.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    type Captured = Arc<Mutex<Option<CapturedRequest>>>;

    #[derive(Clone)]
    struct CapturedRequest {
        method: String,
        path: String,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
    }

    struct MockServer {
        base_url: String,
        captured: Captured,
    }

    impl MockServer {
        fn start(
            responder: impl Fn(&CapturedRequest) -> (u16, Vec<(String, String)>, Vec<u8>)
                + Send
                + 'static,
        ) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            let captured: Captured = Arc::new(Mutex::new(None));
            let shared = Arc::clone(&captured);
            std::thread::spawn(move || {
                if let Ok((mut stream, _)) = listener.accept() {
                    if let Some(request) = read_request(&mut stream) {
                        *shared.lock().unwrap() = Some(request);
                        let (status, headers, body) =
                            responder(shared.lock().unwrap().as_ref().unwrap());
                        let head = format!(
                            "HTTP/1.1 {} {}\r\nContent-Type: text/event-stream\r\n{}\r\n",
                            status,
                            if status == 200 { "OK" } else { "Error" },
                            headers
                                .iter()
                                .map(|(name, value)| format!("{name}: {value}\r\n"))
                                .collect::<String>()
                        );
                        let _ = stream.write_all(head.as_bytes());
                        let _ = stream.write_all(&body);
                        let _ = stream.flush();
                    }
                }
            });
            Self {
                base_url: format!("http://127.0.0.1:{port}/backend-api/codex/responses"),
                captured,
            }
        }
    }

    fn header<'a>(request: &'a CapturedRequest, name: &str) -> &'a str {
        request
            .headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
            .unwrap_or("")
    }

    fn read_request(stream: &mut TcpStream) -> Option<CapturedRequest> {
        let mut raw = Vec::new();
        let mut chunk = [0u8; 1024];
        loop {
            if raw.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
            let read = stream.read(&mut chunk).ok()?;
            if read == 0 {
                return None;
            }
            raw.extend_from_slice(&chunk[..read]);
        }
        let split = raw.windows(4).position(|window| window == b"\r\n\r\n")?;
        let head = String::from_utf8_lossy(&raw[..split]).to_string();
        let mut body = raw[split + 4..].to_vec();
        let mut lines = head.split("\r\n");
        let request_line = lines.next()?;
        let mut parts = request_line.split_whitespace();
        let method = parts.next()?.to_string();
        let path = parts.next()?.to_string();
        let mut headers = Vec::new();
        let mut content_length = 0usize;
        for line in lines {
            let Some((name, value)) = line.split_once(':') else {
                continue;
            };
            let name = name.trim().to_string();
            let value = value.trim().to_string();
            if name.eq_ignore_ascii_case("content-length") {
                content_length = value.parse().unwrap_or(0);
            }
            headers.push((name, value));
        }
        while body.len() < content_length {
            let read = stream.read(&mut chunk).ok()?;
            if read == 0 {
                break;
            }
            body.extend_from_slice(&chunk[..read]);
        }
        Some(CapturedRequest {
            method,
            path,
            headers,
            body,
        })
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn reqwest_client_sends_contract_headers_body_and_streams_events() {
        let stream_body = script_bytes("happy-text");
        let server = MockServer::start(move |_| (200, Vec::new(), stream_body.clone()));
        let backend = ReqwestBackend::new(&server.base_url).unwrap();

        let collected: Arc<Mutex<Vec<StreamEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&collected);
        let auth = sample_auth();
        let request_body = sample_request();
        let disposition = backend
            .create_response(&auth, &request_body, &|| false, &mut move |event| {
                sink.lock().unwrap().push(event);
                Ok(())
            })
            .await
            .unwrap();

        assert_eq!(disposition, TurnDisposition::Completed);
        let events = collected.lock().unwrap();
        assert!(matches!(events.last(), Some(StreamEvent::Completed { .. })));

        let request = server.captured.lock().unwrap().clone().unwrap();
        assert_eq!(request.method, "POST");
        assert_eq!(request.path, "/backend-api/codex/responses");
        assert_eq!(
            header(&request, "authorization"),
            "Bearer super-secret-access-token-value"
        );
        assert_eq!(header(&request, "chatgpt-account-id"), "acc-77");
        assert_eq!(header(&request, "originator"), ORIGINATOR);
        assert_eq!(header(&request, "session-id"), "ses-1234");
        assert_eq!(header(&request, COMPUTE_RESIDENCY_HEADER), "us");
        assert!(
            header(&request, "user-agent").starts_with("quantix/"),
            "user-agent {:?}",
            header(&request, "user-agent")
        );
        assert!(header(&request, "content-type").starts_with("application/json"));
        let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
        assert_eq!(body["model"], "gpt-5.5");
        assert_eq!(body["stream"], true);
        assert_eq!(body["store"], false);
        assert_eq!(
            body["reasoning"],
            serde_json::json!({"effort": "high", "summary": "auto"})
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn reqwest_client_omits_unconstrained_compute_residency() {
        for compute_residency in [None, Some("no_constraint".to_string())] {
            let stream_body = script_bytes("happy-text");
            let server = MockServer::start(move |_| (200, Vec::new(), stream_body.clone()));
            let backend = ReqwestBackend::new(&server.base_url).unwrap();
            let mut auth = sample_auth();
            auth.compute_residency = compute_residency;
            let request_body = sample_request();

            backend
                .create_response(&auth, &request_body, &|| false, &mut |_| Ok(()))
                .await
                .unwrap();

            let request = server.captured.lock().unwrap().clone().unwrap();
            assert_eq!(header(&request, COMPUTE_RESIDENCY_HEADER), "");
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn reqwest_client_rejects_malformed_sse_without_echoing_payload() {
        let server = MockServer::start(|_| {
            (
                200,
                Vec::new(),
                b"data: {private-malformed-payload}\n\n".to_vec(),
            )
        });
        let backend = ReqwestBackend::new(&server.base_url).unwrap();
        let auth = sample_auth();
        let request = sample_request();
        let error = backend
            .create_response(&auth, &request, &|| false, &mut |_| Ok(()))
            .await
            .unwrap_err();

        assert!(matches!(error, BackendError::Protocol(_)));
        assert!(!error.to_string().contains("private-malformed-payload"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn http_401_maps_to_authentication_required_without_leaking_token() {
        let server = MockServer::start(|_| (401, Vec::new(), Vec::new()));
        let backend = ReqwestBackend::new(&server.base_url).unwrap();

        let mut noop = |_: StreamEvent| Ok(());
        let auth = sample_auth();
        let request = sample_request();
        let error = backend
            .create_response(&auth, &request, &|| false, &mut noop)
            .await
            .unwrap_err();

        assert!(matches!(error, BackendError::AuthenticationRequired));
        let rendered = format!("{error:?} / {error}");
        assert!(
            !rendered.contains(SECRET_TOKEN),
            "error rendering must never contain the access token"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn http_429_maps_to_rate_limited_honoring_retry_after() {
        for (headers, expected) in [
            (vec![("retry-after-ms", "1500")], Some(1500)),
            (vec![("retry-after", "2")], Some(2000)),
            (Vec::new(), None),
        ] {
            let header_pairs: Vec<(String, String)> = headers
                .iter()
                .map(|(name, value)| (name.to_string(), value.to_string()))
                .collect();
            let server = MockServer::start(move |_| (429, header_pairs.clone(), Vec::new()));
            let backend = ReqwestBackend::new(&server.base_url).unwrap();

            let mut noop = |_: StreamEvent| Ok(());
            let auth = sample_auth();
            let request = sample_request();
            let error = backend
                .create_response(&auth, &request, &|| false, &mut noop)
                .await
                .unwrap_err();

            match error {
                BackendError::RateLimited { retry_after_ms } => {
                    assert_eq!(retry_after_ms, expected)
                }
                other => panic!("expected rate limit error, got {other:?}"),
            }
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn other_non_success_status_maps_to_protocol_error() {
        let server = MockServer::start(|_| (500, Vec::new(), Vec::new()));
        let backend = ReqwestBackend::new(&server.base_url).unwrap();

        let mut noop = |_: StreamEvent| Ok(());
        let auth = sample_auth();
        let request = sample_request();
        let error = backend
            .create_response(&auth, &request, &|| false, &mut noop)
            .await
            .unwrap_err();

        assert!(matches!(error, BackendError::Protocol(_)));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn socket_close_after_created_is_reported_as_transport_error() {
        // The mock writes a partial SSE payload then drops the connection.
        let partial: Vec<u8> = script_bytes("midstream-abort");
        let server = MockServer::start(move |_| (200, Vec::new(), partial.clone()));
        let backend = ReqwestBackend::new(&server.base_url).unwrap();

        let mut noop = |_: StreamEvent| Ok(());
        let auth = sample_auth();
        let request = sample_request();
        let error = backend
            .create_response(&auth, &request, &|| false, &mut noop)
            .await
            .unwrap_err();

        assert!(matches!(error, BackendError::Transport(_)));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn connection_refusal_maps_to_transport_error() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let backend = ReqwestBackend::new(&format!(
            "http://127.0.0.1:{port}/backend-api/codex/responses"
        ))
        .unwrap();

        let mut noop = |_: StreamEvent| Ok(());
        let auth = sample_auth();
        let request = sample_request();
        let error = backend
            .create_response(&auth, &request, &|| false, &mut noop)
            .await
            .unwrap_err();

        assert!(matches!(error, BackendError::Transport(_)));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn stalled_sse_stream_is_interrupted_promptly_after_provider_acceptance() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let _ = read_request(&mut stream).unwrap();
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\r\ndata: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_stall\"}}\n\n",
                )
                .unwrap();
            stream.flush().unwrap();
            std::thread::sleep(Duration::from_millis(500));
        });
        let backend = ReqwestBackend::new(&format!(
            "http://127.0.0.1:{port}/backend-api/codex/responses"
        ))
        .unwrap();
        let cancelled = Arc::new(AtomicBool::new(false));
        let cancel_for_task = Arc::clone(&cancelled);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(75)).await;
            cancel_for_task.store(true, Ordering::SeqCst);
        });
        let saw_created = Arc::new(AtomicBool::new(false));
        let created_for_sink = Arc::clone(&saw_created);
        let auth = sample_auth();
        let request = sample_request();
        let started = std::time::Instant::now();
        let error = backend
            .create_response(
                &auth,
                &request,
                &|| cancelled.load(Ordering::SeqCst),
                &mut move |event| {
                    if matches!(event, StreamEvent::Created { .. }) {
                        created_for_sink.store(true, Ordering::SeqCst);
                    }
                    Ok(())
                },
            )
            .await
            .unwrap_err();

        assert!(matches!(error, BackendError::Interrupted));
        assert!(saw_created.load(Ordering::SeqCst));
        assert!(started.elapsed() < Duration::from_millis(300));
        server.join().unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn stalled_sse_stream_observes_deadline_callback_promptly() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let _ = read_request(&mut stream).unwrap();
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\r\n")
                .unwrap();
            stream.flush().unwrap();
            std::thread::sleep(Duration::from_millis(350));
        });
        let backend = ReqwestBackend::new(&format!(
            "http://127.0.0.1:{port}/backend-api/codex/responses"
        ))
        .unwrap();
        let auth = sample_auth();
        let request = sample_request();
        let started = std::time::Instant::now();
        let deadline = started + Duration::from_millis(75);
        let error = backend
            .create_response(
                &auth,
                &request,
                &|| std::time::Instant::now() >= deadline,
                &mut |_| Ok(()),
            )
            .await
            .unwrap_err();

        assert!(matches!(error, BackendError::Interrupted));
        assert!(started.elapsed() < Duration::from_millis(300));
        server.join().unwrap();
    }
}
