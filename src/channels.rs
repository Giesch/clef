use std::thread;

use flume::{Receiver, Sender, TryRecvError};

// A message to the audio thread
pub enum ToAudio {
    PlayFilename(String),
}

// A message to the main/ui thread
pub enum ToUi {
    SeekStatus(f32),
}

pub fn spawn_audio_thread(to_audio_rx: Receiver<ToAudio>, to_ui_tx: Sender<ToUi>) {
    thread::spawn(move || loop {
        match to_audio_rx.try_recv() {
            Ok(ToAudio::PlayFilename(file_name)) => {
                // TODO this blocks forever, which also ends up blocking the ui
                crate::audio::play_file(&file_name);
            }

            Err(TryRecvError::Empty) => {}

            Err(TryRecvError::Disconnected) => {
                // TODO should this do cleanup?
                panic!("orphaned audio thread");
            }
        }
    });
}
