# mcp-auth

OAuth 2.1 **resource server** library for MCP hosts. Nest its Axum routes and bearer middleware under a host application (e.g. [skill-master](../skill-master/)) that serves the MCP Streamable HTTP endpoint at `/mcp`.

Authorization (login, consent, token minting) is **delegated** to an external authorization server such as **Keycloak** or **Zitadel**. This crate only:

1. Advertises **RFC 9728** protected-resource metadata pointing at that AS
2. Validates inbound **JWT** access tokens via **OIDC discovery + JWKS**

## Features

| Area | Details |
|------|---------|
| **Resource metadata** | RFC 9728 PRM at `/.well-known/oauth-protected-resource[/mcp]` |
| **Token validation** | JWT signature (JWKS), `iss`, `aud` = resource URI, `exp`, optional `sub` allowlist |
| **Middleware** | `bearer_auth_middleware` for protecting `/mcp` |
| **Discovery** | OIDC `/.well-known/openid-configuration` (path-aware for Keycloak realms) |

## Public API

```rust
use mcp_auth::{McpAuthConfig, oauth};

let config: McpAuthConfig = /* load from TOML */;
let oauth_state = oauth::state(config.clone()).await?; // OIDC discovery + JWKS fetch
let oauth_router = oauth::router(); // PRM routes only

// Nest under your host router:
// .merge(oauth_router.with_state(oauth_state.clone()))
// .nest_service("/mcp", mcp_service.layer(bearer_auth_middleware))
```

## Configuration

```bash
cp mcp-auth/config.toml.example mcp-auth/config.toml
```

```toml
public_url = "http://127.0.0.1:8080"
authorization_server = "https://auth.example.com/realms/mcp"
scope = "mcp"
allowed_subs = []
allowed_origins = []
stateful_mode = false
json_response = true
```

| Field | Description |
|-------|-------------|
| `public_url` | Public origin for resource metadata — **scheme + host + port only, no path** |
| `authorization_server` | External AS **issuer** URL (Keycloak realm or Zitadel issuer). Path allowed. |
| `scope` | Scope advertised in PRM and `WWW-Authenticate` |
| `allowed_subs` | Optional JWT `sub` allowlist (empty = any subject) |
| `allowed_origins` | Browser origins for Streamable HTTP Origin validation |
| `stateful_mode` | Streamable HTTP session mode (`false` = stateless) |
| `json_response` | Return `application/json` instead of SSE when stateless |

Derived values (not config):

- **Resource / audience** = `{public_url}/mcp`
- **JWKS URI** = from OIDC discovery of `authorization_server`

## Authorization server setup (Keycloak / Zitadel)

Clients discover the AS from protected-resource metadata, complete OAuth 2.1 + PKCE **against the AS**, and call MCP with the resulting JWT.

### Audience (required)

MCP requires the access token to be issued for this resource. Validation expects:

```text
aud includes "{public_url}/mcp"
```

Default Keycloak tokens often set `aud` to the client ID only. Configure an **audience mapper** (or Zitadel API / resource indicator) so access tokens include the MCP resource URI, e.g. `http://127.0.0.1:8080/mcp`.

### Client registration

Prefer one of:

1. Pre-register the MCP client on the AS (redirect URIs such as `http://127.0.0.1:9876/callback`)
2. Enable Dynamic Client Registration if your client supports it
3. Client ID Metadata Documents if the AS supports them

### Example Keycloak realm issuer

```toml
authorization_server = "https://keycloak.example.com/realms/mcp"
```

OIDC discovery is tried at:

- `{issuer}/.well-known/openid-configuration`
- path-inserted and RFC 8414 variants as fallback

## Token validation rules

A request to `/mcp` must send `Authorization: Bearer <jwt>`. The JWT must:

1. Verify with a key from the AS JWKS (`kid` match, refresh on unknown kid)
2. `iss` = configured `authorization_server`
3. `aud` contains `{public_url}/mcp`
4. Not be expired (60s clock skew)
5. If `scope` / `scp` claims are present, include the configured `scope`
6. If `allowed_subs` is non-empty, `sub` must be listed

Invalid / missing tokens → `401` with `WWW-Authenticate` including `resource_metadata`.

## Testing

```bash
cargo test -p mcp-auth
```

## Migration from the old embedded AS

Previous versions of this crate minted opaque tokens and ran Google OIDC + DCR on the host. That flow is **removed**. Clients must obtain JWTs from the external AS; restarting the host no longer invalidates AS-issued tokens (until they expire).
