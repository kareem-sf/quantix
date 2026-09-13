//! Launch splash: a frameless, transparent, click-through window that plays the
//! logo animation (`splash.html`) while the main window loads hidden. The
//! interface calls `finish_splash` once its first screen is ready; the main
//! window appears only after the intro has played and the splash has faded.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager, Runtime, WebviewUrl, WebviewWindowBuilder};

const LABEL: &str = "splash";
/// Logo assembly and wordmark reveal in splash.html.
const INTRO: Duration = Duration::from_millis(3000);
/// The splash fade in splash.html (`.qx-leaving`).
const FADE: Duration = Duration::from_millis(300);
/// Never leave Quantix invisible if the interface fails to report ready.
const FALLBACK: Duration = Duration::from_secs(25);

pub struct SplashState {
    opened: Instant,
    started: AtomicBool,
    done: Mutex<bool>,
    shown: Condvar,
}

pub fn open<R: Runtime>(app: &AppHandle<R>, data_directory: Option<PathBuf>) -> tauri::Result<()> {
    let builder = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("splash.html".into()))
        .title("Quantix")
        .inner_size(600.0, 380.0)
        .center()
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(false);
    // Every window must share the main window's WebView2 data directory.
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    let builder = match data_directory {
        Some(directory) => builder.data_directory(directory),
        None => builder,
    };
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    let _ = data_directory;
    let window = builder.build()?;
    let _ = window.set_ignore_cursor_events(true);
    app.manage(Arc::new(SplashState {
        opened: Instant::now(),
        started: AtomicBool::new(false),
        done: Mutex::new(false),
        shown: Condvar::new(),
    }));
    let fallback = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(FALLBACK);
        reveal(&fallback);
    });
    Ok(())
}

/// Called by the interface when its first screen is ready. Returns once the
/// main window is visible, so the interface can start its own entrance.
#[tauri::command]
pub async fn finish_splash(app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || reveal(&app))
        .await
        .map_err(|_| "Quantix could not show its main window.".to_string())
}

fn reveal<R: Runtime>(app: &AppHandle<R>) {
    let Some(state) = app.try_state::<Arc<SplashState>>().map(|state| state.inner().clone()) else {
        // No splash was opened: just make sure the main window is visible.
        show_main(app);
        return;
    };
    if state.started.swap(true, Ordering::SeqCst) {
        let mut done = state.done.lock().unwrap_or_else(|poison| poison.into_inner());
        while !*done {
            done = state.shown.wait(done).unwrap_or_else(|poison| poison.into_inner());
        }
        return;
    }
    if let Some(remaining) = INTRO.checked_sub(state.opened.elapsed()) {
        std::thread::sleep(remaining);
    }
    let splash = app.get_webview_window(LABEL);
    if let Some(splash) = &splash {
        let _ = splash.eval("document.documentElement.classList.add('qx-leaving')");
        std::thread::sleep(FADE);
    }
    show_main(app);
    if let Some(splash) = splash {
        let _ = splash.close();
    }
    *state.done.lock().unwrap_or_else(|poison| poison.into_inner()) = true;
    state.shown.notify_all();
}

fn show_main<R: Runtime>(app: &AppHandle<R>) {
    if let Some(main) = app.get_webview_window("main") {
        if !main.is_visible().unwrap_or(false) {
            let _ = main.show();
            let _ = main.set_focus();
        }
    }
}
