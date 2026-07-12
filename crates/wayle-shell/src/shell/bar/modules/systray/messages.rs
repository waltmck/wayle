use std::{rc::Rc, sync::Arc};

use wayle_config::{ConfigProperty, ConfigService};
use wayle_systray::{SystemTrayService, core::item::TrayItem};

use crate::shell::bar::dropdowns::OpenSurfaceCoordinator;

pub(crate) struct SystrayInit {
    pub is_vertical: ConfigProperty<bool>,
    pub systray: Arc<SystemTrayService>,
    pub config: Arc<ConfigService>,
    pub coordinator: Rc<OpenSurfaceCoordinator>,
}

#[derive(Debug)]
pub(crate) enum SystrayMsg {}

#[derive(Debug)]
#[allow(clippy::enum_variant_names)]
pub(crate) enum SystrayCmd {
    ItemsChanged(Vec<Arc<TrayItem>>),
    StylingChanged,
    OrientationChanged(bool),
}
