use cosmic::iced::Size;

use crate::app::{App, Flags};

pub fn launch(flags: Flags) -> anyhow::Result<()> {
    crate::i18n::init(&i18n_embed::DesktopLanguageRequester::requested_languages());

    let settings = cosmic::app::Settings::default().size(Size::new(1024.0, 768.0));
    cosmic::app::run::<App>(settings, flags)?;

    Ok(())
}
