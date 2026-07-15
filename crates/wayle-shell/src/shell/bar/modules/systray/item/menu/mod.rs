//! Custom, scrollable, keyboard-navigable system-tray dropdown menu.
//!
//! `GtkPopoverMenu` cannot scroll, so a submenu taller than the screen (a VPN
//! applet's country list) overflows and clips. This renders the menu directly
//! from the `wayle_systray` [`MenuItem`] tree as a cascade of **independent,
//! non-grab [`gtk::Popover`]s** — one per column. Each column is its own surface,
//! so it positions and flips independently (the parent stays put when a child
//! opens; a child that would overflow the right edge flips to the left), scrolls
//! on its own, and is naturally its own floating card. No autohide (its Wayland
//! grab freezes and swallows clicks on Hyprland); outside-click/Escape dismissal
//! comes from the shared scrim (see `dropdowns::scrim`).
//!
//! Keyboard: the column popovers never get GTK focus (their buttons are
//! non-focusable), so per-popover key controllers wouldn't fire — keyboard focus
//! sits on whichever surface the pointer is over (the bar or the scrim). Instead
//! the menu registers [`MenuInner::handle_key`] with the open-surface coordinator;
//! the bar window and the scrim forward nav keys to it. It drives the deepest-open
//! ("active") column — arrow nav, Left/Right to leave/enter submenus, Enter/Space
//! to activate, Escape to dismiss. Selection is a `.selected` CSS class, not focus.
//!
//! Submodules: [`column`] (one column: popover + rows + cursor + open child) and
//! [`construct`] (build + wire).

mod column;
mod construct;

use std::{cell::Cell, rc::Rc, sync::Arc};

use chrono::Utc;
use relm4::gtk::{self, prelude::*};
use tracing::error;
use wayle_systray::{
    core::item::TrayItem,
    types::menu::{MenuEvent, MenuItem},
};

use column::{MenuColumn, RowActivation};

/// A built tray menu: the root column plus the shared controller state.
pub(super) struct TrayMenu {
    inner: Rc<MenuInner>,
    root: Rc<MenuColumn>,
}

/// Shared controller state, referenced (weakly) from every row handler.
struct MenuInner {
    item: Arc<TrayItem>,
    /// The root popover, for dismissing the whole cascade.
    root_popover: gtk::Popover,
    /// Suppresses the synthetic hover-enter GTK fires when a submenu pops up
    /// under a stationary pointer, so it can't fight a structural change.
    navigating: Cell<bool>,
    /// The height caps, so reconcile can build submenu columns that appear at
    /// runtime with the same sizing the initial build used.
    ctx: construct::BuildCtx,
}

impl TrayMenu {
    pub(super) fn is_visible(&self) -> bool {
        self.root.popover.is_visible()
    }

    pub(super) fn popup(&self) {
        self.inner.reset(&self.root);
        self.root.popover.popup();
    }

    /// Patch the whole cascade in place from a fresh layout, reusing the popovers,
    /// buttons, and submenu columns — no teardown, no surface churn. Safe whether
    /// the menu is hidden or visible (a visible reconcile is the point: an
    /// AboutToShow/LayoutUpdated change updates the open menu without a flicker).
    pub(super) fn reconcile(&self, new_root: &MenuItem) {
        construct::reconcile_column(&self.inner, &self.root, &new_root.children);
    }

    /// Unparent every popover, deepest-first.
    pub(super) fn teardown(&self) {
        construct::teardown_column(&self.root);
    }

    /// The root popover, for the caller to hook `connect_closed` on.
    pub(super) fn root_popover(&self) -> &gtk::Popover {
        &self.root.popover
    }

    /// A closure that dismisses the whole menu (for the open-surface coordinator).
    pub(super) fn dismiss_handle(&self) -> Rc<dyn Fn()> {
        let popover = self.root.popover.downgrade();
        Rc::new(move || {
            if let Some(popover) = popover.upgrade() {
                popover.popdown();
            }
        })
    }

    /// A key handler for the coordinator: nav keys forwarded from the bar/scrim
    /// (whichever holds keyboard focus) drive the cascade. Returns `true` when the
    /// key was consumed.
    pub(super) fn key_handler(&self) -> Rc<dyn Fn(gtk::gdk::Key) -> bool> {
        let inner = self.inner.clone();
        let root = self.root.clone();
        Rc::new(move |key| inner.handle_key(&root, key))
    }
}

/// Build a menu for `item` rooted at `root_item`, parenting the root popover to
/// `parent` (the tray button).
pub(super) fn build(item: &Arc<TrayItem>, root_item: &MenuItem, parent: &gtk::Widget) -> TrayMenu {
    let ctx = construct::build_ctx(parent);
    let root = construct::build_root(&ctx, root_item, parent);

    let inner = Rc::new(MenuInner {
        item: item.clone(),
        root_popover: root.popover.clone(),
        navigating: Cell::new(false),
        ctx,
    });

    construct::wire_columns(&inner, &root);

    TrayMenu { inner, root }
}

impl MenuInner {
    /// Hover moved onto row `index` of `column`: select it, and open/close its
    /// submenu so exactly the hovered row's child (if any) is shown beneath it.
    fn hover_row(&self, column: &Rc<MenuColumn>, index: usize) {
        if self.navigating.get() {
            return;
        }
        // Guard the structural change: popping a submenu up/down can put a row
        // under a stationary pointer and synthesize a crossing that re-enters.
        self.navigating.set(true);
        column.set_selected(Some(index));
        match column.submenu_at(index) {
            Some(child) => column.open_child(&child),
            None => column.close_open_child(),
        }
        self.navigating.set(false);
    }

    fn activate_leaf(&self, id: i32) {
        let item = self.item.clone();
        tokio::spawn(async move {
            let timestamp = Utc::now().timestamp().max(0) as u32;
            if let Err(error) = item.menu_event(id, MenuEvent::Clicked, timestamp).await {
                error!(error = %error, "cannot send menu event");
            }
        });
        self.root_popover.popdown();
    }

    /// Collapse to the root with nothing selected, for a fresh popup.
    fn reset(&self, root: &Rc<MenuColumn>) {
        root.close_open_child();
        root.set_selected(None);
    }

    /// The deepest currently-open column — the one keyboard nav targets, since
    /// keyboard drives the cascade independent of pointer position.
    fn active_column(&self, root: &Rc<MenuColumn>) -> Rc<MenuColumn> {
        let mut column = root.clone();
        loop {
            let child = column.open_child.borrow().clone();
            match child {
                Some(child) => column = child,
                None => return column,
            }
        }
    }

    /// Dispatch a forwarded nav key to the active (deepest-open) column. Returns
    /// `true` if consumed. Escape is normally intercepted by the bar/scrim
    /// (`dismiss_current`) before it reaches here, but handling it too is harmless.
    fn handle_key(&self, root: &Rc<MenuColumn>, key: gtk::gdk::Key) -> bool {
        use gtk::gdk::Key;

        let column = self.active_column(root);
        match key {
            Key::Down => self.nav(&column, MenuColumn::next_from),
            Key::Up => self.nav(&column, MenuColumn::prev_from),
            Key::Right => self.enter(&column),
            Key::Left => self.leave(&column),
            Key::Return | Key::KP_Enter | Key::space => self.activate(&column),
            Key::Escape => self.root_popover.popdown(),
            _ => return false,
        }
        true
    }

    /// Move the selection within `column` using `step` (next/prev), scrolling the
    /// newly-selected row into view.
    fn nav(
        &self,
        column: &Rc<MenuColumn>,
        step: fn(&MenuColumn, Option<usize>) -> Option<usize>,
    ) {
        let next = step(column, column.cursor.get());
        column.set_selected(next);
        if let Some(index) = next {
            column.scroll_into_view(index);
        }
    }

    /// Right / activate-on-submenu: open the selected submenu and descend into it,
    /// selecting its first row.
    fn enter(&self, column: &Rc<MenuColumn>) {
        let Some(index) = column.cursor.get() else {
            return;
        };
        let Some(child) = column.submenu_at(index) else {
            return;
        };
        // Guard the structural change so the synthetic hover-crossing GTK fires
        // when a popover appears under a stationary pointer can't fight it.
        self.navigating.set(true);
        column.open_child(&child);
        child.set_selected(child.first_selectable());
        self.navigating.set(false);
    }

    /// Left: close this column, returning to its parent (no-op at the root).
    fn leave(&self, column: &Rc<MenuColumn>) {
        if column.depth == 0 {
            return;
        }
        self.navigating.set(true);
        // Popping down fires the popover's `connect_closed`, which clears the
        // parent's `open_child` so the parent becomes the active column again.
        column.popover.popdown();
        self.navigating.set(false);
    }

    /// Enter/Space: activate a leaf, or open a submenu row.
    fn activate(&self, column: &Rc<MenuColumn>) {
        let Some(index) = column.cursor.get() else {
            return;
        };
        match column.activation_at(index) {
            Some(RowActivation::Leaf(id)) => self.activate_leaf(id),
            Some(RowActivation::Submenu) => self.enter(column),
            None => {}
        }
    }
}
