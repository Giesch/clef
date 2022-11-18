use std::sync::Arc;

use clef::audio::player::{AudioAction, AudioMessage, Player};
use clef::db::run_migrations;
use iced::{Application, Settings};
use parking_lot::Mutex;

use clef::app::config::Config;
use clef::app::{App, Flags};
use clef::db;
use souvlaki::{MediaControlEvent, MediaControls, PlatformConfig};

fn main() -> iced::Result {
    pretty_env_logger::init();

    //////

    let config = PlatformConfig {
        dbus_name: "clef.player",
        display_name: "Clef",
        hwnd: None, // required for windows support
    };

    let mut media_controls =
        MediaControls::new(config).expect("failed to create media controls");

    let (from_controls_tx, from_controls_rx) = flume::unbounded::<MediaControlEvent>();

    // FIXME does just creating them here add the 'unknown song'?
    media_controls
        .attach(move |e: MediaControlEvent| {
            from_controls_tx
                .send(e)
                .map_err(|e| log::error!("failed to send media control event: {e:?}"))
                .ok();
        })
        .expect("failed to set up listening to media controls");

    let media_controls = Arc::new(Mutex::new(media_controls));

    //////

    let config = Config::init().expect("unable to build config");
    let db_pool = db::create_pool(&config.db_path).expect("failed to create db pool");

    run_migrations(&db_pool).expect("failed to run migrations");

    let (to_audio_tx, to_audio_rx) = flume::bounded::<AudioAction>(10);
    let (to_ui_tx, to_ui_rx) = flume::bounded::<AudioMessage>(10);

    let audio_handle =
        Player::spawn(to_audio_rx, to_ui_tx).expect("failed to start audio thread");

    let inbox = Arc::new(Mutex::new(to_ui_rx));
    let to_audio = Arc::new(Mutex::new(to_audio_tx));
    let from_controls = Arc::new(Mutex::new(from_controls_rx));
    let flags = Flags {
        inbox,
        to_audio,
        db_pool,
        config,
        media_controls,
        from_controls,
    };

    App::run(Settings::with_flags(flags)).map_err(|e| {
        audio_handle.join().ok();
        e
    })
}
