//! WiFi station facade (the analogue of `wayle-network`'s `Wifi`).
//!
//! A single IWD device object implements the `Device`, `Station`, and
//! `StationDiagnostic` interfaces. [`Station`] wraps that object, exposing
//! reactive [`Property`] state and connection controls.

mod monitoring;

use std::sync::Arc;

use tokio_util::sync::CancellationToken;
use tracing::debug;
use wayle_core::Property;
use wayle_traits::{ModelMonitoring, Reactive};
use zbus::{Connection, zvariant::OwnedObjectPath};

use crate::{
    agent::PassphraseStore,
    error::Error,
    network::Network,
    proxy::{device::DeviceProxy, network::NetworkProxy, station::StationProxy},
    types::{NetworkStatus, signal_to_percent},
};

#[doc(hidden)]
pub struct StationParams<'a> {
    pub(crate) connection: &'a Connection,
    pub(crate) device_path: OwnedObjectPath,
    pub(crate) passphrases: Arc<PassphraseStore>,
}

#[doc(hidden)]
pub struct LiveStationParams<'a> {
    pub(crate) connection: &'a Connection,
    pub(crate) device_path: OwnedObjectPath,
    pub(crate) cancellation_token: &'a CancellationToken,
    pub(crate) passphrases: Arc<PassphraseStore>,
}

pub(crate) fn is_real_path(path: &OwnedObjectPath) -> bool {
    let s = path.as_str();
    !s.is_empty() && s != "/"
}

/// Whether a D-Bus error is the named IWD method error (e.g.
/// `net.connman.iwd.Failed`).
fn is_iwd_error(err: &zbus::Error, name: &str) -> bool {
    matches!(err, zbus::Error::MethodError(error_name, _, _) if error_name.as_str() == name)
}

/// A WiFi station: connection state, scan results, and controls.
#[derive(Clone)]
pub struct Station {
    connection: Connection,
    object_path: OwnedObjectPath,
    cancellation_token: Option<CancellationToken>,
    passphrases: Arc<PassphraseStore>,
    /// Whether the underlying device is powered on (the WiFi enable toggle).
    pub powered: Property<bool>,
    /// Current connectivity status.
    pub state: Property<NetworkStatus>,
    /// Whether a scan is in progress.
    pub scanning: Property<bool>,
    /// SSID of the connected network, if any.
    pub connected_ssid: Property<Option<String>>,
    /// Signal strength of the connected link (0-100), from diagnostics.
    pub strength: Property<Option<u8>>,
    /// Frequency of the connected link in MHz, from diagnostics.
    pub frequency: Property<Option<u32>>,
    /// Visible networks, ordered strongest-first.
    pub networks: Property<Vec<Arc<Network>>>,
}

impl PartialEq for Station {
    fn eq(&self, other: &Self) -> bool {
        self.object_path == other.object_path
    }
}

impl Reactive for Station {
    type Context<'a> = StationParams<'a>;
    type LiveContext<'a> = LiveStationParams<'a>;
    type Error = Error;

    async fn get(params: Self::Context<'_>) -> Result<Self, Self::Error> {
        Self::from_path(
            params.connection,
            params.device_path,
            None,
            params.passphrases,
        )
        .await
    }

    async fn get_live(params: Self::LiveContext<'_>) -> Result<Arc<Self>, Self::Error> {
        let station = Self::from_path(
            params.connection,
            params.device_path,
            Some(params.cancellation_token.child_token()),
            params.passphrases,
        )
        .await?;
        let station = Arc::new(station);
        station.clone().start_monitoring().await?;
        Ok(station)
    }
}

impl Station {
    /// D-Bus object path of the station device.
    pub fn object_path(&self) -> &OwnedObjectPath {
        &self.object_path
    }

    /// Enable or disable the WiFi device (`Device.Powered`).
    ///
    /// # Errors
    /// Returns [`Error::OperationFailed`] if the D-Bus call fails.
    pub async fn set_powered(&self, on: bool) -> Result<(), Error> {
        let device = DeviceProxy::new(&self.connection, self.object_path.clone())
            .await
            .map_err(|e| Error::OperationFailed {
                operation: "create device proxy",
                source: e.into(),
            })?;

        device.set_powered(on).await.map_err(|e| Error::OperationFailed {
            operation: "set device powered",
            source: e.into(),
        })
    }

    /// Request a scan for networks.
    ///
    /// # Errors
    /// Returns [`Error::OperationFailed`] if the D-Bus call fails.
    pub async fn scan(&self) -> Result<(), Error> {
        let station = self.station_proxy().await?;
        station.scan().await.map_err(|e| Error::OperationFailed {
            operation: "scan",
            source: e.into(),
        })
    }

    /// Disconnect from the current network.
    ///
    /// # Errors
    /// Returns [`Error::OperationFailed`] if the D-Bus call fails.
    pub async fn disconnect(&self) -> Result<(), Error> {
        let station = self.station_proxy().await?;
        station.disconnect().await.map_err(|e| Error::OperationFailed {
            operation: "disconnect",
            source: e.into(),
        })
    }

    /// Connect to a network by object path.
    ///
    /// For secured networks, stage the `passphrase` (delivered to IWD via the
    /// agent's `RequestPassphrase`). For open or already-known networks, pass
    /// `None`. Resolves once IWD reports success, or returns an error (e.g. on
    /// a wrong passphrase).
    ///
    /// # Errors
    /// Returns [`Error::OperationFailed`] if the connection fails.
    pub async fn connect(
        &self,
        network_path: OwnedObjectPath,
        passphrase: Option<String>,
    ) -> Result<(), Error> {
        if let Some(passphrase) = passphrase {
            self.passphrases.insert(network_path.clone(), passphrase);
        }

        let proxy = match NetworkProxy::new(&self.connection, network_path.clone()).await {
            Ok(proxy) => proxy,
            Err(err) => {
                self.passphrases.discard(&network_path);
                return Err(Error::OperationFailed {
                    operation: "create network proxy",
                    source: err.into(),
                });
            }
        };

        let result = proxy.connect().await;
        self.passphrases.discard(&network_path);

        result.map_err(|e| {
            if is_iwd_error(&e, "net.connman.iwd.Failed") {
                Error::ConnectionFailed
            } else {
                Error::OperationFailed {
                    operation: "connect to network",
                    source: e.into(),
                }
            }
        })
    }

    async fn station_proxy(&self) -> Result<StationProxy<'static>, Error> {
        StationProxy::new(&self.connection, self.object_path.clone())
            .await
            .map_err(Error::DbusError)
    }

    /// Re-fetch the ordered network list from IWD and publish it.
    ///
    /// When the device is powered off the `Station` interface is absent, so the
    /// list is simply cleared.
    pub(crate) async fn refresh_networks(&self) {
        if !self.powered.get() {
            self.networks.replace(Vec::new());
            return;
        }

        let Ok(station) = self.station_proxy().await else {
            return;
        };

        let ordered = match station.get_ordered_networks().await {
            Ok(ordered) => ordered,
            Err(err) => {
                debug!(error = %err, "cannot fetch ordered networks");
                return;
            }
        };

        let mut networks = Vec::with_capacity(ordered.len());
        for (path, signal) in ordered {
            if let Ok(network) =
                Network::from_path(&self.connection, path, signal_to_percent(signal)).await
            {
                networks.push(Arc::new(network));
            }
        }

        self.networks.replace(networks);
    }

    /// Re-read the `Station`-interface state after the device is powered back
    /// on (the interface reappears), then refresh the list and kick a scan.
    pub(crate) async fn resync_after_power_on(&self) {
        let (state, scanning, connected_ssid) =
            read_station_state(&self.connection, &self.object_path).await;
        self.state.set(state);
        self.scanning.set(scanning);
        self.connected_ssid.set(connected_ssid);
        self.refresh_networks().await;
        // Best-effort fresh scan; ignored if the interface isn't ready yet.
        let _ = self.scan().await;
    }

    async fn from_path(
        connection: &Connection,
        path: OwnedObjectPath,
        cancellation_token: Option<CancellationToken>,
        passphrases: Arc<PassphraseStore>,
    ) -> Result<Self, Error> {
        // Presence is keyed on the Device interface, which survives power-off
        // (the Station interface is removed while powered down).
        let device_proxy = DeviceProxy::new(connection, path.clone())
            .await
            .map_err(Error::DbusError)?;

        let Ok(powered) = device_proxy.powered().await else {
            return Err(Error::ObjectNotFound(path.clone()));
        };

        let (state, scanning, connected_ssid) = if powered {
            read_station_state(connection, &path).await
        } else {
            (NetworkStatus::Disconnected, false, None)
        };

        Ok(Self {
            connection: connection.clone(),
            object_path: path,
            cancellation_token,
            passphrases,
            powered: Property::new(powered),
            state: Property::new(state),
            scanning: Property::new(scanning),
            connected_ssid: Property::new(connected_ssid),
            strength: Property::new(None),
            frequency: Property::new(None),
            networks: Property::new(Vec::new()),
        })
    }
}

/// Reads the `Station`-interface state (state / scanning / connected SSID),
/// defaulting gracefully if the interface is absent (device powered off).
async fn read_station_state(
    connection: &Connection,
    path: &OwnedObjectPath,
) -> (NetworkStatus, bool, Option<String>) {
    let Ok(station_proxy) = StationProxy::new(connection, path.clone()).await else {
        return (NetworkStatus::Disconnected, false, None);
    };

    let state = station_proxy
        .state()
        .await
        .map(|s| NetworkStatus::from_iwd_state(&s))
        .unwrap_or(NetworkStatus::Disconnected);
    let scanning = station_proxy.scanning().await.unwrap_or(false);
    let connected_ssid = resolve_connected_ssid(connection, path).await;

    (state, scanning, connected_ssid)
}

/// Resolve the SSID of the station's connected network, if any.
pub(crate) async fn resolve_connected_ssid(
    connection: &Connection,
    station_path: &OwnedObjectPath,
) -> Option<String> {
    let station = StationProxy::new(connection, station_path.clone()).await.ok()?;
    let path = station.connected_network().await.ok()?;
    if !is_real_path(&path) {
        return None;
    }
    let network = NetworkProxy::new(connection, path).await.ok()?;
    network.name().await.ok()
}
