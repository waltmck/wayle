//! One column of the cascading tray menu: its own [`gtk::Popover`] holding a
//! vertically-scrolling list of rows, plus the hover-selection cursor and the
//! submenu currently open beneath it.
//!
//! Each column is an independent non-grab popover, so the cascade positions,
//! flips, and scrolls per surface, and each is naturally its own floating card.
//! The menu is mouse-driven; the `.selected` CSS class is the hover highlight.

use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

use relm4::gtk::{self, prelude::*};

/// A single menu column.
pub(super) struct MenuColumn {
    /// This column's popover surface (parented to the owning row button, or the
    /// tray button for the root).
    pub(super) popover: gtk::Popover,
    /// Rows in visual order (separators are appended to the list box but are not
    /// entries here).
    pub(super) rows: Vec<MenuRow>,
    /// Index of the hovered row, or `None`.
    pub(super) cursor: Cell<Option<usize>>,
    /// Depth in the cascade; `0` is the root column.
    pub(super) depth: usize,
    /// The submenu column currently shown beneath one of this column's rows.
    pub(super) open_child: RefCell<Option<Rc<MenuColumn>>>,
    /// Back-link to the parent column (empty on the root); used to clear the
    /// parent's `open_child` when this column's popover closes for any reason.
    pub(super) parent: Weak<MenuColumn>,
}

/// A menu row: a button plus what activating it does.
pub(super) struct MenuRow {
    pub(super) button: gtk::Button,
    pub(super) kind: RowKind,
}

pub(super) enum RowKind {
    Leaf { id: i32 },
    Submenu { column: Rc<MenuColumn> },
}

impl MenuColumn {
    /// Move the `.selected` (hover) marker to `next` (or clear it when `None`).
    pub(super) fn set_selected(&self, next: Option<usize>) {
        let previous = self.cursor.replace(next);
        if previous == next {
            return;
        }
        if let Some(index) = previous
            && let Some(row) = self.rows.get(index)
        {
            row.button.remove_css_class("selected");
        }
        if let Some(index) = next
            && let Some(row) = self.rows.get(index)
        {
            row.button.add_css_class("selected");
        }
    }

    /// Show `child`'s popover beneath one of this column's rows, closing any
    /// other child first. No-op if `child` is already the open one.
    pub(super) fn open_child(&self, child: &Rc<MenuColumn>) {
        let already_open = self
            .open_child
            .borrow()
            .as_ref()
            .is_some_and(|open| Rc::ptr_eq(open, child));
        if already_open {
            return;
        }
        self.close_open_child();
        child.set_selected(None);
        child.popover.popup();
        *self.open_child.borrow_mut() = Some(child.clone());
    }

    /// Close the open submenu (and, via `cascade_popdown`, its descendants).
    /// Takes the child out before popping down so no borrow is held across the
    /// GTK call.
    pub(super) fn close_open_child(&self) {
        let child = self.open_child.borrow_mut().take();
        if let Some(child) = child {
            child.popover.popdown();
        }
    }

    /// The submenu column owned by row `index`, if that row is a submenu.
    pub(super) fn submenu_at(&self, index: usize) -> Option<&Rc<MenuColumn>> {
        match self.rows.get(index)?.kind {
            RowKind::Submenu { ref column } => Some(column),
            RowKind::Leaf { .. } => None,
        }
    }

    /// First sensitive row (keyboard nav entry point), or `None` if the column
    /// has no selectable rows.
    pub(super) fn first_selectable(&self) -> Option<usize> {
        self.next_from(None)
    }

    /// Next sensitive row after `current` (wrapping); starts at row 0 when
    /// `current` is `None`. Skips insensitive (disabled) rows.
    pub(super) fn next_from(&self, current: Option<usize>) -> Option<usize> {
        let count = self.rows.len();
        if count == 0 {
            return None;
        }
        let start = current.map_or(0, |index| (index + 1) % count);
        (0..count).find_map(|offset| {
            let index = (start + offset) % count;
            self.rows[index].button.is_sensitive().then_some(index)
        })
    }

    /// Previous sensitive row before `current` (wrapping); starts at the last row
    /// when `current` is `None`. Skips insensitive (disabled) rows.
    pub(super) fn prev_from(&self, current: Option<usize>) -> Option<usize> {
        let count = self.rows.len();
        if count == 0 {
            return None;
        }
        let start = current.map_or(count - 1, |index| (index + count - 1) % count);
        (0..count).find_map(|offset| {
            let index = (start + count - offset) % count;
            self.rows[index].button.is_sensitive().then_some(index)
        })
    }

    /// Scroll this column so row `index` is visible (keyboard nav can land on a
    /// row currently scrolled out of view in a tall column, e.g. a country list).
    pub(super) fn scroll_into_view(&self, index: usize) {
        let Some(row) = self.rows.get(index) else {
            return;
        };
        let Some(scrolled) = self.popover.child().and_downcast::<gtk::ScrolledWindow>() else {
            return;
        };
        let Some(bounds) = row.button.compute_bounds(&scrolled) else {
            return;
        };
        let vadjustment = scrolled.vadjustment();
        let top = f64::from(bounds.y());
        let bottom = top + f64::from(bounds.height());
        let page = vadjustment.page_size();
        if top < 0.0 {
            vadjustment.set_value(vadjustment.value() + top);
        } else if bottom > page {
            vadjustment.set_value(vadjustment.value() + (bottom - page));
        }
    }
}
