use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use hmac::{Hmac, Mac};
use jwt::header::HeaderType;
use jwt::{AlgorithmType, Header, SignWithKey, Token};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::io;
use tui_textarea::TextArea;

// GAE Version structure
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
// Note that for whatever reason we use camel case here and for backwards compatibility
// reasons we cannot change this any more.
pub struct GaeVersion {
    pub api_max_version: u32,
    pub api_min_version: u32,
    pub version: String,
}

// GAE Graphs API structures
#[derive(Debug, Deserialize, Clone)]
pub struct GaeGraph {
    graph_id: u64,
    number_of_vertices: u64,
    number_of_edges: u64,
    memory_usage: u64,
    memory_per_vertex: u64,
    memory_per_edge: u64,
}

// GAE Jobs API structures
#[derive(Debug, Deserialize, Clone)]
pub struct GaeJob {
    job_id: u64,
    graph_id: u64,
    total: u32,
    progress: u32,
    error: bool,
    error_code: i32,
    error_message: String,
    comp_type: String,
    memory_usage: u64,
    runtime_in_microseconds: u64,
}

// JWT token structure
pub struct GaeJwtToken {
    pub token: String,
    pub expiry: std::time::SystemTime,
}

// JWT Claims structure for GAE tokens
#[derive(Debug, Serialize, Deserialize)]
struct GaeJwtClaims {
    iss: String,
    exp: u64,
}

// API functions
pub async fn check_gae_version(
    client: &Client,
    endpoint: &str,
    token: Option<&str>,
) -> Result<GaeVersion> {
    let url = format!("{}/v1/version", endpoint.trim_end_matches('/'));
    let mut request = client.get(&url);

    if let Some(jwt_token) = token {
        request = request.bearer_auth(jwt_token);
    }

    let response = request.send().await.context("Failed to connect to GAE")?;

    if !response.status().is_success() {
        anyhow::bail!("GAE returned error status: {}", response.status());
    }

    let response_text = response
        .text()
        .await
        .context("Failed to read GAE version response")?;

    let version = serde_json::from_str(&response_text);
    if let Err(err) = version {
        anyhow::bail!("Failed to parse GAE version response: {}", err);
    }
    Ok(version.unwrap())
}

async fn get_gae_graphs(
    client: &Client,
    endpoint: &str,
    token: Option<&str>,
) -> Result<Vec<GaeGraph>> {
    let url = format!("{}/v1/graphs", endpoint.trim_end_matches('/'));
    let mut request = client.get(&url);

    if let Some(jwt_token) = token {
        request = request.bearer_auth(jwt_token);
    }

    let response = request.send().await.context("Failed to fetch GAE graphs")?;

    if !response.status().is_success() {
        anyhow::bail!("Failed to fetch GAE graphs: {}", response.status());
    }

    // Get the response text for debugging
    let response_text = response
        .text()
        .await
        .context("Failed to read GAE graphs response")?;

    // The API returns a plain array of graphs
    let graphs = serde_json::from_str(&response_text);
    if let Err(err) = graphs {
        anyhow::bail!("Failed to parse GAE graphs response: {}", err);
    }
    Ok(graphs.unwrap())
}

async fn get_gae_jobs(client: &Client, endpoint: &str, token: Option<&str>) -> Result<Vec<GaeJob>> {
    let url = format!("{}/v1/jobs", endpoint.trim_end_matches('/'));
    let mut request = client.get(&url);

    if let Some(jwt_token) = token {
        request = request.bearer_auth(jwt_token);
    }

    let response = request.send().await.context("Failed to fetch GAE jobs")?;

    if !response.status().is_success() {
        anyhow::bail!("Failed to fetch GAE jobs: {}", response.status());
    }

    // Get the response text for debugging
    let response_text = response
        .text()
        .await
        .context("Failed to read GAE jobs response")?;

    // The API returns a plain array of jobs
    let jobs = serde_json::from_str(&response_text);
    if let Err(err) = jobs {
        anyhow::bail!("Failed to parse GAE jobs response: {}", err);
    }
    Ok(jobs.unwrap())
}

async fn delete_gae_graph(
    client: &Client,
    endpoint: &str,
    graph_id: u64,
    token: Option<&str>,
) -> Result<()> {
    let url = format!("{}/v1/graphs/{}", endpoint.trim_end_matches('/'), graph_id);
    let mut request = client.delete(&url);

    if let Some(jwt_token) = token {
        request = request.bearer_auth(jwt_token);
    }

    let response = request.send().await.context("Failed to delete GAE graph")?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();
        anyhow::bail!(
            "Failed to delete GAE graph {}: {} - {}",
            graph_id,
            status,
            error_text
        );
    }

    Ok(())
}

async fn delete_gae_job(
    client: &Client,
    endpoint: &str,
    job_id: u64,
    token: Option<&str>,
) -> Result<()> {
    let url = format!("{}/v1/jobs/{}", endpoint.trim_end_matches('/'), job_id);
    let mut request = client.delete(&url);

    if let Some(jwt_token) = token {
        request = request.bearer_auth(jwt_token);
    }

    let response = request.send().await.context("Failed to delete GAE job")?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();
        anyhow::bail!(
            "Failed to delete GAE job {}: {} - {}",
            job_id,
            status,
            error_text
        );
    }

    Ok(())
}

// Generate a JWT token for GAE authentication
pub fn create_gae_jwt_token(secret: &[u8], expiry_in_seconds: u64) -> Result<String> {
    let key: Hmac<Sha256> =
        Hmac::new_from_slice(secret).context("Failed to create HMAC key from secret")?;

    let exp = (std::time::SystemTime::now() + std::time::Duration::from_secs(expiry_in_seconds))
        .duration_since(std::time::UNIX_EPOCH)
        .context("System time error")?
        .as_secs();

    let header = Header {
        algorithm: AlgorithmType::Hs256,
        type_: Some(HeaderType::JsonWebToken),
        ..Default::default()
    };

    let claims = GaeJwtClaims {
        iss: "arangotui".to_string(),
        exp,
    };

    let token = Token::new(header, claims)
        .sign_with_key(&key)
        .context("Failed to sign JWT token")?;

    Ok(token.as_str().to_string())
}

// Check if token needs refresh (less than 30min until expiry)
fn needs_token_refresh(token: &Option<GaeJwtToken>) -> bool {
    match token {
        None => true,
        Some(t) => {
            let now = std::time::SystemTime::now();
            let threshold = std::time::Duration::from_secs(30 * 60); // 30 minutes

            match t.expiry.duration_since(now) {
                Ok(remaining) => remaining < threshold,
                Err(_) => true, // Token already expired
            }
        }
    }
}

// Refresh the GAE JWT token if needed
pub fn ensure_gae_token(
    gae_jwt_secret: &Option<Vec<u8>>,
    gae_jwt_token: &mut Option<GaeJwtToken>,
) -> Result<()> {
    if let Some(secret) = gae_jwt_secret {
        if needs_token_refresh(gae_jwt_token) {
            let expiry_seconds = 3600; // 1 hour
            let token = create_gae_jwt_token(secret, expiry_seconds)?;
            let expiry =
                std::time::SystemTime::now() + std::time::Duration::from_secs(expiry_seconds);

            *gae_jwt_token = Some(GaeJwtToken { token, expiry });
        }
    }
    Ok(())
}

// Browser view types
#[derive(Clone, Debug)]
enum GaeView {
    Graphs,
    Jobs,
    LoadGraphInput,
}

#[derive(Clone, Debug)]
enum LoadGraphField {
    JsonInput,
    Submit,
}

struct LoadGraphState {
    textarea: TextArea<'static>,
    json_valid: bool,
    active_field: LoadGraphField,
}

#[derive(Clone)]
enum ConfirmationDialog {
    DeleteGraph(u64), // graph_id
    DeleteJob(u64),   // job_id
}

struct GaeBrowser {
    view: GaeView,
    graphs: Vec<GaeGraph>,
    selected_graph_index: usize,
    jobs: Vec<GaeJob>,
    selected_job_index: usize,
    accessible: bool,
    error_message: Option<String>,
    load_graph_state: Option<LoadGraphState>,
    confirmation_dialog: Option<ConfirmationDialog>,
    error_popup: Option<String>,
}

impl GaeBrowser {
    fn new() -> Self {
        Self {
            view: GaeView::Graphs,
            graphs: Vec::new(),
            selected_graph_index: 0,
            jobs: Vec::new(),
            selected_job_index: 0,
            accessible: true,
            error_message: None,
            load_graph_state: None,
            confirmation_dialog: None,
            error_popup: None,
        }
    }

    fn init_load_graph_state(&mut self) {
        let default_json = serde_json::json!({
            "database": "_system",
            "vertex_collections": ["V"],
            "vertex_attributes": [],
            "vertex_attribute_types": [],
            "edge_collections": ["E"],
            "parallelism": 10,
            "batch_size": 4000000
        });

        let json_str = serde_json::to_string_pretty(&default_json).unwrap_or_default();
        let textarea = TextArea::from(json_str.lines().map(|s| s.to_string()).collect::<Vec<_>>());

        self.load_graph_state = Some(LoadGraphState {
            textarea,
            json_valid: true,
            active_field: LoadGraphField::JsonInput,
        });
    }

    async fn load_graphs(
        &mut self,
        http_client: &Client,
        gae_endpoint: &Option<String>,
        gae_jwt_token: &Option<GaeJwtToken>,
    ) -> Result<()> {
        if let Some(endpoint) = gae_endpoint {
            let token = gae_jwt_token.as_ref().map(|t| t.token.as_str());
            match get_gae_graphs(http_client, endpoint, token).await {
                Ok(graphs) => {
                    self.graphs = graphs;
                    self.selected_graph_index = 0;
                    self.accessible = true;
                    self.error_message = None;
                    Ok(())
                }
                Err(e) => {
                    self.accessible = false;
                    self.error_message = Some(format!("Failed to fetch graphs: {}", e));
                    Err(e)
                }
            }
        } else {
            self.accessible = false;
            self.error_message = Some("GAE endpoint not configured".to_string());
            anyhow::bail!("GAE endpoint not configured")
        }
    }

    async fn load_jobs(
        &mut self,
        http_client: &Client,
        gae_endpoint: &Option<String>,
        gae_jwt_token: &Option<GaeJwtToken>,
    ) -> Result<()> {
        if let Some(endpoint) = gae_endpoint {
            let token = gae_jwt_token.as_ref().map(|t| t.token.as_str());
            match get_gae_jobs(http_client, endpoint, token).await {
                Ok(jobs) => {
                    self.jobs = jobs;
                    self.selected_job_index = 0;
                    self.accessible = true;
                    self.error_message = None;
                    Ok(())
                }
                Err(e) => {
                    self.accessible = false;
                    self.error_message = Some(format!("Failed to fetch jobs: {}", e));
                    Err(e)
                }
            }
        } else {
            self.accessible = false;
            self.error_message = Some("GAE endpoint not configured".to_string());
            anyhow::bail!("GAE endpoint not configured")
        }
    }
}

// Rendering functions
fn render_gae_graphs(f: &mut Frame, area: Rect, browser: &GaeBrowser) {
    if !browser.accessible {
        let error_msg = browser
            .error_message
            .as_deref()
            .unwrap_or("GAE not accessible");
        let no_access = Paragraph::new(error_msg)
            .style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL).title("GAE - Graphs"));
        f.render_widget(no_access, area);
        return;
    }

    if browser.graphs.is_empty() {
        let empty = Paragraph::new("No graphs loaded in GAE")
            .style(Style::default().fg(Color::Yellow))
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("GAE - Graphs | L: Load Graph | J: Jobs | R: Refresh | Q/ESC: Back"),
            );
        f.render_widget(empty, area);
        return;
    }

    let title = format!(
        "GAE - Graphs ({} graphs) | D: Delete | L: Load Graph | J: Jobs | R: Refresh | Q/ESC: Back",
        browser.graphs.len()
    );

    let header = Row::new(vec![
        "Graph ID",
        "Vertices",
        "Edges",
        "Memory (MB)",
        "Mem/Vertex (B)",
        "Mem/Edge (B)",
    ])
    .style(
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )
    .bottom_margin(1);

    let rows: Vec<Row> = browser
        .graphs
        .iter()
        .enumerate()
        .map(|(i, graph)| {
            let style = if i == browser.selected_graph_index {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            let memory_mb = graph.memory_usage as f64 / (1024.0 * 1024.0);

            Row::new(vec![
                Cell::from(graph.graph_id.to_string()),
                Cell::from(graph.number_of_vertices.to_string()),
                Cell::from(graph.number_of_edges.to_string()),
                Cell::from(format!("{:.2}", memory_mb)),
                Cell::from(graph.memory_per_vertex.to_string()),
                Cell::from(graph.memory_per_edge.to_string()),
            ])
            .style(style)
        })
        .collect();

    let widths = [
        Constraint::Percentage(15),
        Constraint::Percentage(15),
        Constraint::Percentage(15),
        Constraint::Percentage(20),
        Constraint::Percentage(17),
        Constraint::Percentage(18),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().borders(Borders::ALL).title(title))
        .column_spacing(2);

    f.render_widget(table, area);
}

fn render_gae_jobs(f: &mut Frame, area: Rect, browser: &GaeBrowser) {
    if !browser.accessible {
        let error_msg = browser
            .error_message
            .as_deref()
            .unwrap_or("GAE not accessible");
        let no_access = Paragraph::new(error_msg)
            .style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL).title("GAE - Jobs"));
        f.render_widget(no_access, area);
        return;
    }

    if browser.jobs.is_empty() {
        let empty = Paragraph::new("No jobs in GAE")
            .style(Style::default().fg(Color::Yellow))
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("GAE - Jobs | L: Load Graph | G: Graphs | R: Refresh | Q/ESC: Back"),
            );
        f.render_widget(empty, area);
        return;
    }

    let title = format!(
        "GAE - Jobs ({} jobs) | D: Delete | L: Load Graph | G: Graphs | R: Refresh | Q/ESC: Back",
        browser.jobs.len()
    );

    let header = Row::new(vec![
        "Job ID",
        "Graph ID",
        "Type",
        "Progress",
        "Status",
        "Memory (MB)",
        "Runtime (ms)",
    ])
    .style(
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )
    .bottom_margin(1);

    let rows: Vec<Row> = browser
        .jobs
        .iter()
        .enumerate()
        .map(|(i, job)| {
            let style = if i == browser.selected_job_index {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else if job.error {
                Style::default().fg(Color::Red)
            } else if job.progress == job.total && job.total > 0 {
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::White)
            };

            let progress_str = if job.total > 0 {
                format!("{}/{}", job.progress, job.total)
            } else {
                "N/A".to_string()
            };

            let status_str = if job.error {
                format!("Error: {}", job.error_message)
            } else if job.progress == job.total && job.total > 0 {
                "Completed".to_string()
            } else {
                "Running".to_string()
            };

            let memory_mb = job.memory_usage as f64 / (1024.0 * 1024.0);
            let runtime_ms = job.runtime_in_microseconds as f64 / 1000.0;

            Row::new(vec![
                Cell::from(job.job_id.to_string()),
                Cell::from(job.graph_id.to_string()),
                Cell::from(job.comp_type.clone()),
                Cell::from(progress_str),
                Cell::from(status_str),
                Cell::from(format!("{:.2}", memory_mb)),
                Cell::from(format!("{:.2}", runtime_ms)),
            ])
            .style(style)
        })
        .collect();

    let widths = [
        Constraint::Percentage(10),
        Constraint::Percentage(10),
        Constraint::Percentage(15),
        Constraint::Percentage(12),
        Constraint::Percentage(25),
        Constraint::Percentage(13),
        Constraint::Percentage(15),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().borders(Borders::ALL).title(title))
        .column_spacing(2);

    f.render_widget(table, area);
}

fn render_confirmation_dialog(f: &mut Frame, area: Rect, dialog: &ConfirmationDialog) {
    use ratatui::widgets::Clear;

    // Create a centered dialog box
    let dialog_width = 60;
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

    let message = match dialog {
        ConfirmationDialog::DeleteGraph(graph_id) => {
            format!("Delete graph {}?", graph_id)
        }
        ConfirmationDialog::DeleteJob(job_id) => {
            format!("Delete job {}?", job_id)
        }
    };

    let text = vec![
        Line::from(""),
        Line::from(vec![Span::styled(
            message,
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )])
        .alignment(Alignment::Center),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Press ENTER or Y to confirm, N or ESC to cancel",
            Style::default().fg(Color::White),
        )])
        .alignment(Alignment::Center),
    ];

    let dialog = Paragraph::new(text).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Confirm Deletion")
            .border_style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
    );

    f.render_widget(dialog, dialog_area);
}

fn render_error_popup(f: &mut Frame, area: Rect, error_message: &str) {
    use ratatui::widgets::Clear;

    // Create a centered dialog box
    let dialog_width = 60;
    let dialog_height = 9;
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

    let text = vec![
        Line::from(""),
        Line::from(vec![Span::styled(
            "Error",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )])
        .alignment(Alignment::Center),
        Line::from(""),
        Line::from(vec![Span::styled(
            error_message,
            Style::default().fg(Color::White),
        )])
        .alignment(Alignment::Center),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Press any key to continue",
            Style::default().fg(Color::Gray),
        )])
        .alignment(Alignment::Center),
    ];

    let dialog = Paragraph::new(text).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Error")
            .border_style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
    );

    f.render_widget(dialog, dialog_area);
}

fn render_gae_load_graph(f: &mut Frame, area: Rect, browser: &mut GaeBrowser) {
    if let Some(load_state) = &mut browser.load_graph_state {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(10),   // JSON input
                Constraint::Length(3), // Submit button
            ])
            .split(area);

        // JSON textarea
        let validation_msg = if load_state.json_valid {
            "✓ Valid JSON"
        } else {
            "✗ Invalid JSON"
        };

        load_state.textarea.set_block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(
                    "Load Graph Configuration (JSON) - {} | TAB: Switch fields | Q/ESC: Back",
                    validation_msg
                ))
                .border_style(
                    if matches!(load_state.active_field, LoadGraphField::JsonInput) {
                        if load_state.json_valid {
                            Style::default().fg(Color::Cyan)
                        } else {
                            Style::default().fg(Color::Red)
                        }
                    } else {
                        Style::default()
                    },
                ),
        );
        load_state.textarea.set_cursor_line_style(Style::default());
        load_state
            .textarea
            .set_cursor_style(Style::default().add_modifier(Modifier::REVERSED));
        f.render_widget(&load_state.textarea, chunks[0]);

        // Submit button
        let submit_text = if matches!(load_state.active_field, LoadGraphField::Submit) {
            ">>> [ SUBMIT - Press ENTER to load graph ] <<<"
        } else {
            "[ SUBMIT - Press TAB then ENTER ]"
        };

        let submit_widget = Paragraph::new(submit_text)
            .style(
                if matches!(load_state.active_field, LoadGraphField::Submit) {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Green)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Green)
                },
            )
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(submit_widget, chunks[1]);
    } else {
        let error = Paragraph::new("Load graph state not initialized")
            .style(Style::default().fg(Color::Red))
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("GAE - Load Graph"),
            );
        f.render_widget(error, area);
    }
}

// Main browser function
pub async fn run_gae_browser(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    http_client: &Client,
    gae_endpoint: &Option<String>,
    gae_jwt_secret: &Option<Vec<u8>>,
    gae_jwt_token: &mut Option<GaeJwtToken>,
) -> Result<()> {
    let mut browser = GaeBrowser::new();

    // Try to load initial data based on current view
    match browser.view {
        GaeView::Graphs => {
            let _ = ensure_gae_token(gae_jwt_secret, gae_jwt_token);
            let _ = browser
                .load_graphs(http_client, gae_endpoint, gae_jwt_token)
                .await;
        }
        GaeView::Jobs => {
            let _ = ensure_gae_token(gae_jwt_secret, gae_jwt_token);
            let _ = browser
                .load_jobs(http_client, gae_endpoint, gae_jwt_token)
                .await;
        }
        GaeView::LoadGraphInput => {}
    }

    loop {
        terminal.draw(|f| {
            match browser.view {
                GaeView::Graphs => render_gae_graphs(f, f.area(), &browser),
                GaeView::Jobs => render_gae_jobs(f, f.area(), &browser),
                GaeView::LoadGraphInput => render_gae_load_graph(f, f.area(), &mut browser),
            }

            // Render confirmation dialog on top if active
            if let Some(ref dialog) = browser.confirmation_dialog {
                render_confirmation_dialog(f, f.area(), dialog);
            }

            // Render error popup on top if active
            if let Some(ref error) = browser.error_popup {
                render_error_popup(f, f.area(), error);
            }
        })?;

        // Poll for events with a timeout of 1 second
        // This allows auto-refresh while still being responsive to user input
        if event::poll(std::time::Duration::from_millis(1000))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    // Handle error popup first if active
                    if browser.error_popup.is_some() {
                        // Any key dismisses the error popup
                        browser.error_popup = None;
                        continue;
                    }

                    // Handle confirmation dialog if active
                    if let Some(dialog) = browser.confirmation_dialog.clone() {
                        match key.code {
                            KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => {
                                // Confirm deletion
                                browser.confirmation_dialog = None;

                                match dialog {
                                    ConfirmationDialog::DeleteGraph(graph_id) => {
                                        if let Some(endpoint) = gae_endpoint.clone() {
                                            let _ = ensure_gae_token(gae_jwt_secret, gae_jwt_token);
                                            let token =
                                                gae_jwt_token.as_ref().map(|t| t.token.as_str());

                                            match delete_gae_graph(
                                                http_client,
                                                &endpoint,
                                                graph_id,
                                                token,
                                            )
                                            .await
                                            {
                                                Ok(_) => {
                                                    // Refresh the graph list
                                                    let _ = browser
                                                        .load_graphs(
                                                            http_client,
                                                            gae_endpoint,
                                                            gae_jwt_token,
                                                        )
                                                        .await;
                                                }
                                                Err(e) => {
                                                    browser.error_popup = Some(format!(
                                                        "Failed to delete graph: {}",
                                                        e
                                                    ));
                                                }
                                            }
                                        }
                                    }
                                    ConfirmationDialog::DeleteJob(job_id) => {
                                        if let Some(endpoint) = gae_endpoint.clone() {
                                            let _ = ensure_gae_token(gae_jwt_secret, gae_jwt_token);
                                            let token =
                                                gae_jwt_token.as_ref().map(|t| t.token.as_str());

                                            match delete_gae_job(
                                                http_client,
                                                &endpoint,
                                                job_id,
                                                token,
                                            )
                                            .await
                                            {
                                                Ok(_) => {
                                                    // Refresh the job list
                                                    let _ = browser
                                                        .load_jobs(
                                                            http_client,
                                                            gae_endpoint,
                                                            gae_jwt_token,
                                                        )
                                                        .await;
                                                }
                                                Err(e) => {
                                                    browser.error_popup = Some(format!(
                                                        "Failed to delete job: {}",
                                                        e
                                                    ));
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                                // Cancel deletion
                                browser.confirmation_dialog = None;
                            }
                            _ => {}
                        }
                        continue;
                    }

                    match browser.view {
                        GaeView::LoadGraphInput => {
                            // Handle load graph input view
                            if let Some(load_state) = &mut browser.load_graph_state {
                                match key.code {
                                    KeyCode::Char('q') | KeyCode::Esc => {
                                        // Go back to graphs view
                                        browser.view = GaeView::Graphs;
                                        browser.load_graph_state = None;
                                    }
                                    KeyCode::Tab => {
                                        // Switch between fields
                                        load_state.active_field = match load_state.active_field {
                                            LoadGraphField::JsonInput => LoadGraphField::Submit,
                                            LoadGraphField::Submit => LoadGraphField::JsonInput,
                                        };
                                    }
                                    KeyCode::Enter => {
                                        // Check if we're on the Submit button
                                        if matches!(load_state.active_field, LoadGraphField::Submit)
                                        {
                                            // Submit the load graph request
                                            if load_state.json_valid {
                                                let json_text =
                                                    load_state.textarea.lines().join("\n");

                                                // Parse the JSON
                                                if let Ok(config) =
                                                    serde_json::from_str::<serde_json::Value>(
                                                        &json_text,
                                                    )
                                                {
                                                    // Call the GAE API to load the graph
                                                    if let Some(endpoint) = gae_endpoint.clone() {
                                                        let _ = ensure_gae_token(
                                                            gae_jwt_secret,
                                                            gae_jwt_token,
                                                        );

                                                        let url = format!(
                                                            "{}/v1/loaddata",
                                                            endpoint.trim_end_matches('/')
                                                        );

                                                        let token = gae_jwt_token
                                                            .as_ref()
                                                            .map(|t| t.token.as_str());
                                                        let mut request =
                                                            http_client.post(&url).json(&config);

                                                        if let Some(jwt_token) = token {
                                                            request =
                                                                request.bearer_auth(jwt_token);
                                                        }

                                                        let response = request.send().await;

                                                        match response {
                                                            Ok(resp)
                                                                if resp.status().is_success() =>
                                                            {
                                                                // Successfully created the job, switch to jobs view
                                                                browser.view = GaeView::Jobs;
                                                                let _ = ensure_gae_token(
                                                                    gae_jwt_secret,
                                                                    gae_jwt_token,
                                                                );
                                                                let _ = browser
                                                                    .load_jobs(
                                                                        http_client,
                                                                        gae_endpoint,
                                                                        gae_jwt_token,
                                                                    )
                                                                    .await;
                                                                browser.load_graph_state = None;
                                                            }
                                                            Ok(resp) => {
                                                                // Error response - stay in load view
                                                                eprintln!(
                                                                    "Failed to load graph: {}",
                                                                    resp.status()
                                                                );
                                                            }
                                                            Err(e) => {
                                                                // Network error - stay in load view
                                                                eprintln!(
                                                                    "Failed to load graph: {}",
                                                                    e
                                                                );
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        } else {
                                            // Pass Enter to the textarea for newline
                                            load_state.textarea.input(key);

                                            // Validate JSON after input
                                            let text = load_state.textarea.lines().join("\n");
                                            load_state.json_valid =
                                                serde_json::from_str::<serde_json::Value>(&text)
                                                    .is_ok();
                                        }
                                    }
                                    _ => {
                                        // Pass other keys to the textarea only if we're in JsonInput field
                                        if matches!(
                                            load_state.active_field,
                                            LoadGraphField::JsonInput
                                        ) {
                                            load_state.textarea.input(key);

                                            // Validate JSON after input
                                            let text = load_state.textarea.lines().join("\n");
                                            load_state.json_valid =
                                                serde_json::from_str::<serde_json::Value>(&text)
                                                    .is_ok();
                                        }
                                    }
                                }
                            }
                        }
                        _ => {
                            // Handle other views
                            match key.code {
                                KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                                KeyCode::Char('d') | KeyCode::Char('D') => {
                                    // Delete current item
                                    match browser.view {
                                        GaeView::Graphs => {
                                            if !browser.graphs.is_empty()
                                                && browser.selected_graph_index
                                                    < browser.graphs.len()
                                            {
                                                let graph_id = browser.graphs
                                                    [browser.selected_graph_index]
                                                    .graph_id;
                                                browser.confirmation_dialog =
                                                    Some(ConfirmationDialog::DeleteGraph(graph_id));
                                            }
                                        }
                                        GaeView::Jobs => {
                                            if !browser.jobs.is_empty()
                                                && browser.selected_job_index < browser.jobs.len()
                                            {
                                                let job_id =
                                                    browser.jobs[browser.selected_job_index].job_id;
                                                browser.confirmation_dialog =
                                                    Some(ConfirmationDialog::DeleteJob(job_id));
                                            }
                                        }
                                        GaeView::LoadGraphInput => {}
                                    }
                                }
                                KeyCode::Char('g') | KeyCode::Char('G') => {
                                    if !matches!(browser.view, GaeView::Graphs) {
                                        browser.view = GaeView::Graphs;
                                        let _ = ensure_gae_token(gae_jwt_secret, gae_jwt_token);
                                        let _ = browser
                                            .load_graphs(http_client, gae_endpoint, gae_jwt_token)
                                            .await;
                                    }
                                }
                                KeyCode::Char('j') | KeyCode::Char('J') => {
                                    if !matches!(browser.view, GaeView::Jobs) {
                                        browser.view = GaeView::Jobs;
                                        let _ = ensure_gae_token(gae_jwt_secret, gae_jwt_token);
                                        let _ = browser
                                            .load_jobs(http_client, gae_endpoint, gae_jwt_token)
                                            .await;
                                    }
                                }
                                KeyCode::Char('l') | KeyCode::Char('L') => {
                                    // Open load graph view (from graphs or jobs view)
                                    if matches!(browser.view, GaeView::Graphs | GaeView::Jobs) {
                                        browser.init_load_graph_state();
                                        browser.view = GaeView::LoadGraphInput;
                                    }
                                }
                                KeyCode::Char('r') | KeyCode::Char('R') => {
                                    // Refresh current view
                                    match browser.view {
                                        GaeView::Graphs => {
                                            let _ = ensure_gae_token(gae_jwt_secret, gae_jwt_token);
                                            let _ = browser
                                                .load_graphs(
                                                    http_client,
                                                    gae_endpoint,
                                                    gae_jwt_token,
                                                )
                                                .await;
                                        }
                                        GaeView::Jobs => {
                                            let _ = ensure_gae_token(gae_jwt_secret, gae_jwt_token);
                                            let _ = browser
                                                .load_jobs(http_client, gae_endpoint, gae_jwt_token)
                                                .await;
                                        }
                                        GaeView::LoadGraphInput => {}
                                    }
                                }
                                KeyCode::Down => match browser.view {
                                    GaeView::Graphs => {
                                        if !browser.graphs.is_empty() {
                                            browser.selected_graph_index =
                                                (browser.selected_graph_index + 1)
                                                    % browser.graphs.len();
                                        }
                                    }
                                    GaeView::Jobs => {
                                        if !browser.jobs.is_empty() {
                                            browser.selected_job_index =
                                                (browser.selected_job_index + 1)
                                                    % browser.jobs.len();
                                        }
                                    }
                                    GaeView::LoadGraphInput => {}
                                },
                                KeyCode::Up => match browser.view {
                                    GaeView::Graphs => {
                                        if !browser.graphs.is_empty() {
                                            browser.selected_graph_index =
                                                if browser.selected_graph_index == 0 {
                                                    browser.graphs.len() - 1
                                                } else {
                                                    browser.selected_graph_index - 1
                                                };
                                        }
                                    }
                                    GaeView::Jobs => {
                                        if !browser.jobs.is_empty() {
                                            browser.selected_job_index =
                                                if browser.selected_job_index == 0 {
                                                    browser.jobs.len() - 1
                                                } else {
                                                    browser.selected_job_index - 1
                                                };
                                        }
                                    }
                                    GaeView::LoadGraphInput => {}
                                },
                                _ => {}
                            }
                        }
                    }
                }
            }
        } else {
            // Timeout occurred - auto-refresh based on current view
            match browser.view {
                GaeView::Graphs => {
                    let _ = ensure_gae_token(gae_jwt_secret, gae_jwt_token);
                    let _ = browser
                        .load_graphs(http_client, gae_endpoint, gae_jwt_token)
                        .await;
                }
                GaeView::Jobs => {
                    let _ = ensure_gae_token(gae_jwt_secret, gae_jwt_token);
                    let _ = browser
                        .load_jobs(http_client, gae_endpoint, gae_jwt_token)
                        .await;
                }
                GaeView::LoadGraphInput => {
                    // Don't auto-refresh while user is editing
                }
            }
        }
    }
}
