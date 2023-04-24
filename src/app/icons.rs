use iced::widget::{svg, Svg};
use iced_style::svg::StyleSheet;

pub fn play<Renderer>() -> Svg<Renderer>
where
    Renderer: iced_native::svg::Renderer,
    Renderer::Theme: StyleSheet,
{
    svg_icon("play.svg")
}

pub fn pause<Renderer>() -> Svg<Renderer>
where
    Renderer: iced_native::svg::Renderer,
    Renderer::Theme: StyleSheet,
{
    svg_icon("pause.svg")
}

pub fn forward<Renderer>() -> Svg<Renderer>
where
    Renderer: iced_native::svg::Renderer,
    Renderer::Theme: StyleSheet,
{
    svg_icon("skip-forward.svg")
}

pub fn back<Renderer>() -> Svg<Renderer>
where
    Renderer: iced_native::svg::Renderer,
    Renderer::Theme: StyleSheet,
{
    svg_icon("skip-back.svg")
}

fn svg_icon<Renderer>(file_name: &str) -> Svg<Renderer>
where
    Renderer: iced_native::svg::Renderer,
    Renderer::Theme: StyleSheet,
{
    let project_root = env!("CARGO_MANIFEST_DIR");
    let path = format!("{project_root}/resources/{file_name}");

    svg(svg::Handle::from_path(path))
}
