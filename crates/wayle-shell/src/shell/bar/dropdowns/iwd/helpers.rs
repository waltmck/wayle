use std::{collections::HashSet, sync::Arc};

use wayle_iwd::{Network, SecurityType, SignalStrength};
use zbus::zvariant::OwnedObjectPath;

/// Snapshot of an IWD network for display in the network list.
#[derive(Debug, Clone)]
pub(crate) struct NetworkSnapshot {
    pub ssid: String,
    pub strength: SignalStrength,
    pub security: SecurityType,
    pub object_path: OwnedObjectPath,
    pub known: bool,
}

/// Picks the configured signal-strength icon for a bucket, scaling the bucket
/// onto the configured icon list (`config.wifi_signal_icons`); `fallback` is the
/// configured "connected, strength unknown" icon used when the list is empty.
pub(crate) fn signal_strength_icon(
    strength: SignalStrength,
    icons: &[String],
    fallback: &str,
) -> String {
    if icons.is_empty() {
        return fallback.to_string();
    }
    let idx = (strength.index() * icons.len() / SignalStrength::COUNT).min(icons.len() - 1);
    icons.get(idx).cloned().unwrap_or_else(|| fallback.to_string())
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

/// Deduplicates networks by SSID and filters out hidden networks and the active
/// SSID (the network shown in the active-connection card — either the connected
/// network or the in-progress connecting target).
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
        // Default config: 4 icons (weak/ok/good/excellent), no "none" — so None
        // and Weak both map to the weakest icon.
        let icons = vec![
            String::from("weak"),
            String::from("ok"),
            String::from("good"),
            String::from("excellent"),
        ];
        assert_eq!(signal_strength_icon(SignalStrength::None, &icons, "connected"), "weak");
        assert_eq!(signal_strength_icon(SignalStrength::Weak, &icons, "connected"), "weak");
        assert_eq!(signal_strength_icon(SignalStrength::Ok, &icons, "connected"), "ok");
        assert_eq!(signal_strength_icon(SignalStrength::Good, &icons, "connected"), "good");
        assert_eq!(
            signal_strength_icon(SignalStrength::Excellent, &icons, "connected"),
            "excellent"
        );
        // Empty list falls back to the configured connected icon.
        assert_eq!(signal_strength_icon(SignalStrength::Good, &[], "connected"), "connected");
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
