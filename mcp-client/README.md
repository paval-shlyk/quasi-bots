# mcp-client

MCP Streamable HTTP client (prefers **`2026-07-28`**, falls back to **`2025-11-25`** initialize) with a **ratatui** TUI for verifying servers such as [skill-master](../skill-master/).

## Features

- Connect to Streamable HTTP MCP endpoints (e.g. `http://127.0.0.1:8080/mcp`)
- Protocol: Auto lifecycle — prefer `2026-07-28` discover, legacy `2025-11-25` initialize fallback
- List tools and call tools with JSON arguments
- Auth:
  - **Bearer token** via `--token` / `MCP_TOKEN` (opaque access token from the AS)
  - **OAuth 2.1 + PKCE** via `--login` (PRM scopes → Zitadel DCR as native public client)

## How to get a Bearer token

skill-master protects `/mcp` as an OAuth **resource server**. Access tokens are typically **opaque** Bearer tokens from Zitadel; the server validates them via **RFC 7662 introspection** (see [mcp-auth/README.md](../mcp-auth/README.md)).

`--login` discovers RFC 9728 protected-resource metadata and requests `scopes_supported` (including Zitadel’s `urn:zitadel:iam:org:project:id:{projectId}:aud`). Without that audience scope Zitadel introspects the token as `{active: false}`.

```bash
# Interactive OAuth, then open the TUI
cargo run -p mcp-client -- --url http://127.0.0.1:8080/mcp --login

# Reuse a previously issued access token
export MCP_TOKEN='…'
cargo run -p mcp-client -- --url http://127.0.0.1:8080/mcp
```

After `--login`, the access token is printed once so you can export `MCP_TOKEN`.

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

## Debugging

```bash
RUST_LOG=debug,mcp_client=debug,rmcp=info \
  cargo run -p mcp-client -- --url http://127.0.0.1:8080/mcp --login
```

`--login` / `--headless-list` default to richer stderr logs unless `RUST_LOG` is set.

## Dev notes

- OAuth discovery uses the MCP URL **origin** (scheme + host + port), not the `/mcp` path.
- DCR registers `application_type: native` for loopback redirects (Zitadel).
- Login prefers PRM `scopes_supported` over `--scope` (CLI scopes are merged in).
- skill-master keeps legacy session mode for older peers; modern `2026-07-28` is always stateless. The client sets `allow_stateless`.
