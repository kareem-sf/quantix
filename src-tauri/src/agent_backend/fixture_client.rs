use super::client::{
    parse_stream_bytes, status_error, BackendError, BackendRequest, ChatGptBackend, StreamEvent,
    TurnDisposition,
};
use crate::chatgpt_oauth::StoredConnection;

const SCRIPT_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/backend_scripts");

pub(crate) struct FixtureBackend {
    script_name: String,
}

impl FixtureBackend {
    pub(crate) fn new(script_name: &str) -> Self {
        Self {
            script_name: script_name.to_string(),
        }
    }

    fn load_script(&self) -> Result<Vec<u8>, BackendError> {
        std::fs::read(format!("{SCRIPT_DIR}/{}.sse", self.script_name)).map_err(|_| {
            BackendError::Protocol(format!("fixture script '{}' unavailable", self.script_name))
        })
    }
}

fn status_directive(bytes: &[u8]) -> Option<u16> {
    let head = bytes.split(|byte| *byte == b'\n').next()?;
    let text = std::str::from_utf8(head).ok()?.trim();
    let code = text.strip_prefix("STATUS ")?;
    code.trim().parse().ok()
}

impl ChatGptBackend for FixtureBackend {
    fn create_response(
        &self,
        _auth: &StoredConnection,
        _req: &BackendRequest,
        on_event: &mut dyn FnMut(StreamEvent),
    ) -> Result<TurnDisposition, BackendError> {
        let bytes = self.load_script()?;
        if let Some(status) = status_directive(&bytes) {
            return Err(status_error(status, None));
        }
        parse_stream_bytes(&bytes, on_event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_backend::client::UsageSnapshot;

    fn sample_request() -> BackendRequest {
        BackendRequest {
            model: "gpt-5.5".to_string(),
            instructions: "system prompt".to_string(),
            input_items: Vec::new(),
            tools: Vec::new(),
            previous_response_id: None,
            store: false,
            include_reasoning: true,
            session_id: "ses-fixture".to_string(),
        }
    }

    fn sample_auth() -> StoredConnection {
        StoredConnection {
            access_token: "unused".to_string(),
            refresh_token: "unused".to_string(),
            id_token: "unused".to_string(),
            expires_at_ms: 0,
            account_id: "acc-1".to_string(),
            plan_type: None,
        }
    }

    #[test]
    fn replays_happy_text_script_to_completed() {
        let backend = FixtureBackend::new("happy-text");
        let mut events = Vec::new();
        let disposition = backend
            .create_response(&sample_auth(), &sample_request(), &mut |event| {
                events.push(event)
            })
            .unwrap();

        assert_eq!(disposition, TurnDisposition::Completed);
        assert_eq!(
            events.last(),
            Some(&StreamEvent::Completed {
                response_id: "resp_text_1".to_string(),
                usage: UsageSnapshot {
                    input_tokens: 120,
                    cached_input_tokens: Some(32),
                    output_tokens: 45,
                    reasoning_output_tokens: Some(16),
                    total_tokens: 165,
                },
            })
        );
    }

    #[test]
    fn replays_tool_roundtrip_script() {
        let backend = FixtureBackend::new("tool-roundtrip");
        let mut events = Vec::new();
        let disposition = backend
            .create_response(&sample_auth(), &sample_request(), &mut |event| {
                events.push(event)
            })
            .unwrap();

        assert_eq!(disposition, TurnDisposition::Completed);
        assert!(matches!(
            events.iter().find(|event| matches!(event, StreamEvent::FunctionCallDone { .. })),
            Some(StreamEvent::FunctionCallDone { call_id, name, arguments })
                if call_id == "call_abc" && name == "read_file" && arguments == r#"{"path":"spec.md"}"#
        ));
    }

    #[test]
    fn midstream_abort_script_maps_to_transport_error() {
        let backend = FixtureBackend::new("midstream-abort");
        let mut delivered = 0;
        let error = backend
            .create_response(&sample_auth(), &sample_request(), &mut |_event| {
                delivered += 1;
            })
            .unwrap_err();

        assert!(matches!(error, BackendError::Transport(_)));
        assert!(delivered > 0, "pre-abort events must be delivered");
    }

    #[test]
    fn status_directive_maps_unauthorized_401_to_authentication_required() {
        let backend = FixtureBackend::new("unauthorized-401");
        let mut noop = |_: StreamEvent| {};
        let error = backend
            .create_response(&sample_auth(), &sample_request(), &mut noop)
            .unwrap_err();

        assert!(matches!(error, BackendError::AuthenticationRequired));
    }

    #[test]
    fn failed_response_script_yields_failed_disposition() {
        let backend = FixtureBackend::new("failed-response");
        let mut events = Vec::new();
        let disposition = backend
            .create_response(&sample_auth(), &sample_request(), &mut |event| {
                events.push(event)
            })
            .unwrap();

        assert_eq!(disposition, TurnDisposition::Failed);
        assert!(matches!(events.last(), Some(StreamEvent::Errored(_))));
    }

    #[test]
    fn missing_script_file_maps_to_protocol_error() {
        let backend = FixtureBackend::new("does-not-exist");
        let mut noop = |_: StreamEvent| {};
        let error = backend
            .create_response(&sample_auth(), &sample_request(), &mut noop)
            .unwrap_err();

        assert!(matches!(error, BackendError::Protocol(_)));
    }
}
