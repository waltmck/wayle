use std::sync::Arc;

use wayle_iwd::{IwdService, NetworkStatus, Station};

use crate::shell::bar::dropdowns::iwd::helpers;

pub(crate) struct ActiveConnectionsInit {
    pub iwd: Arc<IwdService>,
}

pub(super) struct WifiState {
    pub connected: bool,
    pub connecting: bool,
    pub ssid: Option<String>,
    pub strength: Option<u8>,
    pub icon: &'static str,
    pub frequency: Option<u32>,
    pub hovered: bool,
}

impl WifiState {
    pub(super) fn from_station(station: &Station) -> Self {
        let connectivity = station.state.get();
        let strength = station.strength.get();

        Self {
            connected: connectivity == NetworkStatus::Connected,
            connecting: connectivity == NetworkStatus::Connecting,
            ssid: station.connected_ssid.get(),
            strength,
            icon: helpers::signal_strength_icon(strength.unwrap_or(0)),
            frequency: station.frequency.get(),
            hovered: false,
        }
    }
}

impl Default for WifiState {
    fn default() -> Self {
        Self {
            connected: false,
            connecting: false,
            ssid: None,
            strength: None,
            icon: helpers::signal_strength_icon(0),
            frequency: None,
            hovered: false,
        }
    }
}

#[derive(Default)]
pub(super) struct ConnectionProgress {
    pub ssid: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug)]
pub(crate) enum ActiveConnectionsInput {
    DisconnectWifi,
    CancelWifi,
    ForgetWifi,
    DismissError,
    WifiCardHovered(bool),
    SetConnecting(String),
    ClearConnecting,
    SetConnectionError(String),
    ClearConnectionError,
}

#[derive(Debug)]
#[allow(clippy::enum_variant_names)]
pub(crate) enum ActiveConnectionsCmd {
    WifiStateChanged {
        connectivity: NetworkStatus,
        ssid: Option<String>,
        strength: Option<u8>,
        frequency: Option<u32>,
    },
    StationDeviceChanged,
}
