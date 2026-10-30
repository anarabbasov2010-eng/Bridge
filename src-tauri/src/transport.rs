use iroh::{endpoint::presets, Endpoint, EndpointId, endpoint::Connection};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    io::SeekFrom,
    path::PathBuf,
    str::FromStr,
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use subtle::ConstantTimeEq;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_fs::{FilePath, FsExt, OpenOptions as FsOpenOptions};
use tokio::{io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt}, sync::{oneshot, Semaphore}};
use uuid::Uuid;

pub const BRIDGE_ALPN: &[u8] = b"app.bridge.control/1";
const MAX_CONTROL_FRAME: usize = 16 * 1024;
const PROTOCOL_VERSION: u16 = 1;
const PAIRING_CODE_TTL: Duration = Duration::from_secs(180);
const MAX_PAIRING_ATTEMPTS: u8 = 5;
const MAX_FILE_BYTES: u64 = 5 * 1024 * 1024 * 1024;
const MAX_FILE_NAME_BYTES: usize = 240;
const FILE_CHUNK_SIZE: usize = 128 * 1024;
const FILE_OFFER_TTL: Duration = Duration::from_secs(120);
const STREAM_IDLE_TIMEOUT: Duration = Duration::from_secs(90);

#[derive(Default)]
pub struct PeerTransport {
    endpoint: Mutex<Option<Endpoint>>,
    pairing_invite: Arc<Mutex<Option<PairingInvite>>>,
    trusted_peers: Arc<Mutex<HashMap<String, TrustedPeer>>>,
    selected_files: Mutex<HashMap<String, SelectedFile>>,
    pending_file_offers: Arc<Mutex<HashMap<String, PendingFileOffer>>>,
}

#[derive(Clone)]
struct SelectedFile {
    path: FilePath,
    file_name: String,
    size_bytes: u64,
}

struct PairingInvite {
    code_hash: [u8; 32],
    expires_at: Instant,
    attempts_remaining: u8,
}

struct PendingFileOffer {
    info: IncomingFileOffer,
    destination_tx: oneshot::Sender<Option<FilePath>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransportStatus {
    pub running: bool,
    pub endpoint_id: Option<String>,
    pub identity_persisted: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerProbe {
    pub connected: bool,
    pub remote_endpoint_id: String,
    pub protocol_version: u16,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingInviteInfo {
    pub endpoint_id: String,
    pub code: String,
    pub expires_in_seconds: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrustedPeer {
    pub endpoint_id: String,
    pub paired_at_unix: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectedFileInfo {
    pub id: String,
    pub file_name: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IncomingFileOffer {
    pub offer_id: String,
    pub sender_endpoint_id: String,
    pub file_name: String,
    pub size_bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferProgress {
    pub transfer_id: String,
    pub file_name: String,
    pub direction: String,
    pub transferred_bytes: u64,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferReceipt {
    pub transfer_id: String,
    pub file_name: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub remote_endpoint_id: String,
    pub direction: String,
    pub completed_at_unix: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ControlRequest {
    message_type: String,
    protocol_version: u16,
    #[serde(default)]
    pairing_code: Option<String>,
    #[serde(default)]
    transfer_id: Option<String>,
    #[serde(default)]
    file_name: Option<String>,
    #[serde(default)]
    size_bytes: Option<u64>,
    #[serde(default)]
    sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ControlResponse {
    message_type: String,
    protocol_version: u16,
    endpoint_id: String,
    message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileDecision {
    message_type: String,
    protocol_version: u16,
    transfer_id: String,
    accepted: bool,
    message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileTransferResult {
    message_type: String,
    protocol_version: u16,
    transfer_id: String,
    success: bool,
    file_name: String,
    size_bytes: u64,
    sha256: String,
    message: String,
}

impl TrustedPeer {
    fn new(endpoint_id: String) -> Self {
        let paired_at_unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Self { endpoint_id, paired_at_unix }
    }
}

impl PeerTransport {
    pub async fn start(&self, app: AppHandle) -> Result<TransportStatus, String> {
        if let Some(status) = self.status_if_running()? {
            return Ok(status);
        }

        let endpoint = Endpoint::builder(presets::N0)
            .alpns(vec![BRIDGE_ALPN.to_vec()])
            .bind()
            .await
            .map_err(|error| format!("Unable to start Bridge peer transport: {error}"))?;
        let endpoint_id = endpoint.id().to_string();

        let already_running = {
            let mut slot = self.endpoint.lock()
                .map_err(|_| "Bridge transport state is unavailable".to_string())?;
            if let Some(existing) = slot.as_ref() {
                Some(TransportStatus {
                    running: true,
                    endpoint_id: Some(existing.id().to_string()),
                    identity_persisted: false,
                })
            } else {
                *slot = Some(endpoint.clone());
                None
            }
        };

        if let Some(status) = already_running {
            endpoint.close().await;
            return Ok(status);
        }

        tokio::spawn(run_accept_loop(
            app,
            endpoint.clone(),
            Arc::clone(&self.pairing_invite),
            Arc::clone(&self.trusted_peers),
            Arc::clone(&self.pending_file_offers),
        ));

        Ok(TransportStatus {
            running: true,
            endpoint_id: Some(endpoint_id),
            identity_persisted: false,
        })
    }

    pub async fn stop(&self) -> Result<TransportStatus, String> {
        let endpoint = self.endpoint.lock()
            .map_err(|_| "Bridge transport state is unavailable".to_string())?
            .take();

        *self.pairing_invite.lock()
            .map_err(|_| "Bridge pairing state is unavailable".to_string())? = None;
        self.trusted_peers.lock()
            .map_err(|_| "Bridge trusted-device state is unavailable".to_string())?
            .clear();
        self.pending_file_offers.lock()
            .map_err(|_| "Bridge incoming transfer state is unavailable".to_string())?
            .clear();
        self.selected_files.lock()
            .map_err(|_| "Bridge selected file state is unavailable".to_string())?
            .clear();

        if let Some(endpoint) = endpoint {
            endpoint.close().await;
        }

        Ok(TransportStatus {
            running: false,
            endpoint_id: None,
            identity_persisted: false,
        })
    }

    pub fn status(&self) -> Result<TransportStatus, String> {
        self.endpoint.lock()
            .map_err(|_| "Bridge transport state is unavailable".to_string())
            .map(|slot| match slot.as_ref() {
                Some(endpoint) => TransportStatus {
                    running: true,
                    endpoint_id: Some(endpoint.id().to_string()),
                    identity_persisted: false,
                },
                None => TransportStatus {
                    running: false,
                    endpoint_id: None,
                    identity_persisted: false,
                },
            })
    }

    pub fn pick_files_for_transfer(&self, app: &AppHandle) -> Result<Vec<SelectedFileInfo>, String> {
        let Some(paths) = app.dialog().file().blocking_pick_files() else {
            return Ok(Vec::new());
        };

        let mut selected = Vec::with_capacity(paths.len());
        for path in paths {
            let mut options = FsOpenOptions::new();
            options.read(true);
            let file = app.fs().open(path.clone(), options)
                .map_err(|error| format!("Could not open selected file: {error}"))?;
            let size_bytes = file.metadata()
                .map_err(|error| format!("Could not read selected file metadata: {error}"))?
                .len();

            if size_bytes > MAX_FILE_BYTES {
                return Err(format!("{} exceeds the 5 GiB per-file limit.", file_name_from_path(&path)));
            }

            let file_name = safe_file_name(&file_name_from_path(&path))?;
            let id = Uuid::new_v4().to_string();
            self.selected_files.lock()
                .map_err(|_| "Bridge selected file state is unavailable".to_string())?
                .insert(id.clone(), SelectedFile {
                    path,
                    file_name: file_name.clone(),
                    size_bytes,
                });

            selected.push(SelectedFileInfo { id, file_name, size_bytes });
        }

        Ok(selected)
    }

    pub fn release_selected_file(&self, selected_file_id: &str) -> Result<bool, String> {
        self.selected_files.lock()
            .map_err(|_| "Bridge selected file state is unavailable".to_string())
            .map(|mut files| files.remove(selected_file_id).is_some())
    }

    pub fn incoming_file_offers(&self) -> Result<Vec<IncomingFileOffer>, String> {
        let mut offers: Vec<IncomingFileOffer> = self.pending_file_offers.lock()
            .map_err(|_| "Bridge incoming transfer state is unavailable".to_string())?
            .values()
            .map(|offer| offer.info.clone())
            .collect();
        offers.sort_by(|left, right| left.file_name.cmp(&right.file_name));
        Ok(offers)
    }

    pub fn accept_file_offer(
        &self,
        app: &AppHandle,
        offer_id: &str,
        suggested_file_name: &str,
    ) -> Result<bool, String> {
        let safe_name = safe_file_name(suggested_file_name)?;
        let destination = app.dialog().file()
            .set_title("Save incoming Bridge file")
            .set_file_name(safe_name)
            .blocking_save_file();

        let Some(destination) = destination else {
            self.reject_file_offer(offer_id)?;
            return Ok(false);
        };

        let pending = self.pending_file_offers.lock()
            .map_err(|_| "Bridge incoming transfer state is unavailable".to_string())?
            .remove(offer_id)
            .ok_or_else(|| "This file offer has expired or was already handled.".to_string())?;

        pending.destination_tx.send(Some(destination))
            .map_err(|_| "The sender has disconnected or the offer expired.".to_string())?;
        Ok(true)
    }

    pub fn reject_file_offer(&self, offer_id: &str) -> Result<bool, String> {
        let pending = self.pending_file_offers.lock()
            .map_err(|_| "Bridge incoming transfer state is unavailable".to_string())?
            .remove(offer_id);
        if let Some(pending) = pending {
            let _ = pending.destination_tx.send(None);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn create_pairing_invite(&self) -> Result<PairingInviteInfo, String> {
        let endpoint_id = self.endpoint.lock()
            .map_err(|_| "Bridge transport state is unavailable".to_string())?
            .as_ref()
            .map(|endpoint| endpoint.id().to_string())
            .ok_or_else(|| "Start the peer network engine before creating an invite.".to_string())?;

        let code = format!("{:06}", rand::random_range(0..1_000_000u32));
        let code_hash: [u8; 32] = Sha256::digest(code.as_bytes()).into();
        *self.pairing_invite.lock()
            .map_err(|_| "Bridge pairing state is unavailable".to_string())? = Some(PairingInvite {
                code_hash,
                expires_at: Instant::now() + PAIRING_CODE_TTL,
                attempts_remaining: MAX_PAIRING_ATTEMPTS,
            });

        Ok(PairingInviteInfo {
            endpoint_id,
            code,
            expires_in_seconds: PAIRING_CODE_TTL.as_secs(),
        })
    }

    pub async fn probe_peer(&self, remote_endpoint_id: String) -> Result<PeerProbe, String> {
        let endpoint = self.endpoint_snapshot()?;
        let remote_id = parse_endpoint_id(&remote_endpoint_id)?;
        let connection = connect_to_peer(&endpoint, remote_id).await?;

        let (mut send, mut recv) = connection.open_bi().await
            .map_err(|error| format!("Could not open the Bridge control stream: {error}"))?;
        let request = encode_control_request("bridge.hello", None)?;
        send.write_all(&request).await
            .map_err(|error| format!("Could not send the Bridge handshake: {error}"))?;
        send.finish()
            .map_err(|error| format!("Could not finish the Bridge handshake: {error}"))?;

        let response_bytes = tokio::time::timeout(STREAM_IDLE_TIMEOUT, recv.read_to_end(MAX_CONTROL_FRAME))
            .await
            .map_err(|_| "Timed out waiting for the Bridge handshake.".to_string())?
            .map_err(|error| format!("Could not read the Bridge handshake response: {error}"))?;
        let response: ControlResponse = serde_json::from_slice(&response_bytes)
            .map_err(|_| "The remote endpoint returned an invalid Bridge handshake.".to_string())?;

        if response.message_type != "bridge.hello_ack" || response.protocol_version != PROTOCOL_VERSION {
            connection.close(2u32.into(), b"unsupported Bridge protocol");
            return Err("The remote device uses an incompatible Bridge protocol version.".to_string());
        }
        if response.endpoint_id != remote_id.to_string() {
            connection.close(3u32.into(), b"peer identity mismatch");
            return Err("The remote handshake identity did not match the encrypted connection.".to_string());
        }

        connection.close(0u32.into(), b"Bridge connectivity check complete");
        Ok(PeerProbe {
            connected: true,
            remote_endpoint_id: response.endpoint_id,
            protocol_version: response.protocol_version,
            message: "Encrypted Bridge control handshake completed.".to_string(),
        })
    }

    pub async fn pair_with_endpoint(
        &self,
        remote_endpoint_id: String,
        pairing_code: String,
    ) -> Result<TrustedPeer, String> {
        let endpoint = self.endpoint_snapshot()?;
        let remote_id = parse_endpoint_id(&remote_endpoint_id)?;
        let code = pairing_code.trim();

        if code.len() != 6 || !code.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err("Enter the six-digit pairing code shown on the other device.".to_string());
        }

        let connection = connect_to_peer(&endpoint, remote_id).await?;
        let (mut send, mut recv) = connection.open_bi().await
            .map_err(|error| format!("Could not open the Bridge pairing stream: {error}"))?;
        let request = encode_control_request("bridge.pair", Some(code.to_string()))?;
        send.write_all(&request).await
            .map_err(|error| format!("Could not send the pairing request: {error}"))?;
        send.finish()
            .map_err(|error| format!("Could not finish the pairing request: {error}"))?;

        let response_bytes = tokio::time::timeout(STREAM_IDLE_TIMEOUT, recv.read_to_end(MAX_CONTROL_FRAME))
            .await
            .map_err(|_| "Timed out waiting for the pairing response.".to_string())?
            .map_err(|error| format!("Could not read the pairing response: {error}"))?;
        let response: ControlResponse = serde_json::from_slice(&response_bytes)
            .map_err(|_| "The remote device returned an invalid pairing response.".to_string())?;

        if response.message_type != "bridge.pair_ack" || response.protocol_version != PROTOCOL_VERSION {
            connection.close(2u32.into(), b"pairing rejected");
            return Err(response.message);
        }
        if response.endpoint_id != remote_id.to_string() {
            connection.close(3u32.into(), b"peer identity mismatch");
            return Err("The pairing response identity did not match the encrypted connection.".to_string());
        }

        connection.close(0u32.into(), b"Bridge pairing complete");
        let trusted = TrustedPeer::new(remote_id.to_string());
        self.trusted_peers.lock()
            .map_err(|_| "Bridge trusted-device state is unavailable".to_string())?
            .insert(trusted.endpoint_id.clone(), trusted.clone());
        Ok(trusted)
    }

    pub fn trusted_devices(&self) -> Result<Vec<TrustedPeer>, String> {
        let mut peers: Vec<TrustedPeer> = self.trusted_peers.lock()
            .map_err(|_| "Bridge trusted-device state is unavailable".to_string())?
            .values()
            .cloned()
            .collect();
        peers.sort_by(|left, right| right.paired_at_unix.cmp(&left.paired_at_unix));
        Ok(peers)
    }

    pub fn remove_trusted_device(&self, endpoint_id: &str) -> Result<bool, String> {
        self.trusted_peers.lock()
            .map_err(|_| "Bridge trusted-device state is unavailable".to_string())
            .map(|mut peers| peers.remove(endpoint_id).is_some())
    }

    pub async fn send_file_to_peer(
        &self,
        app: AppHandle,
        remote_endpoint_id: String,
        selected_file_id: String,
    ) -> Result<TransferReceipt, String> {
        let remote_id = parse_endpoint_id(&remote_endpoint_id)?;
        let is_trusted = self.trusted_peers.lock()
            .map_err(|_| "Bridge trusted-device state is unavailable".to_string())?
            .contains_key(&remote_id.to_string());
        if !is_trusted {
            return Err("Pair and trust this device before sending files.".to_string());
        }

        let endpoint = self.endpoint_snapshot()?;
        let selected = self.selected_files.lock()
            .map_err(|_| "Bridge selected file state is unavailable".to_string())?
            .get(&selected_file_id)
            .cloned()
            .ok_or_else(|| "This file selection has expired. Choose the file again.".to_string())?;

        let mut options = FsOpenOptions::new();
        options.read(true);
        let native_file = app.fs().open(selected.path.clone(), options)
            .map_err(|error| format!("Could not reopen {}: {error}", selected.file_name))?;
        let metadata_size = native_file.metadata()
            .map_err(|error| format!("Could not read {} metadata: {error}", selected.file_name))?
            .len();
        if metadata_size != selected.size_bytes {
            return Err("The selected file changed after it was added to the queue. Select it again.".to_string());
        }
        if metadata_size > MAX_FILE_BYTES {
            return Err("This file exceeds the 5 GiB transfer limit.".to_string());
        }

        // Hash once before sending the offer, then rewind and stream the file.
        // No file bytes leave the device until the recipient explicitly accepts.
        let mut file = tokio::fs::File::from_std(native_file);
        let mut hasher = Sha256::new();
        let mut buffer = vec![0u8; FILE_CHUNK_SIZE];
        let mut hashed_bytes = 0u64;
        loop {
            let count = tokio::time::timeout(STREAM_IDLE_TIMEOUT, file.read(&mut buffer))
                .await
                .map_err(|_| "Timed out while reading the selected file.".to_string())?
                .map_err(|error| format!("Could not read {}: {error}", selected.file_name))?;
            if count == 0 { break; }
            hashed_bytes += count as u64;
            hasher.update(&buffer[..count]);
        }
        if hashed_bytes != selected.size_bytes {
            return Err("The selected file size changed while it was being prepared.".to_string());
        }
        let sha256 = format!("{:x}", hasher.finalize());
        file.seek(SeekFrom::Start(0)).await
            .map_err(|error| format!("Could not rewind {}: {error}", selected.file_name))?;

        let transfer_id = Uuid::new_v4().to_string();
        let connection = connect_to_peer(&endpoint, remote_id).await?;
        let request = FileOfferRequest {
            message_type: "bridge.file_offer".to_string(),
            protocol_version: PROTOCOL_VERSION,
            transfer_id: transfer_id.clone(),
            file_name: selected.file_name.clone(),
            size_bytes: selected.size_bytes,
            sha256: sha256.clone(),
        };

        let (mut offer_send, _offer_recv) = connection.open_bi().await
            .map_err(|error| format!("Could not open the file offer stream: {error}"))?;
        let request_bytes = serde_json::to_vec(&request)
            .map_err(|error| format!("Could not encode the file offer: {error}"))?;
        offer_send.write_all(&request_bytes).await
            .map_err(|error| format!("Could not send the file offer: {error}"))?;
        offer_send.finish()
            .map_err(|error| format!("Could not finish the file offer: {error}"))?;

        let (_decision_send, mut decision_recv) = tokio::time::timeout(
            FILE_OFFER_TTL,
            connection.accept_bi(),
        )
        .await
        .map_err(|_| "The recipient did not respond to the file offer in time.".to_string())?
        .map_err(|error| format!("Could not receive the recipient decision: {error}"))?;
        let decision_bytes = tokio::time::timeout(STREAM_IDLE_TIMEOUT, decision_recv.read_to_end(MAX_CONTROL_FRAME))
            .await
            .map_err(|_| "Timed out waiting for the recipient decision.".to_string())?
            .map_err(|error| format!("Could not read the recipient decision: {error}"))?;
        let decision: FileDecision = serde_json::from_slice(&decision_bytes)
            .map_err(|_| "The recipient returned an invalid file decision.".to_string())?;
        if decision.message_type != "bridge.file_decision"
            || decision.protocol_version != PROTOCOL_VERSION
            || decision.transfer_id != transfer_id {
            connection.close(4u32.into(), b"invalid file decision");
            return Err("The recipient returned a mismatched file decision.".to_string());
        }
        if !decision.accepted {
            connection.close(0u32.into(), b"file offer declined");
            return Err(if decision.message.is_empty() {
                "The recipient declined this file.".to_string()
            } else {
                decision.message
            });
        }

        let mut data_send = connection.open_uni().await
            .map_err(|error| format!("Could not open the file data stream: {error}"))?;
        let mut sent_bytes = 0u64;
        loop {
            let count = tokio::time::timeout(STREAM_IDLE_TIMEOUT, file.read(&mut buffer))
                .await
                .map_err(|_| "Timed out while reading the file.".to_string())?
                .map_err(|error| format!("Could not read {}: {error}", selected.file_name))?;
            if count == 0 { break; }
            data_send.write_all(&buffer[..count]).await
                .map_err(|error| format!("Could not send {}: {error}", selected.file_name))?;
            sent_bytes += count as u64;
            let _ = app.emit("bridge:file-transfer-progress", TransferProgress {
                transfer_id: transfer_id.clone(),
                file_name: selected.file_name.clone(),
                direction: "sent".to_string(),
                transferred_bytes: sent_bytes,
                total_bytes: selected.size_bytes,
            });
        }
        data_send.finish()
            .map_err(|error| format!("Could not finish sending {}: {error}", selected.file_name))?;

        let (_result_send, mut result_recv) = tokio::time::timeout(
            STREAM_IDLE_TIMEOUT,
            connection.accept_bi(),
        )
        .await
        .map_err(|_| "Timed out waiting for the receiver to verify the file.".to_string())?
        .map_err(|error| format!("Could not receive the transfer result: {error}"))?;
        let result_bytes = tokio::time::timeout(STREAM_IDLE_TIMEOUT, result_recv.read_to_end(MAX_CONTROL_FRAME))
            .await
            .map_err(|_| "Timed out waiting for the transfer verification result.".to_string())?
            .map_err(|error| format!("Could not read the transfer verification result: {error}"))?;
        let result: FileTransferResult = serde_json::from_slice(&result_bytes)
            .map_err(|_| "The receiver returned an invalid transfer result.".to_string())?;

        if result.message_type != "bridge.file_result"
            || result.protocol_version != PROTOCOL_VERSION
            || result.transfer_id != transfer_id {
            connection.close(5u32.into(), b"invalid transfer result");
            return Err("The receiver returned a mismatched transfer result.".to_string());
        }
        if !result.success || result.sha256 != sha256 || result.size_bytes != selected.size_bytes {
            connection.close(6u32.into(), b"file verification failed");
            return Err(if result.message.is_empty() {
                "The receiver could not verify the transferred file.".to_string()
            } else {
                result.message
            });
        }

        connection.close(0u32.into(), b"Bridge file transfer complete");
        let receipt = TransferReceipt {
            transfer_id: transfer_id.clone(),
            file_name: selected.file_name.clone(),
            size_bytes: selected.size_bytes,
            sha256,
            remote_endpoint_id: remote_id.to_string(),
            direction: "sent".to_string(),
            completed_at_unix: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
        };
        let _ = app.emit("bridge:file-transfer-complete", &receipt);
        self.release_selected_file(&selected_file_id)?;
        Ok(receipt)
    }

    fn endpoint_snapshot(&self) -> Result<Endpoint, String> {
        self.endpoint.lock()
            .map_err(|_| "Bridge transport state is unavailable".to_string())?
            .as_ref()
            .cloned()
            .ok_or_else(|| "Start the peer network engine before connecting.".to_string())
    }

    fn status_if_running(&self) -> Result<Option<TransportStatus>, String> {
        self.endpoint.lock()
            .map_err(|_| "Bridge transport state is unavailable".to_string())
            .map(|slot| slot.as_ref().map(|endpoint| TransportStatus {
                running: true,
                endpoint_id: Some(endpoint.id().to_string()),
                identity_persisted: false,
            }))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileOfferRequest {
    message_type: String,
    protocol_version: u16,
    transfer_id: String,
    file_name: String,
    size_bytes: u64,
    sha256: String,
}

impl From<FileOfferRequest> for ControlRequest {
    fn from(value: FileOfferRequest) -> Self {
        Self {
            message_type: value.message_type,
            protocol_version: value.protocol_version,
            pairing_code: None,
            transfer_id: Some(value.transfer_id),
            file_name: Some(value.file_name),
            size_bytes: Some(value.size_bytes),
            sha256: Some(value.sha256),
        }
    }
}

fn parse_endpoint_id(value: &str) -> Result<EndpointId, String> {
    EndpointId::from_str(value.trim())
        .map_err(|_| "That device ID is not a valid iroh endpoint ID.".to_string())
}

async fn connect_to_peer(endpoint: &Endpoint, remote_id: EndpointId) -> Result<Connection, String> {
    let connection = tokio::time::timeout(
        Duration::from_secs(12),
        endpoint.connect(remote_id, BRIDGE_ALPN),
    )
    .await
    .map_err(|_| "Timed out while connecting to that device.".to_string())?
    .map_err(|error| format!("Could not connect to that device: {error}"))?;
    if connection.remote_id() != remote_id {
        connection.close(1u32.into(), b"unexpected peer identity");
        return Err("The connected endpoint identity did not match the requested device.".to_string());
    }
    Ok(connection)
}

fn file_name_from_path(path: &FilePath) -> String {
    match path {
        FilePath::Path(path) => path.file_name().and_then(|name| name.to_str()).unwrap_or("Shared file").to_string(),
        FilePath::Url(url) => url.path_segments()
            .and_then(|segments| segments.last())
            .filter(|name| !name.is_empty())
            .unwrap_or("Shared file")
            .to_string(),
    }
}

fn safe_file_name(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty()
        || trimmed.len() > MAX_FILE_NAME_BYTES
        || trimmed == "."
        || trimmed == ".."
        || trimmed.chars().any(|ch| ch.is_control() || ch == '/' || ch == '\\')
    {
        return Err("The file name is not safe to use.".to_string());
    }
    Ok(trimmed.to_string())
}

fn encode_control_request(message_type: &str, pairing_code: Option<String>) -> Result<Vec<u8>, String> {
    serde_json::to_vec(&ControlRequest {
        message_type: message_type.to_string(),
        protocol_version: PROTOCOL_VERSION,
        pairing_code,
        transfer_id: None,
        file_name: None,
        size_bytes: None,
        sha256: None,
    }).map_err(|error| format!("Could not encode the Bridge control request: {error}"))
}

fn consume_pairing_invite(invite: &Mutex<Option<PairingInvite>>, supplied_code: &str) -> bool {
    let Ok(mut slot) = invite.lock() else { return false; };
    let Some(active) = slot.as_mut() else { return false; };
    if Instant::now() >= active.expires_at || active.attempts_remaining == 0 {
        *slot = None;
        return false;
    }
    active.attempts_remaining = active.attempts_remaining.saturating_sub(1);
    let supplied_hash: [u8; 32] = Sha256::digest(supplied_code.as_bytes()).into();
    let matches = active.code_hash.ct_eq(&supplied_hash).unwrap_u8() == 1;
    if matches || active.attempts_remaining == 0 { *slot = None; }
    matches
}

async fn run_accept_loop(
    app: AppHandle,
    endpoint: Endpoint,
    pairing_invite: Arc<Mutex<Option<PairingInvite>>>,
    trusted_peers: Arc<Mutex<HashMap<String, TrustedPeer>>>,
    pending_file_offers: Arc<Mutex<HashMap<String, PendingFileOffer>>>,
) {
    let active_sessions = Arc::new(Semaphore::new(8));
    while let Some(incoming) = endpoint.accept().await {
        let permit = match Arc::clone(&active_sessions).try_acquire_owned() {
            Ok(permit) => permit,
            Err(_) => { incoming.refuse(); continue; }
        };
        let local_endpoint_id = endpoint.id().to_string();
        let app = app.clone();
        let pairing_invite = Arc::clone(&pairing_invite);
        let trusted_peers = Arc::clone(&trusted_peers);
        let pending_file_offers = Arc::clone(&pending_file_offers);

        tokio::spawn(async move {
            let _session_permit = permit;
            let connection = match tokio::time::timeout(Duration::from_secs(10), incoming).await {
                Ok(Ok(connection)) => connection,
                _ => return,
            };
            let remote_endpoint_id = connection.remote_id().to_string();
            let (mut send, mut recv) = match connection.accept_bi().await {
                Ok(streams) => streams,
                Err(_) => return,
            };
            let request_bytes = match tokio::time::timeout(
                Duration::from_secs(8),
                recv.read_to_end(MAX_CONTROL_FRAME),
            ).await {
                Ok(Ok(bytes)) => bytes,
                _ => return,
            };

            let request = match serde_json::from_slice::<ControlRequest>(&request_bytes) {
                Ok(request) if request.protocol_version == PROTOCOL_VERSION => request,
                _ => {
                    let response = ControlResponse {
                        message_type: "bridge.error".to_string(),
                        protocol_version: PROTOCOL_VERSION,
                        endpoint_id: local_endpoint_id,
                        message: "Unsupported or malformed Bridge control request.".to_string(),
                    };
                    if let Ok(bytes) = serde_json::to_vec(&response) {
                        let _ = send.write_all(&bytes).await;
                        let _ = send.finish();
                    }
                    return;
                }
            };

            if request.message_type == "bridge.file_offer" {
                drop(send);
                let transfer_id = request.transfer_id.unwrap_or_default();
                let file_name = match request.file_name.as_deref().and_then(|name| safe_file_name(name).ok()) {
                    Some(name) => name,
                    None => {
                        send_file_decision(&connection, &transfer_id, false, "The offered file name is invalid.").await;
                        return;
                    }
                };
                let size_bytes = request.size_bytes.unwrap_or(0);
                let sha256 = request.sha256.unwrap_or_default();
                if Uuid::parse_str(&transfer_id).is_err() || size_bytes > MAX_FILE_BYTES || !is_sha256(&sha256) {
                    send_file_decision(&connection, &transfer_id, false, "The offered file metadata is invalid or exceeds the 5 GiB limit.").await;
                    return;
                }
                let trusted = trusted_peers.lock()
                    .map(|peers| peers.contains_key(&remote_endpoint_id))
                    .unwrap_or(false);
                if !trusted {
                    send_file_decision(&connection, &transfer_id, false, "This sender is not paired with this device.").await;
                    return;
                }

                let offer = IncomingFileOffer {
                    offer_id: transfer_id.clone(),
                    sender_endpoint_id: remote_endpoint_id.clone(),
                    file_name: file_name.clone(),
                    size_bytes,
                    sha256: sha256.clone(),
                };
                let (destination_tx, destination_rx) = oneshot::channel();
                let stored = pending_file_offers.lock()
                    .map(|mut pending| {
                        pending.insert(transfer_id.clone(), PendingFileOffer {
                            info: offer.clone(),
                            destination_tx,
                        });
                    })
                    .is_ok();
                if !stored {
                    send_file_decision(&connection, &transfer_id, false, "The incoming transfer service is unavailable.").await;
                    return;
                }
                if app.emit("bridge:file-offer", &offer).is_err() {
                    if let Ok(mut pending) = pending_file_offers.lock() {
                        pending.remove(&transfer_id);
                    }
                    send_file_decision(&connection, &transfer_id, false, "Could not show the incoming file request.").await;
                    return;
                }

                let destination = match tokio::time::timeout(FILE_OFFER_TTL, destination_rx).await {
                    Ok(Ok(Some(path))) => path,
                    _ => {
                        if let Ok(mut pending) = pending_file_offers.lock() {
                            pending.remove(&transfer_id);
                        }
                        send_file_decision(&connection, &transfer_id, false, "The recipient did not accept this file in time.").await;
                        return;
                    }
                };

                if let Err(error) = send_file_decision(&connection, &transfer_id, true, "The recipient accepted the file.").await {
                    let _ = error;
                    return;
                }

                let result = receive_file_payload(
                    &app,
                    &connection,
                    &transfer_id,
                    &file_name,
                    size_bytes,
                    &sha256,
                    destination,
                ).await;
                let transfer_result = match result {
                    Ok(()) => FileTransferResult {
                        message_type: "bridge.file_result".to_string(),
                        protocol_version: PROTOCOL_VERSION,
                        transfer_id: transfer_id.clone(),
                        success: true,
                        file_name: file_name.clone(),
                        size_bytes,
                        sha256: sha256.clone(),
                        message: "File received and SHA-256 verified.".to_string(),
                    },
                    Err(error) => FileTransferResult {
                        message_type: "bridge.file_result".to_string(),
                        protocol_version: PROTOCOL_VERSION,
                        transfer_id: transfer_id.clone(),
                        success: false,
                        file_name: file_name.clone(),
                        size_bytes: 0,
                        sha256: String::new(),
                        message: error,
                    },
                };
                let _ = send_file_result(&connection, &transfer_result).await;
                if transfer_result.success {
                    let _ = app.emit("bridge:file-transfer-complete", &TransferReceipt {
                        transfer_id,
                        file_name,
                        size_bytes,
                        sha256,
                        remote_endpoint_id,
                        direction: "received".to_string(),
                        completed_at_unix: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
                    });
                }
                return;
            }

            let response = match request.message_type.as_str() {
                "bridge.hello" => ControlResponse {
                    message_type: "bridge.hello_ack".to_string(),
                    protocol_version: PROTOCOL_VERSION,
                    endpoint_id: local_endpoint_id.clone(),
                    message: "Bridge peer endpoint is reachable.".to_string(),
                },
                "bridge.pair" => {
                    let accepted = request.pairing_code.as_deref()
                        .map(|code| code.len() == 6
                            && code.bytes().all(|byte| byte.is_ascii_digit())
                            && consume_pairing_invite(&pairing_invite, code))
                        .unwrap_or(false);
                    if accepted {
                        let peer = TrustedPeer::new(remote_endpoint_id.clone());
                        if let Ok(mut devices) = trusted_peers.lock() {
                            devices.insert(remote_endpoint_id.clone(), peer);
                        }
                        ControlResponse {
                            message_type: "bridge.pair_ack".to_string(),
                            protocol_version: PROTOCOL_VERSION,
                            endpoint_id: local_endpoint_id.clone(),
                            message: "This device was paired for the current app session.".to_string(),
                        }
                    } else {
                        ControlResponse {
                            message_type: "bridge.pair_error".to_string(),
                            protocol_version: PROTOCOL_VERSION,
                            endpoint_id: local_endpoint_id.clone(),
                            message: "That pairing code is invalid, expired, or has already been used.".to_string(),
                        }
                    }
                }
                _ => ControlResponse {
                    message_type: "bridge.error".to_string(),
                    protocol_version: PROTOCOL_VERSION,
                    endpoint_id: local_endpoint_id.clone(),
                    message: "Unsupported or malformed Bridge control request.".to_string(),
                },
            };
            if let Ok(bytes) = serde_json::to_vec(&response) {
                let _ = send.write_all(&bytes).await;
                let _ = send.finish();
            }
            connection.closed().await;
        });
    }
}

async fn send_file_decision(
    connection: &Connection,
    transfer_id: &str,
    accepted: bool,
    message: &str,
) -> Result<(), String> {
    let (mut send, _recv) = connection.open_bi().await
        .map_err(|error| format!("Could not open file decision stream: {error}"))?;
    let decision = FileDecision {
        message_type: "bridge.file_decision".to_string(),
        protocol_version: PROTOCOL_VERSION,
        transfer_id: transfer_id.to_string(),
        accepted,
        message: message.to_string(),
    };
    let bytes = serde_json::to_vec(&decision).map_err(|error| error.to_string())?;
    send.write_all(&bytes).await.map_err(|error| error.to_string())?;
    send.finish().map_err(|error| error.to_string())?;
    Ok(())
}

async fn send_file_result(connection: &Connection, result: &FileTransferResult) -> Result<(), String> {
    let (mut send, _recv) = connection.open_bi().await
        .map_err(|error| format!("Could not open file result stream: {error}"))?;
    let bytes = serde_json::to_vec(result).map_err(|error| error.to_string())?;
    send.write_all(&bytes).await.map_err(|error| error.to_string())?;
    send.finish().map_err(|error| error.to_string())?;
    Ok(())
}

async fn receive_file_payload(
    app: &AppHandle,
    connection: &Connection,
    transfer_id: &str,
    file_name: &str,
    expected_size: u64,
    expected_sha256: &str,
    destination: FilePath,
) -> Result<(), String> {
    let mut data_stream = tokio::time::timeout(STREAM_IDLE_TIMEOUT, connection.accept_uni())
        .await
        .map_err(|_| "Timed out while waiting for the file data stream.".to_string())?
        .map_err(|error| format!("Could not accept the file data stream: {error}"))?;

    let cache_dir = app.path().app_cache_dir()
        .map_err(|error| format!("Could not locate Bridge's temporary storage: {error}"))?;
    std::fs::create_dir_all(&cache_dir)
        .map_err(|error| format!("Could not create Bridge's temporary storage: {error}"))?;
    let staging_id = Uuid::new_v4();
    let temp_path: PathBuf = cache_dir.join(format!("bridge-{staging_id}.part"));
    let temp_file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp_path)
        .await
        .map_err(|error| format!("Could not create a temporary received file: {error}"))?;

    let mut temp_file = temp_file;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; FILE_CHUNK_SIZE];
    let mut received = 0u64;

    let receive_result: Result<(), String> = async {
        loop {
            let read = tokio::time::timeout(STREAM_IDLE_TIMEOUT, data_stream.read(&mut buffer))
                .await
                .map_err(|_| "The file transfer stalled. Try sending the file again.".to_string())?
                .map_err(|error| format!("Could not read incoming file data: {error}"))?;
            let Some(count) = read else { break; };
            if count == 0 { break; }
            received = received.saturating_add(count as u64);
            if received > expected_size {
                return Err("The incoming file exceeded the size declared by the sender.".to_string());
            }
            hasher.update(&buffer[..count]);
            temp_file.write_all(&buffer[..count]).await
                .map_err(|error| format!("Could not write temporary file data: {error}"))?;
            let _ = app.emit("bridge:file-transfer-progress", TransferProgress {
                transfer_id: transfer_id.to_string(),
                file_name: file_name.to_string(),
                direction: "received".to_string(),
                transferred_bytes: received,
                total_bytes: expected_size,
            });
        }
        temp_file.flush().await.map_err(|error| error.to_string())?;
        temp_file.sync_all().await.map_err(|error| error.to_string())?;

        if received != expected_size {
            return Err(format!("The transfer ended early: received {received} of {expected_size} bytes."));
        }
        let actual_sha256 = format!("{:x}", hasher.finalize());
        if actual_sha256 != expected_sha256 {
            return Err("The received file failed SHA-256 verification and was not saved.".to_string());
        }
        Ok(())
    }.await;

    drop(temp_file);
    if let Err(error) = receive_result {
        let _ = tokio::fs::remove_file(&temp_path).await;
        return Err(error);
    }

    let mut options = FsOpenOptions::new();
    options.write(true).create(true).truncate(true);
    let destination_file = match app.fs().open(destination, options) {
        Ok(file) => file,
        Err(error) => {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err(format!("The file was received and verified, but the chosen destination could not be opened: {error}"));
        }
    };
    let mut input = tokio::fs::File::open(&temp_path).await
        .map_err(|error| format!("Could not reopen the verified temporary file: {error}"))?;
    let mut output = tokio::fs::File::from_std(destination_file);
    let copy_result = tokio::io::copy(&mut input, &mut output).await;
    if let Err(error) = copy_result {
        let _ = tokio::fs::remove_file(&temp_path).await;
        return Err(format!("The file passed integrity checks but could not be written to the selected destination: {error}"));
    }
    output.flush().await.map_err(|error| format!("Could not flush the received file: {error}"))?;
    let _ = tokio::fs::remove_file(&temp_path).await;
    Ok(())
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairing_invite_is_one_time() {
        let invite = Mutex::new(Some(PairingInvite {
            code_hash: Sha256::digest(b"384921").into(),
            expires_at: Instant::now() + Duration::from_secs(60),
            attempts_remaining: MAX_PAIRING_ATTEMPTS,
        }));
        assert!(consume_pairing_invite(&invite, "384921"));
        assert!(!consume_pairing_invite(&invite, "384921"));
    }

    #[test]
    fn invalid_names_and_hashes_are_rejected() {
        assert!(safe_file_name("../secret.txt").is_err());
        assert!(safe_file_name("safe.txt").is_ok());
        assert!(!is_sha256("abcd"));
        assert!(is_sha256(&"a".repeat(64)));
    }

    #[test]
    fn pairing_invite_expires_after_five_wrong_attempts() {
        let invite = Mutex::new(Some(PairingInvite {
            code_hash: Sha256::digest(b"384921").into(),
            expires_at: Instant::now() + Duration::from_secs(60),
            attempts_remaining: MAX_PAIRING_ATTEMPTS,
        }));
        for _ in 0..MAX_PAIRING_ATTEMPTS {
            assert!(!consume_pairing_invite(&invite, "000000"));
        }
        assert!(!consume_pairing_invite(&invite, "384921"));
    }
}
