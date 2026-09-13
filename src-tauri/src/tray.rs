use crate::paths::StoragePaths;
use crate::service;
use std::sync::Arc;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Runtime, WebviewWindow, WindowEvent};

pub const CURRENT_WORK_EVENT: &str = "quantix://tray/current-work";
pub const SHUTDOWN_RETRY_MESSAGE: &str =
    "Quantix could not finish closing the local workspace. Keep this window open and try again.";

const TRAY_ID: &str = "quantix-tray";
const OPEN_QUANTIX_ID: &str = "open-quantix";
const CURRENT_WORK_ID: &str = "current-work";
const QUIT_ID: &str = "quit";
const RESET_JOURNAL: &str = "pending-reset.json";
const QUIT_RETRY_TITLE: &str = "Quantix — close could not finish; choose Quit again to retry";
const CLOSE_RETRY_TITLE: &str = "Quantix — close could not finish; close again to retry";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LifecycleAction {
    HideToTray,
    Exit,
}

/// Chooses what a window close request does before any Tauri side effect.
///
/// A close request can hide only after a tray has been created and no reset
/// journal is present. Explicit quit, reset exit and OS shutdown never arrive
/// as a window close request, so they keep Tauri's normal exit path.
pub(crate) fn decide_window_close(tray_ready: bool, reset_pending: bool) -> LifecycleAction {
    if tray_ready && !reset_pending {
        LifecycleAction::HideToTray
    } else {
        LifecycleAction::Exit
    }
}

/// Creates the persistent native tray menu.
///
/// Tauri retains a registered tray handle in its resource table after
/// `build`; keeping this function small makes setup failures leave the main
/// window visible and actionable.
pub fn initialize_tray<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    if reset_journal_present()? {
        return Err(
            "The system tray is disabled while Quantix reset recovery is pending. Keep the main window open while recovery is reviewed.".into(),
        );
    }

    let open_quantix = MenuItem::with_id(app, OPEN_QUANTIX_ID, "Open Quantix", true, None::<&str>)
        .map_err(|error| format!("The Open Quantix tray action could not be created: {error}"))?;
    let current_work = MenuItem::with_id(app, CURRENT_WORK_ID, "Current work", true, None::<&str>)
        .map_err(|error| format!("The Current work tray action could not be created: {error}"))?;
    let separator = PredefinedMenuItem::separator(app)
        .map_err(|error| format!("The Quantix tray menu could not be created: {error}"))?;
    let quit = MenuItem::with_id(app, QUIT_ID, "Quit", true, None::<&str>)
        .map_err(|error| format!("The Quit tray action could not be created: {error}"))?;
    let menu = Menu::with_items(app, &[&open_quantix, &current_work, &separator, &quit])
        .map_err(|error| format!("The Quantix tray menu could not be created: {error}"))?;

    let icon = app.default_window_icon().cloned().ok_or_else(|| {
        "Quantix's application icon is missing, so its tray cannot be shown.".to_string()
    })?;
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .tooltip("Quantix")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id.as_ref() {
            OPEN_QUANTIX_ID => report_restore(app),
            CURRENT_WORK_ID => {
                report_restore(app);
                if let Err(error) = app.emit(CURRENT_WORK_EVENT, ()) {
                    report_tray_failure(app, "current_work_event", &error.to_string());
                }
            }
            QUIT_ID => request_quit(app, QUIT_RETRY_TITLE),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::DoubleClick {
                    button: MouseButton::Left,
                    ..
                }
            ) {
                report_restore(tray.app_handle());
            }
        })
        .build(app)
        .map_err(|error| format!("Quantix's system tray could not be initialized: {error}"))?;

    Ok(())
}

/// Restores the existing window and session retained by Tauri.
pub fn restore_main_window<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "Quantix's main window is unavailable. Reopen Quantix.".to_string())?;
    window
        .unminimize()
        .map_err(|error| format!("Quantix could not restore its main window: {error}"))?;
    window
        .show()
        .map_err(|error| format!("Quantix could not show its main window: {error}"))?;
    window
        .set_focus()
        .map_err(|error| format!("Quantix could not focus its main window: {error}"))?;
    Ok(())
}

/// Installs close handling; only a successful tray initialization enables
/// close-to-tray hiding.
pub fn install_window_close_handler<R: Runtime>(window: &WebviewWindow<R>, tray_ready: bool) {
    let retained_window = window.clone();
    let app = retained_window.app_handle().clone();
    window.on_window_event(move |event| {
        let WindowEvent::CloseRequested { api, .. } = event else {
            return;
        };
        let reset_pending = reset_journal_present().unwrap_or(true);
        match decide_window_close(tray_ready, reset_pending) {
            LifecycleAction::HideToTray => {
                api.prevent_close();
                if let Err(error) = retained_window.hide() {
                    report_tray_failure(
                        retained_window.app_handle(),
                        "window_hide",
                        &error.to_string(),
                    );
                }
            }
            LifecycleAction::Exit if reset_pending => {
                // Reset recovery keeps its own normal ExitRequested path.
            }
            LifecycleAction::Exit if !tray_ready => {
                // Without a tray, keep the window visible until its exact
                // workspace shutdown succeeds or can be retried.
                api.prevent_close();
                request_quit(&app, CLOSE_RETRY_TITLE);
            }
            LifecycleAction::Exit => {
                // This branch is reserved for explicit exits, which do not
                // arrive as a window CloseRequested event.
            }
        }
    });
}

fn execute_exit_after_shutdown<Shutdown, Exit>(shutdown: Shutdown, exit: Exit) -> Result<(), String>
where
    Shutdown: FnOnce() -> Result<(), String>,
    Exit: FnOnce(),
{
    shutdown()?;
    exit();
    Ok(())
}

fn request_quit<R: Runtime>(app: &AppHandle<R>, retry_title: &str) {
    let Some(state) = app.try_state::<service::ShutdownState>() else {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.show();
            let _ = window.set_title(retry_title);
        }
        return;
    };
    if !state.try_begin() {
        return;
    }
    let worker_app = app.clone();
    let retry_title = retry_title.to_string();
    tauri::async_runtime::spawn(async move {
        let result = match tauri::async_runtime::spawn_blocking({
            let app = worker_app.clone();
            move || service::shutdown_for_exit(&app)
        })
        .await
        {
            Ok(result) => result,
            Err(_) => Err(SHUTDOWN_RETRY_MESSAGE.to_string()),
        };
        let exit_result = execute_exit_after_shutdown(|| result, || worker_app.exit(0));
        if let Err(error) = exit_result {
            report_tray_failure(&worker_app, "workspace_shutdown", &error);
            if let Some(window) = worker_app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
                let _ = window.set_title(&retry_title);
            }
        }
        if let Some(state) = worker_app.try_state::<service::ShutdownState>() {
            state.finish();
        }
    });
}

fn report_restore<R: Runtime>(app: &AppHandle<R>) {
    if let Err(error) = restore_main_window(app) {
        report_tray_failure(app, "window_restore", &error);
    }
}

fn report_tray_failure<R: Runtime>(app: &AppHandle<R>, operation: &str, _detail: &str) {
    if let Some(logger) = app
        .try_state::<Arc<crate::diagnostics::DiagnosticLogger>>()
        .map(|state| state.inner().clone())
    {
        logger.record_failure("native_tray_action_failed", operation, "tray_error", None);
    }
}

fn reset_journal_present() -> Result<bool, String> {
    let root = StoragePaths::normal()?.root;
    root.join(RESET_JOURNAL)
        .try_exists()
        .map_err(|_| "Quantix could not check reset recovery state.".to_string())
}

#[cfg(test)]
mod tests {
    use super::{decide_window_close, execute_exit_after_shutdown, LifecycleAction};
    use std::cell::Cell;

    #[test]
    fn ordinary_close_hides_after_tray_initializes() {
        assert_eq!(decide_window_close(true, false), LifecycleAction::HideToTray);
    }

    #[test]
    fn pending_reset_window_close_bypasses_tray() {
        assert_eq!(decide_window_close(true, true), LifecycleAction::Exit);
    }

    #[test]
    fn missing_tray_does_not_hide_the_only_window() {
        assert_eq!(decide_window_close(false, false), LifecycleAction::Exit);
    }

    #[test]
    fn failed_shutdown_does_not_run_exit_action() {
        let exited = Cell::new(false);
        let result = execute_exit_after_shutdown(
            || Err("synthetic shutdown rejection".to_string()),
            || exited.set(true),
        );
        assert!(result.is_err());
        assert!(!exited.get());
    }

    #[test]
    fn successful_shutdown_runs_exit_action_once() {
        let exits = Cell::new(0);
        execute_exit_after_shutdown(|| Ok(()), || exits.set(exits.get() + 1)).unwrap();
        assert_eq!(exits.get(), 1);
    }
}
