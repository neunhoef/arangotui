use anyhow::{Context, Result};
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
use serde::Deserialize;
use std::io;

#[derive(Parser, Debug)]
#[command(name = "arangotui")]
#[command(about = "A TUI for ArangoDB and Graph Analytics Engine", long_about = None)]
struct Args {
    /// ArangoDB endpoint URL
    #[arg(long, default_value = "http://localhost:8529")]
    endpoint: String,

    /// Graph Analytics Engine endpoint URL
    #[arg(long)]
    gae: Option<String>,

    /// Username for authentication
    #[arg(long, default_value = "root")]
    username: String,

    /// Password for authentication
    #[arg(long, default_value = "")]
    password: String,
}

#[derive(Debug, Deserialize)]
struct ArangoVersion {
    server: String,
    license: String,
    version: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GaeVersion {
    api_max_version: u32,
    api_min_version: u32,
    version: String,
}

struct AppState {
    arango_endpoint: String,
    gae_endpoint: Option<String>,
    arango_version: ArangoVersion,
    gae_version: Option<GaeVersion>,
    selected_menu_item: usize,
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

async fn check_arango_version(
    client: &Client,
    endpoint: &str,
    username: &str,
    password: &str,
) -> Result<ArangoVersion> {
    let url = format!("{}/_api/version", endpoint.trim_end_matches('/'));
    let response = client
        .get(&url)
        .basic_auth(username, Some(password))
        .send()
        .await
        .context("Failed to connect to ArangoDB")?;

    if !response.status().is_success() {
        anyhow::bail!("ArangoDB returned error status: {}", response.status());
    }

    let version: ArangoVersion = response
        .json()
        .await
        .context("Failed to parse ArangoDB version response")?;

    Ok(version)
}

async fn check_gae_version(client: &Client, endpoint: &str) -> Result<GaeVersion> {
    let url = format!("{}/v1/version", endpoint.trim_end_matches('/'));
    let response = client
        .get(&url)
        .send()
        .await
        .context("Failed to connect to GAE")?;

    if !response.status().is_success() {
        anyhow::bail!("GAE returned error status: {}", response.status());
    }

    let version: GaeVersion = response
        .json()
        .await
        .context("Failed to parse GAE version response")?;

    Ok(version)
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

        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
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
                        if let Some(menu_item) = MenuItem::from_index(app_state.selected_menu_item)
                        {
                            match menu_item {
                                MenuItem::Quit => return Ok(()),
                                MenuItem::BrowseDatabase => {
                                    // TODO: Implement
                                }
                                MenuItem::Gae => {
                                    // TODO: Implement
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
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Create HTTP client with TLS certificate verification disabled
    let client = create_http_client()?;

    // Check ArangoDB version (required)
    println!("Connecting to ArangoDB at {}...", args.endpoint);
    let arango_version =
        check_arango_version(&client, &args.endpoint, &args.username, &args.password).await?;
    println!(
        "Connected to ArangoDB {} ({})",
        arango_version.version, arango_version.license
    );

    // Check GAE version (optional)
    let gae_version = if let Some(gae_endpoint) = &args.gae {
        println!("Connecting to GAE at {}...", gae_endpoint);
        match check_gae_version(&client, gae_endpoint).await {
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
        arango_version,
        gae_version,
        selected_menu_item: 0,
    };

    // Run the TUI
    run_app(&mut app_state).await?;

    Ok(())
}
