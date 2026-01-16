use crate::app::App;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState},
    Frame,
};

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    // Create bordered box with centered title
    let title = format!(" Select Project[{}] ", app.available_projects.len());
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(Span::styled(
            title,
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ))
        .title_alignment(Alignment::Center);

    let inner_area = block.inner(area);
    f.render_widget(block, area);

    // Split into help text and table
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(1)])
        .split(inner_area);

    // Help text
    let help_text = if app.project.is_empty() {
        Line::from(vec![
            Span::styled(
                " Select a project to get started. ",
                Style::default().fg(Color::Yellow),
            ),
            Span::styled(
                "Press Enter to select, Esc to cancel.",
                Style::default().fg(Color::DarkGray),
            ),
        ])
    } else {
        Line::from(vec![
            Span::styled(" Current: ", Style::default().fg(Color::DarkGray)),
            Span::styled(&app.project, Style::default().fg(Color::Green)),
            Span::styled(
                " | Enter: select, Esc: cancel",
                Style::default().fg(Color::DarkGray),
            ),
        ])
    };
    let help = Paragraph::new(help_text);
    f.render_widget(help, chunks[0]);

    // Table header
    let header_cells = [" PROJECT"].iter().map(|h| {
        Cell::from(*h).style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
    });

    let header = Row::new(header_cells).height(1);

    // Table rows
    let rows = app.available_projects.iter().map(|project| {
        let style = if project == &app.project {
            Style::default().fg(Color::Green)
        } else {
            Style::default()
        };

        let marker = if project == &app.project {
            " ● "
        } else {
            "   "
        };

        Row::new(vec![
            Cell::from(format!("{}{}", marker, project)).style(style)
        ])
    });

    let widths = [Constraint::Percentage(100)];

    let table = Table::new(rows, widths).header(header).row_highlight_style(
        Style::default()
            .bg(Color::DarkGray)
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    );

    let mut state = TableState::default();
    state.select(Some(app.projects_selected));

    f.render_stateful_widget(table, chunks[1], &mut state);
}
