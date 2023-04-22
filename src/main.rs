use clef::audio::player::{AudioAction, AudioMessage, Player};
use clef::db::run_migrations;
use clef::logging;
use iced::{Application, Settings};
use iced_native::window::Icon;

use clef::app::config::Config;
use clef::app::{App, Flags};
use clef::db;

fn main() -> iced::Result {
    logging::init();

    let config = Config::init().expect("unable to build config");

    let db_pool = db::create_pool(&config.db_path).expect("failed to create db pool");

    run_migrations(&db_pool).expect("failed to run migrations");

    let (to_audio_tx, to_audio_rx) = flume::unbounded::<AudioAction>();
    let (to_ui_tx, to_ui_rx) = flume::unbounded::<AudioMessage>();

    Player::spawn(to_audio_rx, to_ui_tx, to_audio_tx.clone())
        .expect("failed to start audio thread");

    let flags = Flags {
        inbox: to_ui_rx,
        to_audio: to_audio_tx,
        db_pool,
        config,
    };

    let mut settings = Settings::with_flags(flags);
    settings.window = iced::window::Settings {
        icon: get_icon(),
        ..Default::default()
    };

    App::run(settings)
}

fn get_icon() -> Option<Icon> {
    iced::window::icon::from_file("./base_clef.ico").ok()
}
