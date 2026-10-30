use iroh::{endpoint::presets, Endpoint, EndpointId};
use serde::{Deserialize, Serialize};
use std::{str::FromStr, sync::Mutex};
use tokio::io::AsyncWriteExt;

/// Bridge's versioned peer-control protocol. File transfer ALPN will be added
/// only after an authenticated pairing handshake and transfer limits are in place.
pub const BRIDGE_ALPN: &[u8] = b"app.bridge.control/1";
const MAX_CONTROL_FRAME: usize = 16 * 1024;
const PROTOCOL_VERSION: u16 = 1;

#[derive(Default)]
pub struct PeerTransport {
    endpoint: Mutex<Option<Endpoint>>,
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ControlRequest {
    message_type: String,
    protocol_version: u16,
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

        // The endpoint is cloned into a cancellable accept loop. Closing the
        // stored endpoint causes accept() to return None and the loop to exit.
        tokio::spawn(run_accept_loop(endpoint.clone()));

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

    pub async fn probe_peer(&self, remote_endpoint_id: String) -> Result<PeerProbe, String> {
        let endpoint = {
            self.endpoint.lock()
                .map_err(|_| "Bridge transport state is unavailable".to_string())?
                .clone()
                .ok_or_else(|| "Start the peer network engine before connecting.".to_string())?
        };

        let remote_id = EndpointId::from_str(remote_endpoint_id.trim())
            .map_err(|_| "That device ID is not a valid iroh endpoint ID.".to_string())?;

        let connection = endpoint.connect(remote_id, BRIDGE_ALPN).await
            .map_err(|error| format!("Could not connect to that device: {error}"))?;

        // iroh authenticates the remote endpoint key as part of its encrypted
        // connection handshake. Check it again before sending the application hello.
        if connection.remote_id() != remote_id {
            connection.close(1u32.into(), b"unexpected peer identity");
            return Err("The connected endpoint identity did not match the requested device.".to_string());
        }

        let (mut send, mut recv) = connection.open_bi().await
            .map_err(|error| format!("Could not open the Bridge control stream: {error}"))?;

        let request = serde_json::to_vec(&ControlRequest {
            message_type: "bridge.hello".to_string(),
            protocol_version: PROTOCOL_VERSION,
        }).map_err(|error| format!("Could not encode the Bridge handshake: {error}"))?;

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

async fn run_accept_loop(endpoint: Endpoint) {
    while let Some(incoming) = endpoint.accept().await {
        tokio::spawn(async move {
            let connection = match incoming.await {
                Ok(connection) => connection,
                Err(_) => return,
            };

            let remote_endpoint_id = connection.remote_id().to_string();
            let (mut send, mut recv) = match connection.accept_bi().await {
                Ok(streams) => streams,
                Err(_) => return,
            };

            let request_bytes = match recv.read_to_end(MAX_CONTROL_FRAME).await {
                Ok(bytes) => bytes,
                Err(_) => return,
            };

            let response = match serde_json::from_slice::<ControlRequest>(&request_bytes) {
                Ok(request)
                    if request.message_type == "bridge.hello"
                        && request.protocol_version == PROTOCOL_VERSION =>
                {
                    ControlResponse {
                        message_type: "bridge.hello_ack".to_string(),
                        protocol_version: PROTOCOL_VERSION,
                        endpoint_id: endpoint.id().to_string(),
                        message: "Bridge peer endpoint is reachable.".to_string(),
                    }
                }
                _ => ControlResponse {
                    message_type: "bridge.error".to_string(),
                    protocol_version: PROTOCOL_VERSION,
                    endpoint_id: endpoint.id().to_string(),
                    message: "Unsupported or malformed Bridge control request.".to_string(),
                },
            };

            if let Ok(bytes) = serde_json::to_vec(&response) {
                let _ = send.write_all(&bytes).await;
                let _ = send.finish();
            }

            // This first control protocol only verifies encrypted reachability.
            // It does not grant trust or authorize file transfer.
            let _ = remote_endpoint_id;
            connection.closed().await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
