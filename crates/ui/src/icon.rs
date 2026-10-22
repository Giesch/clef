use iced::window::Icon;

pub fn get_icon() -> Option<Icon> {
    #[cfg(target_os = "windows")]
    {
        let image = image_rs::load_from_memory(include_bytes!("../base_clef.jpg"))
            .ok()?
            .into_rgba8();
        let (width, height) = image.dimensions();
        iced::window::icon::from_rgba(image.into_raw(), width, height).ok()
    }
    #[cfg(not(target_os = "windows"))]
    {
        None
    }
}
