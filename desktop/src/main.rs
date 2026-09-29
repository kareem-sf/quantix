// The desktop window. In development it loads the Vite server, which also starts the local service.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::webview::NewWindowResponse;
use tauri::{AppHandle, Manager, Url, WebviewWindow, WebviewWindowBuilder};
use tauri_plugin_opener::OpenerExt;
use tauri_plugin_window_state::StateFlags;

fn main() {
    tauri::Builder::default()
        // a second launch brings the open window forward instead of starting a second Quantix on the same data
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        // size, place and maximised; never the frame, which Quantix draws itself
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(StateFlags::SIZE | StateFlags::POSITION | StateFlags::MAXIMIZED)
                .build(),
        )
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init()) // the Windows folder and file pickers
        .plugin(tauri_plugin_notification::init()) // a Windows notification when something needs the engineer
        .setup(|app| {
            let config = app.config().app.windows[0].clone();
            let (navigating, opening) = (app.handle().clone(), app.handle().clone());
            let window = WebviewWindowBuilder::from_config(app.handle(), &config)?
                .on_navigation(move |url| {
                    if !outside(url) {
                        return true;
                    }
                    open_outside(&navigating, url);
                    false
                })
                .on_new_window(move |url, _| {
                    open_outside(&opening, &url);
                    NewWindowResponse::Deny
                })
                .build()?;
            quiet(&window);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Quantix could not open its window");
}

/// A web page or an email address, which belongs in the engineer's own browser or mail program, not in Quantix.
fn outside(url: &Url) -> bool {
    match url.scheme() {
        "http" | "https" => !matches!(
            url.host_str(),
            Some("localhost" | "127.0.0.1" | "tauri.localhost")
        ),
        "mailto" => true,
        _ => false,
    }
}

fn open_outside(app: &AppHandle, url: &Url) {
    if outside(url) {
        let _ = app.opener().open_url(url.as_str(), None::<&str>);
    }
}

/// Quantix is an app, not a web page: no browser keys (reload, print, find, zoom, back). Keys for editing text
/// still work. (wry already hides the link address in the corner.)
#[cfg(windows)]
fn quiet(window: &WebviewWindow) {
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Settings3;
    use windows_core::Interface;

    let _ = window.with_webview(|webview| unsafe {
        let Ok(settings) = webview
            .controller()
            .CoreWebView2()
            .and_then(|core| core.Settings())
        else {
            return;
        };
        if let Ok(settings) = settings.cast::<ICoreWebView2Settings3>() {
            let _ = settings.SetAreBrowserAcceleratorKeysEnabled(false);
        }
    });
}

#[cfg(not(windows))]
fn quiet(_: &WebviewWindow) {}
