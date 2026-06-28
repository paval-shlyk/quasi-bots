# mcp-auth

OAuth 2.1 Authorization Server library for MCP resource servers. Nest its Axum routes and bearer middleware under a host application (e.g. [skill-master](../skill-master/)) that serves the MCP Streamable HTTP endpoint at `/mcp`.

## Features

| Area | Details |
|------|---------|
| **Authorization** | OAuth 2.1 with PKCE S256, RFC 9728 protected-resource metadata, RFC 8414 AS metadata |
| **Owner auth** | Google OIDC with allowlisted `sub` values |
| **Storage** | In-memory clients, sessions, and bearer tokens |
| **Middleware** | `bearer_auth_middleware` for protecting `/mcp` |

## Public API

```rust
use mcp_auth::{McpAuthConfig, oauth};

let config: McpAuthConfig = /* load from TOML */;
let oauth_state = oauth::state(config.clone()).await?;
let oauth_router = oauth::router();

// Nest under your host router:
// .merge(oauth_router.with_state(oauth_state.clone()))
// .nest_service("/mcp", mcp_service.layer(bearer_auth_middleware))
```

## Configuration

Copy and edit local config for development:

```bash
cp mcp-auth/config.toml.example mcp-auth/config.toml
```

```toml
public_url = "http://127.0.0.1:8080"
token_ttl_secs = 3600
scope = "mcp"
allowed_origins = []
stateful_mode = false
json_response = true

[auth.google]
client_id = "YOUR_ID.apps.googleusercontent.com"
allowed_google_subs = ["your-google-sub"]
```

| Field | Description |
|-------|-------------|
| `public_url` | Public origin for OAuth metadata — **scheme + host + port only, no path** |
| `auth.google.client_id` | Google OAuth Web client ID |
| `auth.google.allowed_google_subs` | Allowlisted Google `sub` values (owners who may approve MCP clients) |
| `GOOGLE_CLIENT_SECRET` | Env var for client secret (preferred over putting secret in config) |
| `token_ttl_secs` | Access-token lifetime in seconds |
| `scope` | OAuth scope advertised to clients |
| `allowed_origins` | Browser origins for Streamable HTTP Origin validation (empty = default) |
| `stateful_mode` | Streamable HTTP session mode (`false` = stateless per-request tool calls) |
| `json_response` | Return `application/json` instead of SSE when `stateful_mode = false` |

### Google OAuth setup

1. Create a **Web application** OAuth client in [Google Cloud Console](https://console.cloud.google.com/apis/credentials).
2. Add authorized redirect URI matching `public_url`:
   - Dev: `http://127.0.0.1:8080/oauth/google/callback`
   - Prod: `https://your-domain.com/oauth/google/callback`
3. Set `auth.google.client_id` in config.
4. Export `GOOGLE_CLIENT_SECRET` (or set `auth.google.client_secret`).
5. Seed the allowlist:
   - Leave `allowed_google_subs = []` for dev — after first Google sign-in the server logs your `sub`.
   - Add that `sub` to `allowed_google_subs` before production.

`allowed_hosts` is derived automatically from `public_url`.

OAuth clients, sessions, and bearer tokens are held **in memory** — restart clears them and MCP clients must re-authenticate.

## Testing

```bash
cargo test -p mcp-auth
```