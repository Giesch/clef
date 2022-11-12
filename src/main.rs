use std::sync::Arc;

use directories::{ProjectDirs, UserDirs};
use iced::{Application, Settings};
use parking_lot::Mutex;

use clef::channels::*;
use clef::ui::{Flags, Ui};

fn main() -> iced::Result {
    pretty_env_logger::init();

    let project_dirs = ProjectDirs::from("dev.clef", "Giesch", "Clef")
        .expect("no project directory path for app found");
    let user_dirs = UserDirs::new().expect("no user directories found");

    let (to_audio_tx, to_audio_rx) = flume::bounded::<ToAudio>(10);
    let (to_ui_tx, to_ui_rx) = flume::bounded::<ToUi>(10);

    spawn_player(to_audio_rx, to_ui_tx).expect("failed to start audio thread");

    let inbox = Arc::new(Mutex::new(to_ui_rx));
    let to_audio = Arc::new(Mutex::new(to_audio_tx));
    let flags = Flags {
        user_dirs,
        project_dirs,
        inbox,
        to_audio,
    };

    Ui::run(Settings::with_flags(flags))
}
