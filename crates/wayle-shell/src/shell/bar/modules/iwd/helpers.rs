use wayle_config::schemas::modules::IwdConfig;
use wayle_iwd::NetworkStatus;

use crate::i18n::t;

pub(crate) struct WifiContext<'a> {
    pub(crate) enabled: bool,
    pub(crate) connectivity: NetworkStatus,
    pub(crate) strength: Option<u8>,
    pub(crate) ssid: Option<&'a str>,
}

pub(crate) fn wifi_icon(config: &IwdConfig, ctx: &WifiContext<'_>) -> String {
    if !ctx.enabled {
        return config.wifi_disabled_icon.get().clone();
    }

    match ctx.connectivity {
        NetworkStatus::Connecting => config.wifi_acquiring_icon.get().clone(),
        NetworkStatus::Disconnected => config.wifi_offline_icon.get().clone(),
        NetworkStatus::Connected => {
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

pub(crate) fn wifi_label(ctx: &WifiContext<'_>) -> String {
    match ctx.connectivity {
        NetworkStatus::Connected => ctx
            .ssid
            .map(String::from)
            .unwrap_or_else(|| t!("bar-iwd-wifi-fallback")),
        NetworkStatus::Connecting => t!("bar-iwd-connecting"),
        NetworkStatus::Disconnected => t!("bar-iwd-disconnected"),
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
