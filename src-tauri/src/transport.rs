use iroh::{endpoint::presets, Endpoint};
use serde::Serialize;
use std::sync::Mutex;

/// Versioned application protocol. Pairing and transfer handlers will be routed
/// through this ALPN after the authenticated handshake is implemented.
pub const BRIDGE_ALPN: &[u8] = b"app.bridge.transfer/1";

#[derive(Default)]
pub struct PeerTransport {
    endpoint: Mutex<Option<Endpoint>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransportStatus {
    pub running: bool,
    pub endpoint_id: Option<String>,
    /// The first endpoint uses a process-lifetime key. Platform secure storage
    /// must replace this before a public beta so identity survives restarts.
    pub identity_persisted: bool,
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
                *slot = Some(endpoint);
                None
            }
        };

        if let Some(status) = already_running {
            endpoint.close().await;
            return Ok(status);
        }

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
