use wayle_config::schemas::modules::IwdConfig;
use wayle_iwd::{ConnectionState, SignalStrength};

use crate::i18n::t;

pub(crate) struct WifiContext {
    pub(crate) enabled: bool,
    /// Attempt-aware connection state — the same source the dropdown card uses,
    /// so the bar shows "connecting" throughout a switch instead of flickering to
    /// "disconnected" while IWD's raw `State` passes through `disconnected`.
    pub(crate) connection: ConnectionState,
    pub(crate) strength: Option<SignalStrength>,
}

pub(crate) fn wifi_icon(config: &IwdConfig, ctx: &WifiContext) -> String {
    if !ctx.enabled {
        return config.wifi_disabled_icon.get().clone();
    }

    match ctx.connection {
        ConnectionState::Connecting { .. } => config.wifi_acquiring_icon.get().clone(),
        ConnectionState::Idle => config.wifi_offline_icon.get().clone(),
        // Roaming is still connected — show the signal-strength icon.
        ConnectionState::Connected { .. } | ConnectionState::Roaming { .. } => {
            let icons = config.wifi_signal_icons.get();
            ctx.strength
                .and_then(|s| s.icon_index(icons.len()))
                .and_then(|idx| icons.get(idx).cloned())
                .unwrap_or_else(|| config.wifi_connected_icon.get().clone())
        }
    }
}

pub(crate) fn wifi_label(ctx: &WifiContext) -> String {
    match &ctx.connection {
        ConnectionState::Connected { ssid } | ConnectionState::Roaming { ssid }
            if !ssid.is_empty() =>
        {
            ssid.clone()
        }
        ConnectionState::Connected { .. } | ConnectionState::Roaming { .. } => {
            t!("bar-iwd-wifi-fallback")
        }
        ConnectionState::Connecting { .. } => t!("bar-iwd-connecting"),
        ConnectionState::Idle => t!("bar-iwd-disconnected"),
    }
}

