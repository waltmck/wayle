//! Tracks the one open dismissable surface per bar (a dropdown popover or a
//! systray menu) so opening another closes it — the non-grab replacement for
//! GTK autohide's "only one popup at a time" behavior.
//!
//! It also owns the shared [`Scrim`] and drives the whole open/close ceremony, so
//! the two surface types (dropdowns, systray menu) don't each hand-roll it:
//! [`open`](OpenSurfaceCoordinator::open) shows the scrim then closes the previous
//! surface (show-before-close keeps the scrim ref-count positive across a swap),
//! and [`notify_closed`](OpenSurfaceCoordinator::notify_closed) hides it.
//!
//! A surface is represented by *how to close it* (a `DismissFn`) plus an optional
//! *key handler* (`KeyHandler`) for keyboard nav and the *anchor* bar widget it
//! hangs off. Both the bar window and the scrim hold keyboard focus in different
//! situations (pointer over the bar/popover vs the empty desktop), and the menu's
//! popovers never get GTK focus themselves (their buttons are non-focusable), so
//! nav keys are forwarded here from whichever surface has focus rather than handled
//! per-popover. Identity is by `Rc` pointer on the `DismissFn`, which lets a
//! surface's `connect_closed` ask "am I still the registered one?" without
//! downcasting.

use std::{
    cell::{Cell, OnceCell, RefCell},
    rc::Rc,
};

use relm4::gtk::{self, prelude::*};

use super::scrim::Scrim;

/// A currently-open dismissable surface, as a closure that closes it.
pub(crate) type DismissFn = Rc<dyn Fn()>;

/// Consumes a forwarded key for the open surface; returns `true` when handled.
pub(crate) type KeyHandler = Rc<dyn Fn(gtk::gdk::Key) -> bool>;

struct OpenSurface {
    dismiss: DismissFn,
    keys: Option<KeyHandler>,
    /// The bar widget this surface hangs off (a dropdown/tray button). A bar click
    /// on this widget is left to the button's own toggle/swap; a bar click
    /// anywhere else dismisses.
    anchor: Option<gtk::glib::WeakRef<gtk::Widget>>,
}

#[derive(Default)]
pub(crate) struct OpenSurfaceCoordinator {
    current: RefCell<Option<OpenSurface>>,
    /// Set while `open` is closing the previous surface, so that surface's
    /// `connect_closed` → `notify_closed` doesn't clear the newly-registered one.
    dismissing: Cell<bool>,
    /// The shared dismissal scrim. Set once at init (see `set_scrim`); shown on
    /// `open`, hidden on `notify_closed`, so surfaces don't touch it directly.
    scrim: OnceCell<Rc<Scrim>>,
}

impl OpenSurfaceCoordinator {
    /// Attach the scrim. Called once at construction (the scrim needs the
    /// coordinator, so it can't be passed to `new`).
    pub(crate) fn set_scrim(&self, scrim: Rc<Scrim>) {
        let _ = self.scrim.set(scrim);
    }

    fn scrim(&self) -> &Rc<Scrim> {
        self.scrim.get().expect("scrim attached at init")
    }

    /// Register `dismiss` (an optional key handler, and the anchor bar widget) as
    /// the open surface, closing whatever was open first. The caller pops its own
    /// surface up after.
    pub(crate) fn open(
        &self,
        dismiss: DismissFn,
        keys: Option<KeyHandler>,
        anchor: Option<gtk::glib::WeakRef<gtk::Widget>>,
    ) {
        // Show the scrim BEFORE closing the previous surface so its ref-count stays
        // positive across the swap (no flicker / bar-layer thrash).
        self.scrim().show();
        // Install the new one BEFORE closing the old, so the old surface's
        // `connect_closed` (→ `notify_closed`) sees a different `current` and is
        // a no-op for it.
        let previous = self.current.borrow_mut().replace(OpenSurface {
            dismiss,
            keys,
            anchor,
        });
        if let Some(previous) = previous {
            self.dismissing.set(true);
            (previous.dismiss)();
            self.dismissing.set(false);
        }
    }

    /// A surface reports it closed (from its `connect_closed`). Clears the slot
    /// (unless mid-swap or no longer the registered one) and drops its scrim ref.
    /// Callers must invoke this exactly once per `open` so the scrim ref-count
    /// stays balanced.
    pub(crate) fn notify_closed(&self, who: &DismissFn) {
        if !self.dismissing.get() {
            let mut current = self.current.borrow_mut();
            if current
                .as_ref()
                .is_some_and(|open| Rc::ptr_eq(&open.dismiss, who))
            {
                *current = None;
            }
        }
        self.scrim().hide();
    }

    /// Close whatever is open (the scrim/bar Escape handlers, scrim outside-click).
    /// Returns `true` if something was open and dismissed, so callers can decide
    /// whether to consume the key.
    pub(crate) fn dismiss_current(&self) -> bool {
        let current = self.current.borrow_mut().take();
        if let Some(current) = current {
            (current.dismiss)();
            true
        } else {
            false
        }
    }

    /// Dismiss the open surface for a click on the bar at `target` (the picked
    /// widget), unless the click landed on that surface's own anchor button —
    /// which handles its own toggle/swap. So clicking empty bar, a workspace
    /// button, etc. dismisses; clicking the open dropdown's button toggles it.
    pub(crate) fn handle_bar_click(&self, target: Option<&gtk::Widget>) {
        let on_anchor = self
            .current
            .borrow()
            .as_ref()
            .and_then(|open| open.anchor.as_ref())
            .and_then(gtk::glib::WeakRef::upgrade)
            .zip(target)
            .is_some_and(|(anchor, target)| *target == anchor || target.is_ancestor(&anchor));
        if !on_anchor {
            self.dismiss_current();
        }
    }

    /// Forward a key to the open surface's key handler (the systray menu's nav);
    /// returns `true` if consumed. Clones the handler out before calling so no
    /// borrow is held across a re-entrant close (activation/Escape pops the menu
    /// down, which clears `current`).
    pub(crate) fn handle_key(&self, key: gtk::gdk::Key) -> bool {
        let handler = self
            .current
            .borrow()
            .as_ref()
            .and_then(|open| open.keys.clone());
        handler.is_some_and(|handler| handler(key))
    }

    /// The single Escape/nav-key policy for both key controllers (bar window and
    /// scrim): Escape dismisses the open surface, other keys drive the systray
    /// menu's nav. Returns the propagation the controller should report.
    pub(crate) fn handle_key_event(&self, key: gtk::gdk::Key) -> gtk::glib::Propagation {
        let handled = if key == gtk::gdk::Key::Escape {
            self.dismiss_current()
        } else {
            self.handle_key(key)
        };
        if handled {
            gtk::glib::Propagation::Stop
        } else {
            gtk::glib::Propagation::Proceed
        }
    }
}
