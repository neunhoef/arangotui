use anyhow::{Context, Result};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Paragraph},
};
use reqwest::Client;
use serde::Deserialize;
use tui_textarea::TextArea;

#[derive(Debug, Deserialize)]
pub struct AqlQueryResponse {
    pub error: bool,
    pub code: u16,
    pub result: Vec<serde_json::Value>,
    #[serde(rename = "hasMore")]
    pub has_more: bool,
    pub cached: bool,
    pub extra: Option<serde_json::Value>,
    pub id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AqlCursorNextResponse {
    pub error: bool,
    pub code: u16,
    pub result: Vec<serde_json::Value>,
    #[serde(rename = "hasMore")]
    pub has_more: bool,
    pub id: Option<String>,
}

#[derive(Clone, Debug)]
pub enum AqlInputField {
    Query,
    Parameters,
    Options,
    Submit,
}

#[derive(Clone, Debug)]
pub struct AqlQueryOptions {
    pub batch_size: usize,
    pub stream: bool,
    pub max_documents: usize,
}

impl Default for AqlQueryOptions {
    fn default() -> Self {
        Self {
            batch_size: 1000,
            stream: true,
            max_documents: 100000,
        }
    }
}

pub struct AqlState {
    pub query_textarea: TextArea<'static>,
    pub parameters_textarea: TextArea<'static>,
    pub options_textarea: TextArea<'static>,
    pub active_field: AqlInputField,
    pub parameters_valid: bool,
    pub options_valid: bool,
    // Results state
    pub results: Vec<serde_json::Value>,
    pub total_fetched: usize,
    pub has_more: bool,
    pub cursor_id: Option<String>,
    pub current_page: usize,
    pub scroll_offset: usize,
    pub is_fetching: bool,
}

impl AqlState {
    pub fn new() -> Self {
        let default_options = AqlQueryOptions::default();
        let options_json = serde_json::json!({
            "batchSize": default_options.batch_size,
            "stream": default_options.stream,
            "maxDocuments": default_options.max_documents,
        });

        let query_textarea = TextArea::default();
        let parameters_textarea = TextArea::from(["{}".to_string()]);
        let options_textarea = TextArea::from(
            serde_json::to_string_pretty(&options_json)
                .unwrap_or_default()
                .lines()
                .map(|s| s.to_string())
                .collect::<Vec<_>>(),
        );

        Self {
            query_textarea,
            parameters_textarea,
            options_textarea,
            active_field: AqlInputField::Query,
            parameters_valid: true,
            options_valid: true,
            results: Vec::new(),
            total_fetched: 0,
            has_more: false,
            cursor_id: None,
            current_page: 0,
            scroll_offset: 0,
            is_fetching: false,
        }
    }
}

pub async fn execute_aql_query(
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
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();
        anyhow::bail!(
            "Failed to execute AQL query: {} - URL: {} - Error: {}",
            status,
            url,
            error_text
        );
    }

    let response_text = response
        .text()
        .await
        .context("Failed to read AQL query response")?;

    let query_response = serde_json::from_str(&response_text);
    if let Err(err) = query_response {
        anyhow::bail!("Failed to parse AQL query response: {}", err);
    }
    let query_response: AqlQueryResponse = query_response.unwrap();

    Ok(query_response.result)
}

pub async fn execute_aql_query_with_params(
    client: &Client,
    endpoint: &str,
    database: &str,
    query: &str,
    bind_vars: Option<serde_json::Value>,
    batch_size: usize,
    stream: bool,
    username: &str,
    password: &str,
) -> Result<AqlQueryResponse> {
    let url = format!(
        "{}/_db/{}/_api/cursor",
        endpoint.trim_end_matches('/'),
        database
    );

    let mut body = serde_json::json!({
        "query": query,
        "count": false,
        "batchSize": batch_size,
        "options": {
            "stream": stream
        }
    });

    if let Some(vars) = bind_vars {
        body["bindVars"] = vars;
    }

    let response = client
        .post(&url)
        .basic_auth(username, Some(password))
        .json(&body)
        .send()
        .await
        .context("Failed to execute AQL query")?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();
        anyhow::bail!("Failed to execute AQL query: {} - {}", status, error_text);
    }

    let response_text = response
        .text()
        .await
        .context("Failed to read AQL query response")?;

    let query_response = serde_json::from_str(&response_text);
    if let Err(err) = query_response {
        anyhow::bail!("Failed to parse AQL query response: {}", err);
    }
    Ok(query_response.unwrap())
}

pub async fn fetch_cursor_next(
    client: &Client,
    endpoint: &str,
    database: &str,
    cursor_id: &str,
    username: &str,
    password: &str,
) -> Result<AqlCursorNextResponse> {
    let url = format!(
        "{}/_db/{}/_api/cursor/{}",
        endpoint.trim_end_matches('/'),
        database,
        cursor_id
    );

    let response = client
        .put(&url)
        .basic_auth(username, Some(password))
        .send()
        .await
        .context("Failed to fetch cursor")?;

    if !response.status().is_success() {
        anyhow::bail!("Failed to fetch cursor: {}", response.status());
    }

    let response_text = response
        .text()
        .await
        .context("Failed to read cursor response")?;

    let cursor_response = serde_json::from_str(&response_text);
    if let Err(err) = cursor_response {
        anyhow::bail!("Failed to parse cursor response: {}", err);
    }
    Ok(cursor_response.unwrap())
}

pub fn render_aql_query_input(f: &mut Frame, area: Rect, aql_state: &mut AqlState, database: &str) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(35), // Query input
            Constraint::Percentage(25), // Parameters input
            Constraint::Percentage(25), // Options input
            Constraint::Length(3),      // Submit button
        ])
        .split(area);

    // Query textarea
    aql_state.query_textarea.set_block(
        Block::default()
            .borders(Borders::ALL)
            .title("AQL Query (TAB to switch fields)")
            .border_style(if matches!(aql_state.active_field, AqlInputField::Query) {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default()
            }),
    );
    aql_state
        .query_textarea
        .set_cursor_line_style(Style::default());
    aql_state
        .query_textarea
        .set_cursor_style(Style::default().add_modifier(Modifier::REVERSED));
    f.render_widget(&aql_state.query_textarea, chunks[0]);

    // Parameters textarea
    let validation_msg = if aql_state.parameters_valid {
        "✓ Valid JSON"
    } else {
        "✗ Invalid JSON"
    };

    aql_state.parameters_textarea.set_block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!("Query Parameters (JSON) - {}", validation_msg))
            .border_style(
                if matches!(aql_state.active_field, AqlInputField::Parameters) {
                    Style::default().fg(Color::Cyan)
                } else if aql_state.parameters_valid {
                    Style::default()
                } else {
                    Style::default().fg(Color::Red)
                },
            ),
    );
    aql_state
        .parameters_textarea
        .set_cursor_line_style(Style::default());
    aql_state
        .parameters_textarea
        .set_cursor_style(Style::default().add_modifier(Modifier::REVERSED));
    f.render_widget(&aql_state.parameters_textarea, chunks[1]);

    // Options textarea
    let options_validation_msg = if aql_state.options_valid {
        "✓ Valid JSON"
    } else {
        "✗ Invalid JSON"
    };

    aql_state.options_textarea.set_block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!("Query Options (JSON) - {}", options_validation_msg))
            .border_style(
                if matches!(aql_state.active_field, AqlInputField::Options) {
                    Style::default().fg(Color::Cyan)
                } else if aql_state.options_valid {
                    Style::default()
                } else {
                    Style::default().fg(Color::Red)
                },
            ),
    );
    aql_state
        .options_textarea
        .set_cursor_line_style(Style::default());
    aql_state
        .options_textarea
        .set_cursor_style(Style::default().add_modifier(Modifier::REVERSED));
    f.render_widget(&aql_state.options_textarea, chunks[2]);

    // Submit button
    let submit_text = if matches!(aql_state.active_field, AqlInputField::Submit) {
        ">>> [ SUBMIT QUERY - Press ENTER ] <<<"
    } else {
        "[ SUBMIT QUERY - Press TAB then ENTER ]"
    };

    let submit_widget = Paragraph::new(submit_text)
        .style(if matches!(aql_state.active_field, AqlInputField::Submit) {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Green)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Green)
        })
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(submit_widget, chunks[3]);
}

pub fn render_aql_query_results(f: &mut Frame, area: Rect, aql_state: &AqlState, database: &str) {
    if aql_state.is_fetching {
        // Show progress bar
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Progress info
                Constraint::Min(0),    // Progress bar area
            ])
            .split(area);

        let progress_text = format!("Fetching documents: {} fetched", aql_state.total_fetched);
        let progress_para = Paragraph::new(progress_text)
            .style(Style::default().fg(Color::Yellow))
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!("AQL Query Results - {}", database)),
            );
        f.render_widget(progress_para, chunks[0]);

        // Simple progress indicator
        let progress_indicator = Paragraph::new("Loading...")
            .style(Style::default().fg(Color::Cyan))
            .alignment(Alignment::Center);
        f.render_widget(progress_indicator, chunks[1]);
    } else if aql_state.results.is_empty() {
        let empty = Paragraph::new("No results")
            .style(Style::default().fg(Color::Yellow))
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!("AQL Query Results - {}", database)),
            );
        f.render_widget(empty, area);
    } else {
        // Display results with pagination
        let page_size = 100; // ~100 lines per page, but complete documents

        // Calculate which documents to show
        let mut lines_in_page = Vec::new();
        let mut current_line_count = 0;
        let start_doc_idx = aql_state.current_page * page_size;
        let mut docs_in_page = 0;

        for (idx, doc) in aql_state.results.iter().enumerate().skip(start_doc_idx) {
            if current_line_count >= page_size && docs_in_page > 0 {
                break;
            }

            if idx > start_doc_idx {
                lines_in_page.push(Line::from(""));
                current_line_count += 1;
            }

            let json_str =
                serde_json::to_string_pretty(doc).unwrap_or_else(|_| "Error".to_string());
            for line in json_str.lines() {
                lines_in_page.push(Line::from(line.to_string()));
                current_line_count += 1;
            }
            docs_in_page += 1;
        }

        let total_pages = (aql_state.results.len() + page_size - 1) / page_size;

        let title = format!(
            "AQL Query Results - {} | Page {}/{} | {} docs | ← → : pages | ↑ ↓ PgUp PgDn: scroll | Q/ESC: back",
            database,
            aql_state.current_page + 1,
            total_pages.max(1),
            aql_state.results.len()
        );

        let para = Paragraph::new(lines_in_page)
            .style(Style::default().fg(Color::White))
            .block(Block::default().borders(Borders::ALL).title(title))
            .scroll((aql_state.scroll_offset as u16, 0));

        f.render_widget(para, area);
    }
}
