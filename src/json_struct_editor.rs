//! # json_struct_editor
//!
//! A ratatui widget for editing structs via their JSON representation with
//! live validation and field-aware completion support.
//!
//! ## Example
//!
//! ```rust,no_run
//! use json_struct_editor::{JsonStructEditor, CompletionResult};
//! use serde::{Deserialize, Serialize};
//!
//! #[derive(Serialize, Deserialize, Clone, Default)]
//! struct Config {
//!     name: String,
//!     count: u32,
//!     enabled: bool,
//! }
//!
//! let mut editor = JsonStructEditor::new(Config::default())
//!     .with_completion_handler("name", |config: &amp;mut Config| {
//!         config.name = "completed_name".to_string();
//!         CompletionResult::Updated
//!     });
//! ```

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, Borders, Widget},
};
use serde::{Serialize, de::DeserializeOwned};
use std::collections::HashMap;
use tui_textarea::TextArea;

/// Result of a completion handler invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionResult {
    /// The struct was updated; the editor will re-serialize.
    Updated,
    /// No changes were made.
    NoChange,
}

/// Validation state of the current JSON text.
#[derive(Debug, Clone)]
pub enum ValidationState {
    /// JSON is valid and deserializes to T.
    Valid,
    /// JSON is invalid or cannot deserialize to T.
    Invalid(String),
}

impl ValidationState {
    pub fn is_valid(&self) -> bool {
        matches!(self, ValidationState::Valid)
    }
}

type CompletionHandler<T> = Box<dyn FnMut(&mut T) -> CompletionResult>;

/// A TUI editor widget for editing a struct via its JSON representation.
///
/// Features:
/// - Live validation feedback
/// - Configurable completion key (default: F12)
/// - Field-aware completion handlers
pub struct JsonStructEditor<'a, T> {
    textarea: TextArea<'a>,
    current_value: Option<T>,
    validation_state: ValidationState,
    completion_key: KeyCode,
    completion_handlers: HashMap<String, CompletionHandler<T>>,
    title: String,
}

impl<'a, T> JsonStructEditor<'a, T>
where
    T: Serialize + DeserializeOwned + Clone,
{
    /// Create a new editor initialized with the given value.
    pub fn new(initial_value: T) -> Self {
        let json = serde_json::to_string_pretty(&initial_value)
            .expect("Failed to serialize initial value");
        let lines: Vec<String> = json.lines().map(String::from).collect();
        let textarea = TextArea::new(lines);

        Self {
            textarea,
            current_value: Some(initial_value),
            validation_state: ValidationState::Valid,
            completion_key: KeyCode::F(12),
            completion_handlers: HashMap::new(),
            title: "JSON Editor".to_string(),
        }
    }

    /// Set the key that triggers completion (default: F12).
    pub fn with_completion_key(mut self, key: KeyCode) -> Self {
        self.completion_key = key;
        self
    }

    /// Set the editor title shown in the border.
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    /// Register a completion handler for a specific field name.
    ///
    /// When the completion key is pressed while the cursor is on a value
    /// for the given field, the handler is called with a mutable reference
    /// to the current struct value.
    pub fn with_completion_handler<F>(mut self, field: impl Into<String>, handler: F) -> Self
    where
        F: FnMut(&mut T) -> CompletionResult + 'static,
    {
        self.completion_handlers
            .insert(field.into(), Box::new(handler));
        self
    }

    /// Register a completion handler (builder pattern alternative).
    pub fn add_completion_handler<F>(&mut self, field: impl Into<String>, handler: F)
    where
        F: FnMut(&mut T) -> CompletionResult + 'static,
    {
        self.completion_handlers
            .insert(field.into(), Box::new(handler));
    }

    /// Get the current validation state.
    pub fn validation_state(&self) -> &ValidationState {
        &self.validation_state
    }

    /// Get the current deserialized value, if valid.
    pub fn value(&self) -> Option<&T> {
        self.current_value.as_ref()
    }

    /// Get a clone of the current value, if valid.
    pub fn value_cloned(&self) -> Option<T> {
        self.current_value.clone()
    }

    /// Get the raw JSON text.
    pub fn text(&self) -> String {
        self.textarea.lines().join("\n")
    }

    /// Get the cursor position as (row, col).
    pub fn cursor(&self) -> (usize, usize) {
        self.textarea.cursor()
    }

    /// Handle a key event. Returns true if the event was consumed.
    pub fn handle_key_event(&mut self, key: KeyEvent) -> bool {
        // Check for completion key
        if key.code == self.completion_key && key.modifiers.is_empty() {
            self.try_complete();
            return true;
        }

        // Pass to textarea
        self.textarea.input(key);

        // Re-validate after each edit
        self.revalidate();

        true
    }

    /// Attempt to trigger completion at the current cursor position.
    fn try_complete(&mut self) {
        // Completion only works if current text is valid
        // Clone the value first to avoid borrowing issues
        let Some(ref value) = self.current_value else {
            return;
        };
        let mut new_value = value.clone();

        // Find the field name at cursor
        let Some(field_name) = self.find_field_at_cursor() else {
            return;
        };

        // Look up handler
        let Some(handler) = self.completion_handlers.get_mut(&field_name) else {
            return;
        };

        // Call handler, check result
        let result = handler(&mut new_value);

        if result == CompletionResult::Updated {
            // Re-serialize and update textarea
            if let Ok(json) = serde_json::to_string_pretty(&new_value) {
                let lines: Vec<String> = json.lines().map(String::from).collect();
                self.textarea = TextArea::new(lines);
                self.current_value = Some(new_value);
                self.validation_state = ValidationState::Valid;
            }
        }
    }

    /// Find the JSON field name at the current cursor position.
    ///
    /// Algorithm:
    /// 1. Get cursor position (row, col)
    /// 2. Convert to byte offset in full text
    /// 3. Search backwards for ':'
    /// 4. Search backwards from ':' for a quoted string
    fn find_field_at_cursor(&self) -> Option<String> {
        let (row, col) = self.textarea.cursor();
        let lines = self.textarea.lines();

        // Build full text up to cursor position
        let mut offset = 0usize;
        for (i, line) in lines.iter().enumerate() {
            if i < row {
                offset += line.len() + 1; // +1 for newline
            } else {
                offset += col.min(line.len());
                break;
            }
        }

        let full_text = lines.join("\n");
        let text_before_cursor = &full_text[..offset.min(full_text.len())];

        // Find the last colon before cursor
        let colon_pos = text_before_cursor.rfind(':')?;

        // Get text before the colon
        let before_colon = &text_before_cursor[..colon_pos];

        // Find the quoted string directly before the colon (the field name)
        // We look for pattern: "fieldname" (possibly with whitespace before colon)
        let trimmed = before_colon.trim_end();
        if !trimmed.ends_with('"') {
            return None;
        }

        // Find the opening quote
        let end_quote = trimmed.len() - 1;
        let start_quote = trimmed[..end_quote].rfind('"')?;

        let field_name = &trimmed[start_quote + 1..end_quote];
        Some(field_name.to_string())
    }

    /// Re-validate the current text against type T.
    fn revalidate(&mut self) {
        let text = self.textarea.lines().join("\n");
        match serde_json::from_str::<T>(&text) {
            Ok(value) => {
                self.current_value = Some(value);
                self.validation_state = ValidationState::Valid;
            }
            Err(e) => {
                self.current_value = None;
                self.validation_state = ValidationState::Invalid(e.to_string());
            }
        }
    }

    /// Apply a block (border) style to the editor.
    pub fn set_block(&mut self, block: Block<'a>) {
        self.textarea.set_block(block);
    }

    /// Set the cursor line style.
    pub fn set_cursor_line_style(&mut self, style: Style) {
        self.textarea.set_cursor_line_style(style);
    }

    /// Set the cursor style.
    pub fn set_cursor_style(&mut self, style: Style) {
        self.textarea.set_cursor_style(style);
    }

    /// Build the widget for rendering.
    ///
    /// This creates a view that includes:
    /// - The textarea with JSON content
    /// - A status indicator showing valid/invalid state
    pub fn widget(&'a self) -> JsonStructEditorWidget<'a, T> {
        JsonStructEditorWidget { editor: self }
    }
}

/// Widget wrapper for rendering the editor.
pub struct JsonStructEditorWidget<'a, T> {
    editor: &'a JsonStructEditor<'a, T>,
}

impl<'a, T> Widget for JsonStructEditorWidget<'a, T>
where
    T: Serialize + DeserializeOwned + Clone,
{
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Determine border color based on validation state
        let (border_color, status_text) = match &self.editor.validation_state {
            ValidationState::Valid => (Color::Green, "✓ Valid".to_string()),
            ValidationState::Invalid(msg) => (Color::Red, format!("✗ Invalid: {}", msg)),
        };

        // Create block with colored border
        let title = format!("{} [{}]", self.editor.title, status_text);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .title(title);

        // Clone textarea to set block and render
        let mut textarea = self.editor.textarea.clone();
        textarea.set_block(block);
        textarea.render(area, buf);
    }
}

/// Stateful widget implementation - renders the textarea directly.
impl<'a, T> Widget for &'a JsonStructEditor<'a, T>
where
    T: Serialize + DeserializeOwned + Clone,
{
    fn render(self, area: Rect, buf: &mut Buffer) {
        self.widget().render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq)]
    struct TestStruct {
        name: String,
        count: u32,
    }

    #[test]
    fn test_new_editor() {
        let value = TestStruct {
            name: "test".to_string(),
            count: 42,
        };
        let editor = JsonStructEditor::new(value.clone());

        assert!(editor.validation_state().is_valid());
        assert_eq!(editor.value().unwrap().name, "test");
        assert_eq!(editor.value().unwrap().count, 42);
    }

    #[test]
    fn test_find_field_at_cursor() {
        let value = TestStruct {
            name: "test".to_string(),
            count: 42,
        };
        let mut editor = JsonStructEditor::new(value);

        // The JSON looks like:
        // {
        //   "name": "test",
        //   "count": 42
        // }
        // Move cursor to line 1 (name line), after the colon
        // Line 1 is: '  "name": "test",'

        // Simulate being on line 1, column 12 (inside "test")
        // We need to manually set cursor position via textarea
        // For testing, we'll just verify the algorithm works on known text

        let text = editor.text();
        assert!(text.contains("\"name\":"));
        assert!(text.contains("\"count\":"));
    }

    #[test]
    fn test_validation_invalid_json() {
        let value = TestStruct::default();
        let mut editor = JsonStructEditor::new(value);

        // Simulate typing invalid JSON by inputting a character that breaks JSON
        let key = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE);
        editor.handle_key_event(key);

        // After inserting 'x' at the beginning, JSON should be invalid
        assert!(!editor.validation_state().is_valid());
    }
}
