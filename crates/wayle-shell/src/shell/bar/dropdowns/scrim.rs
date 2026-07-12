//! A transparent, fullscreen layer-shell surface shown while a dropdown or
//! systray menu is open — the non-grab replacement for GTK autohide's
//! outside-click dismissal.
//!
//! Why not autohide: on Hyprland the autohide xdg_popup grab freezes input
//! until the next pointer motion after an outside-click and swallows the
//! dismiss-click so clicking another bar button doesn't open it. Non-grab
//! popovers avoid both, but then get no notification of outside interaction —
//! that's what the scrim provides: a click anywhere on it (empty desktop / other
//! windows) closes the open surface, and Escape over it does too.
//!
//! Stacking is the whole game. The dropdown/menu popovers are xdg_popups glued
//! to the *bar* surface, so they inherit the bar's stacking. For the scrim to
//! catch outside clicks it must be above application windows; for the popovers
//! and bar buttons to stay clickable they must be above the scrim. Both the scrim
//! and the bar sit on `overlay` while shown: the scrim is presented first, then
//! the bar is (re-)raised to `overlay` so it stacks *above* the scrim. Using
//! `overlay` (rather than `top`) keeps the scrim above fullscreen windows, so
//! dismissal keeps working while an app is fullscreen. On hide the bar drops back
//! to its configured layer. Result: popover > bar > scrim > everything else.
//!
//! Keyboard follows focus. Both the scrim and the bar use `KeyboardMode::OnDemand`
//! while shown, so under focus-follows-mouse whichever surface the pointer is over
//! receives keys: over a popover, the bar's popover handles them (dropdown Escape,
//! menu arrow-nav); over the empty desktop, the scrim handles Escape → dismiss. No
//! surface grabs the keyboard exclusively, so pointer hover/scroll on the menu is
//! unaffected.
//!
//! Transparent-but-clickable is the other catch. Input is independent of
//! opacity, but `gdk::Surface::set_input_region` only marks the region *dirty* —
//! it reaches the compositor only when a render frame runs and commits the
//! surface. `apply_input_region` therefore calls `queue_render()` right after, to
//! force the frame that commits the full-monitor input region. On wlroots the
//! surface must also commit *painted content* to be hit-tested, so the scrim has a
//! ~1/255 alpha background (see `.dropdown-scrim` in the base stylesheet) — a
//! full-monitor buffer that is visually imperceptible.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};

use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use relm4::gtk::{self, cairo, glib, prelude::*};

use super::coordinator::OpenSurfaceCoordinator;
use crate::shell::{helpers::layer_shell::apply_layer, services::ShellServices};

/// How long the real hide (scrim visibility + bar layer/keyboard restore) is
/// deferred after the last surface closes, so a swap's release-reopen — which
/// follows the press-dismiss of the same bar click — cancels it before the bar's
/// layer thrashes mid-click.
const HIDE_DEBOUNCE: Duration = Duration::from_millis(200);

pub(crate) struct Scrim {
    window: gtk::Window,
    bar_window: glib::WeakRef<gtk::Window>,
    services: ShellServices,
    /// Ref-count so an open/close swap (show-new before close-old) nets to shown.
    shown: Cell<u32>,
    /// Pending debounced hide (see `hide`); cancelled by a re-show mid-swap.
    hide_source: RefCell<Option<glib::SourceId>>,
}

/// Set the surface input region to the full monitor so the (transparent) scrim
/// receives pointer events. `set_input_region` only marks the region dirty; it is
/// committed to the compositor solely by a render frame, so `queue_render` is
/// required to flush it. Input is independent of opacity.
fn apply_input_region(window: &gtk::Window, monitor: &gtk::gdk::Monitor) {
    let Some(surface) = window.surface() else {
        return;
    };
    let geometry = monitor.geometry();
    let region = cairo::Region::create_rectangle(&cairo::RectangleInt::new(
        0,
        0,
        geometry.width(),
        geometry.height(),
    ));
    surface.set_input_region(Some(&region));
    // Schedule a frame so gdk flushes the pending input region via a surface
    // commit; set_input_region alone never reaches the compositor.
    surface.queue_render();
}

impl Scrim {
    pub(crate) fn new(
        services: &ShellServices,
        monitor: &gtk::gdk::Monitor,
        bar_window: &gtk::Window,
        coordinator: &Rc<OpenSurfaceCoordinator>,
    ) -> Rc<Self> {
        let window = gtk::Window::new();
        window.set_decorated(false);
        // Attach to the running application so the window gets a frame clock and
        // renders/allocates like the other shell surfaces.
        window.set_application(Some(&relm4::main_application()));
        // `.dropdown-scrim` gives it a ~1/255 alpha background (visually
        // transparent, but a committed buffer so wlroots hit-tests it).
        window.add_css_class("dropdown-scrim");

        // A layer-shell window anchored to all edges stretches the *surface*, but
        // GTK allocates the *content* from the child's measured size — an empty
        // catcher measures 0×0. Pin the window to the monitor size so it fills.
        let geometry = monitor.geometry();
        window.set_default_size(geometry.width(), geometry.height());

        window.init_layer_shell();
        window.set_namespace(Some("wayle-dropdown-scrim"));
        // `overlay` so the scrim stays above fullscreen windows (dismissal keeps
        // working while fullscreen); the bar is re-raised above it in `show`.
        window.set_layer(Layer::Overlay);
        window.set_monitor(Some(monitor));
        // Keyboard follows focus; on-demand while shown (set in `show`).
        window.set_keyboard_mode(KeyboardMode::None);
        for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
            window.set_anchor(edge, true);
        }
        window.set_exclusive_zone(-1);

        let catcher = gtk::Box::new(gtk::Orientation::Vertical, 0);
        catcher.set_hexpand(true);
        catcher.set_vexpand(true);
        let click = gtk::GestureClick::new();
        click.connect_pressed({
            let coordinator = Rc::downgrade(coordinator);
            move |gesture, _, _, _| {
                gesture.set_state(gtk::EventSequenceState::Claimed);
                if let Some(coordinator) = coordinator.upgrade() {
                    coordinator.dismiss_current();
                }
            }
        });
        catcher.add_controller(click);
        window.set_child(Some(&catcher));

        // When the pointer is over the scrim (empty desktop) the scrim holds
        // keyboard focus: Escape dismisses the open surface and nav keys drive the
        // systray menu's cascade (mirrors the bar's handler for the popover-focused
        // case; the shared policy lives on the coordinator).
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        keys.connect_key_pressed({
            let coordinator = Rc::downgrade(coordinator);
            move |_, keyval, _, _| {
                coordinator
                    .upgrade()
                    .map_or(gtk::glib::Propagation::Proceed, |coordinator| {
                        coordinator.handle_key_event(keyval)
                    })
            }
        });
        window.add_controller(keys);

        // Apply (and commit, via queue_render) the input region each time the
        // surface maps.
        window.connect_map({
            let monitor = monitor.clone();
            move |window| apply_input_region(window, &monitor)
        });

        Rc::new(Self {
            window,
            bar_window: bar_window.downgrade(),
            services: services.clone(),
            shown: Cell::new(0),
            hide_source: RefCell::new(None),
        })
    }

    /// Show the scrim (idempotent, ref-counted). On the first show it raises the
    /// bar above the scrim and switches both to on-demand keyboard.
    pub(crate) fn show(&self) {
        // Cancel any debounced hide: during a swap this keeps the bar on `overlay`
        // across the close/reopen instead of thrashing its layer mid-click.
        self.cancel_pending_hide();
        self.shown.set(self.shown.get() + 1);
        // Bring the surface up only if it isn't already showing — a cancelled
        // debounced hide leaves it visible, so no re-present / re-layer is needed.
        if !self.window.is_visible() {
            self.window.set_keyboard_mode(KeyboardMode::OnDemand);
            self.window.present();
            if let Some(bar) = self.bar_window.upgrade() {
                // Re-raise into overlay *after* the scrim maps so the bar (and its
                // popovers) stack above it; give it on-demand keyboard so hovering
                // a popover routes keys there.
                bar.set_layer(Layer::Overlay);
                bar.set_keyboard_mode(KeyboardMode::OnDemand);
            }
        }
    }

    /// Drop one show; on the last hide, restore the bar and hide — but debounced,
    /// so a swap's immediate re-show cancels it and the bar's layer never changes
    /// mid-click (which otherwise stalls/loses the clicked button's own action).
    pub(crate) fn hide(self: &Rc<Self>) {
        let remaining = self.shown.get().saturating_sub(1);
        self.shown.set(remaining);
        if remaining != 0 {
            return;
        }
        let scrim = Rc::downgrade(self);
        let source = glib::timeout_add_local_once(HIDE_DEBOUNCE, move || {
            if let Some(scrim) = scrim.upgrade() {
                scrim.finish_hide();
            }
        });
        *self.hide_source.borrow_mut() = Some(source);
    }

    fn cancel_pending_hide(&self) {
        if let Some(source) = self.hide_source.borrow_mut().take() {
            source.remove();
        }
    }

    /// The deferred tail of `hide`: actually hide the scrim and restore the bar's
    /// layer/keyboard, unless a re-show reclaimed it in the meantime.
    fn finish_hide(&self) {
        *self.hide_source.borrow_mut() = None;
        if self.shown.get() != 0 {
            return;
        }
        self.window.set_visible(false);
        self.window.set_keyboard_mode(KeyboardMode::None);
        if let Some(bar) = self.bar_window.upgrade() {
            bar.set_keyboard_mode(KeyboardMode::None);
            apply_layer(&bar, self.services.config.config().bar.layer.get(), &self.services.config);
        }
    }
}

impl Drop for Scrim {
    fn drop(&mut self) {
        // Cancel a debounced hide still in flight so teardown is deterministic (its
        // closure holds only a Weak<Self>, so it's harmless either way).
        self.cancel_pending_hide();
        // GTK keeps a ref on mapped toplevels; destroy explicitly so the
        // layer-shell surface goes away with the bar's registry.
        self.window.destroy();
    }
}
