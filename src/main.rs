use std::sync::Arc;

use iced::{Application, Settings};
use parking_lot::Mutex;

use clef::channels::*;
use clef::db;
use clef::platform::db_path;
use clef::ui::{Flags, Ui};

fn main() -> iced::Result {
    pretty_env_logger::init();

    let db_path = db_path().expect("no sqlite db path");
    let db_pool = db::create_pool(&db_path).expect("failed to create db pool");

    let (to_audio_tx, to_audio_rx) = flume::bounded::<AudioAction>(10);
    let (to_ui_tx, to_ui_rx) = flume::bounded::<AudioMessage>(10);

    let audio_handle =
        spawn_player(to_audio_rx, to_ui_tx).expect("failed to start audio thread");

    let inbox = Arc::new(Mutex::new(to_ui_rx));
    let to_audio = Arc::new(Mutex::new(to_audio_tx));
    let flags = Flags { inbox, to_audio, db_pool };

    Ui::run(Settings::with_flags(flags)).map_err(|e| {
        audio_handle.join().ok();
        e
    })
}
