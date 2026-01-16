use crate::app::{App, Mode};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

pub fn render(f: &mut Frame, app: &App) {
    match app.mode {
        Mode::Confirm => render_confirm_dialog(f, app),
        Mode::Warning => render_warning_dialog(f, app),
        _ => {}
    }
}

fn render_confirm_dialog(f: &mut Frame, app: &App) {
    let Some(pending) = &app.pending_action else {
        return;
    };

    let area = centered_rect(60, 9, f.area());

    f.render_widget(Clear, area);

    // Determine title color based on destructive flag
    let title_color = if pending.destructive {
        Color::Red
    } else {
        Color::Yellow
    };

    let title = if pending.destructive {
        "Delete"
    } else {
        "Confirm"
    };

    // Build Cancel/OK buttons with selection indicator
    let cancel_style = if !pending.selected_yes {
        Style::default().fg(Color::Black).bg(Color::Magenta)
    } else {
        Style::default().fg(Color::White)
    };

    let ok_style = if pending.selected_yes {
        Style::default().fg(Color::Black).bg(Color::Magenta)
    } else {
        Style::default().fg(Color::White)
    };

    // Build the dialog content
    let text = vec![
        Line::from(Span::styled(
            format!("<{}>", title),
            Style::default()
                .fg(title_color)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            &pending.message,
            Style::default().fg(Color::White),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(" Cancel ", cancel_style),
            Span::raw("    "),
            Span::styled(" OK ", ok_style),
        ]),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    let paragraph = Paragraph::new(text)
        .block(block)
        .alignment(Alignment::Center);

    f.render_widget(paragraph, area);
}

fn render_warning_dialog(f: &mut Frame, app: &App) {
    // Use warning_message or error
    let (title, message, title_color, border_color) = if let Some(msg) = &app.warning_message {
        ("Warning", msg.clone(), Color::Yellow, Color::Yellow)
    } else if let Some(err) = &app.error {
        ("Error", err.clone(), Color::Red, Color::Red)
    } else {
        return;
    };

    // Calculate height based on message length (wrap at ~50 chars)
    let lines = (message.len() / 50) + 1;
    let height = (6 + lines).min(15) as u16;

    let area = centered_rect(70, height, f.area());

    f.render_widget(Clear, area);

    // Wrap long messages
    let wrapped_lines: Vec<Line> = wrap_text(&message, 60)
        .into_iter()
        .map(|line| Line::from(Span::styled(line, Style::default().fg(Color::White))))
        .collect();

    let mut text = vec![
        Line::from(Span::styled(
            format!(" {} ", title),
            Style::default()
                .fg(Color::Black)
                .bg(title_color)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];

    text.extend(wrapped_lines);
    text.push(Line::from(""));
    text.push(Line::from(vec![Span::styled(
        " OK (Enter/Esc) ",
        Style::default().fg(Color::Black).bg(Color::Magenta),
    )]));

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    let paragraph = Paragraph::new(text)
        .block(block)
        .alignment(Alignment::Center);

    f.render_widget(paragraph, area);
}

/// Wrap text to fit within a given width
fn wrap_text(text: &str, max_width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current_line = String::new();

    for word in text.split_whitespace() {
        if current_line.is_empty() {
            current_line = word.to_string();
        } else if current_line.len() + 1 + word.len() <= max_width {
            current_line.push(' ');
            current_line.push_str(word);
        } else {
            lines.push(current_line);
            current_line = word.to_string();
        }
    }

    if !current_line.is_empty() {
        lines.push(current_line);
    }

    if lines.is_empty() {
        lines.push(String::new());
    }

    lines
}

fn centered_rect(percent_x: u16, height: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(40),
            Constraint::Length(height),
            Constraint::Percentage(40),
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
