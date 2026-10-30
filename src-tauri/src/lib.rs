mod transport;

use serde::Serialize;
use tauri::State;
use transport::{PeerProbe, PeerTransport, TransportStatus};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
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
        platform: if cfg!(target_os = "android") {
            "android"
        } else if cfg!(target_os = "windows") {
            "windows"
        } else {
            "other"
        },
        transport_ready: true,
        pairing_ready: false,
    }
}

#[tauri::command]
async fn start_peer_transport(state: State<'_, PeerTransport>) -> Result<TransportStatus, String> {
    state.start().await
}

#[tauri::command]
async fn stop_peer_transport(state: State<'_, PeerTransport>) -> Result<TransportStatus, String> {
    state.stop().await
}

#[tauri::command]
fn get_peer_transport_status(state: State<'_, PeerTransport>) -> Result<TransportStatus, String> {
    state.status()
}

#[tauri::command]
async fn probe_peer_endpoint(
    state: State<'_, PeerTransport>,
    remote_endpoint_id: String,
) -> Result<PeerProbe, String> {
    state.probe_peer(remote_endpoint_id).await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(PeerTransport::default())
        .invoke_handler(tauri::generate_handler![
            get_runtime_info,
            start_peer_transport,
            stop_peer_transport,
            get_peer_transport_status,
            probe_peer_endpoint
        ])
        .run(tauri::generate_context!())
        .expect("error while running Bridge");
}
