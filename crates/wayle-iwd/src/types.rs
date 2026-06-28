//! Shared IWD type definitions.
//!
//! These mirror the equivalent `wayle-network` types so the UI layer can be
//! shared with minimal changes, while their constructors map from IWD's D-Bus
//! representation (string `Station.State`, string `Network.Type`,
//! `100 x dBm` signal strength) instead of NetworkManager's.

/// Connection-attempt-aware view of what the station is doing with respect to a
/// specific network — the single reactive state model for the "active
/// connection" UI.
///
/// IWD's raw `net.connman.iwd.Station.State` does not name the *target* of an
/// in-progress attempt (during a transition `Station.ConnectedNetwork` may still
/// point at the previous network), and reports no failure as a state. This type
/// augments the raw state with the target SSID. It is purely the positive state;
/// a failed attempt is surfaced via the `Result` returned by
/// [`Station::connect`](crate::Station::connect), not held here.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ConnectionState {
    /// Not connected and not attempting a connection.
    #[default]
    Idle,
    /// Establishing (or roaming to) a connection to `ssid`.
    Connecting {
        /// SSID being connected to.
        ssid: String,
    },
    /// Connected to `ssid`.
    Connected {
        /// SSID currently connected to.
        ssid: String,
    },
}

impl ConnectionState {
    /// Derives a connection state from IWD's raw `Station.State` string
    /// (`connected` / `connecting` / `disconnecting` / `disconnected` /
    /// `roaming`) and the resolved `ConnectedNetwork` SSID. `roaming` is treated
    /// as `Connected` (you remain associated to the same SSID while roaming
    /// between APs); only the terminal `disconnected`/`disconnecting` clear to
    /// `Idle`.
    pub(crate) fn from_raw_state(state: &str, connected_ssid: Option<String>) -> Self {
        match state {
            "connected" | "roaming" => {
                connected_ssid.map_or(Self::Idle, |ssid| Self::Connected { ssid })
            }
            "connecting" => connected_ssid.map_or(Self::Idle, |ssid| Self::Connecting { ssid }),
            _ => Self::Idle,
        }
    }

    /// The SSID of the active or in-progress connection, if any.
    pub fn ssid(&self) -> Option<&str> {
        match self {
            Self::Idle => None,
            Self::Connecting { ssid } | Self::Connected { ssid } => Some(ssid),
        }
    }
}

/// Security type classification for a network.
///
/// Variants mirror `wayle-network`'s `SecurityType` for UI compatibility.
/// IWD only distinguishes `open`, `wep`, `psk`, and `8021x`. Its `psk` type
/// covers WPA2 and WPA3 personal networks alike, so both are reported as the
/// ambiguous [`SecurityType::Psk`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SecurityType {
    /// No security (open network).
    None,
    /// WEP - deprecated and insecure.
    Wep,
    /// Pre-shared key (WPA2 or WPA3 personal) - reported for every IWD `psk`
    /// network, which does not distinguish the two.
    Psk,
    /// Enterprise security (802.1X).
    Enterprise,
}

impl SecurityType {
    /// Derives the security type from IWD's `Network.Type` string
    /// (`open` / `wep` / `psk` / `8021x`).
    pub(crate) fn from_iwd_type(network_type: &str) -> Self {
        match network_type {
            "wep" => Self::Wep,
            "psk" => Self::Psk,
            "8021x" => Self::Enterprise,
            _ => Self::None,
        }
    }
}

/// Converts an IWD signal strength from `Station.GetOrderedNetworks` (reported as
/// `100 * dBm`, e.g. `-6000` for -60 dBm, like iwgtk) to a 0-100 strength bucket.
pub(crate) fn signal_to_percent(signal_100dbm: i16) -> u8 {
    dbm_to_percent(i32::from(signal_100dbm) / 100)
}

/// Maps a dBm signal level to a 0-100 strength bucket using the same thresholds
/// as iwgtk (`{-60, -67, -74, -81}` dBm). The returned values sit in the middle
/// of the five signal-icon buckets so the rendered icon matches iwgtk's levels
/// (excellent / good / ok / weak / none).
///
/// Also used for the connected link's `RSSI` from
/// `StationDiagnostic.GetDiagnostics` (already plain dBm).
pub(crate) fn dbm_to_percent(dbm: i32) -> u8 {
    if dbm > -60 {
        100
    } else if dbm > -67 {
        75
    } else if dbm > -74 {
        55
    } else if dbm > -81 {
        35
    } else {
        10
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_state_from_raw_state() {
        let net = || Some(String::from("net"));
        assert_eq!(
            ConnectionState::from_raw_state("connected", net()),
            ConnectionState::Connected { ssid: "net".into() }
        );
        // Roaming stays Connected, not Connecting.
        assert_eq!(
            ConnectionState::from_raw_state("roaming", net()),
            ConnectionState::Connected { ssid: "net".into() }
        );
        assert_eq!(
            ConnectionState::from_raw_state("connecting", net()),
            ConnectionState::Connecting { ssid: "net".into() }
        );
        // Disconnecting/disconnected/unknown collapse to Idle.
        assert_eq!(ConnectionState::from_raw_state("disconnecting", net()), ConnectionState::Idle);
        assert_eq!(ConnectionState::from_raw_state("disconnected", None), ConnectionState::Idle);
        assert_eq!(ConnectionState::from_raw_state("connected", None), ConnectionState::Idle);
    }

    #[test]
    fn security_from_iwd_type() {
        assert_eq!(SecurityType::from_iwd_type("open"), SecurityType::None);
        assert_eq!(SecurityType::from_iwd_type("wep"), SecurityType::Wep);
        assert_eq!(SecurityType::from_iwd_type("psk"), SecurityType::Psk);
        assert_eq!(SecurityType::from_iwd_type("8021x"), SecurityType::Enterprise);
        assert_eq!(SecurityType::from_iwd_type("other"), SecurityType::None);
    }

    #[test]
    fn signal_conversion_100dbm_scale() {
        assert_eq!(signal_to_percent(-5000), 100); // -50 dBm -> excellent
        assert_eq!(signal_to_percent(-6500), 75); // -65 dBm -> good
        assert_eq!(signal_to_percent(-7000), 55); // -70 dBm -> ok
        assert_eq!(signal_to_percent(-8000), 35); // -80 dBm -> weak
        assert_eq!(signal_to_percent(-9000), 10); // -90 dBm -> none
    }

    #[test]
    fn dbm_thresholds_match_iwgtk() {
        assert_eq!(dbm_to_percent(-55), 100); // excellent
        assert_eq!(dbm_to_percent(-60), 75); // boundary: not > -60 -> good
        assert_eq!(dbm_to_percent(-65), 75); // good
        assert_eq!(dbm_to_percent(-70), 55); // ok
        assert_eq!(dbm_to_percent(-78), 35); // weak
        assert_eq!(dbm_to_percent(-85), 10); // none
    }
}
