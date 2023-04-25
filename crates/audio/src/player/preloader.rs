use std::fs::File;
use std::thread::JoinHandle;

use anyhow::Context;
use camino::Utf8PathBuf;
use flume::{Receiver, RecvError, Sender};
use symphonia::core::codecs::Decoder;
use symphonia::core::formats::{FormatOptions, FormatReader};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use crate::track_info::{first_supported_track, TrackInfo};

pub struct Preloader {
    inbox: Receiver<PreloaderAction>,
    to_player: Sender<PreloaderEffect>,
}

#[derive(Debug)]
pub enum PreloaderAction {
    Load(Utf8PathBuf),
}

#[derive(Debug)]
pub enum PreloaderEffect {
    Loaded(PreloadedContent),
    PreloaderDied,
}

pub struct PreloadedContent {
    pub path: Utf8PathBuf,
    pub reader: Box<dyn FormatReader>,
    pub decoder: Box<dyn Decoder>,
    pub track_info: TrackInfo,
}

impl std::fmt::Debug for PreloadedContent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreloadedContent")
            .field("path", &self.path)
            .field("track_info", &self.track_info)
            .finish()
    }
}

#[derive(thiserror::Error, Debug)]
pub enum PreloaderError {
    #[error("disconnected from main audio thread")]
    Disconnected,

    #[error("unhandled preloader error: {0}")]
    Other(#[from] anyhow::Error),
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
                        PreloaderError::Other(_err) => todo!(),
                    }
                }
            })
    }

    pub fn new(
        inbox: Receiver<PreloaderAction>,
        to_player: Sender<PreloaderEffect>,
    ) -> Self {
        Self { inbox, to_player }
    }

    pub fn run_loop(self) -> Result<(), PreloaderError> {
        let Preloader { inbox, to_player } = self;

        loop {
            let action = match inbox.recv() {
                Ok(action) => action,
                Err(RecvError::Disconnected) => return Err(PreloaderError::Disconnected),
            };

            log::debug!("Got PreloaderAction: {action:#?}");

            let content = match action {
                PreloaderAction::Load(path) => prepare_decoder(path)?,
            };

            to_player.send(PreloaderEffect::Loaded(content)).ok();
        }
    }
}

// TODO, the gap is still audible; this has to do some actual decoding
// TODO see if it's worth sharing this code with the player
fn prepare_decoder(path: Utf8PathBuf) -> anyhow::Result<PreloadedContent> {
    let mut hint = Hint::new();

    if let Some(extension) = path.extension() {
        hint.with_extension(extension);
    }

    let file = File::open(&path).with_context(|| format!("file not found: {}", &path))?;

    let source = Box::new(file);

    let mss = MediaSourceStream::new(source, Default::default());

    let format_opts = FormatOptions {
        enable_gapless: true,
        ..Default::default()
    };

    let metadata_opts: MetadataOptions = Default::default();

    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &format_opts, &metadata_opts)
        .context("The input was not supported by any format reader")?;

    let track =
        first_supported_track(probed.format.tracks()).context("no playable track")?;
    let track_info: TrackInfo = track.into();

    // default decode opts (no verify)
    let decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &Default::default())
        .context("making decoder")?;

    Ok(PreloadedContent {
        reader: probed.format,
        path,
        decoder,
        track_info,
    })
}
