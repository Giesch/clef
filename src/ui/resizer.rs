use std::sync::Arc;

use camino::{Utf8Path, Utf8PathBuf};
use flume::{Receiver, TryRecvError};
use parking_lot::Mutex;

use crate::db::queries::{add_resized_image_location, AlbumId};
use crate::db::SqlitePool;
use crate::platform::project_dirs;
use crate::ui::rgba::{load_rgba, save_rgba, RgbaBytes, IMAGE_SIZE};

#[derive(Clone, Debug)]
pub enum ResizerMessage {
    ResizedImage(ResizedImage),
    ResizerError,
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
    db: SqlitePool,
    inbox: Arc<Mutex<Receiver<ResizeRequest>>>,
) -> iced::Subscription<ResizerMessage> {
    struct ResizerSub;

    iced::subscription::unfold(
        std::any::TypeId::of::<ResizerSub>(),
        ResizerState::Initial,
        move |state| step(state, db.clone(), inbox.clone()),
    )
}

enum ResizerState {
    Initial,
    Working(Utf8PathBuf),
    Final,
}

async fn step(
    state: ResizerState,
    db: SqlitePool,
    inbox: Arc<Mutex<Receiver<ResizeRequest>>>,
) -> (Option<ResizerMessage>, ResizerState) {
    match state {
        ResizerState::Initial => {
            if let Some(images_directory) = get_images_directory() {
                (None, ResizerState::Working(images_directory))
            } else {
                (Some(ResizerMessage::ResizerError), ResizerState::Final)
            }
        }

        ResizerState::Working(images_directory) => {
            let request = match inbox.lock().try_recv() {
                Ok(request) => request,
                Err(TryRecvError::Empty) => {
                    return (None, ResizerState::Working(images_directory));
                }
                Err(TryRecvError::Disconnected) => {
                    return (Some(ResizerMessage::ResizerError), ResizerState::Final);
                }
            };

            let message = match resize(request, &images_directory, db.clone()).await {
                Some(resized_image) => ResizerMessage::ResizedImage(resized_image),
                None => ResizerMessage::ResizerError,
            };

            (Some(message), ResizerState::Working(images_directory))
        }

        ResizerState::Final => (None, ResizerState::Final),
    }
}

fn get_images_directory() -> Option<Utf8PathBuf> {
    // TODO log these Nones as errors, or send useful messages to ui
    let project = project_dirs()?;
    let local_data: &Utf8Path = project.data_local_dir().try_into().ok()?;
    let resized_images_dir = local_data.join("resized_images");

    // ensure the directory is created
    std::fs::create_dir(&resized_images_dir).ok();

    Some(resized_images_dir)
}

async fn resize(
    request: ResizeRequest,
    images_directory: &Utf8Path,
    db: SqlitePool,
) -> Option<ResizedImage> {
    let image_bytes = load_rgba(&request.source_path)?;

    let title = request.album_title;
    let file_name = format!("{title}_{IMAGE_SIZE}.bmp");
    let path = images_directory.join(file_name);

    save_rgba(&path, &image_bytes).ok()?;

    let mut conn = db.get().ok()?;
    conn.immediate_transaction(|tx| {
        add_resized_image_location(tx, request.album_id, &path)
    })
    .ok()?;

    let resized = ResizedImage {
        album_id: request.album_id,
        file: path,
        bytes: image_bytes,
    };

    Some(resized)
}
