use iced_native::window::Icon;

pub fn get_icon() -> Option<Icon> {
    #[cfg(target_os = "windows")]
    let icon = {
        use iced::window::icon;
        use image_rs::ImageFormat;

        let bytes = include_bytes!("../base_clef.jpg");
        icon::from_file_data(bytes, Some(ImageFormat::Jpeg)).ok()
    };

    // making this look ok at a larger size is going to take more work
    #[cfg(not(target_os = "windows"))]
    let icon = None;

    icon
}
