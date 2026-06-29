//! WiFi station facade (the analogue of `wayle-network`'s `Wifi`).
//!
//! A single IWD device object implements the `Device`, `Station`, and
//! `StationDiagnostic` interfaces. [`Station`] wraps that object, exposing
//! reactive [`Property`] state and connection controls.

mod monitoring;

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

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
    types::{ConnectionState, SignalStrength},
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

/// RAII marker that a foreground [`Station::connect`] is in progress. While any
/// guard is alive the background monitor refrains from writing
/// [`Station::connection`] (see [`Station::observe_connection`]), so the
/// foreground attempt fully owns that state and transient IWD signals during a
/// network switch cannot clobber the in-flight target. The count handles
/// overlapping attempts (a new connect superseding a pending one).
struct AttemptGuard(Arc<AtomicUsize>);

impl AttemptGuard {
    fn new(counter: &Arc<AtomicUsize>) -> Self {
        counter.fetch_add(1, Ordering::SeqCst);
        Self(counter.clone())
    }
}

impl Drop for AttemptGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// A WiFi station: connection state, scan results, and controls.
#[derive(Clone)]
pub struct Station {
    zbus_connection: Connection,
    object_path: OwnedObjectPath,
    cancellation_token: Option<CancellationToken>,
    passphrases: Arc<PassphraseStore>,
    /// Number of foreground [`connect`](Self::connect) attempts in progress.
    /// While non-zero the monitor leaves [`connection`](Self::connection) to the
    /// foreground attempt; see [`AttemptGuard`].
    pending_connects: Arc<AtomicUsize>,
    /// Whether the underlying device is powered on (the WiFi enable toggle).
    pub powered: Property<bool>,
    /// Attempt-aware connection state: the active or in-progress connection and
    /// its target SSID. The single source of truth for the "active connection"
    /// UI, reconciled from IWD's `Station.State` + `ConnectedNetwork` and from
    /// foreground [`connect`](Self::connect) attempts.
    pub connection: Property<ConnectionState>,
    /// Whether a scan is in progress.
    pub scanning: Property<bool>,
    /// Bucketed signal strength of the connected link. Pushed by IWD's
    /// `SignalLevelAgent` as the RSSI crosses thresholds, plus a snapshot read
    /// when a connection comes up.
    pub strength: Property<Option<SignalStrength>>,
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
    pub(crate) fn object_path(&self) -> &OwnedObjectPath {
        &self.object_path
    }

    /// Cancel this station's background monitor. Called when the station is being
    /// replaced (e.g. IWD restarted, or the device was removed) so the old
    /// monitor task exits promptly instead of lingering with dead signal streams.
    pub(crate) fn shutdown(&self) {
        if let Some(token) = &self.cancellation_token {
            token.cancel();
        }
    }

    /// Enable or disable the WiFi device (`Device.Powered`).
    ///
    /// # Errors
    /// Returns [`Error::OperationFailed`] if the D-Bus call fails.
    pub async fn set_powered(&self, on: bool) -> Result<(), Error> {
        let device = DeviceProxy::new(&self.zbus_connection, self.object_path.clone())
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
    /// For the duration of the call this attempt owns [`connection`](Self::connection)
    /// (the monitor steps back — see [`observe_connection`](Self::observe_connection)),
    /// publishing the target optimistically and reconciling to the true live
    /// state on completion.
    ///
    /// # Errors
    /// Returns [`Error::ConnectionFailed`] on a rejected passphrase,
    /// [`Error::ConnectionAborted`] if cancelled/superseded, or
    /// [`Error::OperationFailed`] for any other failure.
    pub async fn connect(
        &self,
        network_path: OwnedObjectPath,
        passphrase: Option<String>,
    ) -> Result<(), Error> {
        // Take ownership of `connection` for the lifetime of the attempt so the
        // monitor's transient signals during a network switch cannot clobber the
        // in-flight target. Reconciliation to the true state happens below,
        // before the guard drops.
        let _attempt = AttemptGuard::new(&self.pending_connects);

        if let Some(passphrase) = passphrase {
            self.passphrases.insert(network_path.clone(), passphrase);
        }

        // Publish the in-flight target immediately for instant, flicker-free UI.
        // Resolved from the cached scan list, falling back to the proxy name.
        if let Some(ssid) = self.network_ssid(&network_path) {
            self.connection.set(ConnectionState::Connecting { ssid });
        }

        let proxy = match NetworkProxy::new(&self.zbus_connection, network_path.clone()).await {
            Ok(proxy) => proxy,
            Err(err) => {
                self.passphrases.discard(&network_path);
                self.reconcile_connection_from_live().await;
                return Err(Error::OperationFailed {
                    operation: "create network proxy",
                    source: err.into(),
                });
            }
        };

        if !matches!(self.connection.get(), ConnectionState::Connecting { .. })
            && let Ok(name) = proxy.name().await
        {
            self.connection.set(ConnectionState::Connecting { ssid: name });
        }

        let result = proxy.connect().await;
        self.passphrases.discard(&network_path);

        // Publish the true state IWD settled on (success, stayed on the previous
        // network after a rejected passphrase, or disconnected). Reading live
        // covers every outcome uniformly and is correct even when another connect
        // is concurrently in flight.
        self.reconcile_connection_from_live().await;

        result.map_err(|e| {
            if is_iwd_error(&e, "net.connman.iwd.Failed") {
                Error::ConnectionFailed
            } else if is_iwd_error(&e, "net.connman.iwd.Aborted") {
                Error::ConnectionAborted
            } else {
                Error::OperationFailed {
                    operation: "connect to network",
                    source: e.into(),
                }
            }
        })
    }

    /// SSID of a network in the current scan list, by object path.
    fn network_ssid(&self, network_path: &OwnedObjectPath) -> Option<String> {
        self.networks
            .get()
            .iter()
            .find(|network| network.object_path() == network_path)
            .map(|network| network.ssid.get())
    }

    /// Whether a foreground [`connect`](Self::connect) currently owns
    /// [`connection`](Self::connection).
    fn connecting_in_flight(&self) -> bool {
        self.pending_connects.load(Ordering::SeqCst) > 0
    }

    /// Publish [`connection`](Self::connection) from an observed raw `Station.State`
    /// and resolved SSID — the authoritative driver for connections from any
    /// client (including external ones such as `iwctl`).
    ///
    /// A no-op while a foreground [`connect`](Self::connect) is in flight: that
    /// attempt owns `connection` and the coarse signals seen here during a
    /// network switch would otherwise clobber its in-flight target.
    pub(crate) fn observe_connection(&self, state: &str, connected_ssid: Option<String>) {
        if self.connecting_in_flight() {
            return;
        }
        self.connection
            .set(ConnectionState::from_raw_state(state, connected_ssid));
    }

    /// Reconcile [`connection`](Self::connection) to the live `Station.State` and
    /// `ConnectedNetwork`. Called by [`connect`](Self::connect) on completion to
    /// publish the true outcome, bypassing the in-flight guard (it *is* the owner
    /// finishing).
    async fn reconcile_connection_from_live(&self) {
        let state = match self.station_proxy().await {
            Ok(proxy) => proxy.state().await.unwrap_or_default(),
            Err(_) => String::new(),
        };
        let connected_ssid = resolve_connected_ssid(&self.zbus_connection, &self.object_path).await;
        self.connection
            .set(ConnectionState::from_raw_state(&state, connected_ssid));
    }

    async fn station_proxy(&self) -> Result<StationProxy<'static>, Error> {
        StationProxy::new(&self.zbus_connection, self.object_path.clone())
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
            // `GetOrderedNetworks` reports 100 * dBm; bucket the plain-dBm value.
            let strength = SignalStrength::from_dbm(signal / 100);
            if let Ok(network) = Network::from_path(&self.zbus_connection, path, strength).await {
                networks.push(Arc::new(network));
            }
        }

        self.networks.replace(networks);
    }

    /// Re-read the `Station`-interface state after the device is powered back
    /// on (the interface reappears), then refresh the list and kick a scan.
    pub(crate) async fn resync_after_power_on(&self) {
        let (state, scanning, connected_ssid) =
            read_station_state(&self.zbus_connection, &self.object_path).await;
        self.scanning.set(scanning);
        // Just powered on: no foreground attempt can be in flight, so publish
        // directly from the re-read state.
        self.connection
            .set(ConnectionState::from_raw_state(&state, connected_ssid));

        // The Station interface — and any signal-level agent registration — was
        // dropped while powered off, so re-register to keep strength event-driven.
        if let Ok(station_proxy) = self.station_proxy().await {
            monitoring::setup_signal_level_agent(
                &self.zbus_connection,
                &station_proxy,
                self.strength.clone(),
            )
            .await;
        }

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
            (String::new(), false, None)
        };

        let connection_state = ConnectionState::from_raw_state(&state, connected_ssid);

        Ok(Self {
            zbus_connection: connection.clone(),
            object_path: path,
            cancellation_token,
            passphrases,
            pending_connects: Arc::new(AtomicUsize::new(0)),
            powered: Property::new(powered),
            connection: Property::new(connection_state),
            scanning: Property::new(scanning),
            strength: Property::new(None),
            frequency: Property::new(None),
            networks: Property::new(Vec::new()),
        })
    }
}

/// Reads the `Station`-interface state (raw `State` string / scanning / connected
/// SSID), defaulting gracefully if the interface is absent (device powered off).
async fn read_station_state(
    connection: &Connection,
    path: &OwnedObjectPath,
) -> (String, bool, Option<String>) {
    let Ok(station_proxy) = StationProxy::new(connection, path.clone()).await else {
        return (String::new(), false, None);
    };

    let state = station_proxy.state().await.unwrap_or_default();
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
