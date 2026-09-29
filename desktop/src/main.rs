// The desktop window. In development it loads the Vite server, which also starts the local service.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::webview::NewWindowResponse;
use tauri::{AppHandle, Manager, Url, WebviewWindow, WebviewWindowBuilder};
use tauri_plugin_opener::OpenerExt;
use tauri_plugin_window_state::StateFlags;

fn main() {
    #[cfg(all(windows, debug_assertions))]
    if let [_, flag, entry, id, icon] = &std::env::args().collect::<Vec<_>>()[..]
        && flag == START_MENU_ENTRY
    {
        start_menu_entry(entry, id, icon);
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
            taskbar_icon(&window);
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

/// The taskbar shows a window's large icon. Tauri gives its windows only the small one, so the taskbar fell back to
/// a picture of the program Windows had cached, the old icon. Give the window the program's own icon at that size.
#[cfg(windows)]
fn taskbar_icon(window: &WebviewWindow) {
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetSystemMetrics, ICON_BIG, IMAGE_ICON, LR_DEFAULTCOLOR, LoadImageW, SM_CXICON, SM_CYICON, SendMessageW,
        WM_SETICON,
    };
    use windows_core::PCWSTR;

    const APP_ICON: u16 = 32512; // where Tauri's build puts the app icon
    let Ok(hwnd) = window.hwnd() else {
        return;
    };
    unsafe {
        let Ok(module) = GetModuleHandleW(None) else {
            return;
        };
        let (width, height) = (GetSystemMetrics(SM_CXICON), GetSystemMetrics(SM_CYICON));
        let name = PCWSTR(APP_ICON as usize as *const u16);
        if let Ok(icon) = LoadImageW(Some(module.into()), name, IMAGE_ICON, width, height, LR_DEFAULTCOLOR) {
            let hwnd = HWND(hwnd.0 as _);
            SendMessageW(hwnd, WM_SETICON, Some(WPARAM(ICON_BIG as usize)), Some(LPARAM(icon.0 as isize)));
        }
    }
}

#[cfg(not(windows))]
fn taskbar_icon(_: &WebviewWindow) {}

/// Windows shows a notification only for an app in the Start menu under the id the notification carries, and names
/// the sender from that entry. The installer gives Quantix its entry; a development build adds its own, from a second
/// Quantix process: made inside the window's process, the shortcut corrupts its memory.
///
/// The taskbar draws the window with that entry's icon, and Windows keeps an icon cached by its file's path however
/// often the file changes. So the entry takes its icon from a copy named after the icon's contents, and is made again
/// whenever the icon changes.
#[cfg(all(windows, debug_assertions))]
fn register(app: &AppHandle) {
    const ICON: &[u8] = include_bytes!("../icons/icon.ico");
    let (Ok(exe), Ok(roaming), Ok(local)) =
        (std::env::current_exe(), app.path().data_dir(), app.path().app_local_data_dir())
    else {
        return;
    };
    let entry = roaming.join(r"Microsoft\Windows\Start Menu\Programs\Quantix (development).lnk");
    // FNV-1a: a name that changes with the icon
    let hash = ICON.iter().fold(0xcbf29ce484222325_u64, |h, b| (h ^ u64::from(*b)).wrapping_mul(0x100000001b3));
    let icon = local.join(format!("start-menu-icon-{hash:016x}.ico"));
    let new_icon = !icon.exists();
    if new_icon {
        if let Ok(old) = std::fs::read_dir(&local) {
            for file in old.flatten().filter(|f| f.file_name().to_string_lossy().starts_with("start-menu-icon-")) {
                let _ = std::fs::remove_file(file.path());
            }
        }
        if std::fs::create_dir_all(&local).and_then(|()| std::fs::write(&icon, ICON)).is_err() {
            return;
        }
    }
    if new_icon || !entry.exists() {
        let _ = std::process::Command::new(exe)
            .arg(START_MENU_ENTRY)
            .arg(entry)
            .arg(&app.config().identifier)
            .arg(icon)
            .spawn();
    }
}

#[cfg(all(windows, debug_assertions))]
const START_MENU_ENTRY: &str = "--start-menu-entry";

/// A Start-menu shortcut to this program at `entry`, carrying the app id `id` and drawn with the icon file `icon`.
#[cfg(all(windows, debug_assertions))]
fn start_menu_entry(entry: &str, id: &str, icon: &str) {
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
            .and_then(|()| link.SetIconLocation(&HSTRING::from(icon), 0))
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
