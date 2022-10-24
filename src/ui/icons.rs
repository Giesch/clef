use iced::widget::svg;

pub fn play() -> iced::widget::Svg {
    svg(svg::Handle::from_path(format!(
        "{}/resources/play.svg",
        env!("CARGO_MANIFEST_DIR")
    )))
}

pub fn pause() -> iced::widget::Svg {
    svg(svg::Handle::from_path(format!(
        "{}/resources/pause.svg",
        env!("CARGO_MANIFEST_DIR")
    )))
}
