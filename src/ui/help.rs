use crate::app::App;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

pub fn render(f: &mut Frame, _app: &App) {
    let area = centered_rect(60, 70, f.size());

    f.render_widget(Clear, area);

    let help_text = vec![
        Line::from(""),
        create_section("Navigation"),
        create_key_line("j / Down", "Move down"),
        create_key_line("k / Up", "Move up"),
        create_key_line("gg", "Go to top"),
        create_key_line("G", "Go to bottom"),
        Line::from(""),
        create_section("Views"),
        create_key_line("d / Enter", "Describe item"),
        create_key_line("?", "Toggle help"),
        Line::from(""),
        create_section("Actions"),
        create_key_line("s", "Start instance"),
        create_key_line("x", "Stop instance"),
        create_key_line("Ctrl+d", "Delete (destructive)"),
        Line::from(""),
        create_section("Auto-refresh"),
        create_key_line("", "List refreshes every 5s"),
        Line::from(""),
        create_section("Modes"),
        create_key_line("/", "Filter mode"),
        create_key_line(":", "Resources mode"),
        Line::from(""),
        create_section("Resources"),
        create_key_line(":vm-instances", "Compute Engine VMs"),
        create_key_line(":gke-clusters", "GKE clusters"),
        create_key_line(":buckets", "Cloud Storage"),
        create_key_line(":sql-instances", "Cloud SQL"),
        create_key_line(":functions", "Cloud Functions"),
        create_key_line(":cloudrun-services", "Cloud Run"),
        create_key_line(":secrets", "Secret Manager"),
        create_key_line(":pubsub-topics", "Pub/Sub topics"),
        create_key_line(":networks", "VPC networks"),
        create_key_line(":service-accounts", "IAM accounts"),
        Line::from(""),
        create_section("Navigation"),
        create_key_line(":projects", "Select project"),
        create_key_line(":zones", "Select zone"),
        Line::from(""),
        create_key_line("Esc", "Close / Cancel"),
        create_key_line("Ctrl+c", "Quit application"),
    ];

    let block = Block::default()
        .title(" Help ")
        .title_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let paragraph = Paragraph::new(help_text).block(block);

    f.render_widget(paragraph, area);
}

fn create_section(title: &str) -> Line<'_> {
    Line::from(vec![Span::styled(
        format!("  {} ", title),
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )])
}

fn create_key_line<'a>(key: &'a str, description: &'a str) -> Line<'a> {
    Line::from(vec![
        Span::raw("    "),
        Span::styled(
            format!("{:>15}", key),
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(description, Style::default().fg(Color::White)),
    ])
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
