//! Service-level monitoring: rebuild the station when IWD's bus name comes and
//! goes (the analogue of iwgtk's `g_bus_watch_name`), and handle device hot-plug
//! via ObjectManager signals while IWD is running.

use std::sync::Arc;

use futures::StreamExt;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};
use wayle_core::Property;
use wayle_traits::ServiceMonitoring;
use zbus::{Connection, Proxy};

use crate::{
    agent::PassphraseStore,
    discovery::{DEVICE_INTERFACE, IwdDiscovery},
    error::Error,
    proxy::object_manager::ObjectManagerProxy,
    service::{IwdService, build_station},
    station::Station,
};

/// IWD's well-known bus name.
const IWD_BUS_NAME: &str = "net.connman.iwd";

impl ServiceMonitoring for IwdService {
    type Error = Error;

    async fn start_monitoring(&self) -> Result<(), Self::Error> {
        spawn_station_monitoring(
            self.zbus_connection.clone(),
            self.station.clone(),
            self.passphrases.clone(),
            self.cancellation_token.child_token(),
        )
        .await
    }
}

async fn spawn_station_monitoring(
    connection: Connection,
    station: Property<Option<Arc<Station>>>,
    passphrases: Arc<PassphraseStore>,
    cancellation_token: CancellationToken,
) -> Result<(), Error> {
    let object_manager = ObjectManagerProxy::new(&connection)
        .await
        .map_err(Error::DbusError)?;

    let mut interfaces_added = object_manager
        .receive_interfaces_added()
        .await
        .map_err(Error::DbusError)?;
    let mut interfaces_removed = object_manager
        .receive_interfaces_removed()
        .await
        .map_err(Error::DbusError)?;

    // Watch IWD's bus-name ownership so we rebuild from scratch whenever IWD
    // restarts or re-registers — notably across suspend/resume, where the
    // existing signal subscriptions can silently stop delivering. This mirrors
    // iwgtk's `g_bus_watch_name` (iwd_up / iwd_down).
    let iwd = Proxy::new(&connection, IWD_BUS_NAME, "/", "org.freedesktop.DBus.Peer")
        .await
        .map_err(Error::DbusError)?;
    let mut owner_changed = iwd.receive_owner_changed().await.map_err(Error::DbusError)?;

    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = cancellation_token.cancelled() => {
                    debug!("iwd station monitoring cancelled");
                    return;
                }

                Some(owner) = owner_changed.next() => {
                    // Tear down the old station's monitor (its signal streams may
                    // be dead) before rebuilding against the new owner.
                    if let Some(current) = station.get() {
                        current.shutdown();
                    }

                    match owner {
                        None => {
                            debug!("iwd left the bus");
                            station.replace(None);
                        }
                        Some(_) => {
                            debug!("iwd (re)appeared on the bus; rebuilding station");
                            let new_station = match IwdDiscovery::device_path(&connection).await {
                                Ok(Some(path)) => {
                                    build_station(
                                        &connection,
                                        path,
                                        &cancellation_token,
                                        passphrases.clone(),
                                    )
                                    .await
                                }
                                _ => None,
                            };
                            station.replace(new_station);
                        }
                    }
                }

                Some(signal) = interfaces_added.next() => {
                    let Ok(args) = signal.args() else { continue };

                    if !args.interfaces.contains_key(DEVICE_INTERFACE) || station.get().is_some() {
                        continue;
                    }

                    debug!(path = %args.object_path, "iwd device appeared");
                    let new_station = build_station(
                        &connection,
                        args.object_path.clone(),
                        &cancellation_token,
                        passphrases.clone(),
                    )
                    .await;

                    if new_station.is_some() {
                        station.replace(new_station);
                    } else {
                        warn!("iwd device appeared but could not be initialized");
                    }
                }

                Some(signal) = interfaces_removed.next() => {
                    let Ok(args) = signal.args() else { continue };

                    let lost_device = args
                        .interfaces
                        .iter()
                        .any(|iface| iface.as_str() == DEVICE_INTERFACE);
                    if !lost_device {
                        continue;
                    }

                    if let Some(current) = station.get()
                        && current.object_path().as_str() == args.object_path.as_str()
                    {
                        debug!(path = %args.object_path, "iwd device removed");
                        current.shutdown();
                        station.replace(None);
                    }
                }
            }
        }
    });

    Ok(())
}
