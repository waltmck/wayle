use std::sync::Arc;

use futures::{StreamExt, stream::select_all};
use relm4::prelude::FactorySender;
use tokio_util::sync::CancellationToken;
use wayle_notification::core::notification::Notification;

use super::{NotificationItem, messages::NotificationItemInput};

/// Reactively forwards any change to the notification's displayed fields as a single
/// `Refresh` input, so the item re-renders in place (the `Arc<Notification>` is stable
/// for the notification's lifetime, so this stays valid across `replaces_id` updates).
pub(super) fn spawn_field_watcher(
    sender: &FactorySender<NotificationItem>,
    notification: &Arc<Notification>,
    cancel_token: CancellationToken,
) {
    let streams = vec![
        notification.summary.watch().skip(1).map(|_| ()).boxed(),
        notification.body.watch().skip(1).map(|_| ()).boxed(),
        notification.actions.watch().skip(1).map(|_| ()).boxed(),
        notification.default_action.watch().skip(1).map(|_| ()).boxed(),
        notification.urgency.watch().skip(1).map(|_| ()).boxed(),
        notification.app_icon.watch().skip(1).map(|_| ()).boxed(),
        notification.image_path.watch().skip(1).map(|_| ()).boxed(),
        notification.hints.watch().skip(1).map(|_| ()).boxed(),
        notification.desktop_entry.watch().skip(1).map(|_| ()).boxed(),
    ];
    let stream = select_all(streams);
    let sender = sender.clone();

    relm4::spawn_local(async move {
        futures::pin_mut!(stream);

        loop {
            tokio::select! {
                () = cancel_token.cancelled() => break,
                result = stream.next() => {
                    if result.is_none() {
                        break;
                    }
                    sender.input(NotificationItemInput::Refresh);
                }
            }
        }
    });
}
