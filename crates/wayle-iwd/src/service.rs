//! Top-level IWD service.

use std::sync::Arc;

use tokio_util::sync::CancellationToken;
use tracing::{instrument, warn};
use wayle_core::Property;
use wayle_traits::{Reactive, ServiceMonitoring};
use zbus::{Connection, zvariant::OwnedObjectPath};

use crate::{
    agent::{AGENT_PATH, Agent, PassphraseStore},
    discovery::IwdDiscovery,
    error::Error,
    proxy::agent_manager::AgentManagerProxy,
    station::{LiveStationParams, Station},
};

/// Entry point for WiFi management via IWD.
///
/// Mirrors the WiFi surface of `wayle-network`'s `NetworkService`, but is
/// WiFi-only (IWD does not manage wired connections or IP configuration).
pub struct IwdService {
    pub(crate) zbus_connection: Connection,
    pub(crate) cancellation_token: CancellationToken,
    pub(crate) passphrases: Arc<PassphraseStore>,
    /// WiFi station, if a device is present (live-updated on hot-plug).
    pub station: Property<Option<Arc<Station>>>,
}

impl IwdService {
    /// Connect to IWD, register the passphrase agent, discover the station
    /// device, and begin monitoring.
    ///
    /// # Errors
    /// Returns [`Error::ServiceInitializationFailed`] if the D-Bus connection
    /// or agent registration cannot be established.
    #[instrument]
    pub async fn new() -> Result<Self, Error> {
        let connection = Connection::system().await.map_err(|err| {
            Error::ServiceInitializationFailed(format!("D-Bus connection failed: {err}"))
        })?;

        let cancellation_token = CancellationToken::new();
        let passphrases = Arc::new(PassphraseStore::default());

        register_agent(&connection, passphrases.clone()).await?;

        let station = match IwdDiscovery::device_path(&connection).await? {
            Some(path) => {
                build_station(&connection, path, &cancellation_token, passphrases.clone()).await
            }
            None => None,
        };

        let service = Self {
            zbus_connection: connection,
            cancellation_token,
            passphrases,
            station: Property::new(station),
        };

        service.start_monitoring().await?;

        Ok(service)
    }
}

impl Drop for IwdService {
    fn drop(&mut self) {
        self.cancellation_token.cancel();
    }
}

async fn register_agent(
    connection: &Connection,
    passphrases: Arc<PassphraseStore>,
) -> Result<(), Error> {
    let agent_path = zbus::zvariant::ObjectPath::try_from(AGENT_PATH).map_err(|err| {
        Error::ServiceInitializationFailed(format!("invalid agent path: {err}"))
    })?;

    connection
        .object_server()
        .at(agent_path.clone(), Agent::new(passphrases))
        .await
        .map_err(Error::DbusError)?;

    let manager = AgentManagerProxy::new(connection)
        .await
        .map_err(Error::DbusError)?;

    if let Err(err) = manager.register_agent(&agent_path).await {
        warn!(error = %err, "cannot register iwd agent; passphrase prompts may be unavailable");
    }

    Ok(())
}

/// Build a live [`Station`], logging and returning `None` on failure.
pub(crate) async fn build_station(
    connection: &Connection,
    path: OwnedObjectPath,
    cancellation_token: &CancellationToken,
    passphrases: Arc<PassphraseStore>,
) -> Option<Arc<Station>> {
    match Station::get_live(LiveStationParams {
        connection,
        device_path: path.clone(),
        cancellation_token,
        passphrases,
    })
    .await
    {
        Ok(station) => Some(station),
        Err(err) => {
            warn!(error = %err, path = %path, "cannot create iwd station");
            None
        }
    }
}
