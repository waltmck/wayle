use std::sync::Arc;

use relm4::ComponentSender;
use tokio_util::sync::CancellationToken;
use wayle_iwd::IwdService;
use wayle_widgets::{watch, watch_cancellable};

use crate::shell::bar::dropdowns::iwd::active_connections::{
    ActiveConnections, messages::ActiveConnectionsCmd,
};

pub(super) fn spawn_wifi_watchers(
    sender: &ComponentSender<ActiveConnections>,
    iwd: &Arc<IwdService>,
    token: CancellationToken,
) {
    let Some(station) = iwd.station.get() else {
        return;
    };

    let connection = station.connection.clone();
    let strength = station.strength.clone();
    let frequency = station.frequency.clone();

    watch_cancellable!(
        sender,
        token,
        [connection.watch(), strength.watch(), frequency.watch()],
        |out| {
            let _ = out.send(ActiveConnectionsCmd::WifiChanged {
                connection: connection.get(),
                strength: strength.get(),
                frequency: frequency.get(),
            });
        }
    );
}

pub(super) fn spawn_device_watchers(
    sender: &ComponentSender<ActiveConnections>,
    iwd: &Arc<IwdService>,
) {
    let station = iwd.station.clone();
    watch!(sender, [station.watch()], |out| {
        let _ = out.send(ActiveConnectionsCmd::StationDeviceChanged);
    });
}
