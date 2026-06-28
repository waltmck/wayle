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
}

#[derive(Debug)]
pub(crate) enum AvailableNetworksCmd {
    /// The service connection state or the scan list changed; re-dismiss a stale
    /// password prompt and rebuild the list.
    NetworksChanged,
    /// The attempt reached a stable outcome with no error to show — connected
    /// successfully, or aborted (cancelled via Disconnect / superseded). Reset
    /// the list to normal browsing.
    ConnectionSettled,
    ConnectionAuthFailed,
    ConnectionFailed(String),
}

#[derive(Debug)]
pub(crate) enum AvailableNetworksOutput {
    /// A genuine (non-auth) failure that left the station disconnected; the card
    /// displays it.
    ConnectionFailed { ssid: String, message: String },
}
