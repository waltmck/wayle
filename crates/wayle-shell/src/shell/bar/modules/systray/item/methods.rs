use std::{cell::Cell, rc::Rc};

#[allow(deprecated)]
use gtk4::prelude::StyleContextExt;
use gtk4::gdk;
use relm4::gtk::{self, prelude::*};
use tracing::debug;
use wayle_systray::types::Coordinates;

use super::{
    SystrayItem, menu,
    helpers::{create_texture_from_pixmap, load_icon_from_theme_path, select_best_pixmap},
};
use crate::shell::{
    bar::modules::systray::helpers::find_override, helpers::COMPONENT_CSS_PRIORITY,
};

impl SystrayItem {
    /// Show this item's menu. `toggle`: if the menu is already visible, dismiss it
    /// (right-click / `systray toggle`); when `false` (`systray open`), leave an
    /// already-open menu untouched.
    pub(super) fn request_menu_show(&mut self, toggle: bool) {
        if let Some(menu) = self.menu.as_ref()
            && menu.is_visible()
        {
            if toggle {
                debug!(item_id = %self.item.id.get(), "hiding menu");
                menu.dismiss();
            }
            return;
        }

        // Show synchronously from the cached menu layout so the scrim's bar->overlay
        // re-layer happens INSIDE the click event. Doing an async `AboutToShow`
        // round-trip *before* showing re-layers the bar ~100ms later under a
        // stationary pointer, which Hyprland won't re-focus — silently dropping the
        // bar's pointer focus (the "two right-clicks lose focus" bug), and the async
        // gap also needs coalescing to avoid a swallowed close. Instead refresh in
        // the background; the menu watcher (`MenuUpdated` -> `rebuild_menu_if_visible`)
        // rebuilds the open menu in place if the layout changed.
        self.show_menu();

        let item = self.item.clone();
        tokio::spawn(async move {
            if let Err(error) = item.refresh_menu().await {
                debug!(error = %error, "AboutToShow not supported");
            }
        });
    }

    // A cohesive linear sequence (guard → build → coordinator swap → wire → popup)
    // whose steps must stay together; splitting it would fragment the swap bookkeeping.
    #[allow(clippy::cognitive_complexity)]
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

        // Rebuild from scratch each time so the menu reflects the latest layout. Build
        // the new cascade FIRST, then let the coordinator's `open` (show-before-close)
        // swap it in: the open surface stays registered across the swap, so the scrim
        // never dips to hidden and the bar is never re-layered mid-rebuild. Re-layering
        // the bar under a stationary pointer is exactly what drops its pointer focus, so
        // clearing the old registration first (which hides the scrim, then re-shows it)
        // would reintroduce that on every layout-changing `AboutToShow` refresh.
        let old_menu = self.menu.take();
        let old_reg = self.menu_reg.take();

        let menu = menu::build(&self.item, &root_menu, parent.upcast_ref());
        let dismiss = menu.dismiss_handle();
        let cleaned = Rc::new(Cell::new(false));

        // Register the new surface (its key handler drives the cascade from wherever
        // keyboard focus sits; the tray button is its anchor). `open` closes the
        // previous surface — the old menu, still the coordinator's `current` — under its
        // dismissing guard, so the scrim stays shown across the swap.
        let anchor = parent.upcast_ref::<gtk::Widget>().downgrade();
        self.coordinator
            .open(dismiss.clone(), Some(menu.key_handler()), Some(anchor));

        // `open` already closed the old surface; mark its guard so its `connect_closed`
        // is a no-op (the coordinator has moved on) and tear its now-hidden popovers down.
        if let Some((_old_dismiss, old_cleaned)) = old_reg {
            old_cleaned.set(true);
        }
        if let Some(old_menu) = old_menu {
            old_menu.teardown();
        }

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
