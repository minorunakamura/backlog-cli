use super::detail::Detail;
use crate::safety;
use crossterm::event::KeyCode;
use ratatui::{
    style::Style,
    text::{Line, Span, Text},
};

#[derive(Clone, Copy)]
pub(super) enum Direction {
    Latest,
    Oldest,
    Older,
    Newer,
}
pub(super) struct Entry {
    pub(super) id: String,
    pub(super) preview: crate::comment::Preview,
}
#[derive(Default)]
pub(super) struct Comment {
    pub(super) document: Detail,
    items: Vec<Entry>,
    selected: usize,
    older_end: bool,
    newer_end: bool,
}
impl Comment {
    pub(super) fn select(&mut self, key: Option<String>) {
        *self = Self::default();
        self.document.select(key);
    }
    pub(super) fn projection(
        &self,
        width: u16,
        no_color: bool,
        secret: &str,
    ) -> (String, String, Option<Text<'static>>) {
        let Some(item) = self.items.get(self.selected) else {
            return (String::new(), String::new(), None);
        };
        let preview = &item.preview;
        let single_author = crate::tui_text::display(
            Text::from(preview.author.replace(['\n', '\t'], " ")),
            &[secret],
        )
        .to_string();
        let truncated = Line::from(single_author.as_str()).width() > width as usize;
        let author = if truncated {
            let mut short = String::new();
            let mut columns = 0;
            let span = Span::raw(single_author.as_str());
            for grapheme in span.styled_graphemes(Style::default()) {
                let grapheme_width = Span::raw(grapheme.symbol).width();
                if columns + grapheme_width + 1 > width as usize {
                    break;
                }
                short.push_str(grapheme.symbol);
                columns += grapheme_width;
            }
            if width > 0 {
                short.push('…');
            }
            short
        } else {
            single_author
        };
        let created =
            crate::tui_text::display(Text::from(preview.created.clone()), &[secret]).to_string();
        let date_overflow = preview.created.contains('\n')
            || preview.created.contains('\t')
            || Line::from(created.as_str()).width() > width as usize;
        let created = if date_overflow {
            "変換不可（原値は本文）".to_owned()
        } else {
            created
        };
        let mut text = if no_color {
            preview.no_color_text.clone()
        } else {
            preview.text.clone()
        };
        let mut prefix = Vec::new();
        if truncated {
            prefix.extend(
                preview
                    .author
                    .split('\n')
                    .map(|line| Line::from(line.to_owned())),
            );
            prefix.push(Line::default());
        }
        if date_overflow {
            prefix.extend(
                preview
                    .created
                    .split('\n')
                    .map(|line| Line::from(line.to_owned())),
            );
            prefix.push(Line::default());
        }
        prefix.append(&mut text.lines);
        text.lines = prefix;
        let text = crate::tui_text::display(text, &[secret]);
        let projection = format!("{author}\n{created}\n\n{text}");
        let safe = safety::text(&projection, &[secret]);
        if safe != projection {
            return (String::new(), String::new(), Some(Text::from(safe)));
        }
        (author, created, Some(text))
    }
    pub(super) fn selected_id(&self) -> Option<String> {
        self.items.get(self.selected).map(|item| item.id.clone())
    }
    pub(super) fn update(&mut self, id: &str, preview: crate::comment::Preview) {
        if let Some(item) = self.items.iter_mut().find(|item| item.id == id) {
            item.preview = preview;
            self.show();
        }
    }
    pub(super) fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
    fn show(&mut self) {
        self.document
            .replace(self.items[self.selected].preview.text.clone());
        self.document.notice.clear();
    }
    pub(super) fn replace(&mut self, mut items: Vec<Entry>, direction: Direction) {
        if matches!(direction, Direction::Oldest | Direction::Newer) {
            items.reverse();
        }
        if items.is_empty() && matches!(direction, Direction::Older | Direction::Newer) {
            match direction {
                Direction::Older => self.older_end = true,
                Direction::Newer => self.newer_end = true,
                _ => unreachable!(),
            }
            self.document.notice = "No more comments".into();
            return;
        }
        self.older_end = matches!(direction, Direction::Oldest);
        self.newer_end = matches!(direction, Direction::Latest);
        self.selected = if matches!(direction, Direction::Oldest | Direction::Newer) {
            items.len().saturating_sub(1)
        } else {
            0
        };
        self.items = items;
        if self.items.is_empty() {
            self.document.replace("No comments".into());
        } else {
            self.show();
        }
    }
    pub(super) fn navigate(&mut self, code: KeyCode) -> Option<(Direction, Option<String>)> {
        match code {
            KeyCode::Char('g' | 'R') => Some((Direction::Latest, None)),
            KeyCode::Char('G') => Some((Direction::Oldest, None)),
            KeyCode::Char('n') if self.selected + 1 < self.items.len() => {
                self.selected += 1;
                self.show();
                None
            }
            KeyCode::Char('p') if self.selected > 0 => {
                self.selected -= 1;
                self.show();
                None
            }
            KeyCode::Char('n') if !self.older_end => self
                .items
                .last()
                .map(|item| (Direction::Older, Some(item.id.clone()))),
            KeyCode::Char('p') if !self.newer_end => self
                .items
                .first()
                .map(|item| (Direction::Newer, Some(item.id.clone()))),
            _ => None,
        }
    }
}
