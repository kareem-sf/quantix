//! Factory reset runs before Tauri, logging, and WebView creation.
//! The public/CLI boundary resolves the normal home itself. Only private tests
//! call the purge routine with disposable roots.

use crate::paths::StoragePaths;
use std::path::Path;
use std::time::Duration;

const JOURNAL: &str = "pending-reset.json";
const LOCK: &str = "workspace.lock";
const TIMEOUT: Duration = Duration::from_secs(90);
const INVALID: &str = "Quantix cannot verify its pending reset. No cleanup was started. Keep the application closed and contact support to repair the reset record.";
const FAILED: &str = "Some Quantix files are still in use or could not be removed. Close other Quantix windows and related programs, then reopen Quantix and choose Retry reset.";

#[tauri::command]
pub fn reset_support() -> bool {
    cfg!(windows)
}

fn valid_id(value: &str) -> bool {
    value.len() == 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[tauri::command]
pub async fn finish_reset(app: tauri::AppHandle, reset_id: String) -> Result<(), String> {
    if !reset_support() {
        return Err("Reset is not available on this operating system.".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let root = StoragePaths::normal()?.root;
        let _guard = platform::pin_root(&root).map_err(|_| INVALID.to_string())?;
        let saved = read_journal(&root)?.ok_or(INVALID)?;
        validate(&saved, &root, &reset_id)?;
        let executable = std::env::current_exe()
            .map_err(|_| "Quantix could not find its reset helper.".to_string())?;
        let mut helper = std::process::Command::new(executable);
        helper
            .args([
                "--quantix-reset-helper",
                &reset_id,
                &std::process::id().to_string(),
            ])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            helper.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
        }
        helper.spawn().map_err(|_| {
            "Quantix could not start cleanup. Keep this window open and choose Retry reset."
                .to_string()
        })?;
        // Unlike ordinary development window closure, reset explicitly closes
        // the authenticated normal-home service even without LocalService state.
        let _ = crate::service::request_normal_shutdown();
        app.exit(0);
        Ok(())
    })
    .await
    .map_err(|_| "Quantix could not start cleanup. Choose Retry reset.".to_string())?
}

/// Called first in main. Helper/preflight modes must never start Tauri or log.
pub fn before_startup() -> Option<i32> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args
        .first()
        .is_some_and(|arg| arg == "--quantix-reset-helper")
    {
        let outcome = if args.len() == 3 && valid_id(&args[1]) {
            args[2]
                .parse::<u32>()
                .ok()
                .filter(|pid| *pid != 0 && *pid != std::process::id())
                .ok_or_else(|| INVALID.to_string())
                .and_then(|pid| run_helper(&args[1], pid))
        } else {
            Err(INVALID.into())
        };
        return Some(if outcome.is_ok() { 0 } else { 1 });
    }
    if args
        .first()
        .is_some_and(|arg| arg.starts_with("--quantix-reset"))
        && args.as_slice() != ["--quantix-reset-preflight"]
    {
        startup_error(INVALID);
        return Some(1);
    }
    if let Err(error) = startup_preflight() {
        startup_error(&error);
        return Some(1);
    }
    (args.as_slice() == ["--quantix-reset-preflight"]).then_some(0)
}

fn startup_error(message: &str) {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};
        let body: Vec<_> = message.encode_utf16().chain(Some(0)).collect();
        let title: Vec<_> = "Quantix could not open"
            .encode_utf16()
            .chain(Some(0))
            .collect();
        unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                body.as_ptr(),
                title.as_ptr(),
                MB_OK | MB_ICONERROR,
            );
        }
    }
    #[cfg(not(windows))]
    eprintln!("{message}");
}

fn startup_preflight() -> Result<(), String> {
    let root = StoragePaths::normal()?.root;
    if !root.try_exists().map_err(|_| INVALID.to_string())? {
        return Ok(());
    }
    let _guard = platform::pin_root(&root).map_err(|_| INVALID.to_string())?;
    let Some(saved) = read_journal(&root)? else {
        return Ok(());
    };
    if !reset_support() {
        return Err("This pending reset requires the supported Windows application.".into());
    }
    validate_identity(&saved, &root)?;
    if matches!(saved["phase"].as_str(), Some("ready" | "deleting")) {
        let reset_id = saved["reset_id"].as_str().ok_or(INVALID)?;
        validate(&saved, &root, reset_id)?;
        let _ = crate::service::request_normal_shutdown();
        if purge(&root, reset_id, TIMEOUT).is_err() {
            // A durable failure is recoverable in the gated renderer. A
            // malformed/lost record must not reopen an ordinary workspace.
            let failed = read_journal(&root)?.ok_or(INVALID)?;
            validate(&failed, &root, reset_id)?;
            if failed["phase"] != "failed" {
                return Err(FAILED.into());
            }
        }
    }
    Ok(())
}

fn run_helper(reset_id: &str, parent_pid: u32) -> Result<(), String> {
    if !reset_support() {
        return Err(INVALID.into());
    }
    let root = StoragePaths::normal()?.root;
    let _guard = platform::pin_root(&root).map_err(|_| INVALID.to_string())?;
    let saved = read_journal(&root)?.ok_or(INVALID)?;
    validate(&saved, &root, reset_id)?;
    // Another helper may already own cleanup. Do not publish failure without
    // its workspace lock, including while the desktop itself is still alive.
    platform::wait_parent(parent_pid, TIMEOUT).map_err(|_| FAILED.to_string())?;
    purge(&root, reset_id, TIMEOUT)
}

fn read_journal(root: &Path) -> Result<Option<serde_json::Value>, String> {
    use std::io::Read;
    let file = match platform::read_control(&root.join(JOURNAL)) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(INVALID.into()),
    };
    let mut bytes = Vec::new();
    file.take(8_388_609)
        .read_to_end(&mut bytes)
        .map_err(|_| INVALID.to_string())?;
    if bytes.len() > 8_388_608 {
        return Err(INVALID.into());
    }
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|_| INVALID.into())
}

fn validate_identity(saved: &serde_json::Value, root: &Path) -> Result<(), String> {
    if saved["format"] != 1
        || saved["home"].as_str() != root.to_str()
        || !saved["reset_id"].as_str().is_some_and(valid_id)
        || !saved["credentials_cleared"].is_boolean()
        || !saved["fingerprint"].as_str().is_some_and(|value| {
            value.len() == 64
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        })
        || !saved["confirmed_at"]
            .as_str()
            .is_some_and(|value| !value.is_empty())
        || !matches!(
            saved["phase"].as_str(),
            Some("cleaning_credentials" | "credential_error" | "ready" | "deleting" | "failed")
        )
    {
        return Err(INVALID.into());
    }
    Ok(())
}

fn validate(saved: &serde_json::Value, root: &Path, reset_id: &str) -> Result<(), String> {
    validate_identity(saved, root)?;
    if !valid_id(reset_id)
        || saved["reset_id"] != reset_id
        || saved["credentials_cleared"] != true
        || !matches!(
            saved["phase"].as_str(),
            Some("ready" | "deleting" | "failed")
        )
    {
        return Err(INVALID.into());
    }
    Ok(())
}

fn update_journal(
    root: &Path,
    saved: &mut serde_json::Value,
    phase: &str,
    detail: &str,
) -> Result<(), String> {
    saved["phase"] = phase.into();
    saved["detail"] = detail.into();
    platform::write_control(root, saved).map_err(|_| FAILED.into())
}

fn purge(root: &Path, reset_id: &str, timeout: Duration) -> Result<(), String> {
    let _guard = platform::pin_root(root).map_err(|_| INVALID.to_string())?;
    let Some(mut saved) = read_journal(root)? else {
        return Err(INVALID.into());
    };
    validate(&saved, root, reset_id)?;
    let deadline = std::time::Instant::now() + timeout;
    // Keep this guard through both successful deletion and failed-journal
    // publication. A waiter must never recreate a journal after its peer has
    // completed. Failure to acquire the lock leaves the current journal alone.
    let lock = retry(deadline, || platform::workspace_lock(&root.join(LOCK)))
        .map_err(|_| FAILED.to_string())?;
    let Some(current) = read_journal(root)? else {
        return Ok(());
    };
    validate(&current, root, reset_id)?;
    saved = current;
    let result = (|| -> Result<(), String> {
        update_journal(
            root,
            &mut saved,
            "deleting",
            "Removing Quantix data. Keep Quantix closed until cleanup finishes.",
        )?;
        retry(deadline, || {
            for entry in std::fs::read_dir(root)? {
                let entry = entry?;
                if entry.file_name() == JOURNAL || entry.file_name() == LOCK {
                    continue;
                }
                platform::remove_entry(&entry.path())?;
            }
            // Deletion can stay pending while a metadata-only OS handle is
            // closing. Do not drop the journal until every old name is gone.
            for entry in std::fs::read_dir(root)? {
                let name = entry?.file_name();
                if name != JOURNAL && name != LOCK {
                    return Err(std::io::ErrorKind::DirectoryNotEmpty.into());
                }
            }
            Ok(())
        })
        .map_err(|_| FAILED.to_string())?;
        // Keep the stable byte-range lock until the journal is gone; no old
        // data remains in this permitted empty coordination file.
        lock.set_len(0).map_err(|_| FAILED.to_string())?;
        lock.sync_all().map_err(|_| FAILED.to_string())?;
        retry(deadline, || platform::remove_entry(&root.join(JOURNAL)))
            .map_err(|_| FAILED.to_string())?;
        Ok(())
    })();
    if result.is_err() {
        update_journal(root, &mut saved, "failed", FAILED)?;
    }
    result
}

fn retry<T>(
    deadline: std::time::Instant,
    mut action: impl FnMut() -> std::io::Result<T>,
) -> std::io::Result<T> {
    loop {
        match action() {
            Ok(value) => return Ok(value),
            Err(error) if std::time::Instant::now() >= deadline => return Err(error),
            Err(_) => std::thread::sleep(
                Duration::from_millis(100)
                    .min(deadline.saturating_duration_since(std::time::Instant::now())),
            ),
        }
    }
}

#[cfg(windows)]
mod platform;

#[cfg(not(windows))]
mod platform {
    use super::*;
    use std::{fs::File, io};
    fn unsupported<T>() -> io::Result<T> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Reset is not available on this operating system.",
        ))
    }
    // No reset journal means ordinary unsupported-platform startup is valid.
    pub fn pin_root(_: &Path) -> io::Result<()> {
        Ok(())
    }
    pub fn read_control(path: &Path) -> io::Result<File> {
        if path.try_exists()? {
            unsupported()
        } else {
            Err(io::ErrorKind::NotFound.into())
        }
    }
    pub fn wait_parent(_: u32, _: Duration) -> io::Result<()> {
        unsupported()
    }
    pub fn write_control(_: &Path, _: &serde_json::Value) -> io::Result<()> {
        unsupported()
    }
    pub fn workspace_lock(_: &Path) -> io::Result<File> {
        unsupported()
    }
    pub fn remove_entry(_: &Path) -> io::Result<()> {
        unsupported()
    }
}

#[cfg(all(test, windows))]
mod tests;
