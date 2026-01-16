use crate::app::App;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    // Split header into 5 columns like k9s
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(22), // Left: Context info
            Constraint::Percentage(18), // Zone/Sub-resource shortcuts
            Constraint::Percentage(22), // Keybindings col 1
            Constraint::Percentage(22), // Keybindings col 2
            Constraint::Percentage(16), // Logo
        ])
        .split(area);

    render_context_column(f, app, columns[0]);
    render_shortcuts_column(f, app, columns[1]);
    render_keybindings_col1(f, app, columns[2]);
    render_keybindings_col2(f, app, columns[3]);
    render_logo(f, columns[4]);
}

fn render_context_column(f: &mut Frame, app: &App, area: Rect) {
    let resource_name = app
        .current_resource()
        .map(|r| r.display_name.as_str())
        .unwrap_or(&app.resource_key);

    let mut lines = vec![
        Line::from(vec![
            Span::styled("Project:", Style::default().fg(Color::DarkGray)),
            Span::raw(" "),
            Span::styled(
                &app.project,
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("Zone:   ", Style::default().fg(Color::DarkGray)),
            Span::raw(" "),
            Span::styled(
                &app.zone,
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("Resource:", Style::default().fg(Color::DarkGray)),
            Span::raw(" "),
            Span::styled(
                resource_name.to_string(),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
    ];

    // Show parent context if navigating
    if let Some(parent) = &app.parent_context {
        lines.push(Line::from(vec![
            Span::styled("Context:", Style::default().fg(Color::DarkGray)),
            Span::raw(" "),
            Span::styled(&parent.display_name, Style::default().fg(Color::Yellow)),
        ]));
    }

    // Show read-only mode indicator
    if app.readonly {
        lines.push(Line::from(vec![
            Span::styled("Mode:    ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                "READONLY",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
    }

    let paragraph = Paragraph::new(lines);
    f.render_widget(paragraph, area);
}

fn render_shortcuts_column(f: &mut Frame, app: &App, area: Rect) {
    // If current resource has sub-resources, show those as shortcuts
    // Otherwise show zone shortcuts
    if let Some(resource) = app.current_resource() {
        if !resource.sub_resources.is_empty() {
            render_subresource_shortcuts(f, resource, area);
            return;
        }
    }

    render_zone_shortcuts(f, app, area);
}

fn render_zone_shortcuts(f: &mut Frame, app: &App, area: Rect) {
    let zones = [
        ("0", "us-central1-a"),
        ("1", "us-east1-b"),
        ("2", "us-west1-a"),
        ("3", "europe-west1-b"),
        ("4", "asia-east1-a"),
        ("5", "asia-northeast1-a"),
    ];

    let lines: Vec<Line> = zones
        .iter()
        .map(|(key, zone)| {
            let is_current = *zone == app.zone;
            let style = if is_current {
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            Line::from(vec![
                Span::styled(format!("<{}>", key), Style::default().fg(Color::Yellow)),
                Span::raw(" "),
                Span::styled(*zone, style),
            ])
        })
        .collect();

    let paragraph = Paragraph::new(lines);
    f.render_widget(paragraph, area);
}

fn render_subresource_shortcuts(
    f: &mut Frame,
    resource: &crate::resource::registry::ResourceDef,
    area: Rect,
) {
    let mut lines: Vec<Line> = vec![Line::from(Span::styled(
        "Sub-resources:",
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    ))];

    for sub in resource.sub_resources.iter().take(5) {
        lines.push(Line::from(vec![
            Span::styled(
                format!("<{}>", sub.shortcut),
                Style::default().fg(Color::Yellow),
            ),
            Span::raw(" "),
            Span::styled(sub.display_name.clone(), Style::default().fg(Color::White)),
        ]));
    }

    // Show if there are more
    if resource.sub_resources.len() > 5 {
        lines.push(Line::from(Span::styled(
            format!("  +{} more", resource.sub_resources.len() - 5),
            Style::default().fg(Color::DarkGray),
        )));
    }

    let paragraph = Paragraph::new(lines);
    f.render_widget(paragraph, area);
}

fn render_keybindings_col1(f: &mut Frame, app: &App, area: Rect) {
    // Show resource-specific actions (first half) or generic bindings
    let bindings: Vec<(String, String)> = if let Some(resource) = app.current_resource() {
        let mut b: Vec<(String, String)> = vec![("<d>".to_string(), "Describe".to_string())];

        // Get all actions with shortcuts (skip if shortcut conflicts with 'd')
        let actions: Vec<_> = resource
            .actions
            .iter()
            .filter_map(|a| {
                a.shortcut.as_ref().and_then(|s| {
                    if s != "d" {
                        Some((format!("<{}>", s), a.display_name.clone()))
                    } else {
                        None
                    }
                })
            })
            .collect();

        // Take first half of actions for column 1
        let half = actions.len().div_ceil(2);
        b.extend(actions.into_iter().take(half));
        b.push(("<?>".to_string(), "Help".to_string()));
        b
    } else {
        vec![
            ("<d>".to_string(), "Describe".to_string()),
            ("<?>".to_string(), "Help".to_string()),
        ]
    };

    let lines: Vec<Line> = bindings
        .iter()
        .take(6) // Max 6 lines
        .map(|(key, desc)| {
            Line::from(vec![
                Span::styled(format!("{:<9}", key), Style::default().fg(Color::Yellow)),
                Span::styled(desc.clone(), Style::default().fg(Color::DarkGray)),
            ])
        })
        .collect();

    let paragraph = Paragraph::new(lines);
    f.render_widget(paragraph, area);
}

fn render_keybindings_col2(f: &mut Frame, app: &App, area: Rect) {
    // Show second half of resource actions + navigation bindings
    let mut bindings: Vec<(String, String)> = Vec::new();

    // Add second half of resource actions
    if let Some(resource) = app.current_resource() {
        let actions: Vec<_> = resource
            .actions
            .iter()
            .filter_map(|a| {
                a.shortcut.as_ref().and_then(|s| {
                    if s != "d" {
                        Some((format!("<{}>", s), a.display_name.clone()))
                    } else {
                        None
                    }
                })
            })
            .collect();

        // Skip first half, take second half
        let half = actions.len().div_ceil(2);
        bindings.extend(actions.into_iter().skip(half));
    }

    // Add navigation bindings
    bindings.push(("</>".to_string(), "Filter".to_string()));
    bindings.push(("<:>".to_string(), "Resources".to_string()));
    bindings.push(("<bs>".to_string(), "Parent".to_string()));
    bindings.push(("<ctrl-c>".to_string(), "Quit".to_string()));

    let lines: Vec<Line> = bindings
        .iter()
        .take(6) // Max 6 lines
        .map(|(key, desc)| {
            Line::from(vec![
                Span::styled(format!("{:<9}", key), Style::default().fg(Color::Yellow)),
                Span::styled(desc.clone(), Style::default().fg(Color::DarkGray)),
            ])
        })
        .collect();

    let paragraph = Paragraph::new(lines);
    f.render_widget(paragraph, area);
}

fn render_logo(f: &mut Frame, area: Rect) {
    let logo = vec![
        Line::from(Span::styled(
            "▀█▀ █▀▀ █▀▀ █▀█",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            " █  █▄█ █▄▄ █▀▀",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "GCP TUI",
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(Span::styled(
            crate::VERSION,
            Style::default().fg(Color::DarkGray),
        )),
    ];

    let paragraph = Paragraph::new(logo);
    f.render_widget(paragraph, area);
}
