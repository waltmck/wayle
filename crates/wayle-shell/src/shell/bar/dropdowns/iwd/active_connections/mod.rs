mod messages;
mod methods;
mod watchers;

use std::sync::Arc;

use gtk::prelude::*;
use relm4::{gtk, prelude::*};
use wayle_iwd::{ConnectionState, IwdService};
use wayle_widgets::{WatcherToken, prelude::*};

use self::messages::{ActiveConnectionsCmd, ConnectionError, WifiState};
pub(crate) use self::messages::{ActiveConnectionsInit, ActiveConnectionsInput};
use crate::{i18n::t, shell::bar::dropdowns::iwd::helpers};

pub(crate) struct ActiveConnections {
    iwd: Arc<IwdService>,
    wifi: WifiState,
    /// Transient, shell-owned failure display (mirrors the NM dropdown). Shown
    /// only while `wifi.connection` is `Idle` — see [`Self::has_wifi_error`].
    error: Option<ConnectionError>,
    wifi_watcher: WatcherToken,
}

#[relm4::component(pub(crate))]
impl Component for ActiveConnections {
    type Init = ActiveConnectionsInit;
    type Input = ActiveConnectionsInput;
    type Output = ();
    type CommandOutput = ActiveConnectionsCmd;

    view! {
        #[root]
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            #[watch]
            set_visible: model.card_visible(),

            #[name = "section_label"]
            gtk::Label {
                add_css_class: "section-label",
                set_halign: gtk::Align::Start,
                set_label: &t!("dropdown-iwd-active-connection"),
            },

            #[template]
            Card {
                add_css_class: "network-connections-group",
                set_orientation: gtk::Orientation::Vertical,

                #[name = "wifi_card"]
                gtk::Box {
                    add_css_class: "network-connection-card",
                    #[watch]
                    set_visible: model.card_visible(),
                    #[name = "wifi_icon_container"]
                    gtk::Box {
                        add_css_class: "network-connection-icon",
                        #[watch]
                        set_css_classes: &model.wifi_icon_classes(),
                        set_hexpand: false,

                        #[name = "wifi_icon"]
                        gtk::Image {
                            #[watch]
                            set_icon_name: Some(model.effective_wifi_icon()),
                            set_halign: gtk::Align::Center,
                            set_valign: gtk::Align::Center,
                        },
                    },

                    #[name = "wifi_info"]
                    gtk::Box {
                        add_css_class: "network-connection-info",
                        set_orientation: gtk::Orientation::Vertical,
                        set_hexpand: true,

                        #[name = "wifi_name"]
                        gtk::Label {
                            add_css_class: "network-connection-name",
                            set_xalign: 0.0,
                            set_ellipsize: gtk::pango::EllipsizeMode::End,
                            set_max_width_chars: 1,
                            #[watch]
                            set_label: &model.display_wifi_name(),
                        },

                        #[name = "wifi_detail_box"]
                        gtk::Box {
                            #[watch]
                            set_visible: model.wifi_detail_visible(),
                            #[watch]
                            set_tooltip_text: model.error_tooltip(),

                            #[name = "wifi_detail"]
                            gtk::Label {
                                #[watch]
                                set_css_classes: &model.wifi_detail_classes(),
                                set_hexpand: true,
                                set_xalign: 0.0,
                                set_ellipsize: gtk::pango::EllipsizeMode::End,
                                set_max_width_chars: 1,
                                #[watch]
                                set_label: &model.wifi_detail(),
                            },
                        },
                    },

                    #[name = "wifi_hover_stack"]
                    gtk::Stack {
                        add_css_class: "network-hover-stack",
                        set_transition_type: gtk::StackTransitionType::Crossfade,
                        set_transition_duration: 150,
                        set_valign: gtk::Align::Center,
                        set_hexpand: false,
                        add_named[Some("status")] = &gtk::Box {
                            set_halign: gtk::Align::End,
                            set_valign: gtk::Align::Center,

                            gtk::Label {
                                add_css_class: "network-connection-status",
                                set_label: &t!("dropdown-iwd-connected"),
                                set_vexpand: false,
                                set_valign: gtk::Align::Center,
                                #[watch]
                                set_visible: model.is_connected(),
                            },

                            #[template]
                            SubtleBadge {
                                #[watch]
                                set_css_classes: &model.status_classes(),
                                #[watch]
                                set_label: &model.status_label(),
                                set_vexpand: false,
                                set_valign: gtk::Align::Center,
                                #[watch]
                                set_visible: model.is_connecting() || model.has_wifi_error(),
                            },
                        },

                        add_named[Some("actions")] = &gtk::Box {
                            add_css_class: "network-connection-actions",
                            set_valign: gtk::Align::Center,

                            #[template]
                            GhostButton {
                                add_css_class: "network-action-disconnect",
                                #[template_child]
                                label {
                                    set_label: &t!("dropdown-iwd-disconnect"),
                                },
                                connect_clicked => ActiveConnectionsInput::DisconnectWifi,
                            },

                            #[template]
                            GhostButton {
                                add_css_class: "network-action-forget",
                                #[template_child]
                                label {
                                    set_label: &t!("dropdown-iwd-forget"),
                                },
                                connect_clicked => ActiveConnectionsInput::ForgetWifi,
                            },
                        },

                        add_named[Some("error-actions")] = &gtk::Box {
                            add_css_class: "network-connection-actions",
                            set_halign: gtk::Align::End,
                            set_valign: gtk::Align::Center,

                            #[template]
                            GhostButton {
                                add_css_class: "network-action-dismiss",
                                #[template_child]
                                label {
                                    set_label: &t!("dropdown-iwd-dismiss"),
                                },
                                connect_clicked => ActiveConnectionsInput::DismissError,
                            },
                        },

                        add_named[Some("cancel-actions")] = &gtk::Box {
                            add_css_class: "network-connection-actions",
                            set_halign: gtk::Align::End,
                            set_valign: gtk::Align::Center,

                            #[template]
                            GhostButton {
                                add_css_class: "network-action-disconnect",
                                #[template_child]
                                label {
                                    set_label: &t!("dropdown-iwd-cancel"),
                                },
                                // Cancelling an in-progress connection is just a
                                // disconnect; the service then reconciles us out
                                // of the Connecting state.
                                connect_clicked => ActiveConnectionsInput::DisconnectWifi,
                            },

                            #[template]
                            GhostButton {
                                add_css_class: "network-action-forget",
                                #[template_child]
                                label {
                                    set_label: &t!("dropdown-iwd-forget"),
                                },
                                connect_clicked => ActiveConnectionsInput::ForgetWifi,
                            },
                        },

                        #[watch]
                        set_visible_child_name:
                            if model.wifi.hovered && model.is_connecting() {
                                "cancel-actions"
                            } else if model.wifi.hovered && model.is_connected() {
                                "actions"
                            } else if model.wifi.hovered && model.has_wifi_error() {
                                "error-actions"
                            } else {
                                "status"
                            },
                    },
                },
            },
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let wifi = init
            .iwd
            .station
            .get()
            .map(|station| WifiState::from_station(&station))
            .unwrap_or_default();

        let mut model = Self {
            iwd: init.iwd.clone(),
            wifi,
            error: None,
            wifi_watcher: WatcherToken::new(),
        };

        watchers::spawn_device_watchers(&sender, &init.iwd);

        model.reset_wifi_watchers(&sender);

        let widgets = view_output!();

        let hover = gtk::EventControllerMotion::new();

        let hover_sender = sender.input_sender().clone();
        hover.connect_enter(move |_, _, _| {
            hover_sender.emit(ActiveConnectionsInput::WifiCardHovered(true));
        });

        let leave_sender = sender.input_sender().clone();
        hover.connect_leave(move |_| {
            leave_sender.emit(ActiveConnectionsInput::WifiCardHovered(false));
        });

        widgets.wifi_card.add_controller(hover);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
        match msg {
            ActiveConnectionsInput::DisconnectWifi => self.disconnect_wifi(&sender),
            ActiveConnectionsInput::ForgetWifi => self.forget_wifi(&sender),
            ActiveConnectionsInput::DismissError => {
                self.error = None;
            }
            ActiveConnectionsInput::WifiCardHovered(hovered) => {
                self.wifi.hovered = hovered;
            }
            ActiveConnectionsInput::ShowError { ssid, message } => {
                // The list only routes genuine (non-auth) failures that left us
                // disconnected, so accept it unconditionally. `has_wifi_error`
                // gates display on `connection == Idle`, and a later connection
                // change clears it (see `WifiChanged`), so a stale error can
                // never resurface.
                self.error = Some(ConnectionError { ssid, message });
            }
        }
    }

    fn update_cmd(
        &mut self,
        msg: ActiveConnectionsCmd,
        sender: ComponentSender<Self>,
        _root: &Self::Root,
    ) {
        match msg {
            ActiveConnectionsCmd::WifiChanged {
                connection,
                strength,
                frequency,
            } => {
                // Any connection activity (connecting elsewhere, or connected)
                // supersedes a prior failure display.
                if !matches!(connection, ConnectionState::Idle) {
                    self.error = None;
                }

                self.wifi.connection = connection;
                self.wifi.strength = strength;
                self.wifi.frequency = frequency;
                self.wifi.icon = helpers::signal_strength_icon(self.wifi.strength.unwrap_or(0));
            }
            ActiveConnectionsCmd::StationDeviceChanged => {
                if self.iwd.station.get().is_none() {
                    self.wifi = WifiState::default();
                    self.error = None;
                }

                self.reset_wifi_watchers(&sender);
            }
        }
    }
}
