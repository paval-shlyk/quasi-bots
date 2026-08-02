use clap::Parser;
use tracing_subscriber::EnvFilter;

use mcp_client::{ConnectOptions, McpSession, login_oauth, tui};

#[derive(Parser, Debug)]
#[command(
    name = "mcp-client",
    version,
    about = "MCP 2025-11-25 Streamable HTTP client with ratatui TUI"
)]
struct Cli {
    /// MCP endpoint URL (Streamable HTTP)
    #[arg(
        short,
        long,
        env = "MCP_URL",
        default_value = "http://127.0.0.1:8080/mcp"
    )]
    url: String,

    /// Bearer access token (without "Bearer " prefix)
    #[arg(short, long, env = "MCP_TOKEN")]
    token: Option<String>,

    /// Run OAuth PKCE login before connecting
    #[arg(long)]
    login: bool,

    /// OAuth redirect URI for the local callback server
    #[arg(long, default_value = "http://127.0.0.1:9876/callback")]
    redirect: String,

    /// OAuth scope
    #[arg(long, default_value = "mcp")]
    scope: String,

    /// List tools to stdout and exit (no TUI)
    #[arg(long)]
    headless_list: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // When running the TUI, keep tracing off stderr noise unless RUST_LOG is set.
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("warn"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();

    let mut opts = ConnectOptions {
        url: cli.url,
        token: cli.token,
        oauth_redirect: cli.redirect,
        scope: cli.scope,
        client_name: "mcp-client".into(),
    };

    if cli.login {
        eprintln!("Starting OAuth login…");
        let token = login_oauth(&opts).await?;
        eprintln!("Access token acquired. Export for later runs:");
        eprintln!("  export MCP_TOKEN='{token}'");
        opts.token = Some(token);
    }

    if cli.headless_list {
        let session = McpSession::connect(opts).await?;
        let tools = session.list_tools().await?;
        println!(
            "Server: {} v{} (protocol {})",
            session.server_status().name,
            session.server_status().version,
            session.server_status().protocol_version
        );
        println!("Tools ({}):", tools.len());
        for t in &tools {
            println!("  - {} — {}", t.name, t.description);
        }
        session.disconnect().await?;
        return Ok(());
    }

    tui::run_tui(opts).await?;
    Ok(())
}
