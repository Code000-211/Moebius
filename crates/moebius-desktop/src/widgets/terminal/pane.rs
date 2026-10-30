//! The pane around a terminal's screen: it tells the page its size and whether a click gave it focus, and it selects
//! with the mouse. A selection is copied when the button is released and again with Ctrl+Shift+C; Ctrl+Shift+V and a
//! right click paste. The selection is the pane's own state, so a drag needs no message per move.

use iced::advanced::widget::{Operation, Tree, tree};
use iced::advanced::{Clipboard, Layout, Renderer as _, Widget, clipboard, layout, mouse};
use iced::advanced::{overlay, renderer};
use iced::keyboard::{self, Key};
use iced::{Border, Element, Event, Length, Point, Rectangle, Size, Vector, window};

use crate::theme;

/// What the pane tells the page: its size when that changes, which is how it learns how many columns and rows fit
/// (a view cannot know), whether a click was on it or elsewhere, which is how it gains and loses focus, and text to
/// paste into the shell.
#[derive(Debug, Clone)]
pub enum PaneEvent {
    Resized(Size),
    Focus(bool),
    Paste(String),
}

/// A cell of the screen: its column and its row.
type Cell = (usize, usize);

/// What the mouse selected: the cell the drag began on and the one it reached.
#[derive(Default)]
struct Selection {
    from: Option<Cell>,
    to: Cell,
    dragging: bool,
}

impl Selection {
    /// The first and the last cell selected, in reading order; none while nothing is.
    fn span(&self) -> Option<(Cell, Cell)> {
        let from = self.from?;
        let (a, b) = ((from.1, from.0), (self.to.1, self.to.0));
        let (start, end) = if a <= b { (a, b) } else { (b, a) };
        Some(((start.1, start.0), (end.1, end.0)))
    }
}

/// The text of the cells from `start` to `end` (both included) of the screen's `lines`, row by row, without the
/// spaces at the ends of the rows.
pub(super) fn selected_text(lines: &[String], start: Cell, end: Cell) -> String {
    (start.1..=end.1.min(lines.len().saturating_sub(1)))
        .map(|row| {
            let chars: Vec<char> = lines[row].chars().collect();
            let first = if row == start.1 { start.0 } else { 0 };
            let last = if row == end.1 { end.0 + 1 } else { chars.len() };
            chars[first.min(chars.len())..last.min(chars.len())]
                .iter()
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Reports a [`PaneEvent`] for its content whenever the size differs from `known` or a click lands on the other side
/// of the pane's edge than `focused` says, and selects the text of `lines` on the screen, the content's second child.
pub(super) struct Pane<'a, M> {
    content: Element<'a, M>,
    known: Size,
    focused: bool,
    lines: Vec<String>,
    on_event: Box<dyn Fn(PaneEvent) -> M + 'a>,
}

impl<'a, M> Pane<'a, M> {
    pub(super) fn new(
        content: impl Into<Element<'a, M>>,
        known: Size,
        focused: bool,
        lines: Vec<String>,
        on_event: impl Fn(PaneEvent) -> M + 'a,
    ) -> Self {
        Pane {
            content: content.into(),
            known,
            focused,
            lines,
            on_event: Box::new(on_event),
        }
    }
}

/// Where the cells are drawn: the screen, inside its padding.
fn screen(layout: Layout<'_>) -> Rectangle {
    let screen = layout
        .children()
        .nth(1)
        .map_or(layout.bounds(), |child| child.bounds());
    screen.shrink(theme::SPACE_10)
}

/// The cell under `at` on `screen`, the nearest one when `at` is outside it.
fn cell(screen: Rectangle, at: Point) -> Cell {
    let [width, height] = theme::SHELL_CELL;
    let column = ((at.x - screen.x) / width).max(0.0) as usize;
    let row = ((at.y - screen.y) / height).max(0.0) as usize;
    (column, row)
}

impl<M> Widget<M, iced::Theme, iced::Renderer> for Pane<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<Selection>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(Selection::default())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut iced::advanced::Shell<'_, M>,
        viewport: &Rectangle,
    ) {
        if matches!(event, Event::Window(window::Event::RedrawRequested(_)))
            && layout.bounds().size() != self.known
        {
            shell.publish((self.on_event)(PaneEvent::Resized(layout.bounds().size())));
        }
        if let Event::Mouse(mouse::Event::ButtonPressed(_)) = event {
            let on_pane = cursor.is_over(layout.bounds());
            if on_pane != self.focused {
                shell.publish((self.on_event)(PaneEvent::Focus(on_pane)));
            }
        }
        let screen = screen(layout);
        let selection = tree.state.downcast_mut::<Selection>();
        let copy = |selection: &Selection, clipboard: &mut dyn Clipboard| {
            if let Some((start, end)) = selection.span() {
                let text = selected_text(&self.lines, start, end);
                if !text.is_empty() {
                    clipboard.write(clipboard::Kind::Standard, text);
                }
            }
        };
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                match cursor.position_over(screen) {
                    Some(at) => {
                        let at = cell(screen, at);
                        *selection = Selection {
                            from: Some(at),
                            to: at,
                            dragging: true,
                        };
                        shell.request_redraw();
                    }
                    None if selection.from.is_some() => {
                        *selection = Selection::default();
                        shell.request_redraw();
                    }
                    None => {}
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) if selection.dragging => {
                selection.to = cell(screen, *position);
                shell.request_redraw();
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                if selection.dragging =>
            {
                selection.dragging = false;
                // A click without a drag selects nothing.
                if selection.from == Some(selection.to) {
                    *selection = Selection::default();
                    shell.request_redraw();
                } else {
                    copy(selection, clipboard);
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right))
                if cursor.is_over(screen) =>
            {
                if let Some(text) = clipboard.read(clipboard::Kind::Standard) {
                    shell.publish((self.on_event)(PaneEvent::Paste(text)));
                }
                shell.capture_event();
            }
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: Key::Character(c),
                modifiers,
                ..
            }) if self.focused && modifiers.control() && modifiers.shift() => {
                match c.to_lowercase().as_str() {
                    "c" => copy(selection, clipboard),
                    "v" => {
                        if let Some(text) = clipboard.read(clipboard::Kind::Standard) {
                            shell.publish((self.on_event)(PaneEvent::Paste(text)));
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        if cursor.is_over(screen(layout)) {
            return mouse::Interaction::Text;
        }
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &iced::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
        let Some((start, end)) = tree.state.downcast_ref::<Selection>().span() else {
            return;
        };
        // The selection is a light wash over its cells, row by row; the rows between the first and the last are whole.
        let screen = screen(layout);
        let [width, height] = theme::SHELL_CELL;
        let columns = (screen.width / width).floor().max(1.0) as usize;
        for row in start.1..=end.1 {
            let first = if row == start.1 { start.0 } else { 0 };
            let last = if row == end.1 { end.0 + 1 } else { columns };
            if last <= first {
                continue;
            }
            let bounds = Rectangle {
                x: screen.x + first as f32 * width,
                y: screen.y + row as f32 * height,
                width: (last - first) as f32 * width,
                height,
            };
            if let Some(bounds) = bounds.intersection(&screen) {
                renderer.fill_quad(
                    renderer::Quad {
                        bounds,
                        border: Border::default(),
                        ..renderer::Quad::default()
                    },
                    theme::TERMINAL_SELECTION,
                );
            }
        }
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, M, iced::Theme, iced::Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a, M: 'a> From<Pane<'a, M>> for Element<'a, M> {
    fn from(pane: Pane<'a, M>) -> Self {
        Element::new(pane)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines() -> Vec<String> {
        ["$ cargo test", "running 3 tests", "ok"]
            .map(String::from)
            .to_vec()
    }

    #[test]
    fn a_selection_is_the_text_of_its_cells_in_reading_order() {
        assert_eq!(selected_text(&lines(), (2, 0), (6, 0)), "cargo");
        assert_eq!(selected_text(&lines(), (8, 0), (6, 1)), "test\nrunning");
        // Past the end of a row there is nothing, and a row past the screen is not there.
        assert_eq!(selected_text(&lines(), (0, 2), (40, 5)), "ok");
        let backwards = Selection {
            from: Some((6, 1)),
            to: (8, 0),
            dragging: false,
        };
        assert_eq!(backwards.span(), Some(((8, 0), (6, 1))));
    }
}
