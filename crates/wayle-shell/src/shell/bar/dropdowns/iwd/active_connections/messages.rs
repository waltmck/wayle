use std::sync::Arc;

use wayle_iwd::{ConnectionState, IwdService, Station};

use crate::shell::bar::dropdowns::iwd::helpers;

pub(crate) struct ActiveConnectionsInit {
    pub iwd: Arc<IwdService>,
}

pub(super) struct WifiState {
    /// Service-owned, attempt-aware connection state — the single source of
    /// truth for what the card shows as the active connection.
    pub connection: ConnectionState,
    pub strength: Option<u8>,
    pub icon: &'static str,
    pub frequency: Option<u32>,
    pub hovered: bool,
}

impl WifiState {
    pub(super) fn from_station(station: &Station) -> Self {
        let strength = station.strength.get();

        Self {
            connection: station.connection.get(),
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
            connection: ConnectionState::Idle,
            strength: None,
            icon: helpers::signal_strength_icon(0),
            frequency: None,
            hovered: false,
        }
    }
}

/// A failed connection attempt to display on the card. Transient and
/// shell-owned (mirrors the NetworkManager dropdown's `ConnectionProgress`);
/// shown only while the station is otherwise idle.
pub(super) struct ConnectionError {
    pub ssid: String,
    pub message: String,
}

#[derive(Debug)]
pub(crate) enum ActiveConnectionsInput {
    DisconnectWifi,
    ForgetWifi,
    DismissError,
    WifiCardHovered(bool),
    /// Show a failed-connection error on the card, routed from the
    /// available-networks list (which owns the connect command and its result).
    ShowError { ssid: String, message: String },
}

#[derive(Debug)]
#[allow(clippy::enum_variant_names)]
pub(crate) enum ActiveConnectionsCmd {
    WifiChanged {
        connection: ConnectionState,
        strength: Option<u8>,
        frequency: Option<u32>,
    },
    StationDeviceChanged,
}
