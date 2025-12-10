//! File picker widget with multi-select checkboxes

use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, List, ListItem},
};

use crate::tui::app::App;

/// Draw the file picker overlay
pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    let Some(ref picker) = app.file_picker else {
        return;
    };

    // Calculate popup size
    let max_file_len = picker.files.iter().map(|f| f.len()).max().unwrap_or(20);
    let popup_width = (max_file_len + 10).min(60).max(30) as u16;
    let popup_height = (picker.files.len() + 4).min(20) as u16;

    let popup_area = Rect {
        x: (area.width.saturating_sub(popup_width)) / 2,
        y: (area.height.saturating_sub(popup_height)) / 2,
        width: popup_width,
        height: popup_height,
    };

    // Clear background
    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(" Select Files (Space=toggle, Enter=open, a=all) ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner_area = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    // Build list items with checkboxes
    let items: Vec<ListItem> = picker
        .files
        .iter()
        .enumerate()
        .map(|(i, file)| {
            let is_selected = picker.selected[i];
            let is_cursor = i == picker.cursor;

            let checkbox = if is_selected { "[x]" } else { "[ ]" };

            let style = if is_cursor {
                Style::default().bg(Color::DarkGray).fg(Color::White)
            } else if is_selected {
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::White)
            };

            ListItem::new(Line::from(vec![
                Span::styled(
                    checkbox,
                    if is_selected {
                        Style::default().fg(Color::Green)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    },
                ),
                Span::raw(" "),
                Span::styled(file, style),
            ]))
        })
        .collect();

    let list = List::new(items);
    frame.render_widget(list, inner_area);
}
