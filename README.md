# ArangoTUI

A Terminal User Interface (TUI) for [ArangoDB](https://www.arangodb.com/) and the Graph Analytics Engine (GAE).

## Overview

ArangoTUI is an interactive command-line tool that provides a modern, keyboard-driven interface for managing and exploring ArangoDB databases. Built with Rust and [Ratatui](https://ratatui.rs/), it offers a fast, efficient way to browse databases, collections, and view metadata without leaving your terminal.

### Key Features

- **Database Browser**: Navigate through databases and collections with an intuitive interface
- **Collection Explorer**: View collection properties, document counts, and metadata
- **Real-time Statistics**: See collection counts and database statistics at a glance
- **GAE Support**: Connect to Graph Analytics Engine endpoints (planned)
- **Vim-style Navigation**: Use familiar keyboard shortcuts (j/k for up/down, Enter to select, Esc/q to go back)
- **Secure Connections**: Support for HTTPS endpoints with custom certificate handling

## Installation

### Prerequisites

- Rust 1.70+ (with Cargo)
- Access to an ArangoDB instance

### Building from Source

```bash
git clone https://github.com/neunhoef/arangotui.git
cd arangotui
cargo build --release
```

The compiled binary will be available at `target/release/arangotui`.

### Installing

```bash
cargo install --path .
```

## Usage

### Basic Usage

Connect to a local ArangoDB instance with default settings:

```bash
arangotui
```

### Command-line Options

```bash
arangotui [OPTIONS]

Options:
  --endpoint <URL>     ArangoDB endpoint URL [default: http://localhost:8529]
  --gae <URL>          Graph Analytics Engine endpoint URL (optional)
  --username <USER>    Username for authentication [default: root]
  --password <PASS>    Password for authentication [default: ""]
  -h, --help           Print help
```

### Examples

Connect to a remote ArangoDB instance:

```bash
arangotui --endpoint https://db.example.com:8529 --username admin --password secret
```

Connect to both ArangoDB and GAE:

```bash
arangotui --endpoint http://localhost:8529 --gae http://localhost:9000
```

## Navigation

### Main Menu

- **Arrow Keys** or **j/k**: Navigate menu items
- **Enter**: Select menu item
- **q** or **Esc**: Quit application

### Database Browser

- **Arrow Keys** or **j/k**: Navigate through databases
- **Enter**: Open selected database to view collections
- **q** or **Esc**: Return to main menu

### Collection List

- **Arrow Keys** or **j/k**: Navigate through collections
- **Enter**: View collection properties
- **q** or **Esc**: Return to database list

### Collection Properties

- **Arrow Keys** or **j/k**: Scroll through properties
- **PageUp/PageDown**: Scroll faster
- **q** or **Esc**: Return to collection list

### Collection Content View

- **Arrow Keys** or **j/k**: Navigate through documents
- **PageUp/PageDown**: Scroll faster through document list
- **Enter**: View full document details
- **q** or **Esc**: Return to collection list

### AQL Query Execution

- **Type**: Enter your AQL query
- **Ctrl+Enter** or **F5**: Execute query
- **Arrow Keys** or **j/k**: Navigate through results
- **Tab**: Switch between query input and results view
- **q** or **Esc**: Return to main menu

### Graphs Overview

- **Arrow Keys** or **j/k**: Navigate through available graphs
- **Enter**: View graph details and properties
- **q** or **Esc**: Return to main menu

## Features

### Database Browser

Browse all accessible databases in your ArangoDB instance. The interface shows:

- Database name
- Number of document collections
- Number of edge collections
- Number of system collections
- Access status

### Collection Explorer

View detailed information about collections, including:

- Collection name and type (Document/Edge)
- Document count
- System collection indicator
- Detailed JSON properties including:
  - Write concern settings
  - Sync options
  - Schema definitions
  - Key options
  - And more...

### Collection Content View

Browse and view documents within a collection:

- Display documents in a paginated list
- View full document content in formatted JSON
- Navigate through large collections efficiently
- Quick access to document keys and metadata
- Real-time document count information

### AQL Query Interface

Execute AQL (ArangoDB Query Language) queries directly from the TUI:

- Interactive query editor with syntax input
- Execute queries against the connected database
- View query results in formatted JSON
- Navigate through result sets
- Error reporting for invalid queries
- Support for read and write queries
- Query history (planned)

### Graphs Overview

Explore graph structures defined in your ArangoDB instance:

- List all named graphs in the database
- View graph definitions and properties
- Display edge collections and vertex collections
- Inspect graph configuration including:
  - Edge definitions
  - Orphan collections
  - Smart graph settings
  - Satellite collections
- Navigate graph metadata

### Graph Analytics Engine (GAE)

*Note: GAE support is planned but not yet implemented.*

The GAE integration will provide:

- Graph algorithm execution
- Analytics query interface
- Results visualization
- Performance monitoring

## Architecture

ArangoTUI is built with:

- **[Ratatui](https://ratatui.rs/)**: Modern Rust TUI framework for building rich terminal interfaces
- **[Crossterm](https://github.com/crossterm-rs/crossterm)**: Cross-platform terminal manipulation
- **[Tokio](https://tokio.rs/)**: Async runtime for handling HTTP requests
- **[Reqwest](https://github.com/seanmonstar/reqwest)**: HTTP client for ArangoDB REST API
- **[Clap](https://github.com/clap-rs/clap)**: Command-line argument parsing

## Development

### Project Structure

```
arangotui/
├── src/
│   └── main.rs          # Main application logic
├── Cargo.toml           # Project dependencies
└── README.md            # This file
```

### Building for Development

```bash
cargo build
cargo run -- --endpoint http://localhost:8529
```

### Running Tests

```bash
cargo test
```

## Roadmap

- [x] Database browsing interface
- [x] Collection listing and statistics
- [x] Collection properties viewer
- [x] Collection content viewer
- [x] AQL query interface
- [x] Graphs overview
- [ ] GAE integration
- [ ] Advanced document editing
- [ ] Index management
- [ ] User and permission management
- [ ] Graph visualization
- [ ] Query history and saved queries
- [ ] Configuration file support
- [ ] Export functionality
- [ ] Search and filtering within collections

## Contributing

Contributions are welcome! Please feel free to submit issues or pull requests.

## License

This project is licensed under the Apache License 2.0 - see the [LICENSE](LICENSE) file for details.

Copyright 2025 ArangoTUI Contributors

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.

## Acknowledgments

- Built with [Ratatui](https://ratatui.rs/)
- Designed for [ArangoDB](https://www.arangodb.com/)
