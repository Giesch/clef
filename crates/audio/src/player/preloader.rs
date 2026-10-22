use std::collections::VecDeque;
use std::fs::File;
use std::thread::JoinHandle;

use anyhow::Context;
use camino::Utf8PathBuf;
use flume::{Receiver, Sender};
use log::{error, trace};
use symphonia::core::audio::{GenericAudioBuffer, GenericAudioBufferRef};
use symphonia::core::codecs::audio::AudioDecoder;
use symphonia::core::formats::{probe::Hint, FormatOptions, FormatReader};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::units::Timestamp;

use crate::track_info::{first_supported_track, TrackInfo};

#[derive(Debug)]
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
    pub decoder: Box<dyn AudioDecoder>,
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
    pub timestamp: Timestamp,
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
    ) -> std::io::Result<JoinHandle<()>> {
        std::thread::Builder::new()
            .name("ClefAudioPreloader".to_owned())
            .spawn(move || {
                let preloader = Self::new(inbox, to_player.clone());
                if let Err(err) = preloader.run_loop() {
                    error!("preload error: {err}");
                    to_player.send(PreloaderEffect::PreloaderDied).ok();
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
        loop {
            let action = self
                .inbox
                .recv()
                .map_err(|_| PreloaderError::Disconnected)?;
            let PreloaderAction::Load(path) = action;
            trace!("preloading {path}");
            self.to_player
                .send(PreloaderEffect::Loaded(preload(path)?))
                .ok();
        }
    }
}

pub(super) fn preload(path: Utf8PathBuf) -> anyhow::Result<PreloadedContent> {
    let mut hint = Hint::new();
    if let Some(extension) = path.extension() {
        hint.with_extension(extension);
    }
    let file = File::open(&path).with_context(|| format!("file not found: {path}"))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut reader = symphonia::default::get_probe()
        .probe(&hint, mss, FormatOptions::default(), Default::default())
        .context("The input was not supported by any format reader")?;
    let track = first_supported_track(reader.tracks()).context("no playable track")?;
    let track_info: TrackInfo = track.into();
    let params = track
        .codec_params
        .as_ref()
        .and_then(|params| params.audio())
        .context("missing audio codec parameters")?;
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(params, &Default::default())
        .context("making decoder")?;
    let mut predecoded_packets = VecDeque::new();
    let mut budget = PreloadBudget::default();
    while let Some(packet) = reader.next_packet().context("reading preload packet")? {
        if packet.track_id != track_info.id {
            continue;
        }
        let timestamp = packet.pts;
        let decoded =
            owned_buffer(decoder.decode(&packet).context("decoding preload packet")?);
        let ready = budget.add(decoded.frames(), decoded.spec().rate());
        predecoded_packets.push_back(PredecodedPacket { timestamp, decoded });
        if ready {
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

#[derive(Default)]
struct PreloadBudget(f64);

impl PreloadBudget {
    fn add(&mut self, frames: usize, rate: u32) -> bool {
        self.0 += frames as f64 / f64::from(rate);

        self.0 >= 3.0
    }
}

pub type AnyAudioBuffer = GenericAudioBuffer;

fn owned_buffer(buffer: GenericAudioBufferRef<'_>) -> GenericAudioBuffer {
    match buffer {
        GenericAudioBufferRef::U8(buffer) => GenericAudioBuffer::U8(buffer.clone()),
        GenericAudioBufferRef::U16(buffer) => GenericAudioBuffer::U16(buffer.clone()),
        GenericAudioBufferRef::U24(buffer) => GenericAudioBuffer::U24(buffer.clone()),
        GenericAudioBufferRef::U32(buffer) => GenericAudioBuffer::U32(buffer.clone()),
        GenericAudioBufferRef::S8(buffer) => GenericAudioBuffer::S8(buffer.clone()),
        GenericAudioBufferRef::S16(buffer) => GenericAudioBuffer::S16(buffer.clone()),
        GenericAudioBufferRef::S24(buffer) => GenericAudioBuffer::S24(buffer.clone()),
        GenericAudioBufferRef::S32(buffer) => GenericAudioBuffer::S32(buffer.clone()),
        GenericAudioBufferRef::F32(buffer) => GenericAudioBuffer::F32(buffer.clone()),
        GenericAudioBufferRef::F64(buffer) => GenericAudioBuffer::F64(buffer.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preload_budget_does_not_require_track_duration() {
        let mut budget = PreloadBudget::default();
        assert!(!budget.add(44_100, 44_100));
        assert!(!budget.add(48_000, 48_000));
        assert!(budget.add(44_100, 44_100));
    }
}
