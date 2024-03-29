use std::collections::VecDeque;
use std::fs::File;
use std::thread::JoinHandle;

use anyhow::{bail, Context};
use camino::Utf8PathBuf;
use flume::{Receiver, Sender};
use log::{error, trace};
use symphonia::core::audio::{AsAudioBufferRef, AudioBuffer, AudioBufferRef};
use symphonia::core::codecs::Decoder;
use symphonia::core::formats::{FormatOptions, FormatReader};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::sample::{i24, u24};

use crate::track_info::{first_supported_track, TrackInfo};

#[allow(unused)]
#[cfg(not(target_os = "linux"))]
use super::device_config::CpalDeviceConfig;

pub struct Preloader {
    inbox: Receiver<PreloaderAction>,
    to_player: Sender<PreloaderEffect>,

    #[allow(unused)]
    #[cfg(not(target_os = "linux"))]
    device_config: CpalDeviceConfig,
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
    pub predecoded_packets: VecDeque<PredecodedPacket>,
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

pub struct PredecodedPacket {
    pub timestamp: u64,
    pub decoded: AnyAudioBuffer,
}

impl std::fmt::Debug for PredecodedPacket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PredecodedPacket")
            .field("timestamp", &self.timestamp)
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
                #[allow(unused)]
                #[cfg(not(target_os = "linux"))]
                let device_config = CpalDeviceConfig::get_default()
                    .expect("failed to get default device config");

                let preloader = Self::new(
                    inbox,
                    to_player.clone(),
                    #[allow(unused)]
                    #[cfg(not(target_os = "linux"))]
                    device_config,
                );

                if let Err(err) = preloader.run_loop() {
                    to_player.send(PreloaderEffect::PreloaderDied).ok();

                    match err {
                        PreloaderError::Disconnected => error!("preloader disconnected"),
                        PreloaderError::Other(e) => {
                            error!("preload error: {e}")
                        }
                    }
                }
            })
    }

    pub fn new(
        inbox: Receiver<PreloaderAction>,
        to_player: Sender<PreloaderEffect>,

        #[allow(unused)]
        #[cfg(not(target_os = "linux"))]
        device_config: CpalDeviceConfig,
    ) -> Self {
        #[allow(unused)]
        #[cfg(not(target_os = "linux"))]
        let new = Self { inbox, to_player, device_config };

        #[allow(unused)]
        #[cfg(target_os = "linux")]
        let new = Self { inbox, to_player };

        new
    }

    pub fn run_loop(self) -> Result<(), PreloaderError> {
        #[allow(unused)]
        #[cfg(target_os = "linux")]
        let Preloader { inbox, to_player } = self;

        #[allow(unused)]
        #[cfg(not(target_os = "linux"))]
        let Preloader { inbox, to_player, device_config } = self;

        loop {
            let action = inbox.recv().map_err(|_| PreloaderError::Disconnected)?;

            trace!("Got PreloaderAction: {action:#?}");

            let content = match action {
                PreloaderAction::Load(path) => preload(path)?,
            };

            trace!("Finished preload");

            to_player.send(PreloaderEffect::Loaded(content)).ok();
        }
    }
}

// TODO see if it's worth sharing this code with the player
fn preload(path: Utf8PathBuf) -> anyhow::Result<PreloadedContent> {
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

    let mut predecoded_packets = VecDeque::new();
    loop {
        // TODO handle EOF
        let Ok(packet) = reader.next_packet() else {
            bail!("failed to read packet");
        };

        let timestamp = packet.ts();

        let decoded = match decoder.decode(&packet) {
            Ok(decoded) => AnyAudioBuffer::from_ref(decoded),
            Err(e) => bail!("failed to decode packet: {e}"),
        };

        predecoded_packets.push_back(PredecodedPacket { timestamp, decoded });

        let Some(progress) = track_info.progress_times(timestamp) else {
            bail!("missing track info");
        };

        if progress.elapsed.seconds > 2 {
            break;
        }
    }

    Ok(PreloadedContent {
        reader,
        path,
        decoder,
        track_info,
        predecoded_packets,
    })
}

// an owned version of AudioBufferRef,
// so I don't have to deal with generic lifetimes
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
    pub fn from_ref(buffer_ref: AudioBufferRef<'_>) -> AnyAudioBuffer {
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

    pub fn spec(&self) -> &symphonia::core::audio::SignalSpec {
        match self {
            AnyAudioBuffer::U8(sample) => sample.spec(),
            AnyAudioBuffer::U16(sample) => sample.spec(),
            AnyAudioBuffer::U24(sample) => sample.spec(),
            AnyAudioBuffer::U32(sample) => sample.spec(),
            AnyAudioBuffer::S8(sample) => sample.spec(),
            AnyAudioBuffer::S16(sample) => sample.spec(),
            AnyAudioBuffer::S24(sample) => sample.spec(),
            AnyAudioBuffer::S32(sample) => sample.spec(),
            AnyAudioBuffer::F32(sample) => sample.spec(),
            AnyAudioBuffer::F64(sample) => sample.spec(),
        }
    }

    pub fn capacity(&self) -> usize {
        match self {
            AnyAudioBuffer::U8(sample) => sample.capacity(),
            AnyAudioBuffer::U16(sample) => sample.capacity(),
            AnyAudioBuffer::U24(sample) => sample.capacity(),
            AnyAudioBuffer::U32(sample) => sample.capacity(),
            AnyAudioBuffer::S8(sample) => sample.capacity(),
            AnyAudioBuffer::S16(sample) => sample.capacity(),
            AnyAudioBuffer::S24(sample) => sample.capacity(),
            AnyAudioBuffer::S32(sample) => sample.capacity(),
            AnyAudioBuffer::F32(sample) => sample.capacity(),
            AnyAudioBuffer::F64(sample) => sample.capacity(),
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
