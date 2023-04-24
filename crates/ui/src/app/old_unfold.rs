use std::hash::Hash;

use iced::futures::Future;
use iced_native::Subscription;

/// A copy of the old version of iced::subscription::unfold, that used filter_map
/// This allows for using flume without spamming the app with no-op messages.
/// It could be replaced with a custom flume channel helper, based on iced::subscription::channel.
///   But getting iced streams to hook up to a flume Selector is a bit weird.
/// It's probably better to just use iced streams for non-audio stuff
pub fn old_unfold<I, T, Fut, Message>(
    id: I,
    initial: T,
    mut f: impl FnMut(T) -> Fut + Send + Sync + 'static,
) -> Subscription<Message>
where
    I: Hash + 'static,
    T: Send + 'static,
    Fut: Future<Output = (Option<Message>, T)> + Send + 'static,
    Message: 'static + Send,
{
    use iced::futures::future::{self, FutureExt};
    use iced::futures::stream::StreamExt;

    iced_native::subscription::run_with_id(
        id,
        iced::futures::stream::unfold(initial, move |state| f(state).map(Some))
            .filter_map(future::ready),
    )
}
