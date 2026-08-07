mod host;

pub use host::{inspect_tender_office_readiness, QuantixHost, TenderOfficeReadiness};

use tauri::Manager;

mod tauri_commands {
    use super::{
        inspect_tender_office_readiness as inspect_readiness, QuantixHost, TenderOfficeReadiness,
    };

    #[tauri::command]
    pub(super) fn inspect_tender_office_readiness(
        host: tauri::State<'_, QuantixHost>,
    ) -> Result<TenderOfficeReadiness, &'static str> {
        inspect_readiness(host.inner())
    }
}

pub fn configure_tauri_builder<R: tauri::Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder.invoke_handler(tauri::generate_handler![
        tauri_commands::inspect_tender_office_readiness
    ])
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    configure_tauri_builder(tauri::Builder::default())
        .setup(|app| {
            let application_home = app.path().home_dir()?.join(".quantix");
            app.manage(QuantixHost::new(application_home));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
