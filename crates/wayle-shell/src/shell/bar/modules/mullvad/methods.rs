use std::sync::Arc;

use relm4::ComponentController;
use wayle_config::schemas::modules::MullvadConfig;
use wayle_mullvad::{ConnectionState, MullvadService};
use wayle_widgets::prelude::BarButtonInput;

use super::{
    MullvadModule,
    helpers::{MullvadContext, format_label, select_icon},
};

impl MullvadModule {
    pub(super) fn compute_display(
        config: &MullvadConfig,
        mullvad: &Option<Arc<MullvadService>>,
    ) -> (String, String) {
        let Some(mullvad) = mullvad else {
            let ctx = MullvadContext {
                logged_in: false,
                state: ConnectionState::Disconnected,
                network: None,
            };
            return (select_icon(config, &ctx), format_label(&ctx));
        };

        let ctx = MullvadContext {
            logged_in: mullvad.mullvad.logged_in.get(),
            state: mullvad.mullvad.connection_state.get(),
            network: mullvad.mullvad.connected_network.get(),
        };
        (select_icon(config, &ctx), format_label(&ctx))
    }

    pub(super) fn update_display(
        &self,
        config: &MullvadConfig,
        mullvad: &Option<Arc<MullvadService>>,
    ) {
        let (icon, label) = Self::compute_display(config, mullvad);
        self.bar_button.emit(BarButtonInput::SetIcon(icon));
        self.bar_button.emit(BarButtonInput::SetLabel(label));
    }
}
