use wayle_config::schemas::modules::MullvadConfig;
use wayle_mullvad::{ConnectedNetwork, ConnectionState};

use crate::i18n::t;

pub(crate) struct MullvadContext {
    pub(crate) logged_in: bool,
    pub(crate) state: ConnectionState,
    pub(crate) network: Option<ConnectedNetwork>,
}

pub(crate) fn select_icon(config: &MullvadConfig, ctx: &MullvadContext) -> String {
    if !ctx.logged_in {
        return config.disabled_icon.get().clone();
    }

    match ctx.state {
        ConnectionState::Connected => config.connected_icon.get().clone(),
        ConnectionState::Connecting | ConnectionState::Disconnecting => {
            config.connecting_icon.get().clone()
        }
        ConnectionState::Disconnected => config.disconnected_icon.get().clone(),
        ConnectionState::Error => config.blocked_icon.get().clone(),
    }
}

pub(crate) fn format_label(ctx: &MullvadContext) -> String {
    if !ctx.logged_in {
        return t!("bar-mullvad-logged-out");
    }

    match ctx.state {
        ConnectionState::Connected => ctx
            .network
            .as_ref()
            .and_then(|network| network.city.clone())
            .filter(|city| !city.is_empty())
            .unwrap_or_else(|| t!("bar-mullvad-connected")),
        ConnectionState::Connecting => t!("bar-mullvad-connecting"),
        ConnectionState::Disconnecting => t!("bar-mullvad-disconnecting"),
        ConnectionState::Disconnected => t!("bar-mullvad-disconnected"),
        ConnectionState::Error => t!("bar-mullvad-blocked"),
    }
}
