use iced::theme::{self, Theme};
use iced::widget::button;

pub fn no_background() -> theme::Button {
    theme::Button::Custom(Box::new(NoBackgroundStyle))
}

pub struct NoBackgroundStyle;

impl button::StyleSheet for NoBackgroundStyle {
    type Style = Theme;

    fn active(&self, theme: &Self::Style) -> button::Appearance {
        let mut appearance = theme.active(&theme::Button::Primary);
        appearance.background = None;

        appearance
    }
}
