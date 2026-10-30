use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
struct RuntimeInfo {
    app_version: &'static str,
    platform: &'static str,
    transport_ready: bool,
    pairing_ready: bool,
}

#[tauri::command]
fn get_runtime_info() -> RuntimeInfo {
    RuntimeInfo {
        app_version: env!("CARGO_PKG_VERSION"),
        platform: if cfg!(target_os="android") {"android"} else if cfg!(target_os="windows") {"windows"} else {"other"},
        transport_ready: false,
        pairing_ready: false,
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![get_runtime_info])
        .run(tauri::generate_context!())
        .expect("error while running Bridge");
}
