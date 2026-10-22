use std::hash::{Hash, Hasher};

use iced::futures::{future, stream, Future, FutureExt, StreamExt};
use iced::Subscription;

/// Hash only the worker identity; rebuilding a view must not restart its stream.
struct Worker<I, T, F> {
    id: I,
    initial: fn() -> T,
    step: F,
}

impl<I: Hash, T, F> Hash for Worker<I, T, F> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

pub fn old_unfold<I, T, F, Fut, Message>(
    id: I,
    initial: fn() -> T,
    step: F,
) -> Subscription<Message>
where
    I: Hash + 'static,
    T: Send + 'static,
    F: FnMut(T) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = (Option<Message>, T)> + Send + 'static,
    Message: Send + 'static,
{
    Subscription::run_with(Worker { id, initial, step }, |worker| {
        let mut step = worker.step.clone();

        stream::unfold((worker.initial)(), move |state| step(state).map(Some))
            .filter_map(future::ready)
    })
}
