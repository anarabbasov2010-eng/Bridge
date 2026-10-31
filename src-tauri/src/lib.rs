mod transport;

use serde::Serialize;
use tauri::{AppHandle, State};
use transport::{
    IncomingFileOffer, PairingInviteInfo, PeerProbe, PeerTransport, SelectedFileInfo,
    TransferReceipt, TransportStatus, TrustedPeer,
};

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
        pairing_ready: true,
    }
}

#[tauri::command]
async fn start_peer_transport(app: AppHandle, state: State<'_, PeerTransport>) -> Result<TransportStatus, String> {
    state.start(app).await
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

#[tauri::command]
fn create_pairing_invite(state: State<'_, PeerTransport>) -> Result<PairingInviteInfo, String> {
    state.create_pairing_invite()
}

#[tauri::command]
async fn pair_with_endpoint(
    app: AppHandle,
    state: State<'_, PeerTransport>,
    remote_endpoint_id: String,
    pairing_code: String,
) -> Result<TrustedPeer, String> {
    state.pair_with_endpoint(app, remote_endpoint_id, pairing_code).await
}

#[tauri::command]
fn get_trusted_devices(state: State<'_, PeerTransport>) -> Result<Vec<TrustedPeer>, String> {
    state.trusted_devices()
}

#[tauri::command]
fn remove_trusted_device(
    app: AppHandle,
    state: State<'_, PeerTransport>,
    endpoint_id: String,
) -> Result<bool, String> {
    state.remove_trusted_device(&app, &endpoint_id)
}

#[tauri::command]
fn pick_files_for_transfer(app: AppHandle, state: State<'_, PeerTransport>) -> Result<Vec<SelectedFileInfo>, String> {
    state.pick_files_for_transfer(&app)
}

#[tauri::command]
fn release_selected_file(state: State<'_, PeerTransport>, selected_file_id: String) -> Result<bool, String> {
    state.release_selected_file(&selected_file_id)
}

#[tauri::command]
async fn send_file_to_peer(
    app: AppHandle,
    state: State<'_, PeerTransport>,
    remote_endpoint_id: String,
    selected_file_id: String,
) -> Result<TransferReceipt, String> {
    state.send_file_to_peer(app, remote_endpoint_id, selected_file_id).await
}

#[tauri::command]
fn get_incoming_file_offers(state: State<'_, PeerTransport>) -> Result<Vec<IncomingFileOffer>, String> {
    state.incoming_file_offers()
}

#[tauri::command]
fn accept_file_offer(
    app: AppHandle,
    state: State<'_, PeerTransport>,
    offer_id: String,
    suggested_file_name: String,
) -> Result<bool, String> {
    state.accept_file_offer(&app, &offer_id, &suggested_file_name)
}

#[tauri::command]
fn reject_file_offer(state: State<'_, PeerTransport>, offer_id: String) -> Result<bool, String> {
    state.reject_file_offer(&offer_id)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_secure_keystore::init())
        .manage(PeerTransport::default())
        .invoke_handler(tauri::generate_handler![
            get_runtime_info,
            start_peer_transport,
            stop_peer_transport,
            get_peer_transport_status,
            probe_peer_endpoint,
            create_pairing_invite,
            pair_with_endpoint,
            get_trusted_devices,
            remove_trusted_device,
            pick_files_for_transfer,
            release_selected_file,
            send_file_to_peer,
            get_incoming_file_offers,
            accept_file_offer,
            reject_file_offer
        ])
        .run(tauri::generate_context!())
        .expect("error while running Bridge");
}
