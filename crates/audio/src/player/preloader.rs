use std::fs::File;
use std::thread::JoinHandle;

use anyhow::{bail, Context};
use camino::Utf8PathBuf;
use flume::{Receiver, RecvError, Sender};
use symphonia::core::audio::{AsAudioBufferRef, AudioBuffer, AudioBufferRef};
use symphonia::core::codecs::Decoder;
use symphonia::core::formats::{FormatOptions, FormatReader};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::sample::{i24, u24};

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
    pub predecoded_packets: Vec<AnyAudioBuffer>,
}

impl std::fmt::Debug for PreloadedContent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreloadedContent")
            .field("path", &self.path)
            .field("track_info", &self.track_info)
            .field("predecoded_packets", &self.predecoded_packets.len())
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

    let mut reader = probed.format;

    let track = first_supported_track(reader.tracks()).context("no playable track")?;
    let track_info: TrackInfo = track.into();

    // default decode opts (no verify)
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &Default::default())
        .context("making decoder")?;

    // TODO decoding!

    // TODO handle EOF differently if looping
    let Ok(packet) = reader.next_packet() else {
        bail!("failed to read packet");
    };

    let timestamp = packet.ts();

    let decoded = match decoder.decode(&packet) {
        Ok(decoded) => AnyAudioBuffer::from_ref(decoded),
        Err(e) => bail!("failed to decode packet: {e}"),
    };

    let predecoded_packets = vec![decoded];

    Ok(PreloadedContent {
        reader,
        path,
        decoder,
        track_info,
        predecoded_packets,
    })
}

// an owned version of AudioBufferRef, so I don't have to deal with generic lifetimes
#[allow(missing_debug_implementations)]
pub enum AnyAudioBuffer {
    U8(AudioBuffer<u8>),
    U16(AudioBuffer<u16>),
    U24(AudioBuffer<u24>),
    U32(AudioBuffer<u32>),
    S8(AudioBuffer<i8>),
    S16(AudioBuffer<i16>),
    S24(AudioBuffer<i24>),
    S32(AudioBuffer<i32>),
    F32(AudioBuffer<f32>),
    F64(AudioBuffer<f64>),
}

impl AnyAudioBuffer {
    fn from_ref<'a>(buffer_ref: AudioBufferRef<'a>) -> AnyAudioBuffer {
        match buffer_ref {
            AudioBufferRef::U8(sample) => AnyAudioBuffer::U8(sample.into_owned()),
            AudioBufferRef::U16(sample) => AnyAudioBuffer::U16(sample.into_owned()),
            AudioBufferRef::U24(sample) => AnyAudioBuffer::U24(sample.into_owned()),
            AudioBufferRef::U32(sample) => AnyAudioBuffer::U32(sample.into_owned()),
            AudioBufferRef::S8(sample) => AnyAudioBuffer::S8(sample.into_owned()),
            AudioBufferRef::S16(sample) => AnyAudioBuffer::S16(sample.into_owned()),
            AudioBufferRef::S24(sample) => AnyAudioBuffer::S24(sample.into_owned()),
            AudioBufferRef::S32(sample) => AnyAudioBuffer::S32(sample.into_owned()),
            AudioBufferRef::F32(sample) => AnyAudioBuffer::F32(sample.into_owned()),
            AudioBufferRef::F64(sample) => AnyAudioBuffer::F64(sample.into_owned()),
        }
    }
}

impl AsAudioBufferRef for AnyAudioBuffer {
    fn as_audio_buffer_ref(&self) -> AudioBufferRef<'_> {
        match self {
            AnyAudioBuffer::U8(sample) => sample.as_audio_buffer_ref(),
            AnyAudioBuffer::U16(sample) => sample.as_audio_buffer_ref(),
            AnyAudioBuffer::U24(sample) => sample.as_audio_buffer_ref(),
            AnyAudioBuffer::U32(sample) => sample.as_audio_buffer_ref(),
            AnyAudioBuffer::S8(sample) => sample.as_audio_buffer_ref(),
            AnyAudioBuffer::S16(sample) => sample.as_audio_buffer_ref(),
            AnyAudioBuffer::S24(sample) => sample.as_audio_buffer_ref(),
            AnyAudioBuffer::S32(sample) => sample.as_audio_buffer_ref(),
            AnyAudioBuffer::F32(sample) => sample.as_audio_buffer_ref(),
            AnyAudioBuffer::F64(sample) => sample.as_audio_buffer_ref(),
        }
    }
}
