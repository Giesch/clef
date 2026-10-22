use std::io::Cursor;

use clef_audio::metadata::{decode_metadata, TagKey};
use symphonia::core::{
    formats::{probe::Hint, FormatReader},
    io::MediaSourceStream,
};

fn reader(bytes: Vec<u8>, extension: &str) -> Box<dyn FormatReader> {
    let mut hint = Hint::new();
    hint.with_extension(extension);
    let source = MediaSourceStream::new(Box::new(Cursor::new(bytes)), Default::default());
    symphonia::default::get_probe()
        .probe(&hint, source, Default::default(), Default::default())
        .unwrap()
}

fn decode(bytes: Vec<u8>, extension: &str) -> symphonia::core::errors::Result<Vec<f32>> {
    let mut reader = reader(bytes, extension);
    let track = clef_audio::track_info::first_supported_track(reader.tracks()).unwrap();
    let id = track.id;
    let params = track.codec_params.as_ref().unwrap().audio().unwrap();
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(params, &Default::default())
        .unwrap();
    let mut result = Vec::new();
    while let Some(packet) = reader.next_packet()? {
        if packet.track_id != id {
            continue;
        }
        let decoded = decoder.decode(&packet)?;
        let mut samples: Vec<f32> = Vec::new();
        decoded.copy_to_vec_interleaved(&mut samples);
        result.extend(samples);
    }
    assert!(
        reader.next_packet()?.is_none(),
        "EOF must remain normal on repeated reads"
    );
    Ok(result)
}

#[test]
fn decode_wav_samples_and_eof() {
    let samples = decode(include_bytes!("fixtures/stereo.wav").to_vec(), "wav").unwrap();
    assert_eq!(samples.len(), 44_100 * 2);
    for frame in [1, 25, 100, 1000, 44_099] {
        let time = frame as f32 / 44_100.0;
        let left = 0.25 * (std::f32::consts::TAU * 440.0 * time).sin();
        let right = 0.125 * (std::f32::consts::TAU * 880.0 * time).sin();
        assert!((samples[frame * 2] - left).abs() < 0.0001);
        assert!((samples[frame * 2 + 1] - right).abs() < 0.0001);
    }
}

#[test]
fn decode_tagged_mp3_metadata_and_duration() {
    let path = camino::Utf8Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/tagged.mp3"
    ));
    let metadata = decode_metadata(path).unwrap();
    assert_eq!(metadata.tags[&TagKey::TrackTitle], "Synthetic stereo");
    assert_eq!(metadata.tags[&TagKey::Artist], "Clef tests");
    assert_eq!(metadata.tags[&TagKey::Album], "Regression fixtures");
    assert_eq!(metadata.total_seconds, 1);
    assert_eq!(metadata.tags[&TagKey::TrackNumber], "3");
    assert_eq!(metadata.tags[&TagKey::TrackTotal], "7");
    let mut reader = reader(include_bytes!("fixtures/tagged.mp3").to_vec(), "mp3");
    let metadata = reader.metadata();
    let revision = metadata.current().unwrap();
    assert_eq!(revision.media.visuals.len(), 1);
    assert_eq!(
        revision.media.visuals[0].data.as_ref(),
        include_bytes!("fixtures/cover.png")
    );
}

#[test]
fn mp3_gapless_frame_count() {
    let samples = decode(include_bytes!("fixtures/tagged.mp3").to_vec(), "mp3").unwrap();
    assert_eq!(
        samples.len(),
        44_100 * 2,
        "encoder delay and padding must not become audible frames"
    );
    let frames = samples.as_chunks::<2>().0;
    let left_energy: f32 = frames.iter().map(|frame| frame[0] * frame[0]).sum();
    let right_energy: f32 = frames.iter().map(|frame| frame[1] * frame[1]).sum();
    assert!(left_energy > right_energy * 3.0);
}

#[test]
fn truncated_media_reports_error() {
    let mut bytes = include_bytes!("fixtures/stereo.wav").to_vec();
    bytes.truncate(bytes.len() - 127);
    assert!(
        decode(bytes, "wav").is_err(),
        "a partial PCM frame must not be normal EOF"
    );
}
