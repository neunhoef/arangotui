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

#[derive(Debug, Deserialize)]
struct DatabaseListResponse {
    error: bool,
    code: u16,
    result: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
struct CollectionInfo {
    id: String,
    name: String,
    status: u32,
    #[serde(rename = "type")]
    collection_type: u32,
    #[serde(rename = "isSystem")]
    is_system: bool,
    #[serde(rename = "globallyUniqueId")]
    globally_unique_id: String,
}

#[derive(Debug, Deserialize)]
struct CollectionListResponse {
    error: bool,
    code: u16,
    result: Vec<CollectionInfo>,
}

#[derive(Debug, Deserialize, Clone, serde::Serialize)]
struct CollectionCount {
    error: bool,
    code: u16,
    #[serde(rename = "writeConcern")]
    write_concern: Option<u32>,
    #[serde(rename = "waitForSync")]
    wait_for_sync: Option<bool>,
    #[serde(rename = "usesRevisionsAsDocumentIds")]
    uses_revisions_as_document_ids: Option<bool>,
    #[serde(rename = "syncByRevision")]
    sync_by_revision: Option<bool>,
    #[serde(rename = "statusString")]
    status_string: Option<String>,
    id: Option<String>,
    #[serde(rename = "isSmartChild")]
    is_smart_child: Option<bool>,
    schema: Option<serde_json::Value>,
    name: String,
    #[serde(rename = "type")]
    collection_type: u32,
    status: u32,
    count: u64,
    #[serde(rename = "cacheEnabled")]
    cache_enabled: Option<bool>,
    #[serde(rename = "isSystem")]
    is_system: bool,
    #[serde(rename = "internalValidatorType")]
    internal_validator_type: Option<u32>,
    #[serde(rename = "globallyUniqueId")]
    globally_unique_id: Option<String>,
    #[serde(rename = "keyOptions")]
    key_options: Option<serde_json::Value>,
    #[serde(rename = "computedValues")]
    computed_values: Option<serde_json::Value>,
    #[serde(rename = "objectId")]
    object_id: Option<String>,
}

#[derive(Debug, Clone)]
struct DatabaseStats {
    name: String,
    doc_collections: usize,
    edge_collections: usize,
    system_collections: usize,
    accessible: bool,
}

#[derive(Debug, Clone)]
struct CollectionWithCount {
    info: CollectionInfo,
    count: Option<u64>,
}

struct AppState {
    arango_endpoint: String,
    gae_endpoint: Option<String>,
    username: String,
    password: String,
    arango_version: ArangoVersion,
    gae_version: Option<GaeVersion>,
    selected_menu_item: usize,
    http_client: Client,
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

async fn get_databases(
    client: &Client,
    endpoint: &str,
    username: &str,
    password: &str,
) -> Result<Vec<String>> {
    let url = format!("{}/_api/database", endpoint.trim_end_matches('/'));
    let response = client
        .get(&url)
        .basic_auth(username, Some(password))
        .send()
        .await
        .context("Failed to fetch databases")?;

    if !response.status().is_success() {
        anyhow::bail!("Failed to fetch databases: {}", response.status());
    }

    let db_response: DatabaseListResponse = response
        .json()
        .await
        .context("Failed to parse database list response")?;

    Ok(db_response.result)
}

async fn get_collections(
    client: &Client,
    endpoint: &str,
    database: &str,
    username: &str,
    password: &str,
) -> Result<Vec<CollectionInfo>> {
    let url = format!(
        "{}/_db/{}/_api/collection",
        endpoint.trim_end_matches('/'),
        database
    );
    let response = client
        .get(&url)
        .basic_auth(username, Some(password))
        .send()
        .await
        .context("Failed to fetch collections")?;

    if !response.status().is_success() {
        anyhow::bail!("Failed to fetch collections: {}", response.status());
    }

    let coll_response: CollectionListResponse = response
        .json()
        .await
        .context("Failed to parse collection list response")?;

    Ok(coll_response.result)
}

async fn get_collection_count(
    client: &Client,
    endpoint: &str,
    database: &str,
    collection: &str,
    username: &str,
    password: &str,
) -> Result<CollectionCount> {
    let url = format!(
        "{}/_db/{}/_api/collection/{}/count",
        endpoint.trim_end_matches('/'),
        database,
        collection
    );
    let response = client
        .get(&url)
        .basic_auth(username, Some(password))
        .send()
        .await
        .context("Failed to fetch collection count")?;

    if !response.status().is_success() {
        anyhow::bail!("Failed to fetch collection count: {}", response.status());
    }

    let count_response: CollectionCount = response
        .json()
        .await
        .context("Failed to parse collection count response")?;

    Ok(count_response)
}

async fn get_database_stats(
    client: &Client,
    endpoint: &str,
    database: &str,
    username: &str,
    password: &str,
) -> DatabaseStats {
    match get_collections(client, endpoint, database, username, password).await {
        Ok(collections) => {
            let mut doc_collections = 0;
            let mut edge_collections = 0;
            let mut system_collections = 0;

            for coll in collections {
                if coll.is_system {
                    system_collections += 1;
                } else if coll.collection_type == 2 {
                    doc_collections += 1;
                } else if coll.collection_type == 3 {
                    edge_collections += 1;
                }
            }

            DatabaseStats {
                name: database.to_string(),
                doc_collections,
                edge_collections,
                system_collections,
                accessible: true,
            }
        }
        Err(_) => DatabaseStats {
            name: database.to_string(),
            doc_collections: 0,
            edge_collections: 0,
            system_collections: 0,
            accessible: false,
        },
    }
}

#[derive(Clone)]
enum BrowserView {
    DatabaseList,
    CollectionList(String),               // database name
    CollectionProperties(String, String), // database name, collection name
}

struct DatabaseBrowser {
    view: BrowserView,
    database_stats: Vec<DatabaseStats>,
    selected_db_index: usize,
    collections: Vec<CollectionWithCount>,
    selected_coll_index: usize,
    collection_details: Option<CollectionCount>,
    scroll_offset: usize,
    accessible: bool,
}

impl DatabaseBrowser {
    fn new() -> Self {
        Self {
            view: BrowserView::DatabaseList,
            database_stats: Vec::new(),
            selected_db_index: 0,
            collections: Vec::new(),
            selected_coll_index: 0,
            collection_details: None,
            scroll_offset: 0,
            accessible: true,
        }
    }

    async fn load_databases(&mut self, app_state: &AppState) -> Result<()> {
        match get_databases(
            &app_state.http_client,
            &app_state.arango_endpoint,
            &app_state.username,
            &app_state.password,
        )
        .await
        {
            Ok(databases) => {
                self.accessible = true;
                let mut stats = Vec::new();
                for db in databases {
                    let db_stats = get_database_stats(
                        &app_state.http_client,
                        &app_state.arango_endpoint,
                        &db,
                        &app_state.username,
                        &app_state.password,
                    )
                    .await;
                    stats.push(db_stats);
                }
                self.database_stats = stats;
                self.selected_db_index = 0;
                Ok(())
            }
            Err(_) => {
                self.accessible = false;
                Ok(())
            }
        }
    }

    async fn load_collections(&mut self, app_state: &AppState, database: &str) -> Result<()> {
        let collections = get_collections(
            &app_state.http_client,
            &app_state.arango_endpoint,
            database,
            &app_state.username,
            &app_state.password,
        )
        .await?;

        let mut collections_with_count = Vec::new();
        for coll in collections {
            let count = get_collection_count(
                &app_state.http_client,
                &app_state.arango_endpoint,
                database,
                &coll.name,
                &app_state.username,
                &app_state.password,
            )
            .await
            .ok()
            .map(|c| c.count);

            collections_with_count.push(CollectionWithCount { info: coll, count });
        }

        // Sort: non-system first (alphabetically), then system collections (alphabetically)
        collections_with_count.sort_by(|a, b| match (a.info.is_system, b.info.is_system) {
            (false, true) => std::cmp::Ordering::Less,
            (true, false) => std::cmp::Ordering::Greater,
            _ => a.info.name.cmp(&b.info.name),
        });

        self.collections = collections_with_count;
        self.selected_coll_index = 0;
        self.scroll_offset = 0;
        Ok(())
    }

    async fn load_collection_details(
        &mut self,
        app_state: &AppState,
        database: &str,
        collection: &str,
    ) -> Result<()> {
        let details = get_collection_count(
            &app_state.http_client,
            &app_state.arango_endpoint,
            database,
            collection,
            &app_state.username,
            &app_state.password,
        )
        .await?;

        self.collection_details = Some(details);
        self.scroll_offset = 0;
        Ok(())
    }
}

fn render_database_list(f: &mut Frame, area: Rect, browser: &DatabaseBrowser) {
    use ratatui::widgets::{Cell, Row, Table};

    if !browser.accessible {
        let no_access = Paragraph::new("NO ACCESS")
            .style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Database Browser"),
            );
        f.render_widget(no_access, area);
        return;
    }

    let header = Row::new(vec![
        "Database",
        "Doc Collections",
        "Edge Collections",
        "System",
    ])
    .style(
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )
    .bottom_margin(1);

    let rows: Vec<Row> = browser
        .database_stats
        .iter()
        .enumerate()
        .map(|(i, stats)| {
            let style = if i == browser.selected_db_index {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            if stats.accessible {
                Row::new(vec![
                    Cell::from(stats.name.clone()),
                    Cell::from(stats.doc_collections.to_string()),
                    Cell::from(stats.edge_collections.to_string()),
                    Cell::from(stats.system_collections.to_string()),
                ])
                .style(style)
            } else {
                Row::new(vec![
                    Cell::from(stats.name.clone()),
                    Cell::from("NO ACCESS"),
                    Cell::from(""),
                    Cell::from(""),
                ])
                .style(style.fg(Color::Red))
            }
        })
        .collect();

    let widths = [
        Constraint::Percentage(40),
        Constraint::Percentage(20),
        Constraint::Percentage(20),
        Constraint::Percentage(20),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Database Browser - Select a database"),
        )
        .column_spacing(2);

    f.render_widget(table, area);
}

fn render_collection_list(f: &mut Frame, area: Rect, browser: &DatabaseBrowser, database: &str) {
    use ratatui::widgets::{Cell, Row, Table};

    if browser.collections.is_empty() {
        let empty = Paragraph::new("No collections found")
            .style(Style::default().fg(Color::Yellow))
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!("Database: {}", database)),
            );
        f.render_widget(empty, area);
        return;
    }

    let total_collections = browser.collections.len();
    let total_docs: u64 = browser.collections.iter().filter_map(|c| c.count).sum();

    let title = format!(
        "Database: {} | Collections: {} | Total Documents: {}",
        database, total_collections, total_docs
    );

    let header = Row::new(vec!["Name", "Type", "System", "Count"])
        .style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .bottom_margin(1);

    let rows: Vec<Row> = browser
        .collections
        .iter()
        .enumerate()
        .map(|(i, coll)| {
            let style = if i == browser.selected_coll_index {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            let coll_type = if coll.info.collection_type == 2 {
                "Document"
            } else {
                "Edge"
            };

            let is_system = if coll.info.is_system { "Yes" } else { "No" };

            let count = coll
                .count
                .map(|c| c.to_string())
                .unwrap_or_else(|| "?".to_string());

            Row::new(vec![
                Cell::from(coll.info.name.clone()),
                Cell::from(coll_type),
                Cell::from(is_system),
                Cell::from(count),
            ])
            .style(style)
        })
        .collect();

    let widths = [
        Constraint::Percentage(50),
        Constraint::Percentage(15),
        Constraint::Percentage(10),
        Constraint::Percentage(25),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().borders(Borders::ALL).title(title))
        .column_spacing(2);

    f.render_widget(table, area);
}

fn render_collection_properties(
    f: &mut Frame,
    area: Rect,
    browser: &DatabaseBrowser,
    database: &str,
    collection: &str,
) {
    if let Some(details) = &browser.collection_details {
        let json_str =
            serde_json::to_string_pretty(details).unwrap_or_else(|_| "Error".to_string());
        let lines: Vec<Line> = json_str
            .lines()
            .map(|l| Line::from(l.to_string()))
            .collect();

        let title = format!("Collection Properties: {}.{}", database, collection);

        let para = Paragraph::new(lines)
            .style(Style::default().fg(Color::White))
            .block(Block::default().borders(Borders::ALL).title(title))
            .scroll((browser.scroll_offset as u16, 0));

        f.render_widget(para, area);
    } else {
        let loading = Paragraph::new("Loading...")
            .style(Style::default().fg(Color::Yellow))
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!("Collection: {}.{}", database, collection)),
            );
        f.render_widget(loading, area);
    }
}

async fn run_database_browser(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app_state: &AppState,
) -> Result<()> {
    let mut browser = DatabaseBrowser::new();
    browser.load_databases(app_state).await?;

    loop {
        terminal.draw(|f| match &browser.view {
            BrowserView::DatabaseList => render_database_list(f, f.area(), &browser),
            BrowserView::CollectionList(db) => render_collection_list(f, f.area(), &browser, db),
            BrowserView::CollectionProperties(db, coll) => {
                render_collection_properties(f, f.area(), &browser, db, coll)
            }
        })?;

        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                match browser.view.clone() {
                    BrowserView::DatabaseList => match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                        KeyCode::Down | KeyCode::Char('j') => {
                            if !browser.database_stats.is_empty() {
                                browser.selected_db_index =
                                    (browser.selected_db_index + 1) % browser.database_stats.len();
                            }
                        }
                        KeyCode::Up | KeyCode::Char('k') => {
                            if !browser.database_stats.is_empty() {
                                browser.selected_db_index = if browser.selected_db_index == 0 {
                                    browser.database_stats.len() - 1
                                } else {
                                    browser.selected_db_index - 1
                                };
                            }
                        }
                        KeyCode::Enter => {
                            if browser.selected_db_index < browser.database_stats.len() {
                                let db_name = browser.database_stats[browser.selected_db_index]
                                    .name
                                    .clone();
                                if browser.database_stats[browser.selected_db_index].accessible {
                                    browser.load_collections(app_state, &db_name).await?;
                                    browser.view = BrowserView::CollectionList(db_name);
                                }
                            }
                        }
                        _ => {}
                    },
                    BrowserView::CollectionList(db) => match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => {
                            browser.view = BrowserView::DatabaseList;
                            browser.collections.clear();
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            if !browser.collections.is_empty() {
                                browser.selected_coll_index =
                                    (browser.selected_coll_index + 1) % browser.collections.len();
                            }
                        }
                        KeyCode::Up | KeyCode::Char('k') => {
                            if !browser.collections.is_empty() {
                                browser.selected_coll_index = if browser.selected_coll_index == 0 {
                                    browser.collections.len() - 1
                                } else {
                                    browser.selected_coll_index - 1
                                };
                            }
                        }
                        KeyCode::Enter => {
                            if browser.selected_coll_index < browser.collections.len() {
                                let coll_name = browser.collections[browser.selected_coll_index]
                                    .info
                                    .name
                                    .clone();
                                browser
                                    .load_collection_details(app_state, &db, &coll_name)
                                    .await?;
                                browser.view =
                                    BrowserView::CollectionProperties(db.clone(), coll_name);
                            }
                        }
                        _ => {}
                    },
                    BrowserView::CollectionProperties(db, _coll) => match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => {
                            browser.view = BrowserView::CollectionList(db.clone());
                            browser.collection_details = None;
                            browser.scroll_offset = 0;
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            browser.scroll_offset = browser.scroll_offset.saturating_add(1);
                        }
                        KeyCode::Up | KeyCode::Char('k') => {
                            browser.scroll_offset = browser.scroll_offset.saturating_sub(1);
                        }
                        KeyCode::PageDown => {
                            browser.scroll_offset = browser.scroll_offset.saturating_add(10);
                        }
                        KeyCode::PageUp => {
                            browser.scroll_offset = browser.scroll_offset.saturating_sub(10);
                        }
                        _ => {}
                    },
                }
            }
        }
    }
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
                                    run_database_browser(terminal, app_state).await?;
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
        username: args.username,
        password: args.password,
        arango_version,
        gae_version,
        selected_menu_item: 0,
        http_client: client,
    };

    // Run the TUI
    run_app(&mut app_state).await?;

    Ok(())
}
