//! Background property monitoring for a [`Station`].

use std::{
    sync::{Arc, Weak},
    time::Duration,
};

use futures::StreamExt;
use tokio::time::interval;
use tokio_util::sync::CancellationToken;
use tracing::debug;
use wayle_traits::ModelMonitoring;
use zbus::zvariant::OwnedObjectPath;

use super::{Station, resolve_connected_ssid};
use crate::{
    discovery::NETWORK_INTERFACE,
    error::Error,
    proxy::{
        device::DeviceProxy, diagnostic::StationDiagnosticProxy, object_manager::ObjectManagerProxy,
        station::StationProxy,
    },
    types::{ConnectionState, dbm_to_percent},
};

const DIAGNOSTIC_POLL_INTERVAL: Duration = Duration::from_secs(5);

impl ModelMonitoring for Station {
    type Error = Error;

    async fn start_monitoring(self: Arc<Self>) -> Result<(), Self::Error> {
        let Some(ref cancellation_token) = self.cancellation_token else {
            return Err(Error::MissingCancellationToken);
        };
        let cancel = cancellation_token.clone();

        let device_proxy = DeviceProxy::new(&self.zbus_connection, self.object_path.clone())
            .await
            .map_err(Error::DbusError)?;
        let station_proxy = StationProxy::new(&self.zbus_connection, self.object_path.clone())
            .await
            .map_err(Error::DbusError)?;
        let object_manager = ObjectManagerProxy::new(&self.zbus_connection)
            .await
            .map_err(Error::DbusError)?;

        // Populate the initial scan list (IWD returns its last cached results).
        self.refresh_networks().await;

        let weak_self = Arc::downgrade(&self);
        tokio::spawn(async move {
            monitor(weak_self, device_proxy, station_proxy, object_manager, cancel).await;
        });

        Ok(())
    }
}

/// Whether `candidate` is an object underneath `parent` (i.e. a network object
/// belonging to our device).
fn is_descendant(candidate: &OwnedObjectPath, parent: &OwnedObjectPath) -> bool {
    candidate
        .as_str()
        .strip_prefix(parent.as_str())
        .is_some_and(|rest| rest.starts_with('/'))
}

async fn monitor(
    weak_station: Weak<Station>,
    device_proxy: DeviceProxy<'static>,
    station_proxy: StationProxy<'static>,
    object_manager: ObjectManagerProxy<'static>,
    cancellation_token: CancellationToken,
) {
    let mut powered_changed = device_proxy.receive_powered_changed().await;
    let mut state_changed = station_proxy.receive_state_changed().await;
    let mut scanning_changed = station_proxy.receive_scanning_changed().await;
    let mut connected_changed = station_proxy.receive_connected_network_changed().await;
    let mut interfaces_added = match object_manager.receive_interfaces_added().await {
        Ok(stream) => stream,
        Err(err) => {
            debug!(error = %err, "cannot watch interfaces added");
            return;
        }
    };
    let mut interfaces_removed = match object_manager.receive_interfaces_removed().await {
        Ok(stream) => stream,
        Err(err) => {
            debug!(error = %err, "cannot watch interfaces removed");
            return;
        }
    };
    let mut poll = interval(DIAGNOSTIC_POLL_INTERVAL);

    loop {
        let Some(station) = weak_station.upgrade() else {
            return;
        };

        tokio::select! {
            _ = cancellation_token.cancelled() => {
                debug!("station monitor cancelled");
                return;
            }

            _ = poll.tick() => {
                if matches!(station.connection.get(), ConnectionState::Connected { .. }) {
                    update_diagnostics(&station).await;
                }
            }

            Some(change) = powered_changed.next() => {
                if let Ok(powered) = change.get().await {
                    let was_powered = station.powered.get();
                    station.powered.set(powered);

                    if powered == was_powered {
                        // Initial/no-op emission (e.g. at startup): do not scan.
                    } else if powered {
                        // Genuine off -> on: the Station interface reappears, so
                        // re-read its state and scan for networks.
                        station.resync_after_power_on().await;
                    } else {
                        // Genuine on -> off: clear station state.
                        station.connection.set(ConnectionState::Idle);
                        station.scanning.set(false);
                        station.strength.set(None);
                        station.frequency.set(None);
                        station.networks.replace(Vec::new());
                    }
                }
            }

            Some(change) = state_changed.next() => {
                if let Ok(state) = change.get().await {
                    let connected_ssid =
                        resolve_connected_ssid(&station.zbus_connection, &station.object_path).await;
                    station.observe_connection(&state, connected_ssid);

                    if state == "connected" {
                        update_diagnostics(&station).await;
                    } else {
                        station.strength.set(None);
                        station.frequency.set(None);
                    }

                    station.refresh_networks().await;
                }
            }

            Some(change) = scanning_changed.next() => {
                if let Ok(scanning) = change.get().await {
                    station.scanning.set(scanning);
                    // A finished scan means fresh ordered-network results.
                    if !scanning {
                        station.refresh_networks().await;
                    }
                }
            }

            Some(_) = connected_changed.next() => {
                let connected_ssid =
                    resolve_connected_ssid(&station.zbus_connection, &station.object_path).await;
                // `ConnectedNetwork` changed but `State` did not, so read it
                // fresh to classify the new connection correctly.
                let state = station_proxy.state().await.unwrap_or_default();
                station.observe_connection(&state, connected_ssid);
                station.refresh_networks().await;
            }

            Some(signal) = interfaces_added.next() => {
                if let Ok(args) = signal.args()
                    && station.powered.get()
                    && args.interfaces.contains_key(NETWORK_INTERFACE)
                    && is_descendant(&args.object_path, &station.object_path)
                {
                    station.refresh_networks().await;
                }
            }

            Some(signal) = interfaces_removed.next() => {
                if let Ok(args) = signal.args()
                    && station.powered.get()
                    && args.interfaces.iter().any(|iface| iface.as_str() == NETWORK_INTERFACE)
                    && is_descendant(&args.object_path, &station.object_path)
                {
                    station.refresh_networks().await;
                }
            }

            else => {
                break;
            }
        }
    }
}

/// Read diagnostics (RSSI -> strength, frequency) and publish them. Callers must
/// only invoke this while connected; the `state_changed` "connected" branch reads
/// immediately so the link's strength/frequency appear at once rather than on the
/// next poll tick (the optimistic `connection` may still be `Connecting` during a
/// foreground attempt, so this no longer gates on it).
async fn update_diagnostics(station: &Station) {
    let Ok(proxy) = StationDiagnosticProxy::new(&station.zbus_connection, station.object_path.clone()).await
    else {
        return;
    };

    let Ok(diagnostics) = proxy.get_diagnostics().await else {
        // Diagnostics may require elevated privileges; treat as unavailable.
        return;
    };

    if let Some(rssi) = diagnostics.get("RSSI").and_then(|v| i16::try_from(v).ok()) {
        station.strength.set(Some(dbm_to_percent(i32::from(rssi))));
    }

    if let Some(frequency) = diagnostics.get("Frequency").and_then(|v| u32::try_from(v).ok()) {
        station.frequency.set(Some(frequency));
    }
}
