use crate::safety;
use ratatui::{buffer::CellWidth, style::Style, text::Text};

/// Make graphemes Ratatui drops visible before the final secret check.
pub(crate) fn display(mut text: Text<'static>, secrets: &[&str]) -> Text<'static> {
    for span in text.lines.iter_mut().flat_map(|line| &mut line.spans) {
        if span.content.contains('\t') {
            span.content = span.content.replace('\t', "⇥").into();
        }
        let mut visible = String::new();
        for grapheme in span.styled_graphemes(Style::default()) {
            if grapheme.symbol.cell_width() == 0 {
                for ch in grapheme.symbol.chars() {
                    visible.extend(ch.escape_unicode());
                }
            } else {
                visible.push_str(grapheme.symbol);
            }
        }
        span.content = visible.into();
    }
    let projection = text.to_string();
    let safe = safety::text(&projection, secrets);
    if safe == projection {
        text
    } else {
        Text::from(safe)
    }
}
