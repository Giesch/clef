use super::*;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Default)]
struct Recording {
    samples: Vec<f32>,
    flushes: usize,
}
struct RecordingOutput(Rc<RefCell<Recording>>);
impl AudioOutput for RecordingOutput {
    fn write(&mut self, decoded: GenericAudioBufferRef<'_>) -> output::Result<()> {
        let mut samples: Vec<f32> = Vec::new();
        decoded.copy_to_vec_interleaved(&mut samples);
        self.0.borrow_mut().samples.extend(samples);
        Ok(())
    }
    fn flush(&mut self) {
        self.0.borrow_mut().flushes += 1;
    }
}
fn song(name: &str, id: i32) -> QueuedSong {
    QueuedSong {
        id: SongId::new(id),
        path: Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name),
        title: None,
        artist: None,
        album_title: None,
        resized_art: None,
        duration: None,
    }
}
fn queue(current: QueuedSong) -> Queue<QueuedSong> {
    Queue {
        current,
        next: Default::default(),
        previous: Default::default(),
    }
}
fn attach(state: &mut PlayerState, recording: &Rc<RefCell<Recording>>) {
    state.audio_output = Some(Box::new(RecordingOutput(recording.clone())));
}
fn assert_samples(samples: &[f32], start: usize) {
    for (index, frame) in samples.as_chunks::<2>().0.iter().enumerate().step_by(137) {
        let time = (start + index) as f64 / 44_100.0;
        let left = 0.25 * (std::f64::consts::TAU * 440.0 * time).sin();
        let right = 0.125 * (std::f64::consts::TAU * 880.0 * time).sin();
        assert!((f64::from(frame[0]) - left).abs() < 0.00004);
        assert!((f64::from(frame[1]) - right).abs() < 0.00004);
    }
}
#[test]
fn seek_and_preload_handoff() {
    let first = song("stereo.wav", 1);
    let second = song("long-stereo.wav", 2);
    let preloaded = preloader::preload(second.path.clone()).unwrap();
    let cached_frames: usize = preloaded
        .predecoded_packets
        .iter()
        .map(|packet| packet.decoded.frames())
        .sum();
    assert!((132_300..176_400).contains(&cached_frames));
    let mut queued = queue(first);
    queued.next.push_back(second);
    let mut state = PlayerState::play_queue(queued).unwrap();
    state.preloaded_content = Some(preloaded);
    let first_recording = Rc::new(RefCell::new(Recording::default()));
    let second_recording = Rc::new(RefCell::new(Recording::default()));
    attach(&mut state, &first_recording);
    while state.queue.current.id == SongId::new(1) {
        state = Player::step(Some(state), None)
            .unwrap()
            .player_state
            .unwrap();
    }
    assert_eq!(first_recording.borrow().samples.len(), 44_100 * 2);
    assert_eq!(first_recording.borrow().flushes, 1);
    assert_samples(&first_recording.borrow().samples, 0);
    assert!(
        !state.predecoded_packets.is_empty(),
        "queue transition must use cached content"
    );
    attach(&mut state, &second_recording);
    let mut state = Some(state);
    while let Some(current) = state {
        state = Player::step(Some(current), None).unwrap().player_state;
    }
    assert_eq!(second_recording.borrow().samples.len(), 176_400 * 2);
    assert_eq!(second_recording.borrow().flushes, 1);
    assert_samples(&second_recording.borrow().samples, 0);

    let current = song("long-stereo.wav", 2);
    let preloaded = preloader::preload(current.path.clone()).unwrap();
    let state = PlayerState::play_preloaded(queue(current), preloaded);
    let mut state = Player::step(Some(state), Some(AudioAction::Seek(0.5)))
        .unwrap()
        .player_state
        .unwrap();
    assert!(state.predecoded_packets.is_empty());
    let target = state.seek_ts.unwrap();
    let recording = Rc::new(RefCell::new(Recording::default()));
    attach(&mut state, &recording);
    let mut state = Some(state);
    let mut first_timestamp = None;
    while let Some(current) = state {
        state = Player::step(Some(current), None).unwrap().player_state;
        if !recording.borrow().samples.is_empty() && first_timestamp.is_none() {
            first_timestamp = state.as_ref().map(|state| state.timestamp);
        }
    }
    let timestamp = first_timestamp.unwrap();
    assert!(timestamp >= target);
    let start = (TrackInfo {
        id: 0,
        time_base: Some(
            symphonia::core::units::TimeBase::try_from_recip(44_100).unwrap(),
        ),
        duration: Some(176_400u64.into()),
    })
    .progress_times(timestamp)
    .unwrap()
    .elapsed;
    let start = ((start.seconds as f64 + start.frac) * 44_100.0).round() as usize;
    assert_eq!(recording.borrow().samples.len(), (176_400 - start) * 2);
    assert_samples(&recording.borrow().samples, start);
    assert_eq!(recording.borrow().flushes, 1);
}

#[test]
fn truncated_media_reports_error_without_queue_advance() {
    use std::io::Cursor;
    let mut bytes = include_bytes!("../../tests/fixtures/stereo.wav").to_vec();
    bytes.truncate(bytes.len() - 127);
    let mut hint = Hint::new();
    hint.with_extension("wav");
    let reader = symphonia::default::get_probe()
        .probe(
            &hint,
            MediaSourceStream::new(Box::new(Cursor::new(bytes)), Default::default()),
            Default::default(),
            Default::default(),
        )
        .unwrap();
    let mut queued = queue(song("stereo.wav", 1));
    queued.next.push_back(song("long-stereo.wav", 2));
    let mut state = PlayerState::play_queue(queued).unwrap();
    state.reader = reader;
    let recording = Rc::new(RefCell::new(Recording::default()));
    attach(&mut state, &recording);
    loop {
        assert_eq!(state.queue.current.id, SongId::new(1));
        match Player::step(Some(state), None) {
            Ok(effects) => {
                state = effects
                    .player_state
                    .expect("truncation must not publish normal stop")
            }
            Err(error) => {
                assert!(error.to_string().contains("packet"));
                break;
            }
        }
    }
    assert_eq!(
        recording.borrow().flushes,
        0,
        "truncation must not take normal EOF/queue path"
    );
    assert!(recording.borrow().samples.len() < 44_100 * 2);
}
