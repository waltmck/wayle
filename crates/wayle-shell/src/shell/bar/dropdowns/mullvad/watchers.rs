use std::sync::Arc;

use relm4::ComponentSender;
use tokio_util::sync::CancellationToken;
use wayle_config::ConfigService;
use wayle_core::DeferredService;
use wayle_mullvad::MullvadService;
use wayle_widgets::{watch, watch_cancellable, watch_deferred};

use super::{MullvadDropdown, messages::MullvadDropdownCmd};

pub(super) fn spawn_config_watcher(
    sender: &ComponentSender<MullvadDropdown>,
    config: &Arc<ConfigService>,
) {
    let scale = config.config().styling.scale.clone();

    watch!(sender, [scale.watch()], |out| {
        let _ = out.send(MullvadDropdownCmd::ScaleChanged(scale.get().value()));
    });
}

pub(super) fn spawn_service_watcher(
    sender: &ComponentSender<MullvadDropdown>,
    mullvad: &DeferredService<MullvadService>,
) {
    watch_deferred!(sender, mullvad, MullvadDropdownCmd::ServiceReady);
}

pub(super) fn spawn_state_watchers(
    sender: &ComponentSender<MullvadDropdown>,
    token: CancellationToken,
    mullvad: &Arc<MullvadService>,
) {
    let logged_in = mullvad.mullvad.logged_in.clone();

    watch_cancellable!(sender, token.clone(), [logged_in.watch()], |out| {
        let _ = out.send(MullvadDropdownCmd::LoggedInChanged);
    });

    let networks = mullvad.mullvad.networks.clone();

    watch_cancellable!(sender, token.clone(), [networks.watch()], |out| {
        let _ = out.send(MullvadDropdownCmd::RelaysChanged);
    });

    let connection_state = mullvad.mullvad.connection_state.clone();
    let connected_network = mullvad.mullvad.connected_network.clone();

    watch_cancellable!(
        sender,
        token,
        [connection_state.watch(), connected_network.watch()],
        |out| {
            let _ = out.send(MullvadDropdownCmd::TunnelChanged);
        }
    );
}
