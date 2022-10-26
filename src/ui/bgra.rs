use camino::Utf8PathBuf;
use iced::widget::image;

/// Image pixels in the format that iced converts them to internally
/// Doing the conversion ahead of time (outside the framework)
/// makes it possible to do it off the ui thread
///
/// https://github.com/iced-rs/iced/issues/549
#[derive(Clone)]
pub struct BgraBytes {
    height: u32,
    width: u32,
    bytes: Vec<u8>,
}

impl From<BgraBytes> for image::Handle {
    fn from(bgra_bytes: BgraBytes) -> Self {
        image::Handle::from_pixels(bgra_bytes.width, bgra_bytes.height, bgra_bytes.bytes)
    }
}

impl std::fmt::Debug for BgraBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BgraBytes")
            .field("height", &self.height)
            .field("width", &self.width)
            .finish()
    }
}

/// NOTE this is slow
/// https://github.com/iced-rs/iced/issues/549
pub fn load_bgra(path: &Utf8PathBuf) -> Option<BgraBytes> {
    let img = image_rs::open(path).ok()?;

    #[allow(deprecated)]
    let bgra = img.to_bgra();

    let bgra_bytes = BgraBytes {
        height: bgra.height(),
        width: bgra.width(),
        bytes: bgra.into_raw(),
    };

    Some(bgra_bytes)
}
