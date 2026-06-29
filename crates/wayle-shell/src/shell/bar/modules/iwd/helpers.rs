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

/// Map a [`SignalStrength`] bucket onto the configured icon list, scaling when
/// the list has a different number of entries than there are buckets (the default
/// has 4 icons, no "none" — so None and Weak both map to the weakest icon).
fn signal_to_index(strength: SignalStrength, num_icons: usize) -> usize {
    if num_icons == 0 {
        return 0;
    }
    (strength.index() * num_icons / SignalStrength::COUNT).min(num_icons - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signal_to_index_four_icons() {
        // Default config: 4 icons (weak/ok/good/excellent), no "none" — so None
        // and Weak both fall to the weakest available icon.
        assert_eq!(signal_to_index(SignalStrength::None, 4), 0);
        assert_eq!(signal_to_index(SignalStrength::Weak, 4), 0);
        assert_eq!(signal_to_index(SignalStrength::Ok, 4), 1);
        assert_eq!(signal_to_index(SignalStrength::Good, 4), 2);
        assert_eq!(signal_to_index(SignalStrength::Excellent, 4), 3);
    }

    #[test]
    fn signal_to_index_five_icons_is_identity() {
        assert_eq!(signal_to_index(SignalStrength::None, 5), 0);
        assert_eq!(signal_to_index(SignalStrength::Excellent, 5), 4);
    }

    #[test]
    fn signal_to_index_empty() {
        assert_eq!(signal_to_index(SignalStrength::Ok, 0), 0);
    }
}
