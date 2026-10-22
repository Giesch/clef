use iced::widget::button;
use iced::Theme;

pub fn no_background() -> impl Fn(&Theme, button::Status) -> button::Style {
    |theme, status| {
        let mut style = button::primary(theme, status);
        style.background = None;

        style
    }
}
