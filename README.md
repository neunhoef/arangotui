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
git clone https://github.com/yourusername/arangotui.git
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
- [ ] GAE integration
- [ ] Document browser and viewer
- [ ] AQL query interface
- [ ] Index management
- [ ] User and permission management
- [ ] Graph visualization
- [ ] Configuration file support
- [ ] Export functionality
- [ ] Search and filtering

## Contributing

Contributions are welcome! Please feel free to submit issues or pull requests.

## License

[License information to be added]

## Acknowledgments

- Built with [Ratatui](https://ratatui.rs/)
- Designed for [ArangoDB](https://www.arangodb.com/)
