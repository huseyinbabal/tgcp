use std::path::PathBuf;
use std::{io, time::Duration};

use anyhow::Result;
use clap::{Parser, ValueEnum};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use tracing::Level;
use tracing_subscriber::fmt::writer::MakeWriterExt;

mod app;
mod config;
mod gcp;
mod resource;
mod ui;

use app::{App, Mode};
use config::Config;

/// Version injected at compile time via TGCP_VERSION env var (set by CI/CD),
/// or falls back to Cargo.toml version for local builds.
pub const VERSION: &str = match option_env!("TGCP_VERSION") {
    Some(v) => v,
    None => env!("CARGO_PKG_VERSION"),
};

/// Terminal UI for GCP
#[derive(Parser, Debug)]
#[command(name = "tgcp", version = VERSION, about, long_about = None)]
struct Args {
    /// GCP zone to use (default: us-central1-a)
    #[arg(short, long)]
    zone: Option<String>,

    /// Log level for debugging (logs to ~/.config/tgcp/tgcp.log)
    #[arg(long, value_enum, default_value = "off")]
    log_level: LogLevel,

    /// Run in read-only mode (block all write operations)
    #[arg(long)]
    readonly: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum LogLevel {
    Off,
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl LogLevel {
    fn to_tracing_level(self) -> Option<Level> {
        match self {
            LogLevel::Off => None,
            LogLevel::Error => Some(Level::ERROR),
            LogLevel::Warn => Some(Level::WARN),
            LogLevel::Info => Some(Level::INFO),
            LogLevel::Debug => Some(Level::DEBUG),
            LogLevel::Trace => Some(Level::TRACE),
        }
    }
}

fn setup_logging(level: LogLevel) -> Option<tracing_appender::non_blocking::WorkerGuard> {
    let Some(tracing_level) = level.to_tracing_level() else {
        return None;
    };

    // Get log file path
    let log_path = get_log_path();

    // Ensure parent directory exists
    if let Some(parent) = log_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    // Create file appender
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .expect("Failed to open log file");

    let (non_blocking, guard) = tracing_appender::non_blocking(file);

    tracing_subscriber::fmt()
        .with_max_level(tracing_level)
        .with_writer(non_blocking.with_max_level(tracing_level))
        .with_ansi(false)
        .with_target(true)
        .with_thread_ids(false)
        .with_file(true)
        .with_line_number(true)
        .init();

    tracing::info!("tgcp started with log level: {:?}", level);
    tracing::info!("Log file: {:?}", log_path);

    Some(guard)
}

fn get_log_path() -> PathBuf {
    if let Some(config_dir) = dirs::config_dir() {
        return config_dir.join("tgcp").join("tgcp.log");
    }
    if let Some(home) = dirs::home_dir() {
        return home.join(".tgcp").join("tgcp.log");
    }
    PathBuf::from("tgcp.log")
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse CLI arguments
    let args = Args::parse();

    // Setup logging (keep guard alive for the duration of the program)
    let _log_guard = setup_logging(args.log_level);

    tracing::info!("Starting tgcp v{}", VERSION);
    tracing::debug!("CLI args: {:?}", args);

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Show splash screen while initializing
    let mut splash = ui::splash::SplashState::new();

    // Render initial splash
    terminal.draw(|f| ui::splash::render(f, &splash))?;

    splash.set_message("Loading configuration...");
    splash.complete_step();
    terminal.draw(|f| ui::splash::render(f, &splash))?;

    // Load config
    let config = Config::load();
    tracing::info!("Config loaded: project={:?}, zone={:?}", config.project, config.zone);

    // Determine effective zone: CLI arg > config > default
    let effective_zone = args.zone.or_else(|| Some(config.effective_zone()));
    let effective_project = config.effective_project();

    // Create app
    splash.set_message("Connecting to GCP...");
    splash.complete_step();
    terminal.draw(|f| ui::splash::render(f, &splash))?;

    tracing::info!("Creating GCP client...");
    let mut app = match App::new(effective_zone, effective_project, config, args.readonly).await {
        Ok(app) => {
            tracing::info!("GCP client created successfully");
            tracing::info!("Project: {}, Zone: {}", app.project, app.zone);
            app
        }
        Err(e) => {
            tracing::error!("Failed to create GCP client: {}", e);
            // Restore terminal before returning error
            disable_raw_mode()?;
            execute!(
                terminal.backend_mut(),
                LeaveAlternateScreen,
                DisableMouseCapture
            )?;
            terminal.show_cursor()?;
            return Err(e);
        }
    };

    splash.set_message("Fetching resources...");
    splash.complete_step();
    terminal.draw(|f| ui::splash::render(f, &splash))?;

    // Initial fetch (only if project is set)
    if app.has_project() {
        tracing::info!("Fetching initial resources...");
        app.refresh().await;
        tracing::info!("Initial fetch complete, {} items", app.items.len());
    } else {
        tracing::info!("No project set, will show project selector");
    }

    splash.set_message("Ready!");
    splash.complete_step();
    splash.complete_step();
    terminal.draw(|f| ui::splash::render(f, &splash))?;

    // Small delay to show completion
    std::thread::sleep(Duration::from_millis(200));

    // Main event loop
    let result = run_app(&mut terminal, &mut app).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    tracing::info!("tgcp shutdown");
    result
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<()> {
    loop {
        terminal.draw(|f| ui::render(f, app))?;

        // Auto-refresh check
        if app.needs_refresh() {
            tracing::debug!("Auto-refresh triggered");
            app.refresh().await;
        }

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                tracing::trace!("Key event: {:?}", key);
                // Handle key events based on current mode
                match app.mode {
                    Mode::Normal => {
                        if handle_normal_mode(app, key.code, key.modifiers).await {
                            break;
                        }
                    }
                    Mode::Command => {
                        if handle_command_mode(app, key.code).await {
                            break;
                        }
                    }
                    Mode::Help => {
                        handle_help_mode(app, key.code);
                    }
                    Mode::Confirm => {
                        handle_confirm_mode(app, key.code).await;
                    }
                    Mode::Warning => {
                        handle_warning_mode(app, key.code);
                    }
                    Mode::Projects => {
                        handle_projects_mode(app, key.code).await;
                    }
                    Mode::Zones => {
                        handle_zones_mode(app, key.code).await;
                    }
                    Mode::Describe => {
                        handle_describe_mode(app, key.code);
                    }
                }
            }
        }
    }

    Ok(())
}

async fn handle_normal_mode(app: &mut App, code: KeyCode, modifiers: KeyModifiers) -> bool {
    // Check for Ctrl+C to quit
    if modifiers.contains(KeyModifiers::CONTROL) && code == KeyCode::Char('c') {
        return true;
    }

    // Check for Ctrl+D for delete/destructive action
    if modifiers.contains(KeyModifiers::CONTROL) && code == KeyCode::Char('d') {
        let shortcut = "ctrl+d".to_string();
        if let Some(action_index) = app.find_action_by_shortcut(&shortcut) {
            tracing::info!("Action shortcut 'ctrl+d' triggered");
            app.trigger_action(action_index);
            // If action doesn't need confirmation, execute it
            if app.mode != Mode::Confirm {
                app.execute_pending_action().await;
            }
        }
        return false;
    }



    match code {
        KeyCode::Char('q') => return true,
        KeyCode::Char('j') | KeyCode::Down => app.next(),
        KeyCode::Char('k') | KeyCode::Up => app.previous(),
        KeyCode::Char('g') => {
            // Check for 'gg' sequence (go to top)
            let now = std::time::Instant::now();
            if let Some((KeyCode::Char('g'), last_time)) = app.last_key_press {
                if now.duration_since(last_time) < Duration::from_millis(500) {
                    app.go_to_top();
                    app.last_key_press = None;
                    return false;
                }
            }
            // Store for potential 'gg' sequence
            app.last_key_press = Some((KeyCode::Char('g'), now));
        }

        KeyCode::Char('G') | KeyCode::End => app.go_to_bottom(),
        KeyCode::Home => app.go_to_top(),
        KeyCode::Char('r') => {
            tracing::debug!("Manual refresh triggered");
            app.refresh().await;
        }
        KeyCode::Enter | KeyCode::Char('d') => app.enter_describe_mode(),
        KeyCode::Char('?') => app.enter_help_mode(),
        KeyCode::Char(':') => app.enter_command_mode(),
        KeyCode::Char('/') => {
            app.filter_active = true;
            app.filter_text.clear();
        }
        KeyCode::Char('0') => {
            app.switch_zone("us-central1-a").await;
            app.refresh().await;
        }
        KeyCode::Char('1') => {
            app.switch_zone("us-east1-b").await;
            app.refresh().await;
        }
        KeyCode::Char('2') => {
            app.switch_zone("us-west1-a").await;
            app.refresh().await;
        }
        KeyCode::Char('3') => {
            app.switch_zone("europe-west1-b").await;
            app.refresh().await;
        }
        KeyCode::Char('4') => {
            app.switch_zone("asia-east1-a").await;
            app.refresh().await;
        }
        KeyCode::Char('5') => {
            app.switch_zone("asia-northeast1-a").await;
            app.refresh().await;
        }
        KeyCode::Backspace => {
            app.navigate_back().await;
        }
        KeyCode::Esc => {
            if app.filter_active || !app.filter_text.is_empty() {
                app.clear_filter();
            }
        }
        _ => {
            // Handle filter input if filter is active
            if app.filter_active {
                if let KeyCode::Char(c) = code {
                    app.filter_text.push(c);
                    app.apply_filter();
                }
            } else if let KeyCode::Char(c) = code {
                let shortcut = c.to_string();
                
                // First check if this is a sub-resource shortcut
                if let Some(sub_resource_key) = app.find_sub_resource_by_shortcut(&shortcut) {
                    tracing::info!("Sub-resource shortcut '{}' triggered -> {}", shortcut, sub_resource_key);
                    app.navigate_to_sub_resource(&sub_resource_key).await;
                }
                // Then check if this is an action shortcut
                else if let Some(action_index) = app.find_action_by_shortcut(&shortcut) {
                    tracing::info!("Action shortcut '{}' triggered", shortcut);
                    app.trigger_action(action_index);
                    // If action doesn't need confirmation, execute it
                    if app.mode != Mode::Confirm {
                        app.execute_pending_action().await;
                    }
                }
            }
        }
    }

    false
}

async fn handle_command_mode(app: &mut App, code: KeyCode) -> bool {
    match code {
        KeyCode::Esc => {
            app.exit_mode();
        }
        KeyCode::Enter => {
            tracing::debug!("Executing command: {}", app.command_text);
            let should_quit = app.execute_command().await;
            if should_quit {
                return true;
            }
        }
        KeyCode::Backspace => {
            app.command_text.pop();
            app.update_command_suggestions();
        }
        KeyCode::Tab | KeyCode::Right => {
            app.apply_suggestion();
        }
        KeyCode::Down => {
            app.next_suggestion();
        }
        KeyCode::Up => {
            app.prev_suggestion();
        }
        KeyCode::Char(c) => {
            app.command_text.push(c);
            app.update_command_suggestions();
        }
        _ => {}
    }

    false
}

fn handle_help_mode(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q') => {
            app.exit_mode();
        }
        _ => {}
    }
}

async fn handle_confirm_mode(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
            app.exit_mode();
        }
        KeyCode::Enter => {
            app.execute_pending_action().await;
        }
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            // Quick yes - set selected_yes and execute
            if let Some(ref mut pending) = app.pending_action {
                pending.selected_yes = true;
            }
            app.execute_pending_action().await;
        }
        KeyCode::Left | KeyCode::Char('h') => {
            if let Some(ref mut pending) = app.pending_action {
                pending.selected_yes = false;
            }
        }
        KeyCode::Right | KeyCode::Char('l') => {
            if let Some(ref mut pending) = app.pending_action {
                pending.selected_yes = true;
            }
        }
        KeyCode::Tab => {
            if let Some(ref mut pending) = app.pending_action {
                pending.selected_yes = !pending.selected_yes;
            }
        }
        _ => {}
    }
}

fn handle_warning_mode(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Esc | KeyCode::Enter => {
            app.exit_mode();
        }
        _ => {}
    }
}

async fn handle_projects_mode(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Esc => {
            // Only allow escape if a project is already selected
            if app.has_project() {
                app.exit_mode();
            }
        }
        KeyCode::Enter => {
            app.select_project().await;
        }
        KeyCode::Char('j') | KeyCode::Down => {
            app.next();
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.previous();
        }
        KeyCode::Char('g') => {
            app.go_to_top();
        }
        KeyCode::Char('G') => {
            app.go_to_bottom();
        }
        _ => {}
    }
}

async fn handle_zones_mode(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Esc => {
            app.exit_mode();
        }
        KeyCode::Enter => {
            app.select_zone().await;
        }
        KeyCode::Char('j') | KeyCode::Down => {
            app.next();
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.previous();
        }
        KeyCode::Char('g') => {
            app.go_to_top();
        }
        KeyCode::Char('G') => {
            app.go_to_bottom();
        }
        _ => {}
    }
}

fn handle_describe_mode(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('d') => {
            app.exit_mode();
        }
        KeyCode::Char('j') | KeyCode::Down => {
            app.describe_scroll = app.describe_scroll.saturating_add(1);
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.describe_scroll = app.describe_scroll.saturating_sub(1);
        }
        KeyCode::Char('g') => {
            app.describe_scroll = 0;
        }
        KeyCode::Char('G') => {
            app.describe_scroll_to_bottom(30); // Approximate visible lines
        }
        _ => {}
    }
}
