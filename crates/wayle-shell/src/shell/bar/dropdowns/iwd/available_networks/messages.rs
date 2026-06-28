use std::sync::Arc;

use wayle_iwd::IwdService;
use zbus::zvariant::OwnedObjectPath;

use crate::shell::bar::dropdowns::iwd::password_form::PasswordFormOutput;

pub(super) struct SelectedNetwork {
    pub network_path: OwnedObjectPath,
    pub ssid: String,
    pub security_label: String,
    pub signal_icon: &'static str,
    pub secured: bool,
}

pub(crate) struct AvailableNetworksInit {
    pub iwd: Arc<IwdService>,
}

#[derive(Debug)]
pub(crate) enum AvailableNetworksInput {
    WifiAvailabilityChanged(bool),
    WifiEnabledChanged(bool),
    NetworkSelected(usize),
    ForgetNetwork(OwnedObjectPath),
    PasswordForm(PasswordFormOutput),
    /// The connection was stopped from the active-connection card (Cancel/Forget
    /// while connecting); leave the Connecting state immediately.
    AbortConnecting,
}

#[derive(Debug)]
pub(crate) enum AvailableNetworksCmd {
    NetworksChanged,
    ConnectionActivated,
    ConnectionAuthFailed,
    ConnectionFailed(String),
}

#[derive(Debug)]
pub(crate) enum AvailableNetworksOutput {
    Connecting(String),
    ClearConnecting,
    Connected,
    ConnectionFailed(String),
}
