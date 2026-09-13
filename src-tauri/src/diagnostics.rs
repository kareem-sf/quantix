//! Small, process-local JSONL diagnostics writer.
//!
//! The writer intentionally accepts only a fixed set of safe fields. It is
//! best effort: a filesystem failure disables this writer and emits one
//! generic warning, while the desktop process continues its normal work.

use crate::paths::StoragePaths;
use serde_json::{Map, Value};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const MAX_FILE_BYTES: u64 = 5 * 1024 * 1024;
const BACKUP_COUNT: u8 = 2;
const RETENTION_DAYS: u64 = 14;
const MAX_TOTAL_BYTES: u64 = 100 * 1024 * 1024;

static NEXT_SESSION: AtomicU64 = AtomicU64::new(1);

struct LogState {
    file: Option<File>,
    bytes: u64,
}

pub struct DiagnosticLogger {
    component: String,
    session_id: String,
    path: PathBuf,
    state: Mutex<Option<LogState>>,
    warned: AtomicBool,
}

impl DiagnosticLogger {
    pub fn new(component: &str) -> Arc<Self> {
        let component = safe_identifier(component).unwrap_or_else(|| "desktop".to_string());
        let session_id = session_identifier();
        let path = StoragePaths::normal()
            .ok()
            .map(|paths| paths.logs)
            .map(|directory| directory.join(format!("quantix-{component}-{}-{session_id}.jsonl", std::process::id())))
            .unwrap_or_default();
        let logger = Arc::new(Self {
            component,
            session_id,
            path,
            state: Mutex::new(None),
            warned: AtomicBool::new(false),
        });
        if logger.path.as_os_str().is_empty() {
            logger.disable();
        } else {
            match logger.path.parent() {
                Some(directory) => {
                    let opened = fs::create_dir_all(directory).and_then(|_| secure_open(&logger.path));
                    match opened {
                        Ok(file) => {
                            let bytes = file.metadata().map(|metadata| metadata.len()).unwrap_or(0);
                            *logger.state.lock().unwrap_or_else(|poison| poison.into_inner()) = Some(LogState { file: Some(file), bytes });
                            retain_logs(directory, &logger.path);
                        }
                        Err(_) => logger.disable(),
                    }
                }
                None => logger.disable(),
            }
        }
        logger
    }

    pub fn record(&self, event: &str, level: &str, fields: Value) {
        let Some(event) = safe_identifier(event) else { return };
        let mut object = Map::new();
        object.insert("timestamp".into(), Value::String(utc_now()));
        object.insert("level".into(), Value::String(if level == "error" { "error" } else { "info" }.into()));
        object.insert("component".into(), Value::String(self.component.clone()));
        object.insert("event".into(), Value::String(event));
        object.insert("process_id".into(), Value::from(std::process::id()));
        object.insert("session_id".into(), Value::String(self.session_id.clone()));
        append_safe_fields(&mut object, fields);
        let Ok(mut line) = serde_json::to_vec(&Value::Object(object)) else { return };
        line.push(b'\n');
        let Ok(mut guard) = self.state.lock() else { return };
        let failed = {
            let Some(state) = guard.as_mut() else { return };
            let mut failed = false;
            if state.bytes.saturating_add(line.len() as u64) > MAX_FILE_BYTES {
                if rotate(state, &self.path).is_err() {
                    failed = true;
                } else {
                    state.bytes = state.file.as_ref().and_then(|file| file.metadata().ok()).map(|metadata| metadata.len()).unwrap_or(0);
                }
            }
            if !failed {
                let Some(file) = state.file.as_mut() else { return };
                if file.write_all(&line).and_then(|_| file.flush()).is_err() {
                    failed = true;
                } else {
                    state.bytes = state.bytes.saturating_add(line.len() as u64);
                }
            }
            failed
        };
        drop(guard);
        if failed {
            self.disable();
        }
    }

    pub fn record_error(&self, event: &str, phase: &str, error: &(dyn std::error::Error + 'static)) {
        let mut fields = Map::new();
        fields.insert("phase".into(), Value::String(phase.to_string()));
        fields.insert("error_class".into(), Value::String(error_class(error).into()));
        if let Some(code) = error_code(error) {
            fields.insert("error_code".into(), Value::String(code.into()));
        }
        self.record(event, "error", Value::Object(fields));
    }

    pub fn record_failure(&self, event: &str, phase: &str, error_class_name: &str, error_code_name: Option<&str>) {
        let mut fields = Map::new();
        fields.insert("phase".into(), Value::String(phase.to_string()));
        fields.insert("error_class".into(), Value::String(error_class_name.to_string()));
        if let Some(code) = error_code_name {
            fields.insert("error_code".into(), Value::String(code.to_string()));
        }
        self.record(event, "error", Value::Object(fields));
    }

    fn disable(&self) {
        if let Ok(mut state) = self.state.lock() {
            *state = None;
        }
        if !self.warned.swap(true, Ordering::SeqCst) {
            let mut stderr = io::stderr().lock();
            let _ = writeln!(stderr, "Quantix diagnostic logging is unavailable.");
        }
    }
}

impl Drop for DiagnosticLogger {
    fn drop(&mut self) {
        if let Ok(mut state) = self.state.lock() {
            if let Some(state) = state.as_mut() {
                if let Some(file) = state.file.as_mut() { let _ = file.flush(); }
            }
        }
    }
}

fn append_safe_fields(target: &mut Map<String, Value>, fields: Value) {
    let Value::Object(fields) = fields else { return };
    for (key, value) in fields {
        if matches!(key.as_str(), "phase" | "outcome" | "child" | "process" | "error_class") {
            if let Some(text) = value.as_str().and_then(safe_identifier) {
                target.insert(key, Value::String(text));
            }
        } else if key == "error_code" {
            if let Some(text) = value.as_str().filter(|text| safe_code(text)) {
                target.insert(key, Value::String((*text).to_string()));
            }
        } else if key == "signal_name" {
            if let Some(text) = value.as_str().filter(|text| safe_signal(text)) {
                target.insert(key, Value::String((*text).to_string()));
            }
        } else if key == "exit_code" {
            if let Some(number) = value.as_i64().filter(|number| *number >= -1_000_000 && *number <= 1_000_000_000_000) {
                target.insert(key, Value::from(number));
            }
        } else if matches!(key.as_str(), "child_process_id" | "signal" | "duration_ms" | "line" | "column") {
            if let Some(number) = value.as_u64().filter(|number| *number <= 1_000_000_000_000) {
                target.insert(key, Value::from(number));
            }
        }
    }
}

fn safe_identifier(value: &str) -> Option<String> {
    if value.is_empty() || value.len() > 64 || !value.bytes().all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_' || byte == b'-') {
        return None;
    }
    Some(value.to_string())
}

fn safe_code(value: &str) -> bool {
    !value.is_empty() && value.len() <= 32 && value.bytes().all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'.' | b'-'))
}

fn safe_signal(value: &str) -> bool {
    !value.is_empty() && value.len() <= 24 && value.bytes().all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
}

fn error_class(error: &(dyn std::error::Error + 'static)) -> &'static str {
    if error.downcast_ref::<io::Error>().is_some() { "io_error" } else { "error" }
}

fn error_code(error: &(dyn std::error::Error + 'static)) -> Option<&'static str> {
    let io_error = error.downcast_ref::<io::Error>()?;
    Some(match io_error.kind() {
        io::ErrorKind::NotFound => "ENOENT",
        io::ErrorKind::PermissionDenied => "EACCES",
        io::ErrorKind::ConnectionRefused => "ECONNREFUSED",
        io::ErrorKind::ConnectionReset => "ECONNRESET",
        io::ErrorKind::ConnectionAborted => "ECONNABORTED",
        io::ErrorKind::NotConnected => "ENOTCONN",
        io::ErrorKind::AddrInUse => "EADDRINUSE",
        io::ErrorKind::AddrNotAvailable => "EADDRNOTAVAIL",
        io::ErrorKind::BrokenPipe => "EPIPE",
        io::ErrorKind::AlreadyExists => "EEXIST",
        io::ErrorKind::InvalidInput => "EINVAL",
        io::ErrorKind::TimedOut => "ETIMEDOUT",
        io::ErrorKind::Interrupted => "EINTR",
        io::ErrorKind::WouldBlock => "EAGAIN",
        _ => "EIO",
    })
}

fn session_identifier() -> String {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    let counter = NEXT_SESSION.fetch_add(1, Ordering::Relaxed) as u128;
    let value = now ^ ((std::process::id() as u128) << 48) ^ counter;
    format!("{value:032x}")
}

fn utc_now() -> String {
    let duration = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    let seconds = duration.as_secs();
    let days = (seconds / 86_400) as i64;
    let day_seconds = seconds % 86_400;
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z", day_seconds / 3_600, (day_seconds % 3_600) / 60, day_seconds % 60, duration.subsec_millis())
}

// Gregorian civil date conversion from an epoch day count, avoiding a time
// dependency in the native shell.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = (if z >= 0 { z } else { z - 146_096 }) / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let month_part = (5 * doy + 2) / 153;
    let day = doy - (153 * month_part + 2) / 5 + 1;
    let month = month_part + if month_part < 10 { 3 } else { -9 };
    (year + if month <= 2 { 1 } else { 0 }, month, day)
}

fn rotate(state: &mut LogState, path: &Path) -> io::Result<()> {
    if let Some(mut file) = state.file.take() {
        file.flush()?;
        let _ = file.sync_data();
        drop(file);
    }
    for index in (1..=BACKUP_COUNT).rev() {
        let source = if index == 1 { path.to_path_buf() } else { backup_path(path, index - 1) };
        let destination = backup_path(path, index);
        remove_if_exists(&destination)?;
        if source.exists() {
            fs::rename(source, destination)?;
        }
    }
    state.file = Some(secure_open(path)?);
    Ok(())
}

fn backup_path(path: &Path, index: u8) -> PathBuf {
    PathBuf::from(format!("{}.{}", path.display(), index))
}

fn remove_if_exists(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn retain_logs(directory: &Path, current: &Path) {
    let Ok(entries) = fs::read_dir(directory) else { return };
    let mut files = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path == current || !entry.file_type().map(|kind| kind.is_file()).unwrap_or(false) { continue; }
        let Some(pid) = path.file_name().and_then(|name| name.to_str()).and_then(owned_pid) else { continue };
        if process_alive(pid) { continue; }
        let Ok(metadata) = entry.metadata() else { continue };
        files.push((path, metadata.len(), metadata.modified().unwrap_or(UNIX_EPOCH)));
    }
    files.sort_by_key(|(_, _, modified)| *modified);
    let cutoff = SystemTime::now().checked_sub(Duration::from_secs(RETENTION_DAYS * 86_400)).unwrap_or(UNIX_EPOCH);
    let mut total: u64 = files.iter().map(|(_, bytes, _)| *bytes).sum();
    for (path, bytes, modified) in files {
        if modified > cutoff && total <= MAX_TOTAL_BYTES { continue; }
        if fs::remove_file(path).is_ok() { total = total.saturating_sub(bytes); }
    }
}

fn owned_pid(name: &str) -> Option<u32> {
    let base = name.strip_suffix(".jsonl").or_else(|| {
        (1..=BACKUP_COUNT).find_map(|index| name.strip_suffix(&format!(".jsonl.{index}")))
    })?;
    let mut parts = base.rsplitn(3, '-');
    let session = parts.next()?;
    let pid = parts.next()?.parse::<u32>().ok()?;
    let component = parts.next()?.strip_prefix("quantix-")?;
    if safe_identifier(component).is_none() || session.len() != 32 || !session.bytes().all(|byte| byte.is_ascii_hexdigit()) { return None; }
    Some(pid)
}

fn process_alive(pid: u32) -> bool {
    if pid == 0 || pid == std::process::id() { return true; }
    #[cfg(unix)]
    {
        let result = unsafe { libc::kill(pid as libc::pid_t, 0) };
        result == 0 || io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::{CloseHandle, GetLastError};
        use windows_sys::Win32::System::Threading::{GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if handle.is_null() {
            // ERROR_INVALID_PARAMETER means the PID is definitely gone.
            // Access denied and all other failures remain protected from
            // retention deletion because liveness is unknown.
            return unsafe { GetLastError() } != 87;
        }
        let mut code = 0_u32;
        let queried = unsafe { GetExitCodeProcess(handle, &mut code) } != 0;
        unsafe { CloseHandle(handle); }
        if queried { code == 259 } else { true }
    }
    #[cfg(not(any(unix, windows)))]
    { false }
}

fn secure_open(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.create(true).append(true).read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}
