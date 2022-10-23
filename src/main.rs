#![warn(rust_2018_idioms)]
#![forbid(unsafe_code)]

use std::sync::Arc;

use iced::{Application, Settings};
use parking_lot::Mutex;

use clef::channels::*;
use clef::ui::{Flags, Ui};

fn main() -> iced::Result {
    pretty_env_logger::init();

    let (to_audio_tx, to_audio_rx) = flume::bounded::<ToAudio>(1);
    let (to_ui_tx, to_ui_rx) = flume::bounded::<ToUi>(1);

    spawn_player(to_audio_rx, to_ui_tx);

    let flags = Flags {
        inbox: Arc::new(Mutex::new(to_ui_rx)),
        to_audio: Arc::new(Mutex::new(to_audio_tx)),
    };

    Ui::run(Settings::with_flags(flags))
}
