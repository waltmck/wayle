use std::{
    any::Any,
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
};

use gtk::prelude::*;
use relm4::{gtk, prelude::*};
use tracing::{debug, warn};
use wayle_config::{ClickAction, schemas::bar::Location};
use wayle_widgets::prelude::{BarButton, BarButtonInput};

use super::coordinator::{DismissFn, OpenSurfaceCoordinator};
use super::scrim::Scrim;
use crate::{process, shell::services::ShellServices};

/// Per-bar wiring attached to each dropdown after creation, so its open/close
/// hooks can reach the shared dismissal coordinator (which owns the scrim).
#[derive(Clone)]
struct DropdownWiring {
    coordinator: Rc<OpenSurfaceCoordinator>,
}

/// Returns `value` unchanged, logging at debug if it is `None`.
///
/// Use inside dropdown factories that gate on a service dependency: instead of
/// returning `None` silently, the helper records which dropdown failed and the
/// service it was waiting on, so the dispatch-site catch-all has the cause
/// already in the log before it runs.
pub(crate) fn require_service<T>(
    dropdown: &'static str,
    service: &'static str,
    value: Option<T>,
) -> Option<T> {
    if value.is_none() {
        debug!(
            dropdown,
            service, "service unavailable, dropdown disabled on this system"
        );
    }
    value
}

/// Shared dropdown instance for a dropdown name.
///
/// Reuse keeps dropdown state consistent across modules that reference the same
/// dropdown and avoids rebuilding the same component repeatedly.
pub(crate) struct DropdownInstance {
    popover: gtk::Popover,
    _controller: Box<dyn Any>,
    thaw_target: Rc<Cell<Option<relm4::Sender<BarButtonInput>>>>,
    /// Filled by `DropdownRegistry::get_or_create` after construction (the
    /// factories that call `new` don't have the coordinator/scrim).
    wiring: Rc<RefCell<Option<DropdownWiring>>>,
    /// This dropdown's dismiss closure while it's the open surface, so
    /// `connect_closed` can tell the coordinator it closed.
    current_dismiss: Rc<RefCell<Option<DismissFn>>>,
}

impl DropdownInstance {
    pub(crate) fn new(popover: gtk::Popover, controller: Box<dyn Any>) -> Self {
        let thaw_target: Rc<Cell<Option<relm4::Sender<BarButtonInput>>>> = Rc::default();
        let wiring: Rc<RefCell<Option<DropdownWiring>>> = Rc::default();
        let current_dismiss: Rc<RefCell<Option<DismissFn>>> = Rc::default();

        popover.connect_map(|popover| {
            debug!(
                width = popover.width(),
                height = popover.height(),
                classes = ?popover.css_classes(),
                "popover mapped"
            );
        });

        let thaw = thaw_target.clone();
        let wiring_closed = wiring.clone();
        let dismiss_closed = current_dismiss.clone();
        popover.connect_closed(move |popover| {
            debug!(
                width = popover.width(),
                height = popover.height(),
                classes = ?popover.css_classes(),
                "popover closed"
            );
            let frozen_sender = thaw.take();

            if let Some(sender) = &frozen_sender {
                sender.emit(BarButtonInput::ThawSize);
            }

            if frozen_sender.is_some()
                && let Some(parent) = popover.parent()
            {
                parent.set_size_request(-1, -1);
            }

            // Clone the wiring out before calling into it so no borrow is held
            // across GTK/coordinator calls that could re-enter. `notify_closed`
            // also hides the scrim; Escape/keyboard are handled at the bar/scrim
            // level, not per-popover.
            let wiring = wiring_closed.borrow().clone();
            if let (Some(wiring), Some(dismiss)) = (wiring, dismiss_closed.borrow_mut().take()) {
                wiring.coordinator.notify_closed(&dismiss);
            }
        });

        Self {
            popover,
            _controller: controller,
            thaw_target,
            wiring,
            current_dismiss,
        }
    }

    fn attach(&self, wiring: DropdownWiring) {
        *self.wiring.borrow_mut() = Some(wiring);
    }

    /// Register this dropdown as the open surface (closing any other) and show the
    /// scrim. Called right before `popup()`.
    fn register_open(&self) {
        let Some(wiring) = self.wiring.borrow().clone() else {
            return;
        };
        let popover = self.popover.downgrade();
        let dismiss: DismissFn = Rc::new(move || {
            if let Some(popover) = popover.upgrade() {
                popover.popdown();
            }
        });
        *self.current_dismiss.borrow_mut() = Some(dismiss.clone());
        // Dropdowns have no custom key handler — their own focused widgets handle
        // keys; Escape is handled by the bar/scrim via `dismiss_current`. The
        // anchor is the button the popover hangs off, so clicking it toggles/swaps
        // rather than plain-dismisses. `open` shows the scrim.
        let anchor = self.popover.parent().map(|widget| widget.downgrade());
        wiring.coordinator.open(dismiss, None, anchor);
    }

    /// Toggles popover visibility for the given bar button.
    ///
    /// If the popover is already open for this button, it closes; otherwise it
    /// opens anchored to the current button. Margins are applied from the
    /// registry so individual dropdowns never handle positioning.
    fn toggle_for(&self, bar_button: &Controller<BarButton>, style: DropdownStyle) {
        let widget = bar_button.widget();
        let widget_ref = widget.upcast_ref::<gtk::Widget>();
        let visible = self.popover.is_visible();
        let same_parent = self.popover.parent().as_ref() == Some(widget_ref);

        if visible && same_parent {
            self.popover.popdown();
            return;
        }

        if visible {
            self.reparent_and_show(bar_button, style);
            return;
        }

        self.ensure_parent(widget_ref);
        self.freeze_and_show(bar_button, style);
    }

    /// Toggles popover visibility anchored to an arbitrary widget.
    ///
    /// Unlike `toggle_for`, this does not freeze/thaw a `BarButton` or lock
    /// parent size.
    fn toggle_for_widget(&self, widget: &impl IsA<gtk::Widget>, style: DropdownStyle) {
        let widget_ref = widget.upcast_ref::<gtk::Widget>();
        let same_parent = self.popover.parent().as_ref() == Some(widget_ref);

        if self.popover.is_visible() && same_parent {
            self.popover.popdown();
            return;
        }

        self.ensure_parent(widget_ref);
        self.show_for_widget(style);
    }

    fn show_for_widget(&self, style: DropdownStyle) {
        self.apply_position();
        self.apply_margins(style.margins);
        self.apply_style(&style);
        self.register_open();
        self.popover.popup();
    }

    fn reparent_and_show(&self, bar_button: &Controller<BarButton>, style: DropdownStyle) {
        if let Some(sender) = self.thaw_target.take() {
            sender.emit(BarButtonInput::ThawSize);
        }
        self.ensure_parent(bar_button.widget().upcast_ref());
        self.freeze_and_show(bar_button, style);
    }

    fn ensure_parent(&self, target: &gtk::Widget) {
        if self.popover.parent().as_ref() == Some(target) {
            return;
        }
        if self.popover.parent().is_some() {
            self.popover.unparent();
        }
        self.popover.set_parent(target);

        let popover = self.popover.downgrade();
        target.connect_destroy(move |destroyed| {
            let Some(popover) = popover.upgrade() else {
                return;
            };
            if popover.parent().as_ref() == Some(destroyed) {
                popover.unparent();
            }
        });
    }

    fn freeze_and_show(&self, bar_button: &Controller<BarButton>, style: DropdownStyle) {
        if style.freeze_label {
            self.thaw_target.set(Some(bar_button.sender().clone()));
            bar_button.emit(BarButtonInput::FreezeSize);
            self.lock_parent_size();
        }

        self.apply_position();
        self.apply_margins(style.margins);
        self.apply_style(&style);
        self.register_open();
        self.popover.popup();
    }

    fn apply_style(&self, style: &DropdownStyle) {
        self.popover.set_opacity(style.opacity);
        // Never autohide: the Wayland popup grab freezes input and swallows the
        // dismiss-click on Hyprland. Dismissal is handled by the coordinator +
        // scrim instead (see `register_open`).
        self.popover.set_autohide(false);
        if style.shadow_enabled {
            self.popover.add_css_class("shadow");
        } else {
            self.popover.remove_css_class("shadow");
        }
    }

    fn apply_position(&self) {
        let Some(parent) = self.popover.parent() else {
            return;
        };
        let position = Self::detect_popover_position(&parent);
        self.popover.set_position(position);

        for class in &[
            "position-top",
            "position-bottom",
            "position-left",
            "position-right",
        ] {
            self.popover.remove_css_class(class);
        }
        let class = match position {
            gtk::PositionType::Top => "position-top",
            gtk::PositionType::Bottom => "position-bottom",
            gtk::PositionType::Left => "position-left",
            gtk::PositionType::Right => "position-right",
            _ => "position-bottom",
        };
        self.popover.add_css_class(class);
    }

    fn apply_margins(&self, margins: DropdownMargins) {
        let Some(child) = self.popover.child() else {
            return;
        };
        child.set_margin_top(margins.top);
        child.set_margin_bottom(margins.bottom);
        child.set_margin_start(margins.start);
        child.set_margin_end(margins.end);
    }

    fn lock_parent_size(&self) {
        let Some(parent) = self.popover.parent() else {
            return;
        };
        parent.set_size_request(parent.width(), parent.height());
    }

    fn detect_popover_position(widget: &gtk::Widget) -> gtk::PositionType {
        let Some(window) = widget.root().and_then(|r| r.downcast::<gtk::Window>().ok()) else {
            return gtk::PositionType::Bottom;
        };

        if window.has_css_class("bottom") {
            gtk::PositionType::Top
        } else if window.has_css_class("left") {
            gtk::PositionType::Right
        } else if window.has_css_class("right") {
            gtk::PositionType::Left
        } else {
            gtk::PositionType::Bottom
        }
    }
}

impl Drop for DropdownInstance {
    fn drop(&mut self) {
        if self.popover.parent().is_some() {
            self.popover.unparent();
        }
    }
}

struct DropdownStyle {
    margins: DropdownMargins,
    opacity: f64,
    shadow_enabled: bool,
    freeze_label: bool,
}

const REM_PX: f32 = 16.0;

/// Pixel margins applied to dropdown containers.
///
/// Values are rounded to whole pixels so popover content stays visually crisp.
/// The bar-facing edge gets a smaller gap; the opposite edge and sides get
/// standard content padding.
#[derive(Debug, Clone, Copy)]
struct DropdownMargins {
    top: i32,
    bottom: i32,
    start: i32,
    end: i32,
}

impl DropdownMargins {
    const GAP_REM: f32 = 0.275;
    const CONTENT_REM: f32 = 1.0;

    fn new(scale: f32, location: Location) -> Self {
        let gap = Self::round(Self::GAP_REM, scale);
        let content = Self::round(Self::CONTENT_REM, scale);

        match location {
            Location::Top => Self {
                top: gap,
                bottom: content,
                start: content,
                end: content,
            },
            Location::Bottom => Self {
                top: content,
                bottom: gap,
                start: content,
                end: content,
            },
            Location::Left => Self {
                top: content,
                bottom: content,
                start: gap,
                end: content,
            },
            Location::Right => Self {
                top: content,
                bottom: content,
                start: content,
                end: gap,
            },
        }
    }

    fn round(rem: f32, scale: f32) -> i32 {
        (rem * REM_PX * scale).round() as i32
    }
}

/// Factory trait for creating dropdown component instances.
pub(crate) trait DropdownFactory {
    /// Creates a dropdown component, returning `None` if required services are unavailable.
    fn create(services: &ShellServices) -> Option<DropdownInstance>;
}

/// Cache of dropdown instances keyed by dropdown name.
///
/// Dropdowns are created lazily on first use and reused afterward so repeated
/// interactions resolve to the same logical dropdown instance.
pub(crate) struct DropdownRegistry {
    services: ShellServices,
    cache: RefCell<HashMap<String, Rc<DropdownInstance>>>,
    coordinator: Rc<OpenSurfaceCoordinator>,
}

impl DropdownRegistry {
    pub(crate) fn new(
        services: &ShellServices,
        monitor: &gtk::gdk::Monitor,
        bar_window: &gtk::Window,
    ) -> Self {
        let coordinator = Rc::new(OpenSurfaceCoordinator::default());
        // The scrim needs the coordinator (for its dismiss handlers), and the
        // coordinator owns the scrim thereafter (show/hide on open/close).
        coordinator.set_scrim(Scrim::new(services, monitor, bar_window, &coordinator));
        Self {
            services: services.clone(),
            cache: RefCell::default(),
            coordinator,
        }
    }

    pub(crate) fn coordinator(&self) -> Rc<OpenSurfaceCoordinator> {
        self.coordinator.clone()
    }

    pub(crate) fn warm_all(&self) {
        for name in super::DROPDOWN_NAMES {
            let _ = self.get_or_create(name);
        }
    }

    #[allow(clippy::cognitive_complexity)]
    fn get_or_create(&self, name: &str) -> Option<Rc<DropdownInstance>> {
        let mut cache = self.cache.borrow_mut();
        if let Some(instance) = cache.get(name) {
            debug!(dropdown = name, "cache hit");
            return Some(instance.clone());
        }

        debug!(dropdown = name, "creating dropdown");
        let Some(raw) = super::create(name, &self.services) else {
            debug!(
                dropdown = name,
                "no instance created (factory declined, usually due to a missing service or dependency \
                 -- see preceding debug log from the factory for the specific cause)"
            );
            return None;
        };
        let instance = Rc::new(raw);
        instance.attach(DropdownWiring {
            coordinator: self.coordinator.clone(),
        });
        cache.insert(name.to_owned(), instance.clone());
        debug!(dropdown = name, "dropdown cached");
        Some(instance)
    }
}

/// Dispatches a click action: toggles dropdown, runs shell command, or no-ops.
pub(crate) fn dispatch_click(
    action: &ClickAction,
    registry: &DropdownRegistry,
    bar_button: &Controller<BarButton>,
) {
    dispatch_action(action, registry, |dropdown, style| {
        dropdown.toggle_for(bar_button, style);
    });
}

/// Dispatches a click action anchored to an arbitrary widget instead of a `BarButton`.
pub(crate) fn dispatch_click_widget(
    action: &ClickAction,
    registry: &DropdownRegistry,
    widget: &impl IsA<gtk::Widget>,
) {
    dispatch_action(action, registry, |dropdown, style| {
        dropdown.toggle_for_widget(widget, style);
    });
}

#[allow(clippy::cognitive_complexity)]
fn dispatch_action(
    action: &ClickAction,
    registry: &DropdownRegistry,
    toggle: impl FnOnce(&DropdownInstance, DropdownStyle),
) {
    match action {
        ClickAction::Dropdown(name) => {
            debug!(dropdown = %name, "click: dropdown");
            if let Some(dropdown) = registry.get_or_create(name) {
                let style = dropdown_style(registry);
                toggle(&dropdown, style);
            } else {
                warn!(
                    dropdown = %name,
                    "click dropped: no dropdown available (dropdown is either unregistered or its \
                     backing service is unavailable on this system)"
                );
            }
        }
        ClickAction::Shell(cmd) => {
            debug!(command = %cmd, "click: shell");
            process::run_if_set(cmd);
        }
        ClickAction::None => debug!("click: none"),
    }
}

fn dropdown_style(registry: &DropdownRegistry) -> DropdownStyle {
    let config = registry.services.config.config();
    let bar = &config.bar;
    let scale = bar.scale.get().value();
    DropdownStyle {
        margins: DropdownMargins::new(scale, bar.location.get()),
        opacity: f64::from(bar.dropdown_opacity.get().value()) / 100.0,
        shadow_enabled: bar.dropdown_shadow.get(),
        freeze_label: bar.dropdown_freeze_label.get(),
    }
}
