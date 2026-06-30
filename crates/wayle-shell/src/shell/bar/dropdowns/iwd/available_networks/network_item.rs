use gtk::{pango, prelude::*};
use relm4::{gtk, prelude::*};
use wayle_widgets::prelude::*;
use zbus::zvariant::OwnedObjectPath;

use super::methods;
use crate::{
    i18n::t,
    shell::bar::dropdowns::iwd::helpers::{self, NetworkSnapshot},
};

const HOVER_TRANSITION_MS: u32 = 150;

pub(super) struct NetworkItemInit {
    pub snapshot: NetworkSnapshot,
    /// Configured signal-strength icon, resolved by the parent.
    pub icon: String,
}

pub(super) struct NetworkItem {
    ssid: String,
    icon: String,
    security_label: String,
    object_path: OwnedObjectPath,

    is_secured: bool,
    known: bool,
    hovered: bool,
}

impl NetworkItem {
    /// SSID, the stable identity used to reconcile the list in place.
    pub(super) fn ssid(&self) -> &str {
        &self.ssid
    }

    /// Whether this row can be updated in place for `snapshot`, or must be
    /// recreated. `known` is the only field wired up at construction time (it
    /// gates the hover-to-forget controller in `init_widgets`), so a change there
    /// requires a fresh row; everything else updates via [`Self::refresh`].
    pub(super) fn reusable_for(&self, snapshot: &NetworkSnapshot) -> bool {
        self.known == snapshot.known
    }

    /// Update the mutable display fields in place (icon and security label),
    /// avoiding a destroy/recreate of the row widget.
    pub(super) fn refresh(&mut self, snapshot: &NetworkSnapshot, icon: String) {
        self.icon = icon;
        self.is_secured = helpers::requires_password(snapshot.security);
        self.security_label = security_label(snapshot);
        self.object_path = snapshot.object_path.clone();
    }
}

/// Security label for a network, marking saved secured networks distinctly.
fn security_label(snapshot: &NetworkSnapshot) -> String {
    let base = methods::translate_security_type(snapshot.security);
    if snapshot.known && helpers::requires_password(snapshot.security) {
        t!("dropdown-iwd-security-saved", security = base)
    } else {
        base
    }
}

#[derive(Debug)]
pub(super) enum NetworkItemInput {
    Hovered(bool),
    ForgetClicked,
}

#[derive(Debug)]
pub(super) enum NetworkItemOutput {
    Selected(DynamicIndex),
    ForgetRequested(OwnedObjectPath),
}

#[relm4::factory(pub(super))]
impl FactoryComponent for NetworkItem {
    type Init = NetworkItemInit;
    type Input = NetworkItemInput;
    type Output = NetworkItemOutput;
    type CommandOutput = ();
    type ParentWidget = gtk::Box;

    view! {
        gtk::Box {
            add_css_class: "network-item",
            set_cursor_from_name: Some("pointer"),

            #[name = "signal_icon"]
            gtk::Image {
                add_css_class: "network-item-signal",
                #[watch]
                set_icon_name: Some(self.icon.as_str()),
                set_valign: gtk::Align::Center,
            },

            #[name = "info_column"]
            gtk::Box {
                add_css_class: "network-item-info",
                set_orientation: gtk::Orientation::Vertical,
                set_hexpand: true,

                #[name = "ssid_label"]
                gtk::Label {
                    add_css_class: "network-item-name",
                    set_halign: gtk::Align::Start,
                    set_ellipsize: pango::EllipsizeMode::End,
                    #[watch]
                    set_label: &self.ssid,
                },

                #[name = "security_label"]
                gtk::Label {
                    add_css_class: "network-item-security",
                    set_halign: gtk::Align::Start,
                    #[watch]
                    set_label: &self.security_label,
                },
            },

            #[name = "trailing_stack"]
            gtk::Stack {
                add_css_class: "network-item-trailing",
                set_transition_type: gtk::StackTransitionType::Crossfade,
                set_transition_duration: HOVER_TRANSITION_MS,
                set_valign: gtk::Align::Center,
                set_hexpand: false,
                #[watch]
                set_visible: self.is_secured || self.known,

                add_named[Some("lock")] = &gtk::Box {
                    set_halign: gtk::Align::End,
                    set_valign: gtk::Align::Center,

                    #[name = "lock_icon"]
                    gtk::Image {
                        add_css_class: "network-item-lock",
                        set_icon_name: Some("ld-lock-symbolic"),
                        set_valign: gtk::Align::Center,
                        #[watch]
                        set_visible: self.is_secured,
                    },
                },

                add_named[Some("actions")] = &gtk::Box {
                    add_css_class: "network-item-actions",
                    set_valign: gtk::Align::Center,

                    #[template]
                    GhostButton {
                        add_css_class: "network-item-forget",
                        #[template_child]
                        label {
                            set_label: &t!("dropdown-iwd-forget"),
                        },
                        connect_clicked => NetworkItemInput::ForgetClicked,
                    },
                },

                #[watch]
                set_visible_child_name:
                    if self.hovered && self.known {
                        "actions"
                    } else {
                        "lock"
                    },
            },
        }
    }

    fn init_model(init: Self::Init, _index: &Self::Index, _sender: FactorySender<Self>) -> Self {
        let NetworkItemInit { snapshot, icon } = init;
        Self {
            icon,
            is_secured: helpers::requires_password(snapshot.security),
            known: snapshot.known,
            hovered: false,
            security_label: security_label(&snapshot),
            ssid: snapshot.ssid,
            object_path: snapshot.object_path,
        }
    }

    fn update(&mut self, msg: NetworkItemInput, sender: FactorySender<Self>) {
        match msg {
            NetworkItemInput::Hovered(hovered) => {
                self.hovered = hovered;
            }

            NetworkItemInput::ForgetClicked => {
                let _ = sender.output(NetworkItemOutput::ForgetRequested(self.object_path.clone()));
            }
        }
    }

    fn init_widgets(
        &mut self,
        index: &Self::Index,
        root: Self::Root,
        _returned_widget: &<Self::ParentWidget as relm4::factory::FactoryView>::ReturnedWidget,
        sender: FactorySender<Self>,
    ) -> Self::Widgets {
        let click = gtk::GestureClick::new();
        let idx = index.clone();
        let click_sender = sender.output_sender().clone();

        click.connect_released(move |gesture, _, _, _| {
            gesture.set_state(gtk::EventSequenceState::Claimed);
            click_sender.emit(NetworkItemOutput::Selected(idx.clone()));
        });

        root.add_controller(click);

        if self.known {
            let hover = gtk::EventControllerMotion::new();
            let hover_sender = sender.input_sender().clone();

            hover.connect_enter(move |_, _, _| {
                hover_sender.emit(NetworkItemInput::Hovered(true));
            });

            let leave_sender = sender.input_sender().clone();

            hover.connect_leave(move |_| {
                leave_sender.emit(NetworkItemInput::Hovered(false));
            });

            root.add_controller(hover);
        }

        let widgets = view_output!();
        widgets
    }
}
