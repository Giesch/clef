use std::time::Duration;

use anyhow::anyhow;
use flume::Sender;
use log::{error, info, trace};
use souvlaki::{
    MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, PlatformConfig,
};

use super::AudioAction;

pub struct WrappedControls {
    media_controls: Option<MediaControls>,
    controls_to_audio: Sender<AudioAction>,
}

impl std::fmt::Debug for WrappedControls {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let media_controls = if self.media_controls.is_some() {
            "Some(MediaControls)"
        } else {
            "None"
        };

        f.debug_struct("WrappedControls")
            .field("media_controls", &media_controls)
            .field("controls_to_audio", &self.controls_to_audio)
            .finish()
    }
}

impl WrappedControls {
    pub fn new(controls_to_audio: Sender<AudioAction>) -> Self {
        Self {
            controls_to_audio,
            media_controls: None,
        }
    }

    pub fn set_metadata(&mut self, metadata: MediaMetadata<'_>) {
        self.ensure_init();

        if let Some(ref mut media_controls) = self.media_controls {
            media_controls
                .set_metadata(metadata)
                .map_err(|e| error!("failed to set media controls metadata: {e:?}"))
                .ok();
        }
    }

    pub fn set_playback(&mut self, playback: MediaPlayback) {
        self.ensure_init();

        if let Some(ref mut media_controls) = self.media_controls {
            media_controls
                .set_playback(playback)
                .map_err(|e| error!("failed to set media controls playback: {e:?}"))
                .ok();
        }
    }

    pub fn deinit(&mut self) {
        // NOTE This relies on the controls releasing the dbus name on drop.
        // That previously caused problems with souvlaki 0.5.x, but seems resolved
        self.media_controls = None;
    }

    fn ensure_init(&mut self) {
        if self.media_controls.is_none() {
            self.init()
                .map_err(|e| error!("Failed to init media controls: {e:?}"))
                .ok();
        }
    }

    fn init(&mut self) -> anyhow::Result<()> {
        trace!("initializing media controls");

        #[cfg(not(target_os = "windows"))]
        let hwnd = None;

        #[cfg(target_os = "windows")]
        let hwnd = {
            use std::ffi::c_void;

            let hwnd = clef_shared::window_handle_hack::get_hwnd();
            hwnd.map(|hwnd| hwnd.0 as *mut c_void)
        };

        let platform_config = PlatformConfig {
            dbus_name: "clef.player",
            display_name: "Clef",
            hwnd,
        };

        let mut media_controls = MediaControls::new(platform_config)
            .map_err(|_| anyhow!("failed to create media controls"))?;

        let controls_to_audio = self.controls_to_audio.clone();

        media_controls
            .attach(move |e: MediaControlEvent| {
                trace!("recieved media control event: {e:?}");

                let action: Option<AudioAction> = match &e {
                    MediaControlEvent::Play => Some(AudioAction::PlayPaused),
                    MediaControlEvent::Pause => Some(AudioAction::Pause),
                    MediaControlEvent::Next => Some(AudioAction::Forward),
                    MediaControlEvent::Previous => Some(AudioAction::Back),
                    MediaControlEvent::Toggle => Some(AudioAction::Toggle),

                    MediaControlEvent::Stop => None,
                    MediaControlEvent::Seek(_) => None,
                    MediaControlEvent::SeekBy(_, _) => None,
                    MediaControlEvent::SetPosition(_) => None,
                    MediaControlEvent::OpenUri(_) => None,
                    MediaControlEvent::Raise => None,
                    MediaControlEvent::Quit => None,
                };

                if let Some(action) = action {
                    controls_to_audio
                        .send(action)
                        .map_err(|e| {
                            error!("failed to send from controls to audio: {e:?}")
                        })
                        .ok();
                } else {
                    info!("unsupported media control event: {e:?}");
                }
            })
            .map_err(|_| anyhow!("failed to set up listening to media controls"))?;

        self.media_controls = Some(media_controls);

        Ok(())
    }
}

// an owned version of `souvlaki::MediaMetadata`
#[derive(Debug)]
pub struct ControlsMetadata {
    pub title: Option<String>,
    pub album: Option<String>,
    pub artist: Option<String>,
    pub cover_url: Option<String>,
    pub duration: Option<Duration>,
}

impl<'a> From<&'a ControlsMetadata> for MediaMetadata<'a> {
    fn from(metadata: &'a ControlsMetadata) -> Self {
        MediaMetadata {
            title: metadata.title.as_deref(),
            album: metadata.album.as_deref(),
            artist: metadata.artist.as_deref(),
            cover_url: metadata.cover_url.as_deref(),
            duration: metadata.duration,
        }
    }
}
