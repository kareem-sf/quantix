// The desktop window. In development it loads the Vite server, which also starts the local service.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::webview::NewWindowResponse;
use tauri::{AppHandle, Manager, Url, WebviewWindow, WebviewWindowBuilder};
use tauri_plugin_opener::OpenerExt;
use tauri_plugin_window_state::StateFlags;

fn main() {
    #[cfg(all(windows, debug_assertions))]
    if let [_, flag, entry, id] = &std::env::args().collect::<Vec<_>>()[..]
        && flag == START_MENU_ENTRY
    {
        start_menu_entry(entry, id);
        return;
    }
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
        .invoke_handler(tauri::generate_handler![notify])
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
            register(app.handle());
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

/// Windows shows a notification only for an app in the Start menu under the id the notification carries, and names
/// the sender from that entry. The installer gives Quantix its entry; a development build adds its own once, from a
/// second Quantix process: made inside the window's process, the shortcut corrupts its memory.
#[cfg(all(windows, debug_assertions))]
fn register(app: &AppHandle) {
    let (Ok(exe), Ok(roaming)) = (std::env::current_exe(), app.path().data_dir()) else {
        return;
    };
    let entry = roaming.join(r"Microsoft\Windows\Start Menu\Programs\Quantix (development).lnk");
    if !entry.exists() {
        let _ = std::process::Command::new(exe)
            .arg(START_MENU_ENTRY)
            .arg(entry)
            .arg(&app.config().identifier)
            .spawn();
    }
}

#[cfg(all(windows, debug_assertions))]
const START_MENU_ENTRY: &str = "--start-menu-entry";

/// A Start-menu shortcut to this program at `entry`, carrying the app id `id`.
#[cfg(all(windows, debug_assertions))]
fn start_menu_entry(entry: &str, id: &str) {
    use windows::Win32::Storage::EnhancedStorage::PKEY_AppUserModel_ID;
    use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, IPersistFile,
    };
    use windows::Win32::System::Variant::VT_LPWSTR;
    use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
    use windows_core::{HSTRING, Interface, PWSTR};

    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let mut id: Vec<u16> = id.encode_utf16().chain([0]).collect();
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let Ok(link) = CoCreateInstance::<_, IShellLinkW>(&ShellLink, None, CLSCTX_INPROC_SERVER) else {
            return;
        };
        let mut value = PROPVARIANT::default();
        (*value.Anonymous.Anonymous).vt = VT_LPWSTR;
        (*value.Anonymous.Anonymous).Anonymous.pwszVal = PWSTR(id.as_mut_ptr());
        let _ = link
            .SetPath(&HSTRING::from(exe.as_os_str()))
            .and_then(|()| link.cast::<IPropertyStore>())
            .and_then(|store| store.SetValue(&PKEY_AppUserModel_ID, &value).and_then(|()| store.Commit()))
            .and_then(|()| link.cast::<IPersistFile>())
            .and_then(|file| file.Save(&HSTRING::from(entry), true));
    }
}

#[cfg(not(all(windows, debug_assertions)))]
fn register(_: &AppHandle) {}

/// A Windows notification from Quantix. Clicking it brings Quantix forward and opens `open`, a place in the app.
#[cfg(windows)]
#[tauri::command]
fn notify(app: AppHandle, title: String, body: String, open: String) {
    use tauri::Emitter;
    use tauri_winrt_notification::Toast;

    let clicked = app.clone();
    let _ = Toast::new(&app.config().identifier)
        .title(&title)
        .text1(&body)
        .on_activated(move |_| {
            if let Some(window) = clicked.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
            let _ = clicked.emit_to("main", "notification-clicked", &open);
            Ok(())
        })
        .show();
}

#[cfg(not(windows))]
#[tauri::command]
fn notify(_: String, _: String, _: String) {}
