use std::time::{Duration, Instant};

use serde::Serialize;

const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const USER_CODE_ENDPOINT: &str = "/api/accounts/deviceauth/usercode";
const TOKEN_POLL_ENDPOINT: &str = "/api/accounts/deviceauth/token";
const DEVICE_PATH: &str = "/codex/device";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const POLL_SAFETY_MARGIN: Duration = Duration::from_secs(3);
const DEVICE_LOGIN_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const CANCEL_CHECK_INTERVAL: Duration = Duration::from_millis(100);

pub(crate) fn device_login_deadline() -> Instant {
    let started = Instant::now();
    started.checked_add(DEVICE_LOGIN_TIMEOUT).unwrap_or(started)
}

#[derive(Debug, Clone)]
pub(crate) struct DeviceAuthorization {
    pub verification_url: String,
    pub user_code: String,
    device_auth_id: String,
    interval: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DeviceCodeGrant {
    pub authorization_code: String,
    pub code_verifier: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DevicePollOutcome {
    Authorized(DeviceCodeGrant),
    Cancelled,
    TimedOut,
}

#[derive(Debug)]
pub(crate) enum DeviceError {
    Provider { status: u16 },
    Transport,
    InvalidPayload,
    TimedOut,
}

impl std::fmt::Display for DeviceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Provider { status } => {
                write!(formatter, "device authorization failed with HTTP {status}")
            }
            Self::Transport => formatter.write_str("device authorization service is unreachable"),
            Self::InvalidPayload => {
                formatter.write_str("device authorization returned an invalid response")
            }
            Self::TimedOut => formatter.write_str("device authorization timed out"),
        }
    }
}

impl std::error::Error for DeviceError {}

pub(crate) struct DeviceClient {
    issuer_base: String,
    http: reqwest::blocking::Client,
}

#[derive(Serialize)]
struct UserCodeRequest<'a> {
    client_id: &'a str,
}

#[derive(Serialize)]
struct TokenPollRequest<'a> {
    device_auth_id: &'a str,
    user_code: &'a str,
}

impl DeviceClient {
    pub(crate) fn new(issuer_base: &str) -> Result<Self, DeviceError> {
        let http = reqwest::blocking::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|_| DeviceError::Transport)?;
        Ok(Self {
            issuer_base: issuer_base.trim_end_matches('/').to_owned(),
            http,
        })
    }

    pub(crate) fn initiate(&self, deadline: Instant) -> Result<DeviceAuthorization, DeviceError> {
        let timeout = remaining_request_timeout(deadline).ok_or(DeviceError::TimedOut)?;
        let response = match self
            .http
            .post(format!("{}{USER_CODE_ENDPOINT}", self.issuer_base))
            .timeout(timeout)
            .json(&UserCodeRequest {
                client_id: CLIENT_ID,
            })
            .send()
        {
            Ok(response) => response,
            Err(error) if error.is_timeout() || Instant::now() >= deadline => {
                return Err(DeviceError::TimedOut);
            }
            Err(_) => return Err(DeviceError::Transport),
        };
        if Instant::now() >= deadline {
            return Err(DeviceError::TimedOut);
        }
        let status = response.status();
        if !status.is_success() {
            return Err(DeviceError::Provider {
                status: status.as_u16(),
            });
        }
        let value: serde_json::Value = response.json().map_err(|error| {
            if error.is_timeout() || Instant::now() >= deadline {
                DeviceError::TimedOut
            } else {
                DeviceError::InvalidPayload
            }
        })?;
        if Instant::now() >= deadline {
            return Err(DeviceError::TimedOut);
        }
        let device_auth_id = required_string(&value, "device_auth_id")?;
        let user_code = required_string(&value, "user_code")?;
        let interval_secs = parsed_interval(&value).max(1);
        Ok(DeviceAuthorization {
            verification_url: format!("{}{DEVICE_PATH}", self.issuer_base),
            user_code,
            device_auth_id,
            interval: Duration::from_secs(interval_secs),
        })
    }

    pub(crate) fn poll(
        &self,
        authorization: &DeviceAuthorization,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<DevicePollOutcome, DeviceError> {
        self.poll_with_deadline(
            authorization,
            cancelled,
            deadline,
            pending_poll_delay(authorization.interval),
        )
    }

    fn poll_with_deadline(
        &self,
        authorization: &DeviceAuthorization,
        cancelled: impl Fn() -> bool,
        deadline: Instant,
        pending_delay: Duration,
    ) -> Result<DevicePollOutcome, DeviceError> {
        loop {
            if cancelled() {
                return Ok(DevicePollOutcome::Cancelled);
            }
            if Instant::now() >= deadline {
                return Ok(DevicePollOutcome::TimedOut);
            }
            let Some(request_timeout) = remaining_request_timeout(deadline) else {
                return Ok(DevicePollOutcome::TimedOut);
            };
            let response = match self
                .http
                .post(format!("{}{TOKEN_POLL_ENDPOINT}", self.issuer_base))
                .timeout(request_timeout)
                .json(&TokenPollRequest {
                    device_auth_id: &authorization.device_auth_id,
                    user_code: &authorization.user_code,
                })
                .send()
            {
                Ok(response) => response,
                Err(error) if error.is_timeout() || Instant::now() >= deadline => {
                    return Ok(DevicePollOutcome::TimedOut);
                }
                Err(_) => return Err(DeviceError::Transport),
            };
            if cancelled() {
                return Ok(DevicePollOutcome::Cancelled);
            }
            if Instant::now() >= deadline {
                return Ok(DevicePollOutcome::TimedOut);
            }
            let status = response.status();
            if status.is_success() {
                let value: serde_json::Value = response.json().map_err(|error| {
                    if error.is_timeout() || Instant::now() >= deadline {
                        DeviceError::TimedOut
                    } else {
                        DeviceError::InvalidPayload
                    }
                })?;
                if cancelled() {
                    return Ok(DevicePollOutcome::Cancelled);
                }
                if Instant::now() >= deadline {
                    return Ok(DevicePollOutcome::TimedOut);
                }
                return Ok(DevicePollOutcome::Authorized(DeviceCodeGrant {
                    authorization_code: required_string(&value, "authorization_code")?,
                    code_verifier: required_string(&value, "code_verifier")?,
                }));
            }
            if status.as_u16() != 403 && status.as_u16() != 404 {
                return Err(DeviceError::Provider {
                    status: status.as_u16(),
                });
            }
            if !wait_until_next_poll(deadline, pending_delay, &cancelled) {
                return Ok(if cancelled() {
                    DevicePollOutcome::Cancelled
                } else {
                    DevicePollOutcome::TimedOut
                });
            }
        }
    }
}

fn pending_poll_delay(interval: Duration) -> Duration {
    interval.saturating_add(POLL_SAFETY_MARGIN)
}

fn remaining_request_timeout(deadline: Instant) -> Option<Duration> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    (!remaining.is_zero()).then(|| remaining.min(REQUEST_TIMEOUT))
}

fn required_string(value: &serde_json::Value, field: &str) -> Result<String, DeviceError> {
    value
        .get(field)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or(DeviceError::InvalidPayload)
}

fn parsed_interval(value: &serde_json::Value) -> u64 {
    value
        .get("interval")
        .and_then(|interval| match interval {
            serde_json::Value::String(value) => value.trim().parse().ok(),
            serde_json::Value::Number(value) => value.as_u64(),
            _ => None,
        })
        .unwrap_or(5)
}

fn wait_until_next_poll(deadline: Instant, delay: Duration, cancelled: &impl Fn() -> bool) -> bool {
    let wait_deadline = Instant::now()
        .checked_add(delay)
        .map(|candidate| candidate.min(deadline))
        .unwrap_or(deadline);
    loop {
        if cancelled() || Instant::now() >= deadline {
            return false;
        }
        let remaining = wait_deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return true;
        }
        std::thread::sleep(remaining.min(CANCEL_CHECK_INTERVAL));
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::{Arc, Mutex};

    use super::*;

    #[derive(Clone)]
    struct CapturedRequest {
        path: String,
        body: String,
    }

    fn start_server(
        responses: Vec<(u16, &'static str)>,
    ) -> (String, Arc<Mutex<Vec<CapturedRequest>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let captured = Arc::new(Mutex::new(Vec::new()));
        let shared = Arc::clone(&captured);
        std::thread::spawn(move || {
            for (status, body) in responses {
                let (mut stream, _) = listener.accept().unwrap();
                shared.lock().unwrap().push(read_request(&mut stream));
                let response = format!(
                    "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).unwrap();
            }
        });
        (base, captured)
    }

    fn read_request(stream: &mut TcpStream) -> CapturedRequest {
        let mut raw = Vec::new();
        let mut chunk = [0u8; 1024];
        let split = loop {
            let read = stream.read(&mut chunk).unwrap();
            raw.extend_from_slice(&chunk[..read]);
            if let Some(split) = raw.windows(4).position(|window| window == b"\r\n\r\n") {
                break split;
            }
        };
        let head = String::from_utf8_lossy(&raw[..split]).to_string();
        let length = head
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                if name.eq_ignore_ascii_case("content-length") {
                    value.trim().parse::<usize>().ok()
                } else {
                    None
                }
            })
            .unwrap_or(0);
        let mut body = raw[split + 4..].to_vec();
        while body.len() < length {
            let read = stream.read(&mut chunk).unwrap();
            body.extend_from_slice(&chunk[..read]);
        }
        CapturedRequest {
            path: head
                .lines()
                .next()
                .unwrap()
                .split_whitespace()
                .nth(1)
                .unwrap()
                .to_owned(),
            body: String::from_utf8(body).unwrap(),
        }
    }

    fn authorization(base: &str) -> DeviceAuthorization {
        DeviceAuthorization {
            verification_url: format!("{base}/codex/device"),
            user_code: "CODE-123".to_owned(),
            device_auth_id: "device-123".to_owned(),
            interval: Duration::from_secs(1),
        }
    }

    #[test]
    fn initiation_posts_client_id_and_requires_non_empty_fields() {
        let (base, captured) = start_server(vec![(
            200,
            r#"{"device_auth_id":"device-123","user_code":"CODE-123","interval":"0"}"#,
        )]);
        let client = DeviceClient::new(&base).unwrap();

        let pending = client
            .initiate(Instant::now() + Duration::from_secs(5))
            .unwrap();

        assert_eq!(pending.verification_url, format!("{base}/codex/device"));
        assert_eq!(pending.user_code, "CODE-123");
        assert_eq!(pending.interval, Duration::from_secs(1));
        let requests = captured.lock().unwrap();
        let request = &requests[0];
        assert_eq!(request.path, USER_CODE_ENDPOINT);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&request.body).unwrap(),
            serde_json::json!({ "client_id": CLIENT_ID })
        );
    }

    #[test]
    fn initiation_rejects_empty_device_identifiers() {
        let (base, _) = start_server(vec![(
            200,
            r#"{"device_auth_id":" ","user_code":"CODE-123","interval":"5"}"#,
        )]);
        let client = DeviceClient::new(&base).unwrap();

        assert!(matches!(
            client.initiate(Instant::now() + Duration::from_secs(5)),
            Err(DeviceError::InvalidPayload)
        ));
    }

    #[test]
    fn pending_poll_retries_and_posts_device_credentials() {
        let (base, captured) = start_server(vec![
            (403, r#"{"provider":"details are ignored"}"#),
            (404, r#"{"provider":"still pending"}"#),
            (
                200,
                r#"{"authorization_code":"auth-1","code_verifier":"verify-1"}"#,
            ),
        ]);
        let client = DeviceClient::new(&base).unwrap();

        let outcome = client
            .poll_with_deadline(
                &authorization(&base),
                || false,
                Instant::now() + Duration::from_secs(5),
                Duration::ZERO,
            )
            .unwrap();

        assert_eq!(
            outcome,
            DevicePollOutcome::Authorized(DeviceCodeGrant {
                authorization_code: "auth-1".to_owned(),
                code_verifier: "verify-1".to_owned(),
            })
        );
        let requests = captured.lock().unwrap();
        assert_eq!(requests.len(), 3);
        assert_eq!(requests[0].path, TOKEN_POLL_ENDPOINT);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&requests[0].body).unwrap(),
            serde_json::json!({
                "device_auth_id": "device-123",
                "user_code": "CODE-123",
            })
        );
    }

    #[test]
    fn cancellation_and_timeout_stop_before_another_poll() {
        let (base, captured) = start_server(Vec::new());
        let client = DeviceClient::new(&base).unwrap();
        assert_eq!(
            client
                .poll_with_deadline(
                    &authorization(&base),
                    || true,
                    Instant::now() + Duration::from_secs(5),
                    Duration::ZERO,
                )
                .unwrap(),
            DevicePollOutcome::Cancelled
        );
        assert_eq!(
            client
                .poll_with_deadline(
                    &authorization(&base),
                    || false,
                    Instant::now(),
                    Duration::ZERO,
                )
                .unwrap(),
            DevicePollOutcome::TimedOut
        );
        assert!(captured.lock().unwrap().is_empty());
    }

    #[test]
    fn provider_failure_redacts_the_response_body() {
        let sentinel = "SENSITIVE_PROVIDER_BODY_MUST_NOT_ESCAPE";
        let (base, _) = start_server(vec![(500, sentinel)]);
        let client = DeviceClient::new(&base).unwrap();

        let error = client
            .poll_with_deadline(
                &authorization(&base),
                || false,
                Instant::now() + Duration::from_secs(5),
                Duration::ZERO,
            )
            .unwrap_err();

        assert!(matches!(&error, DeviceError::Provider { status: 500 }));
        assert!(!error.to_string().contains(sentinel));
    }

    #[test]
    fn initiation_request_cannot_outlive_the_shared_deadline() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let _ = read_request(&mut stream);
            std::thread::sleep(Duration::from_secs(1));
        });
        let client = DeviceClient::new(&base).unwrap();
        let started = Instant::now();

        let error = client
            .initiate(started + Duration::from_millis(50))
            .unwrap_err();

        assert!(matches!(error, DeviceError::TimedOut));
        assert!(started.elapsed() < Duration::from_millis(500));
    }

    #[test]
    fn polling_uses_interval_plus_three_seconds_within_the_existing_deadline() {
        assert_eq!(
            pending_poll_delay(Duration::from_secs(2)),
            Duration::from_secs(5)
        );

        let (base, captured) = start_server(Vec::new());
        let client = DeviceClient::new(&base).unwrap();
        let deadline = Instant::now();
        let outcome = client
            .poll(&authorization(&base), deadline, || false)
            .unwrap();

        assert_eq!(outcome, DevicePollOutcome::TimedOut);
        assert!(captured.lock().unwrap().is_empty());
    }
}
