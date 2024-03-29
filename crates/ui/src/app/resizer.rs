use std::sync::Arc;

use anyhow::Context;
use camino::{Utf8Path, Utf8PathBuf};
use flume::{Receiver, TryRecvError};
use log::error;

use crate::app::old_unfold::old_unfold;
use crate::app::rgba::{load_rgba, save_rgba, RgbaBytes, IMAGE_SIZE};
use clef_db::queries::{add_resized_image_location, AlbumId};
use clef_db::SqlitePool;

use super::Config;

#[derive(Clone, Debug)]
pub enum ResizerMessage {
    ResizedImage(ResizedImage),
}

#[derive(Clone, Debug)]
pub struct ResizedImage {
    pub album_id: AlbumId,
    pub file: Utf8PathBuf,
    pub bytes: RgbaBytes,
}

#[derive(Debug)]
pub struct ResizeRequest {
    pub album_id: AlbumId,
    pub album_title: String,
    pub source_path: Utf8PathBuf,
}

pub fn resizer_subscription(
    config: Arc<Config>,
    db: SqlitePool,
    inbox: Receiver<ResizeRequest>,
) -> iced::Subscription<ResizerMessage> {
    struct ResizerSub;

    old_unfold(
        std::any::TypeId::of::<ResizerSub>(),
        ResizerState::Working,
        move |state| step(state, config.clone(), db.clone(), inbox.clone()),
    )
}

enum ResizerState {
    Working,
    Stopped,
}

async fn step(
    state: ResizerState,
    config: Arc<Config>,
    db: SqlitePool,
    inbox: Receiver<ResizeRequest>,
) -> (Option<ResizerMessage>, ResizerState) {
    match state {
        ResizerState::Working => {
            let images_directory = &config.resized_images_directory;

            let request = match inbox.try_recv() {
                Ok(request) => request,
                Err(TryRecvError::Empty) => {
                    return (None, ResizerState::Working);
                }
                Err(TryRecvError::Disconnected) => {
                    return (None, ResizerState::Stopped);
                }
            };

            let message = match resize(&request, images_directory, db) {
                Ok(resized_image) => Some(ResizerMessage::ResizedImage(resized_image)),
                Err(e) => {
                    error!("error resizing image: {request:#?} {e}");
                    None
                }
            };

            (message, ResizerState::Working)
        }

        ResizerState::Stopped => (None, ResizerState::Stopped),
    }
}

fn resize(
    request: &ResizeRequest,
    images_directory: &Utf8Path,
    db: SqlitePool,
) -> anyhow::Result<ResizedImage> {
    let image_bytes = load_rgba(&request.source_path).context("loading original")?;

    let title: String = request
        .album_title
        .chars()
        .filter(|&c| c != '\\' && c != '/')
        .collect();
    let album_id = request.album_id.unpack();
    let file_name = format!("{title}_{album_id}_{IMAGE_SIZE}.bmp");
    let file_name: Utf8PathBuf = file_name.try_into()?;
    let path = images_directory.join(file_name);

    save_rgba(&path, &image_bytes)
        .with_context(|| format!("saving resized bmp: {path}"))?;

    let mut conn = db.get().context("checking out db connection")?;
    conn.immediate_transaction(|tx| {
        add_resized_image_location(tx, request.album_id, &path)
    })?;

    let resized = ResizedImage {
        album_id: request.album_id,
        file: path,
        bytes: image_bytes,
    };

    Ok(resized)
}
