//! Hand-written zbus proxies for the `net.connman.iwd` D-Bus interfaces.

/// Agent manager proxy (`net.connman.iwd.AgentManager`).
pub mod agent_manager;
/// Per-station diagnostics proxy (`net.connman.iwd.StationDiagnostic`).
pub mod diagnostic;
/// Device proxy (`net.connman.iwd.Device`).
pub mod device;
/// Known network proxy (`net.connman.iwd.KnownNetwork`).
pub mod known_network;
/// Network proxy (`net.connman.iwd.Network`).
pub mod network;
/// `org.freedesktop.DBus.ObjectManager` proxy scoped to `net.connman.iwd`.
pub mod object_manager;
/// Station proxy (`net.connman.iwd.Station`).
pub mod station;
