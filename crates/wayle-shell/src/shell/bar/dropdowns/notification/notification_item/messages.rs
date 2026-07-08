use std::sync::Arc;

use wayle_config::schemas::modules::notification::IconSource;
use wayle_notification::core::notification::Notification;

use crate::shell::notification_popup::helpers::ResolvedIcon;

pub(crate) struct NotificationItemInit {
    pub notification: Arc<Notification>,
    pub resolved_icon: ResolvedIcon,
    pub icon_source: IconSource,
    pub symbolic_fallback: bool,
}

#[derive(Debug)]
pub(crate) enum NotificationItemInput {
    RefreshTime,
    /// The underlying notification's content/actions changed; re-render in place.
    Refresh,
}

#[derive(Debug)]
pub(crate) enum NotificationItemOutput {
    Dismissed(u32),
}
