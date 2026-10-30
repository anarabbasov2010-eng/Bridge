use iroh::{endpoint::presets, Endpoint, EndpointId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    str::FromStr,
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use subtle::ConstantTimeEq;
use tokio::{io::AsyncWriteExt, sync::Semaphore};

/// Bridge's versioned peer-control protocol. File transfer ALPN will be added
/// only after approved pairing and transfer limits are in place.
pub const BRIDGE_ALPN: &[u8] = b"app.bridge.control/1";
const MAX_CONTROL_FRAME: usize = 16 * 1024;
const PROTOCOL_VERSION: u16 = 1;
const PAIRING_CODE_TTL: Duration = Duration::from_secs(180);
const MAX_PAIRING_ATTEMPTS: u8 = 5;

#[derive(Default)]
pub struct PeerTransport {
    endpoint: Mutex<Option<Endpoint>>,
    pairing_invite: Arc<Mutex<Option<PairingInvite>>>,
    trusted_peers: Arc<Mutex<HashMap<String, TrustedPeer>>>,
}

struct PairingInvite {
    code_hash: [u8; 32],
    expires_at: Instant,
    attempts_remaining: u8,
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

impl TrustedPeer {
    fn new(endpoint_id: String) -> Self {
        let paired_at_unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Self { endpoint_id, paired_at_unix }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ControlRequest {
    message_type: String,
    protocol_version: u16,
    #[serde(default)]
    pairing_code: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ControlResponse {
    message_type: String,
    protocol_version: u16,
    endpoint_id: String,
    message: String,
}

impl PeerTransport {
    pub async fn start(&self) -> Result<TransportStatus, String> {
        if let Some(status) = self.status_if_running()? {
            return Ok(status);
        }

        let endpoint = Endpoint::builder(presets::N0)
            .alpns(vec![BRIDGE_ALPN.to_vec()])
            .bind()
            .await
            .map_err(|error| format!("Unable to start Bridge peer transport: {error}"))?;

        let endpoint_id = endpoint.id().to_string();

        // Keep the synchronous mutex guard entirely inside this scope. Tauri
        // commands must return Send futures, and a MutexGuard cannot cross await.
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
            endpoint.clone(),
            Arc::clone(&self.pairing_invite),
            Arc::clone(&self.trusted_peers),
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

    pub fn create_pairing_invite(&self) -> Result<PairingInviteInfo, String> {
        let endpoint_id = self.endpoint.lock()
            .map_err(|_| "Bridge transport state is unavailable".to_string())?
            .as_ref()
            .map(|endpoint| endpoint.id().to_string())
            .ok_or_else(|| "Start the peer network engine before creating an invite.".to_string())?;

        // A short human-readable code is protected by one-use semantics,
        // a three-minute lifetime, and a maximum of five verification attempts.
        let code = format!("{:06}", rand::random_range(0..1_000_000u32));
        let code_hash: [u8; 32] = Sha256::digest(code.as_bytes()).into();
        let expires_at = Instant::now() + PAIRING_CODE_TTL;

        *self.pairing_invite.lock()
            .map_err(|_| "Bridge pairing state is unavailable".to_string())? = Some(PairingInvite {
                code_hash,
                expires_at,
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

        let (mut send, mut recv) = connection.open_bi().await
            .map_err(|error| format!("Could not open the Bridge control stream: {error}"))?;

        let request = encode_control_request("bridge.hello", None)?;
        send.write_all(&request).await
            .map_err(|error| format!("Could not send the Bridge handshake: {error}"))?;
        send.finish()
            .map_err(|error| format!("Could not finish the Bridge handshake: {error}"))?;

        let response_bytes = recv.read_to_end(MAX_CONTROL_FRAME).await
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

        let (mut send, mut recv) = connection.open_bi().await
            .map_err(|error| format!("Could not open the Bridge pairing stream: {error}"))?;
        let request = encode_control_request("bridge.pair", Some(code.to_string()))?;

        send.write_all(&request).await
            .map_err(|error| format!("Could not send the pairing request: {error}"))?;
        send.finish()
            .map_err(|error| format!("Could not finish the pairing request: {error}"))?;

        let response_bytes = recv.read_to_end(MAX_CONTROL_FRAME).await
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

fn parse_endpoint_id(value: &str) -> Result<EndpointId, String> {
    EndpointId::from_str(value.trim())
        .map_err(|_| "That device ID is not a valid iroh endpoint ID.".to_string())
}

fn encode_control_request(message_type: &str, pairing_code: Option<String>) -> Result<Vec<u8>, String> {
    serde_json::to_vec(&ControlRequest {
        message_type: message_type.to_string(),
        protocol_version: PROTOCOL_VERSION,
        pairing_code,
    }).map_err(|error| format!("Could not encode the Bridge control request: {error}"))
}

fn consume_pairing_invite(invite: &Mutex<Option<PairingInvite>>, supplied_code: &str) -> bool {
    let Ok(mut slot) = invite.lock() else {
        return false;
    };
    let Some(active) = slot.as_mut() else {
        return false;
    };

    if Instant::now() >= active.expires_at || active.attempts_remaining == 0 {
        *slot = None;
        return false;
    }

    active.attempts_remaining = active.attempts_remaining.saturating_sub(1);
    let supplied_hash: [u8; 32] = Sha256::digest(supplied_code.as_bytes()).into();
    let matches = active.code_hash.ct_eq(&supplied_hash).unwrap_u8() == 1;

    if matches || active.attempts_remaining == 0 {
        *slot = None;
    }
    matches
}

async fn run_accept_loop(
    endpoint: Endpoint,
    pairing_invite: Arc<Mutex<Option<PairingInvite>>>,
    trusted_peers: Arc<Mutex<HashMap<String, TrustedPeer>>>,
) {
    let active_sessions = Arc::new(Semaphore::new(8));

    while let Some(incoming) = endpoint.accept().await {
        let permit = match Arc::clone(&active_sessions).try_acquire_owned() {
            Ok(permit) => permit,
            Err(_) => {
                incoming.refuse();
                continue;
            }
        };

        let local_endpoint_id = endpoint.id().to_string();
        let pairing_invite = Arc::clone(&pairing_invite);
        let trusted_peers = Arc::clone(&trusted_peers);

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

            let response = match serde_json::from_slice::<ControlRequest>(&request_bytes) {
                Ok(request)
                    if request.message_type == "bridge.hello"
                        && request.protocol_version == PROTOCOL_VERSION =>
                {
                    ControlResponse {
                        message_type: "bridge.hello_ack".to_string(),
                        protocol_version: PROTOCOL_VERSION,
                        endpoint_id: local_endpoint_id.clone(),
                        message: "Bridge peer endpoint is reachable.".to_string(),
                    }
                }
                Ok(request)
                    if request.message_type == "bridge.pair"
                        && request.protocol_version == PROTOCOL_VERSION =>
                {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_invite(code: &str) -> Mutex<Option<PairingInvite>> {
        Mutex::new(Some(PairingInvite {
            code_hash: Sha256::digest(code.as_bytes()).into(),
            expires_at: Instant::now() + Duration::from_secs(60),
            attempts_remaining: MAX_PAIRING_ATTEMPTS,
        }))
    }

    #[test]
    fn control_protocol_has_a_fixed_versioned_alpn() {
        assert_eq!(BRIDGE_ALPN, b"app.bridge.control/1");
        assert_eq!(PROTOCOL_VERSION, 1);
    }

    #[test]
    fn accepts_only_the_expected_hello_message() {
        let valid = br#"{"messageType":"bridge.hello","protocolVersion":1}"#;
        let parsed: ControlRequest = serde_json::from_slice(valid).expect("valid hello");
        assert_eq!(parsed.message_type, "bridge.hello");
        assert_eq!(parsed.protocol_version, PROTOCOL_VERSION);

        let malformed = br#"{"messageType":"bridge.transfer","protocolVersion":1}"#;
        let parsed: ControlRequest = serde_json::from_slice(malformed).expect("valid JSON");
        assert_ne!(parsed.message_type, "bridge.hello");
    }

    #[test]
    fn pairing_invite_is_one_time() {
        let invite = make_invite("384921");
        assert!(consume_pairing_invite(&invite, "384921"));
        assert!(!consume_pairing_invite(&invite, "384921"));
    }

    #[test]
    fn pairing_invite_rejects_wrong_codes_and_expires_after_five_attempts() {
        let invite = make_invite("384921");
        for _ in 0..MAX_PAIRING_ATTEMPTS {
            assert!(!consume_pairing_invite(&invite, "000000"));
        }
        assert!(!consume_pairing_invite(&invite, "384921"));
    }
}
