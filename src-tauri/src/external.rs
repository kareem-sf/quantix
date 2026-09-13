use crate::diagnostics::DiagnosticLogger;
use serde_json::json;
use std::sync::Arc;
use std::time::Instant;
use tauri::Manager;
use tauri_plugin_opener::OpenerExt;

fn diagnostics<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Option<Arc<DiagnosticLogger>> {
    app.try_state::<Arc<DiagnosticLogger>>().map(|state| state.inner().clone())
}

fn record_handoff(logger: &Option<Arc<DiagnosticLogger>>, phase: &str, outcome: &str, started: Instant, error_code: Option<&str>) {
    if let Some(logger) = logger {
        let mut fields = json!({"phase": phase, "outcome": outcome, "duration_ms": started.elapsed().as_millis().min(i32::MAX as u128) as u64});
        if let Some(error_code) = error_code { fields["error_code"] = json!(error_code); }
        logger.record("external_browser_handoff", if outcome == "failed" { "error" } else { "info" }, fields);
    }
}

fn web_destination(url: &str) -> Result<url::Url, String> {
    if url.len() > 8192 || url.chars().any(|character| character.is_control()) {
        return Err("This web link is invalid.".into());
    }
    let destination = url::Url::parse(url).map_err(|_| "This web link is invalid.")?;
    if !matches!(destination.scheme(), "http" | "https")
        || destination.host_str().is_none()
        || !destination.username().is_empty()
        || destination.password().is_some()
    {
        return Err("Only web links without embedded account credentials can be opened.".into());
    }
    Ok(destination)
}

#[tauri::command]
pub fn open_external_url(app: tauri::AppHandle, url: String) -> Result<(), String> {
    let logger = diagnostics(&app);
    let started = Instant::now();
    let destination = match web_destination(&url) {
        Ok(destination) => destination,
        Err(error) => {
            record_handoff(&logger, "url_validation", "failed", started, Some("EINVAL"));
            return Err(error);
        }
    };
    match app.opener().open_url(destination.as_str(), None::<&str>) {
        Ok(()) => {
            record_handoff(&logger, "default_browser", "success", started, None);
            Ok(())
        }
        Err(_) => {
            record_handoff(&logger, "default_browser", "failed", started, Some("BROWSER_HANDOFF"));
            Err("The default browser could not open this link. Copy its address into your browser.".to_string())
        }
    }
}

#[cfg(windows)]
fn prepare_windows_browser(destination: &url::Url, logger: &Option<Arc<DiagnosticLogger>>, started: Instant) -> Result<(), String> {
    use std::mem::size_of;
    use std::ptr::null;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT};
    use windows_sys::Win32::System::Com::{
        CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
    };
    use windows_sys::Win32::System::Threading::{GetExitCodeProcess, WaitForInputIdle, WaitForSingleObject};
    use windows_sys::Win32::UI::Shell::{
        ShellExecuteExW, SHELLEXECUTEINFOW, SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC,
        SEE_MASK_NOCLOSEPROCESS,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    // Shell extensions may require an STA. This function runs on a fresh
    // thread so another blocking task's COM apartment cannot conflict with it.
    let initialized = unsafe {
        CoInitializeEx(null(), (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) as u32)
    };
    if initialized < 0 {
        record_handoff(logger, "com_initialize", "failed", started, Some("COM_INIT"));
        return Err("Windows could not prepare the default browser. Open your browser normally, then try sign-in again.".into());
    }
    struct ComApartment;
    impl Drop for ComApartment {
        fn drop(&mut self) { unsafe { CoUninitialize(); } }
    }
    let _apartment = ComApartment;
    struct ProcessHandle(HANDLE);
    impl Drop for ProcessHandle {
        fn drop(&mut self) { unsafe { CloseHandle(self.0); } }
    }

    let verb: Vec<u16> = "open".encode_utf16().chain(Some(0)).collect();
    let target: Vec<u16> = destination.as_str().encode_utf16().chain(Some(0)).collect();
    let mut launch = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI,
        lpVerb: verb.as_ptr(),
        lpFile: target.as_ptr(),
        nShow: SW_SHOWNORMAL,
        ..Default::default()
    };
    // No executable, profile arguments or environment override: Windows uses
    // the user's HTTP(S) association from the normal desktop process, outside
    // every AI process job. NOASYNC is not a page-load guarantee for URLs.
    if unsafe { ShellExecuteExW(&mut launch) } == 0 {
        record_handoff(logger, "shell_execute", "failed", started, Some("SHELL_EXECUTE"));
        return Err("The default browser could not open the sign-in page. Check your default browser in Windows Settings, then try again.".into());
    }
    if launch.hProcess.is_null() {
        // An existing browser or delegated association can accept the URL
        // without returning a process handle. This proves OS handoff only.
        record_handoff(logger, "shell_execute", "success", started, None);
        return Ok(());
    }
    let process = ProcessHandle(launch.hProcess);
    match unsafe { WaitForInputIdle(process.0, 15_000) } {
        0 => {
            record_handoff(logger, "browser_input_idle", "success", started, None);
            Ok(())
        }
        WAIT_TIMEOUT => {
            record_handoff(logger, "browser_input_idle", "failed", started, Some("ETIMEDOUT"));
            Err("The default browser is taking longer to start. Let it finish opening, then try sign-in again.".into())
        }
        _ => {
            // Some associations return a short-lived launcher that hands the
            // URL to an existing browser and exits instead of creating a GUI.
            let mut exit_code = 1;
            if unsafe { WaitForSingleObject(process.0, 0) } == WAIT_OBJECT_0
                && unsafe { GetExitCodeProcess(process.0, &mut exit_code) } != 0
                && exit_code == 0
            {
                record_handoff(logger, "browser_launcher", "success", started, None);
                return Ok(());
            }
            record_handoff(logger, "browser_launcher", "failed", started, Some("BROWSER_START"));
            Err("Windows opened the browser request but could not confirm its startup. Open your browser normally, then try sign-in again.".into())
        }
    }
    // Dropping the observation handle never terminates the user's browser.
}

#[tauri::command]
pub async fn prepare_sign_in_browser(app: tauri::AppHandle, url: String) -> Result<(), String> {
    let logger = diagnostics(&app);
    let started = Instant::now();
    let destination = match web_destination(&url) {
        Ok(destination) => destination,
        Err(error) => {
            record_handoff(&logger, "url_validation", "failed", started, Some("EINVAL"));
            return Err(error);
        }
    };
    let event_logger = logger.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        #[cfg(windows)]
        {
            let _ = app;
            let (sender, receiver) = std::sync::mpsc::sync_channel(1);
            let launch_logger = event_logger.clone();
            let thread_logger = launch_logger.clone();
            let _launch_thread = std::thread::Builder::new().name("quantix-sign-in-browser".into())
                .spawn(move || { let _ = sender.send(prepare_windows_browser(&destination, &thread_logger, started)); })
                .map_err(|_| {
                    record_handoff(&launch_logger, "browser_thread", "failed", started, Some("THREAD_SPAWN"));
                    "Quantix could not prepare the default browser. Try sign-in again.".to_string()
                })?;
            // A shell extension can stall before returning a process handle.
            // Bound the caller too; the desktop-owned launch may still finish
            // later, and is never terminated along with a provider's work.
            receiver.recv_timeout(std::time::Duration::from_secs(30))
                .map_err(|_| {
                    record_handoff(&launch_logger, "browser_handoff_response", "failed", started, Some("ETIMEDOUT"));
                    "Windows has not confirmed the browser handoff. Wait for your browser to open, then try sign-in again.".to_string()
                })?
        }
        #[cfg(not(windows))]
        {
            // The platform opener confirms handoff, not that a page loaded.
            match app.opener().open_url(destination.as_str(), None::<&str>) {
                Ok(()) => {
                    record_handoff(&event_logger, "default_browser", "success", started, None);
                    Ok(())
                }
                Err(_) => {
                    record_handoff(&event_logger, "default_browser", "failed", started, Some("BROWSER_HANDOFF"));
                    Err("The default browser could not open the sign-in page. Open your browser normally, then try again.".to_string())
                }
            }
        }
    }).await;
    match result {
        Ok(result) => result,
        Err(_) => {
            record_handoff(&logger, "browser_task", "failed", started, Some("TASK_JOIN"));
            Err("Quantix could not finish preparing the default browser. Try sign-in again.".into())
        }
    }
}
