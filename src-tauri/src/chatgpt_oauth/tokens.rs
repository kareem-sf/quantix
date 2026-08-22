use super::PkceCodes;

const TOKEN_ENDPOINT_SUFFIX: &str = "/oauth/token";
const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";

#[derive(Debug)]
pub(crate) struct IssuedTokens {
    pub access_token: String,
    pub refresh_token: String,
    pub id_token: String,
    pub expires_in_secs: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TokenErrorKind {
    InvalidGrant,
    InvalidClient,
    Other,
}

#[derive(Debug)]
pub(crate) enum TokenError {
    Provider { status: u16, kind: TokenErrorKind },
    Transport(String),
}

impl std::fmt::Display for TokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenError::Provider { status, kind } => {
                write!(
                    f,
                    "token provider rejected request (HTTP {status}, {kind:?})"
                )
            }
            TokenError::Transport(detail) => write!(f, "token transport error: {detail}"),
        }
    }
}

impl std::error::Error for TokenError {}

pub(crate) struct TokenClient {
    issuer_base: String,
    http: reqwest::blocking::Client,
}

impl TokenClient {
    pub(crate) fn new(issuer_base: &str) -> Result<Self, reqwest::Error> {
        Ok(Self {
            issuer_base: issuer_base.trim_end_matches('/').to_string(),
            http: reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()?,
        })
    }

    pub(crate) fn exchange_code(
        &self,
        code: &str,
        redirect_uri: &str,
        pkce: &PkceCodes,
    ) -> Result<IssuedTokens, TokenError> {
        self.post_form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("client_id", CLIENT_ID),
            ("code_verifier", pkce.verifier.as_str()),
        ])
    }

    pub(crate) fn refresh(&self, refresh_token: &str) -> Result<IssuedTokens, TokenError> {
        self.post_form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", CLIENT_ID),
        ])
    }

    fn post_form(&self, form: &[(&str, &str)]) -> Result<IssuedTokens, TokenError> {
        let url = format!("{}{TOKEN_ENDPOINT_SUFFIX}", self.issuer_base);
        let response = self
            .http
            .post(url)
            .form(form)
            .send()
            .map_err(|error| TokenError::Transport(error.to_string()))?;
        let status = response.status();
        let body = response
            .text()
            .map_err(|error| TokenError::Transport(error.to_string()))?;
        if !status.is_success() {
            return Err(TokenError::Provider {
                status: status.as_u16(),
                kind: rejection_kind(&body),
            });
        }
        parse_issued_tokens(&body)
            .ok_or_else(|| TokenError::Transport("malformed token payload".to_string()))
    }
}

fn rejection_kind(body: &str) -> TokenErrorKind {
    let parsed: Option<serde_json::Value> = serde_json::from_str(body).ok();
    match parsed
        .as_ref()
        .and_then(|value| value.get("error"))
        .and_then(serde_json::Value::as_str)
    {
        Some("invalid_grant") => TokenErrorKind::InvalidGrant,
        Some("invalid_client") => TokenErrorKind::InvalidClient,
        _ => TokenErrorKind::Other,
    }
}

fn parse_issued_tokens(body: &str) -> Option<IssuedTokens> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    Some(IssuedTokens {
        access_token: value.get("access_token")?.as_str()?.to_string(),
        refresh_token: value.get("refresh_token")?.as_str()?.to_string(),
        id_token: value.get("id_token")?.as_str()?.to_string(),
        expires_in_secs: value.get("expires_in")?.as_u64()?,
    })
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::sync::{Arc, Mutex};

    use super::{PkceCodes, TokenClient, TokenError, TokenErrorKind};

    type Captured = Arc<Mutex<Vec<MockRequest>>>;

    #[derive(Clone)]
    struct MockRequest {
        method: String,
        path: String,
        content_type: String,
        form: Vec<(String, String)>,
    }

    const SUCCESS_BODY: &str =
        r#"{"access_token":"at-1","refresh_token":"rt-1","id_token":"idt-1","expires_in":3600}"#;

    fn pkce() -> PkceCodes {
        PkceCodes {
            verifier: "test-verifier_v1".to_string(),
            challenge: "unused-challenge".to_string(),
        }
    }

    fn start_issuer(
        responder: impl Fn(MockRequest) -> (u16, &'static str) + Send + 'static,
    ) -> (String, Captured) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let captured: Captured = Arc::new(Mutex::new(Vec::new()));
        let shared = Arc::clone(&captured);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                if let Some(request) = read_request(&mut stream) {
                    shared.lock().unwrap().push(request.clone());
                    let (status, body) = responder(request);
                    write_response(&mut stream, status, body);
                }
            }
        });
        (format!("http://127.0.0.1:{port}"), captured)
    }

    fn read_request(stream: &mut TcpStream) -> Option<MockRequest> {
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
        let mut content_type = String::new();
        let mut content_length = 0usize;
        for line in lines {
            let Some((name, value)) = line.split_once(':') else {
                continue;
            };
            match name.trim().to_ascii_lowercase().as_str() {
                "content-type" => content_type = value.trim().to_string(),
                "content-length" => content_length = value.trim().parse().unwrap_or(0),
                _ => {}
            }
        }
        while body.len() < content_length {
            let read = stream.read(&mut chunk).ok()?;
            if read == 0 {
                break;
            }
            body.extend_from_slice(&chunk[..read]);
        }
        let body_text = String::from_utf8_lossy(&body).to_string();
        Some(MockRequest {
            method,
            path,
            content_type,
            form: parse_form(&body_text),
        })
    }

    fn parse_form(body: &str) -> Vec<(String, String)> {
        body.split('&')
            .filter(|pair| !pair.is_empty())
            .map(|pair| match pair.split_once('=') {
                Some((name, value)) => (decode_component(name), decode_component(value)),
                None => (decode_component(pair), String::new()),
            })
            .collect()
    }

    fn decode_component(value: &str) -> String {
        let bytes = value.as_bytes();
        let mut out = Vec::with_capacity(bytes.len());
        let mut index = 0;
        while index < bytes.len() {
            match bytes[index] {
                b'%' if index + 2 < bytes.len() => {
                    let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap_or("zz");
                    out.push(u8::from_str_radix(hex, 16).unwrap_or(b'?'));
                    index += 3;
                }
                b'+' => {
                    out.push(b' ');
                    index += 1;
                }
                byte => {
                    out.push(byte);
                    index += 1;
                }
            }
        }
        String::from_utf8(out).unwrap_or_default()
    }

    fn write_response(stream: &mut TcpStream, status: u16, body: &str) {
        let response = format!(
            "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.flush();
    }

    fn form_value<'a>(form: &'a [(String, String)], name: &str) -> &'a str {
        form.iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
            .unwrap_or_default()
    }

    #[test]
    fn exchange_code_posts_form_and_parses_tokens() {
        let (base, captured) = start_issuer(move |_| (200, SUCCESS_BODY));
        let client = TokenClient::new(&base).unwrap();

        let tokens = client
            .exchange_code("code-1", "http://127.0.0.1:1455/auth/callback", &pkce())
            .unwrap();

        assert_eq!(tokens.access_token, "at-1");
        assert_eq!(tokens.refresh_token, "rt-1");
        assert_eq!(tokens.id_token, "idt-1");
        assert_eq!(tokens.expires_in_secs, 3600);

        let requests = captured.lock().unwrap();
        assert_eq!(requests.len(), 1);
        let request = &requests[0];
        assert_eq!(request.method, "POST");
        assert_eq!(request.path, "/oauth/token");
        assert!(request
            .content_type
            .starts_with("application/x-www-form-urlencoded"));
        assert_eq!(request.form.len(), 5);
        assert_eq!(
            form_value(&request.form, "grant_type"),
            "authorization_code"
        );
        assert_eq!(form_value(&request.form, "code"), "code-1");
        assert_eq!(
            form_value(&request.form, "redirect_uri"),
            "http://127.0.0.1:1455/auth/callback"
        );
        assert_eq!(
            form_value(&request.form, "client_id"),
            "app_EMoamEEZ73f0CkXaXp7hrann"
        );
        assert_eq!(
            form_value(&request.form, "code_verifier"),
            "test-verifier_v1"
        );
    }

    #[test]
    fn refresh_posts_refresh_grant_and_parses_tokens() {
        let (base, captured) = start_issuer(move |_| (200, SUCCESS_BODY));
        let client = TokenClient::new(&base).unwrap();

        let tokens = client.refresh("old-refresh-token").unwrap();

        assert_eq!(tokens.access_token, "at-1");
        assert_eq!(tokens.expires_in_secs, 3600);

        let requests = captured.lock().unwrap();
        assert_eq!(requests.len(), 1);
        let request = &requests[0];
        assert_eq!(request.method, "POST");
        assert_eq!(request.path, "/oauth/token");
        assert!(request
            .content_type
            .starts_with("application/x-www-form-urlencoded"));
        assert_eq!(request.form.len(), 3);
        assert_eq!(form_value(&request.form, "grant_type"), "refresh_token");
        assert_eq!(
            form_value(&request.form, "refresh_token"),
            "old-refresh-token"
        );
        assert_eq!(
            form_value(&request.form, "client_id"),
            "app_EMoamEEZ73f0CkXaXp7hrann"
        );
    }

    #[test]
    fn invalid_grant_maps_to_provider_error() {
        let (base, _captured) = start_issuer(move |_| (400, r#"{"error":"invalid_grant"}"#));
        let client = TokenClient::new(&base).unwrap();

        let error = client.refresh("stale-token").unwrap_err();

        match error {
            TokenError::Provider { status, kind } => {
                assert_eq!(status, 400);
                assert_eq!(kind, TokenErrorKind::InvalidGrant);
            }
            other => panic!("expected provider error, got {other:?}"),
        }
    }

    #[test]
    fn connection_refusal_maps_to_transport_error() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        let client = TokenClient::new(&format!("http://127.0.0.1:{port}")).unwrap();
        let error = client
            .exchange_code("code-1", "http://127.0.0.1:1455/auth/callback", &pkce())
            .unwrap_err();

        assert!(matches!(error, TokenError::Transport(_)));
    }

    #[test]
    fn provider_error_kinds_are_classified_from_error_field() {
        assert_eq!(
            super::rejection_kind(r#"{"error":"invalid_client"}"#),
            TokenErrorKind::InvalidClient
        );
        assert_eq!(
            super::rejection_kind(r#"{"error":"slow_down"}"#),
            TokenErrorKind::Other
        );
        assert_eq!(
            super::rejection_kind("<html>oops</html>"),
            TokenErrorKind::Other
        );
        assert_eq!(super::rejection_kind("{}"), TokenErrorKind::Other);
    }

    #[test]
    fn malformed_success_bodies_do_not_parse() {
        assert!(super::parse_issued_tokens("not-json").is_none());
        assert!(super::parse_issued_tokens(r#"{"access_token":"a"}"#).is_none());
        assert!(super::parse_issued_tokens(
            r#"{"access_token":"a","refresh_token":"b","id_token":"c","expires_in":"x"}"#
        )
        .is_none());
    }
}
