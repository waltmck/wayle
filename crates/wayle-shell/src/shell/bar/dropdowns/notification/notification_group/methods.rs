use std::{collections::HashSet, sync::Arc};

use wayle_config::schemas::modules::notification::IconSource;
use wayle_notification::core::notification::Notification;

use super::{super::notification_item::messages::NotificationItemInit, NotificationGroup};
use crate::shell::notification_popup::helpers::{ResolvedIcon, resolve_icon};

const MAX_VISIBLE_ITEMS: usize = 3;

impl NotificationGroup {
    pub(super) fn reconcile_items(&mut self, notifications: Vec<Arc<Notification>>) {
        self.update_metadata(&notifications);

        let cap = self.visible_cap(notifications.len());
        let visible = &notifications[..notifications.len().min(cap)];
        self.overflow_count = notifications.len().saturating_sub(cap);

        self.rebuild_items_if_changed(visible);
        self.notifications = notifications;
    }

    pub(super) fn reset_to_default_cap(&mut self) {
        if self.items.len() <= MAX_VISIBLE_ITEMS {
            return;
        }

        {
            let mut guard = self.items.guard();
            while guard.len() > MAX_VISIBLE_ITEMS {
                guard.pop_back();
            }
        }

        self.overflow_count = self.notifications.len().saturating_sub(MAX_VISIBLE_ITEMS);
    }

    pub(super) fn show_all_items(&mut self) {
        let remaining = &self.notifications[self.items.len()..];
        if remaining.is_empty() {
            self.overflow_count = 0;
            return;
        }

        let inits: Vec<_> = remaining
            .iter()
            .map(|notification| {
                build_item_init(self.icon_source, self.symbolic_fallback, notification)
            })
            .collect();

        {
            let mut guard = self.items.guard();
            for init in inits {
                guard.push_back(init);
            }
        }

        self.overflow_count = 0;
    }

    pub(super) fn resolve_group_icon(
        _icon_source: IconSource,
        symbolic_fallback: bool,
        notifications: &[Arc<Notification>],
    ) -> Option<String> {
        let first = notifications.first()?;
        let resolved = resolve_icon(
            IconSource::Mapped,
            &first.app_name.get(),
            &first.app_icon.get(),
            &first.image_path.get(),
            &first.desktop_entry.get(),
            symbolic_fallback,
        );

        match resolved {
            ResolvedIcon::Named(name) => Some(name),
            _ => None,
        }
    }

    fn update_metadata(&mut self, notifications: &[Arc<Notification>]) {
        self.total_count = notifications.len();
        self.count = notifications.len();

        self.preview = notifications
            .first()
            .map(|notification| notification.summary.get())
            .unwrap_or_default();
    }

    fn visible_cap(&self, total: usize) -> usize {
        let showing_all = self.items.len() > MAX_VISIBLE_ITEMS;
        if showing_all {
            total
        } else {
            MAX_VISIBLE_ITEMS
        }
    }

    fn rebuild_items_if_changed(&mut self, visible: &[Arc<Notification>]) {
        let old_ids: Vec<u32> = (0..self.items.len())
            .filter_map(|idx| self.items.get(idx).map(|item| item.notification.id))
            .collect();

        let new_ids: Vec<u32> = visible.iter().map(|notification| notification.id).collect();

        if old_ids == new_ids {
            return;
        }

        // Reconcile the factory in place within a single guard scope (one render): drop
        // items that are no longer visible, then move/insert so each slot matches
        // `visible`. Items whose id persists keep their existing widget and reactive
        // watchers rather than being destroyed and rebuilt.
        let new_id_set: HashSet<u32> = new_ids.iter().copied().collect();
        let icon_source = self.icon_source;
        let symbolic_fallback = self.symbolic_fallback;
        let mut guard = self.items.guard();

        for idx in (0..guard.len()).rev() {
            let still_visible = guard
                .get(idx)
                .is_some_and(|item| new_id_set.contains(&item.notification.id));
            if !still_visible {
                guard.remove(idx);
            }
        }

        for (target_idx, notification) in visible.iter().enumerate() {
            let existing = (target_idx..guard.len()).find(|&idx| {
                guard
                    .get(idx)
                    .is_some_and(|item| item.notification.id == notification.id)
            });

            match existing {
                Some(idx) if idx == target_idx => {}
                Some(idx) => {
                    guard.move_to(idx, target_idx);
                }
                None => {
                    guard.insert(
                        target_idx,
                        build_item_init(icon_source, symbolic_fallback, notification),
                    );
                }
            }
        }
    }
}

fn build_item_init(
    icon_source: IconSource,
    symbolic_fallback: bool,
    notification: &Arc<Notification>,
) -> NotificationItemInit {
    let resolved_icon = resolve_icon(
        icon_source,
        &notification.app_name.get(),
        &notification.app_icon.get(),
        &notification.image_path.get(),
        &notification.desktop_entry.get(),
        symbolic_fallback,
    );

    NotificationItemInit {
        notification: notification.clone(),
        resolved_icon,
        icon_source,
        symbolic_fallback,
    }
}
