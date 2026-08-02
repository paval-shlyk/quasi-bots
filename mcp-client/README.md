# mcp-client

MCP **2025-11-25** Streamable HTTP client with a **ratatui** TUI for verifying servers such as [skill-master](../skill-master/).

## Features

- Connect to Streamable HTTP MCP endpoints (e.g. `http://127.0.0.1:8080/mcp`)
- Protocol version `2025-11-25`
- List tools and call tools with JSON arguments
- Auth:
  - **Bearer token** via `--token` / `MCP_TOKEN`
  - **OAuth 2.1 + PKCE** via `--login` (dynamic client registration + browser flow against `mcp-auth`)

## How to get a Bearer token

skill-master protects `/mcp` with OAuth. Tokens are issued only after a successful authorization code + PKCE flow (Google owner allowlist).

```bash
# Interactive OAuth, then open the TUI
cargo run -p mcp-client -- --url http://127.0.0.1:8080/mcp --login

# Reuse a previously issued access token
export MCP_TOKEN='…'
cargo run -p mcp-client -- --url http://127.0.0.1:8080/mcp
```

After `--login`, the access token is printed once (and shown in the TUI log) so you can export `MCP_TOKEN` for later runs. Server-side tokens live in memory — restart skill-master and re-login.

## CLI

```text
mcp-client [OPTIONS]

  -u, --url <URL>        MCP endpoint [env: MCP_URL]
                         [default: http://127.0.0.1:8080/mcp]
  -t, --token <TOKEN>    Bearer access token [env: MCP_TOKEN]
      --login            Run OAuth PKCE before connecting
      --redirect <URI>   OAuth redirect URI
                         [default: http://127.0.0.1:9876/callback]
      --scope <SCOPE>    OAuth scope [default: mcp]
      --headless-list    List tools to stdout and exit (no TUI)
```

## TUI keys

| Key | Action |
|-----|--------|
| `Tab` / `1`–`4` | Focus panes |
| `c` | Connect / reconnect |
| `l` | OAuth login |
| `/` | Filter tools |
| `Enter` | Select tool (seed call args from schema) |
| `i` | Invoke `tools/call` |
| `r` | Refresh tool list |
| `e` | Edit call args |
| `q` | Quit |

## Library

```rust
use mcp_client::{ConnectOptions, McpSession};

let opts = ConnectOptions {
    url: "http://127.0.0.1:8080/mcp".into(),
    token: Some(std::env::var("MCP_TOKEN")?),
    ..ConnectOptions::default()
};
let session = McpSession::connect(opts).await?;
let tools = session.list_tools().await?;
let result = session.call_tool("knowledge_list_topics", serde_json::json!({})).await?;
session.disconnect().await?;
```

## Dev notes

- OAuth discovery uses the MCP URL **origin** (scheme + host + port), not the `/mcp` path.
- skill-master is typically `stateful_mode = false` and `json_response = true`; the client uses rmcp's Streamable HTTP client with `allow_stateless` enabled by default.
