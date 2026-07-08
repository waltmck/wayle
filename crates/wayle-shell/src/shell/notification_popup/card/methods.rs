use gtk::prelude::*;
use relm4::{gtk, spawn_local};
use wayle_config::schemas::modules::notification::{PopupCloseBehavior, UrgencyBarThreshold};
use wayle_notification::core::types::Action;

use super::NotificationPopupCard;
use crate::{
    i18n::t,
    shell::notification_popup::helpers::{
        RelativeTime, ResolvedIcon, cached_texture, resolve_icon, urgency_bar_visible,
        urgency_css_class,
    },
};

impl NotificationPopupCard {
    pub(super) fn apply_css_classes(
        &self,
        root: &gtk::Box,
        shadow: bool,
        urgency_bar: UrgencyBarThreshold,
    ) {
        if shadow {
            root.add_css_class("shadow");
        }

        if urgency_bar_visible(self.notification.urgency.get(), urgency_bar) {
            root.add_css_class("urgency-bar");
        }
    }

    pub(super) fn apply_icon(&self, icon: &gtk::Image, icon_container: &gtk::Box) {
        // Reset first so reactive re-application doesn't accumulate the file-icon class
        // or leave a stale image when the source changes.
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

    pub(super) fn format_time_label(time: RelativeTime) -> String {
        match time {
            RelativeTime::JustNow => t!("notification-popup-time-just-now"),
            RelativeTime::Minutes(minutes) => {
                t!(
                    "notification-popup-time-minutes-ago",
                    minutes = minutes.to_string()
                )
            }
            RelativeTime::Hours(hours) => {
                t!(
                    "notification-popup-time-hours-ago",
                    hours = hours.to_string()
                )
            }
        }
    }

    pub(super) fn setup_action_buttons(&self, actions_box: &gtk::Box) {
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

        const MAX_PER_ROW: usize = 3;

        for chunk in visible_actions.chunks(MAX_PER_ROW) {
            let row = self.build_action_row(chunk);
            actions_box.append(&row);
        }
    }

    fn build_action_row(&self, actions: &[&Action]) -> gtk::Box {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        row.add_css_class("notification-popup-action-row");
        row.set_homogeneous(true);

        for action in actions {
            let button = self.build_action_button(action);
            row.append(&button);
        }

        row
    }

    fn build_action_button(&self, action: &Action) -> gtk::Button {
        let button = gtk::Button::with_label(&action.label);
        button.add_css_class("notification-popup-action-btn");
        button.set_cursor_from_name(Some("pointer"));

        let notification = self.notification.clone();
        let action_id = action.id.clone();
        let service = self.service.clone();
        let notif_id = self.notification.id;

        button.connect_clicked(move |_| {
            let notif = notification.clone();
            let aid = action_id.clone();
            tracing::debug!(id = notif_id, action = %aid, "action button clicked");
            service.dismiss_popup(notif_id);
            spawn_local(async move {
                if let Err(err) = notif.invoke(&aid).await {
                    tracing::warn!(action = %aid, error = %err, "action invocation failed");
                }
                notif.dismiss();
            });
        });

        button
    }

    pub(super) fn setup_default_action(&self, root: &gtk::Box) -> Option<gtk::GestureClick> {
        let default_action = self.notification.default_action.get();
        if default_action.is_none() {
            root.set_cursor_from_name(None);
            return None;
        }

        root.set_cursor_from_name(Some("pointer"));

        let notification = self.notification.clone();
        let service = self.service.clone();
        let notif_id = self.notification.id;
        let close_behavior = self.close_behavior;

        let click = gtk::GestureClick::new();
        click.connect_released(move |gesture, _, _, _| {
            gesture.set_state(gtk::EventSequenceState::Claimed);
            let notif = notification.clone();
            match close_behavior {
                PopupCloseBehavior::Dismiss => service.dismiss_popup(notif_id),
                PopupCloseBehavior::Remove => notif.dismiss(),
            }
            spawn_local(async move {
                if let Err(err) = notif.invoke(Action::DEFAULT_ID).await {
                    tracing::warn!(error = %err, "default action invocation failed");
                }
            });
        });
        root.add_controller(click.clone());
        Some(click)
    }

    pub(super) fn apply_urgency_class(&self, root: &gtk::Box) {
        // Idempotent: drop any previously-applied urgency class before adding current.
        for class in ["low", "normal", "critical"] {
            root.remove_css_class(class);
        }
        root.add_css_class(urgency_css_class(self.notification.urgency.get()));
    }

    /// Re-renders the card in place from the current notification state (icon, app
    /// label, action buttons, urgency, default-click gesture). Summary/body labels
    /// refresh declaratively via `#[watch]`.
    pub(super) fn refresh_notification(&mut self, root: &gtk::Box) {
        self.resolved_icon = resolve_icon(
            self.icon_source,
            &self.notification.app_name.get(),
            &self.notification.app_icon.get(),
            &self.notification.image_path.get(),
            &self.notification.desktop_entry.get(),
        );
        self.app_label = self
            .notification
            .app_name
            .get()
            .unwrap_or_else(|| t!("notification-popup-unknown-app"));

        if let (Some(icon), Some(container)) = (self.icon.clone(), self.icon_container.clone()) {
            self.apply_icon(&icon, &container);
        }
        if let Some(actions_box) = self.actions_box.clone() {
            self.setup_action_buttons(&actions_box);
        }

        self.apply_urgency_class(root);
        if urgency_bar_visible(self.notification.urgency.get(), self.urgency_bar) {
            root.add_css_class("urgency-bar");
        } else {
            root.remove_css_class("urgency-bar");
        }

        if let Some(gesture) = self.default_gesture.take() {
            root.remove_controller(&gesture);
        }
        root.set_cursor_from_name(None);
        let gesture = self.setup_default_action(root);
        self.default_gesture = gesture;
    }

    pub(super) fn setup_hover_controller(&self, root: &gtk::Box) {
        if !self.hover_pause {
            return;
        }

        let hover = gtk::EventControllerMotion::new();
        let service_enter = self.service.clone();
        let notif_id = self.notification.id;
        let service_leave = self.service.clone();

        hover.connect_enter(move |_, _, _| {
            service_enter.inhibit_popup(notif_id);
        });
        hover.connect_leave(move |_| {
            service_leave.release_popup(notif_id);
        });
        root.add_controller(hover);
    }
}
