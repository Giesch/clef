use std::thread::JoinHandle;

use camino::Utf8PathBuf;
use flume::{Receiver, RecvError, Sender};

pub struct Preloader {
    state: PreloaderState,
    inbox: Receiver<PreloaderAction>,
    to_player: Sender<PreloaderEffect>,
}

#[derive(Default, Debug)]
enum PreloaderState {
    #[default]
    Ready,
}

#[derive(Debug)]
pub enum PreloaderAction {
    Load(Utf8PathBuf),
}

#[derive(Debug)]
pub enum PreloaderEffect {
    Loaded { path: Utf8PathBuf },
    PreloaderDied,
}

#[derive(thiserror::Error, Debug)]
pub enum PreloaderError {
    #[error("disconnected from main audio thread")]
    Disconnected,
}

impl Preloader {
    pub fn spawn(
        inbox: Receiver<PreloaderAction>,
        to_player: Sender<PreloaderEffect>,
    ) -> std::result::Result<JoinHandle<()>, std::io::Error> {
        std::thread::Builder::new()
            .name("ClefAudioPreloader".to_string())
            .spawn(move || {
                let preloader = Self::new(inbox, to_player.clone());

                if let Err(err) = preloader.run_loop() {
                    to_player.send(PreloaderEffect::PreloaderDied).ok();

                    match err {
                        PreloaderError::Disconnected => todo!(),
                    }
                }
            })
    }

    pub fn new(
        inbox: Receiver<PreloaderAction>,
        to_player: Sender<PreloaderEffect>,
    ) -> Self {
        Self {
            state: Default::default(),
            inbox,
            to_player,
        }
    }

    pub fn run_loop(self) -> Result<(), PreloaderError> {
        let Preloader { state, inbox, to_player } = self;

        loop {
            // TODO use try_recv while preloading, so that work can continue
            let action = match inbox.recv() {
                Ok(action) => Some(action),
                Err(RecvError::Disconnected) => return Err(PreloaderError::Disconnected),
            };

            let Some(action) = action else {
                continue;
            };

            log::debug!("Got PreloaderAction: {action:#?}");
        }
    }
}
