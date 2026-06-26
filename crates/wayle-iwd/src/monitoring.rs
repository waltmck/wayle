//! Service-level monitoring: station hot-plug via ObjectManager signals.

use std::sync::Arc;

use futures::StreamExt;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};
use wayle_core::Property;
use wayle_traits::ServiceMonitoring;
use zbus::Connection;

use crate::{
    agent::PassphraseStore,
    discovery::DEVICE_INTERFACE,
    error::Error,
    proxy::object_manager::ObjectManagerProxy,
    service::{IwdService, build_station},
    station::Station,
};

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
    let proxy = ObjectManagerProxy::new(&connection)
        .await
        .map_err(Error::DbusError)?;

    let mut interfaces_added = proxy
        .receive_interfaces_added()
        .await
        .map_err(Error::DbusError)?;
    let mut interfaces_removed = proxy
        .receive_interfaces_removed()
        .await
        .map_err(Error::DbusError)?;

    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = cancellation_token.cancelled() => {
                    debug!("iwd station hot-plug monitoring cancelled");
                    return;
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
                        station.set(new_station);
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
                        station.set(None);
                    }
                }
            }
        }
    });

    Ok(())
}
