use relm4::prelude::*;
use tracing::warn;
use wayle_iwd::SecurityType;
use zbus::zvariant::OwnedObjectPath;

use crate::{
    i18n::t,
    shell::bar::dropdowns::iwd::{
        available_networks::{
            AvailableNetworks, ListState,
            messages::{
                AvailableNetworksCmd, AvailableNetworksInput, AvailableNetworksOutput,
                SelectedNetwork,
            },
            network_item::{NetworkItemInit, NetworkItemOutput},
            watchers,
        },
        helpers,
        password_form::{PasswordFormInput, PasswordFormOutput},
    },
};

impl AvailableNetworks {
    pub(super) fn clear_selection(&mut self) {
        self.selection = None;
    }

    pub(super) fn handle_connection_failure(
        &mut self,
        message: String,
        sender: &ComponentSender<Self>,
    ) {
        self.state = ListState::Normal;
        self.clear_selection();
        self.rebuild_network_list();
        let _ = sender.output(AvailableNetworksOutput::ConnectionFailed(message));
    }

    pub(super) fn connect_to_selected(
        &mut self,
        password: Option<String>,
        sender: &ComponentSender<Self>,
    ) {
        let Some(selection) = &self.selection else {
            return;
        };

        let Some(station) = self.iwd.station.get() else {
            return;
        };

        let network_path = selection.network_path.clone();
        let ssid = selection.ssid.clone();
        let secured = selection.secured;
        self.state = ListState::Connecting;
        let _ = sender.output(AvailableNetworksOutput::Connecting(ssid));

        // Entering the connecting state changes which SSID is "active" (now the
        // connecting target) without any change to `station.networks`, so no
        // list refresh would otherwise fire. Rebuild now so the target leaves
        // the available list and the previously connected network reappears.
        self.rebuild_network_list();

        sender.command(move |out, _shutdown| async move {
            // If the user supplied a passphrase (e.g. retrying after an auth
            // failure), clear any saved credentials first. IWD reuses a known
            // network's stored passphrase and never asks our agent, so a
            // re-entered password would otherwise be silently ignored. This is a
            // no-op when the network has no saved credentials.
            if password.is_some()
                && let Some(network) = station
                    .networks
                    .get()
                    .into_iter()
                    .find(|network| network.object_path() == &network_path)
            {
                let _ = network.forget().await;
            }

            match station.connect(network_path, password).await {
                Ok(()) => {
                    let _ = out.send(AvailableNetworksCmd::ConnectionActivated);
                }
                Err(err) => {
                    warn!(error = %err, "iwd connection failed");
                    if secured {
                        let _ = out.send(AvailableNetworksCmd::ConnectionAuthFailed);
                    } else {
                        let _ = out.send(AvailableNetworksCmd::ConnectionFailed(t!(
                            "dropdown-iwd-error-generic"
                        )));
                    }
                }
            }
        });
    }

    pub(super) fn handle_wifi_availability(
        &mut self,
        available: bool,
        sender: &ComponentSender<Self>,
    ) {
        self.wifi_available = available;

        let station = self.iwd.station.get();
        self.powered = station.as_ref().is_some_and(|station| station.powered.get());

        let token = self.ap_watcher.reset();
        if let Some(station) = station {
            watchers::spawn(sender, &station, token);
        }

        if !available {
            if self.state == ListState::Connecting {
                let _ = sender.output(AvailableNetworksOutput::ClearConnecting);
            }

            self.state = ListState::Normal;
            self.clear_selection();
        }

        self.rebuild_network_list();
    }

    pub(super) fn handle_wifi_enabled(&mut self, enabled: bool, sender: &ComponentSender<Self>) {
        self.powered = enabled;

        if enabled {
            self.rebuild_network_list();
            return;
        }

        self.ap_cache.clear();
        self.network_list.guard().clear();

        if self.state == ListState::Connecting {
            let _ = sender.output(AvailableNetworksOutput::ClearConnecting);
        }

        self.state = ListState::Normal;
        self.clear_selection();
    }

    /// The SSID currently shown as the Active Connection, and therefore excluded
    /// from the available list: the network we are connecting to while a
    /// connection is in progress, otherwise the connected network. This mirrors
    /// the active-connection card, which favours the in-progress target over the
    /// network IWD still reports as connected during the transition.
    fn active_ssid(&self) -> Option<String> {
        if self.state == ListState::Connecting
            && let Some(selection) = &self.selection
        {
            return Some(selection.ssid.clone());
        }

        self.iwd
            .station
            .get()
            .and_then(|station| station.connected_ssid.get())
    }

    pub(super) fn rebuild_network_list(&mut self) {
        let active_ssid = self.active_ssid();
        let snapshots = match self.iwd.station.get() {
            Some(station) => {
                helpers::unique_networks(&station.networks.get(), active_ssid.as_deref())
            }
            None => vec![],
        };

        self.ap_cache = snapshots;

        let mut guard = self.network_list.guard();
        guard.clear();

        for snapshot in &self.ap_cache {
            guard.push_back(NetworkItemInit {
                snapshot: snapshot.clone(),
            });
        }
    }

    pub(super) fn select_network(&mut self, index: usize, sender: &ComponentSender<Self>) {
        let Some(network) = self.ap_cache.get(index) else {
            return;
        };

        let security_label = translate_security_type(network.security);
        let signal_icon = helpers::signal_strength_icon(network.strength);
        let secured = helpers::requires_password(network.security);

        self.selection = Some(SelectedNetwork {
            network_path: network.object_path.clone(),
            ssid: network.ssid.clone(),
            security_label: security_label.clone(),
            signal_icon,
            secured,
        });

        if secured && !network.known {
            self.state = ListState::PasswordEntry;

            self.password_form.emit(PasswordFormInput::Show {
                ssid: network.ssid.clone(),
                security_label,
                signal_icon,
                error_message: None,
            });
        } else {
            self.connect_to_selected(None, sender);
        }
    }

    pub(super) fn handle_password_form(
        &mut self,
        form_output: PasswordFormOutput,
        sender: &ComponentSender<Self>,
    ) {
        match form_output {
            PasswordFormOutput::Connect { password } => {
                self.connect_to_selected(Some(password), sender);
            }
            PasswordFormOutput::Cancel => {
                self.state = ListState::Normal;
                self.clear_selection();
            }
        }
    }

    pub(super) fn forget_network(
        &self,
        network_path: OwnedObjectPath,
        sender: &ComponentSender<Self>,
    ) {
        let iwd = self.iwd.clone();

        sender.oneshot_command(async move {
            if let Some(station) = iwd.station.get() {
                let target = station
                    .networks
                    .get()
                    .into_iter()
                    .find(|network| network.object_path() == &network_path);

                if let Some(network) = target
                    && let Err(err) = network.forget().await
                {
                    warn!(error = %err, "forget network failed");
                }
            }

            AvailableNetworksCmd::NetworksChanged
        });
    }
}

pub(super) fn translate_security_type(security: SecurityType) -> String {
    match security {
        SecurityType::None => t!("dropdown-iwd-security-open"),
        SecurityType::Wep => t!("dropdown-iwd-security-wep"),
        SecurityType::Psk => t!("dropdown-iwd-security-psk"),
        SecurityType::Enterprise => t!("dropdown-iwd-security-enterprise"),
    }
}

pub(super) fn forward_network_item_output(
    item_output: NetworkItemOutput,
) -> AvailableNetworksInput {
    match item_output {
        NetworkItemOutput::Selected(index) => {
            AvailableNetworksInput::NetworkSelected(index.current_index())
        }

        NetworkItemOutput::ForgetRequested(path) => AvailableNetworksInput::ForgetNetwork(path),
    }
}
