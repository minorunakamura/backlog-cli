use crossterm::event::Event;
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span, Text},
    widgets::Paragraph,
};
use tui_input::{Input, InputRequest, backend::crossterm::EventHandler};

/// Shared single-line editing state; rendering never changes the original value.
pub struct SingleLineInput {
    input: Input,
}
impl SingleLineInput {
    pub fn new(value: String) -> Self {
        Self {
            input: Input::new(value),
        }
    }
    pub fn value(&self) -> &str {
        self.input.value()
    }
    /// Call only for the focused field, with the current terminal area.
    /// Handle Ctrl+C first; do not route the field's remaining keys to global actions.
    pub fn event(&mut self, event: &Event, screen: Rect) {
        if screen.width < 40 || screen.height < 12 {
            return;
        }
        let changed = if let Event::Paste(value) = event {
            let value = value.replace("\r\n", " ").replace(['\r', '\n', '\t'], " ");
            let mut original = self.value().to_owned();
            let cursor = self.input.cursor();
            let byte = original
                .char_indices()
                .nth(cursor)
                .map_or(original.len(), |(byte, _)| byte);
            original.insert_str(byte, &value);
            self.input = std::mem::take(&mut self.input)
                .with_value(original)
                .with_cursor(cursor + value.chars().count());
            true
        } else {
            self.input.handle_event(event).is_some()
        };
        if changed && self.input.cursor() > 0 {
            // Adjacent text can join after editing; snap to its end without changing yank/cut state.
            let mut boundary = self.input.clone();
            boundary.handle(InputRequest::GoToPrevChar);
            boundary.handle(InputRequest::GoToNextChar);
            self.input = std::mem::take(&mut self.input).with_cursor(boundary.cursor());
        }
    }
    pub fn draw(&self, frame: &mut Frame<'_>, area: Rect, focused: bool, secrets: &[&str]) {
        let screen = frame.area();
        let area = area.intersection(screen);
        if screen.width < 40 || screen.height < 12 || area.is_empty() {
            return;
        }
        // Mask any display transformation: raw cursor offsets must not address a changed value.
        if self.value().chars().any(char::is_control)
            || crate::tui_text::display(Text::from(self.value().to_owned()), secrets).to_string()
                != self.value()
        {
            frame.render_widget(
                Paragraph::new(crate::safety::text("[REDACTED]", secrets)),
                area,
            );
            if focused {
                frame.set_cursor_position((area.x, area.y));
            }
            return;
        }
        let scroll = self
            .input
            .visual_scroll(usize::from(area.width.saturating_sub(1)));
        // Paragraph's scroll offset is u16; slice at grapheme boundaries for longer values too.
        let line = Line::from(self.value().to_owned());
        let mut columns = 0;
        let visible: String = line
            .styled_graphemes(Style::default())
            .filter_map(|grapheme| {
                let start = columns;
                columns += Span::raw(grapheme.symbol).width();
                (start >= scroll).then_some(grapheme.symbol)
            })
            .collect();
        frame.render_widget(Paragraph::new(visible), area);
        if focused {
            frame.set_cursor_position((
                area.x + (self.input.visual_cursor() - scroll) as u16,
                area.y,
            ));
        }
    }
}
