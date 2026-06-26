use zbus::zvariant::OwnedObjectPath;

/// IWD service errors.
#[derive(thiserror::Error, Debug)]
pub enum Error {
    /// D-Bus communication error.
    #[error("dbus operation failed: {0}")]
    DbusError(#[from] zbus::Error),

    /// Service initialization failed (used for top-level service startup).
    #[error("cannot initialize iwd service: {0}")]
    ServiceInitializationFailed(String),

    /// Object not found at the specified D-Bus path.
    #[error("object not found at path: {0}")]
    ObjectNotFound(OwnedObjectPath),

    /// A network operation failed.
    #[error("cannot {operation}")]
    OperationFailed {
        /// The operation that failed.
        operation: &'static str,
        /// Underlying error that caused the failure.
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// Monitoring requires a cancellation token.
    #[error("cannot start monitoring: cancellation token not provided")]
    MissingCancellationToken,
}
