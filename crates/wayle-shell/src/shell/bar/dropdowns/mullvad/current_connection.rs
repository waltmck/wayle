use gtk::prelude::*;
use relm4::{gtk, prelude::*};
use wayle_mullvad::{ConnectionState, TunnelStatus};
use wayle_widgets::prelude::*;

use super::helpers;
use crate::i18n::t;

/// The pinned "current connection" card at the top of the dropdown. Displays
/// the active relay + status inside its own elevated surface, and swaps the
/// status for a connect/disconnect button on hover. The flag sits in a square
/// whose background color reflects the connection status.
pub(super) struct CurrentConnection {
    status: TunnelStatus,
    hovered: bool,
}

#[derive(Debug)]
pub(super) enum CurrentConnectionInput {
    SetStatus(TunnelStatus),
    Hovered(bool),
    ActionClicked,
}

#[derive(Debug)]
pub(crate) enum CurrentConnectionOutput {
    ToggleRequested,
}

impl CurrentConnection {
    fn flag(&self) -> String {
        self.status
            .network
            .as_ref()
            .and_then(|network| network.hostname.as_deref())
            .and_then(helpers::country_code_from_hostname)
            .map_or_else(
                || helpers::FLAG_FALLBACK.to_string(),
                |code| helpers::flag_icon(&code),
            )
    }

    fn title(&self) -> String {
        match &self.status.network {
            Some(network) => match &network.city {
                Some(city) if !city.is_empty() => format!("{city}, {}", network.country),
                _ => network.country.clone(),
            },
            // No relay yet: keep the title coherent with the transitional state
            // rather than always showing "Not connected".
            None => match self.status.state {
                ConnectionState::Connecting => t!("dropdown-mullvad-connecting"),
                ConnectionState::Disconnecting => t!("dropdown-mullvad-disconnecting"),
                ConnectionState::Error => t!("dropdown-mullvad-blocked"),
                ConnectionState::Connected | ConnectionState::Disconnected => {
                    t!("dropdown-mullvad-not-connected")
                }
            },
        }
    }

    fn subtitle(&self) -> String {
        self.status
            .network
            .as_ref()
            .and_then(|network| network.hostname.clone())
            .unwrap_or_default()
    }

    fn has_subtitle(&self) -> bool {
        !self.subtitle().is_empty()
    }

    fn status_label(&self) -> String {
        match self.status.state {
            ConnectionState::Connected => t!("dropdown-mullvad-connected"),
            ConnectionState::Connecting => t!("dropdown-mullvad-connecting"),
            ConnectionState::Disconnecting => t!("dropdown-mullvad-disconnecting"),
            ConnectionState::Disconnected => t!("dropdown-mullvad-disconnected"),
            ConnectionState::Error => t!("dropdown-mullvad-blocked"),
        }
    }

    fn action_label(&self) -> String {
        match self.status.state {
            ConnectionState::Disconnected => t!("dropdown-mullvad-connect"),
            _ => t!("dropdown-mullvad-disconnect"),
        }
    }

    /// Classes for the flag square: the base class plus a status modifier that
    /// selects the background/foreground color (see the SCSS).
    fn icon_classes(&self) -> Vec<&'static str> {
        let modifier = match self.status.state {
            ConnectionState::Connected => "mullvad-connected",
            ConnectionState::Connecting | ConnectionState::Disconnecting => "mullvad-connecting",
            ConnectionState::Disconnected => "mullvad-disconnected",
            ConnectionState::Error => "mullvad-blocked",
        };
        vec!["network-connection-icon", modifier]
    }

    fn status_classes(&self) -> Vec<&'static str> {
        if matches!(self.status.state, ConnectionState::Error) {
            vec!["network-connection-status", "error"]
        } else {
            vec!["network-connection-status"]
        }
    }
}

#[relm4::component(pub(super))]
impl Component for CurrentConnection {
    type Init = ();
    type Input = CurrentConnectionInput;
    type Output = CurrentConnectionOutput;
    type CommandOutput = ();

    view! {
        #[root]
        gtk::Box {
            add_css_class: "card",
            add_css_class: "network-connections-group",
            set_orientation: gtk::Orientation::Vertical,

            #[name = "card"]
            gtk::Box {
                add_css_class: "network-connection-card",

                gtk::Box {
                    #[watch]
                    set_css_classes: &model.icon_classes(),
                    set_hexpand: false,
                    gtk::Image {
                        #[watch]
                        set_icon_name: Some(model.flag().as_str()),
                        set_halign: gtk::Align::Center,
                        set_valign: gtk::Align::Center,
                    },
                },

                gtk::Box {
                    add_css_class: "network-connection-info",
                    set_orientation: gtk::Orientation::Vertical,
                    set_hexpand: true,

                    gtk::Label {
                        add_css_class: "network-connection-name",
                        set_xalign: 0.0,
                        set_ellipsize: gtk::pango::EllipsizeMode::End,
                        set_max_width_chars: 1,
                        #[watch]
                        set_label: &model.title(),
                    },

                    gtk::Label {
                        add_css_class: "network-connection-detail",
                        set_xalign: 0.0,
                        set_ellipsize: gtk::pango::EllipsizeMode::End,
                        set_max_width_chars: 1,
                        #[watch]
                        set_visible: model.has_subtitle(),
                        #[watch]
                        set_label: &model.subtitle(),
                    },
                },

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
                            #[watch]
                            set_css_classes: &model.status_classes(),
                            #[watch]
                            set_label: &model.status_label(),
                            set_valign: gtk::Align::Center,
                        },
                    },

                    add_named[Some("actions")] = &gtk::Box {
                        add_css_class: "network-connection-actions",
                        set_halign: gtk::Align::End,
                        set_valign: gtk::Align::Center,
                        #[template]
                        GhostButton {
                            add_css_class: "network-action-toggle",
                            #[template_child]
                            label {
                                #[watch]
                                set_label: &model.action_label(),
                            },
                            connect_clicked => CurrentConnectionInput::ActionClicked,
                        },
                    },

                    #[watch]
                    set_visible_child_name: if model.hovered { "actions" } else { "status" },
                },
            },
        }
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = Self {
            status: TunnelStatus::default(),
            hovered: false,
        };
        let widgets = view_output!();

        let hover = gtk::EventControllerMotion::new();
        let enter_sender = sender.input_sender().clone();
        hover.connect_enter(move |_, _, _| {
            enter_sender.emit(CurrentConnectionInput::Hovered(true));
        });
        let leave_sender = sender.input_sender().clone();
        hover.connect_leave(move |_| {
            leave_sender.emit(CurrentConnectionInput::Hovered(false));
        });
        widgets.card.add_controller(hover);

        // Self-heal: GTK does not reliably deliver a leave when the popover is
        // popped down with the pointer inside, which would otherwise leave the
        // action button showing on next open. Reset hover state on unmap.
        let unmap_sender = sender.input_sender().clone();
        widgets.card.connect_unmap(move |_| {
            unmap_sender.emit(CurrentConnectionInput::Hovered(false));
        });

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
        match msg {
            CurrentConnectionInput::SetStatus(status) => self.status = status,
            CurrentConnectionInput::Hovered(hovered) => self.hovered = hovered,
            CurrentConnectionInput::ActionClicked => {
                let _ = sender.output(CurrentConnectionOutput::ToggleRequested);
            }
        }
    }
}
