mod arangodb;
mod args;
mod gae;
mod json_struct_editor;

use anyhow::{Context, Result};
use args::Args;
use clap::Parser;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};
use reqwest::Client;
use std::io;

struct AppState {
    arango_endpoint: String,
    gae_endpoint: Option<String>,
    username: String,
    password: String,
    arango_version: arangodb::ArangoVersion,
    gae_version: Option<gae::GaeVersion>,
    selected_menu_item: usize,
    http_client: Client,
    gae_jwt_secret: Option<Vec<u8>>,
    gae_jwt_token: Option<gae::GaeJwtToken>,
}

enum MenuItem {
    BrowseDatabase,
    Gae,
    Options,
    Quit,
}

impl MenuItem {
    fn items() -> Vec<&'static str> {
        vec![
            "Browse database",
            "Graph Analytics Engine (GAE)",
            "Options",
            "Quit",
        ]
    }

    fn from_index(index: usize) -> Option<Self> {
        match index {
            0 => Some(MenuItem::BrowseDatabase),
            1 => Some(MenuItem::Gae),
            2 => Some(MenuItem::Options),
            3 => Some(MenuItem::Quit),
            _ => None,
        }
    }
}

fn create_http_client() -> Result<Client> {
    Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .context("Failed to create HTTP client")
}

fn render_main_menu(f: &mut Frame, app_state: &mut AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5), // Header
            Constraint::Min(0),    // Menu
        ])
        .split(f.area());

    render_header(f, chunks[0], app_state);
    render_menu(f, chunks[1], app_state);
}

fn render_header(f: &mut Frame, area: Rect, app_state: &AppState) {
    let mut lines = vec![
        Line::from(vec![Span::styled(
            "arangotui",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )])
        .alignment(Alignment::Center),
    ];

    lines.push(
        Line::from(vec![Span::styled(
            format!(
                "ArangoDB {} ({})",
                app_state.arango_version.version, app_state.arango_version.license
            ),
            Style::default().fg(Color::Green),
        )])
        .alignment(Alignment::Center),
    );

    if let Some(gae_version) = &app_state.gae_version {
        lines.push(
            Line::from(vec![Span::styled(
                format!("GAE {}", gae_version.version),
                Style::default().fg(Color::Green),
            )])
            .alignment(Alignment::Center),
        );
    } else {
        lines.push(
            Line::from(vec![Span::styled(
                "GAE: Not connected",
                Style::default().fg(Color::Yellow),
            )])
            .alignment(Alignment::Center),
        );
    }

    let header = Paragraph::new(lines).block(Block::default().borders(Borders::ALL));
    f.render_widget(header, area);
}

fn render_menu(f: &mut Frame, area: Rect, app_state: &mut AppState) {
    let menu_items: Vec<ListItem> = MenuItem::items()
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let style = if i == app_state.selected_menu_item {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            ListItem::new(*item).style(style)
        })
        .collect();

    let menu =
        List::new(menu_items).block(Block::default().borders(Borders::ALL).title("Main Menu"));

    f.render_widget(menu, area);
}

async fn run_app(app_state: &mut AppState) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = app_loop(&mut terminal, app_state).await;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

async fn app_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app_state: &mut AppState,
) -> Result<()> {
    loop {
        terminal.draw(|f| render_main_menu(f, app_state))?;

        if let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                KeyCode::Down | KeyCode::Char('j') => {
                    let menu_items_count = MenuItem::items().len();
                    app_state.selected_menu_item =
                        (app_state.selected_menu_item + 1) % menu_items_count;
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    let menu_items_count = MenuItem::items().len();
                    app_state.selected_menu_item = if app_state.selected_menu_item == 0 {
                        menu_items_count - 1
                    } else {
                        app_state.selected_menu_item - 1
                    };
                }
                KeyCode::Enter => {
                    if let Some(menu_item) = MenuItem::from_index(app_state.selected_menu_item) {
                        match menu_item {
                            MenuItem::Quit => return Ok(()),
                            MenuItem::BrowseDatabase => {
                                arangodb::run_database_browser(
                                    terminal,
                                    &app_state.http_client,
                                    &app_state.arango_endpoint,
                                    &app_state.username,
                                    &app_state.password,
                                )
                                .await?;
                            }
                            MenuItem::Gae => {
                                gae::run_gae_browser(
                                    terminal,
                                    &app_state.http_client,
                                    &app_state.gae_endpoint,
                                    &app_state.gae_jwt_secret,
                                    &mut app_state.gae_jwt_token,
                                )
                                .await?;
                            }
                            MenuItem::Options => {
                                // TODO: Implement
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Create HTTP client with TLS certificate verification disabled
    let client = create_http_client()?;

    // Check ArangoDB version (required)
    println!("Connecting to ArangoDB at {}...", args.endpoint);
    let arango_version =
        arangodb::check_arango_version(&client, &args.endpoint, &args.username, &args.password)
            .await?;
    println!(
        "Connected to ArangoDB {} ({})",
        arango_version.version, arango_version.license
    );

    // Load GAE JWT secret if provided
    let gae_jwt_secret = if let Some(ref secret_file) = args.gae_jwt_secret_file {
        match std::fs::read(secret_file) {
            Ok(secret) => {
                println!("Loaded GAE JWT secret from {}", secret_file);
                Some(secret)
            }
            Err(e) => {
                eprintln!("Warning: Could not read GAE JWT secret file: {}", e);
                None
            }
        }
    } else {
        None
    };

    // Generate initial GAE JWT token if secret is available
    let gae_jwt_token = if let Some(ref secret) = gae_jwt_secret {
        match gae::create_gae_jwt_token(secret, 3600) {
            Ok(token) => {
                let expiry = std::time::SystemTime::now() + std::time::Duration::from_secs(3600);
                Some(gae::GaeJwtToken { token, expiry })
            }
            Err(e) => {
                eprintln!("Warning: Could not create GAE JWT token: {}", e);
                None
            }
        }
    } else {
        None
    };

    // Check GAE version (optional)
    let gae_version = if let Some(gae_endpoint) = &args.gae {
        println!("Connecting to GAE at {}...", gae_endpoint);
        let token = gae_jwt_token.as_ref().map(|t| t.token.as_str());
        match gae::check_gae_version(&client, gae_endpoint, token).await {
            Ok(version) => {
                println!("Connected to GAE {}", version.version);
                Some(version)
            }
            Err(e) => {
                println!("Warning: Could not connect to GAE: {}", e);
                None
            }
        }
    } else {
        None
    };

    // Create application state
    let mut app_state = AppState {
        arango_endpoint: args.endpoint,
        gae_endpoint: args.gae,
        username: args.username,
        password: args.password,
        arango_version,
        gae_version,
        selected_menu_item: 0,
        http_client: client,
        gae_jwt_secret,
        gae_jwt_token,
    };

    // Run the TUI
    run_app(&mut app_state).await?;

    Ok(())
}
