use std::{collections::HashSet, sync::Arc};

use wayle_iwd::{Network, SecurityType, SignalStrength};
use zbus::zvariant::OwnedObjectPath;

pub(crate) use crate::shell::bar::dropdowns::{
    connected_signal_icon, frequency_to_band, signal_strength_icon,
};

/// Snapshot of an IWD network for display in the network list.
#[derive(Debug, Clone)]
pub(crate) struct NetworkSnapshot {
    pub ssid: String,
    pub strength: SignalStrength,
    pub security: SecurityType,
    pub object_path: OwnedObjectPath,
    pub known: bool,
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
    fn requires_password_logic() {
        assert!(!requires_password(SecurityType::None));
        assert!(!requires_password(SecurityType::Enterprise));
        assert!(requires_password(SecurityType::Psk));
        assert!(requires_password(SecurityType::Wep));
    }
}
