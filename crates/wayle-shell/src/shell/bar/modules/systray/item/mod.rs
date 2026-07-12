mod helpers;
mod menu;
mod methods;
mod watchers;

use std::{cell::Cell, rc::Rc, sync::Arc};

use relm4::{
    gtk::{self, prelude::*},
    prelude::*,
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};
use wayle_config::ConfigService;
use wayle_systray::{core::item::TrayItem, types::Coordinates};

use crate::shell::bar::dropdowns::{DismissFn, OpenSurfaceCoordinator};

pub(super) struct SystrayItemInit {
    pub(super) item: Arc<TrayItem>,
    pub(super) config: Arc<ConfigService>,
    pub(super) coordinator: Rc<OpenSurfaceCoordinator>,
}

pub(super) struct SystrayItem {
    item: Arc<TrayItem>,
    config: Arc<ConfigService>,
    button: Option<gtk::Button>,
    icon: Option<gtk::Image>,
    icon_color_provider: Option<gtk::CssProvider>,
    menu: Option<menu::TrayMenu>,
    coordinator: Rc<OpenSurfaceCoordinator>,
    /// The open menu's dismiss closure and a "already cleaned up" guard, so the
    /// coordinator registration is released exactly once (via either the popover's
    /// `connect_closed` or an explicit teardown, never both).
    menu_reg: Option<(DismissFn, Rc<Cell<bool>>)>,
    /// Set while an async `AboutToShow` refresh is pending, so a second right-click
    /// in that window doesn't spawn a second `ShowMenu` that flaps the menu closed.
    pending_show: Cell<bool>,
    cancel_token: CancellationToken,
}

#[derive(Debug)]
#[allow(clippy::enum_variant_names)]
pub(super) enum SystrayItemMsg {
    LeftClick,
    RightClick,
    MiddleClick,
    ShowMenu,
    MenuUpdated,
    IconUpdated,
}

#[derive(Debug)]
pub(super) enum SystrayItemOutput {}

#[relm4::factory(pub(super))]
impl FactoryComponent for SystrayItem {
    type Init = SystrayItemInit;
    type Input = SystrayItemMsg;
    type Output = SystrayItemOutput;
    type CommandOutput = ();
    type ParentWidget = gtk::Box;

    view! {
        #[root]
        gtk::Button {
            set_css_classes: &["systray-item"],
            set_cursor_from_name: Some("pointer"),

            #[name = "icon"]
            gtk::Image {},
        }
    }

    fn init_model(
        init: Self::Init,
        _index: &relm4::factory::DynamicIndex,
        _sender: relm4::prelude::FactorySender<Self>,
    ) -> Self {
        Self {
            item: init.item,
            config: init.config,
            button: None,
            icon: None,
            icon_color_provider: None,
            menu: None,
            coordinator: init.coordinator,
            menu_reg: None,
            pending_show: Cell::new(false),
            cancel_token: CancellationToken::new(),
        }
    }

    fn init_widgets(
        &mut self,
        _index: &relm4::factory::DynamicIndex,
        root: Self::Root,
        _returned_widget: &<Self::ParentWidget as relm4::factory::FactoryView>::ReturnedWidget,
        sender: relm4::prelude::FactorySender<Self>,
    ) -> Self::Widgets {
        let item_id = self.item.id.get();
        root.set_widget_name(&item_id);
        debug!(item_id = %item_id, "init_widgets: setting up button");

        self.button = Some(root.clone());

        root.connect_clicked({
            let sender = sender.clone();
            move |_| {
                sender.input(SystrayItemMsg::LeftClick);
            }
        });

        let right_click = gtk::GestureClick::builder().button(3).build();
        let middle_click = gtk::GestureClick::builder().button(2).build();

        right_click.connect_released({
            let sender = sender.clone();
            move |gesture, _, _, _| {
                gesture.set_state(gtk::EventSequenceState::Claimed);
                sender.input(SystrayItemMsg::RightClick);
            }
        });

        middle_click.connect_released({
            let sender = sender.clone();
            move |gesture, _, _, _| {
                gesture.set_state(gtk::EventSequenceState::Claimed);
                sender.input(SystrayItemMsg::MiddleClick);
            }
        });

        root.add_controller(right_click);
        root.add_controller(middle_click);

        watchers::spawn_menu_watcher(&sender, &self.item, self.cancel_token.clone());
        watchers::spawn_icon_watcher(&sender, &self.item, self.cancel_token.clone());

        let widgets = view_output!();

        self.icon = Some(widgets.icon.clone());
        self.update_icon(&widgets.icon);

        widgets
    }

    fn update(&mut self, msg: Self::Input, _sender: relm4::prelude::FactorySender<Self>) {
        match msg {
            SystrayItemMsg::LeftClick => {
                let item = self.item.clone();
                let item_is_menu = item.item_is_menu.get();
                tokio::spawn(async move {
                    let result = if item_is_menu {
                        item.context_menu(Coordinates::new(0, 0)).await
                    } else {
                        item.activate(Coordinates::new(0, 0)).await
                    };
                    if let Err(error) = result {
                        warn!(
                            id = %item.id.get(),
                            bus_name = %item.bus_name.get(),
                            error = %error,
                            "systray activate failed"
                        );
                    }
                });
            }
            SystrayItemMsg::RightClick => {
                self.request_menu_show(&_sender);
            }

            SystrayItemMsg::ShowMenu => {
                self.toggle_menu();
            }
            SystrayItemMsg::MiddleClick => {
                let item = self.item.clone();
                tokio::spawn(async move {
                    if let Err(error) = item.secondary_activate(Coordinates::new(0, 0)).await {
                        warn!(
                            id = %item.id.get(),
                            bus_name = %item.bus_name.get(),
                            error = %error,
                            "systray secondary_activate failed"
                        );
                    }
                });
            }
            SystrayItemMsg::MenuUpdated => {
                self.rebuild_menu_if_visible();
            }
            SystrayItemMsg::IconUpdated => {
                if let Some(icon) = self.icon.clone() {
                    self.update_icon(&icon);
                }
            }
        }
    }
}

impl Drop for SystrayItem {
    fn drop(&mut self) {
        self.cancel_token.cancel();
        self.clear_menu_registration();
        if let Some(menu) = self.menu.take() {
            menu.teardown();
        }
    }
}
