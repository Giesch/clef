use iced::{Application, Settings};
use iced_native::window::Icon;

use clef_audio::player::{AudioAction, AudioMessage, Player};
use clef_db as db;
use clef_db::run_migrations;

use clef::logging;
use clef_ui::app::config::Config;
use clef_ui::app::{App, Flags};

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
    #[cfg(target_os = "windows")]
    let icon = {
        use iced::window::icon;
        use image_rs::ImageFormat;

        let bytes = include_bytes!("../base_clef.jpg");
        icon::from_file_data(bytes, Some(ImageFormat::Jpeg)).ok()
    };

    // making this look ok at a larger size is going to take more work
    #[cfg(not(target_os = "windows"))]
    let icon = None;

    icon
}
