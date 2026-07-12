//! Builds the menu columns from the `wayle_systray` [`MenuItem`] tree and wires
//! each column's popover (positioning, submenu alignment) and its rows
//! (hover/click).
//!
//! Every column is its own non-grab popover, built eagerly and parented to the
//! row that owns it (the root to the tray button); it's popped up/down as the
//! cascade opens and closes. There are no per-column key controllers: keyboard
//! comes from the scrim, which holds keyboard while the menu is open and forwards
//! keys to the deepest-open column (see `menu`'s `key_handler`/`handle_key`).

use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

use gtk4::{gdk, glib};
use relm4::gtk::{self, prelude::*};
use wayle_systray::types::menu::{Disposition, MenuItem, MenuItemType, ToggleState, ToggleType};

use super::MenuInner;
use super::column::{MenuColumn, MenuRow, RowKind};

/// Fraction of the monitor height a column may occupy before it scrolls.
const MAX_HEIGHT_FRACTION: f64 = 0.85;
/// Used when the monitor geometry can't be read (surface not yet realized).
const FALLBACK_MONITOR_HEIGHT: i32 = 1080;

struct BuildCtx {
    max_height: i32,
}

/// Build the whole column tree, returning the root column. Its popover is
/// parented to `tray_button`; submenu popovers hang off their owning row.
pub(super) fn build_root(root_item: &MenuItem, tray_button: &gtk::Widget) -> Rc<MenuColumn> {
    let ctx = BuildCtx {
        max_height: monitor_max_height(tray_button),
    };
    build_column(
        &ctx,
        &root_item.children,
        0,
        Weak::new(),
        tray_button,
        gtk::PositionType::Bottom,
    )
}

fn build_column(
    ctx: &BuildCtx,
    children: &[MenuItem],
    depth: usize,
    parent: Weak<MenuColumn>,
    parent_widget: &gtk::Widget,
    position: gtk::PositionType,
) -> Rc<MenuColumn> {
    let popover = gtk::Popover::new();
    popover.set_has_arrow(false);
    // No grab. Nested *autohide* popovers deadlock the wlroots/Hyprland popup
    // grab chain (moving child→parent leaves a stuck grab that freezes all
    // input); *non-autohide* children under an autohide root get no pointer
    // events at all. With no grab anywhere, every popover surface receives its
    // own pointer/scroll/keyboard events and nothing can wedge. Trade-off: no
    // auto-dismiss on outside click — the menu closes on Escape, item
    // activation, or a tray re-click.
    popover.set_autohide(false);
    // Popping a popover down still closes its descendants (deepest-first).
    popover.set_cascade_popdown(true);
    popover.set_position(position);
    popover.add_css_class("systray-menu");
    popover.set_parent(parent_widget);

    let scrolled = gtk::ScrolledWindow::new();
    scrolled.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scrolled.set_propagate_natural_height(true);
    scrolled.set_propagate_natural_width(true);
    scrolled.set_max_content_height(ctx.max_height);
    scrolled.set_focusable(false);
    scrolled.add_css_class("systray-menu-column");

    let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
    scrolled.set_child(Some(&list));
    popover.set_child(Some(&scrolled));

    Rc::new_cyclic(|weak_self| {
        let rows = populate(ctx, &list, children, depth, weak_self);
        MenuColumn {
            popover,
            rows,
            cursor: Cell::new(None),
            depth,
            open_child: RefCell::new(None),
            parent,
        }
    })
}

fn populate(
    ctx: &BuildCtx,
    list: &gtk::Box,
    children: &[MenuItem],
    depth: usize,
    parent: &Weak<MenuColumn>,
) -> Vec<MenuRow> {
    let mut rows = Vec::new();
    let mut has_items = false;
    let mut pending_separator = false;

    for child in children {
        if !child.visible {
            continue;
        }

        if child.item_type == MenuItemType::Separator {
            // Defer separators so leading, trailing, and consecutive ones never
            // render.
            pending_separator = has_items;
            continue;
        }

        if pending_separator {
            list.append(&separator());
            pending_separator = false;
        }

        // A submenu row with no visible children degrades to a plain leaf.
        let has_submenu = child.has_children() && child.children.iter().any(|item| item.visible);
        let button = build_button(child, has_submenu);
        list.append(&button);

        let kind = if has_submenu {
            let child_column = build_column(
                ctx,
                &child.children,
                depth + 1,
                parent.clone(),
                button.upcast_ref(),
                gtk::PositionType::Right,
            );
            RowKind::Submenu {
                column: child_column,
            }
        } else {
            RowKind::Leaf { id: child.id }
        };

        rows.push(MenuRow {
            button,
            kind,
        });
        has_items = true;
    }

    rows
}

fn build_button(item: &MenuItem, is_submenu: bool) -> gtk::Button {
    let button = gtk::Button::new();
    button.add_css_class("systray-menu-item-button");
    button.set_sensitive(item.enabled);
    // Selection is CSS-driven (`.selected`), not GTK focus; buttons stay
    // unfocusable so keyboard focus never leaves the bar/scrim, from which nav keys
    // are forwarded to the active column via the coordinator (see the `menu`
    // module's `key_handler`/`handle_key`).
    button.set_focusable(false);

    let content = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    content.add_css_class("systray-menu-item");
    apply_disposition_class(&content, item.disposition);

    if item.is_checkable() {
        content.append(&toggle_indicator(item));
        if item.toggle_state == ToggleState::Checked {
            content.add_css_class("active");
        }
    } else if let Some(icon) = build_icon(item) {
        content.append(&icon);
    }

    let label = gtk::Label::new(None);
    label.set_use_underline(true);
    label.set_label(item.label.as_deref().unwrap_or_default());
    label.set_halign(gtk::Align::Start);
    label.set_xalign(0.0);
    label.set_hexpand(true);
    label.add_css_class("systray-menu-item-label");
    content.append(&label);

    if is_submenu {
        content.add_css_class("has-submenu");
        let arrow = gtk::Image::from_icon_name("go-next-symbolic");
        arrow.add_css_class("systray-submenu-arrow");
        content.append(&arrow);
    } else if let Some(shortcut) = item.shortcut.as_deref().and_then(format_shortcut) {
        let accel = gtk::Label::new(Some(&shortcut));
        accel.add_css_class("systray-menu-item-shortcut");
        content.append(&accel);
    }

    button.set_child(Some(&content));
    button
}

/// Wire every column's popover (key controller, map/closed lifecycle) and its
/// rows (hover/click). Recurses into submenu columns.
pub(super) fn wire_columns(inner: &Rc<MenuInner>, column: &Rc<MenuColumn>) {
    wire_popover(column);
    wire_rows(inner, column);

    for row in &column.rows {
        if let RowKind::Submenu { column: child } = &row.kind {
            wire_columns(inner, child);
        }
    }
}

fn wire_popover(column: &Rc<MenuColumn>) {
    // No per-popover key controller: keyboard focus sits on the bar/scrim, not the
    // (non-focusable) menu columns, so nav keys are forwarded from there via the
    // coordinator (see `menu`'s `key_handler`/`handle_key`). Submenus only nudge
    // themselves on map so their first row lines up with the row that opened them.
    if column.depth == 0 {
        return;
    }

    column.popover.connect_map(align_submenu_top);

    // When a submenu closes for any reason (Left/Escape, hovering away, or a
    // cascade dismiss), drop the parent's pointer to it so the state stays in
    // sync.
    column.popover.connect_closed({
        let parent = column.parent.clone();
        let this = Rc::downgrade(column);
        move |_| {
            let Some(parent) = parent.upgrade() else {
                return;
            };
            if let Some(this) = this.upgrade() {
                let is_ours = parent
                    .open_child
                    .borrow()
                    .as_ref()
                    .is_some_and(|open| Rc::ptr_eq(open, &this));
                if is_ours {
                    parent.open_child.borrow_mut().take();
                }
            }
        }
    });
}

fn wire_rows(inner: &Rc<MenuInner>, column: &Rc<MenuColumn>) {
    for (index, row) in column.rows.iter().enumerate() {
        let motion = gtk::EventControllerMotion::new();
        motion.connect_enter({
            let inner = Rc::downgrade(inner);
            let column = Rc::downgrade(column);
            move |_, _, _| {
                if let (Some(inner), Some(column)) = (inner.upgrade(), column.upgrade()) {
                    inner.hover_row(&column, index);
                }
            }
        });
        row.button.add_controller(motion);

        match &row.kind {
            RowKind::Leaf { id } => {
                let id = *id;
                let inner = Rc::downgrade(inner);
                row.button.connect_clicked(move |_| {
                    if let Some(inner) = inner.upgrade() {
                        inner.activate_leaf(id);
                    }
                });
            }
            RowKind::Submenu { .. } => {
                let inner = Rc::downgrade(inner);
                let column = Rc::downgrade(column);
                row.button.connect_clicked(move |_| {
                    if let (Some(inner), Some(column)) = (inner.upgrade(), column.upgrade()) {
                        inner.hover_row(&column, index);
                    }
                });
            }
        }
    }
}

/// Offset a submenu popover so its top edge lines up with the top of the row it
/// opened from, instead of GTK's default vertical centring on that row (which
/// pushes tall submenus off the top of the screen).
fn align_submenu_top(popover: &gtk::Popover) {
    let (Some(anchor), Some(content)) = (popover.parent(), popover.child()) else {
        return;
    };
    let anchor_height = anchor.height();
    // Measure rather than read `height()`: on `map` the content isn't allocated
    // yet, so `height()` is 0 and the offset would never apply.
    let (_, content_height, _, _) = content.measure(gtk::Orientation::Vertical, -1);
    if anchor_height > 0 && content_height > 0 {
        // GTK centres the popover on the anchor row; shift it down by half the
        // height difference so the popover's top sits at the row's top.
        popover.set_offset(0, (content_height - anchor_height) / 2);
    }
}

fn monitor_max_height(widget: &gtk::Widget) -> i32 {
    let height = widget
        .native()
        .and_then(|native| native.surface())
        .and_then(|surface| widget.display().monitor_at_surface(&surface))
        .map(|monitor| monitor.geometry().height())
        .filter(|height| *height > 0)
        .unwrap_or(FALLBACK_MONITOR_HEIGHT);

    (f64::from(height) * MAX_HEIGHT_FRACTION) as i32
}

fn separator() -> gtk::Separator {
    let separator = gtk::Separator::new(gtk::Orientation::Horizontal);
    separator.add_css_class("systray-menu-divider");
    separator
}

fn apply_disposition_class(widget: &gtk::Box, disposition: Disposition) {
    match disposition {
        Disposition::Warning => widget.add_css_class("warning"),
        Disposition::Alert => widget.add_css_class("alert"),
        Disposition::Normal | Disposition::Informative => {}
    }
}

/// A left-aligned check/radio indicator. The image is empty when unchecked; CSS
/// reserves its width so labels stay aligned.
fn toggle_indicator(item: &MenuItem) -> gtk::Image {
    let indicator = gtk::Image::new();
    indicator.add_css_class("systray-menu-item-indicator");

    if item.toggle_state == ToggleState::Checked {
        let icon = match item.toggle_type {
            ToggleType::Radio => "radio-checked-symbolic",
            _ => "object-select-symbolic",
        };
        indicator.set_icon_name(Some(icon));
    }

    indicator
}

fn build_icon(item: &MenuItem) -> Option<gtk::Image> {
    if let Some(name) = item.icon_name.as_deref().filter(|name| !name.is_empty()) {
        let image = gtk::Image::from_icon_name(name);
        image.add_css_class("systray-menu-item-icon");
        return Some(image);
    }

    if let Some(data) = item.icon_data.as_deref().filter(|data| !data.is_empty()) {
        let bytes = glib::Bytes::from(data);
        if let Ok(texture) = gdk::Texture::from_bytes(&bytes) {
            let image = gtk::Image::from_paintable(Some(&texture));
            image.add_css_class("systray-menu-item-icon");
            return Some(image);
        }
    }

    None
}

/// Format a DBusMenu shortcut (`[["Control", "q"]]`) as `Control+q` for display.
fn format_shortcut(shortcut: &[Vec<String>]) -> Option<String> {
    let combo = shortcut.first()?;
    if combo.is_empty() {
        return None;
    }
    Some(combo.join("+"))
}
