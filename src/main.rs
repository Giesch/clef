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

    let config = Config::init().expect("unable to build config");
    let db_pool = db::create_pool(&config.db_path).expect("failed to create db pool");

    run_migrations(&db_pool).expect("failed to run migrations");

    let (to_audio_tx, to_audio_rx) = flume::unbounded::<AudioAction>();
    let (to_ui_tx, to_ui_rx) = flume::unbounded::<AudioMessage>();

    //////

    let controls_config = PlatformConfig {
        dbus_name: "clef.player",
        display_name: "Clef",
        hwnd: None, // required for windows support
    };

    let mut media_controls =
        MediaControls::new(controls_config).expect("failed to create media controls");

    let controls_to_audio = to_audio_tx.clone();
    media_controls
        .attach(move |e: MediaControlEvent| {
            let action: Option<AudioAction> = match &e {
                MediaControlEvent::Play => Some(AudioAction::PlayPaused),
                MediaControlEvent::Pause => Some(AudioAction::Pause),
                MediaControlEvent::Next => Some(AudioAction::Forward),
                MediaControlEvent::Previous => Some(AudioAction::Back),
                MediaControlEvent::Toggle => Some(AudioAction::Toggle),

                MediaControlEvent::Stop => None,
                MediaControlEvent::Seek(_) => None,
                MediaControlEvent::SeekBy(_, _) => None,
                MediaControlEvent::SetPosition(_) => None,
                MediaControlEvent::OpenUri(_) => None,
                MediaControlEvent::Raise => None,
                MediaControlEvent::Quit => None,
            };

            if let Some(action) = action {
                controls_to_audio
                    .send(action)
                    .map_err(|e| {
                        log::error!("failed to send from controls to audio: {e:?}")
                    })
                    .ok();
            } else {
                log::info!("unsupported media controls action: {e:?}");
            }
        })
        .expect("failed to set up listening to media controls");

    let media_controls = Arc::new(Mutex::new(media_controls));

    //////

    let audio_handle = Player::spawn(to_audio_rx, to_ui_tx, media_controls.clone())
        .expect("failed to start audio thread");

    let inbox = Arc::new(Mutex::new(to_ui_rx));
    let to_audio = Arc::new(Mutex::new(to_audio_tx));
    let flags = Flags { inbox, to_audio, db_pool, config };

    App::run(Settings::with_flags(flags)).map_err(|e| {
        audio_handle.join().ok();
        e
    })
}
