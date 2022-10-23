#![warn(rust_2018_idioms)]
#![forbid(unsafe_code)]

use flume::{Receiver, Sender, TryRecvError};
use iced::{Application, Settings};

use clef::channels::*;
use clef::ui::{Flags, Ui};

fn main() -> iced::Result {
    let (to_audio_tx, to_audio_rx) = flume::bounded::<ToAudio>(1);
    let (to_ui_tx, to_ui_rx) = flume::bounded::<ToUi>(1);

    spawn_audio_thread(to_audio_rx, to_ui_tx);

    let flags = Flags {
        inbox: to_ui_rx,
        to_audio: to_audio_tx,
    };

    Ui::run(Settings::with_flags(flags))
}
