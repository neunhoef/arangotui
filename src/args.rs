use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "arangotui")]
#[command(about = "A TUI for ArangoDB and Graph Analytics Engine", long_about = None)]
pub struct Args {
    /// ArangoDB endpoint URL
    #[arg(long, default_value = "http://localhost:8529")]
    pub endpoint: String,

    /// Graph Analytics Engine endpoint URL
    #[arg(long, default_value = "http://localhost:9999")]
    pub gae: Option<String>,

    /// Username for authentication
    #[arg(long, default_value = "root")]
    pub username: String,

    /// Password for authentication
    #[arg(long, default_value = "")]
    pub password: String,

    /// GAE JWT secret file for authentication
    #[arg(long)]
    pub gae_jwt_secret_file: Option<String>,
}
