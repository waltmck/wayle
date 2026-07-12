use std::{cell::Cell, rc::Rc};

#[allow(deprecated)]
use gtk4::prelude::StyleContextExt;
use gtk4::gdk;
use relm4::{
    gtk::{self, prelude::*},
    prelude::*,
};
use tracing::debug;
use wayle_systray::types::Coordinates;

use super::{
    SystrayItem, SystrayItemMsg, menu,
    helpers::{create_texture_from_pixmap, load_icon_from_theme_path, select_best_pixmap},
};
use crate::shell::{
    bar::modules::systray::helpers::find_override, helpers::COMPONENT_CSS_PRIORITY,
};

impl SystrayItem {
    pub(super) fn request_menu_show(&self, sender: &FactorySender<Self>) {
        if let Some(menu) = self.menu.as_ref()
            && menu.is_visible()
        {
            debug!(item_id = %self.item.id.get(), "hiding menu");
            menu.dismiss();
            return;
        }

        // Coalesce rapid re-clicks: while the async refresh below is in flight a
        // second click would spawn a second `ShowMenu` that flaps the menu closed.
        // Cleared when `ShowMenu` is handled (see `toggle_menu`).
        if self.pending_show.replace(true) {
            return;
        }

        let item = self.item.clone();
        let sender = sender.clone();

        tokio::spawn(async move {
            if let Err(error) = item.refresh_menu().await {
                debug!(error = %error, "AboutToShow not supported");
            }

            sender.input(SystrayItemMsg::ShowMenu);
        });
    }

    pub(super) fn toggle_menu(&mut self) {
        // The pending `ShowMenu` (if any) is now being handled.
        self.pending_show.set(false);

        if let Some(menu) = self.menu.as_ref()
            && menu.is_visible()
        {
            debug!(item_id = %self.item.id.get(), "hiding menu");
            menu.dismiss();
            return;
        }

        self.show_menu();
    }

    fn show_menu(&mut self) {
        let item_id = self.item.id.get();
        debug!(item_id = %item_id, title = %self.item.title.get(), "show_menu called");

        let menu_data = self.item.menu.get();
        let Some(root_menu) = menu_data else {
            debug!("no menu data, falling back");
            self.spawn_context_menu_fallback();
            return;
        };

        if root_menu.children.is_empty() {
            debug!("empty menu, falling back");
            self.spawn_context_menu_fallback();
            return;
        }

        let Some(parent) = self.button.clone() else {
            debug!("no parent button, cannot show menu");
            return;
        };

        // Rebuild from scratch each time so the menu reflects the latest layout;
        // tear down the previous cascade first so no popover is left parented.
        // Teardown unparents without popdown, so release its coordinator/scrim
        // registration explicitly.
        self.clear_menu_registration();
        if let Some(previous) = self.menu.take() {
            previous.teardown();
        }

        let menu = menu::build(&self.item, &root_menu, parent.upcast_ref());
        let dismiss = menu.dismiss_handle();
        let cleaned = Rc::new(Cell::new(false));

        // `open` shows the scrim. Register the menu's key handler so nav keys
        // forwarded from the bar/scrim (wherever keyboard focus is) drive the
        // cascade, and the tray button as anchor so clicking it toggles rather than
        // plain-dismisses.
        let anchor = parent.upcast_ref::<gtk::Widget>().downgrade();
        self.coordinator
            .open(dismiss.clone(), Some(menu.key_handler()), Some(anchor));

        menu.root_popover().connect_closed({
            let coordinator = self.coordinator.clone();
            let dismiss = dismiss.clone();
            let cleaned = cleaned.clone();
            move |_| {
                if !cleaned.replace(true) {
                    coordinator.notify_closed(&dismiss);
                }
            }
        });

        self.menu_reg = Some((dismiss, cleaned));
        menu.popup();
        self.menu = Some(menu);
    }

    /// Release the open menu's coordinator registration (which hides the scrim) if
    /// the popover's `connect_closed` hasn't already done so. Needed because
    /// `teardown` unparents without popping down, so `connect_closed` may not fire.
    pub(super) fn clear_menu_registration(&mut self) {
        if let Some((dismiss, cleaned)) = self.menu_reg.take()
            && !cleaned.replace(true)
        {
            self.coordinator.notify_closed(&dismiss);
        }
    }

    fn spawn_context_menu_fallback(&self) {
        let item = self.item.clone();
        tokio::spawn(async move {
            let _ = item.context_menu(Coordinates::new(0, 0)).await;
        });
    }

    pub(super) fn update_icon(&mut self, image: &gtk::Image) {
        let overrides = self.config.config().modules.systray.overrides.get();
        let override_match = find_override(&self.item, &overrides);

        let icon_name = override_match
            .and_then(|entry| entry.icon.clone())
            .or_else(|| self.item.icon_name.get());

        self.apply_icon(image, icon_name.as_deref());

        if let Some(color) = override_match.and_then(|entry| entry.color.clone()) {
            self.apply_icon_color(image, &color.to_css());
        } else {
            self.clear_icon_color(image);
        }
    }

    fn apply_icon_color(&mut self, image: &gtk::Image, css_color: &str) {
        let provider = self
            .icon_color_provider
            .get_or_insert_with(gtk::CssProvider::new);

        let css = format!("image {{ color: {css_color}; }}");
        provider.load_from_string(&css);

        #[allow(deprecated)]
        image
            .style_context()
            .add_provider(provider, COMPONENT_CSS_PRIORITY);
    }

    fn clear_icon_color(&mut self, image: &gtk::Image) {
        if let Some(provider) = self.icon_color_provider.take() {
            #[allow(deprecated)]
            image.style_context().remove_provider(&provider);
        }
    }

    fn apply_icon(&self, image: &gtk::Image, icon_name: Option<&str>) {
        if let Some(name) = icon_name {
            let theme_path = self.item.icon_theme_path.get();
            if let Some(texture) = theme_path
                .as_deref()
                .and_then(|path| load_icon_from_theme_path(path, name))
            {
                image.set_paintable(Some(&texture));
                return;
            }

            if let Ok(texture) = gdk::Texture::from_filename(name) {
                image.set_paintable(Some(&texture));
                return;
            }

            image.set_icon_name(Some(name));
            return;
        }

        let pixmaps = self.item.icon_pixmap.get();
        if let Some(texture) = select_best_pixmap(&pixmaps).and_then(create_texture_from_pixmap) {
            image.set_paintable(Some(&texture));
            return;
        }

        image.set_icon_name(Some("application-x-executable-symbolic"));
    }

    pub(super) fn rebuild_menu_if_visible(&mut self) {
        let Some(menu) = self.menu.as_ref() else {
            return;
        };

        if !menu.is_visible() {
            return;
        }

        // Rebuild in place from the updated layout.
        self.show_menu();
    }
}
