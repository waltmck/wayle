use wayle_config::schemas::modules::IwdConfig;
use wayle_iwd::ConnectionState;

use crate::i18n::t;

pub(crate) struct WifiContext {
    pub(crate) enabled: bool,
    /// Attempt-aware connection state — the same source the dropdown card uses,
    /// so the bar shows "connecting" throughout a switch instead of flickering to
    /// "disconnected" while IWD's raw `State` passes through `disconnected`.
    pub(crate) connection: ConnectionState,
    pub(crate) strength: Option<u8>,
}

pub(crate) fn wifi_icon(config: &IwdConfig, ctx: &WifiContext) -> String {
    if !ctx.enabled {
        return config.wifi_disabled_icon.get().clone();
    }

    match ctx.connection {
        ConnectionState::Connecting { .. } => config.wifi_acquiring_icon.get().clone(),
        ConnectionState::Idle => config.wifi_offline_icon.get().clone(),
        ConnectionState::Connected { .. } => {
            let icons = config.wifi_signal_icons.get();
            match ctx.strength {
                Some(s) if !icons.is_empty() => {
                    let idx = signal_to_index(s, icons.len());
                    icons
                        .get(idx)
                        .cloned()
                        .unwrap_or_else(|| config.wifi_connected_icon.get().clone())
                }
                _ => config.wifi_connected_icon.get().clone(),
            }
        }
    }
}

pub(crate) fn wifi_label(ctx: &WifiContext) -> String {
    match &ctx.connection {
        ConnectionState::Connected { ssid } if !ssid.is_empty() => ssid.clone(),
        ConnectionState::Connected { .. } => t!("bar-iwd-wifi-fallback"),
        ConnectionState::Connecting { .. } => t!("bar-iwd-connecting"),
        ConnectionState::Idle => t!("bar-iwd-disconnected"),
    }
}

fn signal_to_index(strength: u8, num_icons: usize) -> usize {
    if num_icons == 0 {
        return 0;
    }
    let clamped = strength.min(100) as usize;
    let bucket_size = 100 / num_icons;
    let idx = clamped / bucket_size.max(1);
    idx.min(num_icons - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signal_to_index_four_icons() {
        assert_eq!(signal_to_index(0, 4), 0);
        assert_eq!(signal_to_index(24, 4), 0);
        assert_eq!(signal_to_index(25, 4), 1);
        assert_eq!(signal_to_index(50, 4), 2);
        assert_eq!(signal_to_index(75, 4), 3);
        assert_eq!(signal_to_index(100, 4), 3);
    }

    #[test]
    fn signal_to_index_empty() {
        assert_eq!(signal_to_index(50, 0), 0);
    }
}
