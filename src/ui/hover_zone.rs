#![allow(unused)]

use iced::event;
use iced::{Element, Length, Padding};
use iced_lazy::lazy;
use iced_native::widget::{tree, Operation, Tree};
use iced_native::{layout, Layout, Widget};

pub struct HoverZone<'a, Message, Renderer>
where
    Renderer: iced_native::Renderer,
{
    view: Box<dyn Fn(bool) -> Element<'a, Message, Renderer> + 'a>,
    width: Length,
    height: Length,
    content: Element<'a, Message, Renderer>,
}

impl<'a, Message, Renderer> HoverZone<'a, Message, Renderer>
where
    Renderer: iced_native::Renderer,
{
    pub fn new(view: impl Fn(bool) -> Element<'a, Message, Renderer> + 'a) -> Self {
        let content = view(false); // NOTE this matches with State::default(), below

        Self {
            view: Box::new(view),
            width: Length::Shrink,
            height: Length::Shrink,
            content,
        }
    }

    pub fn width(mut self, width: Length) -> Self {
        self.width = width;
        self
    }

    pub fn height(mut self, height: Length) -> Self {
        self.height = height;
        self
    }
}

impl<'a, Message, Renderer> Widget<Message, Renderer> for HoverZone<'a, Message, Renderer>
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
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content))
    }

    fn width(&self) -> Length {
        self.width
    }

    fn height(&self) -> Length {
        self.height
    }

    fn on_event(
        &mut self,
        tree: &mut Tree,
        _event: iced::Event,
        layout: iced_native::Layout<'_>,
        cursor_position: iced::Point,
        _renderer: &Renderer,
        _clipboard: &mut dyn iced_native::Clipboard,
        _shell: &mut iced_native::Shell<'_, Message>,
    ) -> event::Status {
        let state = tree.state.downcast_mut::<State>();
        let now_hovered = layout.bounds().contains(cursor_position);

        if state.is_hovered != now_hovered {
            state.is_hovered = now_hovered;
            // TODO would this result in it not being
            // updated when hover state doesn't change?
            // or would that be solved by passing a new view?
            // or returning a new HoverZone
            self.content = (self.view)(state.is_hovered);
        }

        event::Status::Ignored
    }

    fn layout(
        &self,
        renderer: &Renderer,
        limits: &iced_native::layout::Limits,
    ) -> iced_native::layout::Node {
        let limits = limits.width(self.width).height(self.height);
        let content = self.content.as_widget().layout(renderer, &limits);
        let size = limits.resolve(content.size());

        layout::Node::with_children(size, vec![content])
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
        let bounds = layout.bounds();
        let content_layout = layout.children().next().unwrap();

        self.content.as_widget().draw(
            &state.children[0],
            renderer,
            theme,
            style,
            content_layout,
            cursor_position,
            &bounds,
        );
    }

    fn operate(&self, tree: &mut Tree, layout: Layout<'_>, operation: &mut dyn Operation<Message>) {
        operation.container(None, &mut |operation| {
            self.content.as_widget().operate(
                &mut tree.children[0],
                layout.children().next().unwrap(),
                operation,
            );
        });
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
