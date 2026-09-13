use crate::diagnostics::DiagnosticLogger;
use crate::paths::StoragePaths;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
use std::time::{Duration, Instant};
use tauri::{Manager, Runtime};
use tauri_plugin_shell::process::CommandChild;
#[cfg(not(debug_assertions))]
use tauri_plugin_shell::process::CommandEvent;

#[derive(Clone, Deserialize, Serialize)]
pub struct ConnectionInfo {
    pub base_url: String,
    pub token: String,
}

pub struct LocalService {
    pub connection_file: PathBuf,
    pub child: Mutex<Option<CommandChild>>,
}

/// Coordinates the one ExitRequested event that follows a successful
/// deliberate shutdown. OS shutdown and reset exit never set this marker.
pub struct ShutdownState {
    explicit_completion: AtomicBool,
    in_flight: AtomicBool,
}

impl Default for ShutdownState {
    fn default() -> Self {
        Self {
            explicit_completion: AtomicBool::new(false),
            in_flight: AtomicBool::new(false),
        }
    }
}

impl ShutdownState {
    pub(crate) fn mark_explicit_completion(&self) {
        self.explicit_completion.store(true, Ordering::Release);
    }

    pub(crate) fn take_explicit_completion(&self) -> bool {
        self.explicit_completion.swap(false, Ordering::AcqRel)
    }

    pub(crate) fn try_begin(&self) -> bool {
        self.in_flight
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    pub(crate) fn finish(&self) {
        self.in_flight.store(false, Ordering::Release);
    }
}

fn diagnostics<R: Runtime>(app: &tauri::AppHandle<R>) -> Option<Arc<DiagnosticLogger>> {
    app.try_state::<Arc<DiagnosticLogger>>().map(|state| state.inner().clone())
}

pub fn read_connection(path: &std::path::Path) -> Result<ConnectionInfo, String> {
    let bytes = std::fs::read(path)
        .map_err(|_| "The local workspace is starting or unavailable. Reopen Quantix.".to_string())?;
    let connection: ConnectionInfo = serde_json::from_slice(&bytes)
        .map_err(|_| "The local connection is incomplete. Reopen Quantix.".to_string())?;
    if !connection.base_url.starts_with("http://127.0.0.1:")
        || connection.token.len() < 32
        || connection.token.chars().any(|character| character == '\r' || character == '\n')
    {
        return Err("The local connection is invalid.".into());
    }
    Ok(connection)
}

pub fn connection<R: Runtime>(app: &tauri::AppHandle<R>) -> Result<ConnectionInfo, String> {
    let result = if let Some(state) = app.try_state::<LocalService>() {
        read_connection(&state.connection_file)
    } else {
        StoragePaths::normal().and_then(|paths| read_connection(&paths.connection_file))
    };
    if let Some(logger) = diagnostics(app) {
        if result.is_err() {
            logger.record_failure("native_connection_error", "connection_read", "connection_error", None);
        } else {
            logger.record("native_connection_ready", "info", json!({"phase":"connection_read", "outcome":"ready"}));
        }
    }
    result
}

fn request_shutdown_for_path(path: &Path, timeout: Duration) -> Result<(), String> {
    if !path
        .try_exists()
        .map_err(|_| "The local connection could not be checked.".to_string())?
    {
        return Ok(());
    }
    let connection = read_connection(path)?;
    let port = connection
        .base_url
        .strip_prefix("http://127.0.0.1:")
        .and_then(|value| value.strip_suffix("/api"))
        .and_then(|value| value.parse::<u16>().ok())
        .filter(|port| *port != 0)
        .ok_or_else(|| "The local connection is invalid.".to_string())?;
    let timeout = timeout.max(Duration::from_millis(1));
    let mut socket = std::net::TcpStream::connect_timeout(
        &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
        timeout,
    )
    .map_err(|_| "The local workspace is already closed or unavailable.".to_string())?;
    socket
        .set_write_timeout(Some(timeout))
        .map_err(|_| "The local workspace could not close.".to_string())?;
    socket
        .set_read_timeout(Some(timeout))
        .map_err(|_| "The local workspace could not close.".to_string())?;
    socket
        .write_all(format!(
            "POST /api/shutdown HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer {}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            connection.token
        ).as_bytes())
        .map_err(|_| "The local workspace could not close.".to_string())?;
    let mut response = [0u8; 128];
    let length = socket
        .read(&mut response)
        .map_err(|_| "The local workspace did not confirm shutdown.".to_string())?;
    let status = std::str::from_utf8(&response[..length]).unwrap_or_default();
    if status.starts_with("HTTP/1.1 200 ") || status.starts_with("HTTP/1.0 200 ") {
        Ok(())
    } else {
        Err("The local workspace did not accept shutdown. Reopen Quantix and choose Retry reset.".into())
    }
}

fn wait_for_connection_release(path: &Path, timeout: Duration) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    loop {
        match path.try_exists() {
            Ok(false) => return Ok(()),
            Ok(true) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(100));
            }
            Ok(true) => {
                return Err("The local workspace did not finish closing.".into());
            }
            Err(_) if Instant::now() < deadline => {
                // Windows can briefly report a sharing violation while the
                // service removes its connection record.
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(_) => return Err("The local connection could not be checked.".into()),
        }
    }
}

fn coordinate_shutdown(path: &Path, timeout: Duration) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    let request = request_shutdown_for_path(path, timeout);
    match request {
        Ok(()) => wait_for_connection_release(path, deadline.saturating_duration_since(Instant::now())),
        Err(error) => {
            // A service that is already closing may reject a second request;
            // a released connection record is the authoritative completion
            // signal for this local workspace.
            match wait_for_connection_release(path, deadline.saturating_duration_since(Instant::now())) {
                Ok(()) => Ok(()),
                Err(_) => Err(error),
            }
        }
    }
}

fn stop_external_workspace(path: &Path) -> Result<(), String> {
    coordinate_shutdown(path, Duration::from_secs(18))
}

/// Reset explicitly shuts down the normal authenticated service in debug and
/// packaged sessions. Ordinary development window closure stays independent.
/// This path runs before logging as part of confirmed startup recovery too.
pub fn request_normal_shutdown() -> Result<(), String> {
    let path = StoragePaths::normal()?.connection_file;
    request_shutdown_for_path(&path, Duration::from_secs(2))
}

/// Requests and confirms shutdown for deliberate Quit before the process exits.
/// The normal connection record is the only source used for a reused service;
/// native children remain owned and cleaned by the existing stop path.
pub fn shutdown_for_exit<R: Runtime>(app: &tauri::AppHandle<R>) -> Result<(), String> {
    let path = if let Some(state) = app.try_state::<LocalService>() {
        state.connection_file.clone()
    } else {
        StoragePaths::normal()?.connection_file
    };
    coordinate_shutdown(&path, Duration::from_secs(18))?;
    if let Some(state) = app.try_state::<ShutdownState>() {
        state.mark_explicit_completion();
    }
    if app.try_state::<LocalService>().is_some() {
        if let Some(state) = app.try_state::<LocalService>() {
            if let Ok(mut owned) = state.child.lock() {
                if let Some(child) = owned.take() {
                    let _ = child.kill();
                }
            }
        }
    }
    Ok(())
}

#[cfg(not(debug_assertions))]
pub fn start(app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    use tauri_plugin_shell::ShellExt;
    let logger = diagnostics(app);
    if let Some(logger) = &logger { logger.record("native_service_start", "info", json!({"phase":"service_start"})); }
    let storage = match StoragePaths::normal().map_err(std::io::Error::other) {
        Ok(paths) => paths,
        Err(error) => {
            if let Some(logger) = &logger { logger.record_error("native_service_start_failed", "service_home", &error); }
            return Err(Box::new(error));
        }
    };
    if let Err(error) = std::fs::create_dir_all(&storage.runtime) {
        if let Some(logger) = &logger { logger.record_error("native_service_start_failed", "service_home", &error); }
        return Err(Box::new(error));
    }
    if let Err(error) = std::fs::create_dir_all(&storage.tmp) {
        if let Some(logger) = &logger { logger.record_error("native_service_start_failed", "service_temp", &error); }
        return Err(Box::new(error));
    }
    let home = storage.root;
    let connection_file = storage.connection_file;
    let temporary = storage.tmp;
    if connection_file.exists() {
        if let Err(error) = std::fs::remove_file(&connection_file) {
            if let Some(logger) = &logger { logger.record_error("native_service_start_failed", "connection_cleanup", &error); }
            return Err(Box::new(error));
        }
    }
    let sidecar = match app.shell().sidecar("quantix-service") {
        Ok(sidecar) => sidecar,
        Err(error) => {
            if let Some(logger) = &logger { logger.record_failure("native_service_spawn_failed", "sidecar_prepare", "sidecar_error", None); }
            return Err(Box::new(error));
        }
    };
    let mut command = sidecar
        .args(["--port", "0", "--home", &home.to_string_lossy(), "--connection-file", &connection_file.to_string_lossy()])
        .envs([("TEMP", &temporary), ("TMP", &temporary), ("TMPDIR", &temporary)]);
    // OCR and the meaning-search model ship as bundle resources beside the app.
    if let Ok(resources) = app.path().resource_dir() {
        let engines = resources.join("engines");
        if engines.join("engines.json").is_file() {
            command = command.env("QUANTIX_ENGINES_DIR", engines);
        }
    }
    let (mut events, child) = match command.spawn()
    {
        Ok(value) => value,
        Err(error) => {
            if let Some(logger) = &logger { logger.record_failure("native_service_spawn_failed", "sidecar_spawn", "sidecar_error", None); }
            return Err(Box::new(error));
        }
    };
    let child_pid = child.pid();
    if let Some(logger) = &logger { logger.record("native_service_spawned", "info", json!({"phase":"sidecar_spawn", "process":"service", "outcome":"success", "child_process_id":child_pid})); }
    app.manage(LocalService { connection_file: connection_file.clone(), child: Mutex::new(Some(child)) });
    let event_logger = logger.clone();
    tauri::async_runtime::spawn(async move {
        let started = Instant::now();
        while let Some(event) = events.recv().await {
            match event {
                CommandEvent::Error(_) => {
                    if let Some(logger) = &event_logger {
                        logger.record_failure("native_service_command_error", "sidecar_event", "command_error", None);
                    }
                }
                CommandEvent::Terminated(payload) => {
                    if let Some(logger) = &event_logger {
                        logger.record("native_service_process_exit", if payload.code == Some(0) { "info" } else { "error" }, json!({
                            "phase":"service",
                            "process":"service",
                            "outcome": if payload.code == Some(0) { "success" } else { "failed" },
                            "exit_code": payload.code,
                            "signal": payload.signal,
                            "duration_ms": started.elapsed().as_millis().min(i32::MAX as u128) as u64,
                        }));
                    }
                }
                CommandEvent::Stdout(_) | CommandEvent::Stderr(_) => {}
                _ => {}
            }
        }
        if let Some(logger) = &event_logger {
            logger.record("native_service_events_closed", "info", json!({"phase":"sidecar_event_stream", "process":"service"}));
        }
    });
    // Startup coordination reads only the owned service's connection file.
    let started = Instant::now();
    for _ in 0..1200 {
        if read_connection(&connection_file).is_ok() {
            if let Some(logger) = &logger {
                logger.record("native_service_ready", "info", json!({"phase":"connection_probe", "outcome":"success", "duration_ms": started.elapsed().as_millis().min(i32::MAX as u128) as u64}));
            }
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    if let Some(logger) = &logger { logger.record_failure("native_service_start_timeout", "connection_probe", "startup_timeout", Some("ETIMEDOUT")); }
    stop(app);
    Err("The local service did not finish starting. Check installation permissions and reopen Quantix.".into())
}

/// Stops the service owned by the desktop process, or the exact authenticated
/// normal workspace reused by a source-development launch.
pub fn stop<R: Runtime>(app: &tauri::AppHandle<R>) {
    let logger = diagnostics(app);
    if let Some(state) = app.try_state::<ShutdownState>() {
        if state.take_explicit_completion() {
            if let Some(logger) = &logger {
                logger.record("native_service_stop_skipped", "info", json!({"phase":"service_shutdown", "outcome":"already_completed"}));
            }
            return;
        }
    }
    let started = Instant::now();
    let mut stop_succeeded = true;
    if let Some(logger) = &logger { logger.record("native_service_stop", "info", json!({"phase":"service_shutdown", "outcome":"requested"})); }
    if let Some(state) = app.try_state::<LocalService>() {
        if state.connection_file.exists() {
            if let Ok(connection) = read_connection(&state.connection_file) {
            if let Some(port) = connection.base_url.strip_prefix("http://127.0.0.1:").and_then(|value| value.strip_suffix("/api")).and_then(|value| value.parse::<u16>().ok()) {
                match std::net::TcpStream::connect_timeout(&std::net::SocketAddr::from(([127, 0, 0, 1], port)), std::time::Duration::from_secs(1)) {
                    Ok(mut socket) => {
                        let _ = socket.set_write_timeout(Some(std::time::Duration::from_secs(1)));
                        if let Err(error) = socket.write_all(format!("POST /api/shutdown HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer {}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n", connection.token).as_bytes()) {
                            stop_succeeded = false;
                            if let Some(logger) = &logger { logger.record_error("native_service_shutdown_error", "shutdown_request", &error); }
                        }
                    }
                    Err(error) => {
                        stop_succeeded = false;
                        if let Some(logger) = &logger { logger.record_error("native_service_shutdown_error", "shutdown_connect", &error); }
                    }
                }
            } else {
                stop_succeeded = false;
            }
            } else {
                stop_succeeded = false;
                if let Some(logger) = &logger {
                    logger.record_failure("native_connection_error", "shutdown_connection_read", "connection_error", None);
                }
            }
        }
        for _ in 0..180 {
            if !state.connection_file.exists() { break; }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        if let Ok(mut owned) = state.child.lock() {
            if let Some(child) = owned.take() {
                #[cfg(windows)]
                if state.connection_file.exists() {
                    use std::os::windows::process::CommandExt;
                    match std::process::Command::new("taskkill.exe")
                        .args(["/PID", &child.pid().to_string(), "/T", "/F"])
                        .creation_flags(0x08000000).output()
                    {
                        Ok(output) if !output.status.success() => {
                            stop_succeeded = false;
                            if let Some(logger) = &logger { logger.record_failure("native_service_stop_failed", "taskkill", "process_stop_error", None); }
                        }
                        Err(error) => {
                            stop_succeeded = false;
                            if let Some(logger) = &logger { logger.record_error("native_service_stop_failed", "taskkill", &error); }
                        }
                        _ => {}
                    }
                }
                if let Err(error) = child.kill() {
                    stop_succeeded = false;
                    if let Some(logger) = &logger { logger.record_failure("native_service_stop_failed", "child_kill", "process_stop_error", None); }
                    let _ = error;
                }
            }
        } else {
            stop_succeeded = false;
        }
    } else if let Ok(storage) = StoragePaths::normal() {
        // Source development may reuse a backend owned by the launcher rather
        // than registering a native LocalService child. Explicit exit still
        // closes that exact authenticated workspace, without a PID guess or
        // process-tree kill.
        let shutdown = stop_external_workspace(&storage.connection_file);
        if let Err(error) = shutdown {
            stop_succeeded = false;
            if let Some(logger) = &logger {
                logger.record_error("native_service_shutdown_error", "reused_service_shutdown", &std::io::Error::other(error));
            }
        }
    } else {
        stop_succeeded = false;
    }
    if let Some(logger) = &logger {
        logger.record("native_service_stop_finished", if stop_succeeded { "info" } else { "error" }, json!({"phase":"service_shutdown", "outcome": if stop_succeeded { "finished" } else { "failed" }, "duration_ms": started.elapsed().as_millis().min(i32::MAX as u128) as u64}));
    }
}

#[cfg(test)]
mod tests {
    use super::coordinate_shutdown;
    use serde_json::json;
    use std::fs;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::path::{Path, PathBuf};
    use std::time::Duration;
    use std::thread;

    fn connection_fixture(root: &Path, port: u16) -> (PathBuf, String) {
        let connection_file = root.join("connection.json");
        let token = "synthetic-session-token-0123456789abcdef0123456789abcdef".to_string();
        fs::write(
            &connection_file,
            serde_json::to_vec(&json!({
                "base_url": format!("http://127.0.0.1:{port}/api"),
                "token": token,
            }))
            .unwrap(),
        )
        .unwrap();
        (connection_file, token)
    }

    fn read_request(stream: &mut std::net::TcpStream) -> String {
        let mut request = Vec::new();
        let mut buffer = [0u8; 512];
        loop {
            let length = stream.read(&mut buffer).unwrap();
            if length == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..length]);
            if request.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
        }
        String::from_utf8(request).unwrap()
    }

    #[test]
    fn reused_workspace_stop_requests_authenticated_shutdown() {
        let root = tempfile::tempdir().unwrap();
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let (connection_file, token) = connection_fixture(root.path(), port);
        let released_file = connection_file.clone();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_request(&mut stream);
            assert!(request.starts_with("POST /api/shutdown HTTP/1.1\r\n"));
            assert!(request.contains(&format!("Authorization: Bearer {token}\r\n")));
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .unwrap();
            fs::remove_file(released_file).unwrap();
        });

        coordinate_shutdown(&connection_file, Duration::from_secs(2)).unwrap();
        server.join().unwrap();
        assert!(!connection_file.exists());
    }

    #[test]
    fn shutdown_rejection_retains_connection_for_recovery() {
        let root = tempfile::tempdir().unwrap();
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let (connection_file, _) = connection_fixture(root.path(), port);
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let _ = read_request(&mut stream);
            stream
                .write_all(b"HTTP/1.1 409 Conflict\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .unwrap();
        });

        let result = coordinate_shutdown(&connection_file, Duration::from_millis(100));
        server.join().unwrap();
        assert!(result.is_err());
        assert!(connection_file.exists());
    }

    #[test]
    fn shutdown_timeout_retains_connection_for_recovery() {
        let root = tempfile::tempdir().unwrap();
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let (connection_file, _) = connection_fixture(root.path(), port);
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let _ = read_request(&mut stream);
            thread::sleep(Duration::from_millis(300));
        });

        let result = coordinate_shutdown(&connection_file, Duration::from_millis(100));
        server.join().unwrap();
        assert!(result.is_err());
        assert!(connection_file.exists());
    }

    #[test]
    fn already_closing_rejection_succeeds_when_connection_releases() {
        let root = tempfile::tempdir().unwrap();
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let (connection_file, _) = connection_fixture(root.path(), port);
        let released_file = connection_file.clone();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let _ = read_request(&mut stream);
            stream
                .write_all(b"HTTP/1.1 409 Conflict\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .unwrap();
            fs::remove_file(released_file).unwrap();
        });

        coordinate_shutdown(&connection_file, Duration::from_secs(2)).unwrap();
        server.join().unwrap();
        assert!(!connection_file.exists());
    }

    #[test]
    fn failed_shutdown_can_be_retried_without_replacing_connection() {
        let root = tempfile::tempdir().unwrap();
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let (connection_file, _) = connection_fixture(root.path(), port);
        let released_file = connection_file.clone();
        let server = thread::spawn(move || {
            let (mut first, _) = listener.accept().unwrap();
            let _ = read_request(&mut first);
            first
                .write_all(b"HTTP/1.1 409 Conflict\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .unwrap();
            drop(first);
            let (mut second, _) = listener.accept().unwrap();
            let _ = read_request(&mut second);
            second
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .unwrap();
            fs::remove_file(released_file).unwrap();
        });

        assert!(coordinate_shutdown(&connection_file, Duration::from_millis(100)).is_err());
        assert!(connection_file.exists());
        coordinate_shutdown(&connection_file, Duration::from_secs(2)).unwrap();
        server.join().unwrap();
        assert!(!connection_file.exists());
    }

    #[test]
    fn successful_explicit_shutdown_skips_the_following_exit_stop_once() {
        let state = super::ShutdownState::default();
        assert!(!state.take_explicit_completion());
        state.mark_explicit_completion();
        assert!(state.take_explicit_completion());
        assert!(!state.take_explicit_completion());
    }

    #[test]
    fn shutdown_coordinator_does_not_overlap_retries() {
        let state = super::ShutdownState::default();
        assert!(state.try_begin());
        assert!(!state.try_begin());
        state.finish();
        assert!(state.try_begin());
    }
}
