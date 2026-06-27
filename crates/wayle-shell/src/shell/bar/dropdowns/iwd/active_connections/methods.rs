use relm4::ComponentSender;
use tracing::warn;

use super::ActiveConnections;
use crate::{i18n::t, shell::bar::dropdowns::iwd::helpers};

/// Icon shown while a connection is being established — the same "acquiring"
/// icon the bar module displays during a connection.
const CONNECTING_ICON: &str = "network-wireless-acquiring-symbolic";

impl ActiveConnections {
    pub(super) fn has_wifi_error(&self) -> bool {
        self.connection.error.is_some() && !self.wifi.connected
    }

    pub(super) fn is_wifi_connecting(&self) -> bool {
        self.connection.ssid.is_some() || self.wifi.connecting
    }

    /// Whether a connection is actively in progress and has not errored.
    pub(super) fn is_connecting(&self) -> bool {
        self.is_wifi_connecting() && self.connection.error.is_none()
    }

    pub(super) fn update_has_connections(&mut self) {
        self.has_connections = self.wifi.connected || self.wifi.connecting;
    }

    pub(super) fn reset_wifi_watchers(&mut self, sender: &ComponentSender<Self>) {
        let token = self.wifi_watcher.reset();

        super::watchers::spawn_wifi_watchers(sender, &self.iwd, token);
    }

    pub(super) fn display_wifi_name(&self) -> String {
        // A connection we initiated takes precedence: the network we are
        // connecting to is the active connection, even while IWD still reports
        // the previous network as connected during the brief transition.
        if let Some(connecting) = &self.connection.ssid {
            return connecting.clone();
        }

        if let Some(ssid) = &self.wifi.ssid {
            return ssid.clone();
        }

        t!("dropdown-iwd-wifi")
    }

    pub(super) fn status_label(&self) -> String {
        if self.connection.error.is_some() {
            return t!("dropdown-iwd-error");
        }

        if self.is_wifi_connecting() {
            return t!("dropdown-iwd-connecting");
        }

        String::new()
    }

    pub(super) fn wifi_detail_visible(&self) -> bool {
        self.connection.error.is_some() || self.wifi.frequency.is_some()
    }

    pub(super) fn wifi_detail(&self) -> String {
        if let Some(error) = &self.connection.error {
            return error.clone();
        }

        self.wifi
            .frequency
            .and_then(helpers::frequency_to_band)
            .map(str::to_string)
            .unwrap_or_default()
    }

    pub(super) fn wifi_detail_classes(&self) -> Vec<&'static str> {
        let mut classes = vec!["network-connection-detail"];

        if self.has_wifi_error() {
            classes.push("error");
        }

        classes
    }

    pub(super) fn wifi_icon_classes(&self) -> Vec<&'static str> {
        let mut classes = vec!["network-connection-icon"];

        if self.has_wifi_error() {
            classes.push("error");
        } else {
            classes.push("wifi");
        }

        classes
    }

    pub(super) fn effective_wifi_icon(&self) -> &'static str {
        if self.has_wifi_error() {
            return "network-wireless-offline-symbolic";
        }

        if self.is_connecting() {
            return CONNECTING_ICON;
        }

        self.wifi.icon
    }

    pub(super) fn disconnect_wifi(&self, sender: &ComponentSender<Self>) {
        let iwd = self.iwd.clone();
        sender.command(|_out, _shutdown| async move {
            if let Some(station) = iwd.station.get()
                && let Err(err) = station.disconnect().await
            {
                warn!(error = %err, "wifi disconnect failed");
            }
        });
    }

    pub(super) fn forget_wifi(&self, sender: &ComponentSender<Self>) {
        let iwd = self.iwd.clone();
        // While connecting, the active connection is the in-progress target;
        // otherwise it is the connected network.
        let ssid = self
            .connection
            .ssid
            .clone()
            .or_else(|| self.wifi.ssid.clone());

        sender.command(|_out, _shutdown| async move {
            let Some(ssid) = ssid else {
                return;
            };
            let Some(station) = iwd.station.get() else {
                return;
            };

            let target = station
                .networks
                .get()
                .into_iter()
                .find(|network| network.ssid.get() == ssid);

            if let Some(network) = target
                && let Err(err) = network.forget().await
            {
                warn!(error = %err, "wifi forget failed");
            }

            if let Err(err) = station.disconnect().await {
                warn!(error = %err, "wifi disconnect after forget failed");
            }
        });
    }

    pub(super) fn status_classes(&self) -> Vec<&'static str> {
        let mut classes = vec!["badge-subtle", "network-connection-status"];

        if self.connection.error.is_some() {
            classes.push("error");
        } else if self.is_wifi_connecting() {
            classes.push("warning");
        }

        classes
    }
}
