use std::sync::Arc;

use iced::{Application, Settings};
use parking_lot::Mutex;

use clef::channels::*;
use clef::ui::{Flags, Ui};

fn main() -> iced::Result {
    pretty_env_logger::init();

    let (to_audio_tx, to_audio_rx) = flume::bounded::<ToAudio>(10);
    let (to_ui_tx, to_ui_rx) = flume::bounded::<ToUi>(10);

    spawn_player(to_audio_rx, to_ui_tx).expect("failed to start audio thread");

    let flags = Flags {
        inbox: Arc::new(Mutex::new(to_ui_rx)),
        to_audio: Arc::new(Mutex::new(to_audio_tx)),
    };

    Ui::run(Settings::with_flags(flags))
}
