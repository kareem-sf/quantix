use std::collections::HashMap;
use std::io::Read;

use crate::chatgpt_oauth::StoredConnection;

pub(crate) const BACKEND_URL: &str = "https://chatgpt.com/backend-api/codex/responses";
const ORIGINATOR: &str = "quantix";

#[derive(Debug)]
pub(crate) struct BackendRequest {
    pub model: String,
    pub instructions: String,
    pub input_items: Vec<serde_json::Value>,
    pub tools: Vec<serde_json::Value>,
    pub previous_response_id: Option<String>,
    pub store: bool,
    pub include_reasoning: bool,
    pub session_id: String,
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
        }
    }
}

impl std::error::Error for BackendError {}

pub(crate) trait ChatGptBackend: Send + Sync {
    fn create_response(
        &self,
        auth: &StoredConnection,
        req: &BackendRequest,
        on_event: &mut dyn FnMut(StreamEvent),
    ) -> Result<TurnDisposition, BackendError>;
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
    if let Some(previous) = &req.previous_response_id {
        body.insert("previous_response_id".into(), previous.clone().into());
    }
    body.insert("store".into(), req.store.into());
    body.insert("stream".into(), true.into());
    if req.include_reasoning {
        body.insert(
            "include".into(),
            serde_json::json!(["reasoning.encrypted_content"]),
        );
    }
    serde_json::Value::Object(body)
}

enum Terminal {
    Completed,
    Failed,
}

fn str_field(value: &serde_json::Value, field: &str) -> String {
    value
        .get(field)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_string()
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
    on_event: &'a mut dyn FnMut(StreamEvent),
    buffer: Vec<u8>,
    data_lines: Vec<String>,
    pending_calls: HashMap<String, (String, String)>,
    terminal: Option<Terminal>,
    done_seen: bool,
}

impl<'a> SseParser<'a> {
    fn new(on_event: &'a mut dyn FnMut(StreamEvent)) -> Self {
        Self {
            on_event,
            buffer: Vec::new(),
            data_lines: Vec::new(),
            pending_calls: HashMap::new(),
            terminal: None,
            done_seen: false,
        }
    }

    fn feed(&mut self, chunk: &[u8]) {
        if self.terminal.is_some() || self.done_seen {
            return;
        }
        self.buffer.extend_from_slice(chunk);
        while let Some(newline) = self.buffer.iter().position(|byte| *byte == b'\n') {
            let mut line: Vec<u8> = self.buffer.drain(..=newline).collect();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            self.process_line(&line);
            if self.terminal.is_some() || self.done_seen {
                self.buffer.clear();
                return;
            }
        }
    }

    fn finish(&mut self) {
        if self.terminal.is_none() && !self.done_seen && !self.buffer.is_empty() {
            let line = std::mem::take(&mut self.buffer);
            self.process_line(&line);
        } else {
            self.buffer.clear();
        }
        self.dispatch_frame();
    }

    fn process_line(&mut self, line: &[u8]) {
        if line.is_empty() {
            self.dispatch_frame();
            return;
        }
        if line[0] == b':' {
            return;
        }
        let text = String::from_utf8_lossy(line);
        if let Some(rest) = text.strip_prefix("data:") {
            let value = rest.strip_prefix(' ').unwrap_or(rest);
            self.data_lines.push(value.to_string());
        }
    }

    fn dispatch_frame(&mut self) {
        if self.data_lines.is_empty() || self.terminal.is_some() || self.done_seen {
            self.data_lines.clear();
            return;
        }
        let data = self.data_lines.join("\n");
        self.data_lines.clear();
        if data == "[DONE]" {
            self.done_seen = true;
            return;
        }
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&data) else {
            return;
        };
        let Some(event_type) = value.get("type").and_then(serde_json::Value::as_str) else {
            return;
        };
        match event_type {
            "response.output_item.added" => {
                if let Some(item) = value.get("item") {
                    if item.get("type").and_then(serde_json::Value::as_str) == Some("function_call")
                    {
                        self.pending_calls.insert(
                            str_field(item, "id"),
                            (str_field(item, "call_id"), str_field(item, "name")),
                        );
                    }
                }
                (self.on_event)(StreamEvent::ItemAdded(value));
            }
            "response.output_text.delta" => {
                (self.on_event)(StreamEvent::TextDelta(str_field(&value, "delta")));
            }
            "response.function_call_arguments.delta" => {
                let item_id = str_field(&value, "item_id");
                let (call_id, name) = self
                    .pending_calls
                    .get(&item_id)
                    .cloned()
                    .unwrap_or_else(|| (item_id, String::new()));
                (self.on_event)(StreamEvent::FunctionCallDelta {
                    call_id,
                    name,
                    args_delta: str_field(&value, "delta"),
                });
            }
            "response.output_item.done" => {
                let Some(item) = value.get("item") else {
                    return;
                };
                (self.on_event)(StreamEvent::ItemDone(value.clone()));
                if item.get("type").and_then(serde_json::Value::as_str) != Some("function_call") {
                    return;
                }
                let fallback = self.pending_calls.remove(&str_field(item, "id"));
                let (fallback_call_id, fallback_name) = fallback.unwrap_or_default();
                (self.on_event)(StreamEvent::FunctionCallDone {
                    call_id: opt_str_field(item, "call_id").unwrap_or(fallback_call_id),
                    name: opt_str_field(item, "name").unwrap_or(fallback_name),
                    arguments: str_field(item, "arguments"),
                });
            }
            "response.completed" => {
                let empty = serde_json::Value::Null;
                let response = value.get("response").unwrap_or(&empty);
                (self.on_event)(StreamEvent::Completed {
                    response_id: str_field(response, "id"),
                    usage: parse_usage(response),
                });
                self.terminal = Some(Terminal::Completed);
            }
            "response.failed" => {
                (self.on_event)(StreamEvent::Errored(RedactedFailure {
                    code: FailureCode::ResponseFailed,
                    detail: "provider reported response.failed".to_string(),
                }));
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
                }));
                self.terminal = Some(Terminal::Failed);
            }
            "error" => {
                (self.on_event)(StreamEvent::Errored(RedactedFailure {
                    code: FailureCode::InBandError,
                    detail: "provider sent an in-band error event".to_string(),
                }));
                self.terminal = Some(Terminal::Failed);
            }
            _ => {}
        }
    }
}

pub(crate) fn parse_stream_bytes(
    bytes: &[u8],
    on_event: &mut dyn FnMut(StreamEvent),
) -> Result<TurnDisposition, BackendError> {
    let mut parser = SseParser::new(on_event);
    parser.feed(bytes);
    parser.finish();
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

fn pump_reader<R: Read>(
    mut reader: R,
    on_event: &mut dyn FnMut(StreamEvent),
) -> Result<TurnDisposition, BackendError> {
    let mut parser = SseParser::new(on_event);
    let mut buffer = [0u8; 8192];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => parser.feed(&buffer[..read]),
            Err(error) => {
                return Err(BackendError::Transport(format!(
                    "stream read failed: {error}"
                )))
            }
        }
        if parser.terminal.is_some() || parser.done_seen {
            break;
        }
    }
    parser.finish();
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
    http: reqwest::blocking::Client,
}

impl ReqwestBackend {
    pub(crate) fn new(endpoint: &str) -> Result<Self, reqwest::Error> {
        Ok(Self {
            endpoint: endpoint.to_string(),
            http: reqwest::blocking::Client::builder()
                .connect_timeout(std::time::Duration::from_secs(30))
                .build()?,
        })
    }
}

impl ChatGptBackend for ReqwestBackend {
    fn create_response(
        &self,
        auth: &StoredConnection,
        req: &BackendRequest,
        on_event: &mut dyn FnMut(StreamEvent),
    ) -> Result<TurnDisposition, BackendError> {
        let response = self
            .http
            .post(&self.endpoint)
            .bearer_auth(&auth.access_token)
            .header("ChatGPT-Account-Id", &auth.account_id)
            .header("originator", ORIGINATOR)
            .header("session-id", &req.session_id)
            .header("user-agent", user_agent())
            .json(&build_request_body(req))
            .send()
            .map_err(|error| BackendError::Transport(error.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            return Err(status_error(
                status.as_u16(),
                retry_after_ms(response.headers()),
            ));
        }
        pump_reader(response, on_event)
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
            previous_response_id: None,
            store: false,
            include_reasoning: true,
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
        }
    }

    #[test]
    fn happy_text_script_parses_incrementally_to_expected_events() {
        let bytes = script_bytes("happy-text");
        let collected: Arc<Mutex<Vec<StreamEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&collected);
        let mut sink_fn = move |event| sink.lock().unwrap().push(event);
        let mut parser = SseParser::new(&mut sink_fn);
        for chunk in bytes.chunks(7) {
            parser.feed(chunk);
        }
        parser.finish();

        let events = collected.lock().unwrap();
        assert_eq!(events.len(), 5, "unexpected event count: {events:?}");
        assert!(matches!(
            &events[0],
            StreamEvent::ItemAdded(value)
                if value["item"]["type"] == "message" && value["output_index"] == 0
        ));
        match (&events[1], &events[2]) {
            (StreamEvent::TextDelta(a), StreamEvent::TextDelta(b)) => {
                assert_eq!(format!("{a}{b}"), "Hello from the fixture.");
            }
            other => panic!("expected two text deltas, got {other:?}"),
        }
        assert!(matches!(
            &events[3],
            StreamEvent::ItemDone(value)
                if value["item"]["type"] == "message" && value["output_index"] == 0
        ));
        match &events[4] {
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
        let disposition =
            parse_stream_bytes(&bytes, &mut move |event| sink.lock().unwrap().push(event)).unwrap();

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
        let disposition =
            parse_stream_bytes(&bytes, &mut move |event| sink.lock().unwrap().push(event)).unwrap();
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
            let disposition =
                parse_stream_bytes(&bytes, &mut move |event| sink.lock().unwrap().push(event))
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
        let disposition =
            parse_stream_bytes(&bytes, &mut move |event| sink.lock().unwrap().push(event)).unwrap();

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
        let error = parse_stream_bytes(&bytes, &mut move |event| sink.lock().unwrap().push(event))
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
        let disposition =
            parse_stream_bytes(&bytes, &mut move |event| sink.lock().unwrap().push(event)).unwrap();
        assert_eq!(disposition, TurnDisposition::Completed);
        let events = collected.lock().unwrap();
        assert!(matches!(events.last(), Some(StreamEvent::Completed { .. })));
    }

    #[test]
    fn request_body_matches_contract_shape() {
        let mut request = sample_request();
        request.previous_response_id = Some("resp_prev".to_string());
        let body = build_request_body(&request);
        assert_eq!(body["model"], "gpt-5.5");
        assert_eq!(body["instructions"], "system prompt");
        assert_eq!(body["input"][0]["role"], "user");
        assert_eq!(body["tools"][0]["name"], "read_file");
        assert_eq!(body["store"], false);
        assert_eq!(body["stream"], true);
        assert_eq!(body["previous_response_id"], "resp_prev");
        assert_eq!(
            body["include"],
            serde_json::json!(["reasoning.encrypted_content"])
        );
        for forbidden in ["max_output_tokens", "max_tokens", "metadata", "session_id"] {
            assert!(
                body.get(forbidden).is_none(),
                "{forbidden} must not be sent"
            );
        }

        request.previous_response_id = None;
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

    #[test]
    fn reqwest_client_sends_contract_headers_body_and_streams_events() {
        let stream_body = script_bytes("happy-text");
        let server = MockServer::start(move |_| (200, Vec::new(), stream_body.clone()));
        let backend = ReqwestBackend::new(&server.base_url).unwrap();

        let collected: Arc<Mutex<Vec<StreamEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&collected);
        let disposition = backend
            .create_response(&sample_auth(), &sample_request(), &mut move |event| {
                sink.lock().unwrap().push(event)
            })
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
    }

    #[test]
    fn http_401_maps_to_authentication_required_without_leaking_token() {
        let server = MockServer::start(|_| (401, Vec::new(), Vec::new()));
        let backend = ReqwestBackend::new(&server.base_url).unwrap();

        let mut noop = |_: StreamEvent| {};
        let error = backend
            .create_response(&sample_auth(), &sample_request(), &mut noop)
            .unwrap_err();

        assert!(matches!(error, BackendError::AuthenticationRequired));
        let rendered = format!("{error:?} / {error}");
        assert!(
            !rendered.contains(SECRET_TOKEN),
            "error rendering must never contain the access token"
        );
    }

    #[test]
    fn http_429_maps_to_rate_limited_honoring_retry_after() {
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

            let mut noop = |_: StreamEvent| {};
            let error = backend
                .create_response(&sample_auth(), &sample_request(), &mut noop)
                .unwrap_err();

            match error {
                BackendError::RateLimited { retry_after_ms } => {
                    assert_eq!(retry_after_ms, expected)
                }
                other => panic!("expected rate limit error, got {other:?}"),
            }
        }
    }

    #[test]
    fn other_non_success_status_maps_to_protocol_error() {
        let server = MockServer::start(|_| (500, Vec::new(), Vec::new()));
        let backend = ReqwestBackend::new(&server.base_url).unwrap();

        let mut noop = |_: StreamEvent| {};
        let error = backend
            .create_response(&sample_auth(), &sample_request(), &mut noop)
            .unwrap_err();

        assert!(matches!(error, BackendError::Protocol(_)));
    }

    #[test]
    fn socket_close_before_terminal_maps_to_transport_error() {
        // The mock writes a partial SSE payload then drops the connection.
        let partial: Vec<u8> = script_bytes("midstream-abort");
        let server = MockServer::start(move |_| (200, Vec::new(), partial.clone()));
        let backend = ReqwestBackend::new(&server.base_url).unwrap();

        let mut noop = |_: StreamEvent| {};
        let error = backend
            .create_response(&sample_auth(), &sample_request(), &mut noop)
            .unwrap_err();

        assert!(matches!(error, BackendError::Transport(_)));
    }

    #[test]
    fn connection_refusal_maps_to_transport_error() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let backend = ReqwestBackend::new(&format!(
            "http://127.0.0.1:{port}/backend-api/codex/responses"
        ))
        .unwrap();

        let mut noop = |_: StreamEvent| {};
        let error = backend
            .create_response(&sample_auth(), &sample_request(), &mut noop)
            .unwrap_err();

        assert!(matches!(error, BackendError::Transport(_)));
    }
}
