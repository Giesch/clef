#![allow(unused)]

use iced_graphics::Vector;
use iced_native::{
    event, keyboard, layout::Limits, mouse, overlay, renderer, touch, widget::Tree,
    Background, Clipboard, Color, Element, Event, Layout, Length, Point, Rectangle,
    Shell, Size, Widget,
};
use iced_style::Theme;

// From iced_aw::native::modal

// A modal for showing elements as an overlay on top of another.

/// A modal content as an overlay.
///
/// Can be used in combination with the [`Card`](crate::card::Card)
/// widget to form dialog elements.
#[allow(missing_debug_implementations)]
pub struct Modal<'a, Content, Message, Renderer>
where
    Content: Fn() -> Element<'a, Message, Renderer>,
    Message: Clone,
    Renderer: iced_native::Renderer,
    Renderer::Theme: StyleSheet,
{
    /// Show the modal.
    show_modal: bool,
    /// The underlying element.
    underlay: Element<'a, Message, Renderer>,
    /// The content of teh [`ModalOverlay`](ModalOverlay).
    content: Content,
    /// The optional message that will be send when the user clicked on the backdrop.
    backdrop: Option<Message>,
    /// The optional message that will be send when the ESC key was pressed.
    esc: Option<Message>,
    /// The style of the [`ModalOverlay`](ModalOverlay).
    style: <Renderer::Theme as StyleSheet>::Style,
}

impl<'a, Content, Message, Renderer> Modal<'a, Content, Message, Renderer>
where
    Content: Fn() -> Element<'a, Message, Renderer>,
    Message: Clone,
    Renderer: iced_native::Renderer,
    Renderer::Theme: StyleSheet,
{
    /// Creates a new [`Modal`](Modal) wrapping the underlying element to
    /// show some content as an overlay.
    ///
    /// `state` is the content's state, assigned at the creation of the
    /// overlying content.
    ///
    /// It expects:
    ///     * if the overlay of the date picker is visible.
    ///     * the underlay [`Element`](iced_native::Element) on which this [`Modal`](Modal)
    ///         will be wrapped around.
    ///     * the content [`Element`](iced_native::Element) of the [`Modal`](Modal).
    pub fn new<U>(show_modal: bool, underlay: U, content: Content) -> Self
    where
        U: Into<Element<'a, Message, Renderer>>,
    {
        Modal {
            show_modal,
            underlay: underlay.into(),
            content,
            backdrop: None,
            esc: None,
            style: <Renderer::Theme as StyleSheet>::Style::default(),
        }
    }

    /// Sets the message that will be produced when the backdrop of the
    /// [`Modal`](Modal) is clicked.
    #[must_use]
    pub fn backdrop(mut self, message: Message) -> Self {
        self.backdrop = Some(message);
        self
    }

    /// Sets the message that will be produced when the Escape Key is
    /// pressed when the modal is open.
    ///
    /// This can be used to close the modal on ESC.
    #[must_use]
    pub fn on_esc(mut self, message: Message) -> Self {
        self.esc = Some(message);
        self
    }

    /// Sets the style of the [`Modal`](Modal).
    #[must_use]
    pub fn style(mut self, style: <Renderer::Theme as StyleSheet>::Style) -> Self {
        self.style = style;
        self
    }
}

impl<'a, Content, Message, Renderer> Widget<Message, Renderer>
    for Modal<'a, Content, Message, Renderer>
where
    Content: 'a + Fn() -> Element<'a, Message, Renderer>,
    Message: 'a + Clone,
    Renderer: 'a + iced_native::Renderer,
    Renderer::Theme: StyleSheet,
{
    fn children(&self) -> Vec<iced_native::widget::Tree> {
        vec![Tree::new(&self.underlay), Tree::new(&(self.content)())]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&[&self.underlay, &(self.content)()]);
    }

    fn width(&self) -> Length {
        self.underlay.as_widget().width()
    }

    fn height(&self) -> Length {
        self.underlay.as_widget().height()
    }

    fn layout(
        &self,
        renderer: &Renderer,
        limits: &iced_native::layout::Limits,
    ) -> iced_native::layout::Node {
        self.underlay.as_widget().layout(renderer, limits)
    }

    fn on_event(
        &mut self,
        state: &mut Tree,
        event: Event,
        layout: Layout<'_>,
        cursor_position: Point,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
    ) -> event::Status {
        self.underlay.as_widget_mut().on_event(
            &mut state.children[0],
            event,
            layout,
            cursor_position,
            renderer,
            clipboard,
            shell,
        )
    }

    fn mouse_interaction(
        &self,
        state: &Tree,
        layout: Layout<'_>,
        cursor_position: Point,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.underlay.as_widget().mouse_interaction(
            &state.children[0],
            layout,
            cursor_position,
            viewport,
            renderer,
        )
    }

    fn draw(
        &self,
        state: &iced_native::widget::Tree,
        renderer: &mut Renderer,
        theme: &Renderer::Theme,
        style: &iced_native::renderer::Style,
        layout: Layout<'_>,
        cursor_position: Point,
        viewport: &Rectangle,
    ) {
        self.underlay.as_widget().draw(
            &state.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor_position,
            viewport,
        );
    }

    fn overlay<'b>(
        &'b mut self,
        state: &'b mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
    ) -> Option<iced_native::overlay::Element<'b, Message, Renderer>> {
        if !self.show_modal {
            return self.underlay.as_widget_mut().overlay(
                &mut state.children[0],
                layout,
                renderer,
            );
        }

        let bounds = layout.bounds();
        let position = Point::new(bounds.x, bounds.y);

        Some(
            ModalOverlay::new(
                &mut state.children[1],
                (self.content)(),
                self.backdrop.clone(),
                self.esc.clone(),
                self.style,
            )
            .overlay(position),
        )
    }
}

impl<'a, Content, Message, Renderer> From<Modal<'a, Content, Message, Renderer>>
    for Element<'a, Message, Renderer>
where
    Content: 'a + Fn() -> Element<'a, Message, Renderer>,
    Message: 'a + Clone,
    Renderer: 'a + iced_native::Renderer,
    Renderer::Theme: StyleSheet,
{
    fn from(modal: Modal<'a, Content, Message, Renderer>) -> Self {
        Element::new(modal)
    }
}

/// The state of the modal.
#[derive(Debug, Default)]
pub struct State<S> {
    /// The visibility of the [`Modal`](Modal) overlay.
    show: bool,
    /// The state of the content of the [`Modal`](Modal) overlay.
    state: S,
}

impl<S> State<S> {
    /// Creates a new [`State`](State) containing the given state data.
    pub const fn new(s: S) -> Self {
        Self { show: false, state: s }
    }

    /// Setting this to true shows the modal (the modal is open), false means
    /// the modal is hidden (closed).
    pub fn show(&mut self, b: bool) {
        self.show = b;
    }

    /// See if this modal will be shown or not.
    pub const fn is_shown(&self) -> bool {
        self.show
    }

    /// Get a mutable reference to the inner state data.
    pub fn inner_mut(&mut self) -> &mut S {
        &mut self.state
    }

    /// Get a reference to the inner state data.
    pub const fn inner(&self) -> &S {
        &self.state
    }
}

// From iced_aw::native::overlay::modal

// A modal for showing elements as an overlay on top of another.
//
// *This API requires the following crate features to be activated: modal*

/// The overlay of the modal.
#[allow(missing_debug_implementations)]
pub struct ModalOverlay<'a, Message, Renderer>
where
    Message: 'a + Clone,
    Renderer: 'a + iced_native::Renderer,
    Renderer::Theme: StyleSheet,
{
    /// The state of the [`ModalOverlay`](ModalOverlay).
    state: &'a mut Tree,
    /// The content of the [`ModalOverlay`](ModalOverlay).
    content: Element<'a, Message, Renderer>,
    /// The optional message that will be send when the user clicks on the backdrop.
    backdrop: Option<Message>,
    /// The optional message that will be send when the ESC key was pressed.
    esc: Option<Message>,
    /// The style of the [`ModalOverlay`](ModalOverlay).
    style: <Renderer::Theme as StyleSheet>::Style,
}

impl<'a, Message, Renderer> ModalOverlay<'a, Message, Renderer>
where
    Message: Clone,
    Renderer: iced_native::Renderer,
    Renderer::Theme: StyleSheet,
{
    /// Creates a new [`ModalOverlay`](ModalOverlay).
    pub fn new<C>(
        state: &'a mut Tree,
        content: C,
        backdrop: Option<Message>,
        esc: Option<Message>,
        style: <Renderer::Theme as StyleSheet>::Style,
    ) -> Self
    where
        C: Into<Element<'a, Message, Renderer>>,
    {
        ModalOverlay {
            state,
            content: content.into(),
            backdrop,
            esc,
            style,
        }
    }

    /// Turn this [`ModalOverlay`] into an overlay
    /// [`Element`](iced_native::overlay::Element).
    pub fn overlay(self, position: Point) -> overlay::Element<'a, Message, Renderer> {
        overlay::Element::new(position, Box::new(self))
    }
}

impl<'a, Message, Renderer> iced_native::Overlay<Message, Renderer>
    for ModalOverlay<'a, Message, Renderer>
where
    Message: 'a + Clone,
    Renderer: 'a + iced_native::Renderer,
    Renderer::Theme: StyleSheet,
{
    fn layout(
        &self,
        renderer: &Renderer,
        bounds: iced_graphics::Size,
        position: Point,
    ) -> iced_native::layout::Node {
        let limits = Limits::new(Size::ZERO, bounds);

        let mut content = self.content.as_widget().layout(renderer, &limits);

        // Center position
        let max_size = limits.max();
        let container_half_width = max_size.width / 2.0;
        let container_half_height = max_size.height / 2.0;
        let content_half_width = content.bounds().width / 2.0;
        let content_half_height = content.bounds().height / 2.0;

        let position = position
            + Vector::new(
                container_half_width - content_half_width,
                container_half_height - content_half_height,
            );

        content.move_to(position);

        iced_native::layout::Node::with_children(max_size, vec![content])
    }

    fn on_event(
        &mut self,
        event: Event,
        layout: Layout<'_>,
        cursor_position: Point,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
    ) -> event::Status {
        let esc_status = self.esc.as_ref().map_or(event::Status::Ignored, |esc| {
            if counts_as_esc_press(&event) {
                shell.publish(esc.to_owned());
                event::Status::Captured
            } else {
                event::Status::Ignored
            }
        });

        let backdrop_status = self
            .backdrop
            .as_ref()
            .zip(layout.children().next())
            .map_or(event::Status::Ignored, |(backdrop, layout)| match event {
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                | Event::Touch(touch::Event::FingerPressed { .. }) => {
                    if layout.bounds().contains(cursor_position) {
                        event::Status::Ignored
                    } else {
                        shell.publish(backdrop.to_owned());
                        event::Status::Captured
                    }
                }
                _ => event::Status::Ignored,
            });

        match esc_status.merge(backdrop_status) {
            event::Status::Ignored => self.content.as_widget_mut().on_event(
                self.state,
                event,
                layout
                    .children()
                    .next()
                    .expect("Native: Layout should have a content layout."),
                cursor_position,
                renderer,
                clipboard,
                shell,
            ),
            event::Status::Captured => event::Status::Captured,
        }
    }

    fn mouse_interaction(
        &self,
        layout: Layout<'_>,
        cursor_position: Point,
        viewport: &iced_graphics::Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            self.state,
            layout
                .children()
                .next()
                .expect("Native: Layout should have a content layout."),
            cursor_position,
            viewport,
            renderer,
        )
    }

    fn draw(
        &self,
        renderer: &mut Renderer,
        theme: &Renderer::Theme,
        style: &iced_native::renderer::Style,
        layout: iced_native::Layout<'_>,
        cursor_position: Point,
    ) {
        let bounds = layout.bounds();

        let style_sheet = theme.active(self.style);

        // Background
        renderer.fill_quad(
            renderer::Quad {
                bounds,
                border_radius: (0.0).into(),
                border_width: 0.0,
                border_color: Color::TRANSPARENT,
            },
            style_sheet.background,
        );

        let content_layout = layout
            .children()
            .next()
            .expect("Native: Layout should have a content layout.");

        // Modal
        self.content.as_widget().draw(
            self.state,
            renderer,
            theme,
            style,
            content_layout,
            cursor_position,
            &bounds,
        );
    }
}

fn counts_as_esc_press(event: &Event) -> bool {
    use keyboard::Event::KeyPressed;
    let Event::Keyboard(KeyPressed { key_code, modifiers }) = event else {
        return false;
    };

    let key_code = *key_code;
    let literal_escape = key_code == keyboard::KeyCode::Escape;
    let control_g = key_code == keyboard::KeyCode::G && modifiers.control();

    literal_escape || control_g
}

// From iced_aw::style::modal

/// The appearance of a [`Modal`](crate::native::Modal).
#[derive(Clone, Copy, Debug)]
pub struct Appearance {
    /// The backgronud of the [`Modal`](crate::native::Modal).
    ///
    /// This is used to color the backdrop of the modal.
    pub background: Background,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            background: Background::Color([0.87, 0.87, 0.87, 0.30].into()),
        }
    }
}
/// The appearance of a [`Modal`](crate::native::Modal).
pub trait StyleSheet {
    ///Style for the trait to use.
    type Style: Default + Copy;
    /// The normal appearance of a [`Modal`](crate::native::Modal).
    fn active(&self, style: Self::Style) -> Appearance;
}

/// The default appearance of a [`Modal`](crate::native::Modal).
#[derive(Clone, Copy, Debug, Default)]
#[allow(missing_docs, clippy::missing_docs_in_private_items)]
pub enum ModalStyles {
    #[default]
    Default,
}

impl StyleSheet for Theme {
    type Style = ModalStyles;

    fn active(&self, _style: Self::Style) -> Appearance {
        let palette = self.extended_palette();

        Appearance {
            background: iced_graphics::Color {
                a: palette.background.base.color.a * 0.5,
                ..palette.background.base.color
            }
            .into(),
        }
    }
}
