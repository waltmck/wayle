use gtk::prelude::*;
use relm4::{gtk, spawn_local};
use wayle_notification::core::types::Action;

use super::NotificationItem;
use crate::{
    i18n::t,
    shell::notification_popup::helpers::{
        RelativeTime, ResolvedIcon, cached_texture, resolve_icon, urgency_css_class,
    },
};

const MAX_ACTIONS_PER_ROW: usize = 3;

impl NotificationItem {
    pub(super) fn apply_icon(&self, icon: &gtk::Image, icon_container: &gtk::Box) {
        // Reset first so repeated calls (reactive refresh) don't accumulate state or
        // leave a stale image when the source changes (e.g. Named -> File).
        icon.clear();
        icon_container.remove_css_class("file-icon");

        match &self.resolved_icon {
            ResolvedIcon::Named(name) => {
                icon.set_icon_name(Some(name));
                if !name.ends_with("-symbolic") {
                    icon_container.add_css_class("file-icon");
                }
            }

            ResolvedIcon::File(path) => {
                // Share one reference-counted texture across every notification using this
                // image instead of loading a separate copy per widget; fall back to a
                // direct load if the file can't be decoded into a texture.
                match cached_texture(path) {
                    Some(texture) => icon.set_paintable(Some(&texture)),
                    None => icon.set_from_file(Some(path)),
                }
                icon_container.add_css_class("file-icon");
            }
        }
    }

    pub(super) fn build_action_buttons(&self, actions_box: &gtk::Box) {
        // Clear existing rows so this is idempotent on reactive refresh.
        while let Some(child) = actions_box.first_child() {
            actions_box.remove(&child);
        }

        let actions = self.notification.actions.get();
        let visible_actions: Vec<_> = actions
            .iter()
            .filter(|action| action.id != Action::DEFAULT_ID)
            .collect();

        if visible_actions.is_empty() {
            actions_box.set_visible(false);
            return;
        }
        actions_box.set_visible(true);

        for chunk in visible_actions.chunks(MAX_ACTIONS_PER_ROW) {
            let row = self.build_action_row(chunk);
            actions_box.append(&row);
        }
    }

    fn build_action_row(&self, actions: &[&Action]) -> gtk::Box {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        row.add_css_class("notification-dropdown-item-action-row");
        row.set_homogeneous(true);

        for action in actions {
            let button = self.build_action_button(action);
            row.append(&button);
        }

        row
    }

    fn build_action_button(&self, action: &Action) -> gtk::Button {
        let button = gtk::Button::with_label(&action.label);
        button.add_css_class("notification-dropdown-item-action-btn");
        button.set_cursor_from_name(Some("pointer"));

        let notification = self.notification.clone();
        let action_id = action.id.clone();

        button.connect_clicked(move |_| {
            let notif = notification.clone();
            let aid = action_id.clone();

            spawn_local(async move {
                if let Err(err) = notif.invoke(&aid).await {
                    tracing::warn!(action = %aid, error = %err, "action invocation failed");
                }
                notif.dismiss();
            });
        });

        button
    }

    pub(super) fn setup_default_action(&self, main_row: &gtk::Box) -> Option<gtk::GestureClick> {
        if self.notification.default_action.get().is_none() {
            main_row.set_cursor_from_name(None);
            return None;
        }

        main_row.set_cursor_from_name(Some("pointer"));

        let notification = self.notification.clone();
        let click = gtk::GestureClick::new();

        click.connect_released(move |gesture, _, _, _| {
            gesture.set_state(gtk::EventSequenceState::Claimed);

            let notif = notification.clone();
            spawn_local(async move {
                if let Err(err) = notif.invoke(Action::DEFAULT_ID).await {
                    tracing::warn!(error = %err, "default action invocation failed");
                }
                notif.dismiss();
            });
        });

        main_row.add_controller(click.clone());
        Some(click)
    }

    pub(super) fn apply_urgency(&self, root: &gtk::Box) {
        // Idempotent: drop any previously-applied urgency class before adding current.
        for class in ["low", "normal", "critical"] {
            root.remove_css_class(class);
        }
        root.add_css_class(urgency_css_class(self.notification.urgency.get()));
    }

    /// Re-renders the imperative parts of the item in place from the current
    /// notification state (icon, action buttons, urgency, default-click gesture).
    /// Declarative fields (summary/body/time) refresh via `#[watch]`.
    pub(super) fn refresh_widgets(&mut self) {
        self.resolved_icon = resolve_icon(
            self.icon_source,
            &self.notification.app_name.get(),
            &self.notification.app_icon.get(),
            &self.notification.image_path.get(),
            &self.notification.desktop_entry.get(),
            self.symbolic_fallback,
        );

        if let (Some(icon), Some(container)) = (self.icon.clone(), self.icon_container.clone()) {
            self.apply_icon(&icon, &container);
        }

        if let Some(actions_box) = self.actions_box.clone() {
            self.build_action_buttons(&actions_box);
        }

        if let Some(root) = self.root.clone() {
            self.apply_urgency(&root);
        }

        if let Some(main_row) = self.main_row.clone() {
            if let Some(gesture) = self.default_gesture.take() {
                main_row.remove_controller(&gesture);
            }
            let gesture = self.setup_default_action(&main_row);
            self.default_gesture = gesture;
        }
    }

    pub(crate) fn time_to_string(time: RelativeTime) -> String {
        match time {
            RelativeTime::JustNow => t!("notification-dropdown-time-just-now"),

            RelativeTime::Minutes(minutes) => {
                t!(
                    "notification-dropdown-time-minutes-ago",
                    minutes = minutes.to_string()
                )
            }

            RelativeTime::Hours(hours) => {
                t!(
                    "notification-dropdown-time-hours-ago",
                    hours = hours.to_string()
                )
            }
        }
    }
}
