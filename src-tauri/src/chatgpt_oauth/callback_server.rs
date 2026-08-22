use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use super::authorize::build_authorize_url;
use super::crypto::{generate_pkce, generate_state};
use super::tokens::{TokenClient, TokenError, TokenErrorKind};
use super::{IssuedTokens, PkceCodes};

const LOGIN_TIMEOUT: Duration = Duration::from_secs(300);
const ACCEPT_POLL: Duration = Duration::from_millis(25);
const STREAM_READ_TIMEOUT: Duration = Duration::from_secs(10);
const SEND_GUARD: Duration = Duration::from_secs(5);
const MAX_HEAD_BYTES: usize = 16 * 1024;
const CALLBACK_PATH: &str = "/auth/callback";
const CANCEL_PATH: &str = "/cancel";
const LOOPBACK_BIND_HOST: &str = "127.0.0.1";
const LOOPBACK_REDIRECT_HOST: &str = "localhost";

#[derive(Debug)]
pub(crate) enum CallbackOutcome {
    Authorized(IssuedTokens),
    Cancelled,
    Failed(CallbackFailure),
}

#[derive(Debug)]
pub(crate) enum CallbackFailure {
    PortBlocked,
    ProviderDenied,
    StateMismatch,
    ExchangeRejected,
    ExchangeUnreachable,
    Persistence,
    Startup,
    Timeout,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AuthorizationCompletion {
    Connected,
    Cancelled,
    Failed,
}

struct PendingLogin {
    pkce: PkceCodes,
    state: String,
    redirect_uri: String,
}

const CSRF_REASON: &str = "the sign-in response could not be verified";
const MISSING_CODE_REASON: &str = "the provider did not send an authorization code";
const UNREACHABLE_REASON: &str = "Quantix could not reach the sign-in service";
const DENIED_REASON: &str = "ChatGPT did not approve the sign-in request";
const PERSISTENCE_REASON: &str = "Quantix could not save the ChatGPT connection";
const CANCELLED_REASON: &str = "The sign-in request was cancelled";
const CANCEL_ACK_TEXT: &str = "Login cancelled. You can close this window.";
const NOT_FOUND_TEXT: &str = "Not found";

// Quantix palette values from src/quantixDesignSystem.css (light theme):
// ink #3f464d, slate #6f7782, canvas #f4fafe, card #ffffff, border #d8e8f0,
// focus blue #397c9d, danger #b13e3e.
const PAGE_STYLE: &str = "body{margin:0;min-height:100vh;display:flex;align-items:center;justify-content:center;background:#f4fafe;color:#3f464d;font-family:'Segoe UI Variable','Segoe UI',system-ui,sans-serif}.card{background:#ffffff;border:1px solid #d8e8f0;border-radius:12px;box-shadow:0 12px 32px rgb(63 70 77 / 10%);padding:48px 56px;text-align:center;max-width:420px}.card h1{margin:18px 0 6px;font-size:21px}.card p{margin:4px 0;font-size:14px;color:#6f7782}";

const BRAND_MARK_SVG: &str = "<svg width='44' height='44' viewBox='0 0 48 48' role='img' aria-label='Quantix'>\
     <path fill='#397c9d' d='M7 7h5v5H7Zm10 0h5v5h-5Zm14 9h5v5h-5ZM7 26h5v5H7Zm24 11h5v5h-5Zm-14 4h5v5h-5Z'/>\
     <path fill='#9aa3ab' d='m21.2 20.6 2.6-3.2 17.6 14.1-2.6 3.2Z'/></svg>";

fn page_shell(title: &str, body: &str) -> String {
    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
         <title>{title}</title><style>{PAGE_STYLE}</style></head>\
         <body><main class=\"card\">{BRAND_MARK_SVG}{body}</main></body></html>"
    )
}

fn success_page() -> String {
    page_shell(
        "Quantix — ChatGPT connected",
        "<h1>ChatGPT connected to Quantix</h1>\
         <p>You can close this window.</p>",
    )
}

fn error_page(reason: &str) -> String {
    page_shell(
        "Quantix — Sign-in problem",
        format!(
            "<h1>Sign-in could not be completed</h1>\
             <p style=\"color:#b13e3e\">{}</p>\
             <p>You can close this window.</p>",
            html_escape(reason)
        )
        .as_str(),
    )
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

pub(crate) fn run_login(
    port_candidates: &[u16],
    open_browser: impl FnOnce(&str),
    issuer: &str,
    complete_authorization: impl FnOnce(&IssuedTokens) -> AuthorizationCompletion + Send + 'static,
) -> CallbackOutcome {
    let Some((listener, port)) = bind_first_free(port_candidates) else {
        return CallbackOutcome::Failed(CallbackFailure::PortBlocked);
    };
    let Ok(pkce) = generate_pkce() else {
        return CallbackOutcome::Failed(CallbackFailure::Startup);
    };
    let Ok(state) = generate_state() else {
        return CallbackOutcome::Failed(CallbackFailure::Startup);
    };
    let Ok(client) = TokenClient::new(issuer) else {
        return CallbackOutcome::Failed(CallbackFailure::Startup);
    };
    let redirect_uri = format!("http://{LOOPBACK_REDIRECT_HOST}:{port}{CALLBACK_PATH}");
    open_browser(&build_authorize_url(&redirect_uri, &pkce, &state));
    serve_until_terminal(
        listener,
        PendingLogin {
            pkce,
            state,
            redirect_uri,
        },
        client,
        LOGIN_TIMEOUT,
        complete_authorization,
    )
}

fn bind_first_free(candidates: &[u16]) -> Option<(TcpListener, u16)> {
    candidates.iter().find_map(|candidate| {
        let listener = TcpListener::bind((LOOPBACK_BIND_HOST, *candidate)).ok()?;
        let port = listener.local_addr().ok()?.port();
        Some((listener, port))
    })
}

fn serve_until_terminal<F>(
    listener: TcpListener,
    pending: PendingLogin,
    client: TokenClient,
    budget: Duration,
    complete_authorization: F,
) -> CallbackOutcome
where
    F: FnOnce(&IssuedTokens) -> AuthorizationCompletion + Send + 'static,
{
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        if let Some(outcome) =
            accept_until_terminal(listener, &pending, &client, budget, complete_authorization)
        {
            let _ = tx.send(outcome);
        }
    });
    match rx.recv_timeout(budget + SEND_GUARD) {
        Ok(outcome) => outcome,
        Err(RecvTimeoutError::Timeout) => CallbackOutcome::Failed(CallbackFailure::Timeout),
        Err(RecvTimeoutError::Disconnected) => CallbackOutcome::Failed(CallbackFailure::Startup),
    }
}

fn accept_until_terminal<F>(
    listener: TcpListener,
    pending: &PendingLogin,
    client: &TokenClient,
    budget: Duration,
    complete_authorization: F,
) -> Option<CallbackOutcome>
where
    F: FnOnce(&IssuedTokens) -> AuthorizationCompletion,
{
    listener.set_nonblocking(true).ok()?;
    let deadline = Instant::now() + budget;
    let mut complete_authorization = Some(complete_authorization);
    loop {
        if Instant::now() >= deadline {
            return Some(CallbackOutcome::Failed(CallbackFailure::Timeout));
        }
        match listener.accept() {
            Ok((stream, _)) => {
                if let Some(outcome) =
                    handle_connection(stream, pending, client, &mut complete_authorization)
                {
                    return Some(outcome);
                }
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => std::thread::sleep(ACCEPT_POLL),
            Err(_) => {}
        }
    }
}

fn handle_connection<F>(
    mut stream: TcpStream,
    pending: &PendingLogin,
    client: &TokenClient,
    complete_authorization: &mut Option<F>,
) -> Option<CallbackOutcome>
where
    F: FnOnce(&IssuedTokens) -> AuthorizationCompletion,
{
    let _ = stream.set_read_timeout(Some(STREAM_READ_TIMEOUT));
    let head = read_request_head(&mut stream)?;
    let request_line = head.lines().next()?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next()?;
    let target = parts.next()?;
    if method != "GET" {
        write_response(
            &mut stream,
            "405 Method Not Allowed",
            "text/plain; charset=utf-8",
            "Method not allowed\n",
        );
        return None;
    }
    let (path, query) = match target.split_once('?') {
        Some((path, query)) => (path, query),
        None => (target, ""),
    };
    match path {
        CALLBACK_PATH => Some(handle_callback(
            query,
            pending,
            client,
            complete_authorization,
            &mut stream,
        )),
        CANCEL_PATH => {
            write_response(
                &mut stream,
                "200 OK",
                "text/plain; charset=utf-8",
                CANCEL_ACK_TEXT,
            );
            Some(CallbackOutcome::Cancelled)
        }
        _ => {
            write_response(
                &mut stream,
                "404 Not Found",
                "text/plain; charset=utf-8",
                NOT_FOUND_TEXT,
            );
            None
        }
    }
}

fn handle_callback<F>(
    query: &str,
    pending: &PendingLogin,
    client: &TokenClient,
    complete_authorization: &mut Option<F>,
    stream: &mut TcpStream,
) -> CallbackOutcome
where
    F: FnOnce(&IssuedTokens) -> AuthorizationCompletion,
{
    let params = parse_query(query);

    if param(&params, "error").is_some() {
        write_response(
            stream,
            "400 Bad Request",
            "text/html; charset=utf-8",
            &error_page(DENIED_REASON),
        );
        return CallbackOutcome::Failed(CallbackFailure::ProviderDenied);
    }

    if param(&params, "state") != Some(pending.state.as_str()) {
        write_response(
            stream,
            "400 Bad Request",
            "text/html; charset=utf-8",
            &error_page(CSRF_REASON),
        );
        return CallbackOutcome::Failed(CallbackFailure::StateMismatch);
    }

    let Some(code) = param(&params, "code").map(str::to_string) else {
        write_response(
            stream,
            "400 Bad Request",
            "text/html; charset=utf-8",
            &error_page(MISSING_CODE_REASON),
        );
        return CallbackOutcome::Failed(CallbackFailure::ProviderDenied);
    };

    match client.exchange_code(&code, &pending.redirect_uri, &pending.pkce) {
        Ok(tokens) => {
            let completion = complete_authorization
                .take()
                .map(|complete| complete(&tokens))
                .unwrap_or(AuthorizationCompletion::Failed);
            match completion {
                AuthorizationCompletion::Connected => {
                    write_response(
                        stream,
                        "200 OK",
                        "text/html; charset=utf-8",
                        &success_page(),
                    );
                    CallbackOutcome::Authorized(tokens)
                }
                AuthorizationCompletion::Cancelled => {
                    write_response(
                        stream,
                        "400 Bad Request",
                        "text/html; charset=utf-8",
                        &error_page(CANCELLED_REASON),
                    );
                    CallbackOutcome::Cancelled
                }
                AuthorizationCompletion::Failed => {
                    write_response(
                        stream,
                        "500 Internal Server Error",
                        "text/html; charset=utf-8",
                        &error_page(PERSISTENCE_REASON),
                    );
                    CallbackOutcome::Failed(CallbackFailure::Persistence)
                }
            }
        }
        Err(TokenError::Provider { kind, .. }) => {
            write_response(
                stream,
                "502 Bad Gateway",
                "text/html; charset=utf-8",
                &error_page(rejection_reason(kind)),
            );
            CallbackOutcome::Failed(CallbackFailure::ExchangeRejected)
        }
        Err(TokenError::Transport(_)) => {
            write_response(
                stream,
                "502 Bad Gateway",
                "text/html; charset=utf-8",
                &error_page(UNREACHABLE_REASON),
            );
            CallbackOutcome::Failed(CallbackFailure::ExchangeUnreachable)
        }
    }
}

fn rejection_reason(kind: TokenErrorKind) -> &'static str {
    match kind {
        TokenErrorKind::InvalidGrant => "the authorization code was rejected as invalid or expired",
        TokenErrorKind::InvalidClient => {
            "the application configuration was rejected by the provider"
        }
        TokenErrorKind::Other => "the token exchange did not succeed",
    }
}

fn read_request_head(stream: &mut TcpStream) -> Option<String> {
    let mut raw = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        if let Some(split) = raw.windows(4).position(|window| window == b"\r\n\r\n") {
            return Some(String::from_utf8_lossy(&raw[..split]).into_owned());
        }
        if raw.len() > MAX_HEAD_BYTES {
            return None;
        }
        let read = stream.read(&mut chunk).ok()?;
        if read == 0 {
            return None;
        }
        raw.extend_from_slice(&chunk[..read]);
    }
}

fn parse_query(query: &str) -> Vec<(String, String)> {
    query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| match pair.split_once('=') {
            Some((name, value)) => (decode_component(name), decode_component(value)),
            None => (decode_component(pair), String::new()),
        })
        .collect()
}

fn param<'a>(params: &'a [(String, String)], name: &str) -> Option<&'a str> {
    params
        .iter()
        .find(|pair| pair.0 == name)
        .map(|pair| pair.1.as_str())
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

fn write_response(stream: &mut TcpStream, status_line: &str, content_type: &str, body: &str) {
    let response = format!(
        "HTTP/1.1 {status_line}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::mpsc;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use super::*;

    const MOCK_TOKEN_BODY: &str =
        r#"{"access_token":"at-1","refresh_token":"rt-1","id_token":"idt-1","expires_in":3600}"#;
    const WAIT: Duration = Duration::from_secs(10);

    type CapturedForms = Arc<Mutex<Vec<Vec<(String, String)>>>>;

    struct LoginHarness {
        urls: mpsc::Receiver<String>,
        outcomes: mpsc::Receiver<CallbackOutcome>,
    }

    fn spawn_login(port_candidates: &[u16], issuer_base: &str) -> LoginHarness {
        spawn_login_with_completion(port_candidates, issuer_base, |_| {
            AuthorizationCompletion::Connected
        })
    }

    fn spawn_login_with_completion(
        port_candidates: &[u16],
        issuer_base: &str,
        complete_authorization: impl FnOnce(&IssuedTokens) -> AuthorizationCompletion + Send + 'static,
    ) -> LoginHarness {
        let (url_tx, url_rx) = mpsc::channel();
        let (outcome_tx, outcome_rx) = mpsc::channel();
        let candidates = port_candidates.to_vec();
        let base = issuer_base.to_string();
        std::thread::spawn(move || {
            let outcome = run_login(
                &candidates,
                |url| {
                    let _ = url_tx.send(url.to_string());
                },
                &base,
                complete_authorization,
            );
            let _ = outcome_tx.send(outcome);
        });
        LoginHarness {
            urls: url_rx,
            outcomes: outcome_rx,
        }
    }

    fn next_authorize_url(harness: &LoginHarness) -> (u16, String) {
        let url = harness
            .urls
            .recv_timeout(WAIT)
            .expect("browser should receive the authorize URL");
        let redirect_uri = decode_component(&authorize_param(&url, "redirect_uri"));
        let port_part = redirect_uri
            .strip_prefix("http://localhost:")
            .expect("loopback redirect URI")
            .trim_end_matches("/auth/callback");
        let port: u16 = port_part.parse().expect("numeric callback port");
        (port, url)
    }

    fn authorize_param(url: &str, name: &str) -> String {
        let prefix = format!("{name}=");
        url.split('&')
            .find_map(|pair| pair.strip_prefix(prefix.as_str()).map(str::to_string))
            .unwrap_or_else(|| panic!("authorize URL is missing {name}: {url}"))
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

    fn start_mock_issuer() -> (String, CapturedForms) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let captured: CapturedForms = Arc::new(Mutex::new(Vec::new()));
        let shared = Arc::clone(&captured);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let form = read_post_form(&mut stream);
                shared.lock().unwrap().push(form);
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{MOCK_TOKEN_BODY}",
                    MOCK_TOKEN_BODY.len()
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        (format!("http://127.0.0.1:{port}"), captured)
    }

    fn read_post_form(stream: &mut TcpStream) -> Vec<(String, String)> {
        let mut raw = Vec::new();
        let mut chunk = [0u8; 1024];
        loop {
            if raw.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
            let Ok(read) = stream.read(&mut chunk) else {
                break;
            };
            if read == 0 {
                break;
            }
            raw.extend_from_slice(&chunk[..read]);
        }
        let Some(split) = raw.windows(4).position(|window| window == b"\r\n\r\n") else {
            return Vec::new();
        };
        let head = String::from_utf8_lossy(&raw[..split]).to_string();
        let mut length = 0usize;
        for line in head.split("\r\n").skip(1) {
            if let Some((name, value)) = line.split_once(':') {
                if name.trim().eq_ignore_ascii_case("content-length") {
                    length = value.trim().parse().unwrap_or(0);
                }
            }
        }
        let mut body = raw[split + 4..].to_vec();
        while body.len() < length {
            let Ok(read) = stream.read(&mut chunk) else {
                break;
            };
            if read == 0 {
                break;
            }
            body.extend_from_slice(&chunk[..read]);
        }
        String::from_utf8_lossy(&body)
            .split('&')
            .filter(|pair| !pair.is_empty())
            .map(|pair| match pair.split_once('=') {
                Some((name, value)) => (decode_component(name), decode_component(value)),
                None => (decode_component(pair), String::new()),
            })
            .collect()
    }

    fn form_value<'a>(form: &'a [(String, String)], name: &str) -> &'a str {
        form.iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
            .unwrap_or_default()
    }

    fn http_get(port: u16, target: &str) -> String {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let request =
            format!("GET {target} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
        stream.write_all(request.as_bytes()).unwrap();
        let mut raw = String::new();
        stream.read_to_string(&mut raw).unwrap();
        raw
    }

    #[test]
    fn happy_path_opens_browser_exchanges_code_and_shows_success_page() {
        let (issuer_base, forms) = start_mock_issuer();
        let harness = spawn_login(&[0], &issuer_base);
        let (port, url) = next_authorize_url(&harness);

        assert!(
            url.starts_with("https://auth.openai.com/oauth/authorize?"),
            "authorize URL: {url}"
        );
        assert!(url.contains("originator=quantix"));
        let state = authorize_param(&url, "state");

        let response = http_get(
            port,
            &format!("/auth/callback?code=coded-abc&state={state}"),
        );
        assert!(
            response.contains("ChatGPT connected to Quantix"),
            "page: {response}"
        );
        assert!(response.contains("You can close this window."));

        match harness
            .outcomes
            .recv_timeout(WAIT)
            .expect("terminal outcome")
        {
            CallbackOutcome::Authorized(tokens) => {
                assert_eq!(tokens.access_token, "at-1");
                assert_eq!(tokens.refresh_token, "rt-1");
                assert_eq!(tokens.id_token, "idt-1");
                assert_eq!(tokens.expires_in_secs, 3600);
            }
            other => panic!("expected Authorized, got {other:?}"),
        }

        let forms = forms.lock().unwrap();
        assert_eq!(forms.len(), 1);
        let form = &forms[0];
        assert_eq!(form_value(form, "grant_type"), "authorization_code");
        assert_eq!(form_value(form, "code"), "coded-abc");
        assert_eq!(
            form_value(form, "redirect_uri"),
            format!("http://localhost:{port}/auth/callback")
        );
        assert_eq!(
            form_value(form, "client_id"),
            "app_EMoamEEZ73f0CkXaXp7hrann"
        );
        assert_eq!(form_value(form, "code_verifier").len(), 43);
    }

    #[test]
    fn persistence_failure_is_reported_before_any_success_page() {
        let (issuer_base, _forms) = start_mock_issuer();
        let harness =
            spawn_login_with_completion(&[0], &issuer_base, |_| AuthorizationCompletion::Failed);
        let (port, url) = next_authorize_url(&harness);
        let state = authorize_param(&url, "state");

        let response = http_get(
            port,
            &format!("/auth/callback?code=coded-abc&state={state}"),
        );

        assert!(response.starts_with("HTTP/1.1 500"));
        assert!(response.contains(PERSISTENCE_REASON));
        assert!(!response.contains("ChatGPT connected to Quantix"));
        assert!(matches!(
            harness.outcomes.recv_timeout(WAIT).unwrap(),
            CallbackOutcome::Failed(CallbackFailure::Persistence)
        ));
    }

    #[test]
    fn unknown_path_404s_then_cancel_terminates_with_plain_ack() {
        let harness = spawn_login(&[0], "http://127.0.0.1:9");
        let (port, _url) = next_authorize_url(&harness);

        let missing = http_get(port, "/nope");
        assert!(missing.starts_with("HTTP/1.1 404"), "status: {missing}");
        assert!(missing.contains("Not found"));

        let ack = http_get(port, "/cancel");
        assert!(ack.contains("cancelled"), "ack: {ack}");
        assert!(ack.contains("text/plain"));

        match harness
            .outcomes
            .recv_timeout(WAIT)
            .expect("terminal outcome")
        {
            CallbackOutcome::Cancelled => {}
            other => panic!("expected Cancelled, got {other:?}"),
        }
    }

    #[test]
    fn state_mismatch_is_rejected_without_exchange() {
        let (issuer_base, forms) = start_mock_issuer();
        let harness = spawn_login(&[0], &issuer_base);
        let (port, _url) = next_authorize_url(&harness);

        let forged = http_get(port, "/auth/callback?code=zz&state=forged-state");
        assert!(forged.contains("could not be verified"), "page: {forged}");
        assert!(!forged.contains("ChatGPT connected to Quantix"));

        match harness
            .outcomes
            .recv_timeout(WAIT)
            .expect("terminal outcome")
        {
            CallbackOutcome::Failed(CallbackFailure::StateMismatch) => {}
            other => panic!("expected StateMismatch, got {other:?}"),
        }

        let harness = spawn_login(&[0], &issuer_base);
        let (port, _url) = next_authorize_url(&harness);
        let absent_state = http_get(port, "/auth/callback?code=zz");
        assert!(absent_state.contains("could not be verified"));

        match harness
            .outcomes
            .recv_timeout(WAIT)
            .expect("terminal outcome")
        {
            CallbackOutcome::Failed(CallbackFailure::StateMismatch) => {}
            other => panic!("expected StateMismatch, got {other:?}"),
        }

        assert!(forms.lock().unwrap().is_empty());
    }

    #[test]
    fn provider_error_param_maps_to_denied_failure_and_error_page() {
        let (issuer_base, forms) = start_mock_issuer();
        let harness = spawn_login(&[0], &issuer_base);
        let (port, _url) = next_authorize_url(&harness);

        let page = http_get(
            port,
            "/auth/callback?error=access_denied&error_description=User%20denied%20the%20request",
        );
        assert!(page.contains(DENIED_REASON), "page: {page}");
        assert!(!page.contains("User denied the request"));
        assert!(!page.contains("ChatGPT connected to Quantix"));

        match harness
            .outcomes
            .recv_timeout(WAIT)
            .expect("terminal outcome")
        {
            CallbackOutcome::Failed(CallbackFailure::ProviderDenied) => {}
            other => panic!("expected ProviderDenied, got {other:?}"),
        }
        assert!(forms.lock().unwrap().is_empty());
    }

    #[test]
    fn blocked_both_candidates_reports_only_the_blocked_outcome() {
        let Ok(gate_1455) = TcpListener::bind(("127.0.0.1", 1455)) else {
            println!("skipping: port 1455 occupied");
            return;
        };
        let Ok(gate_1457) = TcpListener::bind(("127.0.0.1", 1457)) else {
            println!("skipping: port 1457 occupied");
            return;
        };

        let harness = spawn_login(&[1455, 1457], "http://127.0.0.1:9");
        match harness
            .outcomes
            .recv_timeout(WAIT)
            .expect("terminal outcome")
        {
            CallbackOutcome::Failed(CallbackFailure::PortBlocked) => {}
            other => panic!("expected PortBlocked, got {other:?}"),
        }

        drop(gate_1455);
        drop(gate_1457);
    }

    #[test]
    fn serve_budget_expiry_yields_timeout() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let pending = PendingLogin {
            pkce: generate_pkce().unwrap(),
            state: generate_state().unwrap(),
            redirect_uri: "http://127.0.0.1:1455/auth/callback".to_string(),
        };
        let client = TokenClient::new("http://127.0.0.1:9").unwrap();

        let started = std::time::Instant::now();
        let outcome = serve_until_terminal(
            listener,
            pending,
            client,
            Duration::from_millis(300),
            |_| AuthorizationCompletion::Connected,
        );

        assert!(started.elapsed() < Duration::from_secs(5));
        assert!(matches!(
            outcome,
            CallbackOutcome::Failed(CallbackFailure::Timeout)
        ));
    }
}
