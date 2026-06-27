use std::{collections::HashSet, sync::Arc};

use wayle_iwd::{Network, SecurityType};
use zbus::zvariant::OwnedObjectPath;

/// Snapshot of an IWD network for display in the network list.
#[derive(Debug, Clone)]
pub(crate) struct NetworkSnapshot {
    pub ssid: String,
    pub strength: u8,
    pub security: SecurityType,
    pub object_path: OwnedObjectPath,
    pub known: bool,
}

pub(crate) fn signal_strength_icon(strength: u8) -> &'static str {
    // Standard freedesktop/Adwaita icons (as iwgtk uses). Unlike wayle's bundled
    // `cm-wireless-signal-*` icons, these render with proper per-level distinction.
    match strength {
        0..=19 => "network-wireless-signal-none-symbolic",
        20..=39 => "network-wireless-signal-weak-symbolic",
        40..=59 => "network-wireless-signal-ok-symbolic",
        60..=79 => "network-wireless-signal-good-symbolic",
        _ => "network-wireless-signal-excellent-symbolic",
    }
}

pub(crate) fn frequency_to_band(freq_mhz: u32) -> Option<&'static str> {
    match freq_mhz {
        2400..=2500 => Some("2.4 GHz"),
        5000..=5900 => Some("5 GHz"),
        5901..=7125 => Some("6 GHz"),
        57000..=71000 => Some("60 GHz"),
        _ => None,
    }
}

pub(crate) fn requires_password(security: SecurityType) -> bool {
    !matches!(security, SecurityType::None | SecurityType::Enterprise)
}

/// Deduplicates networks by SSID and filters out hidden networks, enterprise
/// networks, and the active SSID (the network shown in the active-connection
/// card — either the connected network or the in-progress connecting target).
///
/// The input is expected to already be ordered strongest-first (as
/// `Station.GetOrderedNetworks` returns it), and that order is preserved — like
/// iwgtk, we keep IWD's ordering rather than re-sorting by the coarse signal
/// bucket.
pub(crate) fn unique_networks(
    networks: &[Arc<Network>],
    active_ssid: Option<&str>,
) -> Vec<NetworkSnapshot> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut snapshots: Vec<NetworkSnapshot> = Vec::new();

    for network in networks {
        let ssid = network.ssid.get();
        if ssid.is_empty() {
            continue;
        }

        let security = network.security.get();
        if security == SecurityType::Enterprise {
            continue;
        }

        if active_ssid.is_some_and(|active| active == ssid) {
            continue;
        }

        // First occurrence per SSID is the strongest (input is sorted).
        if !seen.insert(ssid.clone()) {
            continue;
        }

        snapshots.push(NetworkSnapshot {
            ssid: ssid.clone(),
            strength: network.strength.get(),
            security,
            object_path: network.object_path().clone(),
            known: network.known.get(),
        });
    }

    snapshots
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signal_icon_buckets() {
        assert_eq!(signal_strength_icon(0), "network-wireless-signal-none-symbolic");
        assert_eq!(signal_strength_icon(30), "network-wireless-signal-weak-symbolic");
        assert_eq!(signal_strength_icon(50), "network-wireless-signal-ok-symbolic");
        assert_eq!(signal_strength_icon(70), "network-wireless-signal-good-symbolic");
        assert_eq!(
            signal_strength_icon(90),
            "network-wireless-signal-excellent-symbolic"
        );
    }

    #[test]
    fn requires_password_logic() {
        assert!(!requires_password(SecurityType::None));
        assert!(!requires_password(SecurityType::Enterprise));
        assert!(requires_password(SecurityType::Psk));
        assert!(requires_password(SecurityType::Wep));
    }

    #[test]
    fn frequency_bands() {
        assert_eq!(frequency_to_band(2412), Some("2.4 GHz"));
        assert_eq!(frequency_to_band(5180), Some("5 GHz"));
        assert_eq!(frequency_to_band(5955), Some("6 GHz"));
        assert_eq!(frequency_to_band(900), None);
    }
}
