# mcp-auth

OAuth 2.1 **resource server** library for MCP hosts. Nest its Axum routes and bearer middleware under a host application (e.g. [skill-master](../skill-master/)) that serves the MCP Streamable HTTP endpoint at `/mcp`.

Authorization (login, consent, token minting) is **delegated** to an external authorization server such as **Zitadel**. This crate:

1. Advertises **RFC 9728** protected-resource metadata pointing at that AS
2. Validates inbound **opaque Bearer** access tokens via **RFC 7662 Token Introspection**

## Features

| Area | Details |
|------|---------|
| **Resource metadata** | RFC 9728 PRM at `/.well-known/oauth-protected-resource[/mcp]` |
| **Token validation** | Introspection (`active`, `iss`, scope, optional `sub` allowlist) |
| **Middleware** | `bearer_auth_middleware` for protecting `/mcp` |
| **Discovery** | OIDC `introspection_endpoint` from `{issuer}/.well-known/openid-configuration` |

## Public API

```rust
use mcp_auth::{McpAuthConfig, oauth};

let config: McpAuthConfig = /* load from TOML */;
let oauth_state = oauth::state(config.clone()).await?; // discover introspect URL
let oauth_router = oauth::router(); // PRM routes only

// .merge(oauth_router.with_state(oauth_state.clone()))
// .nest_service("/mcp", mcp_service.layer(bearer_auth_middleware))
```

## Configuration

```toml
public_url = "http://127.0.0.1:8080"
authorization_server = "https://auth.example.com"
scope = "mcp"
allowed_subs = []
allowed_origins = []
introspection_client_id = "your-api-app-client-id"
# introspection_client_secret = ""  # prefer INTROSPECTION_CLIENT_SECRET
```

| Field | Description |
|-------|-------------|
| `public_url` | Public origin — **scheme + host + port only** |
| `authorization_server` | AS issuer URL (OIDC discovery base) |
| `scope` | Advertised in PRM (`scopes_supported` is split) + `WWW-Authenticate`. Include Zitadel `urn:zitadel:iam:org:project:id:{id}:aud` so clients request the API audience. |
| `allowed_subs` | Optional `sub` allowlist |
| `introspection_client_id` | Zitadel **API** application client id (Basic) |
| `introspection_client_secret` | API app secret; if omitted/empty, filled from `INTROSPECTION_CLIENT_SECRET` at TOML parse time |

Derived: **resource URI** = `{public_url}/mcp` (RFC 9728).

## Zitadel setup

1. Enable OIDC / DCR for MCP clients as needed.
2. Create an **API** application with **Basic** authentication (this is the RS’s introspect client, not the MCP user’s client).
3. Set `introspection_client_id` and export `INTROSPECTION_CLIENT_SECRET`.
4. Advertise the project-audience scope so Zitadel will let **this API** introspect the token (`active: true`):

   `urn:zitadel:iam:org:project:id:{PROJECT_ID}:aud`

   Put it in `scope` next to `mcp` (see [skill-master/config.toml](../skill-master/config.toml)). Clients that honor RFC 9728 `scopes_supported` will request it.

5. Clients may receive **opaque** access tokens (`token_type: Bearer`). skill-master asks Zitadel’s `/oauth/v2/introspect` whether each token is `active` and reads claims from the JSON response — it does **not** decode a JWT locally.

`{active: false}` on a fresh token usually means the client omitted the project-audience scope.

## Token validation rules

1. Call AS introspection with the Bearer token (client_secret_basic).
2. Require `active: true`.
3. If `iss` present → must match `authorization_server`.
4. If introspection `scope` is present → must include advertised `urn:zitadel:iam:org:project:id:{id}:aud` values. `mcp` is advertised to clients; Zitadel does not grant it.
5. If `allowed_subs` non-empty → `sub` must be listed.

Invalid / inactive tokens → `401` with `WWW-Authenticate` including `resource_metadata`.

Successful results are cached briefly (~45s) to limit introspect traffic.

## Testing

```bash
cargo test -p mcp-auth
```

## Migration from JWKS

Local JWKS JWT verification has been **removed**. Opaque tokens + introspection are the only supported path.
