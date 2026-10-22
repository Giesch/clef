use super::*;
use crate::test_util::fake_album;
use iced::futures::{channel::mpsc, executor::LocalPool, task::LocalSpawnExt, StreamExt};
use iced_test::futures::subscription::{into_recipes, Tracker};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

fn library() -> Ui {
    let mut ui = Ui::new();
    update(
        &mut ui,
        Message::FromCrawler(CrawlerMessage::CrawledAlbum(Box::new(fake_album()))),
    );

    ui
}

#[test]
fn ui_library_and_playback_scenario() {
    let mut ui = library();
    let mut simulator = iced_test::simulator(view(&ui));
    for label in ["Album Title", "Fake Artist", "First", "Fifth", "1:40"] {
        assert!(simulator.find(label).is_ok(), "missing {label}");
    }
    drop(simulator);
    update(&mut ui, Message::HoveredSong(SongId::new(1)));
    let mut simulator = iced_test::simulator(view(&ui));
    simulator
        .click(iced_test::selector::id(format!(
            "song-play-{:?}",
            SongId::new(1)
        )))
        .unwrap();
    let messages: Vec<_> = simulator.into_messages().collect();
    let effects: Vec<_> = messages
        .into_iter()
        .map(|message| update(&mut ui, message))
        .collect();
    assert!(effects
        .iter()
        .any(|effect| matches!(effect, Effect::ToAudio(AudioAction::PlayQueue(_)))));
    use clef_audio::player::ProgressTime;
    let display = PlayerDisplay {
        song_id: SongId::new(1),
        playing: true,
        times: ProgressTimes {
            elapsed: ProgressTime { seconds: 10, frac: 0.0 },
            remaining: ProgressTime { seconds: 90, frac: 0.0 },
            total: ProgressTime { seconds: 100, frac: 0.0 },
        },
    };
    update(
        &mut ui,
        Message::FromAudio(AudioMessage::DisplayUpdate(Some(display.clone()))),
    );
    let mut simulator = iced_test::simulator(view(&ui));
    assert!(simulator.find("0:10 / 1:40").is_ok());
    let bounds = simulator
        .find(iced_test::selector::id("playback-seek"))
        .unwrap()
        .visible_bounds()
        .unwrap();
    simulator.point_at(iced::Point::new(
        bounds.x + bounds.width * 0.25,
        bounds.center_y(),
    ));
    simulator.simulate([
        Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left)),
        Event::Mouse(iced::mouse::Event::ButtonReleased(
            iced::mouse::Button::Left,
        )),
    ]);
    let messages: Vec<_> = simulator.into_messages().collect();
    assert!(messages
        .iter()
        .any(|message| matches!(message, Message::SeekDrag(_))));
    assert!(messages
        .iter()
        .any(|message| matches!(message, Message::SeekRelease)));
    let effects: Vec<_> = messages
        .into_iter()
        .map(|message| update(&mut ui, message))
        .collect();
    assert!(effects.iter().any(|effect| matches!(effect, Effect::ToAudio(AudioAction::Seek(proportion)) if (proportion - 0.25).abs() <= 0.02)));
    assert!(matches!(ui.progress, Some(ProgressDisplay::Dragging(_))));
    update(
        &mut ui,
        Message::FromAudio(AudioMessage::DisplayUpdate(Some(display.clone()))),
    );
    assert!(matches!(ui.progress, Some(ProgressDisplay::Dragging(_))));
    update(
        &mut ui,
        Message::FromAudio(AudioMessage::SeekComplete(display)),
    );
    assert!(matches!(ui.progress, Some(ProgressDisplay::FromAudio(_))));
    for expected in ["back", "forward", "pause", "resume"] {
        ui.current_song.as_mut().unwrap().playing = expected != "resume";
        let mut simulator = iced_test::simulator(view(&ui));
        simulator
            .click(iced_test::selector::id(format!("playback-{expected}")))
            .unwrap();
        let messages: Vec<_> = simulator.into_messages().collect();
        let effects: Vec<_> = messages
            .into_iter()
            .map(|message| update(&mut ui, message))
            .collect();
        assert!(effects.into_iter().any(|effect| matches!(
            (expected, effect),
            ("back", Effect::ToAudio(AudioAction::Back))
                | ("forward", Effect::ToAudio(AudioAction::Forward))
                | ("pause", Effect::ToAudio(AudioAction::Pause))
                | ("resume", Effect::ToAudio(AudioAction::PlayPaused))
        )));
    }
}

#[test]
fn song_row_control_geometry_is_stable() {
    for width in [360.0, 640.0] {
        for number in [Some(1), Some(12), Some(123), None] {
            let mut song = crate::test_util::fake_song(1, "Tune", AlbumId::new(1));
            song.track_number = number;
            let mut baseline = None;
            for status in [
                SongRowStatus::Blank,
                SongRowStatus::Hovered,
                SongRowStatus::Playing,
                SongRowStatus::Paused,
            ] {
                let context =
                    format!("width={width}, number={number:?}, status={status:?}");
                let mut simulator = iced_test::simulator(
                    container(view_song_row(&song, status)).width(width),
                );
                let row = simulator
                    .find(iced_test::selector::id(format!("song-row-{:?}", song.id)))
                    .unwrap()
                    .visible_bounds()
                    .unwrap();
                let slot = simulator
                    .find(iced_test::selector::id(format!("song-slot-{:?}", song.id)))
                    .unwrap()
                    .visible_bounds()
                    .unwrap();
                let title = simulator.find("Tune").unwrap().visible_bounds().unwrap();
                let duration = simulator.find("1:40").unwrap().visible_bounds().unwrap();
                let close = |actual: f32, expected: f32| {
                    assert!(
                        (actual - expected).abs() < 0.1,
                        "{context}: expected {expected}, got {actual}"
                    );
                };
                close(row.width, width);
                close(slot.width, 34.0);
                close(slot.height, 34.0);
                close(title.x, row.x + 2.0 + 34.0 + 10.0);
                assert!(title.width > 100.0, "{context}: title has no useful width");
                let geometry = [title.x, title.width, duration.x, duration.y, row.height];
                if let Some(expected) = baseline {
                    for (actual, expected) in geometry.into_iter().zip(expected) {
                        close(actual, expected);
                    }
                } else {
                    baseline = Some(geometry);
                }
            }
        }
    }
}

#[test]
fn song_row_controls_emit_expected_messages() {
    let song = crate::test_util::fake_song(1, "Tune", AlbumId::new(1));
    for status in [
        SongRowStatus::Hovered,
        SongRowStatus::Playing,
        SongRowStatus::Paused,
    ] {
        let expected = match status {
            SongRowStatus::Hovered => Message::PlaySongClicked(song.id),
            SongRowStatus::Playing => Message::PauseClicked,
            SongRowStatus::Paused => Message::PlayPausedClicked,
            SongRowStatus::Blank => unreachable!(),
        };
        let mut simulator =
            iced_test::simulator(container(view_song_row(&song, status)).width(360));
        let row = simulator
            .find(iced_test::selector::id(format!("song-row-{:?}", song.id)))
            .unwrap()
            .visible_bounds()
            .unwrap();
        simulator.point_at(iced::Point::new(row.x + 2.0 + 17.0, row.y + 2.0 + 17.0));
        simulator.simulate([
            Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left)),
            Event::Mouse(iced::mouse::Event::ButtonReleased(
                iced::mouse::Button::Left,
            )),
        ]);
        let messages: Vec<_> = simulator.into_messages().collect();
        assert!(
            messages.iter().any(|message| match (&expected, message) {
                (
                    Message::PlaySongClicked(expected),
                    Message::PlaySongClicked(actual),
                ) => expected == actual,
                (Message::PauseClicked, Message::PauseClicked)
                | (Message::PlayPausedClicked, Message::PlayPausedClicked) => true,
                _ => false,
            }),
            "expected {expected:?}, got {messages:?}"
        );
    }
}

#[test]
fn ui_hover_controls_scenario() {
    let mut ui = library();
    let control_id = format!("song-play-{:?}", SongId::new(1));
    let mut simulator = iced_test::simulator(view(&ui));
    assert!(simulator
        .find(iced_test::selector::id(control_id.clone()))
        .is_err());
    let target = simulator.find("First").unwrap();
    let position = target.visible_bounds().unwrap().center();
    simulator.point_at(position);
    simulator.simulate([Event::Mouse(iced::mouse::Event::CursorMoved { position })]);
    let messages: Vec<_> = simulator.into_messages().collect();
    assert!(messages
        .iter()
        .any(|m| matches!(m, Message::HoveredSong(id) if *id == SongId::new(1))));
    for message in messages {
        update(&mut ui, message);
    }
    assert_eq!(ui.hovered_song_id, Some(SongId::new(1)));
    assert_eq!(
        song_row_status(&ui.current_song, ui.hovered_song_id, SongId::new(1)),
        SongRowStatus::Hovered
    );
    let mut simulator = iced_test::simulator(view(&ui));
    assert!(simulator
        .find(iced_test::selector::id(control_id.clone()))
        .is_ok());
    simulator.point_at(position);
    simulator.simulate([Event::Mouse(iced::mouse::Event::CursorMoved { position })]);
    simulator.point_at(iced::Point::ORIGIN);
    simulator.simulate([Event::Mouse(iced::mouse::Event::CursorMoved {
        position: iced::Point::ORIGIN,
    })]);
    for message in simulator.into_messages() {
        update(&mut ui, message);
    }
    assert_eq!(ui.hovered_song_id, None);
    let mut simulator = iced_test::simulator(view(&ui));
    assert!(simulator.find(iced_test::selector::id(control_id)).is_err());
}

#[test]
fn ui_space_key_mapping() {
    let mut ui = library();
    let events =
        iced_test::simulator::tap_key(Key::Named(Named::Space), None).collect::<Vec<_>>();
    let mut simulator = iced_test::simulator(view(&ui));
    simulator.simulate(events.clone());
    assert_eq!(simulator.into_messages().count(), 0);
    assert!(events
        .iter()
        .cloned()
        .all(|event| matches!(update(&mut ui, Message::Native(event)), Effect::None)));
    for playing in [true, false] {
        ui.current_song = get_current_song(&ui.music_cache, SongId::new(1), playing);
        let effects = events
            .iter()
            .cloned()
            .map(|event| update(&mut ui, Message::Native(event)))
            .collect::<Vec<_>>();
        assert_eq!(
            effects
                .iter()
                .filter(|effect| matches!(effect, Effect::ToAudio(_)))
                .count(),
            1
        );
        assert!(effects.iter().any(|effect| matches!(
            (playing, effect),
            (true, Effect::ToAudio(AudioAction::Pause))
                | (false, Effect::ToAudio(AudioAction::PlayPaused))
        )));
    }
}

#[test]
fn subscription_identity_and_delivery() {
    #[derive(Clone)]
    struct Counts {
        starts: Arc<AtomicUsize>,
        drops: Arc<AtomicUsize>,
        closed: Arc<AtomicUsize>,
    }
    struct Guard(Counts);
    impl Drop for Guard {
        fn drop(&mut self) {
            self.0.drops.fetch_add(1, Ordering::SeqCst);
        }
    }
    fn subscription(
        generation: u64,
        inbox: flume::Receiver<u8>,
        counts: Counts,
    ) -> Subscription<u8> {
        struct TestWorker;
        old_unfold::old_unfold(
            (std::any::TypeId::of::<TestWorker>(), generation),
            || (None::<Guard>, false),
            move |(guard, closed)| {
                let inbox = inbox.clone();
                let counts = counts.clone();
                async move {
                    let guard = guard.unwrap_or_else(|| {
                        counts.starts.fetch_add(1, Ordering::SeqCst);
                        Guard(counts.clone())
                    });
                    if closed {
                        return iced::futures::future::pending().await;
                    }
                    match inbox.recv_async().await {
                        Ok(message) => (Some(message), (Some(guard), false)),
                        Err(_) => {
                            counts.closed.fetch_add(1, Ordering::SeqCst);
                            (Some(255), (Some(guard), true))
                        }
                    }
                }
            },
        )
    }
    let counts = Counts {
        starts: Arc::new(AtomicUsize::new(0)),
        drops: Arc::new(AtomicUsize::new(0)),
        closed: Arc::new(AtomicUsize::new(0)),
    };
    let mut tracker = Tracker::new();
    let mut pool = LocalPool::new();
    let (output, mut received) = mpsc::unbounded();
    let (sender, inbox) = flume::unbounded();
    let install = |tracker: &mut Tracker, pool: &mut LocalPool, generation, inbox| {
        let futures = tracker.update(
            into_recipes(subscription(generation, inbox, counts.clone())).into_iter(),
            output.clone(),
        );
        let started = futures.len();
        for future in futures {
            pool.spawner().spawn_local(future).unwrap();
        }
        pool.run_until_stalled();
        started
    };
    assert_eq!(install(&mut tracker, &mut pool, 0, inbox.clone()), 1);
    for _ in 0..5 {
        assert_eq!(install(&mut tracker, &mut pool, 0, inbox.clone()), 0);
    }
    assert_eq!(counts.starts.load(Ordering::SeqCst), 1);
    sender.send(7).unwrap();
    pool.run_until_stalled();
    assert_eq!(pool.run_until(received.next()), Some(7));
    assert_eq!(install(&mut tracker, &mut pool, 0, inbox.clone()), 0);
    sender.send(8).unwrap();
    pool.run_until_stalled();
    assert_eq!(pool.run_until(received.next()), Some(8));
    let (replacement, replacement_inbox) = flume::unbounded();
    assert_eq!(
        install(&mut tracker, &mut pool, 1, replacement_inbox.clone()),
        1
    );
    assert_eq!(counts.starts.load(Ordering::SeqCst), 2);
    assert_eq!(counts.drops.load(Ordering::SeqCst), 1);
    replacement.send(9).unwrap();
    pool.run_until_stalled();
    assert_eq!(pool.run_until(received.next()), Some(9));
    drop(replacement);
    pool.run_until_stalled();
    assert_eq!(counts.closed.load(Ordering::SeqCst), 1);
    assert_eq!(pool.run_until(received.next()), Some(255));
    assert_eq!(install(&mut tracker, &mut pool, 1, replacement_inbox), 0);
    use iced::futures::FutureExt;
    assert!(
        received.next().now_or_never().is_none(),
        "terminal message delivered more than once"
    );
    tracker.update(
        into_recipes(Subscription::<u8>::none()).into_iter(),
        output.clone(),
    );
    pool.run_until_stalled();
    assert_eq!(counts.drops.load(Ordering::SeqCst), 2);
    assert_eq!(counts.starts.load(Ordering::SeqCst), 2);
}
