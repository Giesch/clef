use clef_audio::player::{AudioAction, AudioMessage, Player};
use clef_ui::Flags;

use clef::config;
use clef::logging;

fn main() -> anyhow::Result<()> {
    logging::init();

    let config = config::init().expect("unable to build config");

    let db_pool =
        clef_db::create_pool(&config.db_path).expect("failed to create db pool");

    clef_db::run_migrations(&db_pool).expect("failed to run migrations");

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

    clef_ui::setup::launch(flags)
}
