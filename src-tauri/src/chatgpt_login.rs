use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::application_settings::{
    load_application_settings, save_chatgpt_disconnected, ApplicationSettingsView,
};
use crate::chatgpt_oauth::{
    clear_unlocked, extract_identity, load, needs_refresh, run_login, save_unlocked,
    with_connection_mutation, AuthorizationCompletion, DeviceAuthorization, DeviceClient,
    DevicePollOutcome, LoadState, StoredConnection, TokenClient,
};
use crate::host::QuantixHost;
use crate::tender_store::{TenderCommandError, TenderErrorCode};

pub(crate) const PRODUCTION_ISSUER: &str = "https://auth.openai.com";
const LOGIN_PORT_CANDIDATES: &[u16] = &[1455, 1457];
const CANCEL_CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const LOOPBACK_HOST: &str = "127.0.0.1";
const REDIRECT_URI_MARKER: &str = "redirect_uri=http://localhost:";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ChatGptConnectionState {
    Absent,
    Connected,
    Unusable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct ChatGptConnectionStatus {
    pub state: ChatGptConnectionState,
    pub account_id: Option<String>,
    pub plan_type: Option<String>,
    pub expires_at_ms: Option<u64>,
    pub login_phase: ChatGptLoginPhase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum StartChatGptLoginStatus {
    AwaitingBrowser,
    Connected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct StartChatGptLoginResult {
    pub status: StartChatGptLoginStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct StartChatGptDeviceLoginResult {
    pub verification_url: String,
    pub user_code: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct StartChatGptLoginError {
    pub code: TenderErrorCode,
}

impl StartChatGptLoginError {
    pub(crate) fn new(code: TenderErrorCode) -> Self {
        Self { code }
    }
}

impl std::fmt::Display for StartChatGptLoginError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "ChatGPT login failed: {:?}", self.code)
    }
}

impl std::error::Error for StartChatGptLoginError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ChatGptLoginPhase {
    Idle,
    AwaitingBrowser,
    AwaitingDevice,
    Completed,
    Failed,
    Cancelled,
}

struct ActiveLoginFlow {
    cancel: AtomicBool,
    callback_port: Mutex<Option<u16>>,
}

impl ActiveLoginFlow {
    fn new() -> Self {
        Self {
            cancel: AtomicBool::new(false),
            callback_port: Mutex::new(None),
        }
    }

    fn opener<F>(self: Arc<Self>, open_browser: F) -> impl FnOnce(&str)
    where
        F: FnOnce(&str) + Send + 'static,
    {
        move |url| {
            let port = callback_port_from_authorize_url(url);
            *self
                .callback_port
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = port;
            if self.cancel.load(Ordering::Acquire) {
                if let Some(port) = port {
                    request_callback_cancel(port);
                }
                return;
            }
            open_browser(url);
        }
    }

    fn signal_cancel(&self) {
        self.cancel.store(true, Ordering::Release);
        let port = *self
            .callback_port
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(port) = port {
            request_callback_cancel(port);
        }
    }

    fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Acquire)
    }
}

pub(crate) struct ChatGptLoginFlowState {
    phase: ChatGptLoginPhase,
    active: Option<Arc<ActiveLoginFlow>>,
}

impl Default for ChatGptLoginFlowState {
    fn default() -> Self {
        Self {
            phase: ChatGptLoginPhase::Idle,
            active: None,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum LoginOutcome {
    Completed,
    Cancelled,
    Failed,
}

#[cfg(test)]
pub(crate) fn run_chatgpt_login_flow(
    home: &Path,
    port_candidates: &[u16],
    open_browser: impl FnOnce(&str),
    issuer: &str,
) -> LoginOutcome {
    run_chatgpt_login_flow_for_active(home, port_candidates, open_browser, issuer, None)
}

fn run_chatgpt_login_flow_for_active(
    home: &Path,
    port_candidates: &[u16],
    open_browser: impl FnOnce(&str),
    issuer: &str,
    active: Option<Arc<ActiveLoginFlow>>,
) -> LoginOutcome {
    let persistence_home = home.to_path_buf();
    let persistence_active = active.clone();
    match run_login(port_candidates, open_browser, issuer, move |tokens| {
        match persist_authorized_connection(
            &persistence_home,
            tokens,
            persistence_active.as_deref(),
        ) {
            LoginOutcome::Completed => AuthorizationCompletion::Connected,
            LoginOutcome::Cancelled => AuthorizationCompletion::Cancelled,
            LoginOutcome::Failed => AuthorizationCompletion::Failed,
        }
    }) {
        crate::chatgpt_oauth::CallbackOutcome::Authorized(_) => LoginOutcome::Completed,
        crate::chatgpt_oauth::CallbackOutcome::Cancelled => LoginOutcome::Cancelled,
        crate::chatgpt_oauth::CallbackOutcome::Failed(
            crate::chatgpt_oauth::CallbackFailure::PortBlocked
            | crate::chatgpt_oauth::CallbackFailure::ProviderDenied
            | crate::chatgpt_oauth::CallbackFailure::StateMismatch
            | crate::chatgpt_oauth::CallbackFailure::ExchangeRejected
            | crate::chatgpt_oauth::CallbackFailure::ExchangeUnreachable
            | crate::chatgpt_oauth::CallbackFailure::Persistence
            | crate::chatgpt_oauth::CallbackFailure::Startup
            | crate::chatgpt_oauth::CallbackFailure::Timeout,
        ) => LoginOutcome::Failed,
    }
}

fn run_chatgpt_device_login_flow(
    home: &Path,
    issuer: &str,
    client: DeviceClient,
    authorization: DeviceAuthorization,
    active: &Arc<ActiveLoginFlow>,
) -> LoginOutcome {
    let grant = match client.poll(&authorization, || active.is_cancelled()) {
        Ok(DevicePollOutcome::Authorized(grant)) => grant,
        Ok(DevicePollOutcome::Cancelled) => return LoginOutcome::Cancelled,
        Ok(DevicePollOutcome::TimedOut) | Err(_) => return LoginOutcome::Failed,
    };
    if active.is_cancelled() {
        return LoginOutcome::Cancelled;
    }
    let token_client = match TokenClient::new(issuer) {
        Ok(client) => client,
        Err(_) => return LoginOutcome::Failed,
    };
    let tokens =
        match token_client.exchange_device_code(&grant.authorization_code, &grant.code_verifier) {
            Ok(tokens) => tokens,
            Err(_) => return LoginOutcome::Failed,
        };
    persist_authorized_connection(home, &tokens, Some(active))
}

fn persist_authorized_connection(
    home: &Path,
    tokens: &crate::chatgpt_oauth::IssuedTokens,
    active: Option<&ActiveLoginFlow>,
) -> LoginOutcome {
    let Some(identity) = extract_identity(&tokens.id_token) else {
        return LoginOutcome::Failed;
    };
    let connection = StoredConnection::from_issued(tokens, &identity, now_ms());
    with_connection_mutation(|| {
        if active.is_some_and(|flow| flow.cancel.load(Ordering::Acquire)) {
            return LoginOutcome::Cancelled;
        }
        match save_unlocked(home, &connection) {
            Ok(()) => LoginOutcome::Completed,
            Err(_) => LoginOutcome::Failed,
        }
    })
}

fn callback_port_from_authorize_url(url: &str) -> Option<u16> {
    let marker_start = url.find(REDIRECT_URI_MARKER)? + REDIRECT_URI_MARKER.len();
    let remainder = &url[marker_start..];
    let end = remainder.find('/').unwrap_or(remainder.len());
    remainder[..end].parse().ok()
}

fn request_callback_cancel(port: u16) {
    use std::io::Write;
    let address = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    if let Ok(mut stream) = std::net::TcpStream::connect_timeout(&address, CANCEL_CONNECT_TIMEOUT) {
        let request =
            format!("GET /cancel HTTP/1.1\r\nHost: {LOOPBACK_HOST}\r\nConnection: close\r\n\r\n");
        let _ = stream.write_all(request.as_bytes());
        let _ = stream.flush();
    }
}

fn all_login_ports_blocked(candidates: &[u16]) -> bool {
    !candidates.is_empty()
        && candidates
            .iter()
            .all(|candidate| std::net::TcpListener::bind((LOOPBACK_HOST, *candidate)).is_err())
}

fn open_in_system_browser(url: &str) {
    let _ = webbrowser::open(url);
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

pub(crate) fn chatgpt_connection_status(home: &Path) -> ChatGptConnectionStatus {
    chatgpt_connection_status_with_phase(home, ChatGptLoginPhase::Idle)
}

pub(crate) fn chatgpt_connection_status_with_phase(
    home: &Path,
    login_phase: ChatGptLoginPhase,
) -> ChatGptConnectionStatus {
    match load(home) {
        LoadState::Connected(connection) => ChatGptConnectionStatus {
            state: ChatGptConnectionState::Connected,
            account_id: Some(connection.account_id.clone()),
            plan_type: connection.plan_type.clone(),
            expires_at_ms: Some(connection.expires_at_ms),
            login_phase,
        },
        LoadState::Absent => ChatGptConnectionStatus {
            state: ChatGptConnectionState::Absent,
            account_id: None,
            plan_type: None,
            expires_at_ms: None,
            login_phase,
        },
        LoadState::Unusable => ChatGptConnectionStatus {
            state: ChatGptConnectionState::Unusable,
            account_id: None,
            plan_type: None,
            expires_at_ms: None,
            login_phase,
        },
    }
}

impl QuantixHost {
    pub(crate) fn chatgpt_login_phase(&self) -> ChatGptLoginPhase {
        self.chatgpt_login_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .phase
    }

    pub fn start_chatgpt_login(&self) -> Result<StartChatGptLoginResult, StartChatGptLoginError> {
        self.begin_chatgpt_login(
            LOGIN_PORT_CANDIDATES,
            PRODUCTION_ISSUER,
            open_in_system_browser,
        )
    }

    pub(crate) fn begin_chatgpt_login(
        &self,
        port_candidates: &[u16],
        issuer: &str,
        open_browser: impl FnOnce(&str) + Send + 'static,
    ) -> Result<StartChatGptLoginResult, StartChatGptLoginError> {
        let mut state = self
            .chatgpt_login_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.active.is_some() {
            return Err(StartChatGptLoginError::new(
                TenderErrorCode::OauthAlreadyRunning,
            ));
        }
        let application_home = self.application_home();
        if let LoadState::Connected(connection) = load(application_home) {
            if !needs_refresh(&connection, now_ms()) {
                state.phase = ChatGptLoginPhase::Completed;
                return Ok(StartChatGptLoginResult {
                    status: StartChatGptLoginStatus::Connected,
                });
            }
        }
        if all_login_ports_blocked(port_candidates) {
            return Err(StartChatGptLoginError::new(
                TenderErrorCode::OauthPortBlocked,
            ));
        }
        let active = Arc::new(ActiveLoginFlow::new());
        state.phase = ChatGptLoginPhase::AwaitingBrowser;
        state.active = Some(Arc::clone(&active));
        let flow_host = self.clone();
        let flow_home = application_home.to_path_buf();
        let flow_ports = port_candidates.to_vec();
        let flow_issuer = issuer.to_owned();
        let thread_active = Arc::clone(&active);
        let spawned = std::thread::Builder::new()
            .name("chatgpt-login".to_owned())
            .spawn(move || {
                let opener = Arc::clone(&thread_active).opener(open_browser);
                let outcome = run_chatgpt_login_flow_for_active(
                    &flow_home,
                    &flow_ports,
                    opener,
                    &flow_issuer,
                    Some(Arc::clone(&thread_active)),
                );
                flow_host.finish_chatgpt_login_flow(&thread_active, outcome);
            });
        if spawned.is_err() {
            state.active = None;
            state.phase = ChatGptLoginPhase::Idle;
            return Err(StartChatGptLoginError::new(
                TenderErrorCode::StoreUnavailable,
            ));
        }
        Ok(StartChatGptLoginResult {
            status: StartChatGptLoginStatus::AwaitingBrowser,
        })
    }

    pub fn start_chatgpt_device_login(
        &self,
    ) -> Result<StartChatGptDeviceLoginResult, StartChatGptLoginError> {
        self.begin_chatgpt_device_login(PRODUCTION_ISSUER)
    }

    pub(crate) fn begin_chatgpt_device_login(
        &self,
        issuer: &str,
    ) -> Result<StartChatGptDeviceLoginResult, StartChatGptLoginError> {
        let active = Arc::new(ActiveLoginFlow::new());
        {
            let mut state = self
                .chatgpt_login_state()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if state.active.is_some() {
                return Err(StartChatGptLoginError::new(
                    TenderErrorCode::OauthAlreadyRunning,
                ));
            }
            state.phase = ChatGptLoginPhase::AwaitingDevice;
            state.active = Some(Arc::clone(&active));
        }

        let client = match DeviceClient::new(issuer) {
            Ok(client) => client,
            Err(_) => {
                self.fail_chatgpt_login_start(&active);
                return Err(StartChatGptLoginError::new(
                    TenderErrorCode::StoreUnavailable,
                ));
            }
        };
        let authorization = match client.initiate() {
            Ok(authorization) => authorization,
            Err(_) => {
                self.fail_chatgpt_login_start(&active);
                return Err(StartChatGptLoginError::new(
                    TenderErrorCode::StoreUnavailable,
                ));
            }
        };
        if active.is_cancelled() {
            self.fail_chatgpt_login_start(&active);
            return Err(StartChatGptLoginError::new(
                TenderErrorCode::StoreUnavailable,
            ));
        }

        let result = StartChatGptDeviceLoginResult {
            verification_url: authorization.verification_url.clone(),
            user_code: authorization.user_code.clone(),
        };
        let flow_host = self.clone();
        let flow_home = self.application_home().to_path_buf();
        let flow_issuer = issuer.to_owned();
        let thread_active = Arc::clone(&active);
        let spawned = std::thread::Builder::new()
            .name("chatgpt-device-login".to_owned())
            .spawn(move || {
                let outcome = run_chatgpt_device_login_flow(
                    &flow_home,
                    &flow_issuer,
                    client,
                    authorization,
                    &thread_active,
                );
                flow_host.finish_chatgpt_login_flow(&thread_active, outcome);
            });
        if spawned.is_err() {
            self.fail_chatgpt_login_start(&active);
            return Err(StartChatGptLoginError::new(
                TenderErrorCode::StoreUnavailable,
            ));
        }
        Ok(result)
    }

    fn fail_chatgpt_login_start(&self, active: &Arc<ActiveLoginFlow>) {
        let mut state = self
            .chatgpt_login_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state
            .active
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, active))
        {
            state.active = None;
            state.phase = ChatGptLoginPhase::Failed;
        }
    }

    fn finish_chatgpt_login_flow(&self, active: &Arc<ActiveLoginFlow>, outcome: LoginOutcome) {
        let mut state = self
            .chatgpt_login_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !state
            .active
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, active))
        {
            return;
        }
        state.active = None;
        state.phase = match outcome {
            LoginOutcome::Completed => ChatGptLoginPhase::Completed,
            LoginOutcome::Cancelled => ChatGptLoginPhase::Cancelled,
            LoginOutcome::Failed => ChatGptLoginPhase::Failed,
        };
    }

    pub fn cancel_chatgpt_login(&self) {
        let mut state = self
            .chatgpt_login_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        // Signalling cannot interrupt an in-flight callback wait; the running
        // flow stops at the next phase boundary instead. Cancel cannot revoke
        // a completed browser authorization: tokens persisted by a flow that
        // finished stay on disk by design and inspect reflects them; the
        // machine still ends Idle.
        if let Some(active) = state.active.take() {
            active.signal_cancel();
            state.phase = ChatGptLoginPhase::Cancelled;
        } else if state.phase != ChatGptLoginPhase::Cancelled {
            state.phase = ChatGptLoginPhase::Idle;
        }
    }

    pub fn disconnect_chatgpt(&self) -> Result<ApplicationSettingsView, TenderCommandError> {
        let mut state = self
            .chatgpt_login_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(active) = state.active.take() {
            active.signal_cancel();
        }
        state.phase = ChatGptLoginPhase::Idle;
        drop(state);
        with_connection_mutation(|| clear_unlocked(self.application_home()))
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        save_chatgpt_disconnected(self.application_home())?;
        load_application_settings(self.application_home())
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::path::PathBuf;
    use std::sync::mpsc;
    use std::sync::Arc;
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    use super::*;
    use crate::chatgpt_oauth::authorize::build_authorize_url;
    use crate::chatgpt_oauth::crypto::{base64url_encode, generate_pkce, generate_state};
    use crate::chatgpt_oauth::save;

    const WAIT: Duration = Duration::from_secs(10);

    fn temp_home(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "quantix-chatgpt-login-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn jwt_id_token(account_id: &str, plan_type: Option<&str>) -> String {
        let mut auth = serde_json::Map::new();
        auth.insert(
            "chatgpt_account_id".to_string(),
            serde_json::Value::String(account_id.to_string()),
        );
        if let Some(plan_type) = plan_type {
            auth.insert(
                "chatgpt_plan_type".to_string(),
                serde_json::Value::String(plan_type.to_string()),
            );
        }
        let payload = serde_json::json!({ "https://api.openai.com/auth": auth });
        let header =
            crate::chatgpt_oauth::crypto::base64url_encode(br#"{"alg":"RS256","typ":"JWT"}"#);
        let body = base64url_encode(payload.to_string().as_bytes());
        format!("{header}.{body}.c2ln")
    }

    struct MockIssuer {
        base: String,
    }

    fn start_mock_issuer(token_body: String) -> MockIssuer {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut chunk = [0u8; 1024];
                let mut raw = Vec::new();
                loop {
                    match stream.read(&mut chunk) {
                        Ok(0) | Err(_) => break,
                        Ok(read) => {
                            raw.extend_from_slice(&chunk[..read]);
                            if raw.windows(4).any(|window| window == b"\r\n\r\n") {
                                break;
                            }
                        }
                    }
                }
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{token_body}",
                    token_body.len()
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        MockIssuer {
            base: format!("http://127.0.0.1:{port}"),
        }
    }

    fn start_pending_device_issuer() -> MockIssuer {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let responses = [
                (
                    200,
                    r#"{"device_auth_id":"device-flow","user_code":"CODE-FLOW","interval":"1"}"#,
                ),
                (403, r#"{"pending":true}"#),
            ];
            for (status, body) in responses {
                let Ok((mut stream, _)) = listener.accept() else {
                    return;
                };
                let mut chunk = [0u8; 1024];
                let mut raw = Vec::new();
                while !raw.windows(4).any(|window| window == b"\r\n\r\n") {
                    let Ok(read) = stream.read(&mut chunk) else {
                        return;
                    };
                    if read == 0 {
                        return;
                    }
                    raw.extend_from_slice(&chunk[..read]);
                }
                let response = format!(
                    "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        MockIssuer {
            base: format!("http://127.0.0.1:{port}"),
        }
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

    fn authorize_state(url: &str) -> String {
        url.split('&')
            .find_map(|pair| pair.strip_prefix("state=").map(str::to_string))
            .expect("authorize URL carries state")
    }

    fn spawn_callback_driver(urls: mpsc::Receiver<String>, target: String) {
        std::thread::spawn(move || {
            let url = urls.recv_timeout(WAIT).expect("browser receives URL");
            let port = callback_port_from_authorize_url(&url).expect("loopback callback port");
            let response = http_get(port, &target.replace("{state}", &authorize_state(&url)));
            assert!(
                response.starts_with("HTTP/1.1"),
                "callback response: {response}"
            );
        });
    }

    fn now_ms() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }

    fn wait_until(mut condition: impl FnMut() -> bool) -> bool {
        let deadline = Instant::now() + WAIT;
        while Instant::now() < deadline {
            if condition() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        condition()
    }

    #[test]
    fn oauth_error_codes_serialize_as_snake_case() {
        assert_eq!(
            serde_json::to_string(&TenderErrorCode::OauthPortBlocked).unwrap(),
            r#""oauth_port_blocked""#
        );
        assert_eq!(
            serde_json::to_string(&TenderErrorCode::OauthAlreadyRunning).unwrap(),
            r#""oauth_already_running""#
        );
    }

    #[test]
    fn core_completes_and_persists_the_connection() {
        let home = temp_home("core-happy");
        let id_token = jwt_id_token("acc-core", Some("plus"));
        let issuer = start_mock_issuer(format!(
            r#"{{"access_token":"at-1","refresh_token":"rt-1","id_token":"{id_token}","expires_in":3600}}"#
        ));
        let (url_tx, url_rx) = mpsc::channel();
        spawn_callback_driver(
            url_rx,
            "/auth/callback?code=coded-abc&state={state}".to_string(),
        );
        let started = now_ms();

        let outcome = run_chatgpt_login_flow(
            &home,
            &[0],
            |url| {
                let _ = url_tx.send(url.to_string());
            },
            &issuer.base,
        );

        assert!(matches!(outcome, LoginOutcome::Completed), "{outcome:?}");
        match load(&home) {
            LoadState::Connected(connection) => {
                assert_eq!(connection.account_id, "acc-core");
                assert_eq!(connection.plan_type.as_deref(), Some("plus"));
                assert!(connection.expires_at_ms >= started + 3_500_000);
                assert!(!needs_refresh(&connection, started));
            }
            other => panic!("expected connected store, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn core_reports_cancellation_and_leaves_the_store_absent() {
        let home = temp_home("core-cancelled");
        let issuer = start_mock_issuer("{}".to_string());
        let (url_tx, url_rx) = mpsc::channel();
        spawn_callback_driver(url_rx, "/cancel".to_string());

        let outcome = run_chatgpt_login_flow(
            &home,
            &[0],
            |url| {
                let _ = url_tx.send(url.to_string());
            },
            &issuer.base,
        );

        assert!(matches!(outcome, LoginOutcome::Cancelled), "{outcome:?}");
        assert!(matches!(load(&home), LoadState::Absent));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn core_maps_provider_denial_without_touching_the_store() {
        let home = temp_home("core-denied");
        let issuer = start_mock_issuer("{}".to_string());
        let (url_tx, url_rx) = mpsc::channel();
        spawn_callback_driver(
            url_rx,
            "/auth/callback?error=access_denied&error_description=User%20said%20no&state={state}"
                .to_string(),
        );

        let outcome = run_chatgpt_login_flow(
            &home,
            &[0],
            |url| {
                let _ = url_tx.send(url.to_string());
            },
            &issuer.base,
        );

        match outcome {
            LoginOutcome::Failed => {}
            other => panic!("expected provider denial failure, got {other:?}"),
        }
        assert!(matches!(load(&home), LoadState::Absent));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn core_reports_blocked_ports_when_every_candidate_is_busy() {
        let home = temp_home("core-blocked");
        let gate_a = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let gate_b = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let candidates = [
            gate_a.local_addr().unwrap().port(),
            gate_b.local_addr().unwrap().port(),
        ];

        let outcome = run_chatgpt_login_flow(
            &home,
            &candidates,
            |_url| {
                panic!("browser must not open when no port is available");
            },
            "http://127.0.0.1:9",
        );

        assert_eq!(outcome, LoginOutcome::Failed);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn core_fails_without_persisting_when_identity_is_missing() {
        let home = temp_home("core-identity");
        let issuer = start_mock_issuer(
            r#"{"access_token":"at-1","refresh_token":"rt-1","id_token":"not-a-jwt","expires_in":3600}"#
                .to_string(),
        );
        let (url_tx, url_rx) = mpsc::channel();
        spawn_callback_driver(
            url_rx,
            "/auth/callback?code=coded-abc&state={state}".to_string(),
        );

        let outcome = run_chatgpt_login_flow(
            &home,
            &[0],
            |url| {
                let _ = url_tx.send(url.to_string());
            },
            &issuer.base,
        );

        assert_eq!(outcome, LoginOutcome::Failed);
        assert!(matches!(load(&home), LoadState::Absent));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn revoked_flow_cannot_persist_after_disconnect_has_claimed_the_mutation_boundary() {
        let home = temp_home("revoked-persist");
        let active = Arc::new(ActiveLoginFlow::new());
        let tokens = crate::chatgpt_oauth::IssuedTokens {
            access_token: "at-race".to_owned(),
            refresh_token: "rt-race".to_owned(),
            id_token: jwt_id_token("acc-race", Some("plus")),
            expires_in_secs: 3_600,
        };
        let (started_tx, started_rx) = mpsc::channel();

        let flow = with_connection_mutation(|| {
            let flow_active = Arc::clone(&active);
            let home = home.clone();
            let flow = std::thread::spawn(move || {
                started_tx.send(()).unwrap();
                persist_authorized_connection(&home, &tokens, Some(&flow_active))
            });
            started_rx
                .recv_timeout(WAIT)
                .expect("authorized flow reaches the persistence barrier");
            active.signal_cancel();
            flow
        });

        assert_eq!(flow.join().unwrap(), LoginOutcome::Cancelled);
        assert!(matches!(load(&home), LoadState::Absent));
        let _ = std::fs::remove_dir_all(&home);
    }

    fn fresh_host() -> (tempfile::TempDir, crate::QuantixHost, PathBuf) {
        let dir = tempfile::tempdir().expect("temporary application home");
        let host = crate::QuantixHost::new(dir.path(), dir.path());
        let outcome = host.ensure_setup();
        assert!(
            matches!(
                outcome.state,
                crate::SetupState::Ready | crate::SetupState::Warning
            ),
            "settings database ready: {outcome:?}"
        );
        let home = dir.path().to_path_buf();
        (dir, host, home)
    }

    #[test]
    fn begin_short_circuits_when_the_store_is_already_fresh() {
        let (_dir, host, home) = fresh_host();
        let connection = StoredConnection {
            access_token: "at".to_string(),
            refresh_token: "rt".to_string(),
            id_token: "idt".to_string(),
            expires_at_ms: now_ms() + 3_600_000,
            account_id: "acc-fresh".to_string(),
            plan_type: Some("plus".to_string()),
            compute_residency: None,
        };
        save(&home, &connection).unwrap();

        let result = host.begin_chatgpt_login(LOGIN_PORT_CANDIDATES, PRODUCTION_ISSUER, |_url| {
            panic!("browser must not open for a fresh connection");
        });

        let result = result.expect("fresh connection short-circuit");
        assert_eq!(result.status, StartChatGptLoginStatus::Connected);
        assert_eq!(
            host.chatgpt_login_state().lock().unwrap().phase,
            ChatGptLoginPhase::Completed
        );
    }

    #[test]
    fn begin_starts_the_flow_and_completion_marks_the_machine_completed() {
        let (_dir, host, home) = fresh_host();
        let id_token = jwt_id_token("acc-flow", Some("team"));
        let issuer = start_mock_issuer(format!(
            r#"{{"access_token":"at-2","refresh_token":"rt-2","id_token":"{id_token}","expires_in":3600}}"#
        ));
        let stale = StoredConnection {
            access_token: "old".to_string(),
            refresh_token: "old".to_string(),
            id_token: "old".to_string(),
            expires_at_ms: now_ms().saturating_sub(1_000),
            account_id: "acc-old".to_string(),
            plan_type: None,
            compute_residency: None,
        };
        save(&home, &stale).unwrap();

        let (url_tx, url_rx) = mpsc::channel();
        let sender = Arc::new(std::sync::Mutex::new(Some(url_tx)));
        let result = host
            .begin_chatgpt_login(&[0], &issuer.base, move |url| {
                if let Some(sender) = sender.lock().unwrap().take() {
                    let _ = sender.send(url.to_string());
                }
            })
            .expect("flow starts");
        assert_eq!(result.status, StartChatGptLoginStatus::AwaitingBrowser);

        spawn_callback_driver(
            url_rx,
            "/auth/callback?code=coded-def&state={state}".to_string(),
        );
        assert!(wait_until(|| {
            host.chatgpt_login_state().lock().unwrap().phase != ChatGptLoginPhase::AwaitingBrowser
        }));
        assert_eq!(
            host.chatgpt_login_state().lock().unwrap().phase,
            ChatGptLoginPhase::Completed
        );
        match load(&home) {
            LoadState::Connected(connection) => {
                assert_eq!(connection.account_id, "acc-flow");
                assert_eq!(connection.plan_type.as_deref(), Some("team"));
            }
            other => panic!("expected refreshed store, got {other:?}"),
        }
    }

    #[test]
    fn begin_rejects_a_second_flow_while_one_is_active() {
        let (_dir, host, _home) = fresh_host();
        let issuer = start_mock_issuer("{}".to_string());
        host.begin_chatgpt_login(&[0], &issuer.base, |_url| {})
            .expect("first flow starts");

        let error = host
            .begin_chatgpt_login(&[0], &issuer.base, |_url| {})
            .expect_err("second flow rejected");
        assert_eq!(error.code, TenderErrorCode::OauthAlreadyRunning);
        let device_error = host
            .begin_chatgpt_device_login(&issuer.base)
            .expect_err("device flow cannot overlap browser flow");
        assert_eq!(device_error.code, TenderErrorCode::OauthAlreadyRunning);

        host.cancel_chatgpt_login();
        assert!(wait_until(|| {
            host.chatgpt_login_state().lock().unwrap().phase == ChatGptLoginPhase::Cancelled
        }));
    }

    #[test]
    fn device_flow_uses_the_shared_guard_and_cancellation_phase() {
        let (_dir, host, _home) = fresh_host();
        let issuer = start_pending_device_issuer();

        let started = host
            .begin_chatgpt_device_login(&issuer.base)
            .expect("device flow starts");

        assert_eq!(started.user_code, "CODE-FLOW");
        assert_eq!(
            started.verification_url,
            format!("{}/codex/device", issuer.base)
        );
        assert_eq!(
            host.chatgpt_login_state().lock().unwrap().phase,
            ChatGptLoginPhase::AwaitingDevice
        );
        let error = host
            .begin_chatgpt_login(&[0], &issuer.base, |_url| {})
            .expect_err("browser flow cannot overlap device flow");
        assert_eq!(error.code, TenderErrorCode::OauthAlreadyRunning);

        host.cancel_chatgpt_login();
        assert!(wait_until(|| {
            host.chatgpt_login_state().lock().unwrap().phase == ChatGptLoginPhase::Cancelled
        }));
    }

    #[test]
    fn cancel_terminates_the_running_flow_and_is_idempotent() {
        let (_dir, host, home) = fresh_host();
        let issuer = start_mock_issuer("{}".to_string());
        host.begin_chatgpt_login(&[0], &issuer.base, |_url| {})
            .expect("flow starts");

        host.cancel_chatgpt_login();
        host.cancel_chatgpt_login();
        assert!(wait_until(|| {
            host.chatgpt_login_state().lock().unwrap().phase == ChatGptLoginPhase::Cancelled
        }));
        assert!(matches!(load(&home), LoadState::Absent));

        let restarted = host
            .begin_chatgpt_login(&[0], &issuer.base, |_url| {})
            .expect("restart after cancel");
        assert_eq!(restarted.status, StartChatGptLoginStatus::AwaitingBrowser);
        host.cancel_chatgpt_login();
        assert!(wait_until(|| {
            host.chatgpt_login_state().lock().unwrap().phase == ChatGptLoginPhase::Cancelled
        }));
    }

    #[test]
    fn cancel_after_completion_keeps_tokens_and_ends_idle() {
        let (_dir, host, home) = fresh_host();
        let id_token = jwt_id_token("acc-late-cancel", Some("plus"));
        let issuer = start_mock_issuer(format!(
            r#"{{"access_token":"at-9","refresh_token":"rt-9","id_token":"{id_token}","expires_in":3600}}"#
        ));
        let (url_tx, url_rx) = mpsc::channel();
        let sender = Arc::new(std::sync::Mutex::new(Some(url_tx)));
        host.begin_chatgpt_login(&[0], &issuer.base, move |url| {
            if let Some(sender) = sender.lock().unwrap().take() {
                let _ = sender.send(url.to_string());
            }
        })
        .expect("flow starts");

        spawn_callback_driver(
            url_rx,
            "/auth/callback?code=coded-late&state={state}".to_string(),
        );
        assert!(wait_until(|| {
            host.chatgpt_login_state().lock().unwrap().phase == ChatGptLoginPhase::Completed
        }));

        host.cancel_chatgpt_login();

        assert_eq!(
            host.chatgpt_login_state().lock().unwrap().phase,
            ChatGptLoginPhase::Idle
        );
        match load(&home) {
            LoadState::Connected(connection) => {
                assert_eq!(connection.account_id, "acc-late-cancel");
                assert_eq!(connection.plan_type.as_deref(), Some("plus"));
                assert!(!needs_refresh(&connection, now_ms()));
            }
            other => panic!("completed authorization must persist past cancel, got {other:?}"),
        }
    }

    #[test]
    fn disconnect_clears_the_store_and_resets_the_machine() {
        let (_dir, host, home) = fresh_host();
        let connection = StoredConnection {
            access_token: "at".to_string(),
            refresh_token: "rt".to_string(),
            id_token: "idt".to_string(),
            expires_at_ms: now_ms() + 3_600_000,
            account_id: "acc-gone".to_string(),
            plan_type: None,
            compute_residency: None,
        };
        save(&home, &connection).unwrap();
        crate::application_settings::save_live_connection(
            &home,
            &crate::application_settings::ProviderConnectionView {
                connection_id: crate::application_settings::CODEX_CONNECTION_ID.to_owned(),
                provider: crate::application_settings::AiProviderKind::Codex,
                display_name: "ChatGPT subscription".to_owned(),
                status: crate::application_settings::ProviderConnectionStatus::Ready,
                account_label: Some("acc-gone".to_owned()),
                account_plan: Some("plus".to_owned()),
                models: Vec::new(),
                catalogue_fetched_at: Some("chatgpt-direct-v1".to_owned()),
                adapter_version: "chatgpt-direct-v1".to_owned(),
                status_summary: "Ready".to_owned(),
            },
        )
        .unwrap();

        let view = host.disconnect_chatgpt().expect("disconnect succeeds");
        assert_eq!(view.chatgpt.state, ChatGptConnectionState::Absent);
        assert!(view.chatgpt.account_id.is_none());
        let provider = view
            .provider_connections
            .iter()
            .find(|candidate| {
                candidate.connection_id == crate::application_settings::CODEX_CONNECTION_ID
            })
            .expect("ChatGPT provider status remains visible after disconnect");
        assert_eq!(
            provider.status,
            crate::application_settings::ProviderConnectionStatus::AuthenticationRequired
        );
        assert!(provider.account_label.is_none());
        assert!(provider.models.is_empty());
        assert!(matches!(load(&home), LoadState::Absent));
        assert!(!home.join("auth.json").exists());
        assert_eq!(
            host.chatgpt_login_state().lock().unwrap().phase,
            ChatGptLoginPhase::Idle
        );
    }

    #[test]
    fn disconnect_cancels_an_active_flow() {
        let (_dir, host, home) = fresh_host();
        let issuer = start_mock_issuer("{}".to_string());
        host.begin_chatgpt_login(&[0], &issuer.base, |_url| {})
            .expect("flow starts");

        let view = host.disconnect_chatgpt().expect("disconnect succeeds");
        assert_eq!(view.chatgpt.state, ChatGptConnectionState::Absent);
        assert!(wait_until(|| {
            host.chatgpt_login_state().lock().unwrap().phase == ChatGptLoginPhase::Idle
        }));
        assert!(host.chatgpt_login_state().lock().unwrap().active.is_none());
        assert!(matches!(load(&home), LoadState::Absent));
    }

    #[test]
    fn begin_reports_blocked_ports_without_process_details() {
        let (_dir, host, _home) = fresh_host();
        let Ok(gate_1455) = TcpListener::bind(("127.0.0.1", 1455)) else {
            println!("skipping: port 1455 occupied");
            return;
        };
        let Ok(gate_1457) = TcpListener::bind(("127.0.0.1", 1457)) else {
            println!("skipping: port 1457 occupied");
            return;
        };

        let error = host
            .begin_chatgpt_login(&[1455, 1457], PRODUCTION_ISSUER, |_url| {
                panic!("browser must not open when ports are blocked");
            })
            .expect_err("blocked ports reject synchronously");

        assert_eq!(error.code, TenderErrorCode::OauthPortBlocked);
        assert_eq!(
            serde_json::to_value(&error).unwrap(),
            serde_json::json!({ "code": "oauth_port_blocked" })
        );
        assert_eq!(
            host.chatgpt_login_state().lock().unwrap().phase,
            ChatGptLoginPhase::Idle
        );
        drop(gate_1455);
        drop(gate_1457);
    }

    #[test]
    fn begin_uses_the_next_candidate_when_the_first_port_is_busy() {
        let (_dir, host, _home) = fresh_host();
        let busy = TcpListener::bind((LOOPBACK_HOST, 0)).unwrap();
        let busy_port = busy.local_addr().unwrap().port();

        let started = host
            .begin_chatgpt_login(&[busy_port, 0], "http://127.0.0.1:9", |_url| {})
            .expect("a free fallback candidate starts the login flow");

        assert_eq!(started.status, StartChatGptLoginStatus::AwaitingBrowser);
        host.cancel_chatgpt_login();
    }

    #[test]
    fn connection_status_maps_every_load_state() {
        let home = temp_home("status-states");

        let absent = chatgpt_connection_status(&home);
        assert_eq!(absent.state, ChatGptConnectionState::Absent);
        assert!(absent.account_id.is_none());
        assert!(absent.plan_type.is_none());
        assert!(absent.expires_at_ms.is_none());

        save(
            &home,
            &StoredConnection {
                access_token: "at".to_string(),
                refresh_token: "rt".to_string(),
                id_token: "idt".to_string(),
                expires_at_ms: 5_000,
                account_id: "acc-status".to_string(),
                plan_type: Some("pro".to_string()),
                compute_residency: None,
            },
        )
        .unwrap();
        let connected = chatgpt_connection_status(&home);
        assert_eq!(connected.state, ChatGptConnectionState::Connected);
        assert_eq!(connected.account_id.as_deref(), Some("acc-status"));
        assert_eq!(connected.plan_type.as_deref(), Some("pro"));
        assert_eq!(connected.expires_at_ms, Some(5_000));

        std::fs::write(home.join("auth.json"), "<html>garbage</html>").unwrap();
        let unusable = chatgpt_connection_status(&home);
        assert_eq!(unusable.state, ChatGptConnectionState::Unusable);
        assert!(unusable.account_id.is_none());
        assert!(unusable.expires_at_ms.is_none());

        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn cancelled_opener_skips_the_browser_and_self_cancels_the_server() {
        let flow = Arc::new(ActiveLoginFlow::new());
        flow.signal_cancel();

        let gate = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = gate.local_addr().unwrap().port();
        let url = build_authorize_url(
            &format!("http://localhost:{port}/auth/callback"),
            &generate_pkce().unwrap(),
            &generate_state().unwrap(),
        );

        let opener = Arc::clone(&flow).opener(|_opened| {
            panic!("cancelled flows must not launch a browser");
        });
        std::thread::spawn(move || opener(&url));

        gate.set_nonblocking(true).unwrap();
        let deadline = Instant::now() + WAIT;
        let mut observed = false;
        while Instant::now() < deadline && !observed {
            if let Ok((mut stream, _)) = gate.accept() {
                let mut request = String::new();
                let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                let _ = stream.read_to_string(&mut request);
                observed = request.starts_with("GET /cancel");
            } else {
                std::thread::sleep(Duration::from_millis(20));
            }
        }
        assert!(observed, "the loopback server received the cancel signal");
    }
}
