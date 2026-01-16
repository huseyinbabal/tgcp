use crate::app::App;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

pub fn render(f: &mut Frame, app: &App) {
    let area = centered_rect(50, 40, f.area());

    f.render_widget(Clear, area);

    // Split area into input box and suggestions
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Input box
            Constraint::Min(1),    // Suggestions list
        ])
        .split(area);

    // Input box - show total resource count
    let total_count = app.get_available_commands().len();
    let title = format!(" Resource Types ({}) ", total_count);
    let input_block = Block::default()
        .title(title)
        .title_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    // Build input with ghost text preview
    let input_line = if let Some(preview) = &app.command_preview {
        // Show typed text in white, remaining preview in dark gray (ghost)
        let typed = &app.command_text;
        if preview.starts_with(typed) && preview.len() > typed.len() {
            let ghost_part = &preview[typed.len()..];
            Line::from(vec![
                Span::raw("> "),
                Span::styled(typed, Style::default().fg(Color::White)),
                Span::styled(ghost_part, Style::default().fg(Color::DarkGray)),
            ])
        } else {
            // Preview doesn't match typed text, just show typed
            Line::from(vec![
                Span::raw("> "),
                Span::styled(typed, Style::default().fg(Color::White)),
            ])
        }
    } else {
        Line::from(vec![
            Span::raw("> "),
            Span::styled(&app.command_text, Style::default().fg(Color::White)),
        ])
    };

    let input = Paragraph::new(input_line).block(input_block);

    f.render_widget(input, chunks[0]);

    // Suggestions list with scroll
    let suggestions_block = Block::default()
        .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
        .border_style(Style::default().fg(Color::Cyan));

    let inner_height = suggestions_block.inner(chunks[1]).height as usize;

    // Calculate scroll offset to keep selected item visible
    let scroll_offset = if app.command_suggestions.is_empty() {
        0
    } else if app.command_suggestion_selected >= inner_height {
        // Selected item is below visible area, scroll down
        app.command_suggestion_selected - inner_height + 1
    } else {
        0
    };

    let suggestion_lines: Vec<Line> = app
        .command_suggestions
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(inner_height)
        .map(|(i, suggestion)| {
            let style = if i == app.command_suggestion_selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            Line::from(vec![Span::raw("  "), Span::styled(suggestion, style)])
        })
        .collect();

    let suggestions = Paragraph::new(suggestion_lines).block(suggestions_block);

    f.render_widget(suggestions, chunks[1]);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
