pub mod aql;

use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
};
use reqwest::Client;
use serde::Deserialize;
use std::io;

#[derive(Debug, Deserialize)]
pub struct ArangoVersion {
    pub license: String,
    pub version: String,
}

#[derive(Debug, Deserialize)]
struct DatabaseListResponse {
    result: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct CollectionInfo {
    pub name: String,
    #[serde(rename = "type")]
    pub collection_type: u32,
    #[serde(rename = "isSystem")]
    pub is_system: bool,
}

#[derive(Debug, Deserialize)]
struct CollectionListResponse {
    result: Vec<CollectionInfo>,
}

#[derive(Debug, Deserialize, Clone, serde::Serialize)]
pub struct CollectionCount {
    pub error: bool,
    pub code: u16,
    #[serde(rename = "writeConcern")]
    pub write_concern: Option<u32>,
    #[serde(rename = "waitForSync")]
    pub wait_for_sync: Option<bool>,
    #[serde(rename = "usesRevisionsAsDocumentIds")]
    pub uses_revisions_as_document_ids: Option<bool>,
    #[serde(rename = "syncByRevision")]
    pub sync_by_revision: Option<bool>,
    #[serde(rename = "statusString")]
    pub status_string: Option<String>,
    pub id: Option<String>,
    #[serde(rename = "isSmartChild")]
    pub is_smart_child: Option<bool>,
    pub schema: Option<serde_json::Value>,
    pub name: String,
    #[serde(rename = "type")]
    pub collection_type: u32,
    pub status: u32,
    pub count: u64,
    #[serde(rename = "cacheEnabled")]
    pub cache_enabled: Option<bool>,
    #[serde(rename = "isSystem")]
    pub is_system: bool,
    #[serde(rename = "internalValidatorType")]
    pub internal_validator_type: Option<u32>,
    #[serde(rename = "globallyUniqueId")]
    pub globally_unique_id: Option<String>,
    #[serde(rename = "keyOptions")]
    pub key_options: Option<serde_json::Value>,
    #[serde(rename = "computedValues")]
    pub computed_values: Option<serde_json::Value>,
    #[serde(rename = "objectId")]
    pub object_id: Option<String>,
}

#[derive(Debug, Deserialize, Clone, serde::Serialize)]
pub struct EdgeDefinition {
    pub collection: String,
    pub from: Vec<String>,
    pub to: Vec<String>,
}

#[derive(Debug, Deserialize, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphInfo {
    #[serde(rename = "_id")]
    pub id: String,
    #[serde(rename = "_key")]
    pub key: String,
    #[serde(rename = "_rev")]
    pub rev: String,
    pub edge_definitions: Vec<EdgeDefinition>,
    pub orphan_collections: Vec<String>,
    pub name: String,
    pub is_smart: Option<bool>,
    pub is_disjoint: Option<bool>,
    pub smart_graph_attribute: Option<String>,
    pub number_of_shards: Option<u32>,
    pub replication_factor: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct GraphListResponse {
    graphs: Vec<GraphInfo>,
}

#[derive(Debug, Clone)]
pub struct DatabaseStats {
    pub name: String,
    pub doc_collections: usize,
    pub edge_collections: usize,
    pub system_collections: usize,
    pub accessible: bool,
}

#[derive(Debug, Clone)]
pub struct CollectionWithCount {
    pub info: CollectionInfo,
    pub count: Option<u64>,
}

#[derive(Clone, Debug)]
pub enum BrowserView {
    DatabaseList,
    CollectionList(String),               // database name
    GraphList(String),                    // database name
    CollectionProperties(String, String), // database name, collection name
    DocumentViewer(String, String),       // database name, collection name
    GraphProperties(String, String),      // database name, graph name
    AqlQueryInput(String),                // database name
    AqlQueryResults(String),              // database name
}

pub enum InputState {
    None,
    EnteringDocumentCount(String), // Current input string
}

pub struct DatabaseBrowser {
    pub view: BrowserView,
    pub database_stats: Vec<DatabaseStats>,
    pub selected_db_index: usize,
    pub collections: Vec<CollectionWithCount>,
    pub selected_coll_index: usize,
    pub graphs: Vec<GraphInfo>,
    pub selected_graph_index: usize,
    pub collection_details: Option<CollectionCount>,
    pub scroll_offset: usize,
    pub accessible: bool,
    pub input_state: InputState,
    pub documents: Vec<serde_json::Value>,
    pub navigation_stack: Vec<(BrowserView, usize)>, // Stack to track navigation history (view, selected_index)
    pub graph_details: Option<GraphInfo>,
    pub aql_state: Option<aql::AqlState>,
}

impl DatabaseBrowser {
    pub fn new() -> Self {
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
            aql_state: None,
        }
    }

    pub async fn load_databases(
        &mut self,
        client: &Client,
        endpoint: &str,
        username: &str,
        password: &str,
    ) -> Result<()> {
        match get_databases(client, endpoint, username, password).await {
            Ok(databases) => {
                self.accessible = true;
                let mut stats = Vec::new();
                for db in databases {
                    let db_stats =
                        get_database_stats(client, endpoint, &db, username, password).await;
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

    pub async fn load_collections(
        &mut self,
        client: &Client,
        endpoint: &str,
        database: &str,
        username: &str,
        password: &str,
    ) -> Result<()> {
        let collections = get_collections(client, endpoint, database, username, password).await?;

        let mut collections_with_count = Vec::new();
        for coll in collections {
            let count =
                get_collection_count(client, endpoint, database, &coll.name, username, password)
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

    pub async fn load_collection_details(
        &mut self,
        client: &Client,
        endpoint: &str,
        database: &str,
        collection: &str,
        username: &str,
        password: &str,
    ) -> Result<()> {
        let details =
            get_collection_count(client, endpoint, database, collection, username, password)
                .await?;

        self.collection_details = Some(details);
        self.scroll_offset = 0;
        Ok(())
    }

    pub async fn load_graphs(
        &mut self,
        client: &Client,
        endpoint: &str,
        database: &str,
        username: &str,
        password: &str,
    ) -> Result<()> {
        let graphs = get_graphs(client, endpoint, database, username, password).await?;

        self.graphs = graphs;
        self.selected_graph_index = 0;
        self.scroll_offset = 0;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn load_documents(
        &mut self,
        client: &Client,
        endpoint: &str,
        database: &str,
        collection: &str,
        count: usize,
        username: &str,
        password: &str,
    ) -> Result<()> {
        let query = format!("FOR d IN `{}` LIMIT {} RETURN d", collection, count);
        let documents =
            aql::execute_aql_query(client, endpoint, database, &query, username, password).await?;

        self.documents = documents;
        self.scroll_offset = 0;
        Ok(())
    }

    // Helper to find which graph and edge definition row is selected
    pub fn find_selected_graph_item(&self) -> Option<(usize, Option<usize>)> {
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

    pub async fn load_graph_details(
        &mut self,
        _client: &Client,
        _endpoint: &str,
        _database: &str,
        graph_name: &str,
        _username: &str,
        _password: &str,
    ) -> Result<()> {
        // Find the graph in our list
        let graph = self.graphs.iter().find(|g| g.name == graph_name).cloned();
        self.graph_details = graph;
        self.scroll_offset = 0;
        Ok(())
    }

    pub fn init_aql_state(&mut self) {
        self.aql_state = Some(aql::AqlState::new());
    }
}

pub async fn check_arango_version(
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

    let response_text = response
        .text()
        .await
        .context("Failed to read ArangoDB version response")?;

    let version = serde_json::from_str(&response_text);
    if let Err(err) = version {
        anyhow::bail!("Failed to parse ArangoDB version response: {}", err);
    }
    Ok(version.unwrap())
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

    let response_text = response
        .text()
        .await
        .context("Failed to read database list response")?;

    let db_response = serde_json::from_str(&response_text);
    if let Err(err) = db_response {
        anyhow::bail!("Failed to parse database list response: {}", err);
    }
    let db_response: DatabaseListResponse = db_response.unwrap();

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

    let response_text = response
        .text()
        .await
        .context("Failed to read collection list response")?;

    let coll_response = serde_json::from_str(&response_text);
    if let Err(err) = coll_response {
        anyhow::bail!("Failed to parse collection list response: {}", err);
    }
    let coll_response: CollectionListResponse = coll_response.unwrap();

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

    let response_text = response
        .text()
        .await
        .context("Failed to read collection count response")?;

    let count_response = serde_json::from_str(&response_text);
    if let Err(err) = count_response {
        anyhow::bail!("Failed to parse collection count response: {}", err);
    }
    Ok(count_response.unwrap())
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

    let response_text = response
        .text()
        .await
        .context("Failed to read graph list response")?;

    let graph_response = serde_json::from_str(&response_text);
    if let Err(err) = graph_response {
        anyhow::bail!("Failed to parse graph list response: {}", err);
    }
    let graph_response: GraphListResponse = graph_response.unwrap();

    Ok(graph_response.graphs)
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

pub fn render_database_list(f: &mut Frame, area: Rect, browser: &DatabaseBrowser) {
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
        ratatui::layout::Constraint::Percentage(40),
        ratatui::layout::Constraint::Percentage(20),
        ratatui::layout::Constraint::Percentage(20),
        ratatui::layout::Constraint::Percentage(20),
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

pub fn render_collection_list(
    f: &mut Frame,
    area: Rect,
    browser: &DatabaseBrowser,
    database: &str,
) {
    if browser.collections.is_empty() {
        let empty = Paragraph::new("No collections found")
            .style(Style::default().fg(Color::Yellow))
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!("Database: {} | G: Graphs | A: AQL Query", database)),
            );
        f.render_widget(empty, area);
        return;
    }

    let total_collections = browser.collections.len();
    let total_docs: u64 = browser.collections.iter().filter_map(|c| c.count).sum();

    let title = format!(
        "Database: {} | Collections: {} | Total Documents: {} | G: Graphs | A: AQL Query | SPACE: view documents",
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
        ratatui::layout::Constraint::Percentage(50),
        ratatui::layout::Constraint::Percentage(15),
        ratatui::layout::Constraint::Percentage(10),
        ratatui::layout::Constraint::Percentage(25),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().borders(Borders::ALL).title(title))
        .column_spacing(2);

    f.render_widget(table, area);
}

pub fn render_graph_list(f: &mut Frame, area: Rect, browser: &DatabaseBrowser, database: &str) {
    if browser.graphs.is_empty() {
        let empty = Paragraph::new("No graphs found")
            .style(Style::default().fg(Color::Yellow))
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL).title(format!(
                "Database: {} | C: Collections | A: AQL Query",
                database
            )));
        f.render_widget(empty, area);
        return;
    }

    let total_graphs = browser.graphs.len();

    // Determine if we're on a graph row or edge definition row
    let title = if let Some((_, edge_idx)) = browser.find_selected_graph_item() {
        if edge_idx.is_some() {
            // On edge definition row
            format!(
                "Database: {} | Graphs: {} | C: Collections | A: AQL Query | ENTER: Edge collection | V: Vertex collection",
                database, total_graphs
            )
        } else {
            // On graph row
            format!(
                "Database: {} | Graphs: {} | C: Collections | A: AQL Query | ENTER: Graph details (JSON)",
                database, total_graphs
            )
        }
    } else {
        // Fallback
        format!(
            "Database: {} | Graphs: {} | C: Collections | A: AQL Query",
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
        ratatui::layout::Constraint::Percentage(25),
        ratatui::layout::Constraint::Percentage(20),
        ratatui::layout::Constraint::Percentage(40),
        ratatui::layout::Constraint::Percentage(15),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().borders(Borders::ALL).title(title))
        .column_spacing(2);

    f.render_widget(table, area);
}

pub fn render_collection_properties(
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

pub fn render_document_viewer(
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

pub fn render_input_dialog(f: &mut Frame, area: Rect, input_text: &str) {
    use ratatui::layout::{Constraint, Direction, Layout};
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

pub fn render_graph_properties(
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
pub async fn run_database_browser(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    client: &Client,
    endpoint: &str,
    username: &str,
    password: &str,
) -> Result<()> {
    let mut browser = DatabaseBrowser::new();
    browser
        .load_databases(client, endpoint, username, password)
        .await?;

    loop {
        terminal.draw(|f| {
            match &browser.view.clone() {
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
                BrowserView::AqlQueryInput(db) => {
                    if let Some(aql_state) = &mut browser.aql_state {
                        aql::render_aql_query_input(f, f.area(), aql_state, db)
                    }
                }
                BrowserView::AqlQueryResults(db) => {
                    if let Some(aql_state) = &browser.aql_state {
                        aql::render_aql_query_results(f, f.area(), aql_state, db)
                    }
                }
            }

            // Render input dialog on top if active
            if let InputState::EnteringDocumentCount(input) = &browser.input_state {
                render_input_dialog(f, f.area(), input);
            }
        })?;

        if let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
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
                        if let BrowserView::CollectionList(db) = &browser.view
                            && browser.selected_coll_index < browser.collections.len()
                        {
                            let coll_name = browser.collections[browser.selected_coll_index]
                                .info
                                .name
                                .clone();
                            let db_clone = db.clone();
                            browser
                                .load_documents(
                                    client, endpoint, &db_clone, &coll_name, count, username,
                                    password,
                                )
                                .await?;
                            browser.view = BrowserView::DocumentViewer(db_clone, coll_name);
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
                                browser
                                    .load_collections(
                                        client, endpoint, &db_name, username, password,
                                    )
                                    .await?;
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
                                    browser
                                        .load_graphs(client, endpoint, username, password, prev_db)
                                        .await?;
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
                        browser
                            .load_graphs(client, endpoint, &db, username, password)
                            .await?;
                        browser.view = BrowserView::GraphList(db.clone());
                    }
                    KeyCode::Char('a') | KeyCode::Char('A') => {
                        // Open AQL query view (initialize state only if needed)
                        if browser.aql_state.is_none() {
                            browser.init_aql_state();
                        }
                        browser.view = BrowserView::AqlQueryInput(db.clone());
                    }
                    KeyCode::Char(' ') => {
                        // Open input dialog for document count
                        browser.input_state = InputState::EnteringDocumentCount("10".to_string());
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
                                .load_collection_details(
                                    client, endpoint, &db, &coll_name, username, password,
                                )
                                .await?;
                            browser.view = BrowserView::CollectionProperties(db.clone(), coll_name);
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
                    KeyCode::Char('a') | KeyCode::Char('A') => {
                        // Open AQL query view (initialize state only if needed)
                        if browser.aql_state.is_none() {
                            browser.init_aql_state();
                        }
                        browser.view = BrowserView::AqlQueryInput(db.clone());
                    }
                    KeyCode::Enter => {
                        // Determine what was selected
                        if let Some((graph_idx, edge_idx)) = browser.find_selected_graph_item() {
                            if let Some(edge_idx) = edge_idx {
                                // Edge definition row selected - navigate to edge collection
                                let edge_collection = browser.graphs[graph_idx].edge_definitions
                                    [edge_idx]
                                    .collection
                                    .clone();

                                // Push current view to navigation stack
                                browser
                                    .navigation_stack
                                    .push((browser.view.clone(), browser.selected_graph_index));

                                // Load collections and find the edge collection
                                browser
                                    .load_collections(client, endpoint, &db, username, password)
                                    .await?;
                                if let Some(pos) = browser
                                    .collections
                                    .iter()
                                    .position(|c| c.info.name == edge_collection)
                                {
                                    browser.selected_coll_index = pos;
                                }
                                browser.view = BrowserView::CollectionList(db.clone());
                            } else {
                                // Graph row selected - show graph properties
                                let graph_name = browser.graphs[graph_idx].name.clone();
                                browser
                                    .load_graph_details(
                                        client,
                                        endpoint,
                                        &db,
                                        &graph_name,
                                        username,
                                        password,
                                    )
                                    .await?;
                                browser.view = BrowserView::GraphProperties(db.clone(), graph_name);
                            }
                        }
                    }
                    KeyCode::Char('v') | KeyCode::Char('V') => {
                        // Navigate to first vertex collection in the edge definition
                        if let Some((graph_idx, Some(edge_idx))) =
                            browser.find_selected_graph_item()
                        {
                            let edge_def = &browser.graphs[graph_idx].edge_definitions[edge_idx];
                            if let Some(first_from) = edge_def.from.first().cloned() {
                                // Push current view to navigation stack
                                browser
                                    .navigation_stack
                                    .push((browser.view.clone(), browser.selected_graph_index));

                                // Load collections and find the vertex collection
                                browser
                                    .load_collections(client, endpoint, &db, username, password)
                                    .await?;
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
                                browser.selected_graph_index = if browser.selected_graph_index == 0
                                {
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
                BrowserView::AqlQueryInput(db) => {
                    // Handle AQL input view keys
                    use crossterm::event::KeyModifiers;

                    if let Some(aql_state) = &mut browser.aql_state {
                        // Check for Ctrl+Enter first (before other key handling)
                        if key.code == KeyCode::Enter
                            && key.modifiers.contains(KeyModifiers::CONTROL)
                        {
                            // Execute query
                            if aql_state.parameters_valid && aql_state.options_valid {
                                let query_text = aql_state.query_textarea.lines().join("\n");
                                let params_text = aql_state.parameters_textarea.lines().join("\n");
                                let options_text = aql_state.options_textarea.lines().join("\n");

                                // Parse parameters
                                let bind_vars = if params_text.trim() == "{}" {
                                    None
                                } else {
                                    serde_json::from_str(&params_text).ok()
                                };

                                // Parse options
                                let options: Result<serde_json::Value, _> =
                                    serde_json::from_str(&options_text);

                                if let Ok(opts) = options {
                                    let batch_size =
                                        opts["batchSize"].as_u64().unwrap_or(1000) as usize;
                                    let stream = opts["stream"].as_bool().unwrap_or(true);
                                    let max_documents =
                                        opts["maxDocuments"].as_u64().unwrap_or(100000) as usize;

                                    // Execute query
                                    aql_state.is_fetching = true;
                                    aql_state.results.clear();
                                    aql_state.total_fetched = 0;
                                    aql_state.has_more = false;
                                    aql_state.cursor_id = None;

                                    browser.view = BrowserView::AqlQueryResults(db.clone());

                                    // Start fetching - first batch
                                    match aql::execute_aql_query_with_params(
                                        client,
                                        endpoint,
                                        &db,
                                        &query_text,
                                        bind_vars.clone(),
                                        batch_size,
                                        stream,
                                        username,
                                        password,
                                    )
                                    .await
                                    {
                                        Ok(response) => {
                                            // Store initial results
                                            let mut all_results = response.result;
                                            let mut has_more = response.has_more;
                                            let mut cursor_id = response.id;

                                            // Continue fetching if there's more
                                            while has_more && all_results.len() < max_documents {
                                                if let Some(ref cursor) = cursor_id {
                                                    match aql::fetch_cursor_next(
                                                        client, endpoint, &db, cursor, username,
                                                        password,
                                                    )
                                                    .await
                                                    {
                                                        Ok(cursor_response) => {
                                                            all_results
                                                                .extend(cursor_response.result);
                                                            has_more = cursor_response.has_more;
                                                            cursor_id = cursor_response.id;
                                                        }
                                                        Err(_) => {
                                                            break;
                                                        }
                                                    }
                                                } else {
                                                    break;
                                                }
                                            }

                                            // Update state once with all results
                                            if let Some(aql_state) = browser.aql_state.as_mut() {
                                                aql_state.results = all_results;
                                                aql_state.total_fetched = aql_state.results.len();
                                                aql_state.has_more = has_more;
                                                aql_state.cursor_id = cursor_id;
                                                aql_state.is_fetching = false;
                                            }
                                        }
                                        Err(_e) => {
                                            // Query failed - go back to input
                                            browser.view = BrowserView::AqlQueryInput(db.clone());
                                            if let Some(aql_state) = &mut browser.aql_state {
                                                aql_state.is_fetching = false;
                                            }
                                        }
                                    }
                                }
                            }
                        } else {
                            // Handle other keys
                            match key.code {
                                KeyCode::Char('q') | KeyCode::Esc => {
                                    // Return to collection list but keep AQL state
                                    browser.view = BrowserView::CollectionList(db.clone());
                                }
                                KeyCode::Tab => {
                                    // Switch between fields including Submit button
                                    aql_state.active_field = match aql_state.active_field {
                                        aql::AqlInputField::Query => aql::AqlInputField::Parameters,
                                        aql::AqlInputField::Parameters => {
                                            aql::AqlInputField::Options
                                        }
                                        aql::AqlInputField::Options => aql::AqlInputField::Submit,
                                        aql::AqlInputField::Submit => aql::AqlInputField::Query,
                                    };
                                }
                                KeyCode::Enter => {
                                    // Check if we're on the Submit button
                                    if matches!(aql_state.active_field, aql::AqlInputField::Submit)
                                    {
                                        // Execute query when Enter is pressed on Submit button
                                        if aql_state.parameters_valid && aql_state.options_valid {
                                            let query_text =
                                                aql_state.query_textarea.lines().join("\n");
                                            let params_text =
                                                aql_state.parameters_textarea.lines().join("\n");
                                            let options_text =
                                                aql_state.options_textarea.lines().join("\n");

                                            // Parse parameters
                                            let bind_vars = if params_text.trim() == "{}" {
                                                None
                                            } else {
                                                serde_json::from_str(&params_text).ok()
                                            };

                                            // Parse options
                                            let options: Result<serde_json::Value, _> =
                                                serde_json::from_str(&options_text);

                                            if let Ok(opts) = options {
                                                let batch_size =
                                                    opts["batchSize"].as_u64().unwrap_or(1000)
                                                        as usize;
                                                let stream =
                                                    opts["stream"].as_bool().unwrap_or(true);
                                                let max_documents =
                                                    opts["maxDocuments"].as_u64().unwrap_or(100000)
                                                        as usize;

                                                // Execute query (same logic as before)
                                                aql_state.is_fetching = true;
                                                aql_state.results.clear();
                                                aql_state.total_fetched = 0;
                                                aql_state.has_more = false;
                                                aql_state.cursor_id = None;

                                                browser.view =
                                                    BrowserView::AqlQueryResults(db.clone());

                                                // Start fetching
                                                match aql::execute_aql_query_with_params(
                                                    client,
                                                    endpoint,
                                                    &db,
                                                    &query_text,
                                                    bind_vars.clone(),
                                                    batch_size,
                                                    stream,
                                                    username,
                                                    password,
                                                )
                                                .await
                                                {
                                                    Ok(response) => {
                                                        let mut all_results = response.result;
                                                        let mut has_more = response.has_more;
                                                        let mut cursor_id = response.id;

                                                        while has_more
                                                            && all_results.len() < max_documents
                                                        {
                                                            if let Some(ref cursor) = cursor_id {
                                                                match aql::fetch_cursor_next(
                                                                    client, endpoint, &db, cursor,
                                                                    username, password,
                                                                )
                                                                .await
                                                                {
                                                                    Ok(cursor_response) => {
                                                                        all_results.extend(
                                                                            cursor_response.result,
                                                                        );
                                                                        has_more = cursor_response
                                                                            .has_more;
                                                                        cursor_id =
                                                                            cursor_response.id;
                                                                    }
                                                                    Err(_) => {
                                                                        break;
                                                                    }
                                                                }
                                                            } else {
                                                                break;
                                                            }
                                                        }

                                                        if let Some(aql_state) =
                                                            browser.aql_state.as_mut()
                                                        {
                                                            aql_state.results = all_results;
                                                            aql_state.total_fetched =
                                                                aql_state.results.len();
                                                            aql_state.has_more = has_more;
                                                            aql_state.cursor_id = cursor_id;
                                                            aql_state.is_fetching = false;
                                                        }
                                                    }
                                                    Err(_e) => {
                                                        browser.view =
                                                            BrowserView::AqlQueryInput(db.clone());
                                                        if let Some(aql_state) =
                                                            &mut browser.aql_state
                                                        {
                                                            aql_state.is_fetching = false;
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    } else {
                                        // Pass Enter to the active TextArea for newline
                                        match aql_state.active_field {
                                            aql::AqlInputField::Query => {
                                                aql_state.query_textarea.input(key);
                                            }
                                            aql::AqlInputField::Parameters => {
                                                aql_state.parameters_textarea.input(key);
                                                let text = aql_state
                                                    .parameters_textarea
                                                    .lines()
                                                    .join("\n");
                                                aql_state.parameters_valid =
                                                    serde_json::from_str::<serde_json::Value>(
                                                        &text,
                                                    )
                                                    .is_ok();
                                            }
                                            aql::AqlInputField::Options => {
                                                aql_state.options_textarea.input(key);
                                                let text =
                                                    aql_state.options_textarea.lines().join("\n");
                                                aql_state.options_valid =
                                                    serde_json::from_str::<serde_json::Value>(
                                                        &text,
                                                    )
                                                    .is_ok();
                                            }
                                            aql::AqlInputField::Submit => {
                                                // No input on submit button
                                            }
                                        }
                                    }
                                }
                                _ => {
                                    // Pass all other keys to the active TextArea
                                    match aql_state.active_field {
                                        aql::AqlInputField::Query => {
                                            aql_state.query_textarea.input(key);
                                        }
                                        aql::AqlInputField::Parameters => {
                                            aql_state.parameters_textarea.input(key);
                                            // Validate JSON after input
                                            let text =
                                                aql_state.parameters_textarea.lines().join("\n");
                                            aql_state.parameters_valid =
                                                serde_json::from_str::<serde_json::Value>(&text)
                                                    .is_ok();
                                        }
                                        aql::AqlInputField::Options => {
                                            aql_state.options_textarea.input(key);
                                            // Validate JSON after input
                                            let text =
                                                aql_state.options_textarea.lines().join("\n");
                                            aql_state.options_valid =
                                                serde_json::from_str::<serde_json::Value>(&text)
                                                    .is_ok();
                                        }
                                        aql::AqlInputField::Submit => {
                                            // No input on submit button
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                BrowserView::AqlQueryResults(db) => {
                    if let Some(aql_state) = &mut browser.aql_state {
                        match key.code {
                            KeyCode::Char('q') | KeyCode::Esc => {
                                browser.view = BrowserView::AqlQueryInput(db.clone());
                                aql_state.scroll_offset = 0;
                                aql_state.current_page = 0;
                            }
                            KeyCode::Down | KeyCode::Char('j') => {
                                aql_state.scroll_offset = aql_state.scroll_offset.saturating_add(1);
                            }
                            KeyCode::Up | KeyCode::Char('k') => {
                                aql_state.scroll_offset = aql_state.scroll_offset.saturating_sub(1);
                            }
                            KeyCode::PageDown => {
                                aql_state.scroll_offset =
                                    aql_state.scroll_offset.saturating_add(10);
                            }
                            KeyCode::PageUp => {
                                aql_state.scroll_offset =
                                    aql_state.scroll_offset.saturating_sub(10);
                            }
                            KeyCode::Left => {
                                if aql_state.current_page > 0 {
                                    aql_state.current_page -= 1;
                                    aql_state.scroll_offset = 0;
                                }
                            }
                            KeyCode::Right => {
                                let page_size = 100;
                                let total_pages = aql_state.results.len().div_ceil(page_size);
                                if aql_state.current_page + 1 < total_pages {
                                    aql_state.current_page += 1;
                                    aql_state.scroll_offset = 0;
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }
}
