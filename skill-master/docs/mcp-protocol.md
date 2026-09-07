# skill-master MCP protocol inventory

This document is the written inventory for issue #15: what skill-master spoke
before the rmcp 3.x upgrade, what “legacy” means in this codebase vs current
rmcp, and what we advertise after the upgrade.

## Terminology (this repo)

| Term | Meaning here |
|------|----------------|
| **Legacy protocol** | MCP revisions **before** `2026-07-28` (`2024-11-05` … `2025-11-25`). These still use `initialize` / `notifications/initialized` and may use `Mcp-Session-Id` when the Streamable HTTP server runs in **legacy session mode**. In rmcp ≥ 3.0 this is `StreamableHttpServerConfig::legacy_session_mode` (formerly `stateful_mode`). |
| **Modern protocol** | MCP `2026-07-28` (and later): **stateless** core — no initialize handshake, no protocol session; per-request `_meta` + `MCP-Protocol-Version` / `Mcp-Method` / `Mcp-Name` headers; optional `server/discover` (SEP-2575, SEP-2567, SEP-2243). |
| **Legacy HTTP+SSE transport** | Pre–Streamable HTTP transport. **Not used** by skill-master (removed from rmcp since 0.11). |

Schema nullability (`anyOf` vs `"type": [T,"null"]`) is **out of scope** for #15 — that is issue #14 / PR #16.

## Before (rmcp 1.7 on `dev`)

| Surface | State |
|---------|--------|
| Transport | Streamable HTTP only (`StreamableHttpService` at `/mcp`) |
| SDK | `rmcp = 1.7` |
| Advertised initialize version | Hard-coded `ProtocolVersion::V_2025_11_25` |
| Session mode | Default **stateful** sessions (`stateful_mode: true` in rmcp 1.7) via `LocalSessionManager` |
| Discovery | No `server/discover`; clients must `initialize` |
| Capabilities | Tools + `listChanged` |
| Clients | Cursor / Grok Bot connectors and in-repo `mcp-client` expected Streamable HTTP + initialize on `2025-11-25` |

Gaps vs current MCP / rmcp 3.x:

1. No support for negotiating **`2026-07-28`** (stateless, header routing, discover).
2. Session / initialize path is what rmcp now labels **legacy session mode**.
3. Model types (`Content` / `RawContent`, auth `discover_metadata`, peer info) lag the draft-aligned 3.x SDK.

## After (this change)

| Surface | State |
|---------|--------|
| SDK | `rmcp = 3.2` (skill-master + mcp-client) |
| Transport | Unchanged: Streamable HTTP at `/mcp` |
| Supported versions | All `ProtocolVersion::KNOWN_VERSIONS` (includes `2024-11-05` … **`2026-07-28`**) via `ServerHandler::supported_protocol_versions` |
| Initialize fallback | `V_2025_11_25` (`ProtocolVersion::LATEST` in rmcp 3.2) — safe default for today’s Cursor-style clients |
| Modern path | Clients that prefer `2026-07-28` negotiate it; those requests are served **statelessly** (SEP-2567) even while `legacy_session_mode` remains `true` for older peers |
| Legacy session mode | Still **enabled** (rmcp default) so initialize + session clients keep working on mcp-dev |
| `server/discover` | Available through rmcp’s default `ServerHandler::discover` |
| Schemas | Unchanged from #14 (`anyOf` rewrite) |

## Compatibility matrix (mcp-dev)

| Client class | Expected path | Risk |
|--------------|---------------|------|
| Cursor / connectors still on `2025-11-25` + initialize | Legacy initialize + session | **Low** if deploy keeps `legacy_session_mode` (default) |
| Modern clients / rmcp 3.x `ClientLifecycleMode::Auto` or `Discover` | Prefer `2026-07-28` discover / per-request meta | **Low–medium** until smoke-tested on mcp-dev |
| Old HTTP+SSE-only clients | Unsupported | Already broken before this PR |
| Strict schema clients | Rely on #14 anyOf rewrite | Unrelated to protocol revision |

## Phased follow-ups (not in this PR)

1. **Phase 2** — Prefer modern-only session config for new deploys (`legacy_session_mode: false` + optional `stateless_protocol_metadata_required`) once Cursor connectors speak `2026-07-28`.
2. **Phase 3** — Drop advertising pre-`2025-11-25` versions if nothing in the fleet needs them.
3. **Phase 4** — Auth hardening aligned with 2026-07-28 (CIMD vs DCR, issuer checks) in mcp-client / mcp-auth.

## Smoke checklist (post-deploy)

1. `GET /health` returns cargo+git version (unauthenticated).
2. Legacy client: initialize → `tools/list` → one smoke `tools/call`.
3. Modern client (`mcp-client` Auto lifecycle): discover or negotiate `2026-07-28` → list → call.
