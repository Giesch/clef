#![allow(unused)]

use iced::event;
use iced::{Element, Length, Padding};
use iced_lazy::lazy;
use iced_native::widget::{tree, Operation, Tree};
use iced_native::{layout, Layout, Widget};

pub fn hoverable<'a, Message, Renderer>(
    view: impl Fn(bool) -> Element<'a, Message, Renderer> + 'a,
) -> Hoverable<'a, Message, Renderer>
where
    Renderer: iced_native::Renderer,
{
    Hoverable::new(view)
}

#[allow(missing_debug_implementations)]
pub struct Hoverable<'a, Message, Renderer>
where
    Renderer: iced_native::Renderer,
{
    hovered_content: Element<'a, Message, Renderer>,
    unhovered_content: Element<'a, Message, Renderer>,
    is_hovered: bool,
}

impl<'a, Message, Renderer> Hoverable<'a, Message, Renderer>
where
    Renderer: iced_native::Renderer,
{
    pub fn new(view: impl Fn(bool) -> Element<'a, Message, Renderer> + 'a) -> Self {
        let hovered_content = view(true);
        let unhovered_content = view(false);

        Self {
            hovered_content,
            unhovered_content,
            is_hovered: false,
        }
    }

    fn content(&self) -> &Element<'a, Message, Renderer> {
        if self.is_hovered {
            &self.hovered_content
        } else {
            &self.unhovered_content
        }
    }

    fn content_mut(&mut self) -> &mut Element<'a, Message, Renderer> {
        if self.is_hovered {
            &mut self.hovered_content
        } else {
            &mut self.unhovered_content
        }
    }
}

impl<'a, Message, Renderer> Widget<Message, Renderer> for Hoverable<'a, Message, Renderer>
where
    Message: 'a + Clone,
    Renderer: 'a + iced_native::Renderer,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::new())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(self.content())]
    }

    fn diff(&self, tree: &mut Tree) {
        let current_state = tree.state.downcast_mut::<State>();

        if current_state.is_hovered != self.is_hovered {
            current_state.is_hovered = self.is_hovered;
        }

        tree.diff_children(std::slice::from_ref(self.content()));
    }

    fn width(&self) -> Length {
        self.content().as_widget().width()
    }

    fn height(&self) -> Length {
        self.content().as_widget().height()
    }

    fn on_event(
        &mut self,
        tree: &mut Tree,
        event: iced::Event,
        layout: iced_native::Layout<'_>,
        cursor_position: iced::Point,
        renderer: &Renderer,
        clipboard: &mut dyn iced_native::Clipboard,
        shell: &mut iced_native::Shell<'_, Message>,
    ) -> event::Status {
        if let event::Status::Captured = self.content_mut().as_widget_mut().on_event(
            &mut tree.children[0],
            event.clone(),
            layout.children().next().unwrap(),
            cursor_position,
            renderer,
            clipboard,
            shell,
        ) {
            return event::Status::Captured;
        }

        let mut state = tree.state.downcast_mut::<State>();
        let now_hovered = layout.bounds().contains(cursor_position);

        if state.is_hovered != now_hovered {
            // FIXME this causes a panic in Button::on_event
            // at the unwrap on line 196
            state.is_hovered = now_hovered;
            self.is_hovered = now_hovered;
            // TODO publish a message instead
        }

        event::Status::Ignored
    }

    fn mouse_interaction(
        &self,
        state: &Tree,
        layout: Layout<'_>,
        cursor_position: iced::Point,
        viewport: &iced::Rectangle,
        renderer: &Renderer,
    ) -> iced_native::mouse::Interaction {
        self.content().as_widget().mouse_interaction(
            state,
            layout,
            cursor_position,
            viewport,
            renderer,
        )
    }

    fn layout(
        &self,
        renderer: &Renderer,
        limits: &iced_native::layout::Limits,
    ) -> iced_native::layout::Node {
        self.content().as_widget().layout(renderer, limits)
    }

    fn draw(
        &self,
        state: &Tree,
        renderer: &mut Renderer,
        theme: &<Renderer as iced_native::Renderer>::Theme,
        style: &iced_native::renderer::Style,
        layout: iced_native::Layout<'_>,
        cursor_position: iced::Point,
        viewport: &iced::Rectangle,
    ) {
        self.content().as_widget().draw(
            &state.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor_position,
            viewport,
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct State {
    is_hovered: bool,
}

impl State {
    pub fn new() -> State {
        State::default()
    }
}

impl<'a, Message, Renderer> From<Hoverable<'a, Message, Renderer>>
    for Element<'a, Message, Renderer>
where
    Message: Clone + 'a,
    Renderer: iced_native::Renderer + 'a,
{
    fn from(hoverable: Hoverable<'a, Message, Renderer>) -> Self {
        Self::new(hoverable)
    }
}
