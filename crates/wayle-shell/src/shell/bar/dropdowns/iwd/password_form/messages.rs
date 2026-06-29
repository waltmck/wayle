#[derive(Debug)]
pub(crate) enum PasswordFormInput {
    Show {
        ssid: String,
        security_label: String,
        signal_icon: String,
        error_message: Option<String>,
    },
    ConnectClicked,
    CancelClicked,
    /// Update the displayed signal icon without resetting the entry (used when
    /// the icon config changes while the form is open).
    SetSignalIcon(String),
}

#[derive(Debug)]
pub(crate) enum PasswordFormOutput {
    Connect { password: String },
    Cancel,
}
