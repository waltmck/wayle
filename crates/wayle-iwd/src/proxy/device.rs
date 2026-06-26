//! IWD Device interface (`net.connman.iwd.Device`).

use zbus::proxy;

#[proxy(
    default_service = "net.connman.iwd",
    interface = "net.connman.iwd.Device"
)]
pub(crate) trait Device {
    /// Whether the device is powered on (the WiFi enable toggle).
    #[zbus(property)]
    fn powered(&self) -> zbus::Result<bool>;

    /// Set the device's powered state.
    #[zbus(property)]
    fn set_powered(&self, value: bool) -> zbus::Result<()>;
}
