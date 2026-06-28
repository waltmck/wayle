use std::sync::Arc;

use tokio_util::sync::CancellationToken;
use wayle_iwd::Station;
use wayle_widgets::watch_cancellable;
use relm4::ComponentSender;

use crate::shell::bar::dropdowns::iwd::available_networks::{
    AvailableNetworks, messages::AvailableNetworksCmd,
};

pub(super) fn spawn(
    sender: &ComponentSender<AvailableNetworks>,
    station: &Arc<Station>,
    token: CancellationToken,
) {
    let networks = station.networks.clone();
    let connection = station.connection.clone();
    watch_cancellable!(
        sender,
        token,
        [networks.watch(), connection.watch()],
        |out| {
            let _ = out.send(AvailableNetworksCmd::NetworksChanged);
        }
    );
}
