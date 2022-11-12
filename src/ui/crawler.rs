use camino::Utf8PathBuf;
use directories::UserDirs;

pub struct Crawler {
    audio_dir: Utf8PathBuf,
}

// NOTE this will appear in an iced message,
// so it must be Clone and not use anyhow
#[derive(thiserror::Error, Debug, Clone)]
pub enum CrawlerError {
    #[error("no audio directory found")]
    NoAudioDir,
}

impl Crawler {
    pub fn init(user_dirs: &UserDirs) -> Result<Self, CrawlerError> {
        let audio_dir = user_dirs.audio_dir().ok_or(CrawlerError::NoAudioDir)?;
        let audio_dir: Utf8PathBuf = audio_dir
            .to_owned()
            .try_into()
            .map_err(|_| CrawlerError::NoAudioDir)?;

        Ok(Self { audio_dir })
    }
}
