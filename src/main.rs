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

#[derive(Debug, Deserialize, Clone, serde::Serialize)]
struct EdgeDefinition {
    collection: String,
    from: Vec<String>,
    to: Vec<String>,
}

#[derive(Debug, Deserialize, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct GraphInfo {
    #[serde(rename = "_id")]
    id: String,
    #[serde(rename = "_key")]
    key: String,
    #[serde(rename = "_rev")]
    rev: String,
    edge_definitions: Vec<EdgeDefinition>,
    orphan_collections: Vec<String>,
    name: String,
    #[serde(rename = "isSmart")]
    is_smart: Option<bool>,
    #[serde(rename = "isDisjoint")]
    is_disjoint: Option<bool>,
    #[serde(rename = "smartGraphAttribute")]
    smart_graph_attribute: Option<String>,
    number_of_shards: Option<u32>,
    replication_factor: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct GraphListResponse {
    error: bool,
    code: u16,
    graphs: Vec<GraphInfo>,
}

#[derive(Debug, Deserialize)]
struct AqlQueryResponse {
    error: bool,
    code: u16,
    result: Vec<serde_json::Value>,
    #[serde(rename = "hasMore")]
    has_more: bool,
    cached: bool,
    extra: Option<serde_json::Value>,
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

async fn get_graphs(
    client: &Client,
    endpoint: &str,
    database: &str,
    username: &str,
    password: &str,
) -> Result<Vec<GraphInfo>> {
    let url = format!(
        "{}/_db/{}/_api/gharial",
        endpoint.trim_end_matches('/'),
        database
    );
    let response = client
        .get(&url)
        .basic_auth(username, Some(password))
        .send()
        .await
        .context("Failed to fetch graphs")?;

    if !response.status().is_success() {
        anyhow::bail!("Failed to fetch graphs: {}", response.status());
    }

    let graph_response: GraphListResponse = response
        .json()
        .await
        .context("Failed to parse graph list response")?;

    Ok(graph_response.graphs)
}

async fn execute_aql_query(
    client: &Client,
    endpoint: &str,
    database: &str,
    query: &str,
    username: &str,
    password: &str,
) -> Result<Vec<serde_json::Value>> {
    let url = format!(
        "{}/_db/{}/_api/cursor",
        endpoint.trim_end_matches('/'),
        database
    );

    let body = serde_json::json!({
        "query": query,
        "count": false,
        "batchSize": 1000,
        "options": {
            "stream": true
        }
    });

    let response = client
        .post(&url)
        .basic_auth(username, Some(password))
        .json(&body)
        .send()
        .await
        .context("Failed to execute AQL query")?;

    if !response.status().is_success() {
        anyhow::bail!("Failed to execute AQL query: {}", response.status());
    }

    let query_response: AqlQueryResponse = response
        .json()
        .await
        .context("Failed to parse AQL query response")?;

    Ok(query_response.result)
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

#[derive(Clone, Debug)]
enum BrowserView {
    DatabaseList,
    CollectionList(String),               // database name
    GraphList(String),                    // database name
    CollectionProperties(String, String), // database name, collection name
    DocumentViewer(String, String),       // database name, collection name
    GraphProperties(String, String),      // database name, graph name
}

enum InputState {
    None,
    EnteringDocumentCount(String), // Current input string
}

struct DatabaseBrowser {
    view: BrowserView,
    database_stats: Vec<DatabaseStats>,
    selected_db_index: usize,
    collections: Vec<CollectionWithCount>,
    selected_coll_index: usize,
    graphs: Vec<GraphInfo>,
    selected_graph_index: usize,
    collection_details: Option<CollectionCount>,
    scroll_offset: usize,
    accessible: bool,
    input_state: InputState,
    documents: Vec<serde_json::Value>,
    navigation_stack: Vec<(BrowserView, usize)>, // Stack to track navigation history (view, selected_index)
    graph_details: Option<GraphInfo>,
}

impl DatabaseBrowser {
    fn new() -> Self {
        Self {
            view: BrowserView::DatabaseList,
            database_stats: Vec::new(),
            selected_db_index: 0,
            collections: Vec::new(),
            selected_coll_index: 0,
            graphs: Vec::new(),
            selected_graph_index: 0,
            collection_details: None,
            scroll_offset: 0,
            accessible: true,
            input_state: InputState::None,
            documents: Vec::new(),
            navigation_stack: Vec::new(),
            graph_details: None,
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

    async fn load_graphs(&mut self, app_state: &AppState, database: &str) -> Result<()> {
        let graphs = get_graphs(
            &app_state.http_client,
            &app_state.arango_endpoint,
            database,
            &app_state.username,
            &app_state.password,
        )
        .await?;

        self.graphs = graphs;
        self.selected_graph_index = 0;
        self.scroll_offset = 0;
        Ok(())
    }

    async fn load_documents(
        &mut self,
        app_state: &AppState,
        database: &str,
        collection: &str,
        count: usize,
    ) -> Result<()> {
        let query = format!("FOR d IN {} LIMIT {} RETURN d", collection, count);
        let documents = execute_aql_query(
            &app_state.http_client,
            &app_state.arango_endpoint,
            database,
            &query,
            &app_state.username,
            &app_state.password,
        )
        .await?;

        self.documents = documents;
        self.scroll_offset = 0;
        Ok(())
    }

    // Helper to find which graph and edge definition row is selected
    fn find_selected_graph_item(&self) -> Option<(usize, Option<usize>)> {
        let mut current_row = 0;
        for (graph_idx, graph) in self.graphs.iter().enumerate() {
            if current_row == self.selected_graph_index {
                return Some((graph_idx, None));
            }
            current_row += 1;

            for (edge_idx, _) in graph.edge_definitions.iter().enumerate() {
                if current_row == self.selected_graph_index {
                    return Some((graph_idx, Some(edge_idx)));
                }
                current_row += 1;
            }

            // Skip spacing row
            if graph_idx < self.graphs.len() - 1 {
                current_row += 1;
            }
        }
        None
    }

    async fn load_graph_details(
        &mut self,
        _app_state: &AppState,
        _database: &str,
        graph_name: &str,
    ) -> Result<()> {
        // Find the graph in our list
        let graph = self.graphs.iter().find(|g| g.name == graph_name).cloned();
        self.graph_details = graph;
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
                    .title(format!("Database: {} | Press G for Graphs", database)),
            );
        f.render_widget(empty, area);
        return;
    }

    let total_collections = browser.collections.len();
    let total_docs: u64 = browser.collections.iter().filter_map(|c| c.count).sum();

    let title = format!(
        "Database: {} | Collections: {} | Total Documents: {} | Press G for Graphs | SPACE to view documents",
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

fn render_graph_list(f: &mut Frame, area: Rect, browser: &DatabaseBrowser, database: &str) {
    use ratatui::widgets::{Cell, Row, Table};

    if browser.graphs.is_empty() {
        let empty = Paragraph::new("No graphs found")
            .style(Style::default().fg(Color::Yellow))
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!("Database: {} | Press C for Collections", database)),
            );
        f.render_widget(empty, area);
        return;
    }

    let total_graphs = browser.graphs.len();

    // Determine if we're on a graph row or edge definition row
    let title = if let Some((_, edge_idx)) = browser.find_selected_graph_item() {
        if edge_idx.is_some() {
            // On edge definition row
            format!(
                "Database: {} | Graphs: {} | C: Collections | ENTER: Edge collection | V: Vertex collection",
                database, total_graphs
            )
        } else {
            // On graph row
            format!(
                "Database: {} | Graphs: {} | C: Collections | ENTER: Graph details (JSON)",
                database, total_graphs
            )
        }
    } else {
        // Fallback
        format!(
            "Database: {} | Graphs: {} | C: Collections",
            database, total_graphs
        )
    };

    let header = Row::new(vec![
        "Graph/Edge",
        "Edge Collection",
        "From → To",
        "Smart/Disjoint",
    ])
    .style(
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )
    .bottom_margin(1);

    let mut rows: Vec<Row> = Vec::new();
    let mut current_row_index = 0;

    for (graph_idx, graph) in browser.graphs.iter().enumerate() {
        // Add graph name row
        let graph_style = if current_row_index == browser.selected_graph_index {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD)
        };

        let mut smart_disjoint_parts = Vec::new();
        if graph.is_smart.unwrap_or(false) {
            smart_disjoint_parts.push("Smart");
        }
        if graph.is_disjoint.unwrap_or(false) {
            smart_disjoint_parts.push("Disjoint");
        }
        let smart_disjoint = if smart_disjoint_parts.is_empty() {
            "-".to_string()
        } else {
            smart_disjoint_parts.join(", ")
        };

        rows.push(
            Row::new(vec![
                Cell::from(graph.name.clone()),
                Cell::from(""),
                Cell::from(""),
                Cell::from(smart_disjoint),
            ])
            .style(graph_style),
        );
        current_row_index += 1;

        // Add edge definition rows
        for edge_def in &graph.edge_definitions {
            let edge_style = if current_row_index == browser.selected_graph_index {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            let from_to = format!("{} → {}", edge_def.from.join(", "), edge_def.to.join(", "));

            rows.push(
                Row::new(vec![
                    Cell::from(format!("  └─ {}", edge_def.collection)),
                    Cell::from(edge_def.collection.clone()),
                    Cell::from(from_to),
                    Cell::from(""),
                ])
                .style(edge_style),
            );
            current_row_index += 1;
        }

        // Add spacing between graphs (except after the last one)
        if graph_idx < browser.graphs.len() - 1 {
            rows.push(Row::new(vec![
                Cell::from(""),
                Cell::from(""),
                Cell::from(""),
                Cell::from(""),
            ]));
            current_row_index += 1;
        }
    }

    let widths = [
        Constraint::Percentage(25),
        Constraint::Percentage(20),
        Constraint::Percentage(40),
        Constraint::Percentage(15),
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

fn render_document_viewer(
    f: &mut Frame,
    area: Rect,
    browser: &DatabaseBrowser,
    database: &str,
    collection: &str,
) {
    if browser.documents.is_empty() {
        let empty = Paragraph::new("No documents found")
            .style(Style::default().fg(Color::Yellow))
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!("Documents: {}.{}", database, collection)),
            );
        f.render_widget(empty, area);
        return;
    }

    let mut lines = Vec::new();
    for (i, doc) in browser.documents.iter().enumerate() {
        if i > 0 {
            lines.push(Line::from(""));
        }
        let json_str = serde_json::to_string_pretty(doc).unwrap_or_else(|_| "Error".to_string());
        for line in json_str.lines() {
            lines.push(Line::from(line.to_string()));
        }
    }

    let title = format!(
        "Documents: {}.{} ({} documents) | Press ESC or Q to go back",
        database,
        collection,
        browser.documents.len()
    );

    let para = Paragraph::new(lines)
        .style(Style::default().fg(Color::White))
        .block(Block::default().borders(Borders::ALL).title(title))
        .scroll((browser.scroll_offset as u16, 0));

    f.render_widget(para, area);
}

fn render_input_dialog(f: &mut Frame, area: Rect, input_text: &str) {
    use ratatui::widgets::Clear;

    // Create a centered dialog box
    let dialog_width = 50;
    let dialog_height = 7;
    let x = (area.width.saturating_sub(dialog_width)) / 2;
    let y = (area.height.saturating_sub(dialog_height)) / 2;

    let dialog_area = Rect {
        x: area.x + x,
        y: area.y + y,
        width: dialog_width,
        height: dialog_height,
    };

    // Clear the area behind the dialog
    f.render_widget(Clear, dialog_area);

    // Create the dialog content
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Title and prompt
            Constraint::Length(3), // Input field
        ])
        .split(dialog_area);

    let prompt = Paragraph::new("Enter number of documents to fetch:")
        .style(Style::default().fg(Color::White))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Fetch Documents"),
        );

    f.render_widget(prompt, chunks[0]);

    let input = Paragraph::new(input_text)
        .style(Style::default().fg(Color::Yellow))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .style(Style::default().fg(Color::Cyan)),
        );

    f.render_widget(input, chunks[1]);
}

fn render_graph_properties(
    f: &mut Frame,
    area: Rect,
    browser: &DatabaseBrowser,
    database: &str,
    graph_name: &str,
) {
    if let Some(details) = &browser.graph_details {
        let json_str =
            serde_json::to_string_pretty(details).unwrap_or_else(|_| "Error".to_string());
        let lines: Vec<Line> = json_str
            .lines()
            .map(|l| Line::from(l.to_string()))
            .collect();

        let title = format!("Graph Properties: {}.{}", database, graph_name);

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
                    .title(format!("Graph: {}.{}", database, graph_name)),
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
        terminal.draw(|f| {
            match &browser.view {
                BrowserView::DatabaseList => render_database_list(f, f.area(), &browser),
                BrowserView::CollectionList(db) => {
                    render_collection_list(f, f.area(), &browser, db)
                }
                BrowserView::GraphList(db) => render_graph_list(f, f.area(), &browser, db),
                BrowserView::CollectionProperties(db, coll) => {
                    render_collection_properties(f, f.area(), &browser, db, coll)
                }
                BrowserView::DocumentViewer(db, coll) => {
                    render_document_viewer(f, f.area(), &browser, db, coll)
                }
                BrowserView::GraphProperties(db, graph) => {
                    render_graph_properties(f, f.area(), &browser, db, graph)
                }
            }

            // Render input dialog on top if active
            if let InputState::EnteringDocumentCount(input) = &browser.input_state {
                render_input_dialog(f, f.area(), input);
            }
        })?;

        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                // Handle input dialog first if active
                if let InputState::EnteringDocumentCount(ref mut input) = browser.input_state {
                    match key.code {
                        KeyCode::Char(c) if c.is_ascii_digit() => {
                            input.push(c);
                        }
                        KeyCode::Backspace => {
                            input.pop();
                        }
                        KeyCode::Enter => {
                            let count: usize = input.parse().unwrap_or(10);
                            browser.input_state = InputState::None;

                            // Load documents based on current view
                            if let BrowserView::CollectionList(db) = &browser.view {
                                if browser.selected_coll_index < browser.collections.len() {
                                    let coll_name = browser.collections
                                        [browser.selected_coll_index]
                                        .info
                                        .name
                                        .clone();
                                    let db_clone = db.clone();
                                    browser
                                        .load_documents(app_state, &db_clone, &coll_name, count)
                                        .await?;
                                    browser.view = BrowserView::DocumentViewer(db_clone, coll_name);
                                }
                            }
                        }
                        KeyCode::Esc => {
                            browser.input_state = InputState::None;
                        }
                        _ => {}
                    }
                    continue;
                }

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
                        KeyCode::Backspace => {
                            // Navigate back to previous view if we came from graph view
                            if let Some((prev_view, prev_index)) = browser.navigation_stack.pop() {
                                match &prev_view {
                                    BrowserView::GraphList(prev_db) => {
                                        browser.load_graphs(app_state, prev_db).await?;
                                        browser.selected_graph_index = prev_index;
                                        browser.view = prev_view;
                                    }
                                    _ => {
                                        // For other views, just restore
                                        browser.view = prev_view;
                                    }
                                }
                            }
                        }
                        KeyCode::Char('g') | KeyCode::Char('G') => {
                            browser.load_graphs(app_state, &db).await?;
                            browser.view = BrowserView::GraphList(db.clone());
                        }
                        KeyCode::Char(' ') => {
                            // Open input dialog for document count
                            browser.input_state =
                                InputState::EnteringDocumentCount("10".to_string());
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
                    BrowserView::GraphList(db) => match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => {
                            browser.view = BrowserView::DatabaseList;
                            browser.graphs.clear();
                        }
                        KeyCode::Char('c') | KeyCode::Char('C') => {
                            browser.view = BrowserView::CollectionList(db.clone());
                        }
                        KeyCode::Enter => {
                            // Determine what was selected
                            if let Some((graph_idx, edge_idx)) = browser.find_selected_graph_item()
                            {
                                if edge_idx.is_none() {
                                    // Graph row selected - show graph properties
                                    let graph_name = browser.graphs[graph_idx].name.clone();
                                    browser
                                        .load_graph_details(app_state, &db, &graph_name)
                                        .await?;
                                    browser.view =
                                        BrowserView::GraphProperties(db.clone(), graph_name);
                                } else {
                                    // Edge definition row selected - navigate to edge collection
                                    let edge_idx = edge_idx.unwrap();
                                    let edge_collection = browser.graphs[graph_idx]
                                        .edge_definitions[edge_idx]
                                        .collection
                                        .clone();

                                    // Push current view to navigation stack
                                    browser
                                        .navigation_stack
                                        .push((browser.view.clone(), browser.selected_graph_index));

                                    // Load collections and find the edge collection
                                    browser.load_collections(app_state, &db).await?;
                                    if let Some(pos) = browser
                                        .collections
                                        .iter()
                                        .position(|c| c.info.name == edge_collection)
                                    {
                                        browser.selected_coll_index = pos;
                                    }
                                    browser.view = BrowserView::CollectionList(db.clone());
                                }
                            }
                        }
                        KeyCode::Char('v') | KeyCode::Char('V') => {
                            // Navigate to first vertex collection in the edge definition
                            if let Some((graph_idx, Some(edge_idx))) =
                                browser.find_selected_graph_item()
                            {
                                let edge_def =
                                    &browser.graphs[graph_idx].edge_definitions[edge_idx];
                                if let Some(first_from) = edge_def.from.first().cloned() {
                                    // Push current view to navigation stack
                                    browser
                                        .navigation_stack
                                        .push((browser.view.clone(), browser.selected_graph_index));

                                    // Load collections and find the vertex collection
                                    browser.load_collections(app_state, &db).await?;
                                    if let Some(pos) = browser
                                        .collections
                                        .iter()
                                        .position(|c| c.info.name == first_from)
                                    {
                                        browser.selected_coll_index = pos;
                                    }
                                    browser.view = BrowserView::CollectionList(db.clone());
                                }
                            }
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            if !browser.graphs.is_empty() {
                                // Calculate total number of rows (graphs + edge definitions + spacing)
                                let mut total_rows = 0;
                                for graph in &browser.graphs {
                                    total_rows += 1; // graph name row
                                    total_rows += graph.edge_definitions.len(); // edge definition rows
                                }
                                total_rows += browser.graphs.len().saturating_sub(1); // spacing rows between graphs

                                if total_rows > 0 {
                                    browser.selected_graph_index =
                                        (browser.selected_graph_index + 1) % total_rows;
                                }
                            }
                        }
                        KeyCode::Up | KeyCode::Char('k') => {
                            if !browser.graphs.is_empty() {
                                // Calculate total number of rows
                                let mut total_rows = 0;
                                for graph in &browser.graphs {
                                    total_rows += 1; // graph name row
                                    total_rows += graph.edge_definitions.len(); // edge definition rows
                                }
                                total_rows += browser.graphs.len().saturating_sub(1); // spacing rows between graphs

                                if total_rows > 0 {
                                    browser.selected_graph_index =
                                        if browser.selected_graph_index == 0 {
                                            total_rows - 1
                                        } else {
                                            browser.selected_graph_index - 1
                                        };
                                }
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
                    BrowserView::DocumentViewer(db, _coll) => match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => {
                            browser.view = BrowserView::CollectionList(db.clone());
                            browser.documents.clear();
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
                    BrowserView::GraphProperties(db, _graph) => match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => {
                            browser.view = BrowserView::GraphList(db.clone());
                            browser.graph_details = None;
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
