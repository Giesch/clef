use std::sync::atomic::{self, AtomicUsize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SongId(usize);

// Id counter impl taken from iced_native::widget
static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

impl SongId {
    pub fn unique() -> Self {
        let id = NEXT_ID.fetch_add(1, atomic::Ordering::Relaxed);

        Self(id)
    }
}
