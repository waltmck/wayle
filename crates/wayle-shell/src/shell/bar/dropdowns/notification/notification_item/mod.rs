pub(crate) mod messages;
mod methods;
mod watchers;

use std::sync::Arc;

use gtk::prelude::*;
use relm4::{gtk, prelude::*};
use tokio_util::sync::CancellationToken;
use wayle_config::schemas::modules::notification::IconSource;
use wayle_notification::core::notification::Notification;

use self::messages::{NotificationItemInit, NotificationItemInput, NotificationItemOutput};
use crate::shell::notification_popup::helpers::{ResolvedIcon, relative_time, sanitize_markup};

pub(crate) struct NotificationItem {
    pub(crate) notification: Arc<Notification>,

    resolved_icon: ResolvedIcon,
    icon_source: IconSource,
    symbolic_fallback: bool,
    time_label: String,

    root: Option<gtk::Box>,
    main_row: Option<gtk::Box>,
    actions_box: Option<gtk::Box>,
    icon: Option<gtk::Image>,
    icon_container: Option<gtk::Box>,
    default_gesture: Option<gtk::GestureClick>,
    cancel_token: CancellationToken,
}

#[relm4::factory(pub(crate))]
impl FactoryComponent for NotificationItem {
    type Init = NotificationItemInit;
    type Input = NotificationItemInput;
    type Output = NotificationItemOutput;
    type CommandOutput = ();
    type ParentWidget = gtk::Box;

    view! {
        #[root]
        gtk::Box {
            add_css_class: "notification-dropdown-item",
            set_orientation: gtk::Orientation::Vertical,

            #[name = "main_row"]
            gtk::Box {
                add_css_class: "notification-dropdown-item-main",

                #[name = "icon_container"]
                gtk::Box {
                    add_css_class: "notification-dropdown-item-icon",
                    set_valign: gtk::Align::Start,

                    #[name = "icon"]
                    gtk::Image {
                        add_css_class: "notification-dropdown-item-icon-img",
                        set_halign: gtk::Align::Center,
                        set_valign: gtk::Align::Center,
                    },
                },

                gtk::Box {
                    add_css_class: "notification-dropdown-item-content",
                    set_orientation: gtk::Orientation::Vertical,
                    set_hexpand: true,

                    gtk::Box {
                        add_css_class: "notification-dropdown-item-header",

                        gtk::Label {
                            add_css_class: "notification-dropdown-item-title",
                            set_hexpand: true,
                            set_halign: gtk::Align::Start,
                            set_ellipsize: gtk::pango::EllipsizeMode::End,
                            #[watch]
                            set_label: &self.notification.summary.get(),
                        },

                        gtk::Label {
                            add_css_class: "notification-dropdown-item-time",
                            #[watch]
                            set_label: &self.time_label,
                        },

                        #[name = "dismiss_btn"]
                        gtk::Button {
                            set_css_classes: &["ghost-icon", "notification-dropdown-item-dismiss"],
                            set_icon_name: "ld-x-symbolic",
                            set_cursor_from_name: Some("pointer"),
                        },
                    },

                    gtk::Label {
                        add_css_class: "notification-dropdown-item-body",
                        set_halign: gtk::Align::Start,
                        set_use_markup: true,
                        set_ellipsize: gtk::pango::EllipsizeMode::End,
                        set_lines: 2,
                        set_wrap: true,
                        set_wrap_mode: gtk::pango::WrapMode::WordChar,
                        #[watch]
                        set_label: &self
                            .notification
                            .body
                            .get()
                            .as_deref()
                            .map_or_else(String::new, sanitize_markup),
                        #[watch]
                        set_visible: self.notification.body.get().is_some(),
                    },
                },
            },

            #[name = "actions_box"]
            gtk::Box {
                add_css_class: "notification-dropdown-item-actions",
                set_orientation: gtk::Orientation::Vertical,
            },
        }
    }

    fn init_model(init: Self::Init, _index: &Self::Index, _sender: FactorySender<Self>) -> Self {
        let time_label = Self::time_to_string(relative_time(&init.notification.timestamp.get()));

        Self {
            notification: init.notification,
            resolved_icon: init.resolved_icon,
            icon_source: init.icon_source,
            symbolic_fallback: init.symbolic_fallback,
            time_label,
            root: None,
            main_row: None,
            actions_box: None,
            icon: None,
            icon_container: None,
            default_gesture: None,
            cancel_token: CancellationToken::new(),
        }
    }

    fn init_widgets(
        &mut self,
        _index: &Self::Index,
        root: Self::Root,
        _returned_widget: &<Self::ParentWidget as relm4::factory::FactoryView>::ReturnedWidget,
        sender: FactorySender<Self>,
    ) -> Self::Widgets {
        let widgets = view_output!();

        self.apply_icon(&widgets.icon, &widgets.icon_container);
        self.build_action_buttons(&widgets.actions_box);
        self.apply_urgency(&root);

        let id = self.notification.id;
        let output_sender = sender.output_sender().clone();

        widgets.dismiss_btn.connect_clicked(move |_| {
            output_sender.emit(NotificationItemOutput::Dismissed(id));
        });

        let gesture = self.setup_default_action(&widgets.main_row);
        self.default_gesture = gesture;

        self.root = Some(root.clone());
        self.main_row = Some(widgets.main_row.clone());
        self.actions_box = Some(widgets.actions_box.clone());
        self.icon = Some(widgets.icon.clone());
        self.icon_container = Some(widgets.icon_container.clone());

        watchers::spawn_field_watcher(&sender, &self.notification, self.cancel_token.clone());

        widgets
    }

    fn update(&mut self, msg: Self::Input, _sender: FactorySender<Self>) {
        match msg {
            NotificationItemInput::RefreshTime => {
                self.time_label =
                    Self::time_to_string(relative_time(&self.notification.timestamp.get()));
            }
            NotificationItemInput::Refresh => {
                self.refresh_widgets();
            }
        }
    }
}

impl Drop for NotificationItem {
    fn drop(&mut self) {
        self.cancel_token.cancel();
    }
}
